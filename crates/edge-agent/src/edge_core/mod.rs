#![allow(dead_code)]
#![allow(unused_imports)]
//! Routing engine, service-adapter trait, and WebSocket client for `edge-agent`.

pub mod adapter;
pub mod cache;
pub mod intent;
pub mod registry;
pub mod routing;
pub mod ws_client;

pub use adapter::{ServiceAdapter, StateUpdate};
pub use intent::{Direction, InputPrimitive, Intent, TouchArea};
pub use registry::GlyphRegistry;
pub use routing::{RoutedIntent, RoutingEngine};
pub use ws_client::WsClient;
