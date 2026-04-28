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
    /// In-air swipe (Nuimo only): the user waves a hand left/right above
    /// the device without touching the surface. Distinct from `Swipe`
    /// (which involves physical contact) so route mappings can target
    /// either independently. Nuimo only emits Left/Right; the enum reuses
    /// `Direction` for symmetry with `Swipe` rather than introducing a
    /// new two-variant type.
    Fly {
        direction: Direction,
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

impl Intent {
    /// Serialize the intent into its snake-case discriminant + remaining
    /// params payload — matches the on-wire shape of
    /// `EdgeToServer::Command` and `EdgeToServer::DispatchIntent`. Used
    /// by both the dispatch telemetry frame and cross-edge intent
    /// forwarding so both ends see the same encoding.
    pub fn split(&self) -> (String, serde_json::Value) {
        match serde_json::to_value(self) {
            Ok(serde_json::Value::Object(mut map)) => {
                let name = map
                    .remove("type")
                    .and_then(|v| v.as_str().map(str::to_string))
                    .unwrap_or_else(|| "unknown".to_string());
                (name, serde_json::Value::Object(map))
            }
            _ => ("unknown".to_string(), serde_json::json!({})),
        }
    }

    /// Inverse of `split`: reassemble an `Intent` from its snake-case
    /// discriminant and params payload. Used on the receiving end of
    /// `ServerToEdge::DispatchIntent`. Returns `Err` if `intent` does
    /// not name a known variant or if `params` shape doesn't match.
    pub fn reassemble(intent: &str, params: &serde_json::Value) -> Result<Self, serde_json::Error> {
        let value = match params {
            serde_json::Value::Object(map) => {
                let mut m = map.clone();
                m.insert(
                    "type".to_string(),
                    serde_json::Value::String(intent.to_string()),
                );
                serde_json::Value::Object(m)
            }
            _ => serde_json::json!({ "type": intent }),
        };
        serde_json::from_value(value)
    }
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
            (InputPrimitive::Fly { direction }, s) => matches!(
                (direction, s),
                (Direction::Left, "fly_left") | (Direction::Right, "fly_right")
            ),
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

    #[test]
    fn fly_left_matches_only_fly_left_route() {
        let f = InputPrimitive::Fly {
            direction: Direction::Left,
        };
        assert!(f.matches_route("fly_left"));
        assert!(!f.matches_route("fly_right"));
        assert!(!f.matches_route("swipe_left"));
    }

    #[test]
    fn fly_right_matches_only_fly_right_route() {
        let f = InputPrimitive::Fly {
            direction: Direction::Right,
        };
        assert!(f.matches_route("fly_right"));
        assert!(!f.matches_route("fly_left"));
        assert!(!f.matches_route("swipe_right"));
    }

    #[test]
    fn fly_up_down_directions_do_not_match_any_fly_route() {
        // Nuimo doesn't emit fly_up / fly_down at the device level —
        // those codes carry proximity (decoded as `Hover`). If a caller
        // synthesises `Fly { Up }` / `Fly { Down }`, no route string
        // matches: the wire vocabulary has no `fly_up` / `fly_down`.
        for dir in [Direction::Up, Direction::Down] {
            let f = InputPrimitive::Fly { direction: dir };
            assert!(!f.matches_route("fly_up"));
            assert!(!f.matches_route("fly_down"));
            assert!(!f.matches_route("fly_left"));
            assert!(!f.matches_route("fly_right"));
        }
    }

    #[test]
    fn fly_does_not_collide_with_swipe_route() {
        let fly = InputPrimitive::Fly {
            direction: Direction::Left,
        };
        let swipe = InputPrimitive::Swipe {
            direction: Direction::Left,
        };
        assert!(fly.matches_route("fly_left"));
        assert!(!fly.matches_route("swipe_left"));
        assert!(swipe.matches_route("swipe_left"));
        assert!(!swipe.matches_route("fly_left"));
    }

    #[test]
    fn split_payloadless_intent_yields_empty_params() {
        let (name, params) = Intent::PlayPause.split();
        assert_eq!(name, "play_pause");
        assert_eq!(params, serde_json::json!({}));
    }

    #[test]
    fn split_continuous_intent_carries_params() {
        let (name, params) = Intent::VolumeChange { delta: 0.25 }.split();
        assert_eq!(name, "volume_change");
        assert_eq!(params, serde_json::json!({ "delta": 0.25 }));
    }

    #[test]
    fn reassemble_payloadless_round_trips() {
        let (name, params) = Intent::Play.split();
        let recovered = Intent::reassemble(&name, &params).unwrap();
        assert_eq!(recovered, Intent::Play);
    }

    #[test]
    fn reassemble_continuous_round_trips() {
        let (name, params) = Intent::SeekRelative { seconds: -5.0 }.split();
        let recovered = Intent::reassemble(&name, &params).unwrap();
        assert_eq!(recovered, Intent::SeekRelative { seconds: -5.0 });
    }

    #[test]
    fn reassemble_unknown_name_errors() {
        let err = Intent::reassemble("definitely_not_an_intent", &serde_json::json!({}));
        assert!(err.is_err());
    }
}
