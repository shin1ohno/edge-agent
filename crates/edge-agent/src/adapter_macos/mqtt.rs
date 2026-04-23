//! MQTT client lifecycle for the macOS adapter.
//!
//! Owns the `rumqttc` `EventLoop` poll task. Incoming `service/macos/+/state/+`
//! publishes are parsed via [`super::types::parse_state_topic`] and fanned
//! out as [`StateUpdate`]s on the adapter's broadcast channel.
//!
//! `rumqttc` handles reconnect internally (`poll()` returns an error when
//! the session drops, we log it, sleep briefly, and keep polling). No
//! manual reconnect loop is needed, but subscriptions have to be re-issued
//! on each `ConnAck` — otherwise a reconnected broker would not resume
//! forwarding state publishes to us.

use std::time::Duration;

use edge_core::StateUpdate;
use rumqttc::{AsyncClient, Event, EventLoop, Incoming, MqttOptions, Packet, QoS};
use tokio::sync::broadcast;

use super::types::parse_state_topic;

/// Topic filter subscribed on every connect. `+` matches any single level so
/// we pick up every edge_id × property combination the hub publishes.
pub(crate) const STATE_TOPIC_FILTER: &str = "service/macos/+/state/+";

/// Service identifier baked into every emitted `StateUpdate`. Kept here
/// alongside the topic filter so any future rename of the service stays
/// grouped with its wire-format dependencies.
pub(crate) const SERVICE_TYPE: &str = "macos";

/// Backoff applied after a poll error so a broker outage doesn't spin the
/// event loop. Deliberately short because `rumqttc` already paces its own
/// reconnect attempts; this just softens the log-spam in pathological cases.
const POLL_ERROR_BACKOFF: Duration = Duration::from_secs(2);

pub(crate) struct MqttConn {
    pub client: AsyncClient,
    pub event_loop: EventLoop,
}

/// Connect to the broker. Returns the handle + the raw event loop so the
/// caller can own the poll task lifetime (spawned from `MacosAdapter::start`).
pub(crate) fn connect(host: &str, port: u16, client_id: &str) -> MqttConn {
    let mut opts = MqttOptions::new(client_id, host, port);
    opts.set_keep_alive(Duration::from_secs(30));
    // Cap in-flight state publishes from the hub — prevents a burst of
    // `available_outputs` frames on broker restart from overflowing the
    // internal buffer.
    opts.set_max_packet_size(256 * 1024, 256 * 1024);

    let (client, event_loop) = AsyncClient::new(opts, 64);
    MqttConn { client, event_loop }
}

/// Run the MQTT event loop forever. On every (re)connect we re-subscribe to
/// the state topic filter so a broker restart doesn't silently leave us
/// listening to nothing. Publishes matching the filter are parsed and fanned
/// out to `state_tx`.
pub(crate) async fn run_event_loop(
    client: AsyncClient,
    mut event_loop: EventLoop,
    state_tx: broadcast::Sender<StateUpdate>,
) {
    loop {
        match event_loop.poll().await {
            Ok(Event::Incoming(Incoming::ConnAck(_))) => {
                // Re-subscribe on every new session. `clean_session` is
                // defaulted true by rumqttc, so any prior subscription is
                // lost across a disconnect — must re-issue explicitly.
                match client.subscribe(STATE_TOPIC_FILTER, QoS::AtLeastOnce).await {
                    Ok(()) => {
                        tracing::info!(filter = STATE_TOPIC_FILTER, "macos adapter subscribed")
                    }
                    Err(e) => tracing::warn!(
                        error = %e,
                        filter = STATE_TOPIC_FILTER,
                        "macos adapter subscribe failed",
                    ),
                }
            }
            Ok(Event::Incoming(Packet::Publish(msg))) => {
                handle_incoming_publish(&msg.topic, &msg.payload, &state_tx);
            }
            Ok(_) => {
                // Outgoing acks, pings, and other bookkeeping — nothing to do.
            }
            Err(e) => {
                // rumqttc surfaces every reconnect-triggering failure here
                // (broker down, DNS failure, TLS handshake errors, keepalive
                // timeout). The library then sleeps internally before the
                // next `poll()` reconnect attempt; the extra backoff below
                // mainly keeps the log readable when the broker is down
                // for an extended period.
                tracing::warn!(error = %e, "macos adapter MQTT loop error; will retry");
                tokio::time::sleep(POLL_ERROR_BACKOFF).await;
            }
        }
    }
}

fn handle_incoming_publish(topic: &str, payload: &[u8], state_tx: &broadcast::Sender<StateUpdate>) {
    let Some((edge_id, property)) = parse_state_topic(topic) else {
        tracing::debug!(topic, "ignoring non-state macos publish");
        return;
    };

    // Tolerate empty payloads — macos-hub uses them as explicit "unknown"
    // signals. `serde_json::from_slice` returns `null` for empty input only
    // when the payload is literally the 4-byte `null` string; an empty
    // byte slice errors out, which we treat as `null`.
    let value: serde_json::Value = if payload.is_empty() {
        serde_json::Value::Null
    } else {
        match serde_json::from_slice(payload) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    topic,
                    property,
                    "skipping unparsable macos state payload",
                );
                return;
            }
        }
    };

    let _ = state_tx.send(StateUpdate {
        service_type: SERVICE_TYPE.to_string(),
        target: edge_id,
        property,
        output_id: None,
        value,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handle_incoming_publish_forwards_valid_state() {
        let (tx, mut rx) = broadcast::channel::<StateUpdate>(4);
        handle_incoming_publish("service/macos/studio/state/volume", b"42", &tx);
        let update = rx.try_recv().expect("update enqueued");
        assert_eq!(update.service_type, "macos");
        assert_eq!(update.target, "studio");
        assert_eq!(update.property, "volume");
        assert_eq!(update.value, serde_json::json!(42));
    }

    #[test]
    fn handle_incoming_publish_treats_empty_payload_as_null() {
        let (tx, mut rx) = broadcast::channel::<StateUpdate>(4);
        handle_incoming_publish("service/macos/studio/state/playback_active", b"", &tx);
        let update = rx.try_recv().expect("update enqueued");
        assert_eq!(update.value, serde_json::Value::Null);
    }

    #[test]
    fn handle_incoming_publish_drops_unparsable_payload() {
        let (tx, mut rx) = broadcast::channel::<StateUpdate>(4);
        handle_incoming_publish("service/macos/studio/state/volume", b"not-json{", &tx);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn handle_incoming_publish_drops_foreign_topic() {
        let (tx, mut rx) = broadcast::channel::<StateUpdate>(4);
        handle_incoming_publish("service/hue/light/state/on", b"true", &tx);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn handle_incoming_publish_parses_object_values() {
        let (tx, mut rx) = broadcast::channel::<StateUpdate>(4);
        handle_incoming_publish(
            "service/macos/studio/state/output_device",
            br#"{"uid":"BuiltInSpeakerDevice","name":"MacBook Pro Speakers"}"#,
            &tx,
        );
        let update = rx.try_recv().expect("update enqueued");
        assert_eq!(update.property, "output_device");
        assert_eq!(
            update.value,
            serde_json::json!({"uid":"BuiltInSpeakerDevice","name":"MacBook Pro Speakers"}),
        );
    }
}
