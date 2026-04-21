//! Persistence for the Hue bridge pairing credentials written by
//! `edge-agent pair-hue`.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// `$XDG_STATE_HOME/edge-agent/hue-token.json` (falls back to `$HOME/.local/state`).
pub fn default_path() -> PathBuf {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("state")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("edge-agent").join("hue-token.json")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HueToken {
    pub host: String,
    pub app_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_key: Option<String>,
}

pub fn load(path: &Path) -> anyhow::Result<HueToken> {
    let text =
        fs::read_to_string(path).map_err(|e| anyhow::anyhow!("read {}: {}", path.display(), e))?;
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
