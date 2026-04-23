use std::path::Path;

use serde::Deserialize;

/// Hub configuration loaded from TOML with env var overrides.
///
/// Env var precedence (highest to lowest):
///   MACOS_HUB_*  > TOML file > defaults
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
    /// Edge identifier used as the `{target}` segment in
    /// `service/macos/{target}/command/{intent}`. Only commands whose target
    /// matches this id (or the wildcard `all`) are applied locally.
    pub edge_id: String,
    /// Seconds between periodic re-publishes of volume / default-output state.
    /// MQTT retained topics already cover late subscribers, but the timer
    /// covers drift when state is changed by another app (e.g. a user moves
    /// the volume slider in Sound Settings).
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
    /// Load config from a TOML file, then apply env var overrides.
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let mut config: Config = if path.exists() {
            let content = std::fs::read_to_string(path)?;
            toml::from_str(&content)?
        } else {
            Config::default()
        };

        if let Ok(v) = std::env::var("MACOS_HUB_MQTT_HOST") {
            config.mqtt.host = v;
        }
        if let Ok(v) = std::env::var("MACOS_HUB_MQTT_PORT")
            && let Ok(port) = v.parse()
        {
            config.mqtt.port = port;
        }
        if let Ok(v) = std::env::var("MACOS_HUB_MQTT_CLIENT_ID") {
            config.mqtt.client_id = v;
        }
        if let Ok(v) = std::env::var("MACOS_HUB_EDGE_ID") {
            config.macos.edge_id = v;
        }
        if let Ok(v) = std::env::var("MACOS_HUB_PERIODIC_PUBLISH_INTERVAL_SECS")
            && let Ok(secs) = v.parse()
        {
            config.macos.periodic_publish_interval_secs = secs;
        }

        Ok(config)
    }
}
