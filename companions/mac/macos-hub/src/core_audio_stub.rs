//! Non-macOS stub: lets the crate compile on Linux for topic-parsing and
//! volume-math unit tests. All runtime calls return errors.

#![cfg(not(target_os = "macos"))]

use anyhow::{bail, Result};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct OutputDevice {
    pub id: u32,
    pub uid: String,
    pub name: String,
    pub transport_type: u32,
    pub is_airplay: bool,
}

pub fn list_outputs() -> Result<Vec<OutputDevice>> {
    bail!("Core Audio is only available on macOS")
}

pub fn get_default_output() -> Result<u32> {
    bail!("Core Audio is only available on macOS")
}

pub fn set_default_output(_id: u32) -> Result<()> {
    bail!("Core Audio is only available on macOS")
}

pub fn get_system_volume() -> Result<f32> {
    bail!("Core Audio is only available on macOS")
}

pub fn set_system_volume(_level: f32) -> Result<()> {
    bail!("Core Audio is only available on macOS")
}

pub fn find_device_by_uid(_uid: &str) -> Result<OutputDevice> {
    bail!("Core Audio is only available on macOS")
}
