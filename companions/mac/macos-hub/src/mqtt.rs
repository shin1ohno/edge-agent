use rumqttc::{AsyncClient, EventLoop, MqttOptions, QoS};
use serde::Serialize;
use tokio::sync::mpsc;

use crate::config::MqttConfig;
use crate::core_audio::OutputDevice;

/// MQTT bridge that publishes macOS audio state and receives commands.
///
/// Topic structure (weave SPEC):
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

        Ok((client, self.command_rx))
    }
}

#[derive(Debug, Serialize)]
pub struct VolumePayload {
    /// 0-100 integer percent. We publish percent (not 0.0-1.0 float) to match
    /// the `volume` command payload shape described in the README.
    pub value: u8,
}

#[derive(Debug, Serialize)]
pub struct OutputDevicePayload<'a> {
    pub device_uid: &'a str,
    pub name: &'a str,
    pub is_airplay: bool,
    pub transport_type: u32,
}

#[derive(Debug, Serialize)]
pub struct AvailableOutputEntry<'a> {
    pub device_uid: &'a str,
    pub name: &'a str,
    pub is_airplay: bool,
    pub transport_type: u32,
}

/// Publish current system volume (0.0-1.0 source) as percent.
pub async fn publish_volume(
    client: &AsyncClient,
    edge_id: &str,
    volume_0_1: f32,
) -> anyhow::Result<()> {
    let pct = (volume_0_1.clamp(0.0, 1.0) * 100.0).round() as u8;
    let topic = format!("service/macos/{}/state/volume", edge_id);
    let payload = serde_json::to_string(&VolumePayload { value: pct })?;
    client
        .publish(&topic, QoS::AtLeastOnce, true, payload)
        .await?;
    Ok(())
}

/// Publish the currently-selected default output device.
pub async fn publish_output_device(
    client: &AsyncClient,
    edge_id: &str,
    device: &OutputDevice,
) -> anyhow::Result<()> {
    let topic = format!("service/macos/{}/state/output_device", edge_id);
    let payload = serde_json::to_string(&OutputDevicePayload {
        device_uid: &device.uid,
        name: &device.name,
        is_airplay: device.is_airplay,
        transport_type: device.transport_type,
    })?;
    client
        .publish(&topic, QoS::AtLeastOnce, true, payload)
        .await?;
    Ok(())
}

/// Publish `null` retained marker when default output cannot be resolved.
pub async fn publish_output_device_unknown(
    client: &AsyncClient,
    edge_id: &str,
) -> anyhow::Result<()> {
    let topic = format!("service/macos/{}/state/output_device", edge_id);
    client
        .publish(&topic, QoS::AtLeastOnce, true, "null")
        .await?;
    Ok(())
}

/// Publish the list of enumerable output devices.
pub async fn publish_available_outputs(
    client: &AsyncClient,
    edge_id: &str,
    devices: &[OutputDevice],
) -> anyhow::Result<()> {
    let topic = format!("service/macos/{}/state/available_outputs", edge_id);
    let entries: Vec<AvailableOutputEntry> = devices
        .iter()
        .map(|d| AvailableOutputEntry {
            device_uid: &d.uid,
            name: &d.name,
            is_airplay: d.is_airplay,
            transport_type: d.transport_type,
        })
        .collect();
    let payload = serde_json::to_string(&entries)?;
    client
        .publish(&topic, QoS::AtLeastOnce, true, payload)
        .await?;
    Ok(())
}

/// Publish playback_active state. For MVP this is always published as `null`
/// (unknown) because accurate detection requires audio-level sampling which
/// is out of scope. Kept as a helper so a later implementation can plug in.
pub async fn publish_playback_active(
    client: &AsyncClient,
    edge_id: &str,
    active: Option<bool>,
) -> anyhow::Result<()> {
    let topic = format!("service/macos/{}/state/playback_active", edge_id);
    let payload = match active {
        Some(true) => "true".to_string(),
        Some(false) => "false".to_string(),
        None => "null".to_string(),
    };
    client
        .publish(&topic, QoS::AtLeastOnce, true, payload)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    /// Parse `service/macos/{target}/command/{intent}` into (target, intent).
    /// Shared helper shape used by the command handler. Duplicated inline so
    /// the test does not depend on the handler module.
    fn parse_topic(topic: &str) -> Option<(&str, &str)> {
        let parts: Vec<&str> = topic.split('/').collect();
        if parts.len() != 5 {
            return None;
        }
        if parts[0] != "service" || parts[1] != "macos" || parts[3] != "command" {
            return None;
        }
        Some((parts[2], parts[4]))
    }

    #[test]
    fn parses_valid_topic() {
        assert_eq!(
            parse_topic("service/macos/air/command/play_pause"),
            Some(("air", "play_pause"))
        );
        assert_eq!(
            parse_topic("service/macos/mac-studio/command/volume"),
            Some(("mac-studio", "volume"))
        );
    }

    #[test]
    fn rejects_wrong_prefix() {
        assert_eq!(parse_topic("service/roon/air/command/play"), None);
        assert_eq!(parse_topic("other/macos/air/command/play"), None);
    }

    #[test]
    fn rejects_wrong_shape() {
        assert_eq!(parse_topic("service/macos/air/state/volume"), None);
        assert_eq!(parse_topic("service/macos/air/command"), None);
        assert_eq!(
            parse_topic("service/macos/air/command/play/extra"),
            None
        );
    }
}
