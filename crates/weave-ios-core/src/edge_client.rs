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
use std::sync::Arc;
use std::time::Duration;

use edge_core::RoutingEngine;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::{mpsc, Mutex};
use tokio_tungstenite::tungstenite::protocol::Message;
use weave_contracts::{EdgeToServer, PatchOp, ServerToEdge};

use crate::WeaveError;

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
}

#[derive(uniffi::Object)]
pub struct EdgeClient {
    shutdown_tx: Mutex<Option<mpsc::Sender<()>>>,
    outbox_tx: mpsc::Sender<OutboundCommand>,
    /// Held so the routing engine outlives the WS loop. Read by the
    /// adapter-dispatch surface added in the follow-up `ios_media` PR.
    #[allow(dead_code)]
    engine: Arc<RoutingEngine>,
}

const OUTBOX_CAPACITY: usize = 64;

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
        let engine = Arc::new(RoutingEngine::new());

        tokio::spawn(run_ws_loop(
            ws_url,
            edge_id,
            capabilities,
            sink,
            shutdown_rx,
            outbox_rx,
            engine.clone(),
        ));

        Ok(Arc::new(Self {
            shutdown_tx: Mutex::new(Some(shutdown_tx)),
            outbox_tx,
            engine,
        }))
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
) {
    let mut backoff = Duration::from_millis(500);
    let max_backoff = Duration::from_secs(15);
    // Last-write-wins replay cache, keyed by (device_type, device_id, property).
    let mut state_cache: HashMap<(String, String, String), serde_json::Value> = HashMap::new();
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
                        for ((device_type, device_id, property), value) in &state_cache {
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
                                            state_cache.insert(
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
                                                    apply_inbound_frame(&engine, other).await;
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

/// Apply an inbound `ServerToEdge` frame (other than `Ping`, handled by the
/// WS loop directly) to the local routing engine. Returns immediately when
/// the frame does not affect routing state — `GlyphsUpdate` and
/// `TargetSwitch` are not yet wired to a feedback layer on iOS.
///
/// Extracted as a free async fn so unit tests can exercise the
/// frame-to-engine plumbing without standing up a WebSocket server.
async fn apply_inbound_frame(engine: &RoutingEngine, frame: ServerToEdge) {
    match frame {
        ServerToEdge::ConfigFull { config } => {
            let count = config.mappings.len();
            engine.replace_all(config.mappings).await;
            tracing::info!(mapping_count = count, "ws/edge: config_full applied");
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
        ServerToEdge::GlyphsUpdate { glyphs } => {
            // Glyph rendering is not yet wired on iOS — log and skip.
            // PR introducing on-device LED feedback will surface this to a
            // GlyphRegistry alongside the engine.
            tracing::debug!(
                count = glyphs.len(),
                "ws/edge: glyphs_update (not yet rendered)"
            );
        }
        ServerToEdge::Ping => {
            // Caller handles Pong reply directly to keep the WS handle scoped.
            unreachable!("apply_inbound_frame must not be called with Ping");
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

    #[tokio::test]
    async fn config_full_replaces_engine_state() {
        let engine = RoutingEngine::new();
        let mapping = ios_media_mapping("nuimo-1");
        let config = EdgeConfig {
            edge_id: "ipad".into(),
            mappings: vec![mapping.clone()],
            glyphs: vec![],
        };

        apply_inbound_frame(&engine, ServerToEdge::ConfigFull { config }).await;

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
        let first = ios_media_mapping("nuimo-1");
        engine.replace_all(vec![first]).await;

        let config = EdgeConfig {
            edge_id: "ipad".into(),
            mappings: vec![],
            glyphs: vec![],
        };
        apply_inbound_frame(&engine, ServerToEdge::ConfigFull { config }).await;

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
        let mapping = ios_media_mapping("nuimo-1");
        let mapping_id = mapping.mapping_id;

        apply_inbound_frame(
            &engine,
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
        let mapping = ios_media_mapping("nuimo-1");
        let mapping_id = mapping.mapping_id;
        engine.replace_all(vec![mapping]).await;

        apply_inbound_frame(
            &engine,
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
        let mapping = ios_media_mapping("nuimo-1");
        let mapping_id = mapping.mapping_id;
        engine.replace_all(vec![mapping]).await;

        apply_inbound_frame(
            &engine,
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
        let mapping = ios_media_mapping("nuimo-1");
        engine.replace_all(vec![mapping.clone()]).await;

        apply_inbound_frame(
            &engine,
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
