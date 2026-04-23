//! Intent → MQTT payload conversions and state-topic parsing.
//!
//! Topic schema (contract with `macos-hub`):
//!
//!   Publish  (edge-agent → broker → macos-hub):
//!     `service/macos/{target}/command/{intent_name}`
//!   Subscribe (macos-hub → broker → edge-agent):
//!     `service/macos/+/state/+`
//!
//! Payload shapes are documented inline on each converter.

use edge_core::Intent;

/// Translate an [`Intent`] into the `(topic, payload)` pair to publish.
///
/// Returns `None` for intents that have no macOS equivalent (brightness,
/// power toggles, etc.) so the caller can log-and-skip instead of erroring.
pub fn intent_to_mqtt(intent: &Intent, target: &str) -> Option<(String, Vec<u8>)> {
    match intent {
        Intent::Play => Some((format!("service/macos/{}/command/play", target), vec![])),
        Intent::Pause => Some((format!("service/macos/{}/command/pause", target), vec![])),
        Intent::PlayPause => Some((
            format!("service/macos/{}/command/play_pause", target),
            vec![],
        )),
        Intent::Next => Some((format!("service/macos/{}/command/next", target), vec![])),
        Intent::Previous => Some((format!("service/macos/{}/command/previous", target), vec![])),
        Intent::Stop => Some((format!("service/macos/{}/command/stop", target), vec![])),
        Intent::VolumeSet { value } => {
            // Intent::VolumeSet.value is 0..=100 in the existing edge-agent
            // convention (see adapter_hue for brightness). macos-hub expects
            // an integer percent in the same range.
            let v = value.clamp(0.0, 100.0).round() as i32;
            let payload = serde_json::json!({"how": "absolute", "value": v});
            Some((
                format!("service/macos/{}/command/volume", target),
                serde_json::to_vec(&payload).ok()?,
            ))
        }
        Intent::VolumeChange { delta } => {
            // delta is unit-less relative to the same 0..=100 scale. Clamp
            // to +/-100 so a runaway gesture cannot send a pathological
            // value to the hub.
            let v = delta.clamp(-100.0, 100.0).round() as i32;
            let payload = serde_json::json!({"how": "relative", "value": v});
            Some((
                format!("service/macos/{}/command/volume", target),
                serde_json::to_vec(&payload).ok()?,
            ))
        }
        Intent::Mute => {
            let payload = serde_json::json!({"action": "toggle"});
            Some((
                format!("service/macos/{}/command/mute", target),
                serde_json::to_vec(&payload).ok()?,
            ))
        }
        Intent::Unmute => {
            let payload = serde_json::json!({"action": "unmute"});
            Some((
                format!("service/macos/{}/command/mute", target),
                serde_json::to_vec(&payload).ok()?,
            ))
        }
        _ => None,
    }
}

/// Parse a `service/macos/{edge_id}/state/{property}` topic into its
/// `(edge_id, property)` parts. Returns `None` for any other shape so the
/// caller can drop malformed / unrelated publishes silently.
pub fn parse_state_topic(topic: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = topic.split('/').collect();
    if parts.len() == 5
        && parts[0] == "service"
        && parts[1] == "macos"
        && parts[3] == "state"
        && !parts[2].is_empty()
        && !parts[4].is_empty()
    {
        Some((parts[2].to_string(), parts[4].to_string()))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn play_pause_encodes_empty_payload() {
        let (topic, payload) = intent_to_mqtt(&Intent::PlayPause, "studio").unwrap();
        assert_eq!(topic, "service/macos/studio/command/play_pause");
        assert!(payload.is_empty());
    }

    #[test]
    fn discrete_playback_intents_use_distinct_topics() {
        for (intent, suffix) in [
            (Intent::Play, "play"),
            (Intent::Pause, "pause"),
            (Intent::Next, "next"),
            (Intent::Previous, "previous"),
            (Intent::Stop, "stop"),
        ] {
            let (topic, payload) = intent_to_mqtt(&intent, "t").unwrap();
            assert_eq!(topic, format!("service/macos/t/command/{suffix}"));
            assert!(payload.is_empty());
        }
    }

    #[test]
    fn volume_set_encodes_absolute() {
        let (topic, payload) =
            intent_to_mqtt(&Intent::VolumeSet { value: 42.7 }, "studio").unwrap();
        assert_eq!(topic, "service/macos/studio/command/volume");
        let v: serde_json::Value = serde_json::from_slice(&payload).unwrap();
        assert_eq!(v, serde_json::json!({"how": "absolute", "value": 43}));
    }

    #[test]
    fn volume_set_clamps_out_of_range() {
        let (_, payload) = intent_to_mqtt(&Intent::VolumeSet { value: 250.0 }, "studio").unwrap();
        let v: serde_json::Value = serde_json::from_slice(&payload).unwrap();
        assert_eq!(v["value"], 100);
    }

    #[test]
    fn volume_change_encodes_relative() {
        let (topic, payload) =
            intent_to_mqtt(&Intent::VolumeChange { delta: -5.0 }, "studio").unwrap();
        assert_eq!(topic, "service/macos/studio/command/volume");
        let v: serde_json::Value = serde_json::from_slice(&payload).unwrap();
        assert_eq!(v, serde_json::json!({"how": "relative", "value": -5}));
    }

    #[test]
    fn mute_toggle_and_unmute() {
        let (_, toggle) = intent_to_mqtt(&Intent::Mute, "t").unwrap();
        let t: serde_json::Value = serde_json::from_slice(&toggle).unwrap();
        assert_eq!(t, serde_json::json!({"action": "toggle"}));

        let (_, unmute) = intent_to_mqtt(&Intent::Unmute, "t").unwrap();
        let u: serde_json::Value = serde_json::from_slice(&unmute).unwrap();
        assert_eq!(u, serde_json::json!({"action": "unmute"}));
    }

    #[test]
    fn non_applicable_intents_return_none() {
        assert!(intent_to_mqtt(&Intent::PowerOn, "t").is_none());
        assert!(intent_to_mqtt(&Intent::BrightnessSet { value: 50.0 }, "t").is_none());
        assert!(intent_to_mqtt(&Intent::SeekRelative { seconds: 10.0 }, "t").is_none());
    }

    #[test]
    fn parse_state_topic_happy_path() {
        let parsed = parse_state_topic("service/macos/studio/state/playback_active").unwrap();
        assert_eq!(
            parsed,
            ("studio".to_string(), "playback_active".to_string())
        );
    }

    #[test]
    fn parse_state_topic_rejects_bad_shape() {
        assert!(parse_state_topic("service/macos/studio/command/play").is_none());
        assert!(parse_state_topic("service/hue/light/state/on").is_none());
        assert!(parse_state_topic("service/macos/studio/state").is_none());
        assert!(parse_state_topic("service/macos//state/volume").is_none());
        assert!(parse_state_topic("service/macos/studio/state/").is_none());
    }
}
