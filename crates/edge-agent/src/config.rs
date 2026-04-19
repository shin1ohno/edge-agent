//! Bootstrap config for edge-agent. Loaded from a TOML file, then overridden
//! by `EDGE_AGENT_*` environment variables.

use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub edge_id: String,
    pub config_server_url: String,
    #[serde(default)]
    pub roon: RoonSection,
    #[serde(default)]
    pub nuimo: NuimoSection,
}

#[derive(Debug, Deserialize, Default)]
pub struct RoonSection {
    pub host: Option<String>,
    pub port: Option<u16>,
    pub extension_id: Option<String>,
    pub display_name: Option<String>,
    pub publisher: Option<String>,
    pub email: Option<String>,
    pub token_path: Option<PathBuf>,
}

#[derive(Debug, Deserialize, Default)]
pub struct NuimoSection {
    pub ble_address: Option<String>,
    /// When true, skip BLE discovery/connection entirely. Useful for running
    /// an edge on a host that has no devices attached, to verify WS routing
    /// or to run as a dashboard-only witness for the weave hub.
    #[serde(default)]
    pub skip: bool,
}

impl Config {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("failed to read {}: {}", path.display(), e))?;
        let mut cfg: Self = toml::from_str(&text)?;
        cfg.apply_env_overrides();
        Ok(cfg)
    }

    fn apply_env_overrides(&mut self) {
        if let Ok(v) = std::env::var("EDGE_AGENT_EDGE_ID") {
            self.edge_id = v;
        }
        if let Ok(v) = std::env::var("EDGE_AGENT_CONFIG_SERVER_URL") {
            self.config_server_url = v;
        }
        if let Ok(v) = std::env::var("EDGE_AGENT_ROON_HOST") {
            self.roon.host = Some(v);
        }
        if let Ok(v) = std::env::var("EDGE_AGENT_ROON_PORT") {
            if let Ok(p) = v.parse() {
                self.roon.port = Some(p);
            }
        }
        if let Ok(v) = std::env::var("EDGE_AGENT_NUIMO_BLE_ADDRESS") {
            self.nuimo.ble_address = Some(v);
        }
        if let Ok(v) = std::env::var("EDGE_AGENT_NUIMO_SKIP") {
            self.nuimo.skip = matches!(v.as_str(), "1" | "true" | "yes");
        }
    }
}
