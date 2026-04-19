//! Low-level HTTPS client for the Hue Bridge v2 API.
//!
//! The bridge serves a self-signed cert — `danger_accept_invalid_certs(true)`
//! is the Philips-accepted pattern for LAN clients.

use reqwest::{Client, ClientBuilder};

use crate::types::{Light, LightUpdate, LightsResponse};

const TIMEOUT_SECS: u64 = 10;

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
