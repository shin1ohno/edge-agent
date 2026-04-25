//! UniFFI wrapper for the weave iOS/iPad app.
//!
//! Swift owns CoreBluetooth + the UI. This crate provides the pure-Rust
//! logic the app needs: parsing Nuimo GATT notifications, encoding the LED
//! matrix payload, and (future: Phase 3+) the WebSocket/REST clients and
//! routing runtime.
//!
//! All exports flow through UniFFI proc-macros — there is no UDL file.

uniffi::setup_scaffolding!();

mod edge_client;
mod ui_client;
pub use edge_client::{EdgeClient, EdgeEventSink};
pub use ui_client::{UiClient, UiEventSink};

use nuimo_protocol as np;
use thiserror::Error;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors returned across the FFI boundary. Kept flat so Swift sees a simple
/// enum; the original `ParseError` variants collapse into `ParseFailed`.
#[derive(Debug, Error, uniffi::Error)]
pub enum WeaveError {
    #[error("invalid UUID: {message}")]
    InvalidUuid { message: String },
    #[error("parse failed: {message}")]
    ParseFailed { message: String },
    #[error("network: {message}")]
    Network { message: String },
    #[error("HTTP {status}: {message}")]
    Http { status: u16, message: String },
}

impl From<uuid::Error> for WeaveError {
    fn from(e: uuid::Error) -> Self {
        Self::InvalidUuid {
            message: e.to_string(),
        }
    }
}

impl From<np::ParseError> for WeaveError {
    fn from(e: np::ParseError) -> Self {
        Self::ParseFailed {
            message: e.to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Value types — mirror nuimo_protocol but with UniFFI-compatible shape
// ---------------------------------------------------------------------------

/// Events emitted by a Nuimo device.
///
/// Mirrors `nuimo_protocol::NuimoEvent`; duplicated here because that crate
/// does not depend on UniFFI.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum NuimoEvent {
    ButtonDown,
    ButtonUp,
    Rotate { delta: f64, rotation: f64 },
    SwipeUp,
    SwipeDown,
    SwipeLeft,
    SwipeRight,
    TouchTop,
    TouchBottom,
    TouchLeft,
    TouchRight,
    LongTouchLeft,
    LongTouchRight,
    LongTouchTop,
    LongTouchBottom,
    FlyLeft,
    FlyRight,
    Hover { proximity: f64 },
    BatteryLevel { level: u8 },
}

impl From<np::NuimoEvent> for NuimoEvent {
    fn from(e: np::NuimoEvent) -> Self {
        match e {
            np::NuimoEvent::ButtonDown => Self::ButtonDown,
            np::NuimoEvent::ButtonUp => Self::ButtonUp,
            np::NuimoEvent::Rotate { delta, rotation } => Self::Rotate { delta, rotation },
            np::NuimoEvent::SwipeUp => Self::SwipeUp,
            np::NuimoEvent::SwipeDown => Self::SwipeDown,
            np::NuimoEvent::SwipeLeft => Self::SwipeLeft,
            np::NuimoEvent::SwipeRight => Self::SwipeRight,
            np::NuimoEvent::TouchTop => Self::TouchTop,
            np::NuimoEvent::TouchBottom => Self::TouchBottom,
            np::NuimoEvent::TouchLeft => Self::TouchLeft,
            np::NuimoEvent::TouchRight => Self::TouchRight,
            np::NuimoEvent::LongTouchLeft => Self::LongTouchLeft,
            np::NuimoEvent::LongTouchRight => Self::LongTouchRight,
            np::NuimoEvent::LongTouchTop => Self::LongTouchTop,
            np::NuimoEvent::LongTouchBottom => Self::LongTouchBottom,
            np::NuimoEvent::FlyLeft => Self::FlyLeft,
            np::NuimoEvent::FlyRight => Self::FlyRight,
            np::NuimoEvent::Hover { proximity } => Self::Hover { proximity },
            np::NuimoEvent::BatteryLevel { level } => Self::BatteryLevel { level },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum DisplayTransition {
    Immediate,
    CrossFade,
}

impl From<DisplayTransition> for np::DisplayTransition {
    fn from(t: DisplayTransition) -> Self {
        match t {
            DisplayTransition::Immediate => np::DisplayTransition::Immediate,
            DisplayTransition::CrossFade => np::DisplayTransition::CrossFade,
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct DisplayOptions {
    /// Brightness 0.0..=1.0.
    pub brightness: f64,
    /// Auto-clear timeout in milliseconds, clamped to 25500.
    pub timeout_ms: u32,
    pub transition: DisplayTransition,
}

impl From<DisplayOptions> for np::DisplayOptions {
    fn from(o: DisplayOptions) -> Self {
        np::DisplayOptions {
            brightness: o.brightness,
            timeout_ms: o.timeout_ms,
            transition: o.transition.into(),
        }
    }
}

/// A 9×9 LED glyph. `rows` contains exactly 9 entries; each row is a
/// 9-bit-wide bitmask (bit 0 = leftmost pixel). Out-of-range values are
/// accepted at the FFI boundary and masked to 9 bits during encode.
#[derive(Debug, Clone, uniffi::Record)]
pub struct Glyph {
    pub rows: Vec<u16>,
}

impl TryFrom<Glyph> for np::Glyph {
    type Error = WeaveError;

    fn try_from(g: Glyph) -> Result<Self, Self::Error> {
        if g.rows.len() != np::LED_ROWS {
            return Err(WeaveError::ParseFailed {
                message: format!(
                    "glyph must have exactly {} rows, got {}",
                    np::LED_ROWS,
                    g.rows.len()
                ),
            });
        }
        let mut rows = [0u16; np::LED_ROWS];
        for (i, r) in g.rows.iter().enumerate() {
            rows[i] = r & 0x1FF;
        }
        Ok(np::Glyph { rows })
    }
}

// ---------------------------------------------------------------------------
// Exported functions
// ---------------------------------------------------------------------------

/// Parse a raw BLE notification payload into a Nuimo event.
///
/// `char_uuid` is the characteristic UUID the notification came from, in
/// standard lowercase hyphenated form (e.g. `"f29b1529-cb19-40f3-be5c-7241ecb82fd2"`).
/// Returns `Ok(None)` when the UUID is a known notify source but the payload
/// maps to a non-event code (e.g. reserved fly byte).
#[uniffi::export]
pub fn parse_nuimo_notification(
    char_uuid: String,
    data: Vec<u8>,
) -> Result<Option<NuimoEvent>, WeaveError> {
    let uuid = Uuid::parse_str(&char_uuid)?;
    let parsed = np::parse_notification(&uuid, &data)?;
    Ok(parsed.map(NuimoEvent::from))
}

/// Encode a glyph + display options into the 13-byte payload the Nuimo LED
/// matrix characteristic expects.
#[uniffi::export]
pub fn build_led_payload(glyph: Glyph, opts: DisplayOptions) -> Result<Vec<u8>, WeaveError> {
    let g: np::Glyph = glyph.try_into()?;
    let o: np::DisplayOptions = opts.into();
    Ok(np::build_led_payload(&g, &o).to_vec())
}

/// Return the Nuimo service advertising UUID the iOS scanner should filter on.
/// Exported so Swift doesn't have to hard-code the GATT constant.
#[uniffi::export]
pub fn nuimo_service_uuid() -> String {
    np::NUIMO_SERVICE.to_string()
}

/// Return the LED-matrix characteristic UUID for writing display bytes.
#[uniffi::export]
pub fn led_matrix_uuid() -> String {
    np::LED_MATRIX.to_string()
}

// ---------------------------------------------------------------------------
// Host-side sanity tests (don't exercise the FFI, just the wrapping logic).
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_button_down_roundtrips() {
        let uuid = np::BUTTON_CLICK.to_string();
        let ev = parse_nuimo_notification(uuid, vec![0x01]).unwrap();
        assert_eq!(ev, Some(NuimoEvent::ButtonDown));
    }

    #[test]
    fn parse_rejects_malformed_uuid() {
        let err = parse_nuimo_notification("not-a-uuid".into(), vec![0x01]).unwrap_err();
        assert!(matches!(err, WeaveError::InvalidUuid { .. }));
    }

    #[test]
    fn encode_empty_glyph_matches_nuimo_protocol() {
        let g = Glyph {
            rows: vec![0u16; 9],
        };
        let opts = DisplayOptions {
            brightness: 1.0,
            timeout_ms: 2000,
            transition: DisplayTransition::CrossFade,
        };
        let payload = build_led_payload(g, opts).unwrap();
        assert_eq!(payload, vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 20]);
    }

    #[test]
    fn wrong_row_count_is_parse_error() {
        let g = Glyph { rows: vec![0; 8] };
        let opts = DisplayOptions {
            brightness: 1.0,
            timeout_ms: 2000,
            transition: DisplayTransition::CrossFade,
        };
        let err = build_led_payload(g, opts).unwrap_err();
        assert!(matches!(err, WeaveError::ParseFailed { .. }));
    }

    #[test]
    fn service_uuid_matches_gatt_constant() {
        assert_eq!(nuimo_service_uuid(), np::NUIMO_SERVICE.to_string());
    }
}
