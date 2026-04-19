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

    let config_path = first
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("configs/example.toml"));
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
            publisher: cfg.roon.publisher.clone().unwrap_or_else(|| "shin1ohno".into()),
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

    #[cfg(feature = "hue")]
    let hue_adapter: Option<Arc<dyn ServiceAdapter>> = match cfg.hue.token_path.as_ref() {
        Some(path) => match hue_token::load(path) {
            Ok(creds) => {
                let adapter = HueAdapter::start(HueConfig {
                    host: creds.host,
                    app_key: creds.app_key,
                })
                .await?;
                Some(Arc::new(adapter))
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    path = %path.display(),
                    "hue token load failed — hue adapter disabled (run `edge-agent pair-hue` to create one)",
                );
                None
            }
        },
        None => {
            tracing::info!("no [hue] token_path configured — hue adapter disabled");
            None
        }
    };

    // nuimo.skip lets an edge run as a WS-only witness (dashboard / hub
    // validation / multi-edge routing tests without requiring physical BLE
    // hardware on every host).
    if cfg.nuimo.skip {
        tracing::info!("nuimo.skip=true — running WS-only mode (no BLE)");

        #[cfg(feature = "roon")]
        spawn_state_pump(roon_adapter.subscribe_state(), ws_outbox.clone());

        #[cfg(feature = "hue")]
        if let Some(adapter) = &hue_adapter {
            spawn_state_pump(adapter.subscribe_state(), ws_outbox.clone());
        }

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
    #[cfg(feature = "roon")]
    {
        let mut state_rx = roon_adapter.subscribe_state();
        let outbox = ws_outbox.clone();
        let dev = device.clone();
        let glyphs_for_feedback = glyphs.clone();
        tokio::spawn(async move {
            let mut filter = FeedbackFilter::new();
            loop {
                match state_rx.recv().await {
                    Ok(update) => {
                        let frame = EdgeToServer::State {
                            service_type: update.service_type.clone(),
                            target: update.target.clone(),
                            property: update.property.clone(),
                            output_id: update.output_id.clone(),
                            value: update.value.clone(),
                        };
                        let _ = outbox.send(frame).await;

                        if let Some(plan) = FeedbackPlan::from(&update) {
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

    #[cfg(feature = "hue")]
    if let Some(adapter) = &hue_adapter {
        // Hue state flows straight to the WS outbox — no glyph feedback for
        // now, the lights themselves carry the visible confirmation.
        spawn_state_pump(adapter.subscribe_state(), ws_outbox.clone());
    }

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
        hue_adapter.clone(),
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
                        reconnect_nuimo(&device).await;
                    }
                    Ok(event) => {
                        let Some(primitive) = translate_nuimo_event(&event) else { continue; };
                        let routed = engine.route(device_type, &device_id, &primitive).await;
                        for r in routed {
                            if let Err(e) = intent_tx.try_send(r) {
                                tracing::warn!(error = %e, "intent channel full; dropping event");
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

/// Fan incoming `RoutedIntent`s out to per-target workers, spawning a new
/// worker the first time we see a given `(service_type, target)` pair.
async fn run_dispatcher(
    mut rx: tokio::sync::mpsc::Receiver<RoutedIntent>,
    #[cfg(feature = "roon")] roon: Arc<dyn ServiceAdapter>,
    #[cfg(feature = "hue")] hue: Option<Arc<dyn ServiceAdapter>>,
) {
    let mut workers: HashMap<(String, String), tokio::sync::mpsc::Sender<Intent>> = HashMap::new();

    while let Some(r) = rx.recv().await {
        let key = (r.service_type.clone(), r.service_target.clone());

        if !workers.contains_key(&key) {
            let adapter: Option<Arc<dyn ServiceAdapter>> = match key.0.as_str() {
                #[cfg(feature = "roon")]
                "roon" => Some(roon.clone()),
                #[cfg(feature = "hue")]
                "hue" => hue.clone(),
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
) {
    tokio::spawn(async move {
        loop {
            match state_rx.recv().await {
                Ok(update) => {
                    let frame = EdgeToServer::State {
                        service_type: update.service_type,
                        target: update.target,
                        property: update.property,
                        output_id: update.output_id,
                        value: update.value,
                    };
                    let _ = outbox.send(frame).await;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(skipped = n, "adapter state lag");
                }
            }
        }
    });
}

fn translate_nuimo_event(event: &NuimoEvent) -> Option<InputPrimitive> {
    use edge_core::{Direction, TouchArea};
    Some(match event {
        NuimoEvent::ButtonDown => InputPrimitive::Press,
        NuimoEvent::ButtonUp => InputPrimitive::Release,
        NuimoEvent::Rotate { delta, .. } => InputPrimitive::Rotate { delta: *delta },
        NuimoEvent::SwipeUp => InputPrimitive::Swipe { direction: Direction::Up },
        NuimoEvent::SwipeDown => InputPrimitive::Swipe { direction: Direction::Down },
        NuimoEvent::SwipeLeft | NuimoEvent::FlyLeft => {
            InputPrimitive::Swipe { direction: Direction::Left }
        }
        NuimoEvent::SwipeRight | NuimoEvent::FlyRight => {
            InputPrimitive::Swipe { direction: Direction::Right }
        }
        NuimoEvent::TouchTop => InputPrimitive::Touch { area: TouchArea::Top },
        NuimoEvent::TouchBottom => InputPrimitive::Touch { area: TouchArea::Bottom },
        NuimoEvent::TouchLeft => InputPrimitive::Touch { area: TouchArea::Left },
        NuimoEvent::TouchRight => InputPrimitive::Touch { area: TouchArea::Right },
        NuimoEvent::LongTouchTop => InputPrimitive::LongTouch { area: TouchArea::Top },
        NuimoEvent::LongTouchBottom => InputPrimitive::LongTouch { area: TouchArea::Bottom },
        NuimoEvent::LongTouchLeft => InputPrimitive::LongTouch { area: TouchArea::Left },
        NuimoEvent::LongTouchRight => InputPrimitive::LongTouch { area: TouchArea::Right },
        NuimoEvent::Hover { proximity } => InputPrimitive::Hover { proximity: *proximity },
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
    /// Volume bar, 0..=9 LEDs from the bottom.
    VolumeBar(u8),
    /// Named glyph from the registry (play / pause / ...).
    NamedGlyph(&'static str),
}

#[cfg(feature = "roon")]
impl FeedbackPlan {
    /// Project a StateUpdate into the visible frame it should produce, or
    /// `None` if nothing on the device should change.
    fn from(update: &StateUpdate) -> Option<Self> {
        match (update.property.as_str(), &update.value) {
            ("playback", serde_json::Value::String(s)) => match s.as_str() {
                "playing" => Some(Self::NamedGlyph("play")),
                "paused" | "stopped" => Some(Self::NamedGlyph("pause")),
                _ => None,
            },
            ("volume", serde_json::Value::Object(obj)) => {
                let value = obj.get("value").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let max = obj
                    .get("max")
                    .and_then(|v| v.as_f64())
                    .filter(|v| *v > 0.0)
                    .unwrap_or(100.0);
                let pct = ((value / max) * 100.0).clamp(0.0, 100.0);
                let bars = ((pct / 100.0) * 9.0).round() as u8;
                Some(Self::VolumeBar(bars))
            }
            _ => None,
        }
    }

    /// Stable identifier for "what's on the LED right now". Filter dedups
    /// on this.
    fn signature(&self) -> String {
        match self {
            Self::VolumeBar(bars) => format!("vol:{bars}"),
            Self::NamedGlyph(name) => (*name).to_string(),
        }
    }

    async fn execute(&self, device: &NuimoDevice, registry: &GlyphRegistry) {
        let (glyph, transition, timeout_ms) = match self {
            Self::VolumeBar(bars) => (
                glyphs::volume_bars(*bars),
                DisplayTransition::Immediate,
                3000,
            ),
            Self::NamedGlyph(name) => {
                let Some(entry) = registry.get(name).await else {
                    tracing::debug!(name, "glyph missing from registry; skipping feedback");
                    return;
                };
                if entry.builtin {
                    tracing::debug!(name, "glyph is builtin; expected parametric render");
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

        self.last_sig.insert(update.target.clone(), signature.to_string());
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
