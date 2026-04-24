//! MQTT client lifecycle for the macOS adapter.
//!
//! Responsibilities:
//! * Build the `rumqttc::AsyncClient` + `EventLoop` pair.
//! * Subscribe to `service/macos/+/state/+` on every connect (rumqttc
//!   drops subscriptions across reconnects, so we re-subscribe on the
//!   `ConnAck` event).
//! * Translate inbound state publishes into `StateUpdate`s via
//!   [`super::types::parse_state_topic`] and broadcast them.

use edge_core::StateUpdate;
use rumqttc::{AsyncClient, Event, EventLoop, Incoming, MqttOptions, QoS};
use tokio::sync::broadcast;

use super::types::parse_state_topic;

pub const STATE_WILDCARD: &str = "service/macos/+/state/+";

/// Build an `AsyncClient` + `EventLoop` pair with sensible defaults for
/// the macOS adapter. Keep-alive is 30s to match the Roon hub pattern.
pub fn build_client(host: &str, port: u16, client_id: &str) -> (AsyncClient, EventLoop) {
    let mut opts = MqttOptions::new(client_id, host, port);
    opts.set_keep_alive(std::time::Duration::from_secs(30));
    opts.set_clean_session(true);
    AsyncClient::new(opts, 64)
}

/// Event-loop pump. Runs forever: on `ConnAck`, re-subscribe to the state
/// wildcard; on `Publish`, parse + broadcast; on error, back off 5s before
/// the next `poll` attempt (rumqttc reconnects internally).
pub async fn run_event_loop(
    client: AsyncClient,
    mut event_loop: EventLoop,
    state_tx: broadcast::Sender<StateUpdate>,
) {
    loop {
        match event_loop.poll().await {
            Ok(Event::Incoming(Incoming::ConnAck(_))) => {
                if let Err(e) = client.subscribe(STATE_WILDCARD, QoS::AtLeastOnce).await {
                    tracing::warn!(error = %e, "macos mqtt resubscribe failed");
                } else {
                    tracing::info!(topic = STATE_WILDCARD, "macos mqtt subscribed");
                }
            }
            Ok(Event::Incoming(Incoming::Publish(publish))) => {
                handle_publish(&publish.topic, &publish.payload, &state_tx);
            }
            Ok(_) => {}
            Err(e) => {
                tracing::warn!(error = %e, "macos mqtt event loop error; backing off");
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
        }
    }
}

/// Translate a single MQTT state publish into a `StateUpdate` and broadcast
/// it. Exposed separately so unit tests can exercise the translation
/// without driving the event loop.
pub fn handle_publish(topic: &str, payload: &[u8], state_tx: &broadcast::Sender<StateUpdate>) {
    let Some((target, property)) = parse_state_topic(topic) else {
        tracing::debug!(%topic, "unrecognised macos mqtt topic; skipping");
        return;
    };
    let value: serde_json::Value = match serde_json::from_slice(payload) {
        Ok(v) => v,
        Err(e) => {
            tracing::debug!(error = %e, %topic, "macos mqtt payload is not JSON; skipping");
            return;
        }
    };
    let update = StateUpdate {
        service_type: super::adapter::SERVICE_TYPE.to_string(),
        target,
        property,
        output_id: None,
        value,
    };
    match state_tx.send(update) {
        Ok(n) => tracing::debug!(%topic, delivered_to = n, "macos state forwarded"),
        Err(_) => {
            tracing::warn!(%topic, "macos state dropped — no active receivers on broadcast channel")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn drain(rx: &mut broadcast::Receiver<StateUpdate>) -> Vec<StateUpdate> {
        let mut out = Vec::new();
        while let Ok(u) = rx.try_recv() {
            out.push(u);
        }
        out
    }

    #[test]
    fn handle_publish_bool_playback_active() {
        let (tx, mut rx) = broadcast::channel(16);
        handle_publish("service/macos/host-a/state/playback_active", b"true", &tx);
        let got = drain(&mut rx);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].service_type, "macos");
        assert_eq!(got[0].target, "host-a");
        assert_eq!(got[0].property, "playback_active");
        assert_eq!(got[0].output_id, None);
        assert_eq!(got[0].value, json!(true));
    }

    #[test]
    fn handle_publish_int_volume() {
        let (tx, mut rx) = broadcast::channel(16);
        handle_publish("service/macos/host-a/state/volume", b"42", &tx);
        let got = drain(&mut rx);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].property, "volume");
        assert_eq!(got[0].value, json!(42));
    }

    #[test]
    fn handle_publish_output_device_object() {
        let (tx, mut rx) = broadcast::channel(16);
        let payload = br#"{"uid":"uid-1","name":"Living Room"}"#;
        handle_publish("service/macos/host-a/state/output_device", payload, &tx);
        let got = drain(&mut rx);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].property, "output_device");
        assert_eq!(got[0].value, json!({"uid": "uid-1", "name": "Living Room"}));
    }

    #[test]
    fn handle_publish_available_outputs_array() {
        let (tx, mut rx) = broadcast::channel(16);
        let payload = br#"[{"uid":"a","name":"A","transport":"airplay","is_airplay":true}]"#;
        handle_publish("service/macos/host-a/state/available_outputs", payload, &tx);
        let got = drain(&mut rx);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].property, "available_outputs");
        let arr = got[0].value.as_array().expect("array");
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["uid"], "a");
    }

    #[test]
    fn handle_publish_rejects_bad_topic() {
        let (tx, mut rx) = broadcast::channel(16);
        handle_publish("service/macos/bad", b"1", &tx);
        assert!(drain(&mut rx).is_empty());
    }

    #[test]
    fn handle_publish_rejects_non_json_payload() {
        let (tx, mut rx) = broadcast::channel(16);
        handle_publish("service/macos/z/state/volume", b"not json{", &tx);
        assert!(drain(&mut rx).is_empty());
    }
}
