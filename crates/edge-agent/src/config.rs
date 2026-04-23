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
    /// `None` when the `[hue]` section is absent from TOML — Hue adapter stays
    /// disabled. `Some(_)` with an empty body enables Hue with the default
    /// XDG_STATE_HOME token path.
    pub hue: Option<HueSection>,
    /// `None` when the `[macos]` section is absent from TOML — macOS adapter
    /// stays disabled. `Some(_)` with an empty body enables it with the
    /// default localhost broker settings.
    pub macos: Option<MacosSection>,
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

#[derive(Debug, Deserialize, Default)]
pub struct HueSection {
    /// Path to the JSON token file written by `edge-agent pair-hue`.
    /// Expected shape: `{"host": "...", "app_key": "...", "client_key": "..."}`.
    /// When omitted, defaults to `$XDG_STATE_HOME/edge-agent/hue-token.json`.
    /// If the resolved file is missing or unreadable, the Hue adapter logs a
    /// warning and stays disabled at runtime.
    pub token_path: Option<PathBuf>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct MacosSection {
    #[serde(default = "default_macos_mqtt_host")]
    pub mqtt_host: String,
    #[serde(default = "default_macos_mqtt_port")]
    pub mqtt_port: u16,
    #[serde(default = "default_macos_mqtt_client_id")]
    pub mqtt_client_id: String,
}

impl Default for MacosSection {
    fn default() -> Self {
        Self {
            mqtt_host: default_macos_mqtt_host(),
            mqtt_port: default_macos_mqtt_port(),
            mqtt_client_id: default_macos_mqtt_client_id(),
        }
    }
}

fn default_macos_mqtt_host() -> String {
    "localhost".into()
}

fn default_macos_mqtt_port() -> u16 {
    1883
}

fn default_macos_mqtt_client_id() -> String {
    "edge-agent-macos".into()
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
        if let Ok(v) = std::env::var("EDGE_AGENT_HUE_TOKEN_PATH") {
            let hue = self.hue.get_or_insert_with(HueSection::default);
            hue.token_path = Some(PathBuf::from(v));
        }
        if let Ok(v) = std::env::var("EDGE_AGENT_MACOS_MQTT_HOST") {
            let macos = self.macos.get_or_insert_with(MacosSection::default);
            macos.mqtt_host = v;
        }
        if let Ok(v) = std::env::var("EDGE_AGENT_MACOS_MQTT_PORT") {
            if let Ok(p) = v.parse() {
                let macos = self.macos.get_or_insert_with(MacosSection::default);
                macos.mqtt_port = p;
            }
        }
        if let Ok(v) = std::env::var("EDGE_AGENT_MACOS_MQTT_CLIENT_ID") {
            let macos = self.macos.get_or_insert_with(MacosSection::default);
            macos.mqtt_client_id = v;
        }
    }
}
