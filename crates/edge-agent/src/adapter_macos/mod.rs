//! macOS service adapter for the edge-agent path.
//!
//! Exposes a `MacosAdapter` that implements [`edge_core::ServiceAdapter`].
//! The adapter is a thin bridge: intents are serialized to MQTT command
//! topics consumed by the `macos-hub` binary running on a macOS host; state
//! topics published by `macos-hub` are parsed back into `StateUpdate`s.
//!
//! Topic schema (contract with `macos-hub`):
//!   Publish  (edge-agent → broker): `service/macos/{target}/command/{intent}`
//!   Subscribe (edge-agent ← broker): `service/macos/+/state/+`

pub mod adapter;
pub mod mqtt;
pub mod types;

pub use adapter::{MacosAdapter, MacosConfig};
