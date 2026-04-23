//! `MacosAdapter`: `ServiceAdapter` implementation that speaks MQTT to a
//! sibling `macos-hub` binary.

use async_trait::async_trait;
use edge_core::{Intent, ServiceAdapter, StateUpdate};
use rumqttc::{AsyncClient, QoS};
use tokio::sync::broadcast;

use super::mqtt::{self};
use super::types;

/// Stable identifier used by the routing engine to match
/// `Mapping.service_type == "macos"`.
pub const SERVICE_TYPE: &str = "macos";

/// Runtime configuration for [`MacosAdapter::start`]. Mirrors the
/// `MacosSection` in `config.rs`, minus the serde plumbing.
#[derive(Debug, Clone)]
pub struct MacosConfig {
    pub mqtt_host: String,
    pub mqtt_port: u16,
    pub mqtt_client_id: String,
}

impl Default for MacosConfig {
    fn default() -> Self {
        Self {
            mqtt_host: "localhost".to_string(),
            mqtt_port: 1883,
            mqtt_client_id: "edge-agent-macos".to_string(),
        }
    }
}

pub struct MacosAdapter {
    mqtt_client: AsyncClient,
    state_tx: broadcast::Sender<StateUpdate>,
}

impl MacosAdapter {
    /// Connect to the broker and start the event-loop task. Returns as
    /// soon as the client handle is ready — the first actual `ConnAck`
    /// may not have arrived yet, so callers should treat the adapter as
    /// eventually-consistent (first state update from `macos-hub` is the
    /// earliest proof of a working session).
    pub async fn start(config: MacosConfig) -> anyhow::Result<Self> {
        let (state_tx, _) = broadcast::channel::<StateUpdate>(256);
        let conn = mqtt::connect(&config.mqtt_host, config.mqtt_port, &config.mqtt_client_id);
        let client = conn.client.clone();

        tracing::info!(
            host = %config.mqtt_host,
            port = config.mqtt_port,
            client_id = %config.mqtt_client_id,
            "macos adapter connecting to MQTT broker",
        );

        tokio::spawn(mqtt::run_event_loop(
            conn.client,
            conn.event_loop,
            state_tx.clone(),
        ));

        Ok(Self {
            mqtt_client: client,
            state_tx,
        })
    }
}

#[async_trait]
impl ServiceAdapter for MacosAdapter {
    fn service_type(&self) -> &'static str {
        SERVICE_TYPE
    }

    async fn send_intent(&self, target: &str, intent: &Intent) -> anyhow::Result<()> {
        match types::intent_to_mqtt(intent, target) {
            Some((topic, payload)) => {
                self.mqtt_client
                    .publish(&topic, QoS::AtLeastOnce, false, payload)
                    .await?;
                Ok(())
            }
            None => {
                tracing::debug!(?intent, "intent not applicable to macos adapter; skipping");
                Ok(())
            }
        }
    }

    fn subscribe_state(&self) -> broadcast::Receiver<StateUpdate> {
        self.state_tx.subscribe()
    }
}
