//! SSE event stream consumer for Hue v2 bridges.
//!
//! Subscribes to `https://<bridge>/eventstream/clip/v2` and dispatches
//! `update` events by `resource_type`:
//! * `light` — refresh `LightCache` and broadcast `StateUpdate`s
//! * `button` — translate to `InputPrimitive::Button { id }` for the
//!   owner Tap Dial device
//! * `relative_rotary` — translate to `InputPrimitive::Rotate { delta }`
//! * `device_power` — surface the new battery level to the device-state
//!   pump
//!
//! Reconnects with a short backoff on stream errors. The Tap Dial owner
//! index is built once at adapter startup; SSE events carry resource ids
//! that this loop reverse-looks-up to a (device_id, role) so the
//! consumer never has to call back into the bridge.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use edge_core::{InputPrimitive, StateUpdate};
use futures_util::StreamExt;
use tokio::sync::broadcast;

use super::adapter::TapDialEvent;
use super::api::{HueClient, TapDialResource};
use super::cache::LightCache;
use super::types::{Light, SseEvent, SseEventData};

pub const SERVICE_TYPE: &str = "hue";

const RECONNECT_DELAY: Duration = Duration::from_secs(5);

/// Empirical normalisation: the Tap Dial reports rotation in `steps`
/// (≈ 1 step per detent click) and Nuimo `Rotate { delta }` is roughly
/// "fraction of a full revolution per event". Dividing by 24 puts a
/// full-knob rotation (~24 steps) at delta=1.0, which lines up with
/// existing volume_change damping (80) → 80% volume per turn.
const STEPS_PER_FULL_TURN: f64 = 24.0;

pub async fn run(
    client: HueClient,
    cache: Arc<LightCache>,
    state_tx: broadcast::Sender<StateUpdate>,
    owner_index: Arc<HashMap<String, TapDialResource>>,
    tap_dial_tx: broadcast::Sender<TapDialEvent>,
) {
    loop {
        if let Err(e) = stream_once(&client, &cache, &state_tx, &owner_index, &tap_dial_tx).await {
            tracing::warn!(error = %e, "hue event stream ended; reconnecting");
        }
        tokio::time::sleep(RECONNECT_DELAY).await;
    }
}

async fn stream_once(
    client: &HueClient,
    cache: &Arc<LightCache>,
    state_tx: &broadcast::Sender<StateUpdate>,
    owner_index: &Arc<HashMap<String, TapDialResource>>,
    tap_dial_tx: &broadcast::Sender<TapDialEvent>,
) -> anyhow::Result<()> {
    let url = format!("https://{}/eventstream/clip/v2", client.host());
    let res = client
        .http()
        .get(&url)
        .header("hue-application-key", client.app_key())
        .header("Accept", "text/event-stream")
        .send()
        .await?
        .error_for_status()?;

    tracing::info!(%url, "hue SSE connected");

    // Replay the cached snapshot now that state_pump subscribers are
    // attached. `HueAdapter::start` also broadcasts each light right after
    // `list_lights`, but that fires before any subscriber exists — the
    // broadcast channel silently drops those messages. Without this replay,
    // a light whose state never changes after edge-agent start (e.g. a bulb
    // that stays `off`) would never reach weave-server, and the mapping UI
    // couldn't list it as a target. Also handles the SSE-reconnect case
    // after a bridge reboot.
    for light in cache.values().await {
        broadcast_light(&light, state_tx);
    }

    let mut stream = res.bytes_stream();
    let mut buf = String::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        buf.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(idx) = buf.find("\n\n") {
            let block: String = buf.drain(..idx + 2).collect();
            for line in block.lines() {
                let Some(payload) = line.strip_prefix("data: ") else {
                    continue;
                };
                let events: Vec<SseEvent> = match serde_json::from_str(payload) {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::debug!(error = %e, payload, "skipping unparsable SSE");
                        continue;
                    }
                };
                for event in events {
                    if event.kind != "update" {
                        continue;
                    }
                    for d in event.data {
                        dispatch_event(&d, cache, state_tx, owner_index, tap_dial_tx).await;
                    }
                }
            }
        }
    }
    Ok(())
}

async fn dispatch_event(
    d: &SseEventData,
    cache: &Arc<LightCache>,
    state_tx: &broadcast::Sender<StateUpdate>,
    owner_index: &Arc<HashMap<String, TapDialResource>>,
    tap_dial_tx: &broadcast::Sender<TapDialEvent>,
) {
    match d.resource_type.as_str() {
        "light" => apply_light_event(&d.id, d, cache, state_tx).await,
        "button" => apply_button_event(d, owner_index, tap_dial_tx),
        "relative_rotary" => apply_rotary_event(d, owner_index, tap_dial_tx),
        "device_power" => apply_power_event(d, owner_index, tap_dial_tx),
        _ => {}
    }
}

async fn apply_light_event(
    light_id: &str,
    data: &SseEventData,
    cache: &Arc<LightCache>,
    state_tx: &broadcast::Sender<StateUpdate>,
) {
    let updated = cache.merge_partial(light_id, data.on, data.dimming).await;
    if let Some(light) = updated {
        broadcast_light(&light, state_tx);
    }
}

/// Translate one Hue button event to `Button { id }`. Only the
/// `short_release` event (single tap) is forwarded today — `long_press`
/// and `long_release` will need a separate `InputPrimitive` variant
/// before they can route, and `repeat` produces too many duplicates.
fn apply_button_event(
    d: &SseEventData,
    owner_index: &Arc<HashMap<String, TapDialResource>>,
    tap_dial_tx: &broadcast::Sender<TapDialEvent>,
) {
    let event_kind = d
        .button
        .as_ref()
        .and_then(|b| b.button_report.as_ref().map(|r| r.event.as_str()));
    if event_kind != Some("short_release") {
        return;
    }
    let Some(TapDialResource::Button {
        device_id,
        control_id,
    }) = owner_index.get(&d.id)
    else {
        tracing::debug!(resource_id = %d.id, "button event for unknown resource; skipping");
        return;
    };
    let _ = tap_dial_tx.send(TapDialEvent::Input {
        device_id: device_id.clone(),
        primitive: InputPrimitive::Button { id: *control_id },
    });
}

/// Translate one rotary event to `Rotate { delta }`. The bridge reports
/// `steps` (signed by `direction`) — convert to a Nuimo-equivalent
/// fractional delta so existing `damping` parameters keep their meaning.
fn apply_rotary_event(
    d: &SseEventData,
    owner_index: &Arc<HashMap<String, TapDialResource>>,
    tap_dial_tx: &broadcast::Sender<TapDialEvent>,
) {
    let Some(rotary) = d.relative_rotary.as_ref() else {
        return;
    };
    let Some(last) = rotary.last_event.as_ref() else {
        return;
    };
    let signed_steps = match last.rotation.direction.as_str() {
        "clock_wise" => last.rotation.steps as f64,
        "counter_clock_wise" => -(last.rotation.steps as f64),
        _ => return,
    };
    let delta = signed_steps / STEPS_PER_FULL_TURN;
    let Some(TapDialResource::Rotary { device_id }) = owner_index.get(&d.id) else {
        tracing::debug!(resource_id = %d.id, "rotary event for unknown resource; skipping");
        return;
    };
    let _ = tap_dial_tx.send(TapDialEvent::Input {
        device_id: device_id.clone(),
        primitive: InputPrimitive::Rotate { delta },
    });
}

fn apply_power_event(
    d: &SseEventData,
    owner_index: &Arc<HashMap<String, TapDialResource>>,
    tap_dial_tx: &broadcast::Sender<TapDialEvent>,
) {
    let Some(power) = d.power_state.as_ref() else {
        return;
    };
    let Some(TapDialResource::Power { device_id }) = owner_index.get(&d.id) else {
        tracing::debug!(resource_id = %d.id, "power event for unknown resource; skipping");
        return;
    };
    let _ = tap_dial_tx.send(TapDialEvent::Battery {
        device_id: device_id.clone(),
        level: power.battery_level,
    });
}

pub fn broadcast_light(light: &Light, state_tx: &broadcast::Sender<StateUpdate>) {
    let target = light.id.clone();

    let _ = state_tx.send(StateUpdate {
        service_type: SERVICE_TYPE.into(),
        target: target.clone(),
        property: "light".into(),
        output_id: None,
        value: serde_json::json!({
            "display_name": light.metadata.name,
            "on": light.on.on,
            "brightness": light.dimming.map(|d| d.brightness),
        }),
    });
    let _ = state_tx.send(StateUpdate {
        service_type: SERVICE_TYPE.into(),
        target: target.clone(),
        property: "on".into(),
        output_id: None,
        value: serde_json::Value::Bool(light.on.on),
    });
    if let Some(d) = light.dimming {
        let _ = state_tx.send(StateUpdate {
            service_type: SERVICE_TYPE.into(),
            target,
            property: "brightness".into(),
            output_id: None,
            value: serde_json::json!(d.brightness),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn owner_index() -> Arc<HashMap<String, TapDialResource>> {
        let mut m = HashMap::new();
        m.insert(
            "btn-1".into(),
            TapDialResource::Button {
                device_id: "dev-A".into(),
                control_id: 1,
            },
        );
        m.insert(
            "btn-3".into(),
            TapDialResource::Button {
                device_id: "dev-A".into(),
                control_id: 3,
            },
        );
        m.insert(
            "rot".into(),
            TapDialResource::Rotary {
                device_id: "dev-A".into(),
            },
        );
        m.insert(
            "pwr".into(),
            TapDialResource::Power {
                device_id: "dev-A".into(),
            },
        );
        Arc::new(m)
    }

    fn parse_data(payload: serde_json::Value) -> SseEventData {
        serde_json::from_value(payload).expect("test payload should deserialize")
    }

    #[test]
    fn button_short_release_emits_button_input() {
        let (tx, mut rx) = broadcast::channel::<TapDialEvent>(8);
        let d = parse_data(json!({
            "id": "btn-1",
            "type": "button",
            "button": { "button_report": { "event": "short_release" } },
        }));
        apply_button_event(&d, &owner_index(), &tx);
        let evt = rx.try_recv().expect("event should be sent");
        match evt {
            TapDialEvent::Input {
                device_id,
                primitive,
            } => {
                assert_eq!(device_id, "dev-A");
                assert_eq!(primitive, InputPrimitive::Button { id: 1 });
            }
            other => panic!("expected Input, got {other:?}"),
        }
    }

    #[test]
    fn button_long_press_is_ignored() {
        let (tx, mut rx) = broadcast::channel::<TapDialEvent>(8);
        let d = parse_data(json!({
            "id": "btn-1",
            "type": "button",
            "button": { "button_report": { "event": "long_press" } },
        }));
        apply_button_event(&d, &owner_index(), &tx);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn rotary_clockwise_produces_positive_delta() {
        let (tx, mut rx) = broadcast::channel::<TapDialEvent>(8);
        let d = parse_data(json!({
            "id": "rot",
            "type": "relative_rotary",
            "relative_rotary": {
                "last_event": {
                    "action": "start",
                    "rotation": { "direction": "clock_wise", "steps": 6 }
                }
            }
        }));
        apply_rotary_event(&d, &owner_index(), &tx);
        let evt = rx.try_recv().expect("rotation event");
        match evt {
            TapDialEvent::Input {
                device_id,
                primitive: InputPrimitive::Rotate { delta },
            } => {
                assert_eq!(device_id, "dev-A");
                assert!((delta - 6.0 / STEPS_PER_FULL_TURN).abs() < 1e-9);
            }
            other => panic!("expected Rotate, got {other:?}"),
        }
    }

    #[test]
    fn rotary_counter_clockwise_inverts_sign() {
        let (tx, mut rx) = broadcast::channel::<TapDialEvent>(8);
        let d = parse_data(json!({
            "id": "rot",
            "type": "relative_rotary",
            "relative_rotary": {
                "last_event": {
                    "action": "repeat",
                    "rotation": { "direction": "counter_clock_wise", "steps": 2 }
                }
            }
        }));
        apply_rotary_event(&d, &owner_index(), &tx);
        let evt = rx.try_recv().unwrap();
        match evt {
            TapDialEvent::Input {
                primitive: InputPrimitive::Rotate { delta },
                ..
            } => assert!(delta < 0.0),
            other => panic!("expected negative Rotate, got {other:?}"),
        }
    }

    #[test]
    fn power_event_emits_battery() {
        let (tx, mut rx) = broadcast::channel::<TapDialEvent>(8);
        let d = parse_data(json!({
            "id": "pwr",
            "type": "device_power",
            "power_state": { "battery_level": 42 }
        }));
        apply_power_event(&d, &owner_index(), &tx);
        match rx.try_recv().unwrap() {
            TapDialEvent::Battery { device_id, level } => {
                assert_eq!(device_id, "dev-A");
                assert_eq!(level, 42);
            }
            other => panic!("expected Battery, got {other:?}"),
        }
    }

    #[test]
    fn unknown_resource_id_is_skipped() {
        let (tx, mut rx) = broadcast::channel::<TapDialEvent>(8);
        let d = parse_data(json!({
            "id": "ghost-id",
            "type": "button",
            "button": { "button_report": { "event": "short_release" } }
        }));
        apply_button_event(&d, &owner_index(), &tx);
        assert!(rx.try_recv().is_err());
    }
}
