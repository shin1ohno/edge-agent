//! One-shot pairing flow against a Hue Bridge. Polls `POST /api` every 2s
//! until the user presses the link button (returns `success.username`) or
//! the timeout expires.

use std::time::Duration;

use serde::Deserialize;

use super::types::HueError;

const POLL_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Deserialize)]
struct PairEntry {
    #[serde(default)]
    success: Option<PairSuccess>,
    #[serde(default)]
    error: Option<HueError>,
}

#[derive(Debug, Clone, Deserialize)]
struct PairSuccess {
    username: String,
    #[serde(default)]
    clientkey: Option<String>,
}

/// Paired credentials returned by `pair()`. Persist as JSON alongside the
/// bridge host so the caller can reconnect later. `bridge_id` is fetched
/// via the unauthenticated `/api/config` endpoint immediately after
/// pairing so the token has a stable identifier for DHCP-resilient
/// reconnect later.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PairedCredentials {
    pub host: String,
    pub app_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bridge_id: Option<String>,
}

pub async fn pair(
    host: &str,
    device_type: &str,
    timeout: Duration,
) -> anyhow::Result<PairedCredentials> {
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .timeout(Duration::from_secs(5))
        .build()?;
    let url = format!("https://{}/api", host);
    let body = serde_json::json!({ "devicetype": device_type });

    let deadline = std::time::Instant::now() + timeout;
    let mut attempts = 0u32;
    loop {
        attempts += 1;
        let res = client.post(&url).json(&body).send().await;
        match res {
            Ok(res) => {
                let entries: Vec<PairEntry> = res.json().await?;
                if let Some(entry) = entries.into_iter().next() {
                    if let Some(s) = entry.success {
                        let bridge_id = super::api::fetch_bridge_config(
                            host,
                            Duration::from_secs(5),
                        )
                        .await
                        .map(|cfg| cfg.bridge_id.to_ascii_lowercase())
                        .map_err(|e| {
                            tracing::warn!(
                                error = %e,
                                host,
                                "paired but failed to fetch /api/config for bridge_id; token will be written without it",
                            );
                            e
                        })
                        .ok();
                        return Ok(PairedCredentials {
                            host: host.to_string(),
                            app_key: s.username,
                            client_key: s.clientkey,
                            bridge_id,
                        });
                    }
                    if let Some(e) = entry.error {
                        tracing::debug!(attempt = attempts, description = %e.description, "pair attempt rejected");
                    }
                }
            }
            Err(e) => {
                tracing::debug!(attempt = attempts, error = %e, "pair request failed");
            }
        }
        if std::time::Instant::now() >= deadline {
            anyhow::bail!(
                "timed out after {}s waiting for link button press ({} attempts)",
                timeout.as_secs(),
                attempts,
            );
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}
