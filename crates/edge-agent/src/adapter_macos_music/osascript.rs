//! AppleScript intent translation + subprocess execution.

use edge_core::Intent;
use std::process::Stdio;
use tokio::process::Command;

/// Convert a routing-engine `Intent` into an AppleScript snippet, or
/// `None` when the intent has no Music.app analog (brightness, power,
/// color temperature). Volume is in the 0..=100 scale (the routing
/// engine emits damped percentage deltas — see iOS `ios_media` adapter
/// for the precedent — and `Music.app`'s `sound volume` uses the same
/// scale).
pub fn intent_to_script(intent: &Intent) -> Option<String> {
    Some(match intent {
        Intent::Play => "tell application \"Music\" to play".into(),
        Intent::Pause => "tell application \"Music\" to pause".into(),
        Intent::PlayPause => "tell application \"Music\" to playpause".into(),
        Intent::Stop => "tell application \"Music\" to stop".into(),
        Intent::Next => "tell application \"Music\" to next track".into(),
        Intent::Previous => "tell application \"Music\" to previous track".into(),
        Intent::VolumeChange { delta } => {
            let d = *delta;
            format!(
                "tell application \"Music\"\n\
                 set _v to (sound volume) + ({d})\n\
                 if _v < 0 then set _v to 0\n\
                 if _v > 100 then set _v to 100\n\
                 set sound volume to _v\n\
                 end tell"
            )
        }
        Intent::VolumeSet { value } => {
            let pct = value.clamp(0.0, 100.0).round() as i64;
            format!("tell application \"Music\" to set sound volume to {pct}")
        }
        Intent::Mute => "tell application \"Music\" to set sound volume to 0".into(),
        // Music.app has no first-class "unmute" — the user expects volume
        // restoration. Without remembering the pre-mute level there's no
        // sensible default; bump to 50% so the UI reacts. iOS adapter has
        // the same constraint.
        Intent::Unmute => "tell application \"Music\" to set sound volume to 50".into(),
        Intent::SeekRelative { seconds } => {
            let s = *seconds;
            format!(
                "tell application \"Music\" to set player position to (player position + ({s}))"
            )
        }
        Intent::SeekAbsolute { seconds } => {
            let s = seconds.max(0.0);
            format!("tell application \"Music\" to set player position to {s}")
        }
        Intent::BrightnessChange { .. }
        | Intent::BrightnessSet { .. }
        | Intent::ColorTemperatureChange { .. }
        | Intent::PowerToggle
        | Intent::PowerOn
        | Intent::PowerOff => return None,
    })
}

/// Run an AppleScript fragment via `osascript -e <script>`. Returns the
/// trimmed stdout on success. On non-zero exit, returns an error
/// containing both stdout and stderr so adapter logs surface AppleScript
/// errors verbatim.
pub async fn run_script(script: &str) -> anyhow::Result<String> {
    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        anyhow::bail!(
            "osascript failed (exit {}): {}",
            output.status.code().unwrap_or(-1),
            stderr.trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
