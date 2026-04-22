//! Low-level HTTPS client for the Hue Bridge v2 API.
//!
//! The bridge serves a self-signed cert — `danger_accept_invalid_certs(true)`
//! is the Philips-accepted pattern for LAN clients.

use std::time::Duration;

use reqwest::{Client, ClientBuilder};
use serde::Deserialize;

use super::types::{Light, LightUpdate, LightsResponse};

// Applies to REST calls *and* the SSE bytes stream (shared `Client`). The
// Hue bridge emits SSE keepalive comments every ~9s; at 10s the stream was
// timing out within one keepalive window on any brief network delay,
// producing a perpetual 10s connect → "error decoding response body" →
// reconnect loop. 20s gives ~2x the keepalive interval of slack while
// still surfacing a genuinely dead connection in a bounded time.
const TIMEOUT_SECS: u64 = 20;

/// Minimal subset of the legacy `GET /api/config` response. Unauthenticated
/// (the Hue bridge exposes `bridgeid` and a few other identifying fields
/// to any LAN peer so clients can confirm which bridge they're talking to
/// before pairing).
#[derive(Debug, Clone, Deserialize)]
pub struct BridgeConfig {
    #[serde(alias = "bridgeid")]
    pub bridge_id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub mac: Option<String>,
}

/// Fetch `/api/config` on `host` with a bounded timeout. Used as a
/// reachability probe at startup and to learn the `bridgeid` for tokens
/// written by older versions. Does not require an app key.
pub async fn fetch_bridge_config(host: &str, timeout: Duration) -> anyhow::Result<BridgeConfig> {
    let client = ClientBuilder::new()
        .danger_accept_invalid_certs(true)
        .timeout(timeout)
        .build()?;
    let url = format!("https://{host}/api/config");
    let res = client
        .get(&url)
        .send()
        .await?
        .error_for_status()?
        .json::<BridgeConfig>()
        .await?;
    Ok(res)
}

#[derive(Clone)]
pub struct HueClient {
    inner: Client,
    host: String,
    app_key: String,
}

impl HueClient {
    pub fn new(host: impl Into<String>, app_key: impl Into<String>) -> anyhow::Result<Self> {
        let inner = ClientBuilder::new()
            .danger_accept_invalid_certs(true)
            .timeout(std::time::Duration::from_secs(TIMEOUT_SECS))
            .build()?;
        Ok(Self {
            inner,
            host: host.into(),
            app_key: app_key.into(),
        })
    }

    pub fn http(&self) -> &Client {
        &self.inner
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn app_key(&self) -> &str {
        &self.app_key
    }

    pub async fn list_lights(&self) -> anyhow::Result<Vec<Light>> {
        let url = format!("https://{}/clip/v2/resource/light", self.host);
        let res = self
            .inner
            .get(&url)
            .header("hue-application-key", &self.app_key)
            .send()
            .await?
            .error_for_status()?
            .json::<LightsResponse>()
            .await?;
        if !res.errors.is_empty() {
            tracing::warn!(
                errors = ?res.errors.iter().map(|e| &e.description).collect::<Vec<_>>(),
                "hue list_lights returned errors",
            );
        }
        Ok(res.data)
    }

    pub async fn put_light(&self, light_id: &str, update: &LightUpdate) -> anyhow::Result<()> {
        let url = format!("https://{}/clip/v2/resource/light/{}", self.host, light_id);
        let res = self
            .inner
            .put(&url)
            .header("hue-application-key", &self.app_key)
            .json(update)
            .send()
            .await?;
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        if !status.is_success() {
            anyhow::bail!("PUT light {} failed: {} {}", light_id, status, body);
        }
        Ok(())
    }
}
