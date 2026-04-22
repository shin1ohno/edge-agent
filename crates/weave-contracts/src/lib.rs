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
    /// Replace the edge's glyph set. Sent after any glyph CRUD on the server.
    GlyphsUpdate { glyphs: Vec<Glyph> },
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
    /// The edge committed a target switch via on-device selection mode.
    /// Server replies by calling the same code path as `POST
    /// /api/mappings/:id/target`: persist the new `service_target`, then
    /// broadcast a `ConfigPatch` upsert back to all edges (including the
    /// sender) and a `MappingChanged` to UI subscribers.
    SwitchTarget {
        mapping_id: Uuid,
        service_target: String,
    },
    /// A command that the edge's adapter emitted to an external service
    /// (Roon MOO RPC, Hue REST, …). One frame per `adapter.send_intent`
    /// call, carrying the outcome and measured latency so the UI live
    /// stream can show "sent → ok (42ms)" rows alongside input and
    /// state-echo rows.
    Command {
        service_type: String,
        target: String,
        /// Snake-case intent name (`volume_change`, `play_pause`, …).
        intent: String,
        /// Intent parameters serialized as JSON. Shape matches the
        /// `weave-engine::Intent` discriminant's payload.
        #[serde(default)]
        params: serde_json::Value,
        result: CommandResult,
        #[serde(skip_serializing_if = "Option::is_none")]
        latency_ms: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        output_id: Option<String>,
    },
    /// Adapter-level or routing-level error not tied to a specific
    /// command (bridge disconnect, auth token expired, pairing lost).
    /// Command-level failures use `Command { result: Err { .. } }`
    /// instead — `Error` is for ambient conditions.
    Error {
        context: String,
        message: String,
        severity: ErrorSeverity,
    },
}

/// Outcome of an `EdgeToServer::Command`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CommandResult {
    Ok,
    Err { message: String },
}

/// Severity classification for `EdgeToServer::Error` and `UiFrame::Error`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorSeverity {
    Warn,
    Error,
    Fatal,
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
    /// Named glyph patterns the edge should use when rendering feedback.
    /// Consumers look up by `name`. Entries with `builtin == true` have an
    /// empty `pattern` and are expected to be rendered programmatically by
    /// the consumer (e.g. `volume_bar` scales with percentage).
    #[serde(default)]
    pub glyphs: Vec<Glyph>,
}

/// A named Nuimo LED glyph. `pattern` is a 9x9 ASCII grid compatible with
/// `nuimo::Glyph::from_str` (`*` = LED on, anything else = off, rows
/// separated by `\n`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Glyph {
    pub name: String,
    #[serde(default)]
    pub pattern: String,
    #[serde(default)]
    pub builtin: bool,
}

/// Frames sent from `weave-server` to a Web UI client on `/ws/ui`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UiFrame {
    /// Initial full snapshot, pushed once on connect.
    Snapshot { snapshot: UiSnapshot },
    /// An edge completed its `Hello` handshake or has otherwise come online.
    EdgeOnline { edge: EdgeInfo },
    /// An edge has disconnected (ws closed).
    EdgeOffline { edge_id: String },
    /// One service-state update from a connected edge.
    ServiceState {
        edge_id: String,
        service_type: String,
        target: String,
        property: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        output_id: Option<String>,
        value: serde_json::Value,
    },
    /// One device-state update from a connected edge (battery, RSSI, etc.).
    DeviceState {
        edge_id: String,
        device_type: String,
        device_id: String,
        property: String,
        value: serde_json::Value,
    },
    /// Mapping CRUD happened on the server. UIs replace their copy.
    MappingChanged {
        mapping_id: Uuid,
        op: PatchOp,
        mapping: Option<Mapping>,
    },
    /// The glyph set changed. UIs refresh their registry.
    GlyphsChanged { glyphs: Vec<Glyph> },
    /// Fan-out of an edge-emitted `Command`. Transient — never stored in
    /// `UiSnapshot`; dashboards that open after the fact will not see it.
    Command {
        edge_id: String,
        service_type: String,
        target: String,
        intent: String,
        #[serde(default)]
        params: serde_json::Value,
        result: CommandResult,
        #[serde(skip_serializing_if = "Option::is_none")]
        latency_ms: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        output_id: Option<String>,
        /// RFC3339 timestamp assigned by the server on fan-out.
        at: String,
    },
    /// Fan-out of an edge-emitted `Error`. Transient.
    Error {
        edge_id: String,
        context: String,
        message: String,
        severity: ErrorSeverity,
        /// RFC3339 timestamp assigned by the server on fan-out.
        at: String,
    },
}

/// Initial full state sent on `/ws/ui` connect. Subsequent changes arrive
/// as `UiFrame` variants.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiSnapshot {
    pub edges: Vec<EdgeInfo>,
    pub service_states: Vec<ServiceStateEntry>,
    pub device_states: Vec<DeviceStateEntry>,
    pub mappings: Vec<Mapping>,
    pub glyphs: Vec<Glyph>,
}

/// Identity + status for one connected (or previously-seen) edge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeInfo {
    pub edge_id: String,
    pub online: bool,
    pub version: String,
    pub capabilities: Vec<String>,
    /// RFC3339 timestamp.
    pub last_seen: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceStateEntry {
    pub edge_id: String,
    pub service_type: String,
    pub target: String,
    pub property: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_id: Option<String>,
    pub value: serde_json::Value,
    /// RFC3339 timestamp of last update.
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceStateEntry {
    pub edge_id: String,
    pub device_type: String,
    pub device_id: String,
    pub property: String,
    pub value: serde_json::Value,
    pub updated_at: String,
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
    /// Ordered list of candidate `service_target` values the edge can cycle
    /// through at runtime. Empty = switching disabled.
    #[serde(default)]
    pub target_candidates: Vec<TargetCandidate>,
    /// Input primitive (snake-case `InputType` name, e.g. `"long_press"`)
    /// that enters selection mode on the device. `None` = feature disabled
    /// for this mapping, regardless of `target_candidates`.
    ///
    /// MVP constraint (not enforced in-schema): at most one mapping per
    /// `(edge_id, device_id)` should set this; the edge router picks the
    /// first encountered if multiple are set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_switch_on: Option<String>,
}

/// One entry in `Mapping::target_candidates`. During selection mode the
/// device displays `glyph` and, on confirm, the mapping's `service_target`
/// is replaced with `target`.
///
/// Optional `service_type` and `routes` overrides let a single mapping's
/// candidates straddle services — e.g. `long_press` cycles between a Roon
/// zone (rotate→volume_change) and a Hue light (rotate→brightness_change),
/// each with its own route table. When absent, the candidate inherits the
/// mapping's `service_type` / `routes`, which matches pre-override behavior
/// so historical mappings deserialize unchanged.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetCandidate {
    /// The `service_target` value to switch to (e.g. a Roon zone ID).
    pub target: String,
    /// Human-readable label for the UI only — the edge does not need it.
    #[serde(default)]
    pub label: String,
    /// Name of a glyph in the edge's glyph registry to display while this
    /// candidate is highlighted in selection mode.
    pub glyph: String,
    /// Override the mapping's `service_type` when this candidate is active.
    /// `None` = inherit from the parent `Mapping::service_type`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_type: Option<String>,
    /// Override the mapping's `routes` when this candidate is active. Required
    /// in practice whenever `service_type` differs from the mapping's, because
    /// intents are service-specific (Roon `volume_change` won't work against
    /// a Hue target). `None` = inherit from the parent `Mapping::routes`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub routes: Option<Vec<Route>>,
}

impl Mapping {
    /// Resolve the effective `(service_type, routes)` for a given target.
    /// If `target` matches a `target_candidates` entry with overrides,
    /// those win; otherwise the mapping's own fields are returned.
    ///
    /// Callers on the routing hot path should pass the currently active
    /// `service_target` to get the right adapter + intent table for the
    /// next emitted `RoutedIntent`.
    pub fn effective_for<'a>(&'a self, target: &str) -> (&'a str, &'a [Route]) {
        let candidate = self.target_candidates.iter().find(|c| c.target == target);
        let service_type = candidate
            .and_then(|c| c.service_type.as_deref())
            .unwrap_or(self.service_type.as_str());
        let routes = candidate
            .and_then(|c| c.routes.as_deref())
            .unwrap_or(self.routes.as_slice());
        (service_type, routes)
    }
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
                        params: BTreeMap::from([("damping".into(), serde_json::json!(80))]),
                    }],
                    feedback: vec![],
                    active: true,
                    target_candidates: vec![],
                    target_switch_on: None,
                }],
                glyphs: vec![Glyph {
                    name: "play".into(),
                    pattern: "    *    \n     **  ".into(),
                    builtin: false,
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
    fn edge_to_server_command_roundtrip() {
        let ok = EdgeToServer::Command {
            service_type: "roon".into(),
            target: "zone-1".into(),
            intent: "volume_change".into(),
            params: serde_json::json!({"delta": 3}),
            result: CommandResult::Ok,
            latency_ms: Some(42),
            output_id: None,
        };
        let json = serde_json::to_string(&ok).unwrap();
        assert!(json.contains("\"type\":\"command\""));
        assert!(json.contains("\"kind\":\"ok\""));
        assert!(json.contains("\"latency_ms\":42"));
        assert!(!json.contains("output_id"));
        let parsed: EdgeToServer = serde_json::from_str(&json).unwrap();
        match parsed {
            EdgeToServer::Command { intent, result, .. } => {
                assert_eq!(intent, "volume_change");
                assert!(matches!(result, CommandResult::Ok));
            }
            _ => panic!("wrong variant"),
        }

        let err = EdgeToServer::Command {
            service_type: "hue".into(),
            target: "light-1".into(),
            intent: "on_off".into(),
            params: serde_json::json!({"on": true}),
            result: CommandResult::Err {
                message: "bridge timeout".into(),
            },
            latency_ms: None,
            output_id: None,
        };
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("\"kind\":\"err\""));
        assert!(json.contains("\"message\":\"bridge timeout\""));
    }

    #[test]
    fn edge_to_server_error_roundtrip() {
        let msg = EdgeToServer::Error {
            context: "hue.bridge".into(),
            message: "connection refused".into(),
            severity: ErrorSeverity::Error,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"error\""));
        assert!(json.contains("\"severity\":\"error\""));
    }

    #[test]
    fn ui_frame_command_and_error_roundtrip() {
        let cmd = UiFrame::Command {
            edge_id: "air".into(),
            service_type: "roon".into(),
            target: "zone-1".into(),
            intent: "play_pause".into(),
            params: serde_json::json!({}),
            result: CommandResult::Ok,
            latency_ms: Some(18),
            output_id: None,
            at: "2026-04-23T12:00:00Z".into(),
        };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("\"type\":\"command\""));
        let _: UiFrame = serde_json::from_str(&json).unwrap();

        let err = UiFrame::Error {
            edge_id: "air".into(),
            context: "roon.client".into(),
            message: "pair lost".into(),
            severity: ErrorSeverity::Warn,
            at: "2026-04-23T12:00:00Z".into(),
        };
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("\"type\":\"error\""));
        assert!(json.contains("\"severity\":\"warn\""));
        let _: UiFrame = serde_json::from_str(&json).unwrap();
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
