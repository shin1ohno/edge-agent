use anyhow::{anyhow, Result};
use rumqttc::AsyncClient;
use serde::Deserialize;

use crate::{core_audio, media_keys, mqtt};

/// Parse `service/macos/{target}/command/{intent}` into (target, intent).
pub fn parse_topic(topic: &str) -> Option<(&str, &str)> {
    let parts: Vec<&str> = topic.split('/').collect();
    if parts.len() != 5 {
        return None;
    }
    if parts[0] != "service" || parts[1] != "macos" || parts[3] != "command" {
        return None;
    }
    Some((parts[2], parts[4]))
}

/// Whether this host should handle a command addressed to `target`.
/// Accepts exact match with `edge_id` or the wildcard `all`.
fn is_for_us(target: &str, edge_id: &str) -> bool {
    target == edge_id || target == "all"
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VolumeHow {
    Absolute,
    Relative,
    Step,
}

#[derive(Debug, Deserialize)]
pub struct VolumePayload {
    pub how: VolumeHow,
    pub value: f32,
}

#[derive(Debug, Deserialize)]
pub struct SetOutputPayload {
    pub device_uid: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MuteAction {
    Mute,
    Unmute,
    Toggle,
}

#[derive(Debug, Deserialize)]
pub struct MutePayload {
    pub action: MuteAction,
}

/// Compute the new volume (0-100 scale) from a `volume` command.
/// Public for unit testing.
pub fn compute_new_volume_pct(current_pct: f32, payload: &VolumePayload) -> f32 {
    let v = match payload.how {
        VolumeHow::Absolute => payload.value,
        VolumeHow::Relative => current_pct + payload.value,
        VolumeHow::Step => {
            // `value` sign determines direction; magnitude is ignored in the
            // step variant. Step size is fixed at 5.
            if payload.value >= 0.0 {
                current_pct + 5.0
            } else {
                current_pct - 5.0
            }
        }
    };
    v.clamp(0.0, 100.0)
}

pub async fn handle(
    client: &AsyncClient,
    edge_id: &str,
    topic: &str,
    payload: &str,
) -> Result<()> {
    let Some((target, intent)) = parse_topic(topic) else {
        return Ok(());
    };
    if !is_for_us(target, edge_id) {
        return Ok(());
    }

    tracing::debug!("command received: target={} intent={}", target, intent);

    match intent {
        "play_pause" | "playpause" => {
            media_keys::play_pause()?;
        }
        "play" => {
            // No dedicated "play" media key — play/pause toggles.
            media_keys::play_pause()?;
        }
        "pause" => {
            media_keys::play_pause()?;
        }
        "next" => {
            media_keys::next_track()?;
        }
        "previous" => {
            media_keys::previous_track()?;
        }
        "stop" => {
            media_keys::stop()?;
        }
        "volume" => {
            handle_volume(client, edge_id, payload).await?;
        }
        "set_output" => {
            handle_set_output(client, edge_id, payload).await?;
        }
        "mute" => {
            handle_mute(client, edge_id, payload).await?;
        }
        other => {
            tracing::warn!("unknown intent: {}", other);
        }
    }
    Ok(())
}

async fn handle_volume(client: &AsyncClient, edge_id: &str, payload: &str) -> Result<()> {
    let parsed: VolumePayload = serde_json::from_str(payload)
        .map_err(|e| anyhow!("volume payload parse error: {}", e))?;

    let current_0_1 = core_audio::get_system_volume()?;
    let current_pct = current_0_1 * 100.0;
    let new_pct = compute_new_volume_pct(current_pct, &parsed);
    core_audio::set_system_volume(new_pct / 100.0)?;

    mqtt::publish_volume(client, edge_id, new_pct / 100.0).await?;
    Ok(())
}

async fn handle_set_output(client: &AsyncClient, edge_id: &str, payload: &str) -> Result<()> {
    let parsed: SetOutputPayload = serde_json::from_str(payload)
        .map_err(|e| anyhow!("set_output payload parse error: {}", e))?;

    let device = core_audio::find_output_by_uid(&parsed.device_uid)?;
    core_audio::set_default_output(device.id)?;

    mqtt::publish_output_device(client, edge_id, &device).await?;
    // Volume may change when the active device changes; republish.
    if let Ok(v) = core_audio::get_system_volume() {
        let _ = mqtt::publish_volume(client, edge_id, v).await;
    }
    Ok(())
}

async fn handle_mute(client: &AsyncClient, edge_id: &str, payload: &str) -> Result<()> {
    // MVP: mute = set volume 0, unmute = restore to 50%, toggle = whichever
    // is opposite of current. Proper mute uses the device's `Mute` property
    // (kAudioDevicePropertyMute) but that requires scope=Output + element per
    // channel, which the Swift spike does not cover. See TODO in README.
    let parsed: MutePayload = serde_json::from_str(payload)
        .map_err(|e| anyhow!("mute payload parse error: {}", e))?;

    let current = core_audio::get_system_volume()?;
    let new_volume = match parsed.action {
        MuteAction::Mute => 0.0,
        MuteAction::Unmute => {
            if current == 0.0 {
                0.5
            } else {
                current
            }
        }
        MuteAction::Toggle => {
            if current == 0.0 {
                0.5
            } else {
                0.0
            }
        }
    };
    core_audio::set_system_volume(new_volume)?;
    mqtt::publish_volume(client, edge_id, new_volume).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_topic() {
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
    fn rejects_malformed_topic() {
        assert!(parse_topic("service/macos/air/state/volume").is_none());
        assert!(parse_topic("service/macos/air/command").is_none());
        assert!(parse_topic("service/macos/air/command/play/extra").is_none());
        assert!(parse_topic("wrong/macos/air/command/play").is_none());
    }

    #[test]
    fn target_filter() {
        assert!(is_for_us("air", "air"));
        assert!(is_for_us("all", "air"));
        assert!(!is_for_us("studio", "air"));
    }

    #[test]
    fn volume_absolute() {
        let p: VolumePayload = serde_json::from_str(r#"{"how":"absolute","value":50}"#).unwrap();
        assert_eq!(compute_new_volume_pct(80.0, &p), 50.0);
    }

    #[test]
    fn volume_relative_up() {
        let p: VolumePayload = serde_json::from_str(r#"{"how":"relative","value":10}"#).unwrap();
        assert_eq!(compute_new_volume_pct(30.0, &p), 40.0);
    }

    #[test]
    fn volume_relative_down() {
        let p: VolumePayload = serde_json::from_str(r#"{"how":"relative","value":-20}"#).unwrap();
        assert_eq!(compute_new_volume_pct(30.0, &p), 10.0);
    }

    #[test]
    fn volume_step_up() {
        let p: VolumePayload = serde_json::from_str(r#"{"how":"step","value":1}"#).unwrap();
        assert_eq!(compute_new_volume_pct(50.0, &p), 55.0);
    }

    #[test]
    fn volume_step_down() {
        let p: VolumePayload = serde_json::from_str(r#"{"how":"step","value":-1}"#).unwrap();
        assert_eq!(compute_new_volume_pct(50.0, &p), 45.0);
    }

    #[test]
    fn volume_clamps_high() {
        let p: VolumePayload = serde_json::from_str(r#"{"how":"relative","value":200}"#).unwrap();
        assert_eq!(compute_new_volume_pct(50.0, &p), 100.0);
    }

    #[test]
    fn volume_clamps_low() {
        let p: VolumePayload = serde_json::from_str(r#"{"how":"relative","value":-200}"#).unwrap();
        assert_eq!(compute_new_volume_pct(50.0, &p), 0.0);
    }
}
