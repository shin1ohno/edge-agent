use rumqttc::{AsyncClient, EventLoop, MqttOptions, QoS};
use tokio::sync::mpsc;

use crate::config::MqttConfig;

#[cfg(target_os = "macos")]
use crate::core_audio::OutputDevice;

#[cfg(not(target_os = "macos"))]
use crate::core_audio_stub::OutputDevice;

/// MQTT bridge that publishes macOS audio state and receives commands.
///
/// Topic structure:
///   Publish:   service/macos/{edge_id}/state/{property}
///   Subscribe: service/macos/+/command/+
pub struct MqttBridge {
    client: AsyncClient,
    event_loop: EventLoop,
    command_rx: mpsc::Receiver<(String, String)>,
    command_tx: mpsc::Sender<(String, String)>,
}

impl MqttBridge {
    pub fn new(config: &MqttConfig) -> Self {
        let mut opts = MqttOptions::new(&config.client_id, &config.host, config.port);
        opts.set_keep_alive(std::time::Duration::from_secs(30));

        let (client, event_loop) = AsyncClient::new(opts, 64);
        let (command_tx, command_rx) = mpsc::channel(64);

        MqttBridge {
            client,
            event_loop,
            command_rx,
            command_tx,
        }
    }

    pub async fn start(
        mut self,
    ) -> anyhow::Result<(AsyncClient, mpsc::Receiver<(String, String)>)> {
        self.client
            .subscribe("service/macos/+/command/+", QoS::AtLeastOnce)
            .await?;

        let command_tx = self.command_tx.clone();
        let client = self.client.clone();

        tokio::spawn(async move {
            loop {
                match self.event_loop.poll().await {
                    Ok(rumqttc::Event::Incoming(rumqttc::Packet::Publish(msg))) => {
                        let topic = msg.topic.clone();
                        if let Ok(payload) = String::from_utf8(msg.payload.to_vec()) {
                            let _ = command_tx.send((topic, payload)).await;
                        }
                    }
                    Ok(_) => {}
                    Err(e) => {
                        tracing::warn!("MQTT error: {}", e);
                        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    }
                }
            }
        });

        let _ = client;
        Ok((self.client, self.command_rx))
    }
}

/// Publish current system volume (0..=100) to
/// `service/macos/{edge_id}/state/volume`.
pub async fn publish_volume(
    client: &AsyncClient,
    edge_id: &str,
    level: u8,
) -> anyhow::Result<()> {
    let topic = format!("service/macos/{}/state/volume", edge_id);
    let payload = serde_json::json!({ "level": level }).to_string();
    client
        .publish(&topic, QoS::AtLeastOnce, true, payload)
        .await?;
    Ok(())
}

/// Publish the currently selected default output device.
pub async fn publish_output_device(
    client: &AsyncClient,
    edge_id: &str,
    device: &OutputDevice,
) -> anyhow::Result<()> {
    let topic = format!("service/macos/{}/state/output_device", edge_id);
    let payload = serde_json::to_string(device)?;
    client
        .publish(&topic, QoS::AtLeastOnce, true, payload)
        .await?;
    Ok(())
}

/// Publish the list of available output devices.
pub async fn publish_available_outputs(
    client: &AsyncClient,
    edge_id: &str,
    outputs: &[OutputDevice],
) -> anyhow::Result<()> {
    let topic = format!("service/macos/{}/state/available_outputs", edge_id);
    let payload = serde_json::to_string(outputs)?;
    client
        .publish(&topic, QoS::AtLeastOnce, true, payload)
        .await?;
    Ok(())
}

/// Publish playback_active state. `None` is serialized as JSON null — MVP
/// cannot reliably detect audio activity across apps.
pub async fn publish_playback_active(
    client: &AsyncClient,
    edge_id: &str,
    active: Option<bool>,
) -> anyhow::Result<()> {
    let topic = format!("service/macos/{}/state/playback_active", edge_id);
    let payload = match active {
        Some(v) => serde_json::json!({ "active": v }).to_string(),
        None => serde_json::json!({ "active": serde_json::Value::Null }).to_string(),
    };
    client
        .publish(&topic, QoS::AtLeastOnce, true, payload)
        .await?;
    Ok(())
}

/// Extract `(target, intent)` from a topic of shape
/// `service/macos/{target}/command/{intent}`. Returns `None` if the topic
/// does not match.
pub fn parse_command_topic(topic: &str) -> Option<(&str, &str)> {
    let parts: Vec<&str> = topic.split('/').collect();
    if parts.len() != 5 {
        return None;
    }
    if parts[0] != "service" || parts[1] != "macos" || parts[3] != "command" {
        return None;
    }
    Some((parts[2], parts[4]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_command_topic() {
        let (target, intent) =
            parse_command_topic("service/macos/livingroom/command/play_pause").unwrap();
        assert_eq!(target, "livingroom");
        assert_eq!(intent, "play_pause");
    }

    #[test]
    fn parses_volume_intent() {
        let (target, intent) =
            parse_command_topic("service/macos/mac/command/volume").unwrap();
        assert_eq!(target, "mac");
        assert_eq!(intent, "volume");
    }

    #[test]
    fn rejects_wrong_prefix() {
        assert!(parse_command_topic("service/roon/mac/command/volume").is_none());
        assert!(parse_command_topic("foo/macos/mac/command/volume").is_none());
    }

    #[test]
    fn rejects_wrong_arity() {
        assert!(parse_command_topic("service/macos/mac/command").is_none());
        assert!(parse_command_topic("service/macos/mac/command/volume/extra").is_none());
    }

    #[test]
    fn rejects_state_topic() {
        assert!(parse_command_topic("service/macos/mac/state/volume").is_none());
    }
}
