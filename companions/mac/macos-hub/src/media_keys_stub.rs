//! Non-macOS stub so the binary compiles on Linux for unit tests.

#![cfg(not(target_os = "macos"))]

use anyhow::{bail, Result};

#[allow(dead_code)]
pub fn is_accessibility_trusted() -> bool {
    true // MediaRemote path doesn't need Accessibility; stub is true for parity
}

pub fn is_media_remote_available() -> bool {
    false // not on macOS
}

pub fn play_pause() -> Result<()> {
    bail!("media keys are only available on macOS")
}

pub fn next_track() -> Result<()> {
    bail!("media keys are only available on macOS")
}

pub fn previous_track() -> Result<()> {
    bail!("media keys are only available on macOS")
}
