//! macOS Music.app service adapter.
//!
//! Drives the local `Music.app` via AppleScript (`osascript`) so any host
//! running edge-agent on macOS can answer to `service_type = "macos_music"`.
//! Mirrors the iOS `ios_media` publish shape (`property: "now_playing"`,
//! flat `{title, artist, album, state, volume, position_seconds,
//! duration_seconds}`) so the same feedback rules — `track_scroll`,
//! `volume_bar`, `playback_glyph`, `mute_glyph` — bind without service-
//! specific branches.
//!
//! The dispatcher path is local: a Nuimo paired with this host routes
//! straight into `Music.app` without crossing weave-server. Cross-edge
//! dispatch keeps working for hosts that *don't* have this adapter — they
//! forward to a peer that does, via the path landed in PR #92.

pub mod adapter;
pub mod now_playing;
pub mod osascript;

pub use adapter::{MacosMusicAdapter, MacosMusicConfig};
