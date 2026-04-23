use std::path::Path;

use serde::Deserialize;

/// Hub configuration loaded from TOML.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    pub mqtt: MqttConfig,
    pub macos: MacosConfig,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct MqttConfig {
    pub host: String,
    pub port: u16,
    pub client_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct MacosConfig {
    /// Target segment used in `service/macos/{edge_id}/...` topics.
    pub edge_id: String,
    /// Seconds between periodic re-publish of volume / output device.
    pub periodic_publish_interval_secs: u64,
}

impl Default for MqttConfig {
    fn default() -> Self {
        Self {
            host: "localhost".into(),
            port: 1883,
            client_id: "macos-hub".into(),
        }
    }
}

impl Default for MacosConfig {
    fn default() -> Self {
        Self {
            edge_id: "mac".into(),
            periodic_publish_interval_secs: 5,
        }
    }
}

impl Config {
    /// Load config from a TOML file, falling back to defaults if absent.
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let config: Config = if path.exists() {
            let content = std::fs::read_to_string(path)?;
            toml::from_str(&content)?
        } else {
            Config::default()
        };
        Ok(config)
    }
}
