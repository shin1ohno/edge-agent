//! `MacosAdapter`: a `ServiceAdapter` that talks to a `macos-hub` instance
//! via MQTT. Intents get serialized through [`super::types::intent_to_mqtt`]
//! and published; inbound state topics are parsed + broadcast by the
//! event-loop task spawned in `start`.

use async_trait::async_trait;
use edge_core::{Intent, ServiceAdapter, StateUpdate};
use rumqttc::{AsyncClient, QoS};
use tokio::sync::broadcast;

pub const SERVICE_TYPE: &str = "macos";

#[derive(Debug, Clone)]
pub struct MacosConfig {
    pub mqtt_host: String,
    pub mqtt_port: u16,
    pub mqtt_client_id: String,
}

pub struct MacosAdapter {
    mqtt_client: AsyncClient,
    state_tx: broadcast::Sender<StateUpdate>,
}

impl MacosAdapter {
    /// Build the MQTT client, spawn the event-loop pump, and return the
    /// adapter. The pump task owns the connection — `ConnAck` triggers a
    /// re-subscribe so state topics keep flowing after reconnects.
    pub async fn start(config: MacosConfig) -> anyhow::Result<Self> {
        let (client, event_loop) =
            super::mqtt::build_client(&config.mqtt_host, config.mqtt_port, &config.mqtt_client_id);
        let (state_tx, _) = broadcast::channel(256);

        tracing::info!(
            host = %config.mqtt_host,
            port = config.mqtt_port,
            client_id = %config.mqtt_client_id,
            "macos adapter connecting"
        );

        tokio::spawn(super::mqtt::run_event_loop(
            client.clone(),
            event_loop,
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
        let Some((topic, payload)) = super::types::intent_to_mqtt(intent, target) else {
            tracing::debug!(?intent, "intent not applicable to macos adapter; skipping");
            return Ok(());
        };
        self.mqtt_client
            .publish(&topic, QoS::AtLeastOnce, false, payload)
            .await?;
        Ok(())
    }

    fn subscribe_state(&self) -> broadcast::Receiver<StateUpdate> {
        self.state_tx.subscribe()
    }
}
