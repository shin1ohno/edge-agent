//! `HueAdapter`: the `ServiceAdapter` implementation wired around
//! `HueClient` + `LightCache` + the SSE event stream.

use std::sync::Arc;

use async_trait::async_trait;
use edge_core::{Intent, ServiceAdapter, StateUpdate};
use tokio::sync::broadcast;

use super::api::HueClient;
use super::cache::LightCache;
use super::events::{self, SERVICE_TYPE};
use super::types::{DimmingAction, DimmingDelta, LightUpdate, OnState};

#[derive(Debug, Clone)]
pub struct HueConfig {
    pub host: String,
    pub app_key: String,
}

pub struct HueAdapter {
    client: HueClient,
    cache: Arc<LightCache>,
    state_tx: broadcast::Sender<StateUpdate>,
}

impl HueAdapter {
    pub async fn start(config: HueConfig) -> anyhow::Result<Self> {
        let client = HueClient::new(&config.host, &config.app_key)?;
        let cache = Arc::new(LightCache::new());
        let (state_tx, _) = broadcast::channel(256);

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

        tokio::spawn(events::run(client.clone(), cache.clone(), state_tx.clone()));

        Ok(Self {
            client,
            cache,
            state_tx,
        })
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
