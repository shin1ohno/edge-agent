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
#[cfg(feature = "macos")]
mod adapter_macos;
#[cfg(feature = "roon")]
mod adapter_roon;
mod config;
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
#[cfg(feature = "macos")]
use adapter_macos::{MacosAdapter, MacosConfig};
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
        if cfg!(feature = "macos") {
            caps.push("macos".to_string());
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

    // Like Hue: lazy bootstrap so a broker that isn't up yet doesn't crash
    // the agent. The watch channel lets the dispatcher pick the adapter up
    // the moment it's online, and the bootstrap task owns state-pump spawn
    // so macOS updates forward to /ws/edge immediately.
    #[cfg(feature = "macos")]
    let macos_adapter_rx: tokio::sync::watch::Receiver<Option<Arc<dyn ServiceAdapter>>> = {
        let (tx, rx) = tokio::sync::watch::channel(None);
        match cfg.macos.as_ref() {
            Some(macos_cfg) => {
                let macos_cfg = macos_cfg.clone();
                let outbox = ws_outbox.clone();
                let resync = ws_resync.clone();
                tokio::spawn(run_macos_bootstrap(macos_cfg, tx, outbox, resync));
            }
            None => {
                tracing::info!("no [macos] section in config — macos adapter disabled");
            }
        }
        rx
    };

    // Global adapter → WS state pump and intent dispatcher. Spawned before
    // the Nuimo supervisor so service state continues to flow to
    // `/ws/edge` even in WS-only mode (no allowlist entries) or while the
    // first Nuimo hasn't connected yet.
    #[cfg(feature = "roon")]
    spawn_state_pump(
        roon_adapter.subscribe_state(),
        ws_outbox.clone(),
        ws_resync.subscribe(),
    );

    let (intent_tx, intent_rx) = tokio::sync::mpsc::channel::<RoutedIntent>(256);
    tokio::spawn(run_dispatcher(
        intent_rx,
        ws_outbox.clone(),
        #[cfg(feature = "roon")]
        roon_adapter.clone(),
        #[cfg(feature = "hue")]
        hue_adapter_rx.clone(),
        #[cfg(feature = "macos")]
        macos_adapter_rx.clone(),
    ));

    // WS-only mode: (a) explicit `nuimo.skip=true`, or (b) empty
    // `ble_addresses` allowlist. Either way, no BLE scan is started.
    if cfg.nuimo.skip || cfg.nuimo.ble_addresses.is_empty() {
        if cfg.nuimo.skip {
            tracing::info!("nuimo.skip=true — running WS-only mode (no BLE)");
        } else {
            tracing::warn!(
                "nuimo.ble_addresses is empty — running WS-only (no Nuimos bound to this edge)",
            );
        }
        // Keep `hue_adapter_rx` / `macos_adapter_rx` alive: their bootstrap
        // tasks own their state pumps and depend on the watch channel
        // receiver staying in scope.
        #[cfg(feature = "hue")]
        let _ = &hue_adapter_rx;
        #[cfg(feature = "macos")]
        let _ = &macos_adapter_rx;

        tokio::signal::ctrl_c().await?;
        tracing::info!("shutting down");
        return Ok(());
    }

    let deps = NuimoDeps {
        engine: engine.clone(),
        glyphs: glyphs.clone(),
        ws_outbox: ws_outbox.clone(),
        ws_resync: ws_resync.clone(),
        intent_tx,
        #[cfg(feature = "roon")]
        roon_adapter: roon_adapter.clone(),
        #[cfg(feature = "hue")]
        hue_adapter_rx: hue_adapter_rx.clone(),
    };

    run_nuimo_supervisor(cfg.nuimo.ble_addresses, deps).await
}

/// Shared context every per-device Nuimo task needs. Collected into one
/// struct so `connect_and_spawn` doesn't grow an ever-widening signature
/// as new features land.
#[derive(Clone)]
struct NuimoDeps {
    engine: Arc<RoutingEngine>,
    glyphs: Arc<GlyphRegistry>,
    ws_outbox: tokio::sync::mpsc::Sender<EdgeToServer>,
    ws_resync: tokio::sync::broadcast::Sender<()>,
    intent_tx: tokio::sync::mpsc::Sender<RoutedIntent>,
    #[cfg(feature = "roon")]
    roon_adapter: Arc<dyn ServiceAdapter>,
    #[cfg(feature = "hue")]
    hue_adapter_rx: tokio::sync::watch::Receiver<Option<Arc<dyn ServiceAdapter>>>,
}

/// Long-running task that consumes `nuimo::discover()` forever and brings
/// each allowlisted Nuimo online in parallel. Handles hot-plug: a Nuimo
/// powered on after edge-agent startup is picked up on the next
/// discovery sweep without restart.
///
/// `allowlist` is the set of BLE addresses (case-insensitive) the edge
/// is allowed to bind. Discoveries outside the list are logged at debug
/// and ignored. Duplicate discoveries for an already-supervised address
/// are silently skipped.
async fn run_nuimo_supervisor(allowlist: Vec<String>, deps: NuimoDeps) -> anyhow::Result<()> {
    let allowlist = build_allowlist(&allowlist);
    tracing::info!(
        allowlist_count = allowlist.len(),
        "scanning for Nuimo (multi-device supervisor)",
    );

    let (mut discovered_rx, _discovery_handle) = discover().await?;
    // Tracked devices: key = uppercased BLE address so lookup is
    // case-insensitive. Value carries the device + spawned task handles
    // (handles kept solely to keep the tasks owned by the supervisor so
    // they live as long as the process; we do not poll or abort them).
    let mut tracked: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut devices: HashMap<String, NuimoContext> = HashMap::new();

    loop {
        tokio::select! {
            maybe_d = discovered_rx.recv() => {
                let Some(d) = maybe_d else {
                    tracing::warn!("nuimo discovery channel closed — supervisor exiting");
                    return Ok(());
                };
                match supervisor_decision(&d.address, &allowlist, &tracked) {
                    SupervisorDecision::Ignore => {
                        tracing::debug!(found = %d.address, "nuimo not in allowlist — ignoring");
                        continue;
                    }
                    SupervisorDecision::AlreadyTracked => {
                        // Re-discovery of an already-connected Nuimo (BlueZ
                        // cache sweep). Ignore silently.
                        continue;
                    }
                    SupervisorDecision::Admit(key) => {
                        tracing::info!(name = %d.name, addr = %d.address, "nuimo found");
                        match connect_and_spawn(d, deps.clone()).await {
                            Ok(ctx) => {
                                tracked.insert(key.clone());
                                devices.insert(key, ctx);
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, key = %key, "failed to bring up nuimo");
                            }
                        }
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

/// Normalize a list of BLE addresses into a case-insensitive allowlist.
/// Trims whitespace and skips empty entries; duplicates are collapsed by
/// the `HashSet`.
fn build_allowlist(addrs: &[String]) -> std::collections::HashSet<String> {
    addrs
        .iter()
        .map(|s| s.trim().to_ascii_uppercase())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Pure decision for a single discovered Nuimo: admit, already tracked,
/// or ignore (not in allowlist). Extracted from the supervisor loop so
/// it can be unit tested without BLE hardware.
#[derive(Debug, PartialEq, Eq)]
enum SupervisorDecision {
    /// Admit a new device. Carries the uppercased key so the caller can
    /// insert into its tracking set without re-normalizing.
    Admit(String),
    /// Address matches the allowlist but is already being supervised.
    AlreadyTracked,
    /// Address is not in the allowlist.
    Ignore,
}

fn supervisor_decision(
    address: &str,
    allowlist: &std::collections::HashSet<String>,
    tracked: &std::collections::HashSet<String>,
) -> SupervisorDecision {
    let key = address.trim().to_ascii_uppercase();
    if !allowlist.contains(&key) {
        return SupervisorDecision::Ignore;
    }
    if tracked.contains(&key) {
        return SupervisorDecision::AlreadyTracked;
    }
    SupervisorDecision::Admit(key)
}

/// Per-device state kept by the supervisor. Holds the `Arc<NuimoDevice>`
/// plus the `JoinHandle`s for every task spawned on behalf of the device;
/// the handles exist only to tie the tasks' lifetimes to the supervisor
/// entry.
#[allow(dead_code)]
struct NuimoContext {
    device: Arc<NuimoDevice>,
    device_id: String,
    handles: Vec<tokio::task::JoinHandle<()>>,
}

/// Bring one discovered Nuimo online: connect BLE, register device-state
/// pump, render the link glyph, and spawn the per-device feedback pumps
/// and event loop. Returns the tracking context; dropping it does NOT
/// disconnect the device — the tasks outlive their handles.
async fn connect_and_spawn(
    discovered: nuimo::DiscoveredNuimo,
    deps: NuimoDeps,
) -> anyhow::Result<NuimoContext> {
    let device = Arc::new(NuimoDevice::new(discovered.address, &discovered.adapter));
    device.connect().await?;
    device.set_rotation_mode(RotationMode::Continuous).await;
    let device_id = device.id();
    tracing::info!(%device_id, "nuimo connected");

    // Device-state pump for weave-web visibility. Cache + replay on every
    // ws resync so weave-server restarts don't leave the UI stuck on
    // stale state. Initial `connected: true` is sent here because the
    // `NuimoEvent::Connected` emitted by `device.connect()` above fires
    // before the event loop subscribes.
    let device_state_tx = spawn_device_state_pump(
        "nuimo",
        device_id.clone(),
        deps.ws_outbox.clone(),
        deps.ws_resync.subscribe(),
    );
    let _ = device_state_tx
        .send(("connected".into(), serde_json::json!(true)))
        .await;

    // Best-effort link glyph on connect — skipped if the registry isn't
    // populated yet (first run with no cache and server unreachable).
    if let Some(link) = deps.glyphs.get("link").await {
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

    let mut handles: Vec<tokio::task::JoinHandle<()>> = Vec::new();

    // Per-device feedback pumps. Each pump subscribes to an adapter's
    // state broadcast independently and filters by (device_type,
    // device_id) so a second Nuimo mapped elsewhere never reacts to
    // unrelated service activity.
    #[cfg(feature = "roon")]
    {
        handles.push(tokio::spawn(run_feedback_pump(
            deps.roon_adapter.subscribe_state(),
            device.clone(),
            device_id.clone(),
            deps.glyphs.clone(),
            deps.engine.clone(),
        )));
    }

    #[cfg(all(feature = "roon", feature = "hue"))]
    {
        let dev = device.clone();
        let dev_id = device_id.clone();
        let glyphs_for_feedback = deps.glyphs.clone();
        let engine_for_feedback = deps.engine.clone();
        let mut hue_rx_watch = deps.hue_adapter_rx.clone();
        handles.push(tokio::spawn(async move {
            let adapter = loop {
                if let Some(a) = hue_rx_watch.borrow().clone() {
                    break a;
                }
                if hue_rx_watch.changed().await.is_err() {
                    return;
                }
            };
            run_feedback_pump(
                adapter.subscribe_state(),
                dev,
                dev_id,
                glyphs_for_feedback,
                engine_for_feedback,
            )
            .await;
        }));
    }

    // Event loop: consumes `device.events()` forever, translates to
    // routed intents, drives reconnect on disconnect. One task per
    // Nuimo; tasks are fully independent so Nuimo A's reconnect never
    // blocks Nuimo B's event flow.
    let event_loop = tokio::spawn(run_nuimo_event_loop(
        device.clone(),
        device_id.clone(),
        device_state_tx,
        deps.engine.clone(),
        deps.intent_tx.clone(),
        deps.glyphs.clone(),
        deps.ws_outbox.clone(),
    ));
    handles.push(event_loop);

    Ok(NuimoContext {
        device,
        device_id,
        handles,
    })
}

/// Per-device event pump. Runs forever (until the broadcast closes or
/// the runtime shuts down). Owns reconnect on `Disconnected`, routing of
/// input events, and selection-mode LED rendering for one Nuimo.
async fn run_nuimo_event_loop(
    device: Arc<NuimoDevice>,
    device_id: String,
    device_state_tx: tokio::sync::mpsc::Sender<(String, serde_json::Value)>,
    engine: Arc<RoutingEngine>,
    intent_tx: tokio::sync::mpsc::Sender<RoutedIntent>,
    glyphs: Arc<GlyphRegistry>,
    ws_outbox: tokio::sync::mpsc::Sender<EdgeToServer>,
) {
    let mut events = device.events();
    let device_type = "nuimo";
    loop {
        match events.recv().await {
            Ok(NuimoEvent::Disconnected) => {
                tracing::warn!(%device_id, "nuimo BLE disconnected — reconnecting");
                let _ = ws_outbox
                    .send(EdgeToServer::Error {
                        context: "nuimo.ble".into(),
                        message: format!("{device_id}: disconnected — reconnecting"),
                        severity: weave_contracts::ErrorSeverity::Warn,
                    })
                    .await;
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
                emit_input_device_state(&ws_outbox, &device_id, &event).await;
                let Some(primitive) = translate_nuimo_event(&event) else {
                    continue;
                };
                use edge_core::RouteOutcome;
                match engine
                    .route_with_mode(device_type, &device_id, &primitive)
                    .await
                {
                    RouteOutcome::Normal(routed) => {
                        for r in routed {
                            if let Err(e) = intent_tx.try_send(r) {
                                tracing::warn!(error = %e, "intent channel full; dropping event");
                            }
                        }
                    }
                    RouteOutcome::EnterSelection {
                        edge_id: _,
                        mapping_id,
                        glyph,
                    }
                    | RouteOutcome::UpdateSelection { mapping_id, glyph } => {
                        tracing::info!(%mapping_id, %glyph, "target selection: showing candidate");
                        if let Some(entry) = glyphs.get(&glyph).await {
                            let n_glyph = nuimo::Glyph::from_str(&entry.pattern);
                            if let Err(e) = device
                                .display_glyph(
                                    &n_glyph,
                                    &DisplayOptions {
                                        brightness: 1.0,
                                        timeout_ms: 10_000,
                                        transition: DisplayTransition::CrossFade,
                                    },
                                )
                                .await
                            {
                                tracing::warn!(error = %e, "failed to push selection glyph");
                            }
                        } else {
                            tracing::warn!(%glyph, "selection glyph not in registry — skipping LED push");
                        }
                    }
                    RouteOutcome::CommitSelection {
                        edge_id: _,
                        mapping_id,
                        service_target,
                    } => {
                        tracing::info!(%mapping_id, %service_target, "target selection: committing");
                        let _ = ws_outbox
                            .send(EdgeToServer::SwitchTarget {
                                mapping_id,
                                service_target,
                            })
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
                tracing::error!(%device_id, "nuimo event broadcast closed — event loop exiting");
                return;
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
                let _ = outbox
                    .send(EdgeToServer::Error {
                        context: "hue.bootstrap".into(),
                        message: format!(
                            "init failed (attempt {attempt}, next retry {}s): {e}",
                            delay.as_secs()
                        ),
                        severity: weave_contracts::ErrorSeverity::Warn,
                    })
                    .await;
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

/// Bring up the macOS adapter, retrying indefinitely with exponential
/// backoff (start 30s, cap 300s) when the broker is unreachable. On
/// success, publishes the adapter into the watch channel so the
/// dispatcher picks it up, and spawns the state pump so macOS updates
/// flow to `/ws/edge`.
#[cfg(feature = "macos")]
async fn run_macos_bootstrap(
    cfg: config::MacosSection,
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
        match try_macos_init(&cfg).await {
            Ok(adapter) => {
                let arc: Arc<dyn ServiceAdapter> = Arc::new(adapter);
                tracing::info!(attempt, "macos adapter online");
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
                    "macos adapter init failed — retrying in background",
                );
                let _ = outbox
                    .send(EdgeToServer::Error {
                        context: "macos.bootstrap".into(),
                        message: format!(
                            "init failed (attempt {attempt}, next retry {}s): {e}",
                            delay.as_secs()
                        ),
                        severity: weave_contracts::ErrorSeverity::Warn,
                    })
                    .await;
            }
        }
    }
}

#[cfg(feature = "macos")]
async fn try_macos_init(cfg: &config::MacosSection) -> anyhow::Result<MacosAdapter> {
    MacosAdapter::start(MacosConfig {
        mqtt_host: cfg.mqtt_host.clone(),
        mqtt_port: cfg.mqtt_port,
        mqtt_client_id: cfg.mqtt_client_id.clone(),
    })
    .await
}

/// Fan incoming `RoutedIntent`s out to per-target workers, spawning a new
/// worker the first time we see a given `(service_type, target)` pair.
async fn run_dispatcher(
    mut rx: tokio::sync::mpsc::Receiver<RoutedIntent>,
    outbox: tokio::sync::mpsc::Sender<EdgeToServer>,
    #[cfg(feature = "roon")] roon: Arc<dyn ServiceAdapter>,
    #[cfg(feature = "hue")] hue: tokio::sync::watch::Receiver<Option<Arc<dyn ServiceAdapter>>>,
    #[cfg(feature = "macos")] macos: tokio::sync::watch::Receiver<Option<Arc<dyn ServiceAdapter>>>,
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
                #[cfg(feature = "macos")]
                "macos" => macos.borrow().clone(),
                _ => None,
            };
            let Some(adapter) = adapter else {
                tracing::warn!(service_type = %key.0, "no adapter for service_type; dropping intent");
                continue;
            };
            let (tx, worker_rx) = tokio::sync::mpsc::channel::<Intent>(64);
            tokio::spawn(run_target_worker(
                key.clone(),
                worker_rx,
                adapter,
                outbox.clone(),
            ));
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
    outbox: tokio::sync::mpsc::Sender<EdgeToServer>,
) {
    let (service_type, target) = key;
    while let Some(first) = rx.recv().await {
        let mut pending: Vec<Intent> = Vec::new();
        push_merged(&mut pending, first);
        while let Ok(next) = rx.try_recv() {
            push_merged(&mut pending, next);
        }
        for intent in pending {
            let started = std::time::Instant::now();
            let outcome = adapter.send_intent(&target, &intent).await;
            let latency_ms = u32::try_from(started.elapsed().as_millis()).ok();
            let (intent_name, params) = split_intent(&intent);
            let result = match &outcome {
                Ok(()) => weave_contracts::CommandResult::Ok,
                Err(e) => weave_contracts::CommandResult::Err {
                    message: e.to_string(),
                },
            };
            let frame = EdgeToServer::Command {
                service_type: service_type.clone(),
                target: target.clone(),
                intent: intent_name,
                params,
                result,
                latency_ms,
                output_id: None,
            };
            let _ = outbox.send(frame).await;
            if let Err(e) = outcome {
                tracing::warn!(error = %e, %service_type, %target, ?intent, "intent failed");
            }
        }
    }
}

/// Serialize an `Intent` into its snake-case tag and the remaining params
/// object. Relies on `#[serde(tag = "type")]` on `Intent`, so the output
/// is always a JSON object with a `"type"` key that we lift out. Payload-
/// less variants yield `{}`.
fn split_intent(intent: &Intent) -> (String, serde_json::Value) {
    match serde_json::to_value(intent) {
        Ok(serde_json::Value::Object(mut map)) => {
            let name = map
                .remove("type")
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_else(|| "unknown".to_string());
            (name, serde_json::Value::Object(map))
        }
        _ => ("unknown".to_string(), serde_json::json!({})),
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

/// Nuimo LED feedback loop. Subscribes to an adapter's state broadcast
/// (Roon or Hue today), consults mapping-level `feedback` rules with
/// hardcoded fallback, and pushes the resulting glyph/bar to the Nuimo
/// — deduped by the rendered signature so identical frames don't
/// monopolise the BLE write side.
///
/// Exits when the upstream broadcast closes. Lag events are logged and
/// the pump keeps running — a transient spike shouldn't wedge feedback
/// for the rest of the session.
#[cfg(feature = "roon")]
async fn run_feedback_pump(
    mut state_rx: tokio::sync::broadcast::Receiver<StateUpdate>,
    device: Arc<NuimoDevice>,
    device_id: String,
    glyphs: Arc<GlyphRegistry>,
    engine: Arc<RoutingEngine>,
) {
    let mut filter = FeedbackFilter::new();
    loop {
        match state_rx.recv().await {
            Ok(update) => {
                // Scoped to this device's mappings: `None` means this
                // Nuimo owns no mapping for the target (skip, another
                // Nuimo may handle it); `Some(vec)` means a mapping
                // exists and we hand off to `FeedbackPlan::resolve`,
                // which falls back to hardcoded defaults when the user
                // configured no explicit rules.
                let Some(rules) = engine
                    .feedback_rules_for_device_target(
                        "nuimo",
                        &device_id,
                        &update.service_type,
                        &update.target,
                    )
                    .await
                else {
                    continue;
                };
                if let Some(plan) = FeedbackPlan::resolve(&update, &rules) {
                    let sig = plan.signature();
                    if filter.should_render(&update, &sig) {
                        plan.execute(&device, &glyphs).await;
                    }
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!(skipped = n, "adapter state lag");
            }
        }
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
/// Project a state-update value into a 9-bar fill + direction.
///
/// Two inputs are recognised:
///   * `Number` — a raw scalar assumed to be on the closed interval
///     `0..=100` (Hue brightness). Rendered bottom-up.
///   * `Object` — a Roon-style `{value, min, max, type?}` envelope. Reads
///     the full range, treats `type="db"` (or an all-negative range) as a
///     top-down dB meter, and otherwise renders bottom-up.
///
/// Returns `None` for other shapes so the caller can decide whether to
/// skip or fall through to a named-glyph plan.
#[cfg(feature = "roon")]
fn volume_bar_from_value(value: &serde_json::Value) -> Option<(u8, glyphs::VolumeDirection)> {
    match value {
        serde_json::Value::Number(_) => {
            let v = value.as_f64()?;
            let ratio = (v / 100.0).clamp(0.0, 1.0);
            let bars = (ratio * 9.0).round() as u8;
            Some((bars, glyphs::VolumeDirection::BottomUp))
        }
        serde_json::Value::Object(obj) => {
            let value = obj.get("value").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let max = obj.get("max").and_then(|v| v.as_f64()).unwrap_or(100.0);
            let min = obj.get("min").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let vtype = obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
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
            Some((bars, direction))
        }
        _ => None,
    }
}

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
    ///   - `feedback_type == "glyph"`: the `rule.mapping` dict is keyed by
    ///     the stringified update value (for `playback`: `"playing"`,
    ///     `"paused"`, …). The looked-up string is the glyph name. If the
    ///     glyph name is `"volume_bar"`, the update's numeric value drives
    ///     a VolumeBar instead of a registry lookup — this lets a single
    ///     "glyph" rule carry both named-glyph and bar-style targets.
    ///   - `feedback_type == "volume_bar"`: the update's value drives a
    ///     VolumeBar directly, with no `mapping` lookup. Works with either
    ///     a raw number (Hue brightness, 0..=100) or a Roon-style object
    ///     `{value, min, max, type}`.
    ///   - Unknown feedback types fall through to the hardcoded path so
    ///     existing deployments that never touched the field keep working.
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
            match rule.feedback_type.as_str() {
                "glyph" => {
                    let value_key = match &update.value {
                        serde_json::Value::String(s) => s.clone(),
                        _ => continue,
                    };
                    let Some(glyph_name) = rule
                        .mapping
                        .as_object()
                        .and_then(|m| m.get(&value_key))
                        .and_then(|v| v.as_str())
                    else {
                        continue;
                    };
                    if glyph_name == "volume_bar" {
                        // "volume_bar" as a glyph name is a display-type
                        // hint, not a registry entry — render a bar from
                        // the value if it's numeric. A string-valued
                        // playback update cannot drive a bar, so skip.
                        if let Some((bars, dir)) = volume_bar_from_value(&update.value) {
                            return Some(Self::VolumeBar(bars, dir));
                        }
                        continue;
                    }
                    return Some(Self::NamedGlyph(glyph_name.to_string()));
                }
                "volume_bar" => {
                    if let Some((bars, dir)) = volume_bar_from_value(&update.value) {
                        return Some(Self::VolumeBar(bars, dir));
                    }
                }
                _ => continue,
            }
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
            ("volume", _) | ("brightness", _) => {
                // Roon volumes arrive as `{value, min, max, type}` objects;
                // Hue brightness arrives as a raw 0..=100 number. Both
                // project onto the same 9-bar display — the helper picks
                // the right parsing based on value shape.
                volume_bar_from_value(&update.value).map(|(bars, dir)| Self::VolumeBar(bars, dir))
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
    fn feedback_plan_from_rules_volume_bar_marker_skips_string_value() {
        // A "volume_bar" entry in a glyph mapping needs a numeric or
        // object-shaped value to derive the bar count from. A string-valued
        // playback update has no bar height to project, so the rule is
        // skipped and the caller falls back to hardcoded.
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
    fn feedback_plan_from_rules_volume_bar_type_on_brightness() {
        // Hue brightness arrives as a raw 0..=100 number. A
        // `feedback_type: "volume_bar"` rule should render a bottom-up
        // bar without needing a `mapping` dict.
        let rules = vec![weave_contracts::FeedbackRule {
            state: "brightness".into(),
            feedback_type: "volume_bar".into(),
            mapping: json!({}),
        }];
        let update = state_update("brightness", json!(75.0));
        let plan = FeedbackPlan::from_rules(&update, &rules).expect("rule match");
        match plan {
            FeedbackPlan::VolumeBar(bars, glyphs::VolumeDirection::BottomUp) => {
                // 75/100 * 9 = 6.75, rounds to 7
                assert_eq!(bars, 7);
            }
            _ => panic!("expected VolumeBar BottomUp"),
        }
    }

    #[cfg(feature = "roon")]
    #[test]
    fn feedback_plan_from_hardcoded_renders_brightness_as_bar() {
        let update = state_update("brightness", json!(0));
        let plan = FeedbackPlan::from(&update).expect("hardcoded brightness");
        match plan {
            FeedbackPlan::VolumeBar(bars, glyphs::VolumeDirection::BottomUp) => {
                assert_eq!(bars, 0, "0% brightness = 0 bars");
            }
            _ => panic!("expected VolumeBar BottomUp"),
        }
        let update = state_update("brightness", json!(100.0));
        let plan = FeedbackPlan::from(&update).expect("hardcoded brightness");
        match plan {
            FeedbackPlan::VolumeBar(bars, _) => assert_eq!(bars, 9),
            _ => panic!("expected VolumeBar"),
        }
    }

    #[cfg(feature = "roon")]
    #[test]
    fn volume_bar_from_value_handles_db_object() {
        // Roon dB zones publish max=0, min=-80 (or similar). The helper
        // should flip to TopDown so a quieter setting reads as fewer bars
        // on a top-filled bar.
        let v = json!({"value": -20.0, "min": -80.0, "max": 0.0, "type": "db"});
        let (bars, dir) = volume_bar_from_value(&v).expect("object parses");
        assert!(matches!(dir, glyphs::VolumeDirection::TopDown));
        // (-20 - -80) / (0 - -80) = 60/80 = 0.75 → 7 bars
        assert_eq!(bars, 7);
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

    #[test]
    fn allowlist_normalizes_case_and_whitespace() {
        let addrs = vec![
            "AA:BB:CC:DD:EE:FF".into(),
            "  aa:bb:cc:dd:ee:ff  ".into(), // duplicate after trim+upper
            "11:22:33:44:55:66".into(),
            "".into(), // dropped
        ];
        let set = build_allowlist(&addrs);
        assert_eq!(set.len(), 2);
        assert!(set.contains("AA:BB:CC:DD:EE:FF"));
        assert!(set.contains("11:22:33:44:55:66"));
    }

    #[test]
    fn supervisor_decision_admits_allowlisted() {
        let allowlist = build_allowlist(&["AA:BB:CC:DD:EE:FF".into()]);
        let tracked = std::collections::HashSet::new();
        let decision = supervisor_decision("aa:bb:cc:dd:ee:ff", &allowlist, &tracked);
        assert_eq!(
            decision,
            SupervisorDecision::Admit("AA:BB:CC:DD:EE:FF".into()),
        );
    }

    #[test]
    fn supervisor_decision_ignores_unknown() {
        let allowlist = build_allowlist(&["AA:BB:CC:DD:EE:FF".into()]);
        let tracked = std::collections::HashSet::new();
        let decision = supervisor_decision("99:99:99:99:99:99", &allowlist, &tracked);
        assert_eq!(decision, SupervisorDecision::Ignore);
    }

    #[test]
    fn supervisor_decision_skips_already_tracked() {
        let allowlist = build_allowlist(&["AA:BB:CC:DD:EE:FF".into()]);
        let mut tracked = std::collections::HashSet::new();
        tracked.insert("AA:BB:CC:DD:EE:FF".into());
        let decision = supervisor_decision("aa:bb:cc:dd:ee:ff", &allowlist, &tracked);
        assert_eq!(decision, SupervisorDecision::AlreadyTracked);
    }

    #[test]
    fn supervisor_decision_empty_allowlist_admits_nothing() {
        let allowlist = std::collections::HashSet::new();
        let tracked = std::collections::HashSet::new();
        let decision = supervisor_decision("AA:BB:CC:DD:EE:FF", &allowlist, &tracked);
        assert_eq!(
            decision,
            SupervisorDecision::Ignore,
            "empty allowlist must not admit — WS-only mode is the correct fallback",
        );
    }

    #[test]
    fn supervisor_decision_handles_multiple_nuimos_independently() {
        // Simulate the supervisor accepting two different Nuimos in sequence,
        // then rejecting a re-discovery of the first.
        let allowlist = build_allowlist(&["AA:BB:CC:DD:EE:FF".into(), "11:22:33:44:55:66".into()]);
        let mut tracked = std::collections::HashSet::new();

        // Nuimo A arrives.
        match supervisor_decision("AA:BB:CC:DD:EE:FF", &allowlist, &tracked) {
            SupervisorDecision::Admit(key) => tracked.insert(key),
            other => panic!("expected Admit, got {:?}", other),
        };
        // Nuimo B arrives next.
        match supervisor_decision("11:22:33:44:55:66", &allowlist, &tracked) {
            SupervisorDecision::Admit(key) => tracked.insert(key),
            other => panic!("expected Admit, got {:?}", other),
        };
        // Nuimo A re-discovered (BlueZ cache sweep).
        assert_eq!(
            supervisor_decision("aa:bb:cc:dd:ee:ff", &allowlist, &tracked),
            SupervisorDecision::AlreadyTracked,
        );
        // Unknown Nuimo.
        assert_eq!(
            supervisor_decision("99:99:99:99:99:99", &allowlist, &tracked),
            SupervisorDecision::Ignore,
        );
        assert_eq!(tracked.len(), 2);
    }
}
