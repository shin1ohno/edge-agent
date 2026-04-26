//! Device-control abstraction surfaced over `/ws/edge`.
//!
//! `weave-server` may send `ServerToEdge::DisplayGlyph` /
//! `ServerToEdge::DeviceConnect` / `ServerToEdge::DeviceDisconnect` frames
//! to drive a specific device on the edge — used by the weave-web UI
//! buttons (Connect / Disconnect / Test LED). The native edge-agent owns
//! per-device handles (BLE for Nuimo, etc.) that are foreign to this crate,
//! so the WS handler dispatches via this trait. The agent registers an
//! impl backed by its device map; targets the trait doesn't recognise
//! return `Ok(())` after logging.
//!
//! Errors propagate as `anyhow::Error` for symmetry with the rest of the
//! WS client surface and to keep the trait dyn-friendly.

use async_trait::async_trait;

/// Trait the WS client uses to dispatch device-control frames. The
/// concrete impl lives in the binary that owns the device handles
/// (`edge-agent` for Nuimo today; future targets register their own).
#[async_trait]
pub trait DeviceControlHook: Send + Sync {
    /// Render `pattern` on the device's display.
    /// `pattern` is a 9-line ASCII grid (`*` = on).
    /// `brightness` is on the closed interval `0.0..=1.0`; `None` =
    /// implementation default.
    /// `timeout_ms` is the auto-clear timeout; `None` = implementation
    /// default.
    /// `transition` is `"immediate"` or `"cross_fade"`; `None` =
    /// implementation default.
    async fn display_glyph(
        &self,
        device_type: &str,
        device_id: &str,
        pattern: &str,
        brightness: Option<f32>,
        timeout_ms: Option<u32>,
        transition: Option<&str>,
    ) -> anyhow::Result<()>;

    /// Re-attempt a connection to the device. Implementations should clear
    /// any "paused" flag that suppresses auto-reconnect.
    async fn connect_device(&self, device_type: &str, device_id: &str) -> anyhow::Result<()>;

    /// Drop the active connection and set "paused" so the auto-reconnect
    /// loop does not immediately re-establish the link.
    async fn disconnect_device(&self, device_type: &str, device_id: &str) -> anyhow::Result<()>;
}

/// No-op implementation used when the host hasn't registered a device-
/// control backend. Keeps the WS frames from crashing a stripped-down
/// build that doesn't supervise BLE devices.
pub struct NoopDeviceControl;

#[async_trait]
impl DeviceControlHook for NoopDeviceControl {
    async fn display_glyph(
        &self,
        device_type: &str,
        device_id: &str,
        _pattern: &str,
        _brightness: Option<f32>,
        _timeout_ms: Option<u32>,
        _transition: Option<&str>,
    ) -> anyhow::Result<()> {
        tracing::warn!(
            device_type,
            device_id,
            "DisplayGlyph received but no device-control hook is registered"
        );
        Ok(())
    }

    async fn connect_device(&self, device_type: &str, device_id: &str) -> anyhow::Result<()> {
        tracing::warn!(
            device_type,
            device_id,
            "DeviceConnect received but no device-control hook is registered"
        );
        Ok(())
    }

    async fn disconnect_device(&self, device_type: &str, device_id: &str) -> anyhow::Result<()> {
        tracing::warn!(
            device_type,
            device_id,
            "DeviceDisconnect received but no device-control hook is registered"
        );
        Ok(())
    }
}
