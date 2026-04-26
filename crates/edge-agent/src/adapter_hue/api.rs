//! Low-level HTTPS client for the Hue Bridge v2 API.
//!
//! The bridge serves a self-signed cert — `danger_accept_invalid_certs(true)`
//! is the Philips-accepted pattern for LAN clients.

use std::time::Duration;

use reqwest::{Client, ClientBuilder};
use serde::Deserialize;

use std::collections::HashMap;

use super::types::{
    ButtonsResponse, DevicePowerResponse, DevicesResponse, Light, LightUpdate, LightsResponse,
};

/// Owner-index entry: which physical Tap Dial does this resource belong
/// to, and what role does it play. Built once at startup from
/// `list_tap_dials`, consulted by the SSE event loop to translate a
/// per-resource id back into a per-device input.
#[derive(Debug, Clone)]
pub enum TapDialResource {
    Button { device_id: String, control_id: u8 },
    Rotary { device_id: String },
    Power { device_id: String },
}

/// Enumerated Hue Tap Dial controller. Used to (a) prime the per-device
/// state pump on startup with `connected/nickname/battery`, and (b)
/// build the owner index that the SSE loop uses to dispatch button /
/// rotary / battery events to the correct device.
#[derive(Debug, Clone)]
pub struct TapDial {
    pub id: String,
    pub name: String,
    pub battery: Option<u8>,
}

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

    /// Enumerate every Hue Tap Dial controller paired to the bridge.
    /// Returns the per-device summary plus an owner index keyed by the
    /// resource ids that show up in SSE events (button/rotary/power),
    /// so the consumer can translate `event.id` back to a `device_id`
    /// without re-querying the bridge.
    ///
    /// Identification is by `product_data.product_name`; the bridge
    /// reports "Hue tap dial switch" verbatim. Other Hue controllers
    /// (Dimmer Switch, Smart Button) are skipped by design — they have
    /// different input topologies and aren't covered yet.
    pub async fn list_tap_dials(
        &self,
    ) -> anyhow::Result<(Vec<TapDial>, HashMap<String, TapDialResource>)> {
        let devices = self
            .get_json::<DevicesResponse>("/clip/v2/resource/device")
            .await?;
        let buttons = self
            .get_json::<ButtonsResponse>("/clip/v2/resource/button")
            .await?;
        let powers = self
            .get_json::<DevicePowerResponse>("/clip/v2/resource/device_power")
            .await?;

        // Index buttons + powers by their owner device id so each device
        // can pick out its own resources in one pass.
        let mut owner_index: HashMap<String, TapDialResource> = HashMap::new();
        let mut tap_dials = Vec::new();

        for dev in &devices.data {
            let is_tap_dial = dev
                .product_data
                .as_ref()
                .and_then(|p| p.product_name.as_deref())
                .is_some_and(|name| name.eq_ignore_ascii_case("Hue tap dial switch"));
            if !is_tap_dial {
                continue;
            }

            let name = dev
                .metadata
                .as_ref()
                .and_then(|m| m.name.clone())
                .unwrap_or_else(|| "Hue Tap Dial".to_string());

            // Index every button service this device owns. control_id
            // (1..=4) comes from /resource/button.metadata, NOT from the
            // device service list.
            for svc in &dev.services {
                match svc.rtype.as_str() {
                    "button" => {
                        if let Some(b) = buttons.data.iter().find(|b| b.id == svc.rid) {
                            if let Some(ctrl) = b.metadata.as_ref().and_then(|m| m.control_id) {
                                owner_index.insert(
                                    svc.rid.clone(),
                                    TapDialResource::Button {
                                        device_id: dev.id.clone(),
                                        control_id: ctrl,
                                    },
                                );
                            }
                        }
                    }
                    "relative_rotary" => {
                        owner_index.insert(
                            svc.rid.clone(),
                            TapDialResource::Rotary {
                                device_id: dev.id.clone(),
                            },
                        );
                    }
                    "device_power" => {
                        owner_index.insert(
                            svc.rid.clone(),
                            TapDialResource::Power {
                                device_id: dev.id.clone(),
                            },
                        );
                    }
                    _ => {}
                }
            }

            // Initial battery snapshot (SSE will refresh on changes).
            let battery = powers
                .data
                .iter()
                .find(|p| p.owner.as_ref().map(|o| o.rid == dev.id).unwrap_or(false))
                .and_then(|p| p.power_state.as_ref().map(|s| s.battery_level));

            tap_dials.push(TapDial {
                id: dev.id.clone(),
                name,
                battery,
            });
        }

        Ok((tap_dials, owner_index))
    }

    async fn get_json<T: serde::de::DeserializeOwned>(&self, path: &str) -> anyhow::Result<T> {
        let url = format!("https://{}{path}", self.host);
        let res = self
            .inner
            .get(&url)
            .header("hue-application-key", &self.app_key)
            .send()
            .await?
            .error_for_status()?
            .json::<T>()
            .await?;
        Ok(res)
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
