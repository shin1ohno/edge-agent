//! Persistence for the Hue bridge pairing credentials written by
//! `edge-agent pair-hue`.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HueToken {
    pub host: String,
    pub app_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_key: Option<String>,
}

pub fn load(path: &Path) -> anyhow::Result<HueToken> {
    let text = fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("read {}: {}", path.display(), e))?;
    Ok(serde_json::from_str(&text)?)
}

pub fn save(path: &Path, token: &HueToken) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_string_pretty(token)?)?;
    fs::rename(&tmp, path)?;
    Ok(())
}
