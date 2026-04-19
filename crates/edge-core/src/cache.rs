//! Local file cache for the last-received `EdgeConfig`. When `weave-server`
//! is unreachable on startup, the agent loads this file so local operation
//! can continue with the previous mapping set.

use std::path::{Path, PathBuf};

use weave_contracts::EdgeConfig;

/// Resolve the cache path for an edge: `${XDG_STATE_HOME:-~/.local/state}/edge-agent/config-cache-${edge_id}.json`.
pub fn default_cache_path(edge_id: &str) -> PathBuf {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("state"))
        })
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("edge-agent")
        .join(format!("config-cache-{}.json", edge_id))
}

pub async fn load(path: &Path) -> anyhow::Result<Option<EdgeConfig>> {
    match tokio::fs::read_to_string(path).await {
        Ok(s) => Ok(Some(serde_json::from_str(&s)?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub async fn save(path: &Path, config: &EdgeConfig) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let json = serde_json::to_string_pretty(config)?;
    tokio::fs::write(path, json).await?;
    Ok(())
}
