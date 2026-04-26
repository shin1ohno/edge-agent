#![allow(dead_code)]
#![allow(unused_imports)]
//! Routing engine, service-adapter trait, and WebSocket client for `edge-agent`.

pub mod adapter;
pub mod cache;
pub mod device_control;
pub mod intent;
pub mod registry;
pub mod routing;
pub mod ws_client;

pub use adapter::{ServiceAdapter, StateUpdate};
pub use device_control::{DeviceControlHook, NoopDeviceControl};
pub use intent::{Direction, InputPrimitive, Intent, TouchArea};
pub use registry::GlyphRegistry;
pub use routing::{RouteOutcome, RoutedIntent, RoutingEngine};
pub use ws_client::WsClient;
