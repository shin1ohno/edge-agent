//! macOS service adapter: publishes Intents to a sibling `macos-hub` process
//! over MQTT, and converts inbound `service/macos/+/state/+` publishes into
//! [`edge_core::StateUpdate`]s for the state-pump and feedback loop.
//!
//! The adapter itself contains no macOS-specific FFI — it is a pure MQTT
//! client. Any host (including Linux) that can reach the broker can run this
//! adapter and control a macOS host running `macos-hub` on the same broker.

pub mod adapter;
pub mod mqtt;
pub mod types;

#[allow(unused_imports)]
pub use adapter::{MacosAdapter, MacosConfig, SERVICE_TYPE};
