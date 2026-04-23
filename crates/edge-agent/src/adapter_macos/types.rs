//! Intent → MQTT command translation and state topic parsing for the
//! macOS adapter. Kept separate from the adapter so it can be unit-tested
//! without spinning up an MQTT client.

use edge_core::Intent;

/// Project an [`Intent`] into the `(topic, payload)` pair that should be
/// published for a given `target` (macos-hub edge id).
///
/// Returns `None` for intents that do not apply to the macOS service
/// (brightness, color temperature, ...); the caller should treat `None`
/// as a no-op skip rather than an error.
pub fn intent_to_mqtt(intent: &Intent, target: &str) -> Option<(String, Vec<u8>)> {
    match intent {
        Intent::Play => Some((format!("service/macos/{}/command/play", target), Vec::new())),
        Intent::Pause => Some((
            format!("service/macos/{}/command/pause", target),
            Vec::new(),
        )),
        Intent::PlayPause => Some((
            format!("service/macos/{}/command/play_pause", target),
            Vec::new(),
        )),
        Intent::Next => Some((format!("service/macos/{}/command/next", target), Vec::new())),
        Intent::Previous => Some((
            format!("service/macos/{}/command/previous", target),
            Vec::new(),
        )),
        Intent::Stop => Some((format!("service/macos/{}/command/stop", target), Vec::new())),
        Intent::VolumeSet { value } => {
            // Routing engine produces `value` as a 0.0..=1.0 ratio; macos-hub
            // expects a 0..=100 integer percentage in absolute mode.
            let v = (value * 100.0).round().clamp(0.0, 100.0) as i32;
            let payload = serde_json::to_vec(&serde_json::json!({
                "how": "absolute",
                "value": v,
            }))
            .ok()?;
            Some((format!("service/macos/{}/command/volume", target), payload))
        }
        Intent::VolumeChange { delta } => {
            // `delta` is signed, ±1.0 covering the full range. Rescale to
            // percentage points and clamp to the -100..=100 domain so a
            // runaway gesture can't produce out-of-range hub commands.
            let v = (delta * 100.0).round().clamp(-100.0, 100.0) as i32;
            let payload = serde_json::to_vec(&serde_json::json!({
                "how": "relative",
                "value": v,
            }))
            .ok()?;
            Some((format!("service/macos/{}/command/volume", target), payload))
        }
        Intent::Mute => {
            let payload = serde_json::to_vec(&serde_json::json!({"action": "toggle"})).ok()?;
            Some((format!("service/macos/{}/command/mute", target), payload))
        }
        Intent::Unmute => {
            let payload = serde_json::to_vec(&serde_json::json!({"action": "unmute"})).ok()?;
            Some((format!("service/macos/{}/command/mute", target), payload))
        }
        // Brightness / color temperature / power / seek intents have no macOS
        // counterpart — the dispatcher drops them via the `None` return.
        _ => None,
    }
}

/// Parse a state topic `service/macos/{target}/state/{property}` into its
/// `(target, property)` components. Returns `None` for topics that do not
/// match the schema; the caller should log and skip.
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
    use serde_json::json;

    fn payload_as_json(bytes: &[u8]) -> serde_json::Value {
        serde_json::from_slice(bytes).expect("payload is valid JSON")
    }

    // ---------------------- intent_to_mqtt ----------------------

    #[test]
    fn play_pause_has_empty_payload() {
        let (topic, payload) = intent_to_mqtt(&Intent::PlayPause, "zone-1").unwrap();
        assert_eq!(topic, "service/macos/zone-1/command/play_pause");
        assert!(payload.is_empty());
    }

    #[test]
    fn play_maps_to_play_topic() {
        let (topic, payload) = intent_to_mqtt(&Intent::Play, "zone-1").unwrap();
        assert_eq!(topic, "service/macos/zone-1/command/play");
        assert!(payload.is_empty());
    }

    #[test]
    fn pause_maps_to_pause_topic() {
        let (topic, payload) = intent_to_mqtt(&Intent::Pause, "host-a").unwrap();
        assert_eq!(topic, "service/macos/host-a/command/pause");
        assert!(payload.is_empty());
    }

    #[test]
    fn next_previous_stop_have_empty_payload() {
        for (intent, name) in [
            (Intent::Next, "next"),
            (Intent::Previous, "previous"),
            (Intent::Stop, "stop"),
        ] {
            let (topic, payload) = intent_to_mqtt(&intent, "z").unwrap();
            assert_eq!(topic, format!("service/macos/z/command/{}", name));
            assert!(payload.is_empty(), "{name} payload must be empty");
        }
    }

    #[test]
    fn volume_set_uses_absolute_how_and_int_percentage() {
        let (topic, payload) = intent_to_mqtt(&Intent::VolumeSet { value: 0.37 }, "z").unwrap();
        assert_eq!(topic, "service/macos/z/command/volume");
        assert_eq!(
            payload_as_json(&payload),
            json!({"how": "absolute", "value": 37})
        );
    }

    #[test]
    fn volume_set_clamps_over_range() {
        let (_topic, payload) = intent_to_mqtt(&Intent::VolumeSet { value: 1.5 }, "z").unwrap();
        assert_eq!(
            payload_as_json(&payload),
            json!({"how": "absolute", "value": 100})
        );
        let (_topic, payload) = intent_to_mqtt(&Intent::VolumeSet { value: -0.3 }, "z").unwrap();
        assert_eq!(
            payload_as_json(&payload),
            json!({"how": "absolute", "value": 0})
        );
    }

    #[test]
    fn volume_change_uses_relative_how_signed() {
        let (topic, payload) = intent_to_mqtt(&Intent::VolumeChange { delta: -0.05 }, "z").unwrap();
        assert_eq!(topic, "service/macos/z/command/volume");
        assert_eq!(
            payload_as_json(&payload),
            json!({"how": "relative", "value": -5})
        );
    }

    #[test]
    fn volume_change_clamps_both_directions() {
        let (_topic, payload) = intent_to_mqtt(&Intent::VolumeChange { delta: 5.0 }, "z").unwrap();
        assert_eq!(
            payload_as_json(&payload),
            json!({"how": "relative", "value": 100})
        );
        let (_topic, payload) = intent_to_mqtt(&Intent::VolumeChange { delta: -5.0 }, "z").unwrap();
        assert_eq!(
            payload_as_json(&payload),
            json!({"how": "relative", "value": -100})
        );
    }

    #[test]
    fn mute_emits_toggle_action() {
        let (topic, payload) = intent_to_mqtt(&Intent::Mute, "z").unwrap();
        assert_eq!(topic, "service/macos/z/command/mute");
        assert_eq!(payload_as_json(&payload), json!({"action": "toggle"}));
    }

    #[test]
    fn unmute_emits_unmute_action() {
        let (topic, payload) = intent_to_mqtt(&Intent::Unmute, "z").unwrap();
        assert_eq!(topic, "service/macos/z/command/mute");
        assert_eq!(payload_as_json(&payload), json!({"action": "unmute"}));
    }

    #[test]
    fn inapplicable_intents_return_none() {
        assert!(intent_to_mqtt(&Intent::PowerOn, "z").is_none());
        assert!(intent_to_mqtt(&Intent::PowerOff, "z").is_none());
        assert!(intent_to_mqtt(&Intent::PowerToggle, "z").is_none());
        assert!(intent_to_mqtt(&Intent::BrightnessSet { value: 50.0 }, "z").is_none());
        assert!(intent_to_mqtt(&Intent::BrightnessChange { delta: 0.1 }, "z").is_none());
        assert!(intent_to_mqtt(&Intent::ColorTemperatureChange { delta: 0.1 }, "z").is_none());
        assert!(intent_to_mqtt(&Intent::SeekRelative { seconds: 5.0 }, "z").is_none());
        assert!(intent_to_mqtt(&Intent::SeekAbsolute { seconds: 5.0 }, "z").is_none());
    }

    // ---------------------- parse_state_topic ----------------------

    #[test]
    fn parse_state_topic_accepts_canonical() {
        assert_eq!(
            parse_state_topic("service/macos/host-a/state/volume"),
            Some(("host-a".into(), "volume".into()))
        );
    }

    #[test]
    fn parse_state_topic_accepts_playback_active() {
        assert_eq!(
            parse_state_topic("service/macos/mac-studio/state/playback_active"),
            Some(("mac-studio".into(), "playback_active".into()))
        );
    }

    #[test]
    fn parse_state_topic_rejects_wrong_service() {
        assert!(parse_state_topic("service/roon/z/state/volume").is_none());
    }

    #[test]
    fn parse_state_topic_rejects_command_topic() {
        assert!(parse_state_topic("service/macos/z/command/play").is_none());
    }

    #[test]
    fn parse_state_topic_rejects_short_and_long_topics() {
        assert!(parse_state_topic("service/macos/z/state").is_none());
        assert!(parse_state_topic("service/macos/z/state/volume/extra").is_none());
        assert!(parse_state_topic("").is_none());
    }

    #[test]
    fn parse_state_topic_rejects_empty_segments() {
        assert!(parse_state_topic("service/macos//state/volume").is_none());
        assert!(parse_state_topic("service/macos/z/state/").is_none());
    }
}
