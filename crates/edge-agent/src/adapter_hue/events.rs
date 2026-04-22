//! SSE event stream consumer for Hue v2 bridges.
//!
//! Subscribes to `https://<bridge>/eventstream/clip/v2` and updates the
//! shared `LightCache` + broadcasts `StateUpdate`s for consumers. Handles
//! reconnection with a short backoff.

use std::sync::Arc;
use std::time::Duration;

use crate::edge_core::StateUpdate;
use futures_util::StreamExt;
use tokio::sync::broadcast;

use super::api::HueClient;
use super::cache::LightCache;
use super::types::{Light, SseEvent};

pub const SERVICE_TYPE: &str = "hue";

const RECONNECT_DELAY: Duration = Duration::from_secs(5);

pub async fn run(
    client: HueClient,
    cache: Arc<LightCache>,
    state_tx: broadcast::Sender<StateUpdate>,
) {
    loop {
        if let Err(e) = stream_once(&client, &cache, &state_tx).await {
            tracing::warn!(error = %e, "hue event stream ended; reconnecting");
        }
        tokio::time::sleep(RECONNECT_DELAY).await;
    }
}

async fn stream_once(
    client: &HueClient,
    cache: &Arc<LightCache>,
    state_tx: &broadcast::Sender<StateUpdate>,
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
                        if d.resource_type != "light" {
                            continue;
                        }
                        apply_event(&d.id, &d, cache, state_tx).await;
                    }
                }
            }
        }
    }
    Ok(())
}

async fn apply_event(
    light_id: &str,
    data: &super::types::SseEventData,
    cache: &Arc<LightCache>,
    state_tx: &broadcast::Sender<StateUpdate>,
) {
    let updated = cache.merge_partial(light_id, data.on, data.dimming).await;
    if let Some(light) = updated {
        broadcast_light(&light, state_tx);
    }
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
