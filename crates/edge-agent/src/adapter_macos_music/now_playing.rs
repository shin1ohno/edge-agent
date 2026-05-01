//! Polling task that reads Music.app's current player state every
//! `POLL_INTERVAL` and publishes a `now_playing` `StateUpdate` on
//! observable change.
//!
//! Music.app does not emit reliable distributed notifications for track
//! changes (the `com.apple.Music.playerInfo` darwin notification exists
//! but is undocumented and intermittent). Polling at 1 Hz keeps CPU cost
//! negligible while ensuring the LED's `track_scroll` glyph fires within
//! a second of the actual track change. The publish is deduplicated
//! against the previous payload — a stalled `paused` zone never spams
//! the WS outbox.

use edge_core::StateUpdate;
use serde_json::json;
use std::time::Duration;
use tokio::sync::broadcast;

use super::adapter::SERVICE_TYPE;
use super::osascript::run_script;

const POLL_INTERVAL: Duration = Duration::from_secs(1);
const TARGET: &str = "apple_music";

/// Combined `current_state + current_track` query. Fields are joined by
/// ASCII Unit Separator (``, `0x1F`) — the canonical record-
/// delimiter that AppleScript and Rust both pass through verbatim, and
/// that is virtually guaranteed not to appear in a track / artist /
/// album field. On `stopped` the track block is empty.
const NOW_PLAYING_SCRIPT: &str = r#"
tell application "Music"
    set _sep to (ASCII character 31)
    set _state to (player state as text)
    set _vol to sound volume
    if player state is stopped then
        return _state & _sep & _vol & _sep & _sep & _sep & _sep & _sep
    end if
    try
        set _pos to player position
        set _track to current track
        set _name to (name of _track) as text
        set _artist to (artist of _track) as text
        set _album to (album of _track) as text
        set _dur to (duration of _track)
        return _state & _sep & _vol & _sep & _pos & _sep & _name & _sep & _artist & _sep & _album & _sep & _dur
    on error
        return _state & _sep & _vol & _sep & _sep & _sep & _sep & _sep
    end try
end tell
"#;

#[derive(Debug, Default, PartialEq, Clone)]
struct PlayerSnapshot {
    state: String,
    volume: i64,
    position_seconds: Option<f64>,
    title: Option<String>,
    artist: Option<String>,
    album: Option<String>,
    duration_seconds: Option<f64>,
}

impl PlayerSnapshot {
    fn to_value(&self) -> serde_json::Value {
        json!({
            "state": self.state,
            "volume": self.volume,
            "position_seconds": self.position_seconds,
            "title": self.title,
            "artist": self.artist,
            "album": self.album,
            "duration_seconds": self.duration_seconds,
        })
    }
}

fn parse_snapshot(stdout: &str) -> Option<PlayerSnapshot> {
    let parts: Vec<&str> = stdout.split('\u{1f}').collect();
    if parts.len() < 7 {
        return None;
    }
    let state = parts[0].trim().to_string();
    let volume = parts[1].trim().parse::<i64>().ok()?;
    let position_seconds = parts[2].trim().parse::<f64>().ok();
    let title = (!parts[3].trim().is_empty()).then(|| parts[3].trim().to_string());
    let artist = (!parts[4].trim().is_empty()).then(|| parts[4].trim().to_string());
    let album = (!parts[5].trim().is_empty()).then(|| parts[5].trim().to_string());
    let duration_seconds = parts[6].trim().parse::<f64>().ok();
    Some(PlayerSnapshot {
        state,
        volume,
        position_seconds,
        title,
        artist,
        album,
        duration_seconds,
    })
}

/// Long-running publisher. Polls Music.app, publishes a `StateUpdate`
/// only when fields the user cares about (`state`, `title`, `artist`,
/// `volume`) change. Position is intentionally NOT compared — it ticks
/// every second by design and would flood the channel.
pub async fn run(state_tx: broadcast::Sender<StateUpdate>) {
    let mut prev_key: Option<(String, i64, Option<String>, Option<String>)> = None;
    loop {
        match run_script(NOW_PLAYING_SCRIPT).await {
            Ok(stdout) => {
                if let Some(snap) = parse_snapshot(&stdout) {
                    let key = (
                        snap.state.clone(),
                        snap.volume,
                        snap.title.clone(),
                        snap.artist.clone(),
                    );
                    if prev_key.as_ref() != Some(&key) {
                        let update = StateUpdate {
                            service_type: SERVICE_TYPE.into(),
                            target: TARGET.into(),
                            property: "now_playing".into(),
                            output_id: None,
                            value: snap.to_value(),
                        };
                        if let Err(e) = state_tx.send(update) {
                            tracing::debug!(error = %e, "no state subscribers — drop");
                        }
                        prev_key = Some(key);
                    }
                } else {
                    tracing::debug!(stdout = %stdout, "macos_music: unparseable snapshot");
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "macos_music: now_playing query failed");
            }
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_full_track_snapshot() {
        let line =
            "playing\u{1f}65\u{1f}42.5\u{1f}Veins\u{1f}The Chainsmokers / Daya\u{1f}Breathe\u{1f}214.0";
        let snap = parse_snapshot(line).expect("parse");
        assert_eq!(snap.state, "playing");
        assert_eq!(snap.volume, 65);
        assert_eq!(snap.position_seconds, Some(42.5));
        assert_eq!(snap.title.as_deref(), Some("Veins"));
        assert_eq!(snap.artist.as_deref(), Some("The Chainsmokers / Daya"));
        assert_eq!(snap.album.as_deref(), Some("Breathe"));
        assert_eq!(snap.duration_seconds, Some(214.0));
    }

    #[test]
    fn parse_stopped_snapshot() {
        let line = "stopped\u{1f}50\u{1f}\u{1f}\u{1f}\u{1f}\u{1f}";
        let snap = parse_snapshot(line).expect("parse");
        assert_eq!(snap.state, "stopped");
        assert_eq!(snap.volume, 50);
        assert_eq!(snap.position_seconds, None);
        assert_eq!(snap.title, None);
    }

    #[test]
    fn parse_handles_track_with_separator_in_name_gracefully() {
        // ASCII Unit Separator embedded mid-title would split the field.
        // Music.app doesn't allow this character in metadata in practice,
        // but the parser must not panic if it ever appears.
        let line = "playing\u{1f}65\u{1f}10.0\u{1f}A\u{1f}B\u{1f}C\u{1f}\u{1f}D";
        // 8 parts now; first 7 are taken — "extra" `D` is ignored. The
        // duration_seconds field will be empty so parse_f64 fails →
        // returns None for that field.
        let snap = parse_snapshot(line).expect("parse");
        assert_eq!(snap.state, "playing");
        // duration empty → None
        assert_eq!(snap.duration_seconds, None);
    }
}
