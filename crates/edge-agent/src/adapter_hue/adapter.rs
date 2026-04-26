//! `HueAdapter`: the `ServiceAdapter` implementation wired around
//! `HueClient` + `LightCache` + the SSE event stream.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use edge_core::{InputPrimitive, Intent, ServiceAdapter, StateUpdate};
use tokio::sync::broadcast;

use super::api::{HueClient, TapDial, TapDialResource};
use super::cache::LightCache;
use super::events::{self, SERVICE_TYPE};
use super::types::{DimmingAction, DimmingDelta, LightUpdate, OnState};

#[derive(Debug, Clone)]
pub struct HueConfig {
    pub host: String,
    pub app_key: String,
}

/// One Tap Dial event surfaced to consumers outside `adapter_hue`.
/// `device_id` matches the Hue device UUID (stable across sessions).
/// For `Battery` events, the value is the new percent reading; the SSE
/// loop emits one `Battery` per push.
#[derive(Debug, Clone)]
pub enum TapDialEvent {
    Input {
        device_id: String,
        primitive: InputPrimitive,
    },
    Battery {
        device_id: String,
        level: u8,
    },
}

pub struct HueAdapter {
    client: HueClient,
    cache: Arc<LightCache>,
    state_tx: broadcast::Sender<StateUpdate>,
    tap_dials: Vec<TapDial>,
    tap_dial_tx: broadcast::Sender<TapDialEvent>,
}

impl HueAdapter {
    pub async fn start(config: HueConfig) -> anyhow::Result<Self> {
        let client = HueClient::new(&config.host, &config.app_key)?;
        let cache = Arc::new(LightCache::new());
        let (state_tx, _) = broadcast::channel(256);
        let (tap_dial_tx, _) = broadcast::channel::<TapDialEvent>(256);

        let initial = client.list_lights().await?;
        tracing::info!(
            bridge = %config.host,
            lights = initial.len(),
            "hue adapter primed"
        );
        cache.replace_all(initial.clone()).await;
        for light in &initial {
            events::broadcast_light(light, &state_tx);
        }

        // Tap Dial enumeration: tolerated to fail (legacy bridges or
        // bridges with no Tap Dial paired). On error, log and continue
        // with an empty list — the rest of the adapter still works.
        let (tap_dials, owner_index) = match client.list_tap_dials().await {
            Ok((tds, idx)) => {
                tracing::info!(count = tds.len(), "hue tap dials discovered");
                (tds, idx)
            }
            Err(e) => {
                tracing::warn!(error = %e, "hue list_tap_dials failed; continuing without");
                (Vec::new(), HashMap::new())
            }
        };
        let owner_index = Arc::new(owner_index);

        tokio::spawn(events::run(
            client.clone(),
            cache.clone(),
            state_tx.clone(),
            owner_index,
            tap_dial_tx.clone(),
        ));

        Ok(Self {
            client,
            cache,
            state_tx,
            tap_dials,
            tap_dial_tx,
        })
    }

    /// Tap Dials enumerated at startup. Iterate to spawn per-device
    /// state pumps with the initial `connected/nickname/battery` values.
    pub fn tap_dials(&self) -> &[TapDial] {
        &self.tap_dials
    }

    /// Subscribe to `TapDialEvent`s. Each receiver gets every event;
    /// fan-out to a per-device task happens on the consumer side
    /// (typically by filtering on `device_id`).
    pub fn subscribe_tap_dial_events(&self) -> broadcast::Receiver<TapDialEvent> {
        self.tap_dial_tx.subscribe()
    }
}

#[async_trait]
impl ServiceAdapter for HueAdapter {
    fn service_type(&self) -> &'static str {
        SERVICE_TYPE
    }

    async fn send_intent(&self, target: &str, intent: &Intent) -> anyhow::Result<()> {
        let update = match intent {
            Intent::PowerOn => LightUpdate {
                on: Some(OnState { on: true }),
                ..Default::default()
            },
            Intent::PowerOff => LightUpdate {
                on: Some(OnState { on: false }),
                ..Default::default()
            },
            Intent::PowerToggle => {
                let current = self
                    .cache
                    .get(target)
                    .await
                    .map(|l| l.on.on)
                    .unwrap_or(false);
                LightUpdate {
                    on: Some(OnState { on: !current }),
                    ..Default::default()
                }
            }
            Intent::BrightnessChange { delta } => {
                let magnitude = delta.abs();
                if magnitude < 0.01 {
                    return Ok(());
                }
                LightUpdate {
                    dimming_delta: Some(DimmingDelta {
                        action: if *delta >= 0.0 {
                            DimmingAction::Up
                        } else {
                            DimmingAction::Down
                        },
                        brightness_delta: magnitude.min(100.0),
                    }),
                    ..Default::default()
                }
            }
            Intent::BrightnessSet { value } => LightUpdate {
                dimming: Some(super::types::Dimming {
                    brightness: value.clamp(0.0, 100.0),
                }),
                ..Default::default()
            },
            other => {
                tracing::debug!(?other, "intent not applicable to hue adapter; skipping");
                return Ok(());
            }
        };

        self.client.put_light(target, &update).await
    }

    fn subscribe_state(&self) -> broadcast::Receiver<StateUpdate> {
        self.state_tx.subscribe()
    }
}
