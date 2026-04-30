//! Transport-agnostic parser and encoder for the Nuimo BLE GATT protocol.
//!
//! This crate turns raw BLE notification bytes into [`NuimoEvent`] values and
//! encodes [`Glyph`] + [`DisplayOptions`] into the 13-byte payload the Nuimo
//! LED matrix characteristic expects. It has no BLE transport dependency so
//! the same logic can be used by `btleplug` on macOS, `bluer` on Linux, and
//! CoreBluetooth on iOS.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

pub mod font;
pub use font::{char_bits, char_glyph, char_pattern, FONT_CHAR_HEIGHT, FONT_CHAR_WIDTH};

// ---------------------------------------------------------------------------
// GATT constants
// ---------------------------------------------------------------------------

pub const BATTERY_SERVICE: Uuid = Uuid::from_u128(0x0000180f_0000_1000_8000_00805f9b34fb);
pub const LED_SERVICE: Uuid = Uuid::from_u128(0xf29b1523_cb19_40f3_be5c_7241ecb82fd1);
pub const NUIMO_SERVICE: Uuid = Uuid::from_u128(0xf29b1525_cb19_40f3_be5c_7241ecb82fd2);

pub const BATTERY_LEVEL: Uuid = Uuid::from_u128(0x00002a19_0000_1000_8000_00805f9b34fb);
pub const LED_MATRIX: Uuid = Uuid::from_u128(0xf29b1524_cb19_40f3_be5c_7241ecb82fd1);
pub const BUTTON_CLICK: Uuid = Uuid::from_u128(0xf29b1529_cb19_40f3_be5c_7241ecb82fd2);
pub const FLY: Uuid = Uuid::from_u128(0xf29b1526_cb19_40f3_be5c_7241ecb82fd2);
pub const ROTATION: Uuid = Uuid::from_u128(0xf29b1528_cb19_40f3_be5c_7241ecb82fd2);
pub const TOUCH_OR_SWIPE: Uuid = Uuid::from_u128(0xf29b1527_cb19_40f3_be5c_7241ecb82fd2);

pub const DEVICE_NAME: &str = "Nuimo";
pub const ROTATION_POINTS_PER_CYCLE: f64 = 2650.0;
pub const HOVER_PROXIMITY_POINTS: f64 = 250.0;
pub const HOVER_PROXIMITY_MIN_CLAMP: f64 = 2.0;
pub const HOVER_PROXIMITY_MAX_CLAMP: f64 = 1.0;

pub const LED_ROWS: usize = 9;
pub const LED_COLS: usize = 9;
pub const LED_BITMAP_BYTES: usize = 11;
pub const LED_FADE_FLAG: u8 = 0b0001_0000;
pub const LED_DISPLAY_BYTES: usize = 13;

// ---------------------------------------------------------------------------
// Event types
// ---------------------------------------------------------------------------

/// Events emitted by a Nuimo device.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
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

/// Errors produced when parsing a GATT notification payload.
#[derive(Debug, Error, PartialEq)]
pub enum ParseError {
    #[error("unknown characteristic UUID: {0}")]
    UnknownCharacteristic(Uuid),
    #[error("payload too short for {kind}: got {got} bytes, need {need}")]
    TooShort {
        kind: &'static str,
        got: usize,
        need: usize,
    },
    #[error("unrecognized code {code} for {kind}")]
    UnknownCode { kind: &'static str, code: u8 },
}

/// Parse a BLE notification payload into a [`NuimoEvent`].
///
/// `char_uuid` is the GATT characteristic UUID the notification came from.
/// Returns `Ok(None)` when the UUID is a known notify source but the payload
/// doesn't map to a user-visible event (e.g. fly code used for non-event
/// telemetry). Returns `Err` for malformed payloads.
pub fn parse_notification(char_uuid: &Uuid, data: &[u8]) -> Result<Option<NuimoEvent>, ParseError> {
    if *char_uuid == BATTERY_LEVEL {
        require_len("battery_level", data, 1)?;
        return Ok(Some(NuimoEvent::BatteryLevel { level: data[0] }));
    }
    if *char_uuid == BUTTON_CLICK {
        require_len("button_click", data, 1)?;
        return Ok(match data[0] {
            1 => Some(NuimoEvent::ButtonDown),
            0 => Some(NuimoEvent::ButtonUp),
            code => {
                return Err(ParseError::UnknownCode {
                    kind: "button_click",
                    code,
                });
            }
        });
    }
    if *char_uuid == ROTATION {
        require_len("rotation", data, 2)?;
        let raw = i16::from_le_bytes([data[0], data[1]]);
        let delta = raw as f64 / ROTATION_POINTS_PER_CYCLE;
        return Ok(Some(NuimoEvent::Rotate {
            delta,
            rotation: 0.0,
        }));
    }
    if *char_uuid == TOUCH_OR_SWIPE {
        require_len("touch_or_swipe", data, 1)?;
        return Ok(parse_touch_or_swipe(data[0]));
    }
    if *char_uuid == FLY {
        require_len("fly", data, 1)?;
        return Ok(parse_fly(data));
    }
    Err(ParseError::UnknownCharacteristic(*char_uuid))
}

fn require_len(kind: &'static str, data: &[u8], need: usize) -> Result<(), ParseError> {
    if data.len() < need {
        return Err(ParseError::TooShort {
            kind,
            got: data.len(),
            need,
        });
    }
    Ok(())
}

fn parse_touch_or_swipe(code: u8) -> Option<NuimoEvent> {
    match code {
        0 => Some(NuimoEvent::SwipeLeft),
        1 => Some(NuimoEvent::SwipeRight),
        2 => Some(NuimoEvent::SwipeUp),
        3 => Some(NuimoEvent::SwipeDown),
        4 => Some(NuimoEvent::TouchLeft),
        5 => Some(NuimoEvent::TouchRight),
        6 => Some(NuimoEvent::TouchTop),
        7 => Some(NuimoEvent::TouchBottom),
        8 => Some(NuimoEvent::LongTouchLeft),
        9 => Some(NuimoEvent::LongTouchRight),
        10 => Some(NuimoEvent::LongTouchTop),
        11 => Some(NuimoEvent::LongTouchBottom),
        _ => None,
    }
}

fn parse_fly(data: &[u8]) -> Option<NuimoEvent> {
    if data.is_empty() {
        return None;
    }
    match data[0] {
        0 => Some(NuimoEvent::FlyLeft),
        1 => Some(NuimoEvent::FlyRight),
        4 if data.len() >= 2 => {
            let raw = data[1] as f64;
            let proximity = ((raw - HOVER_PROXIMITY_MIN_CLAMP)
                / (HOVER_PROXIMITY_POINTS - HOVER_PROXIMITY_MIN_CLAMP - HOVER_PROXIMITY_MAX_CLAMP))
                .clamp(0.0, 1.0);
            Some(NuimoEvent::Hover { proximity })
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Glyph + display encoding
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DisplayTransition {
    Immediate,
    CrossFade,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayOptions {
    /// Brightness 0.0..=1.0.
    pub brightness: f64,
    /// Auto-clear timeout in milliseconds, clamped to 25500.
    pub timeout_ms: u32,
    pub transition: DisplayTransition,
}

impl Default for DisplayOptions {
    fn default() -> Self {
        Self {
            brightness: 1.0,
            timeout_ms: 2000,
            transition: DisplayTransition::CrossFade,
        }
    }
}

/// A 9x9 LED glyph for the Nuimo display.
///
/// Each row is a 9-bit value (bit 0 = leftmost pixel).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Glyph {
    pub rows: [u16; LED_ROWS],
}

impl Glyph {
    pub fn empty() -> Self {
        Self {
            rows: [0; LED_ROWS],
        }
    }

    pub fn filled() -> Self {
        Self {
            rows: [0x1FF; LED_ROWS],
        }
    }

    /// Parse an ASCII grid: `*` is on, anything else is off. Newlines separate
    /// rows; excess rows/columns are silently dropped.
    pub fn from_ascii(s: &str) -> Self {
        let mut rows = [0u16; LED_ROWS];
        for (row_idx, line) in s.lines().enumerate() {
            if row_idx >= LED_ROWS {
                break;
            }
            let mut val = 0u16;
            for (col_idx, ch) in line.chars().enumerate() {
                if col_idx >= LED_COLS {
                    break;
                }
                if ch == '*' {
                    val |= 1 << col_idx;
                }
            }
            rows[row_idx] = val;
        }
        Self { rows }
    }

    pub fn invert(&self) -> Self {
        let mut rows = self.rows;
        for row in &mut rows {
            *row ^= 0x1FF;
        }
        Self { rows }
    }

    /// Inverse of `from_ascii`: encode the 9x9 grid back to a
    /// newline-separated `*`/`.` string. Useful for callers that need
    /// to round-trip a Glyph through APIs that consume the ASCII grid
    /// shape (e.g. `nuimo::Glyph::from_str`,
    /// `DeviceControlSink::display_glyph`).
    pub fn to_ascii(&self) -> String {
        let mut s = String::with_capacity(LED_ROWS * (LED_COLS + 1));
        for (r, row_bits) in self.rows.iter().enumerate() {
            for col in 0..LED_COLS {
                s.push(if row_bits & (1 << col) != 0 { '*' } else { '.' });
            }
            if r + 1 < LED_ROWS {
                s.push('\n');
            }
        }
        s
    }

    /// Encode the 9x9 grid into the 11-byte bitmap Nuimo's LED characteristic
    /// expects (81 bits packed LSB-first).
    pub fn to_bitmap(&self) -> [u8; LED_BITMAP_BYTES] {
        let mut buf = [0u8; LED_BITMAP_BYTES];
        let mut bit_pos = 0usize;
        for row in &self.rows {
            for col in 0..LED_COLS {
                if row & (1 << col) != 0 {
                    buf[bit_pos / 8] |= 1 << (bit_pos % 8);
                }
                bit_pos += 1;
            }
        }
        buf
    }
}

/// Fill direction for the volume bar.
///
/// `BottomUp` matches linear volumes (0..=max): zero bars = empty, max
/// bars = full, with the bottom row lit first. `TopDown` matches dB-style
/// volumes whose max is 0 — the topmost row represents "at 0 dB" and
/// fewer dots below mean more attenuation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VolumeDirection {
    BottomUp,
    TopDown,
}

/// Volume-bar glyph, 0..=9 LEDs lit in the centre column. The bar count
/// plus direction is the rendered unit of change; callers can dedup on
/// the tuple so two state updates that round to the same `(bars,
/// direction)` don't trigger a redraw.
pub fn volume_bars(bars: u8, direction: VolumeDirection) -> Glyph {
    let bars = bars.min(LED_ROWS as u8) as usize;
    let mut rows = [0u16; LED_ROWS];
    let lit_col = 1u16 << (LED_COLS / 2); // centre column (col 4 of 9)
    for (row_idx, row) in rows.iter_mut().enumerate() {
        let lit = match direction {
            VolumeDirection::BottomUp => (LED_ROWS - 1 - row_idx) < bars,
            VolumeDirection::TopDown => row_idx < bars,
        };
        if lit {
            *row = lit_col;
        }
    }
    Glyph { rows }
}

/// Build the 13-byte payload for the LED matrix characteristic:
/// 11 bytes of bitmap, 1 byte brightness (0-255), 1 byte timeout (ms/100).
pub fn build_led_payload(glyph: &Glyph, opts: &DisplayOptions) -> [u8; LED_DISPLAY_BYTES] {
    let mut bitmap = glyph.to_bitmap();

    // The fade flag is inverted: when the bit is set, Nuimo uses Immediate.
    if opts.transition == DisplayTransition::Immediate {
        bitmap[10] ^= LED_FADE_FLAG;
    }

    let brightness = (opts.brightness.clamp(0.0, 1.0) * 255.0) as u8;
    let timeout = (opts.timeout_ms.min(25500) / 100) as u8;

    let mut out = [0u8; LED_DISPLAY_BYTES];
    out[..LED_BITMAP_BYTES].copy_from_slice(&bitmap);
    out[LED_BITMAP_BYTES] = brightness;
    out[LED_BITMAP_BYTES + 1] = timeout;
    out
}

#[cfg(test)]
mod volume_bars_tests {
    use super::*;

    #[test]
    fn bottom_up_three_lights_bottom_three_rows() {
        let g = volume_bars(3, VolumeDirection::BottomUp);
        // Centre column = bit 4 = 0x10. Bottom three rows (indices 6, 7, 8) lit.
        assert_eq!(
            g.rows,
            [0, 0, 0, 0, 0, 0, 0x10, 0x10, 0x10],
            "bottom-up 3 should light rows 6..=8 in the centre column"
        );
    }

    #[test]
    fn top_down_three_lights_top_three_rows() {
        let g = volume_bars(3, VolumeDirection::TopDown);
        assert_eq!(
            g.rows,
            [0x10, 0x10, 0x10, 0, 0, 0, 0, 0, 0],
            "top-down 3 should light rows 0..=2 in the centre column"
        );
    }

    #[test]
    fn nine_is_full_either_direction() {
        let bu = volume_bars(9, VolumeDirection::BottomUp);
        let td = volume_bars(9, VolumeDirection::TopDown);
        assert_eq!(bu.rows, [0x10; LED_ROWS]);
        assert_eq!(bu, td);
    }

    #[test]
    fn zero_is_empty_either_direction() {
        let bu = volume_bars(0, VolumeDirection::BottomUp);
        let td = volume_bars(0, VolumeDirection::TopDown);
        assert_eq!(bu.rows, [0; LED_ROWS]);
        assert_eq!(bu, td);
    }

    #[test]
    fn over_nine_clamps_to_full() {
        assert_eq!(
            volume_bars(50, VolumeDirection::BottomUp),
            volume_bars(9, VolumeDirection::BottomUp)
        );
    }
}
