//! Input primitives and service intents exchanged by the routing engine.

use serde::{Deserialize, Serialize};

/// Physical input from a device. Device-agnostic: a Nuimo rotate and a
/// dial rotate both produce `Rotate { delta }`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum InputPrimitive {
    Rotate {
        delta: f64,
    },
    Press,
    Release,
    LongPress,
    Swipe {
        direction: Direction,
    },
    Slide {
        value: f64,
    },
    Hover {
        proximity: f64,
    },
    Touch {
        area: TouchArea,
    },
    LongTouch {
        area: TouchArea,
    },
    KeyPress {
        key: u32,
    },
    /// Numbered button press from a multi-button controller (Hue Tap Dial
    /// has 1..=4). Wire format: `"button_<id>"` (e.g. `button_1`).
    Button {
        id: u8,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TouchArea {
    Top,
    Bottom,
    Left,
    Right,
}

/// Service-level intent produced by the routing engine. Adapters translate
/// this into their service's native command.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Intent {
    Play,
    Pause,
    PlayPause,
    Stop,
    Next,
    Previous,
    VolumeChange { delta: f64 },
    VolumeSet { value: f64 },
    Mute,
    Unmute,
    SeekRelative { seconds: f64 },
    SeekAbsolute { seconds: f64 },
    BrightnessChange { delta: f64 },
    BrightnessSet { value: f64 },
    ColorTemperatureChange { delta: f64 },
    PowerToggle,
    PowerOn,
    PowerOff,
}

impl InputPrimitive {
    /// Match this primitive against a wire-format route input string
    /// (e.g. "rotate", "press", "swipe_right", "touch_top").
    pub fn matches_route(&self, route_input: &str) -> bool {
        match (self, route_input) {
            (InputPrimitive::Rotate { .. }, "rotate") => true,
            (InputPrimitive::Press, "press") => true,
            (InputPrimitive::Release, "release") => true,
            (InputPrimitive::LongPress, "long_press") => true,
            (InputPrimitive::Slide { .. }, "slide") => true,
            (InputPrimitive::Hover { .. }, "hover") => true,
            (InputPrimitive::Swipe { direction }, s) => matches!(
                (direction, s),
                (Direction::Up, "swipe_up")
                    | (Direction::Down, "swipe_down")
                    | (Direction::Left, "swipe_left")
                    | (Direction::Right, "swipe_right")
            ),
            (InputPrimitive::Touch { area }, s) => matches!(
                (area, s),
                (TouchArea::Top, "touch_top")
                    | (TouchArea::Bottom, "touch_bottom")
                    | (TouchArea::Left, "touch_left")
                    | (TouchArea::Right, "touch_right")
            ),
            (InputPrimitive::LongTouch { area }, s) => matches!(
                (area, s),
                (TouchArea::Top, "long_touch_top")
                    | (TouchArea::Bottom, "long_touch_bottom")
                    | (TouchArea::Left, "long_touch_left")
                    | (TouchArea::Right, "long_touch_right")
            ),
            (InputPrimitive::Button { id }, s) => s
                .strip_prefix("button_")
                .and_then(|n| n.parse::<u8>().ok())
                .is_some_and(|parsed| parsed == *id),
            _ => false,
        }
    }

    /// Extract the continuous value for a rotate/slide, if applicable.
    pub fn continuous_value(&self) -> Option<f64> {
        match self {
            InputPrimitive::Rotate { delta } | InputPrimitive::Slide { value: delta } => {
                Some(*delta)
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotate_matches_rotate_route() {
        let r = InputPrimitive::Rotate { delta: 0.03 };
        assert!(r.matches_route("rotate"));
        assert!(!r.matches_route("press"));
    }

    #[test]
    fn swipe_direction_matters() {
        let s = InputPrimitive::Swipe {
            direction: Direction::Right,
        };
        assert!(s.matches_route("swipe_right"));
        assert!(!s.matches_route("swipe_left"));
        assert!(!s.matches_route("swipe_up"));
    }

    #[test]
    fn button_id_matches_numbered_route() {
        let b = InputPrimitive::Button { id: 1 };
        assert!(b.matches_route("button_1"));
        assert!(!b.matches_route("button_2"));
        assert!(!b.matches_route("button_"));
        assert!(!b.matches_route("press"));
    }

    #[test]
    fn button_route_rejects_non_numeric_suffix() {
        let b = InputPrimitive::Button { id: 3 };
        assert!(!b.matches_route("button_x"));
        assert!(!b.matches_route("button_3a"));
        assert!(b.matches_route("button_3"));
    }
}
