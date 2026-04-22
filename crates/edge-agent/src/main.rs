//! `edge-agent` binary. Connects to a Nuimo (BLE), to `weave-server`
//! (`/ws/edge`), and to configured services (Roon today; more via features).
//!
//! Flow on startup:
//!   1. Load TOML config (`EDGE_AGENT_*` env overrides applied).
//!   2. Build `RoutingEngine`, prime it from the on-disk cache so local
//!      routing works even if `weave-server` is unreachable.
//!   3. Spawn `WsClient::run()` in the background (reconnect loop).
//!   4. Start service adapters (Roon).
//!   5. Discover Nuimo, connect BLE, subscribe to events.
//!   6. Pump Nuimo events → routing → adapter; pump adapter state →
//!      `/ws/edge` outbox + local glyph feedback.

#[cfg(feature = "hue")]
mod adapter_hue;
#[cfg(feature = "roon")]
mod adapter_roon;
mod config;
mod edge_core;
mod glyphs;
#[cfg(feature = "hue")]
mod hue_token;
#[cfg(feature = "hue")]
mod pair_hue;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

#[cfg(feature = "hue")]
use adapter_hue::{HueAdapter, HueConfig};
#[cfg(feature = "roon")]
use adapter_roon::{RoonAdapter, RoonConfig};
use edge_core::{
    GlyphRegistry, InputPrimitive, Intent, RoutedIntent, RoutingEngine, ServiceAdapter,
    StateUpdate, WsClient,
};
use nuimo::{discover, DisplayOptions, DisplayTransition, NuimoDevice, NuimoEvent, RotationMode};
use weave_contracts::EdgeToServer;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let mut args = std::env::args().skip(1);
    let first = args.next();

    #[cfg(feature = "hue")]
    if first.as_deref() == Some("pair-hue") {
        let remaining: Vec<String> = args.collect();
        return pair_hue::run(&remaining).await;
    }

    let config_path = resolve_config_path(first.as_deref())?;
    let cfg = config::Config::load(&config_path)?;
    tracing::info!(
        edge_id = %cfg.edge_id,
        config_server = %cfg.config_server_url,
        config_path = %config_path.display(),
        "config loaded",
    );

    let engine = Arc::new(RoutingEngine::new());
    let glyphs = Arc::new(GlyphRegistry::new());

    let capabilities = {
        let mut caps = Vec::new();
        if cfg!(feature = "roon") {
            caps.push("roon".to_string());
        }
        if cfg!(feature = "hue") {
            caps.push("hue".to_string());
        }
        caps
    };
    let ws_client = WsClient::new(
        cfg.config_server_url.clone(),
        cfg.edge_id.clone(),
        VERSION.to_string(),
        capabilities,
        engine.clone(),
        glyphs.clone(),
    );
    let ws_outbox = ws_client.outbox();
    let ws_resync = ws_client.resync_sender();

    if let Err(e) = ws_client.prime_from_cache().await {
        tracing::warn!(error = %e, "failed to prime from config cache");
    }
    tokio::spawn(ws_client.run());

    #[cfg(feature = "roon")]
    let roon_adapter: Arc<dyn ServiceAdapter> = {
        let extension_id = cfg
            .roon
            .extension_id
            .clone()
            .unwrap_or_else(|| format!("com.shin1ohno.edge-agent.{}", cfg.edge_id));
        let display_name = cfg
            .roon
            .display_name
            .clone()
            .unwrap_or_else(|| format!("edge-agent ({})", cfg.edge_id));
        let token_path = cfg
            .roon
            .token_path
            .clone()
            .unwrap_or_else(|| default_roon_token_path(&cfg.edge_id));

        let adapter = RoonAdapter::start(RoonConfig {
            extension_id,
            display_name,
            display_version: VERSION.to_string(),
            publisher: cfg
                .roon
                .publisher
                .clone()
                .unwrap_or_else(|| "shin1ohno".into()),
            email: cfg
                .roon
                .email
                .clone()
                .unwrap_or_else(|| "edge-agent@example.invalid".into()),
            host: cfg.roon.host.clone(),
            port: cfg.roon.port,
            token_path,
        })
        .await?;
        Arc::new(adapter)
    };

    // Hue is brought up lazily in a background task so a transient
    // bridge outage (DHCP lease rotation, bridge reboot, internet down)
    // doesn't take the whole edge-agent with it. The watch channel lets
    // the dispatcher see the adapter the moment it appears, and the
    // bootstrap task owns state-pump spawning so Hue state forwards to
    // `/ws/edge` as soon as the adapter is online.
    #[cfg(feature = "hue")]
    let hue_adapter_rx: tokio::sync::watch::Receiver<Option<Arc<dyn ServiceAdapter>>> = {
        let (tx, rx) = tokio::sync::watch::channel(None);
        match cfg.hue.as_ref() {
            Some(hue_cfg) => {
                let path = hue_cfg
                    .token_path
                    .clone()
                    .unwrap_or_else(hue_token::default_path);
                let outbox = ws_outbox.clone();
                let resync = ws_resync.clone();
                tokio::spawn(run_hue_bootstrap(path, tx, outbox, resync));
            }
            None => {
                tracing::info!("no [hue] section in config — hue adapter disabled");
            }
        }
        rx
    };

    // nuimo.skip lets an edge run as a WS-only witness (dashboard / hub
    // validation / multi-edge routing tests without requiring physical BLE
    // hardware on every host).
    if cfg.nuimo.skip {
        tracing::info!("nuimo.skip=true — running WS-only mode (no BLE)");

        #[cfg(feature = "roon")]
        spawn_state_pump(
            roon_adapter.subscribe_state(),
            ws_outbox.clone(),
            ws_resync.subscribe(),
        );

        // Hue state pump is spawned from inside `run_hue_bootstrap` when
        // the adapter first comes online, so nothing to do here.
        #[cfg(feature = "hue")]
        let _ = &hue_adapter_rx;

        tokio::signal::ctrl_c().await?;
        tracing::info!("shutting down");
        return Ok(());
    }

    // Discover a Nuimo. Optionally pin to a specific BLE address.
    tracing::info!("scanning for Nuimo...");
    let (mut discovered_rx, _discovery_handle) = discover().await?;
    let discovered = loop {
        let Some(d) = tokio::time::timeout(Duration::from_secs(60), discovered_rx.recv())
            .await
            .map_err(|_| anyhow::anyhow!("nuimo discovery timed out after 60s"))?
        else {
            anyhow::bail!("nuimo discovery channel closed");
        };
        if let Some(wanted) = cfg.nuimo.ble_address.as_deref() {
            if d.address != wanted {
                tracing::debug!(found = %d.address, wanted, "skipping non-matching Nuimo");
                continue;
            }
        }
        break d;
    };
    tracing::info!(name = %discovered.name, addr = %discovered.address, "nuimo found");

    let device = Arc::new(NuimoDevice::new(discovered.address, &discovered.adapter));
    device.connect().await?;
    device.set_rotation_mode(RotationMode::Continuous).await;
    let device_id = device.id();
    tracing::info!(%device_id, "nuimo connected");

    // Device-state pump for weave-web visibility. `spawn_device_state_pump`
    // caches the last value per property and replays on every ws reconnect,
    // so a weave-server restart does not leave the UI stuck on "offline".
    // Input events bypass the cache (transient; see `emit_input_device_state`).
    //
    // The initial `connected: true` is sent explicitly here because the
    // `NuimoEvent::Connected` emitted by `device.connect()` above fires
    // before the event loop below subscribes, so relying on the event
    // broadcast alone would miss the startup transition.
    let device_state_tx = spawn_device_state_pump(
        "nuimo",
        device_id.clone(),
        ws_outbox.clone(),
        ws_resync.subscribe(),
    );
    let _ = device_state_tx
        .send(("connected".into(), serde_json::json!(true)))
        .await;

    // Best-effort link glyph on connect — skipped if the registry isn't
    // populated yet (first run with no cache and server unreachable).
    if let Some(link) = glyphs.get("link").await {
        let _ = device
            .display_glyph(
                &nuimo::Glyph::from_str(&link.pattern),
                &DisplayOptions {
                    brightness: 1.0,
                    timeout_ms: 3000,
                    transition: DisplayTransition::CrossFade,
                },
            )
            .await;
    }

    // State pump: adapter state → /ws/edge outbox + local glyph feedback.
    //
    // adapter-roon already suppresses unchanged values at the source, so
    // here we only need to throttle BLE-bound writes. Feedback LED writes
    // share the Nuimo's single BLE connection with rotate notifications —
    // volume bar renders are limited to ~10 Hz so the gesture stays smooth.
    //
    // WS-forward and local glyph feedback subscribe to the same adapter
    // broadcast independently: WS-forward goes through `spawn_state_pump`
    // (which handles replay-on-reconnect so weave-server recovers its full
    // snapshot after a restart), and feedback runs on its own task so a
    // stall on the BLE write side can't block outbound WS frames.
    #[cfg(feature = "roon")]
    {
        spawn_state_pump(
            roon_adapter.subscribe_state(),
            ws_outbox.clone(),
            ws_resync.subscribe(),
        );

        let mut state_rx = roon_adapter.subscribe_state();
        let dev = device.clone();
        let glyphs_for_feedback = glyphs.clone();
        let engine_for_feedback = engine.clone();
        tokio::spawn(async move {
            let mut filter = FeedbackFilter::new();
            loop {
                match state_rx.recv().await {
                    Ok(update) => {
                        // Consult mapping-level feedback rules first — they
                        // let a user override e.g. the "paused" glyph per
                        // target from the weave-web UI. Hardcoded defaults
                        // apply when no rule matches (preserves behavior
                        // for deployments that never populated `feedback`).
                        let rules = engine_for_feedback
                            .feedback_rules_for_target(&update.service_type, &update.target)
                            .await;
                        if let Some(plan) = FeedbackPlan::resolve(&update, &rules) {
                            let sig = plan.signature();
                            if filter.should_render(&update, &sig) {
                                plan.execute(&dev, &glyphs_for_feedback).await;
                            }
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(skipped = n, "adapter state lag");
                    }
                }
            }
        });
    }

    // Hue state pump is spawned from `run_hue_bootstrap` when the adapter
    // first comes online; no glyph feedback for Hue — the lights
    // themselves carry the visible confirmation.

    // Dispatch pipeline. Each (service_type, service_target) gets a dedicated
    // worker task that serializes RPCs for that target: one RPC in flight at
    // a time, with natural back-pressure driving the coalescing. While an
    // RPC is awaited, incoming continuous deltas (volume_change etc.) queue
    // and get merged on the next drain — so on a fast knob turn the service
    // sees one large merged change instead of a race of many small ones.
    let (intent_tx, intent_rx) = tokio::sync::mpsc::channel::<RoutedIntent>(256);
    tokio::spawn(run_dispatcher(
        intent_rx,
        #[cfg(feature = "roon")]
        roon_adapter.clone(),
        #[cfg(feature = "hue")]
        hue_adapter_rx.clone(),
    ));

    // Input pump: Nuimo BLE events → routing → coalescer.
    //
    // BLE connections to the Nuimo drop occasionally (peripheral side
    // resets, host BlueZ hiccups). The `event_tx` broadcast inside the
    // nuimo SDK is stable across reconnects, so we keep the same
    // subscription and re-drive `device.connect()` with exponential
    // backoff when a Disconnected event fires.
    let mut events = device.events();
    let device_type = "nuimo";
    loop {
        tokio::select! {
            res = events.recv() => {
                match res {
                    Ok(NuimoEvent::Disconnected) => {
                        tracing::warn!("nuimo BLE disconnected — reconnecting");
                        let _ = device_state_tx
                            .send(("connected".into(), serde_json::json!(false)))
                            .await;
                        reconnect_nuimo(&device).await;
                        let _ = device_state_tx
                            .send(("connected".into(), serde_json::json!(true)))
                            .await;
                    }
                    Ok(NuimoEvent::Connected) => {
                        // Idempotent: the Disconnected arm already re-sent
                        // connected:true after reconnect. This catches any
                        // SDK-initiated Connected emission we didn't trigger
                        // explicitly (defensive, cache-deduped on server).
                        let _ = device_state_tx
                            .send(("connected".into(), serde_json::json!(true)))
                            .await;
                    }
                    Ok(NuimoEvent::BatteryLevel(pct)) => {
                        let _ = device_state_tx
                            .send(("battery".into(), serde_json::json!(pct)))
                            .await;
                    }
                    Ok(NuimoEvent::Rssi(_)) => {
                        // Not surfaced today; skip without routing.
                    }
                    Ok(event) => {
                        // Unconditional input-event forward for observability
                        // (Try It Now, debug dashboards). Sent even when
                        // routing has no mapping — the event itself is the
                        // diagnostic signal.
                        emit_input_device_state(&ws_outbox, &device_id, &event).await;
                        let Some(primitive) = translate_nuimo_event(&event) else { continue; };
                        use edge_core::RouteOutcome;
                        match engine.route_with_mode(device_type, &device_id, &primitive).await {
                            RouteOutcome::Normal(routed) => {
                                for r in routed {
                                    if let Err(e) = intent_tx.try_send(r) {
                                        tracing::warn!(error = %e, "intent channel full; dropping event");
                                    }
                                }
                            }
                            RouteOutcome::EnterSelection { edge_id: _, mapping_id, glyph }
                            | RouteOutcome::UpdateSelection { mapping_id, glyph } => {
                                tracing::info!(%mapping_id, %glyph, "target selection: showing candidate");
                                if let Some(entry) = glyphs.get(&glyph).await {
                                    let n_glyph = nuimo::Glyph::from_str(&entry.pattern);
                                    if let Err(e) = device
                                        .display_glyph(&n_glyph, &DisplayOptions {
                                            brightness: 1.0,
                                            timeout_ms: 10_000,
                                            transition: DisplayTransition::CrossFade,
                                        })
                                        .await
                                    {
                                        tracing::warn!(error = %e, "failed to push selection glyph");
                                    }
                                } else {
                                    tracing::warn!(%glyph, "selection glyph not in registry — skipping LED push");
                                }
                            }
                            RouteOutcome::CommitSelection { edge_id: _, mapping_id, service_target } => {
                                tracing::info!(%mapping_id, %service_target, "target selection: committing");
                                let _ = ws_outbox
                                    .send(EdgeToServer::SwitchTarget { mapping_id, service_target })
                                    .await;
                            }
                            RouteOutcome::CancelSelection { mapping_id } => {
                                tracing::info!(%mapping_id, "target selection: cancelled");
                            }
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(skipped = n, "nuimo event lag");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        tracing::error!("nuimo event broadcast closed — cannot continue");
                        return Ok(());
                    }
                }
            }
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("shutting down");
                return Ok(());
            }
        }
    }
}

/// Retry `device.connect()` with exponential backoff (1s → 30s cap) until
/// it succeeds. Called when a `NuimoEvent::Disconnected` is observed.
async fn reconnect_nuimo(device: &Arc<NuimoDevice>) {
    let mut delay = Duration::from_secs(1);
    let cap = Duration::from_secs(30);
    let mut attempt: u32 = 0;
    loop {
        tokio::time::sleep(delay).await;
        attempt += 1;
        match device.connect().await {
            Ok(()) => {
                tracing::info!(attempt, "nuimo reconnected");
                return;
            }
            Err(e) => {
                tracing::warn!(error = %e, attempt, delay_secs = delay.as_secs(), "reconnect failed");
                delay = (delay * 2).min(cap);
            }
        }
    }
}

/// Resolve the Hue bridge and start its adapter, retrying indefinitely
/// with exponential backoff if the bridge is unreachable. On success,
/// publishes the adapter into the watch channel so the dispatcher picks
/// it up, and spawns the state pump so Hue updates flow to `/ws/edge`.
#[cfg(feature = "hue")]
async fn run_hue_bootstrap(
    token_path: PathBuf,
    tx: tokio::sync::watch::Sender<Option<Arc<dyn ServiceAdapter>>>,
    outbox: tokio::sync::mpsc::Sender<EdgeToServer>,
    resync: tokio::sync::broadcast::Sender<()>,
) {
    let mut delay = Duration::from_secs(0);
    let cap = Duration::from_secs(300);
    let mut attempt: u32 = 0;
    loop {
        if !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }
        attempt += 1;
        match try_hue_init(&token_path).await {
            Ok(adapter) => {
                let arc: Arc<dyn ServiceAdapter> = Arc::new(adapter);
                tracing::info!(attempt, "hue adapter online");
                spawn_state_pump(arc.subscribe_state(), outbox.clone(), resync.subscribe());
                let _ = tx.send(Some(arc));
                return;
            }
            Err(e) => {
                delay = if delay.is_zero() {
                    Duration::from_secs(30)
                } else {
                    (delay * 2).min(cap)
                };
                tracing::warn!(
                    error = %e,
                    attempt,
                    next_retry_secs = delay.as_secs(),
                    "hue adapter init failed — retrying in background",
                );
            }
        }
    }
}

#[cfg(feature = "hue")]
async fn try_hue_init(token_path: &std::path::Path) -> anyhow::Result<HueAdapter> {
    let mut token = hue_token::load(token_path)?;
    let source = adapter_hue::resolve_bridge(&mut token, token_path).await?;
    tracing::debug!(?source, host = %token.host, "hue bridge resolution complete");
    HueAdapter::start(HueConfig {
        host: token.host.clone(),
        app_key: token.app_key.clone(),
    })
    .await
}

/// Fan incoming `RoutedIntent`s out to per-target workers, spawning a new
/// worker the first time we see a given `(service_type, target)` pair.
async fn run_dispatcher(
    mut rx: tokio::sync::mpsc::Receiver<RoutedIntent>,
    #[cfg(feature = "roon")] roon: Arc<dyn ServiceAdapter>,
    #[cfg(feature = "hue")] hue: tokio::sync::watch::Receiver<Option<Arc<dyn ServiceAdapter>>>,
) {
    let mut workers: HashMap<(String, String), tokio::sync::mpsc::Sender<Intent>> = HashMap::new();

    while let Some(r) = rx.recv().await {
        let key = (r.service_type.clone(), r.service_target.clone());

        if !workers.contains_key(&key) {
            let adapter: Option<Arc<dyn ServiceAdapter>> = match key.0.as_str() {
                #[cfg(feature = "roon")]
                "roon" => Some(roon.clone()),
                #[cfg(feature = "hue")]
                "hue" => hue.borrow().clone(),
                _ => None,
            };
            let Some(adapter) = adapter else {
                tracing::warn!(service_type = %key.0, "no adapter for service_type; dropping intent");
                continue;
            };
            let (tx, worker_rx) = tokio::sync::mpsc::channel::<Intent>(64);
            tokio::spawn(run_target_worker(key.clone(), worker_rx, adapter));
            workers.insert(key.clone(), tx);
        }

        let tx = workers.get(&key).expect("worker inserted above");
        if let Err(e) = tx.try_send(r.intent) {
            tracing::warn!(error = %e, ?key, "target worker backlog; dropping intent");
        }
    }
}

/// One worker per `(service_type, target)`. Awaits RPCs serially so only a
/// single request is in flight per target — a gesture's worth of continuous
/// deltas that arrive during one in-flight RPC get merged into one RPC on
/// the next drain. Discrete intents (play, pause, etc.) keep their arrival
/// ordering relative to surrounding continuous intents.
async fn run_target_worker(
    key: (String, String),
    mut rx: tokio::sync::mpsc::Receiver<Intent>,
    adapter: Arc<dyn ServiceAdapter>,
) {
    let (service_type, target) = key;
    while let Some(first) = rx.recv().await {
        let mut pending: Vec<Intent> = Vec::new();
        push_merged(&mut pending, first);
        while let Ok(next) = rx.try_recv() {
            push_merged(&mut pending, next);
        }
        for intent in pending {
            if let Err(e) = adapter.send_intent(&target, &intent).await {
                tracing::warn!(error = %e, %service_type, %target, ?intent, "intent failed");
            }
        }
    }
}

/// Append `intent` to `pending`, merging with the tail when both are the
/// same continuous-delta kind. Preserves ordering for discrete intents.
fn push_merged(pending: &mut Vec<Intent>, intent: Intent) {
    match (pending.last_mut(), &intent) {
        (Some(Intent::VolumeChange { delta: a }), Intent::VolumeChange { delta: b }) => *a += *b,
        (Some(Intent::BrightnessChange { delta: a }), Intent::BrightnessChange { delta: b }) => {
            *a += *b
        }
        (
            Some(Intent::ColorTemperatureChange { delta: a }),
            Intent::ColorTemperatureChange { delta: b },
        ) => *a += *b,
        (Some(Intent::SeekRelative { seconds: a }), Intent::SeekRelative { seconds: b }) => {
            *a += *b
        }
        _ => pending.push(intent),
    }
}

fn spawn_state_pump(
    mut state_rx: tokio::sync::broadcast::Receiver<StateUpdate>,
    outbox: tokio::sync::mpsc::Sender<EdgeToServer>,
    mut resync_rx: tokio::sync::broadcast::Receiver<()>,
) {
    tokio::spawn(async move {
        // Last-write-wins cache keyed by
        // (service_type, target, property, output_id). Replayed on every
        // ws reconnect so weave-server recovers its full snapshot after a
        // restart even when the downstream adapter hasn't seen any state
        // change since the last connection — otherwise idle zones / lights
        // stay missing from the UI until they happen to change.
        let mut last: std::collections::HashMap<
            (String, String, String, Option<String>),
            EdgeToServer,
        > = std::collections::HashMap::new();
        loop {
            tokio::select! {
                res = state_rx.recv() => match res {
                    Ok(update) => {
                        let frame = EdgeToServer::State {
                            service_type: update.service_type.clone(),
                            target: update.target.clone(),
                            property: update.property.clone(),
                            output_id: update.output_id.clone(),
                            value: update.value,
                        };
                        let key = (
                            update.service_type,
                            update.target,
                            update.property,
                            update.output_id,
                        );
                        last.insert(key, frame.clone());
                        let _ = outbox.send(frame).await;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(skipped = n, "adapter state lag");
                    }
                },
                // Lagged/Closed on the resync channel are non-fatal — keep
                // the state pump alive and keep forwarding fresh updates.
                res = resync_rx.recv() => {
                    if let Ok(()) = res {
                        tracing::info!(
                            n = last.len(),
                            "ws reconnect — replaying cached state",
                        );
                        for frame in last.values() {
                            let _ = outbox.send(frame.clone()).await;
                        }
                    }
                }
            }
        }
    });
}

/// Cache-and-replay pump for per-device state updates (connected, battery,
/// …). Mirrors `spawn_state_pump` in intent, but keyed on `property` with
/// `device_type` / `device_id` baked into every emitted frame.
///
/// The returned `Sender` is how the event loop enqueues updates; the
/// spawned task forwards them to `outbox` and keeps a last-write-wins
/// cache so every ws reconnect can replay the latest known state for each
/// property. Input events are transient and bypass this pump — they are
/// forwarded directly by `emit_input_device_state`.
fn spawn_device_state_pump(
    device_type: &'static str,
    device_id: String,
    outbox: tokio::sync::mpsc::Sender<EdgeToServer>,
    mut resync_rx: tokio::sync::broadcast::Receiver<()>,
) -> tokio::sync::mpsc::Sender<(String, serde_json::Value)> {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<(String, serde_json::Value)>(32);
    tokio::spawn(async move {
        let mut last: std::collections::HashMap<String, EdgeToServer> =
            std::collections::HashMap::new();
        loop {
            tokio::select! {
                msg = rx.recv() => match msg {
                    Some((property, value)) => {
                        let frame = EdgeToServer::DeviceState {
                            device_type: device_type.to_string(),
                            device_id: device_id.clone(),
                            property: property.clone(),
                            value,
                        };
                        tracing::debug!(%property, "device state update");
                        last.insert(property, frame.clone());
                        let _ = outbox.send(frame).await;
                    }
                    None => break,
                },
                // Lagged/Closed on the resync channel are non-fatal — keep
                // the pump alive and keep forwarding fresh updates.
                res = resync_rx.recv() => {
                    if let Ok(()) = res {
                        tracing::info!(
                            n = last.len(),
                            "ws reconnect — replaying cached device state",
                        );
                        for frame in last.values() {
                            let _ = outbox.send(frame.clone()).await;
                        }
                    }
                }
            }
        }
    });
    tx
}

/// Forward a `NuimoEvent` as a `DeviceState { property: "input", ... }`
/// frame. Non-input variants (battery, connection, rssi) return without
/// sending — the caller handles those via the pump.
async fn emit_input_device_state(
    outbox: &tokio::sync::mpsc::Sender<EdgeToServer>,
    device_id: &str,
    event: &NuimoEvent,
) {
    let Some(value) = input_event_json(event) else {
        return;
    };
    let frame = EdgeToServer::DeviceState {
        device_type: "nuimo".to_string(),
        device_id: device_id.to_string(),
        property: "input".to_string(),
        value,
    };
    let _ = outbox.send(frame).await;
}

/// Project a `NuimoEvent` into the JSON shape consumed by weave-web's
/// `InputStreamPanel`. Names mirror the edge-core `InputPrimitive` /
/// mapping-route naming (`press`, `rotate`, `swipe_<dir>`, …) so the
/// panel can display them without translation.
fn input_event_json(event: &NuimoEvent) -> Option<serde_json::Value> {
    use serde_json::json;
    Some(match event {
        NuimoEvent::ButtonDown => json!({"input": "press"}),
        NuimoEvent::ButtonUp => json!({"input": "release"}),
        NuimoEvent::Rotate { delta, .. } => json!({"input": "rotate", "delta": delta}),
        NuimoEvent::SwipeUp => json!({"input": "swipe_up"}),
        NuimoEvent::SwipeDown => json!({"input": "swipe_down"}),
        NuimoEvent::SwipeLeft | NuimoEvent::FlyLeft => json!({"input": "swipe_left"}),
        NuimoEvent::SwipeRight | NuimoEvent::FlyRight => json!({"input": "swipe_right"}),
        NuimoEvent::TouchTop => json!({"input": "touch_top"}),
        NuimoEvent::TouchBottom => json!({"input": "touch_bottom"}),
        NuimoEvent::TouchLeft => json!({"input": "touch_left"}),
        NuimoEvent::TouchRight => json!({"input": "touch_right"}),
        NuimoEvent::LongTouchTop => json!({"input": "long_touch_top"}),
        NuimoEvent::LongTouchBottom => json!({"input": "long_touch_bottom"}),
        NuimoEvent::LongTouchLeft => json!({"input": "long_touch_left"}),
        NuimoEvent::LongTouchRight => json!({"input": "long_touch_right"}),
        NuimoEvent::Hover { proximity } => json!({"input": "hover", "proximity": proximity}),
        NuimoEvent::BatteryLevel(_)
        | NuimoEvent::Rssi(_)
        | NuimoEvent::Connected
        | NuimoEvent::Disconnected => return None,
    })
}

fn translate_nuimo_event(event: &NuimoEvent) -> Option<InputPrimitive> {
    use edge_core::{Direction, TouchArea};
    Some(match event {
        NuimoEvent::ButtonDown => InputPrimitive::Press,
        NuimoEvent::ButtonUp => InputPrimitive::Release,
        NuimoEvent::Rotate { delta, .. } => InputPrimitive::Rotate { delta: *delta },
        NuimoEvent::SwipeUp => InputPrimitive::Swipe {
            direction: Direction::Up,
        },
        NuimoEvent::SwipeDown => InputPrimitive::Swipe {
            direction: Direction::Down,
        },
        NuimoEvent::SwipeLeft | NuimoEvent::FlyLeft => InputPrimitive::Swipe {
            direction: Direction::Left,
        },
        NuimoEvent::SwipeRight | NuimoEvent::FlyRight => InputPrimitive::Swipe {
            direction: Direction::Right,
        },
        NuimoEvent::TouchTop => InputPrimitive::Touch {
            area: TouchArea::Top,
        },
        NuimoEvent::TouchBottom => InputPrimitive::Touch {
            area: TouchArea::Bottom,
        },
        NuimoEvent::TouchLeft => InputPrimitive::Touch {
            area: TouchArea::Left,
        },
        NuimoEvent::TouchRight => InputPrimitive::Touch {
            area: TouchArea::Right,
        },
        NuimoEvent::LongTouchTop => InputPrimitive::LongTouch {
            area: TouchArea::Top,
        },
        NuimoEvent::LongTouchBottom => InputPrimitive::LongTouch {
            area: TouchArea::Bottom,
        },
        NuimoEvent::LongTouchLeft => InputPrimitive::LongTouch {
            area: TouchArea::Left,
        },
        NuimoEvent::LongTouchRight => InputPrimitive::LongTouch {
            area: TouchArea::Right,
        },
        NuimoEvent::Hover { proximity } => InputPrimitive::Hover {
            proximity: *proximity,
        },
        // BatteryLevel, Rssi, Connected, Disconnected are handled elsewhere (or not yet).
        _ => return None,
    })
}

/// Two-stage feedback: decide what to draw (`plan`), then check whether that
/// specific frame differs from what's currently on the LED (`should_render`),
/// and only if it does, actually push it over BLE.
///
/// Roon republishes volume during a gesture at a higher cadence than the bar
/// count changes, and emits intermediate values during hardware ramping —
/// dedup'ing on the *rendered* signature (e.g. `vol:5`) means the LED only
/// gets a write when the visible frame actually differs. That eliminates the
/// near-identical rewrites that were reading as "blinking".
#[cfg(feature = "roon")]
enum FeedbackPlan {
    /// Volume bar. `bars` = 0..=9 lit LEDs; `direction` decides which end
    /// of the column fills first (bottom-up for linear 0..=max zones,
    /// top-down for dB zones whose max is 0).
    VolumeBar(u8, glyphs::VolumeDirection),
    /// Named glyph from the registry (play / pause / ...). Holds an owned
    /// String because rule-driven plans pull names from mapping JSON at
    /// runtime — not a `&'static str` as in the hardcoded path.
    NamedGlyph(String),
}

#[cfg(feature = "roon")]
impl FeedbackPlan {
    /// Resolve a StateUpdate against mapping-level `feedback` rules first;
    /// fall back to the hardcoded defaults if no rule covers this update.
    ///
    /// Rule semantics:
    ///   - `rule.state` must equal `update.property`.
    ///   - For `feedback_type == "glyph"`, the `rule.mapping` dict is keyed
    ///     by the stringified update value (for `playback`: `"playing"`,
    ///     `"paused"`, …). The looked-up string is the glyph name.
    ///     Special case: `"volume_bar"` as a glyph name means the hardcoded
    ///     VolumeBar rendering — the dict entry is acting as a display-type
    ///     hint rather than a literal glyph from the registry.
    ///   - Non-matching / unknown feedback types fall through to the
    ///     hardcoded path so existing deployments that never touched the
    ///     field keep working.
    fn resolve(update: &StateUpdate, rules: &[weave_contracts::FeedbackRule]) -> Option<Self> {
        if let Some(plan) = Self::from_rules(update, rules) {
            return Some(plan);
        }
        Self::from(update)
    }

    /// Consult mapping-level feedback rules only. Returns `None` when no
    /// rule covers this update — the caller is expected to try the
    /// hardcoded fallback.
    fn from_rules(update: &StateUpdate, rules: &[weave_contracts::FeedbackRule]) -> Option<Self> {
        for rule in rules {
            if rule.state != update.property {
                continue;
            }
            if rule.feedback_type != "glyph" {
                continue;
            }
            let value_key = match &update.value {
                serde_json::Value::String(s) => s.clone(),
                _ => continue,
            };
            let glyph_name = rule
                .mapping
                .as_object()
                .and_then(|m| m.get(&value_key))
                .and_then(|v| v.as_str())?;
            if glyph_name == "volume_bar" {
                // Hardcoded VolumeBar rendering needs the object-shaped
                // volume payload; a playback string update can't drive it.
                continue;
            }
            return Some(Self::NamedGlyph(glyph_name.to_string()));
        }
        None
    }

    /// Project a StateUpdate into the visible frame it should produce, or
    /// `None` if nothing on the device should change. Hardcoded defaults
    /// used when no mapping-level feedback rule matches.
    fn from(update: &StateUpdate) -> Option<Self> {
        match (update.property.as_str(), &update.value) {
            ("playback", serde_json::Value::String(s)) => match s.as_str() {
                "playing" => Some(Self::NamedGlyph("play".to_string())),
                "paused" | "stopped" => Some(Self::NamedGlyph("pause".to_string())),
                _ => None,
            },
            ("volume", serde_json::Value::Object(obj)) => {
                let value = obj.get("value").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let max = obj.get("max").and_then(|v| v.as_f64()).unwrap_or(100.0);
                let min = obj.get("min").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let vtype = obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
                // "db" zones publish a max of 0 and a negative min
                // (e.g. -80..0). Linear zones publish 0..=100 (or
                // similar) with a positive max.
                let is_db = vtype.eq_ignore_ascii_case("db") || (max <= 0.0 && min < 0.0);
                let span = max - min;
                let ratio = if span > 0.0 {
                    ((value - min) / span).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let bars = (ratio * 9.0).round() as u8;
                let direction = if is_db {
                    glyphs::VolumeDirection::TopDown
                } else {
                    glyphs::VolumeDirection::BottomUp
                };
                Some(Self::VolumeBar(bars, direction))
            }
            _ => None,
        }
    }

    /// Stable identifier for "what's on the LED right now". Filter dedups
    /// on this.
    fn signature(&self) -> String {
        match self {
            Self::VolumeBar(bars, direction) => {
                let d = match direction {
                    glyphs::VolumeDirection::BottomUp => "up",
                    glyphs::VolumeDirection::TopDown => "down",
                };
                format!("vol:{bars}:{d}")
            }
            Self::NamedGlyph(name) => name.clone(),
        }
    }

    async fn execute(&self, device: &NuimoDevice, registry: &GlyphRegistry) {
        let (glyph, transition, timeout_ms) = match self {
            Self::VolumeBar(bars, direction) => (
                glyphs::volume_bars(*bars, *direction),
                DisplayTransition::Immediate,
                3000,
            ),
            Self::NamedGlyph(name) => {
                let Some(entry) = registry.get(name).await else {
                    tracing::debug!(%name, "glyph missing from registry; skipping feedback");
                    return;
                };
                if entry.builtin {
                    tracing::debug!(%name, "glyph is builtin; expected parametric render");
                    return;
                }
                (
                    nuimo::Glyph::from_str(&entry.pattern),
                    DisplayTransition::CrossFade,
                    1000,
                )
            }
        };

        let _ = device
            .display_glyph(
                &glyph,
                &DisplayOptions {
                    brightness: 1.0,
                    timeout_ms,
                    transition,
                },
            )
            .await;
    }
}

/// Gates BLE-bound feedback writes: time throttle for volume (so we don't
/// saturate the single BLE connection), plus dedup on the rendered frame
/// signature (so we skip writes that wouldn't change what's visible).
#[cfg(feature = "roon")]
struct FeedbackFilter {
    last_at: HashMap<(String, String), std::time::Instant>,
    last_sig: HashMap<String, String>,
}

#[cfg(feature = "roon")]
impl FeedbackFilter {
    const MIN_GAP: Duration = Duration::from_millis(100);

    fn new() -> Self {
        Self {
            last_at: HashMap::new(),
            last_sig: HashMap::new(),
        }
    }

    fn should_render(&mut self, update: &StateUpdate, signature: &str) -> bool {
        // Dedup: same visible frame as last write → skip.
        if self.last_sig.get(&update.target).map(String::as_str) == Some(signature) {
            return false;
        }

        // Throttle continuous volume writes to protect BLE bandwidth.
        if matches!(update.property.as_str(), "volume") {
            let key = (update.property.clone(), update.target.clone());
            let now = std::time::Instant::now();
            if let Some(last) = self.last_at.get(&key) {
                if now.duration_since(*last) < Self::MIN_GAP {
                    return false;
                }
            }
            self.last_at.insert(key, now);
        }

        self.last_sig
            .insert(update.target.clone(), signature.to_string());
        true
    }
}

fn default_roon_token_path(edge_id: &str) -> PathBuf {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("state")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("edge-agent")
        .join(format!("roon-token-{}.json", edge_id))
}

/// Resolve the bootstrap config path. Precedence:
///   1. CLI positional argument
///   2. `EDGE_AGENT_CONFIG` env var
///   3. `$XDG_CONFIG_HOME/edge-agent/config.toml` (or `$HOME/.config/edge-agent/config.toml`)
///   4. `/etc/edge-agent/config.toml`
///
/// If none of the candidate paths exist, returns an error listing what was searched.
fn resolve_config_path(cli: Option<&str>) -> anyhow::Result<PathBuf> {
    if let Some(p) = cli {
        return Ok(PathBuf::from(p));
    }
    if let Some(env_val) = std::env::var_os("EDGE_AGENT_CONFIG") {
        return Ok(PathBuf::from(env_val));
    }

    let xdg = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .map(|base| base.join("edge-agent").join("config.toml"));
    let etc = PathBuf::from("/etc/edge-agent/config.toml");

    let mut searched: Vec<PathBuf> = Vec::new();
    if let Some(ref p) = xdg {
        if p.is_file() {
            return Ok(p.clone());
        }
        searched.push(p.clone());
    }
    if etc.is_file() {
        return Ok(etc);
    }
    searched.push(etc);

    let lines: Vec<String> = searched
        .iter()
        .map(|p| format!("  - {}", p.display()))
        .collect();
    anyhow::bail!(
        "no edge-agent config found. Searched:\n  - $EDGE_AGENT_CONFIG (unset)\n{}\nSee docs/config-example.toml in the edge-agent repository for a template.",
        lines.join("\n")
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tokio::sync::{broadcast, mpsc};

    fn drain(outbox: &mut mpsc::Receiver<EdgeToServer>) -> Vec<EdgeToServer> {
        let mut out = Vec::new();
        while let Ok(frame) = outbox.try_recv() {
            out.push(frame);
        }
        out
    }

    fn unwrap_device_state(frame: &EdgeToServer) -> (&str, &str, &str, &serde_json::Value) {
        match frame {
            EdgeToServer::DeviceState {
                device_type,
                device_id,
                property,
                value,
            } => (device_type, device_id, property, value),
            _ => panic!("expected DeviceState, got {:?}", frame),
        }
    }

    #[test]
    fn input_event_json_projects_core_variants() {
        assert_eq!(
            input_event_json(&NuimoEvent::ButtonDown),
            Some(json!({"input": "press"})),
        );
        assert_eq!(
            input_event_json(&NuimoEvent::ButtonUp),
            Some(json!({"input": "release"})),
        );
        assert_eq!(
            input_event_json(&NuimoEvent::Rotate {
                delta: 0.25,
                rotation: 0.0
            }),
            Some(json!({"input": "rotate", "delta": 0.25})),
        );
        assert_eq!(
            input_event_json(&NuimoEvent::SwipeLeft),
            Some(json!({"input": "swipe_left"})),
        );
        assert_eq!(
            input_event_json(&NuimoEvent::FlyRight),
            Some(json!({"input": "swipe_right"})),
            "FlyRight collapses onto swipe_right to mirror translate_nuimo_event",
        );
        assert_eq!(
            input_event_json(&NuimoEvent::TouchTop),
            Some(json!({"input": "touch_top"})),
        );
        assert_eq!(
            input_event_json(&NuimoEvent::LongTouchLeft),
            Some(json!({"input": "long_touch_left"})),
        );
        assert_eq!(
            input_event_json(&NuimoEvent::Hover { proximity: 0.8 }),
            Some(json!({"input": "hover", "proximity": 0.8})),
        );
    }

    #[test]
    fn input_event_json_skips_non_input() {
        assert!(input_event_json(&NuimoEvent::BatteryLevel(87)).is_none());
        assert!(input_event_json(&NuimoEvent::Rssi(-40)).is_none());
        assert!(input_event_json(&NuimoEvent::Connected).is_none());
        assert!(input_event_json(&NuimoEvent::Disconnected).is_none());
    }

    #[tokio::test]
    async fn emit_input_device_state_sends_property_input() {
        let (tx, mut rx) = mpsc::channel::<EdgeToServer>(8);
        emit_input_device_state(&tx, "dev-123", &NuimoEvent::ButtonDown).await;
        let frame = rx.try_recv().expect("frame enqueued");
        let (device_type, device_id, property, value) = unwrap_device_state(&frame);
        assert_eq!(device_type, "nuimo");
        assert_eq!(device_id, "dev-123");
        assert_eq!(property, "input");
        assert_eq!(value, &json!({"input": "press"}));
    }

    #[tokio::test]
    async fn emit_input_device_state_drops_non_input_silently() {
        let (tx, mut rx) = mpsc::channel::<EdgeToServer>(8);
        emit_input_device_state(&tx, "dev-123", &NuimoEvent::BatteryLevel(80)).await;
        emit_input_device_state(&tx, "dev-123", &NuimoEvent::Connected).await;
        emit_input_device_state(&tx, "dev-123", &NuimoEvent::Disconnected).await;
        assert!(rx.try_recv().is_err(), "non-input events must not forward");
    }

    #[tokio::test]
    async fn device_state_pump_forwards_and_caches_updates() {
        let (outbox_tx, mut outbox_rx) = mpsc::channel::<EdgeToServer>(16);
        let (resync_tx, resync_rx) = broadcast::channel::<()>(4);
        let tx = spawn_device_state_pump("nuimo", "dev-1".to_string(), outbox_tx, resync_rx);

        tx.send(("connected".into(), json!(true))).await.unwrap();
        tx.send(("battery".into(), json!(87))).await.unwrap();

        // Allow the pump task to drain both messages.
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        let live = drain(&mut outbox_rx);
        assert_eq!(live.len(), 2);
        let connected = unwrap_device_state(&live[0]);
        assert_eq!(connected.2, "connected");
        assert_eq!(connected.3, &json!(true));
        let battery = unwrap_device_state(&live[1]);
        assert_eq!(battery.2, "battery");
        assert_eq!(battery.3, &json!(87));

        // Fire a resync and assert both cached values are replayed.
        resync_tx.send(()).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        let replayed = drain(&mut outbox_rx);
        assert_eq!(replayed.len(), 2, "both cached properties must replay");
        let props: std::collections::HashSet<&str> =
            replayed.iter().map(|f| unwrap_device_state(f).2).collect();
        assert!(props.contains("connected"));
        assert!(props.contains("battery"));
    }

    #[cfg(feature = "roon")]
    fn state_update(property: &str, value: serde_json::Value) -> StateUpdate {
        StateUpdate {
            service_type: "roon".into(),
            target: "zone-1".into(),
            property: property.into(),
            output_id: None,
            value,
        }
    }

    #[cfg(feature = "roon")]
    fn named_glyph(plan: &FeedbackPlan) -> &str {
        match plan {
            FeedbackPlan::NamedGlyph(s) => s.as_str(),
            _ => panic!("expected NamedGlyph"),
        }
    }

    #[cfg(feature = "roon")]
    #[test]
    fn feedback_plan_from_rules_picks_mapped_glyph() {
        let rules = vec![weave_contracts::FeedbackRule {
            state: "playback".into(),
            feedback_type: "glyph".into(),
            mapping: json!({"playing": "custom_play", "paused": "custom_pause"}),
        }];
        let update = state_update("playback", json!("playing"));
        let plan = FeedbackPlan::from_rules(&update, &rules).expect("rule match");
        assert_eq!(named_glyph(&plan), "custom_play");
    }

    #[cfg(feature = "roon")]
    #[test]
    fn feedback_plan_from_rules_skips_volume_bar_marker() {
        // "volume_bar" as a glyph name requires the object-shaped volume
        // payload; a string-valued playback update can't drive it.
        let rules = vec![weave_contracts::FeedbackRule {
            state: "playback".into(),
            feedback_type: "glyph".into(),
            mapping: json!({"playing": "volume_bar"}),
        }];
        let update = state_update("playback", json!("playing"));
        assert!(FeedbackPlan::from_rules(&update, &rules).is_none());
    }

    #[cfg(feature = "roon")]
    #[test]
    fn feedback_plan_from_rules_ignores_unmatched_state() {
        let rules = vec![weave_contracts::FeedbackRule {
            state: "playback".into(),
            feedback_type: "glyph".into(),
            mapping: json!({"playing": "play"}),
        }];
        let update = state_update("volume", json!({"value": 50.0, "max": 100.0, "min": 0.0}));
        assert!(FeedbackPlan::from_rules(&update, &rules).is_none());
    }

    #[cfg(feature = "roon")]
    #[test]
    fn feedback_plan_resolve_falls_back_to_hardcoded_when_rules_empty() {
        let update = state_update("playback", json!("playing"));
        let plan = FeedbackPlan::resolve(&update, &[]).expect("hardcoded fallback");
        assert_eq!(named_glyph(&plan), "play");
    }

    #[cfg(feature = "roon")]
    #[test]
    fn feedback_plan_resolve_prefers_rule_over_hardcoded() {
        let rules = vec![weave_contracts::FeedbackRule {
            state: "playback".into(),
            feedback_type: "glyph".into(),
            mapping: json!({"playing": "custom_play"}),
        }];
        let update = state_update("playback", json!("playing"));
        let plan = FeedbackPlan::resolve(&update, &rules).expect("rule wins");
        assert_eq!(named_glyph(&plan), "custom_play");
    }

    #[tokio::test]
    async fn device_state_pump_replays_only_latest_per_property() {
        let (outbox_tx, mut outbox_rx) = mpsc::channel::<EdgeToServer>(16);
        let (resync_tx, resync_rx) = broadcast::channel::<()>(4);
        let tx = spawn_device_state_pump("nuimo", "dev-1".to_string(), outbox_tx, resync_rx);

        tx.send(("battery".into(), json!(80))).await.unwrap();
        tx.send(("battery".into(), json!(60))).await.unwrap();
        tx.send(("battery".into(), json!(55))).await.unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        drain(&mut outbox_rx); // consume the live stream

        resync_tx.send(()).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        let replayed = drain(&mut outbox_rx);
        assert_eq!(replayed.len(), 1, "cache is keyed by property");
        let (_, _, property, value) = unwrap_device_state(&replayed[0]);
        assert_eq!(property, "battery");
        assert_eq!(value, &json!(55), "latest value wins");
    }
}
