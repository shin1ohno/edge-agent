//! WebSocket protocol types shared between `edge-agent` and `weave-server`.
//!
//! Wire format: JSON text frames. Each frame is a single `ServerToEdge` or
//! `EdgeToServer` value serialized as JSON. The runtime binds to a LAN IP
//! and performs no authentication.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

/// Frames sent from `weave-server` to an `edge-agent`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerToEdge {
    /// Full config snapshot. Sent on (re)connect and on bulk reload.
    ConfigFull { config: EdgeConfig },
    /// Incremental mapping change.
    ConfigPatch {
        mapping_id: Uuid,
        op: PatchOp,
        mapping: Option<Mapping>,
    },
    /// Server-initiated active-target switch for an existing mapping.
    TargetSwitch {
        mapping_id: Uuid,
        service_target: String,
    },
    /// Periodic keepalive to keep NAT/proxies open and detect half-open TCP.
    Ping,
}

/// Frames sent from an `edge-agent` to `weave-server`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EdgeToServer {
    /// First frame after connect. Declares identity and adapter capabilities.
    Hello {
        edge_id: String,
        version: String,
        capabilities: Vec<String>,
    },
    /// State update for a service target (e.g. Roon zone playback / volume).
    State {
        service_type: String,
        target: String,
        property: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        output_id: Option<String>,
        value: serde_json::Value,
    },
    /// State update for a device (battery, RSSI, connected).
    DeviceState {
        device_type: String,
        device_id: String,
        property: String,
        value: serde_json::Value,
    },
    /// Reply to server `Ping`.
    Pong,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatchOp {
    Upsert,
    Delete,
}

/// Complete config for one edge, pushed as a `ConfigFull` frame.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeConfig {
    pub edge_id: String,
    pub mappings: Vec<Mapping>,
}

/// A device-to-service mapping. Mirrors the structure already used by
/// `weave-server`'s REST API. `edge_id` is new; all other fields retain
/// their existing semantics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mapping {
    pub mapping_id: Uuid,
    pub edge_id: String,
    pub device_type: String,
    pub device_id: String,
    pub service_type: String,
    pub service_target: String,
    pub routes: Vec<Route>,
    #[serde(default)]
    pub feedback: Vec<FeedbackRule>,
    #[serde(default = "default_true")]
    pub active: bool,
}

fn default_true() -> bool {
    true
}

/// One input-to-intent route inside a mapping.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Route {
    pub input: String,
    pub intent: String,
    #[serde(default)]
    pub params: BTreeMap<String, serde_json::Value>,
}

/// Feedback rule: service state → device visual feedback.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedbackRule {
    pub state: String,
    pub feedback_type: String,
    pub mapping: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_to_edge_config_full_roundtrip() {
        let msg = ServerToEdge::ConfigFull {
            config: EdgeConfig {
                edge_id: "living-room".into(),
                mappings: vec![Mapping {
                    mapping_id: Uuid::nil(),
                    edge_id: "living-room".into(),
                    device_type: "nuimo".into(),
                    device_id: "C3:81:DF:4E:FF:6A".into(),
                    service_type: "roon".into(),
                    service_target: "zone-1".into(),
                    routes: vec![Route {
                        input: "rotate".into(),
                        intent: "volume_change".into(),
                        params: BTreeMap::from([(
                            "damping".into(),
                            serde_json::json!(80),
                        )]),
                    }],
                    feedback: vec![],
                    active: true,
                }],
            },
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"config_full\""));
        assert!(json.contains("\"edge_id\":\"living-room\""));

        let parsed: ServerToEdge = serde_json::from_str(&json).unwrap();
        match parsed {
            ServerToEdge::ConfigFull { config } => {
                assert_eq!(config.edge_id, "living-room");
                assert_eq!(config.mappings.len(), 1);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn edge_to_server_state_with_optional_output_id() {
        let msg = EdgeToServer::State {
            service_type: "roon".into(),
            target: "zone-1".into(),
            property: "volume".into(),
            output_id: Some("output-1".into()),
            value: serde_json::json!(50),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"output_id\":\"output-1\""));

        let msg2 = EdgeToServer::State {
            service_type: "roon".into(),
            target: "zone-1".into(),
            property: "playback".into(),
            output_id: None,
            value: serde_json::json!("playing"),
        };
        let json2 = serde_json::to_string(&msg2).unwrap();
        assert!(!json2.contains("output_id"));
    }
}
