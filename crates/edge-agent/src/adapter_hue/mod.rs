#![allow(dead_code)]
#![allow(unused_imports)]
//! Philips Hue v2 service adapter for the direct edge-agent path.
//!
//! Exposes a `HueAdapter` that implements [`edge_core::ServiceAdapter`].
//! Bridge discovery + first-time pairing live in `discovery` / `pair` so
//! the `pair-hue` subcommand of `edge-agent` can reuse them.

pub mod adapter;
pub mod api;
pub mod cache;
pub mod discovery;
pub mod events;
pub mod mdns;
pub mod pair;
pub mod resolver;
pub mod types;

pub use adapter::{HueAdapter, HueConfig, TapDialEvent};
pub use discovery::{discover, DiscoveredBridge};
pub use pair::{pair, PairedCredentials};
pub use resolver::{resolve_bridge, ResolveSource};
