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
    /// Deprecated: single-address shim for backward compat. When set, its
    /// value is prepended to `ble_addresses` during load. Prefer
    /// `ble_addresses` in new configs.
    #[serde(default)]
    pub ble_address: Option<String>,
    /// Allowlist of BLE addresses for Nuimo devices managed by this edge.
    /// An empty list disables BLE entirely (WS-only mode, same as `skip`).
    /// Addresses are compared case-insensitively.
    #[serde(default)]
    pub ble_addresses: Vec<String>,
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

impl NuimoSection {
    /// Merge deprecated `ble_address` (single) into `ble_addresses` and
    /// dedupe case-insensitively while preserving config order. Run once
    /// after env overrides so both code paths converge on a single
    /// `Vec<String>` for downstream use.
    fn normalize(&mut self) {
        if let Some(addr) = self.ble_address.take() {
            let trimmed = addr.trim();
            if !trimmed.is_empty()
                && !self
                    .ble_addresses
                    .iter()
                    .any(|existing| existing.eq_ignore_ascii_case(trimmed))
            {
                self.ble_addresses.insert(0, trimmed.to_string());
                tracing::warn!(
                    "nuimo.ble_address is deprecated — use nuimo.ble_addresses = [\"...\"]"
                );
            }
        }
        // Dedupe case-insensitively, preserving first-seen order.
        let mut seen: Vec<String> = Vec::with_capacity(self.ble_addresses.len());
        self.ble_addresses.retain(|addr| {
            let trimmed = addr.trim();
            if trimmed.is_empty() {
                return false;
            }
            if seen.iter().any(|s| s.eq_ignore_ascii_case(trimmed)) {
                return false;
            }
            seen.push(trimmed.to_string());
            true
        });
        // Strip any surrounding whitespace now that we've accepted the entry.
        for addr in &mut self.ble_addresses {
            *addr = addr.trim().to_string();
        }
    }
}

impl Config {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("failed to read {}: {}", path.display(), e))?;
        let mut cfg: Self = toml::from_str(&text)?;
        cfg.apply_env_overrides();
        cfg.nuimo.normalize();
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
        if let Ok(v) = std::env::var("EDGE_AGENT_NUIMO_BLE_ADDRESSES") {
            self.nuimo.ble_addresses = v
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_promotes_deprecated_single_address() {
        let mut s = NuimoSection {
            ble_address: Some("AA:BB:CC:DD:EE:FF".into()),
            ble_addresses: vec![],
            skip: false,
        };
        s.normalize();
        assert_eq!(s.ble_address, None);
        assert_eq!(s.ble_addresses, vec!["AA:BB:CC:DD:EE:FF"]);
    }

    #[test]
    fn normalize_dedupes_case_insensitively() {
        let mut s = NuimoSection {
            ble_address: Some("aa:bb:cc:dd:ee:ff".into()),
            ble_addresses: vec!["AA:BB:CC:DD:EE:FF".into(), "11:22:33:44:55:66".into()],
            skip: false,
        };
        s.normalize();
        assert_eq!(
            s.ble_addresses,
            vec!["AA:BB:CC:DD:EE:FF", "11:22:33:44:55:66"]
        );
    }

    #[test]
    fn normalize_preserves_order_of_ble_addresses() {
        let mut s = NuimoSection {
            ble_address: None,
            ble_addresses: vec!["11:22:33:44:55:66".into(), "AA:BB:CC:DD:EE:FF".into()],
            skip: false,
        };
        s.normalize();
        assert_eq!(
            s.ble_addresses,
            vec!["11:22:33:44:55:66", "AA:BB:CC:DD:EE:FF"]
        );
    }

    #[test]
    fn normalize_prepends_deprecated_when_not_in_list() {
        let mut s = NuimoSection {
            ble_address: Some("99:99:99:99:99:99".into()),
            ble_addresses: vec!["AA:BB:CC:DD:EE:FF".into()],
            skip: false,
        };
        s.normalize();
        assert_eq!(
            s.ble_addresses,
            vec!["99:99:99:99:99:99", "AA:BB:CC:DD:EE:FF"]
        );
    }

    #[test]
    fn normalize_drops_empty_and_whitespace_entries() {
        let mut s = NuimoSection {
            ble_address: Some("  ".into()),
            ble_addresses: vec![
                "".into(),
                "  AA:BB:CC:DD:EE:FF  ".into(),
                "11:22:33:44:55:66".into(),
            ],
            skip: false,
        };
        s.normalize();
        assert_eq!(
            s.ble_addresses,
            vec!["AA:BB:CC:DD:EE:FF", "11:22:33:44:55:66"]
        );
    }
}
