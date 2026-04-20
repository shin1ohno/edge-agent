//! Philips N-UPnP cloud discovery (`https://discovery.meethue.com/`).
//!
//! Returns JSON `[{"id": "...", "internalipaddress": "..."}]`. We keep
//! only the IP — the `id` is surfaced for reference logging.

use serde::Deserialize;

const DISCOVERY_URL: &str = "https://discovery.meethue.com/";

#[derive(Debug, Clone, Deserialize)]
pub struct DiscoveredBridge {
    pub id: String,
    #[serde(rename = "internalipaddress")]
    pub host: String,
}

pub async fn discover() -> anyhow::Result<Vec<DiscoveredBridge>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    let res = client
        .get(DISCOVERY_URL)
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<DiscoveredBridge>>()
        .await?;
    Ok(res)
}
