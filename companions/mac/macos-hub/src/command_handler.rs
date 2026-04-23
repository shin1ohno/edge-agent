//! Dispatch MQTT commands to `core_audio` / `media_keys`.
//!
//! Topic shape: `service/macos/{target}/command/{intent}`
//! Intents handled: play_pause, play, pause, next, previous, stop,
//! volume, set_output, mute.

use anyhow::{anyhow, Result};
use serde::Deserialize;

#[cfg(target_os = "macos")]
use crate::{core_audio as audio, media_keys};

#[cfg(not(target_os = "macos"))]
use crate::{core_audio_stub as audio, media_keys_stub as media_keys};

use crate::mqtt::parse_command_topic;

/// State mutation summary returned from command dispatch. Lets main decide
/// which state topics to re-publish.
#[derive(Debug, Default, Clone, Copy)]
pub struct SideEffects {
    pub volume_changed: bool,
    pub output_changed: bool,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "how", rename_all = "snake_case")]
pub enum VolumeCommand {
    Absolute { value: i32 },
    Relative { value: i32 },
    Step { value: i32 },
}

#[derive(Debug, Deserialize)]
pub struct SetOutputCommand {
    pub device_uid: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MuteAction {
    Mute,
    Unmute,
    Toggle,
}

#[derive(Debug, Deserialize)]
pub struct MuteCommand {
    pub action: MuteAction,
}

/// Compute the new absolute volume (0..=100) after applying `cmd` to `current`.
/// `Step` treats `value` as a signed number of 5% increments.
pub fn apply_volume_command(current: u8, cmd: &VolumeCommand) -> u8 {
    let next: i32 = match cmd {
        VolumeCommand::Absolute { value } => *value,
        VolumeCommand::Relative { value } => current as i32 + *value,
        VolumeCommand::Step { value } => current as i32 + *value * 5,
    };
    next.clamp(0, 100) as u8
}

/// Dispatch a single command. Returns which state categories changed so the
/// caller can re-publish. Unknown intents are logged and ignored.
pub async fn handle_command(
    topic: &str,
    payload: &str,
    // Per-session mute memory: volume level to restore on `unmute`.
    // Passed by reference; None means nothing has been muted from 0.
    mute_restore: &mut Option<u8>,
) -> Result<SideEffects> {
    let (target, intent) = parse_command_topic(topic)
        .ok_or_else(|| anyhow!("unparseable topic: {}", topic))?;
    let _ = target; // target is currently unused — we only run one hub per Mac

    let mut fx = SideEffects::default();

    match intent {
        "play_pause" => media_keys::play_pause()?,
        "play" | "pause" => media_keys::play_pause()?, // toggle — real macOS media keys don't distinguish
        "next" => media_keys::next_track()?,
        "previous" => media_keys::previous_track()?,
        "stop" => {
            tracing::info!("stop: no NX key for stop on macOS media keys; ignoring");
        }

        "volume" => {
            let cmd: VolumeCommand = serde_json::from_str(payload)?;
            let current_f = audio::get_system_volume().unwrap_or(0.0);
            let current = (current_f * 100.0).round().clamp(0.0, 100.0) as u8;
            let next = apply_volume_command(current, &cmd);
            audio::set_system_volume(next as f32 / 100.0)?;
            fx.volume_changed = true;
        }

        "set_output" => {
            let cmd: SetOutputCommand = serde_json::from_str(payload)?;
            let device = audio::find_device_by_uid(&cmd.device_uid)?;
            audio::set_default_output(device.id)?;
            fx.output_changed = true;
            fx.volume_changed = true; // volume selector follows default device
        }

        // TODO: real mute requires kAudioDevicePropertyMute on the output
        // device, or the reserved Virtual Main Mute selector. For MVP we
        // approximate: mute → save current volume, set to 0; unmute → restore.
        "mute" => {
            let cmd: MuteCommand = serde_json::from_str(payload)?;
            let current_f = audio::get_system_volume().unwrap_or(0.0);
            let current = (current_f * 100.0).round().clamp(0.0, 100.0) as u8;
            match cmd.action {
                MuteAction::Mute => {
                    if current > 0 {
                        *mute_restore = Some(current);
                    }
                    audio::set_system_volume(0.0)?;
                }
                MuteAction::Unmute => {
                    let restore = mute_restore.take().unwrap_or(30);
                    audio::set_system_volume(restore as f32 / 100.0)?;
                }
                MuteAction::Toggle => {
                    if current == 0 {
                        let restore = mute_restore.take().unwrap_or(30);
                        audio::set_system_volume(restore as f32 / 100.0)?;
                    } else {
                        *mute_restore = Some(current);
                        audio::set_system_volume(0.0)?;
                    }
                }
            }
            fx.volume_changed = true;
        }

        other => {
            tracing::warn!("ignoring unknown intent: {}", other);
        }
    }

    Ok(fx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absolute_volume() {
        let cmd: VolumeCommand =
            serde_json::from_str(r#"{"how":"absolute","value":42}"#).unwrap();
        assert_eq!(apply_volume_command(10, &cmd), 42);
    }

    #[test]
    fn absolute_clamps_high() {
        let cmd: VolumeCommand =
            serde_json::from_str(r#"{"how":"absolute","value":150}"#).unwrap();
        assert_eq!(apply_volume_command(10, &cmd), 100);
    }

    #[test]
    fn absolute_clamps_low() {
        let cmd: VolumeCommand =
            serde_json::from_str(r#"{"how":"absolute","value":-10}"#).unwrap();
        assert_eq!(apply_volume_command(50, &cmd), 0);
    }

    #[test]
    fn relative_positive() {
        let cmd: VolumeCommand =
            serde_json::from_str(r#"{"how":"relative","value":15}"#).unwrap();
        assert_eq!(apply_volume_command(30, &cmd), 45);
    }

    #[test]
    fn relative_negative_clamps() {
        let cmd: VolumeCommand =
            serde_json::from_str(r#"{"how":"relative","value":-50}"#).unwrap();
        assert_eq!(apply_volume_command(30, &cmd), 0);
    }

    #[test]
    fn step_positive() {
        let cmd: VolumeCommand =
            serde_json::from_str(r#"{"how":"step","value":2}"#).unwrap();
        assert_eq!(apply_volume_command(30, &cmd), 40);
    }

    #[test]
    fn step_negative() {
        let cmd: VolumeCommand =
            serde_json::from_str(r#"{"how":"step","value":-1}"#).unwrap();
        assert_eq!(apply_volume_command(30, &cmd), 25);
    }

    #[test]
    fn step_clamps_high() {
        let cmd: VolumeCommand =
            serde_json::from_str(r#"{"how":"step","value":50}"#).unwrap();
        assert_eq!(apply_volume_command(80, &cmd), 100);
    }

    #[test]
    fn set_output_parses() {
        let cmd: SetOutputCommand =
            serde_json::from_str(r#"{"device_uid":"BuiltInSpeakerDevice"}"#).unwrap();
        assert_eq!(cmd.device_uid, "BuiltInSpeakerDevice");
    }

    #[test]
    fn mute_action_parses() {
        let cmd: MuteCommand = serde_json::from_str(r#"{"action":"toggle"}"#).unwrap();
        matches!(cmd.action, MuteAction::Toggle);
    }
}
