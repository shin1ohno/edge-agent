//! Non-macOS stub so the binary compiles on Linux for unit tests.

#![cfg(not(target_os = "macos"))]

use anyhow::{bail, Result};

pub fn play_pause() -> Result<()> {
    bail!("media keys are only available on macOS")
}

pub fn next_track() -> Result<()> {
    bail!("media keys are only available on macOS")
}

pub fn previous_track() -> Result<()> {
    bail!("media keys are only available on macOS")
}
