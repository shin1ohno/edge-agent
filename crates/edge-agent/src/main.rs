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

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

#[cfg(feature = "hue")]
use adapter_hue::{HueAdapter, HueConfig};
#[cfg(feature = "roon")]
use adapter_roon::{RoonAdapter, RoonConfig};
use edge_core::{
    GlyphRegistry, InputPrimitive, RoutingEngine, ServiceAdapter, StateUpdate, WsClient,
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
    #[cfg(feature = "roon")]
    {
        let mut state_rx = roon_adapter.subscribe_state();
        let outbox = ws_outbox.clone();
        let dev = device.clone();
        let glyphs_for_feedback = glyphs.clone();
        tokio::spawn(async move {
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
                        render_feedback(&dev, &update, &glyphs_for_feedback).await;
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

    // Input pump: Nuimo BLE events → routing → adapter dispatch.
    let mut events = device.events();
    let device_type = "nuimo";
    loop {
        tokio::select! {
            Ok(event) = events.recv() => {
                let Some(primitive) = translate_nuimo_event(&event) else { continue; };
                let routed = engine.route(device_type, &device_id, &primitive).await;
                for r in routed {
                    match r.service_type.as_str() {
                        #[cfg(feature = "roon")]
                        "roon" => {
                            if let Err(e) = roon_adapter.send_intent(&r.service_target, &r.intent).await {
                                tracing::warn!(error = %e, target = %r.service_target, "failed to send roon intent");
                            }
                        }
                        #[cfg(feature = "hue")]
                        "hue" => {
                            if let Some(adapter) = &hue_adapter {
                                if let Err(e) = adapter.send_intent(&r.service_target, &r.intent).await {
                                    tracing::warn!(error = %e, target = %r.service_target, "failed to send hue intent");
                                }
                            } else {
                                tracing::debug!(target = %r.service_target, "hue intent dropped — adapter disabled");
                            }
                        }
                        other => {
                            tracing::warn!(service_type = %other, "no adapter registered for service_type");
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

#[cfg(feature = "roon")]
async fn render_feedback(device: &NuimoDevice, update: &StateUpdate, registry: &GlyphRegistry) {
    // Resolve state → glyph name. Keep a minimal playback-state mapping here;
    // richer FeedbackRule-driven dispatch is a future enhancement.
    let (glyph_name, transition) = match (update.property.as_str(), &update.value) {
        ("playback", serde_json::Value::String(s)) => match s.as_str() {
            "playing" => ("play", DisplayTransition::CrossFade),
            "paused" | "stopped" => ("pause", DisplayTransition::CrossFade),
            _ => return,
        },
        ("volume", serde_json::Value::Object(_)) => ("volume_bar", DisplayTransition::Immediate),
        _ => return,
    };

    let glyph = if glyph_name == "volume_bar" {
        let Some(obj) = update.value.as_object() else {
            return;
        };
        let value = obj.get("value").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let max = obj
            .get("max")
            .and_then(|v| v.as_f64())
            .filter(|v| *v > 0.0)
            .unwrap_or(100.0);
        let pct = ((value / max) * 100.0).round().clamp(0.0, 100.0) as u8;
        glyphs::volume(pct)
    } else {
        match registry.get(glyph_name).await {
            Some(entry) if !entry.builtin => nuimo::Glyph::from_str(&entry.pattern),
            _ => {
                tracing::debug!(glyph_name, "glyph missing from registry; skipping feedback");
                return;
            }
        }
    };

    let _ = device
        .display_glyph(
            &glyph,
            &DisplayOptions {
                brightness: 1.0,
                timeout_ms: 1000,
                transition,
            },
        )
        .await;
}

fn default_roon_token_path(edge_id: &str) -> PathBuf {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("state")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("edge-agent")
        .join(format!("roon-token-{}.json", edge_id))
}
