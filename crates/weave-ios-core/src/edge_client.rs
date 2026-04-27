//! `EdgeClient` — WebSocket `/ws/edge` producer for the iOS app.
//!
//! Mirrors the pattern of `UiClient` but in the opposite direction: the iPad
//! announces itself as an edge with `Hello`, replies to server `Ping` with
//! `Pong`, and forwards locally-observed Nuimo state via `DeviceState`
//! frames. Inbound `ConfigFull` / `ConfigPatch` frames feed a local
//! `RoutingEngine` so on-device adapters (added in subsequent PRs) can act
//! on Nuimo input without a server round-trip — matching the Linux/Mac
//! edge-agent's adapter model.
//!
//! `device_id` for Nuimos is the `peripheral.identifier.uuidString` from
//! CoreBluetooth (lowercased UUID), not the BLE MAC the Linux edge-agent
//! uses. CoreBluetooth hides MAC on iOS, and uniqueness is preserved by
//! `(edge_id, device_id)` in `weave-contracts::DeviceStateEntry`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use edge_core::{RoutingEngine, ServiceAdapter};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::{broadcast, mpsc, Mutex};
use tokio_tungstenite::tungstenite::protocol::Message;
use weave_contracts::{EdgeToServer, PatchOp, ServerToEdge};

use crate::adapter_ios_media::{IosMediaAdapter, IosMediaCallback, NowPlayingInfo, PlaybackState};
use crate::device_control::DeviceControlSink;
use crate::feedback_pump::{run_feedback_pump, LedFeedbackSink, StateUpdate};
use crate::glyph_registry::GlyphRegistry;
use crate::{nuimo_event_to_input_primitive, NuimoEvent, WeaveError};

/// Swift-implemented callback for edge connection state.
///
/// Inbound frames (`ConfigFull`, `Ping`, …) are handled internally for now;
/// we only surface connection liveness to Swift.
#[uniffi::export(with_foreign)]
pub trait EdgeEventSink: Send + Sync {
    fn on_connection_changed(&self, connected: bool);
}

/// Internal command from public methods to the WS outbox loop.
enum OutboundCommand {
    DeviceState {
        device_type: String,
        device_id: String,
        property: String,
        value_json: String,
    },
    ServiceState {
        service_type: String,
        target: String,
        property: String,
        output_id: Option<String>,
        value_json: String,
    },
    EdgeStatus {
        wifi: Option<u8>,
    },
    /// Forward a routed intent the iOS edge can't dispatch locally
    /// (anything other than `ios_media`). The WS loop translates this
    /// into `EdgeToServer::DispatchIntent` so weave-server can find an
    /// edge with the matching capability and forward the work there.
    DispatchIntent {
        service_type: String,
        service_target: String,
        intent: String,
        params: serde_json::Value,
        output_id: Option<String>,
    },
    /// Local cycle-gesture detection advanced the active mapping.
    /// Translated into `EdgeToServer::SwitchActiveConnection` so the
    /// server can persist + broadcast to peer edges + the web UI.
    SwitchActiveConnection {
        device_type: String,
        device_id: String,
        active_mapping_id: uuid::Uuid,
    },
}

#[derive(uniffi::Object)]
pub struct EdgeClient {
    shutdown_tx: Mutex<Option<mpsc::Sender<()>>>,
    outbox_tx: mpsc::Sender<OutboundCommand>,
    /// Mappings cache populated from inbound `ConfigFull` / `ConfigPatch`.
    /// Read by `route_nuimo_event` to translate device input into intents.
    engine: Arc<RoutingEngine>,
    /// Optional iOS media dispatcher. Set lazily by Swift via
    /// `register_ios_media_callback`; absent until then, in which case
    /// `ios_media`-typed routing produces a debug log and no dispatch.
    ios_media_adapter: StdMutex<Option<Arc<IosMediaAdapter>>>,
    /// Named LED glyphs pushed by weave-server in `ConfigFull` /
    /// `GlyphsUpdate`. Consumed by the feedback pump to render
    /// `FeedbackPlan::NamedGlyph(name)` against the 9x9 grid.
    /// Held on the struct so the registry outlives the WS loop and
    /// the pump task.
    #[allow(dead_code)]
    glyphs: Arc<GlyphRegistry>,
    /// In-process broadcast for the feedback pump. Each `publish_*`
    /// method writes to both the WS outbox (server-bound) and this
    /// channel (LED-bound on the same iPad).
    state_tx: broadcast::Sender<StateUpdate>,
    /// Swift-registered LED write sink. The pump reads under a guard,
    /// drops it before awaiting, and does nothing while the slot is
    /// `None` — letting Swift register after `connect` finishes.
    led_sink: Arc<StdMutex<Option<Arc<dyn LedFeedbackSink>>>>,
    /// Swift-registered server-driven device control sink. Receives
    /// `ServerToEdge::DisplayGlyph` / `DeviceConnect` / `DeviceDisconnect`
    /// from the WS handler and dispatches into `BleBridge`. Same
    /// `Arc<StdMutex<Option<...>>>` pattern as `led_sink` so Swift can
    /// register after `connect` finishes.
    device_control: Arc<StdMutex<Option<Arc<dyn DeviceControlSink>>>>,
}

const OUTBOX_CAPACITY: usize = 64;
/// Capacity of the in-process state broadcast that the feedback pump
/// drains. iOS state updates are infrequent (NowPlaying changes + 5 s
/// poll + per-volume KVO), so 64 is plenty.
const STATE_CHANNEL_CAPACITY: usize = 64;

#[uniffi::export(async_runtime = "tokio")]
impl EdgeClient {
    /// Connect to weave-server's `/ws/edge`. `server_url` accepts the same
    /// shape as `UiClient::connect` (`http(s)://` or `ws(s)://` base); we
    /// derive `ws[s]://host:port/ws/edge`.
    #[uniffi::constructor]
    pub async fn connect(
        server_url: String,
        edge_id: String,
        capabilities: Vec<String>,
        sink: Arc<dyn EdgeEventSink>,
    ) -> Result<Arc<Self>, WeaveError> {
        let base = normalize_base(&server_url)?;
        let ws_url = derive_edge_ws_url(&base)?;

        let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>(1);
        let (outbox_tx, outbox_rx) = mpsc::channel::<OutboundCommand>(OUTBOX_CAPACITY);
        let (state_tx, state_rx) = broadcast::channel::<StateUpdate>(STATE_CHANNEL_CAPACITY);
        let engine = Arc::new(RoutingEngine::new());
        let glyphs = Arc::new(GlyphRegistry::new());
        let led_sink: Arc<StdMutex<Option<Arc<dyn LedFeedbackSink>>>> =
            Arc::new(StdMutex::new(None));
        let device_control: Arc<StdMutex<Option<Arc<dyn DeviceControlSink>>>> =
            Arc::new(StdMutex::new(None));

        tokio::spawn(run_ws_loop(
            ws_url,
            edge_id,
            capabilities,
            sink,
            shutdown_rx,
            outbox_rx,
            engine.clone(),
            glyphs.clone(),
            device_control.clone(),
        ));

        // Feedback pump exits when `state_tx` (held below) is dropped —
        // i.e., when this `EdgeClient` is dropped on shutdown.
        tokio::spawn(run_feedback_pump(
            state_rx,
            engine.clone(),
            glyphs.clone(),
            led_sink.clone(),
        ));

        Ok(Arc::new(Self {
            shutdown_tx: Mutex::new(Some(shutdown_tx)),
            outbox_tx,
            engine,
            ios_media_adapter: StdMutex::new(None),
            glyphs,
            state_tx,
            led_sink,
            device_control,
        }))
    }

    /// Register the Swift-side `MPRemoteCommandCenter` dispatcher. Replaces
    /// any previously-registered callback.
    ///
    /// Idempotent in shape: calling twice with the same callback is safe;
    /// the second call rebuilds the adapter on top of the new callback
    /// reference.
    pub fn register_ios_media_callback(&self, callback: Arc<dyn IosMediaCallback>) {
        let adapter = Arc::new(IosMediaAdapter::new(callback));
        *self
            .ios_media_adapter
            .lock()
            .expect("ios_media adapter mutex") = Some(adapter);
        tracing::info!("ios_media adapter registered");
    }

    /// Route a Nuimo event through the local engine and dispatch any
    /// produced intents to the registered adapter(s).
    ///
    /// Errors from the adapter are logged but do not propagate to Swift —
    /// the BLE callback site doesn't have a useful response to give the
    /// user beyond the Web UI Live Console row, which a future PR will
    /// publish via `EdgeToServer::Command`.
    pub async fn route_nuimo_event(
        &self,
        device_type: String,
        device_id: String,
        event: NuimoEvent,
    ) {
        let Some(input) = nuimo_event_to_input_primitive(&event) else {
            return; // Battery / non-input events: nothing to route.
        };

        // Cycle-gesture short-circuit: if the device has a DeviceCycle
        // and the input matches `cycle_gesture`, the engine has already
        // advanced active locally — relay to the server and skip normal
        // routing for this input.
        if let Some(active_mapping_id) = self
            .engine
            .try_cycle_switch(&device_type, &device_id, &input)
            .await
        {
            let cmd = OutboundCommand::SwitchActiveConnection {
                device_type: device_type.clone(),
                device_id: device_id.clone(),
                active_mapping_id,
            };
            if let Err(e) = self.outbox_tx.send(cmd).await {
                tracing::warn!(
                    error = %e,
                    %device_type, %device_id, %active_mapping_id,
                    "failed to enqueue switch_active_connection",
                );
            }
            return;
        }

        let routed = self.engine.route(&device_type, &device_id, &input).await;
        if routed.is_empty() {
            return;
        }

        // Snapshot the adapter slot under the std::sync::Mutex; release the
        // guard before any `.await` so we never hold a sync lock across a
        // suspend point.
        let adapter = self
            .ios_media_adapter
            .lock()
            .expect("ios_media adapter mutex")
            .clone();

        for ri in routed {
            match ri.service_type.as_str() {
                "ios_media" => {
                    let Some(adapter) = adapter.as_ref() else {
                        tracing::warn!(
                            target = %ri.service_target,
                            intent = ?ri.intent,
                            "ios_media routed but no dispatcher registered"
                        );
                        continue;
                    };
                    if let Err(e) = adapter.send_intent(&ri.service_target, &ri.intent).await {
                        tracing::warn!(
                            target = %ri.service_target,
                            error = %e,
                            "ios_media dispatch failed"
                        );
                    }
                }
                _ => {
                    // iPad has no adapter for this service_type. Forward
                    // the routed intent over WS so weave-server can
                    // dispatch via a peer edge with the right capability
                    // (typically pro for `roon` / `hue`). The executing
                    // edge emits the `Command` telemetry frame after
                    // running the adapter, so latency reflects the full
                    // path and the live console row carries the actual
                    // outcome.
                    let (intent_name, params) = ri.intent.split();
                    let cmd = OutboundCommand::DispatchIntent {
                        service_type: ri.service_type.clone(),
                        service_target: ri.service_target.clone(),
                        intent: intent_name,
                        params,
                        output_id: None,
                    };
                    if let Err(e) = self.outbox_tx.send(cmd).await {
                        tracing::warn!(
                            error = %e,
                            service_type = %ri.service_type,
                            target = %ri.service_target,
                            "failed to enqueue dispatch_intent for forwarding",
                        );
                    }
                }
            }
        }
    }

    /// Signal the WS loop to exit and release its sink reference.
    pub async fn shutdown(&self) {
        let tx = self.shutdown_tx.lock().await.take();
        if let Some(tx) = tx {
            let _ = tx.send(()).await;
        }
    }

    /// Push a `DeviceState` frame. Cached internally so that a subsequent
    /// reconnect replays the latest value for each `(device_id, property)`
    /// — same behavior as the Linux edge-agent's `spawn_device_state_pump`.
    ///
    /// `value_json` must be valid JSON; it is parsed eagerly so callers
    /// learn about malformed values at the call site.
    pub async fn publish_device_state(
        &self,
        device_type: String,
        device_id: String,
        property: String,
        value_json: String,
    ) -> Result<(), WeaveError> {
        serde_json::from_str::<serde_json::Value>(&value_json).map_err(|e| {
            WeaveError::ParseFailed {
                message: format!("publish_device_state value_json: {e}"),
            }
        })?;

        self.outbox_tx
            .send(OutboundCommand::DeviceState {
                device_type,
                device_id,
                property,
                value_json,
            })
            .await
            .map_err(|e| WeaveError::Network {
                message: format!("edge outbox closed: {e}"),
            })
    }

    /// Publish the iPad's Apple Music Now Playing snapshot to weave-server.
    /// Sent as `EdgeToServer::State { service_type: "ios_media", target:
    /// "apple_music", property: "now_playing", value: {...} }` so it
    /// surfaces in the same UI panel as Roon's now-playing data.
    ///
    /// Cached internally so reconnect replays the most recent snapshot.
    pub async fn publish_now_playing(&self, info: NowPlayingInfo) -> Result<(), WeaveError> {
        let value_json = serde_json::to_string(&now_playing_value(&info)).map_err(|e| {
            WeaveError::ParseFailed {
                message: format!("now_playing serialize: {e}"),
            }
        })?;

        self.outbox_tx
            .send(OutboundCommand::ServiceState {
                service_type: "ios_media".to_string(),
                target: "apple_music".to_string(),
                property: "now_playing".to_string(),
                output_id: None,
                value_json,
            })
            .await
            .map_err(|e| WeaveError::Network {
                message: format!("edge outbox closed: {e}"),
            })
    }

    /// Publish the iPad's playback state as a top-level service-state
    /// property. Mirrors Roon's `playback` shape ("playing" / "paused"
    /// / "stopped") so mapping-level `feedback` rules with
    /// `state: "playback", feedback_type: "glyph"` resolve through
    /// `FeedbackPlan::from_rules` exactly the same way they do for
    /// Roon zones. Sent in addition to (not instead of) the existing
    /// `now_playing` payload — that one carries title / artist / etc.
    /// for UI display and is left untouched.
    ///
    /// Also published into the in-process state broadcast so the
    /// feedback pump can drive the LED on the same iPad without a WS
    /// round-trip.
    pub async fn publish_playback(&self, state: String) -> Result<(), WeaveError> {
        let value = serde_json::Value::String(state);
        self.publish_ios_media_state("playback".into(), value).await
    }

    /// Publish the iPad's system volume as a top-level service-state
    /// property, on the 0..=100 scale Roon's `volume` / Hue's
    /// `brightness` use. Drives `feedback_type: "volume_bar"` rules
    /// through the same shared resolver.
    pub async fn publish_volume(&self, value: f64) -> Result<(), WeaveError> {
        let value = serde_json::Value::from(value);
        self.publish_ios_media_state("volume".into(), value).await
    }

    /// Publish the iPad's wifi signal strength so the server can render
    /// edge health in `/ws/ui` dashboards. `wifi` is `Some(percent)` on
    /// the 0..=100 scale, or `None` when the host can't read signal
    /// strength (no entitlement, no wifi adapter, fetchCurrent failed).
    /// Swift owns the timer and the platform-API call; this method is
    /// just the publish endpoint.
    pub async fn publish_edge_status(&self, wifi: Option<u8>) -> Result<(), WeaveError> {
        self.outbox_tx
            .send(OutboundCommand::EdgeStatus { wifi })
            .await
            .map_err(|e| WeaveError::Network {
                message: format!("publish_edge_status: outbox closed: {e}"),
            })
    }

    /// Register the Swift-side LED feedback sink. Replaces any prior
    /// callback. Until called, the feedback pump runs but every
    /// dispatch is a no-op (it logs nothing visible to keep the
    /// connection-setup window clean).
    pub fn register_led_feedback_callback(&self, sink: Arc<dyn LedFeedbackSink>) {
        *self.led_sink.lock().expect("led sink mutex") = Some(sink);
        tracing::info!("led feedback sink registered");
    }

    /// Register the Swift-side server-driven device control sink.
    /// Replaces any prior callback. Until called, inbound `DisplayGlyph`
    /// / `DeviceConnect` / `DeviceDisconnect` frames are dropped with a
    /// debug log — the WS loop keeps running so the WebSocket session
    /// itself stays healthy through the registration window.
    pub fn register_device_control_callback(&self, sink: Arc<dyn DeviceControlSink>) {
        *self.device_control.lock().expect("device control mutex") = Some(sink);
        tracing::info!("device control sink registered");
    }
}

// Internal helpers kept outside the `#[uniffi::export]` impl block
// because UniFFI tries to lift every pub method's parameters across
// the FFI boundary, and `serde_json::Value` doesn't have a `Lift` impl.
impl EdgeClient {
    async fn publish_ios_media_state(
        &self,
        property: String,
        value: serde_json::Value,
    ) -> Result<(), WeaveError> {
        let value_json = serde_json::to_string(&value).map_err(|e| WeaveError::ParseFailed {
            message: format!("ios_media state serialize: {e}"),
        })?;

        // 1. WS outbox — server-bound (Web UI, history).
        self.outbox_tx
            .send(OutboundCommand::ServiceState {
                service_type: "ios_media".to_string(),
                target: "apple_music".to_string(),
                property: property.clone(),
                output_id: None,
                value_json,
            })
            .await
            .map_err(|e| WeaveError::Network {
                message: format!("edge outbox closed: {e}"),
            })?;

        // 2. In-process broadcast — feedback pump on the same iPad.
        // `send` errs only when there are no live receivers, which
        // means the pump task has died. That's worth a warn but not a
        // user-visible failure.
        if let Err(e) = self.state_tx.send(StateUpdate {
            service_type: "ios_media".into(),
            target: "apple_music".into(),
            property,
            value,
        }) {
            tracing::warn!(error = %e, "feedback state broadcast: no live receivers");
        }
        Ok(())
    }
}

fn now_playing_value(info: &NowPlayingInfo) -> serde_json::Value {
    // Wire format keeps `volume` matching Roon's 0..100 convention so
    // weave-web's `extractLevel` (which already reads `volume` /
    // `brightness` / `level` keys) renders it as a percentage with no
    // special-casing. NowPlayingInfo's `system_volume` field stays in
    // the iOS-native 0..1 ratio for clarity at the Swift boundary.
    serde_json::json!({
        "title": info.title,
        "artist": info.artist,
        "album": info.album,
        "duration_seconds": info.duration_seconds,
        "position_seconds": info.position_seconds,
        "state": match info.state {
            PlaybackState::Stopped => "stopped",
            PlaybackState::Playing => "playing",
            PlaybackState::Paused => "paused",
        },
        "volume": info.system_volume.map(|v| v * 100.0),
    })
}

// ----- WebSocket loop -----------------------------------------------------

#[allow(clippy::too_many_arguments)]
async fn run_ws_loop(
    url: String,
    edge_id: String,
    capabilities: Vec<String>,
    sink: Arc<dyn EdgeEventSink>,
    mut shutdown_rx: mpsc::Receiver<()>,
    mut outbox_rx: mpsc::Receiver<OutboundCommand>,
    engine: Arc<RoutingEngine>,
    glyphs: Arc<GlyphRegistry>,
    device_control: Arc<StdMutex<Option<Arc<dyn DeviceControlSink>>>>,
) {
    let mut backoff = Duration::from_millis(500);
    let max_backoff = Duration::from_secs(15);
    // Last-write-wins replay cache, keyed by (device_type, device_id, property).
    let mut device_state_cache: HashMap<(String, String, String), serde_json::Value> =
        HashMap::new();
    // Last-write-wins replay cache for service state, keyed by
    // (service_type, target, property, output_id). Holds the most recent
    // payload for each so a reconnect repopulates the server's snapshot.
    type ServiceStateKey = (String, String, String, Option<String>);
    let mut service_state_cache: HashMap<ServiceStateKey, serde_json::Value> = HashMap::new();
    let version = env!("CARGO_PKG_VERSION").to_string();

    loop {
        tokio::select! {
            _ = shutdown_rx.recv() => return,
            res = tokio_tungstenite::connect_async(&url) => {
                match res {
                    Ok((mut ws, _resp)) => {
                        tracing::info!(url = %url, "ws/edge connected");
                        sink.on_connection_changed(true);
                        backoff = Duration::from_millis(500);

                        // Hello.
                        let hello = EdgeToServer::Hello {
                            edge_id: edge_id.clone(),
                            version: version.clone(),
                            capabilities: capabilities.clone(),
                        };
                        if !send_frame(&mut ws, &hello).await {
                            sink.on_connection_changed(false);
                            continue;
                        }

                        // Replay cached state.
                        let mut replay_failed = false;
                        for ((device_type, device_id, property), value) in &device_state_cache {
                            let frame = EdgeToServer::DeviceState {
                                device_type: device_type.clone(),
                                device_id: device_id.clone(),
                                property: property.clone(),
                                value: value.clone(),
                            };
                            if !send_frame(&mut ws, &frame).await {
                                replay_failed = true;
                                break;
                            }
                        }
                        if !replay_failed {
                            for (
                                (service_type, target, property, output_id),
                                value,
                            ) in &service_state_cache
                            {
                                let frame = EdgeToServer::State {
                                    service_type: service_type.clone(),
                                    target: target.clone(),
                                    property: property.clone(),
                                    output_id: output_id.clone(),
                                    value: value.clone(),
                                };
                                if !send_frame(&mut ws, &frame).await {
                                    replay_failed = true;
                                    break;
                                }
                            }
                        }
                        if replay_failed {
                            sink.on_connection_changed(false);
                            continue;
                        }

                        // Steady-state loop.
                        loop {
                            tokio::select! {
                                _ = shutdown_rx.recv() => {
                                    let _ = ws.send(Message::Close(None)).await;
                                    sink.on_connection_changed(false);
                                    return;
                                }
                                cmd = outbox_rx.recv() => {
                                    let Some(cmd) = cmd else { break; };
                                    match cmd {
                                        OutboundCommand::DeviceState {
                                            device_type, device_id, property, value_json,
                                        } => {
                                            let value: serde_json::Value =
                                                serde_json::from_str(&value_json)
                                                    .unwrap_or(serde_json::Value::Null);
                                            device_state_cache.insert(
                                                (device_type.clone(), device_id.clone(), property.clone()),
                                                value.clone(),
                                            );
                                            let frame = EdgeToServer::DeviceState {
                                                device_type, device_id, property, value,
                                            };
                                            if !send_frame(&mut ws, &frame).await {
                                                break;
                                            }
                                        }
                                        OutboundCommand::ServiceState {
                                            service_type, target, property, output_id, value_json,
                                        } => {
                                            let value: serde_json::Value =
                                                serde_json::from_str(&value_json)
                                                    .unwrap_or(serde_json::Value::Null);
                                            service_state_cache.insert(
                                                (
                                                    service_type.clone(),
                                                    target.clone(),
                                                    property.clone(),
                                                    output_id.clone(),
                                                ),
                                                value.clone(),
                                            );
                                            let frame = EdgeToServer::State {
                                                service_type, target, property, output_id, value,
                                            };
                                            if !send_frame(&mut ws, &frame).await {
                                                break;
                                            }
                                        }
                                        OutboundCommand::EdgeStatus { wifi } => {
                                            let frame = EdgeToServer::EdgeStatus { wifi };
                                            if !send_frame(&mut ws, &frame).await {
                                                break;
                                            }
                                        }
                                        OutboundCommand::DispatchIntent {
                                            service_type, service_target, intent, params, output_id,
                                        } => {
                                            let frame = EdgeToServer::DispatchIntent {
                                                service_type, service_target, intent, params, output_id,
                                            };
                                            if !send_frame(&mut ws, &frame).await {
                                                break;
                                            }
                                        }
                                        OutboundCommand::SwitchActiveConnection {
                                            device_type, device_id, active_mapping_id,
                                        } => {
                                            let frame = EdgeToServer::SwitchActiveConnection {
                                                device_type, device_id, active_mapping_id,
                                            };
                                            if !send_frame(&mut ws, &frame).await {
                                                break;
                                            }
                                        }
                                    }
                                }
                                msg = ws.next() => {
                                    match msg {
                                        Some(Ok(Message::Text(text))) => {
                                            match serde_json::from_str::<ServerToEdge>(&text) {
                                                Ok(ServerToEdge::Ping) => {
                                                    if !send_frame(&mut ws, &EdgeToServer::Pong).await {
                                                        break;
                                                    }
                                                }
                                                Ok(other) => {
                                                    apply_inbound_frame(
                                                        &engine,
                                                        &glyphs,
                                                        &device_control,
                                                        other,
                                                    )
                                                    .await;
                                                }
                                                Err(e) => {
                                                    tracing::warn!(
                                                        error = %e,
                                                        payload = %text,
                                                        "ws/edge: invalid frame"
                                                    );
                                                }
                                            }
                                        }
                                        Some(Ok(Message::Ping(p))) => {
                                            let _ = ws.send(Message::Pong(p)).await;
                                        }
                                        Some(Ok(_)) => {}
                                        Some(Err(e)) => {
                                            tracing::warn!(error = %e, "ws/edge read error");
                                            break;
                                        }
                                        None => break,
                                    }
                                }
                            }
                        }
                        sink.on_connection_changed(false);
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, url = %url, "ws/edge connect failed");
                    }
                }
            }
        }

        tokio::select! {
            _ = shutdown_rx.recv() => return,
            _ = tokio::time::sleep(backoff) => {
                backoff = (backoff * 2).min(max_backoff);
            }
        }
    }
}

async fn send_frame(
    ws: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    frame: &EdgeToServer,
) -> bool {
    let Ok(json) = serde_json::to_string(frame) else {
        return true;
    };
    ws.send(Message::Text(json)).await.is_ok()
}

/// Apply an inbound `ServerToEdge` frame (other than `Ping`, handled by
/// the WS loop directly) to the local routing engine, glyph registry,
/// and Swift-registered device control sink.
///
/// `device_control` is the slot Swift fills via
/// `register_device_control_callback`. While `None`, server-driven
/// device control frames (`DisplayGlyph` / `DeviceConnect` /
/// `DeviceDisconnect`) are dropped with a debug log — that's expected
/// during the connection-setup window before Swift hands its sink
/// across.
///
/// Extracted as a free async fn so unit tests can exercise the
/// frame-to-engine plumbing without standing up a WebSocket server.
async fn apply_inbound_frame(
    engine: &RoutingEngine,
    glyphs: &GlyphRegistry,
    device_control: &StdMutex<Option<Arc<dyn DeviceControlSink>>>,
    frame: ServerToEdge,
) {
    match frame {
        ServerToEdge::ConfigFull { config } => {
            let mapping_count = config.mappings.len();
            let glyph_count = config.glyphs.len();
            let cycle_count = config.device_cycles.len();
            engine.replace_all(config.mappings).await;
            engine.replace_cycles(config.device_cycles).await;
            glyphs.replace_all(config.glyphs).await;
            tracing::info!(
                mapping_count,
                glyph_count,
                cycle_count,
                "ws/edge: config_full applied"
            );
        }
        ServerToEdge::ConfigPatch {
            mapping_id,
            op,
            mapping,
        } => match (op, mapping) {
            (PatchOp::Upsert, Some(m)) => {
                engine.upsert_mapping(m).await;
                tracing::info!(%mapping_id, "ws/edge: mapping upserted");
            }
            (PatchOp::Delete, _) => {
                engine.remove_mapping(&mapping_id).await;
                tracing::info!(%mapping_id, "ws/edge: mapping deleted");
            }
            (PatchOp::Upsert, None) => {
                tracing::warn!(%mapping_id, "ws/edge: upsert without mapping payload");
            }
        },
        ServerToEdge::TargetSwitch {
            mapping_id,
            service_target,
        } => {
            // Server-pushed active-target change. Update the matching
            // mapping's `service_target` so subsequent `route` calls use
            // the new target. Engine has no direct setter, so we read,
            // mutate, and re-upsert.
            let mut snapshot = engine.snapshot().await;
            if let Some(m) = snapshot.iter_mut().find(|m| m.mapping_id == mapping_id) {
                m.service_target = service_target.clone();
                let updated = m.clone();
                engine.upsert_mapping(updated).await;
                tracing::info!(%mapping_id, %service_target, "ws/edge: target_switch applied");
            } else {
                tracing::warn!(%mapping_id, "ws/edge: target_switch for unknown mapping");
            }
        }
        ServerToEdge::GlyphsUpdate { glyphs: incoming } => {
            let count = incoming.len();
            glyphs.replace_all(incoming).await;
            tracing::info!(count, "ws/edge: glyphs_update applied");
        }
        // Device-control frames are dispatched across `device_control`
        // into Swift, which drives `BleBridge`. The slot is filled by
        // `register_device_control_callback`; while empty (initial
        // connection-setup window) the frame is dropped with a debug log
        // — same fail-soft behavior as `LedFeedbackSink`.
        ServerToEdge::DisplayGlyph {
            device_type,
            device_id,
            pattern,
            brightness,
            timeout_ms,
            transition,
        } => {
            // Snapshot the sink under the std::sync::Mutex. The trait is
            // synchronous (UniFFI's async-foreign-trait codegen breaks
            // Swift 6 strict concurrency), so Swift implementations
            // dispatch their own async work — typically a
            // `DispatchQueue.main.async` hop into BleBridge.
            let registered = { device_control.lock().expect("device control mutex").clone() };
            if let Some(sink) = registered {
                sink.display_glyph(
                    device_type,
                    device_id,
                    pattern,
                    brightness,
                    timeout_ms,
                    transition,
                );
            } else {
                tracing::debug!(
                    %device_type,
                    %device_id,
                    "ws/edge: display_glyph dropped — no device_control sink registered",
                );
            }
        }
        ServerToEdge::DeviceConnect {
            device_type,
            device_id,
        } => {
            let registered = { device_control.lock().expect("device control mutex").clone() };
            if let Some(sink) = registered {
                sink.connect_device(device_type, device_id);
            } else {
                tracing::debug!(
                    %device_type,
                    %device_id,
                    "ws/edge: device_connect dropped — no device_control sink registered",
                );
            }
        }
        ServerToEdge::DeviceDisconnect {
            device_type,
            device_id,
        } => {
            let registered = { device_control.lock().expect("device control mutex").clone() };
            if let Some(sink) = registered {
                sink.disconnect_device(device_type, device_id);
            } else {
                tracing::debug!(
                    %device_type,
                    %device_id,
                    "ws/edge: device_disconnect dropped — no device_control sink registered",
                );
            }
        }
        ServerToEdge::DispatchIntent {
            service_type,
            service_target,
            intent,
            ..
        } => {
            // weave-server only forwards `DispatchIntent` to edges whose
            // Hello capabilities advertise `service_type`. iPad reports
            // `["nuimo:ble", "ios_media"]`, so receiving anything other
            // than ios_media here means the server made a mistake (or
            // the capability set drifted). Log and ignore — never
            // silently dispatch a stranger's intent.
            tracing::warn!(
                %service_type,
                target = %service_target,
                %intent,
                "ws/edge: ignoring dispatch_intent — iOS edge has no matching adapter",
            );
        }
        ServerToEdge::Ping => {
            // Caller handles Pong reply directly to keep the WS handle scoped.
            unreachable!("apply_inbound_frame must not be called with Ping");
        }
        ServerToEdge::DeviceCyclePatch { cycle, op } => match op {
            weave_contracts::PatchOp::Upsert => {
                tracing::info!(
                    device_type = %cycle.device_type,
                    device_id = %cycle.device_id,
                    active = ?cycle.active_mapping_id,
                    gesture = ?cycle.cycle_gesture,
                    "device_cycle_patch upsert (iOS)"
                );
                engine.upsert_cycle(cycle).await;
            }
            weave_contracts::PatchOp::Delete => {
                tracing::info!(
                    device_type = %cycle.device_type,
                    device_id = %cycle.device_id,
                    "device_cycle_patch delete (iOS)"
                );
                engine
                    .remove_cycle(&cycle.device_type, &cycle.device_id)
                    .await;
            }
        },
        ServerToEdge::SwitchActiveConnection {
            device_type,
            device_id,
            active_mapping_id,
        } => {
            tracing::info!(
                %device_type,
                %device_id,
                %active_mapping_id,
                "switch_active_connection from server (iOS)"
            );
            let applied = engine
                .set_cycle_active(&device_type, &device_id, active_mapping_id)
                .await;
            if !applied {
                tracing::warn!(
                    %device_type, %device_id, %active_mapping_id,
                    "switch_active_connection: cycle missing or active not in mapping_ids — ignoring"
                );
            }
        }
    }
}

// ----- URL helpers --------------------------------------------------------
// Duplicated from ui_client.rs to keep the two clients independent. If a
// third client appears, fold these into a shared `url_util` module.

fn normalize_base(url: &str) -> Result<String, WeaveError> {
    let trimmed = url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err(WeaveError::Network {
            message: "empty server URL".into(),
        });
    }
    if !(trimmed.starts_with("http://")
        || trimmed.starts_with("https://")
        || trimmed.starts_with("ws://")
        || trimmed.starts_with("wss://"))
    {
        return Err(WeaveError::Network {
            message: format!("server URL must have a scheme: {trimmed}"),
        });
    }
    let base = trimmed
        .replacen("ws://", "http://", 1)
        .replacen("wss://", "https://", 1);
    Ok(base)
}

fn derive_edge_ws_url(http_base: &str) -> Result<String, WeaveError> {
    let ws_base = http_base
        .replacen("http://", "ws://", 1)
        .replacen("https://", "wss://", 1);
    Ok(format!("{ws_base}/ws/edge"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter_ios_media::{NowPlayingInfo, PlaybackState};
    use edge_core::{InputPrimitive, Intent};
    use std::collections::BTreeMap;
    use uuid::Uuid;
    use weave_contracts::{EdgeConfig, Mapping, Route};

    fn ios_media_mapping(device_id: &str) -> Mapping {
        Mapping {
            mapping_id: Uuid::new_v4(),
            edge_id: "ipad".into(),
            device_type: "nuimo".into(),
            device_id: device_id.into(),
            service_type: "ios_media".into(),
            service_target: "default".into(),
            routes: vec![Route {
                input: "press".into(),
                intent: "play_pause".into(),
                params: BTreeMap::new(),
            }],
            feedback: vec![],
            active: true,
            target_candidates: vec![],
            target_switch_on: None,
        }
    }

    /// Empty device-control slot for tests that exercise non-control
    /// arms of `apply_inbound_frame`. Same shape `EdgeClient::connect`
    /// constructs in production before Swift registers a sink.
    fn empty_device_control_slot() -> Arc<StdMutex<Option<Arc<dyn DeviceControlSink>>>> {
        Arc::new(StdMutex::new(None))
    }

    /// `(device_type, device_id, pattern, brightness, timeout_ms, transition)`
    type DisplayGlyphCapture = (
        String,
        String,
        String,
        Option<f32>,
        Option<u32>,
        Option<String>,
    );

    /// Recording sink that captures every dispatch so the test can
    /// assert the WS-loop dispatched into Swift correctly.
    #[derive(Default)]
    struct RecordingDeviceControlSink {
        connect: StdMutex<Vec<(String, String)>>,
        disconnect: StdMutex<Vec<(String, String)>>,
        display: StdMutex<Vec<DisplayGlyphCapture>>,
    }

    impl DeviceControlSink for RecordingDeviceControlSink {
        fn connect_device(&self, device_type: String, device_id: String) {
            self.connect.lock().unwrap().push((device_type, device_id));
        }
        fn disconnect_device(&self, device_type: String, device_id: String) {
            self.disconnect
                .lock()
                .unwrap()
                .push((device_type, device_id));
        }
        fn display_glyph(
            &self,
            device_type: String,
            device_id: String,
            pattern: String,
            brightness: Option<f32>,
            timeout_ms: Option<u32>,
            transition: Option<String>,
        ) {
            self.display.lock().unwrap().push((
                device_type,
                device_id,
                pattern,
                brightness,
                timeout_ms,
                transition,
            ));
        }
    }

    #[tokio::test]
    async fn config_full_replaces_engine_state() {
        let engine = RoutingEngine::new();
        let glyphs = GlyphRegistry::new();
        let mapping = ios_media_mapping("nuimo-1");
        let config = EdgeConfig {
            edge_id: "ipad".into(),
            mappings: vec![mapping.clone()],
            glyphs: vec![],
            device_cycles: vec![],
        };

        apply_inbound_frame(
            &engine,
            &glyphs,
            &empty_device_control_slot(),
            ServerToEdge::ConfigFull { config },
        )
        .await;

        let intents = engine
            .route("nuimo", "nuimo-1", &InputPrimitive::Press)
            .await;
        assert_eq!(intents.len(), 1);
        assert_eq!(intents[0].service_type, "ios_media");
        assert!(matches!(intents[0].intent, Intent::PlayPause));
    }

    #[tokio::test]
    async fn config_full_clears_prior_mappings() {
        let engine = RoutingEngine::new();
        let glyphs = GlyphRegistry::new();
        let first = ios_media_mapping("nuimo-1");
        engine.replace_all(vec![first]).await;

        let config = EdgeConfig {
            edge_id: "ipad".into(),
            mappings: vec![],
            glyphs: vec![],
            device_cycles: vec![],
        };
        apply_inbound_frame(
            &engine,
            &glyphs,
            &empty_device_control_slot(),
            ServerToEdge::ConfigFull { config },
        )
        .await;

        let intents = engine
            .route("nuimo", "nuimo-1", &InputPrimitive::Press)
            .await;
        assert!(
            intents.is_empty(),
            "config_full with no mappings must clear engine"
        );
    }

    #[tokio::test]
    async fn config_patch_upsert_adds_mapping() {
        let engine = RoutingEngine::new();
        let glyphs = GlyphRegistry::new();
        let mapping = ios_media_mapping("nuimo-1");
        let mapping_id = mapping.mapping_id;

        apply_inbound_frame(
            &engine,
            &glyphs,
            &empty_device_control_slot(),
            ServerToEdge::ConfigPatch {
                mapping_id,
                op: PatchOp::Upsert,
                mapping: Some(mapping),
            },
        )
        .await;

        let intents = engine
            .route("nuimo", "nuimo-1", &InputPrimitive::Press)
            .await;
        assert_eq!(intents.len(), 1);
    }

    #[tokio::test]
    async fn config_patch_delete_removes_mapping() {
        let engine = RoutingEngine::new();
        let glyphs = GlyphRegistry::new();
        let mapping = ios_media_mapping("nuimo-1");
        let mapping_id = mapping.mapping_id;
        engine.replace_all(vec![mapping]).await;

        apply_inbound_frame(
            &engine,
            &glyphs,
            &empty_device_control_slot(),
            ServerToEdge::ConfigPatch {
                mapping_id,
                op: PatchOp::Delete,
                mapping: None,
            },
        )
        .await;

        let intents = engine
            .route("nuimo", "nuimo-1", &InputPrimitive::Press)
            .await;
        assert!(intents.is_empty());
    }

    #[tokio::test]
    async fn target_switch_updates_service_target() {
        let engine = RoutingEngine::new();
        let glyphs = GlyphRegistry::new();
        let mapping = ios_media_mapping("nuimo-1");
        let mapping_id = mapping.mapping_id;
        engine.replace_all(vec![mapping]).await;

        apply_inbound_frame(
            &engine,
            &glyphs,
            &empty_device_control_slot(),
            ServerToEdge::TargetSwitch {
                mapping_id,
                service_target: "alt".into(),
            },
        )
        .await;

        let snapshot = engine.snapshot().await;
        let updated = snapshot
            .iter()
            .find(|m| m.mapping_id == mapping_id)
            .expect("mapping must remain present");
        assert_eq!(updated.service_target, "alt");
    }

    #[tokio::test]
    async fn target_switch_for_unknown_mapping_is_noop() {
        let engine = RoutingEngine::new();
        let glyphs = GlyphRegistry::new();
        let mapping = ios_media_mapping("nuimo-1");
        engine.replace_all(vec![mapping.clone()]).await;

        apply_inbound_frame(
            &engine,
            &glyphs,
            &empty_device_control_slot(),
            ServerToEdge::TargetSwitch {
                mapping_id: Uuid::new_v4(),
                service_target: "phantom".into(),
            },
        )
        .await;

        let snapshot = engine.snapshot().await;
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].service_target, "default");
    }

    #[tokio::test]
    async fn display_glyph_dispatches_to_registered_sink() {
        let engine = RoutingEngine::new();
        let glyphs = GlyphRegistry::new();
        let recorder: Arc<RecordingDeviceControlSink> =
            Arc::new(RecordingDeviceControlSink::default());
        let slot: Arc<StdMutex<Option<Arc<dyn DeviceControlSink>>>> = Arc::new(StdMutex::new(
            Some(recorder.clone() as Arc<dyn DeviceControlSink>),
        ));

        apply_inbound_frame(
            &engine,
            &glyphs,
            &slot,
            ServerToEdge::DisplayGlyph {
                device_type: "nuimo".into(),
                device_id: "nuimo-1".into(),
                pattern: "*".into(),
                brightness: Some(0.5),
                timeout_ms: Some(2000),
                transition: Some("cross_fade".into()),
            },
        )
        .await;

        let captured = recorder.display.lock().unwrap().clone();
        assert_eq!(captured.len(), 1);
        let (dt, did, pat, br, to, tr) = &captured[0];
        assert_eq!(dt, "nuimo");
        assert_eq!(did, "nuimo-1");
        assert_eq!(pat, "*");
        assert_eq!(*br, Some(0.5));
        assert_eq!(*to, Some(2000));
        assert_eq!(tr.as_deref(), Some("cross_fade"));
    }

    #[tokio::test]
    async fn device_connect_dispatches_to_registered_sink() {
        let engine = RoutingEngine::new();
        let glyphs = GlyphRegistry::new();
        let recorder: Arc<RecordingDeviceControlSink> =
            Arc::new(RecordingDeviceControlSink::default());
        let slot: Arc<StdMutex<Option<Arc<dyn DeviceControlSink>>>> = Arc::new(StdMutex::new(
            Some(recorder.clone() as Arc<dyn DeviceControlSink>),
        ));

        apply_inbound_frame(
            &engine,
            &glyphs,
            &slot,
            ServerToEdge::DeviceConnect {
                device_type: "nuimo".into(),
                device_id: "nuimo-1".into(),
            },
        )
        .await;

        let captured = recorder.connect.lock().unwrap().clone();
        assert_eq!(captured, vec![("nuimo".to_string(), "nuimo-1".to_string())]);
    }

    #[tokio::test]
    async fn device_disconnect_dispatches_to_registered_sink() {
        let engine = RoutingEngine::new();
        let glyphs = GlyphRegistry::new();
        let recorder: Arc<RecordingDeviceControlSink> =
            Arc::new(RecordingDeviceControlSink::default());
        let slot: Arc<StdMutex<Option<Arc<dyn DeviceControlSink>>>> = Arc::new(StdMutex::new(
            Some(recorder.clone() as Arc<dyn DeviceControlSink>),
        ));

        apply_inbound_frame(
            &engine,
            &glyphs,
            &slot,
            ServerToEdge::DeviceDisconnect {
                device_type: "nuimo".into(),
                device_id: "nuimo-1".into(),
            },
        )
        .await;

        let captured = recorder.disconnect.lock().unwrap().clone();
        assert_eq!(captured, vec![("nuimo".to_string(), "nuimo-1".to_string())]);
    }

    #[tokio::test]
    async fn device_control_frames_drop_when_no_sink_registered() {
        // No-panic guarantee: WS loop continues even without a sink so the
        // connection-setup window stays clean.
        let engine = RoutingEngine::new();
        let glyphs = GlyphRegistry::new();
        let slot = empty_device_control_slot();

        apply_inbound_frame(
            &engine,
            &glyphs,
            &slot,
            ServerToEdge::DisplayGlyph {
                device_type: "nuimo".into(),
                device_id: "nuimo-1".into(),
                pattern: "*".into(),
                brightness: None,
                timeout_ms: None,
                transition: None,
            },
        )
        .await;
        apply_inbound_frame(
            &engine,
            &glyphs,
            &slot,
            ServerToEdge::DeviceConnect {
                device_type: "nuimo".into(),
                device_id: "nuimo-1".into(),
            },
        )
        .await;
        apply_inbound_frame(
            &engine,
            &glyphs,
            &slot,
            ServerToEdge::DeviceDisconnect {
                device_type: "nuimo".into(),
                device_id: "nuimo-1".into(),
            },
        )
        .await;
    }

    #[test]
    fn now_playing_value_includes_all_fields_with_snake_case_state() {
        let info = NowPlayingInfo {
            title: Some("Lateralus".into()),
            artist: Some("Tool".into()),
            album: Some("Lateralus".into()),
            duration_seconds: Some(563.0),
            position_seconds: 120.5,
            state: PlaybackState::Playing,
            system_volume: Some(0.475),
        };
        let value = now_playing_value(&info);
        assert_eq!(value["title"], "Lateralus");
        assert_eq!(value["artist"], "Tool");
        assert_eq!(value["album"], "Lateralus");
        assert_eq!(value["duration_seconds"], 563.0);
        assert_eq!(value["position_seconds"], 120.5);
        assert_eq!(value["state"], "playing");
        assert_eq!(value["volume"], 47.5);
    }

    #[test]
    fn now_playing_value_optional_fields_serialize_as_null() {
        let info = NowPlayingInfo {
            title: None,
            artist: None,
            album: None,
            duration_seconds: None,
            position_seconds: 0.0,
            state: PlaybackState::Stopped,
            system_volume: None,
        };
        let value = now_playing_value(&info);
        assert!(value["title"].is_null());
        assert!(value["artist"].is_null());
        assert!(value["album"].is_null());
        assert!(value["duration_seconds"].is_null());
        assert_eq!(value["position_seconds"], 0.0);
        assert_eq!(value["state"], "stopped");
        assert!(value["volume"].is_null());
    }

    #[test]
    fn now_playing_state_paused_serializes_to_paused_string() {
        let info = NowPlayingInfo {
            title: None,
            artist: None,
            album: None,
            duration_seconds: None,
            position_seconds: 42.0,
            state: PlaybackState::Paused,
            system_volume: None,
        };
        assert_eq!(now_playing_value(&info)["state"], "paused");
    }

    #[test]
    fn now_playing_value_volume_zero_serializes_as_zero_not_null() {
        let info = NowPlayingInfo {
            title: None,
            artist: None,
            album: None,
            duration_seconds: None,
            position_seconds: 0.0,
            state: PlaybackState::Stopped,
            system_volume: Some(0.0),
        };
        let value = now_playing_value(&info);
        assert_eq!(
            value["volume"], 0.0,
            "muted (Some(0.0)) must serialize as 0, not null"
        );
    }

    #[test]
    fn derive_edge_appends_ws_edge_path() {
        assert_eq!(
            derive_edge_ws_url("http://host:3100").unwrap(),
            "ws://host:3100/ws/edge"
        );
        assert_eq!(
            derive_edge_ws_url("https://host").unwrap(),
            "wss://host/ws/edge"
        );
    }

    #[test]
    fn hello_frame_serializes_with_snake_case_tag() {
        let hello = EdgeToServer::Hello {
            edge_id: "ios-ipad".into(),
            version: "0.5.3".into(),
            capabilities: vec!["nuimo:ble".into()],
        };
        let json = serde_json::to_string(&hello).unwrap();
        assert!(json.contains("\"type\":\"hello\""));
        assert!(json.contains("\"edge_id\":\"ios-ipad\""));
        assert!(json.contains("\"capabilities\":[\"nuimo:ble\"]"));
    }

    #[test]
    fn device_state_frame_serializes() {
        let frame = EdgeToServer::DeviceState {
            device_type: "nuimo".into(),
            device_id: "abc-123".into(),
            property: "connected".into(),
            value: serde_json::json!(true),
        };
        let json = serde_json::to_string(&frame).unwrap();
        assert!(json.contains("\"type\":\"device_state\""));
        assert!(json.contains("\"device_type\":\"nuimo\""));
        assert!(json.contains("\"value\":true"));
    }

    #[test]
    fn pong_frame_serializes_as_unit_variant() {
        let json = serde_json::to_string(&EdgeToServer::Pong).unwrap();
        assert!(json.contains("\"type\":\"pong\""));
    }
}
