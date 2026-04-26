//! Device control sink for the iOS edge.
//!
//! Mirrors the `LedFeedbackSink` pattern from `feedback_pump.rs`: the Rust
//! WebSocket loop receives `ServerToEdge::DisplayGlyph` /
//! `DeviceConnect` / `DeviceDisconnect` frames, and dispatches them
//! across this UniFFI-exported foreign trait so Swift can drive its
//! `BleBridge` without re-implementing the BLE stack in Rust on iOS.
//!
//! The trait is intentionally **synchronous**. UniFFI generates a
//! `Task { ... }` wrapper for async foreign trait methods that captures
//! the call closure — Swift 6 strict concurrency rejects the generated
//! code as a `sending`-parameter data race (`WeaveIosCore.swift:3776`).
//! The sync shape sidesteps that entirely. Implementations that need to
//! hop onto a main thread should do so themselves (see
//! `DeviceControlBridge.swift`'s `DispatchQueue.main.async`).
//!
//! Linux/macOS edges don't use this trait — they have a direct in-process
//! `DeviceControlHook` (see `crates/edge-agent/src/main.rs`).

/// Swift-implemented sink for server-driven device control.
///
/// Each method maps 1:1 to a `ServerToEdge` variant the WS loop receives.
/// Implementations should be quick — long-running BLE work belongs on a
/// task the implementation spawns, not inside the sink call.
#[uniffi::export(with_foreign)]
pub trait DeviceControlSink: Send + Sync {
    /// Reconnect a previously-paired device. Idempotent: already-connected
    /// devices are a no-op aside from clearing any "paused" state that
    /// previously suppressed reconnect attempts.
    fn connect_device(&self, device_type: String, device_id: String);

    /// Disconnect a device. The implementation should set a paused flag
    /// so the auto-reconnect loop does not immediately re-establish.
    fn disconnect_device(&self, device_type: String, device_id: String);

    /// Render a glyph on the device's LED. Used by the weave-web "Test
    /// LED" affordance to verify the display path without waiting for a
    /// service-state event.
    ///
    /// `pattern` is a 9-line ASCII grid (`*` = on, anything else = off);
    /// optional fields default at the implementation's discretion.
    fn display_glyph(
        &self,
        device_type: String,
        device_id: String,
        pattern: String,
        brightness: Option<f32>,
        timeout_ms: Option<u32>,
        transition: Option<String>,
    );
}
