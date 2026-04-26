//! Platform-abstracted wifi signal-strength reader.
//!
//! Returns 0..=100 percent. Returns `None` when:
//! - the platform doesn't expose a signal-strength API to user code
//! - the host has no wifi adapter (wired-only)
//! - the relevant API call fails (entitlement missing, permission denied)
//!
//! The implementation is best-effort and intentionally non-fatal: a
//! caller publishes the `Option<u8>` verbatim to the server, which
//! displays a `—` placeholder when the value is absent.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;

/// Read the wifi signal strength of the primary wireless interface, if
/// any. The value is normalised to 0..=100 percent. See module docs for
/// the conditions that produce `None`.
pub async fn measure_wifi() -> Option<u8> {
    #[cfg(target_os = "linux")]
    {
        linux::read().await
    }
    #[cfg(target_os = "macos")]
    {
        macos::read().await
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        None
    }
}
