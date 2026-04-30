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
    /// Render a glyph on a specific device immediately. Used by the
    /// weave-web "Test LED" affordance to verify a device's display path
    /// without waiting for a service-state event.
    DisplayGlyph {
        device_type: String,
        device_id: String,
        /// 9-line ASCII grid (`*` = on, anything else = off). Matches
        /// the `Glyph::pattern` shape used in `GlyphsUpdate`.
        pattern: String,
        /// Brightness 0.0..=1.0. Defaults to 1.0 when absent.
        #[serde(skip_serializing_if = "Option::is_none")]
        brightness: Option<f32>,
        /// Auto-clear timeout in milliseconds. Defaults to a short value
        /// when absent so test renders don't linger.
        #[serde(skip_serializing_if = "Option::is_none")]
        timeout_ms: Option<u32>,
        /// Transition kind (`"immediate"` or `"cross_fade"`). Defaults to
        /// cross-fade when absent.
        #[serde(skip_serializing_if = "Option::is_none")]
        transition: Option<String>,
    },
    /// Server-initiated request to (re)connect a specific device. Idempotent
    /// — already-connected devices are a no-op aside from clearing any
    /// "paused" state that previously suppressed reconnect attempts.
    DeviceConnect {
        device_type: String,
        device_id: String,
    },
    /// Server-initiated request to disconnect a specific device. Sets a
    /// paused flag so the auto-reconnect loop does not immediately
    /// re-establish the link.
    DeviceDisconnect {
        device_type: String,
        device_id: String,
    },
    /// Server-forwarded intent dispatch. The originating edge routed an
    /// input but lacked the adapter for `service_type`; the server
    /// looked up an edge whose Hello capabilities include `service_type`
    /// and forwarded the intent here. The receiving edge feeds the
    /// payload into its existing dispatcher (same path the local
    /// routing engine uses) so an `EdgeToServer::Command` telemetry
    /// frame still emits with the actual outcome.
    ///
    /// Wire shape mirrors `EdgeToServer::Command` (intent name + params)
    /// so receivers can reuse the same `Intent` reassembly logic both
    /// for self-routed intents and forwarded ones.
    DispatchIntent {
        service_type: String,
        service_target: String,
        /// Snake-case intent name (`play_pause`, `volume_change`, …).
        intent: String,
        /// Intent parameters serialized as JSON. Shape matches the
        /// `Intent` enum's payload after `#[serde(tag = "type")]` lifts
        /// the discriminant out.
        #[serde(default)]
        params: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        output_id: Option<String>,
    },
    /// Incremental cycle change for one device. Sent to the edge that owns
    /// the device whenever the server applies a cycle CRUD operation.
    /// `op == Delete` removes the cycle on the edge (mappings revert to the
    /// no-cycle "fire all matching" behavior).
    DeviceCyclePatch { cycle: DeviceCycle, op: PatchOp },
    /// Server-initiated active-connection switch for a device's cycle. The
    /// receiving edge updates its local cycle snapshot and routes input
    /// only through `active_mapping_id` going forward (until the next
    /// switch). Originates from a REST `POST .../cycle/switch`, an
    /// `EdgeToServer::SwitchActiveConnection` from a peer edge that
    /// observed the cycle gesture, or an automatic advance.
    SwitchActiveConnection {
        device_type: String,
        device_id: String,
        active_mapping_id: Uuid,
        /// Optional human-readable label for the new active mapping's
        /// `service_target`, resolved server-side from the StateHub's
        /// service_state cache (Roon zone display_name, Hue light
        /// metadata.name, hardcoded "Apple Music" for `ios_media`).
        /// When present, edges use the first ASCII alphanumeric char
        /// as the LED letter hint. Older servers omit the field;
        /// receivers must treat `None` as a fallback signal.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        service_target_label: Option<String>,
    },
    /// Echo of a service state update originally published by another
    /// edge. weave-server fans out `EdgeToServer::State` to every other
    /// connected edge so locally-mapped Nuimos can render LED feedback
    /// for services the edge itself doesn't dispatch (cross-edge
    /// scenario: iPad maps Nuimo → Roon, but Roon adapter lives on a
    /// peer edge). Receiver feeds this directly into its local feedback
    /// pump as a `StateUpdate`. The `edge_id` carries the originating
    /// edge so receivers can ignore loop-backs (server already filters
    /// the source out, but the field is useful for diagnostics).
    ServiceState {
        edge_id: String,
        service_type: String,
        target: String,
        property: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        output_id: Option<String>,
        value: serde_json::Value,
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
    /// Periodic edge-side metrics. Emitted on a fixed cadence (typically
    /// every 10 s) so the server can surface edge health in `/ws/ui`
    /// dashboards. Server-side latency is measured separately from
    /// `Ping`/`Pong` round trips and is not carried here.
    EdgeStatus {
        /// Wifi signal strength normalized to 0..=100 percent. `None`
        /// when the platform doesn't expose a signal-strength API to
        /// user code, when the host has no wifi adapter, or when the
        /// API call failed (entitlement missing, permission denied).
        #[serde(skip_serializing_if = "Option::is_none")]
        wifi: Option<u8>,
    },
    /// Edge routed an input locally but has no adapter for the resulting
    /// `service_type` and asks the server to forward to a capable peer.
    /// The server resolves a target edge from `Hello` capabilities and
    /// re-emits as `ServerToEdge::DispatchIntent`. Wire shape mirrors
    /// `Command` (intent name + params) so the same reassembly logic
    /// works on both ends.
    ///
    /// The originating edge does NOT emit a `Command` frame for
    /// forwarded intents — the executing edge does that after running
    /// the adapter, so latency measurement reflects the full path.
    DispatchIntent {
        service_type: String,
        service_target: String,
        intent: String,
        #[serde(default)]
        params: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        output_id: Option<String>,
    },
    /// The edge advanced its local cycle (typically via `cycle_gesture`
    /// firing on the device) and asks the server to persist the new
    /// active. Server applies the change, broadcasts
    /// `UiFrame::DeviceCycleChanged` to web UIs, and echoes
    /// `ServerToEdge::SwitchActiveConnection` to other edges that observe
    /// the same device so they stay in sync.
    SwitchActiveConnection {
        device_type: String,
        device_id: String,
        active_mapping_id: Uuid,
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
    /// Device-level cycle rows for this edge's devices. Empty when no
    /// device under this edge has an active cycle. Older edge-agents
    /// receiving this field as unknown deserialize an empty vec via the
    /// default annotation.
    #[serde(default)]
    pub device_cycles: Vec<DeviceCycle>,
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
    /// Periodic edge metrics. Carries the latest known wifi signal
    /// strength (edge-reported) and round-trip latency (server-measured
    /// from `Ping`/`Pong`). Each field is `None` when unknown:
    /// either because no measurement has arrived yet, or because the
    /// edge cannot read the value on its platform. Emitted whenever
    /// either field changes; UIs apply it as a partial update on the
    /// matching `edge_id` row.
    EdgeStatus {
        edge_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        wifi: Option<u8>,
        #[serde(skip_serializing_if = "Option::is_none")]
        latency_ms: Option<u32>,
    },
    /// Device-cycle CRUD broadcast. UIs replace their copy. `cycle` is
    /// `None` when `op == Delete` (the cycle row was removed and the
    /// device's mappings revert to the all-fire default).
    DeviceCycleChanged {
        device_type: String,
        device_id: String,
        op: PatchOp,
        cycle: Option<DeviceCycle>,
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
    /// All device cycles known to the server. Empty when no device has
    /// a cycle. `#[serde(default)]` so older clients without the field
    /// deserialize as an empty vec.
    #[serde(default)]
    pub device_cycles: Vec<DeviceCycle>,
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

/// Device-level Connection cycle. When a row exists for `(device_type,
/// device_id)`, only the mapping identified by `active_mapping_id` routes
/// input for that device — the other mappings in `mapping_ids` sit dormant
/// until cycled in. Mappings outside the cycle (i.e. not in `mapping_ids`)
/// are unaffected and continue to fire normally.
///
/// `cycle_gesture`, if set, is the input primitive that advances the active
/// pointer to the next entry in `mapping_ids` order. Both edge and server
/// emit `SwitchActiveConnection` to keep state in sync; the receiver applies
/// the change idempotently.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeviceCycle {
    pub device_type: String,
    pub device_id: String,
    /// Mappings to rotate through, in cycle order. The cycle gesture
    /// advances active to the next entry, wrapping at the end.
    pub mapping_ids: Vec<Uuid>,
    /// Currently-active mapping (must be one of `mapping_ids`). `None`
    /// when the cycle is empty (transient — the server normally clears
    /// the cycle row in that case).
    #[serde(default)]
    pub active_mapping_id: Option<Uuid>,
    /// Snake-case `InputType` name (e.g. `"swipe_up"`, `"long_press"`)
    /// that advances active. `None` = the cycle exists but only switches
    /// via API; no on-device gesture binding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cycle_gesture: Option<String>,
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
                device_cycles: vec![],
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
    fn server_to_edge_display_glyph_roundtrip() {
        let msg = ServerToEdge::DisplayGlyph {
            device_type: "nuimo".into(),
            device_id: "C3:81:DF:4E:FF:6A".into(),
            pattern: "    *    \n  *****  ".into(),
            brightness: Some(0.5),
            timeout_ms: Some(2000),
            transition: Some("cross_fade".into()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"display_glyph\""));
        assert!(json.contains("\"device_type\":\"nuimo\""));
        assert!(json.contains("\"brightness\":0.5"));

        let parsed: ServerToEdge = serde_json::from_str(&json).unwrap();
        match parsed {
            ServerToEdge::DisplayGlyph {
                device_type,
                device_id,
                pattern,
                brightness,
                timeout_ms,
                transition,
            } => {
                assert_eq!(device_type, "nuimo");
                assert_eq!(device_id, "C3:81:DF:4E:FF:6A");
                assert!(pattern.contains('*'));
                assert_eq!(brightness, Some(0.5));
                assert_eq!(timeout_ms, Some(2000));
                assert_eq!(transition.as_deref(), Some("cross_fade"));
            }
            _ => panic!("wrong variant"),
        }

        // Optional fields elided when None.
        let minimal = ServerToEdge::DisplayGlyph {
            device_type: "nuimo".into(),
            device_id: "dev-1".into(),
            pattern: "*".into(),
            brightness: None,
            timeout_ms: None,
            transition: None,
        };
        let json = serde_json::to_string(&minimal).unwrap();
        assert!(!json.contains("brightness"));
        assert!(!json.contains("timeout_ms"));
        assert!(!json.contains("transition"));
    }

    #[test]
    fn server_to_edge_device_connect_disconnect_roundtrip() {
        let connect = ServerToEdge::DeviceConnect {
            device_type: "nuimo".into(),
            device_id: "dev-1".into(),
        };
        let json = serde_json::to_string(&connect).unwrap();
        assert!(json.contains("\"type\":\"device_connect\""));
        let parsed: ServerToEdge = serde_json::from_str(&json).unwrap();
        match parsed {
            ServerToEdge::DeviceConnect {
                device_type,
                device_id,
            } => {
                assert_eq!(device_type, "nuimo");
                assert_eq!(device_id, "dev-1");
            }
            _ => panic!("wrong variant"),
        }

        let disconnect = ServerToEdge::DeviceDisconnect {
            device_type: "nuimo".into(),
            device_id: "dev-1".into(),
        };
        let json = serde_json::to_string(&disconnect).unwrap();
        assert!(json.contains("\"type\":\"device_disconnect\""));
        let parsed: ServerToEdge = serde_json::from_str(&json).unwrap();
        match parsed {
            ServerToEdge::DeviceDisconnect {
                device_type,
                device_id,
            } => {
                assert_eq!(device_type, "nuimo");
                assert_eq!(device_id, "dev-1");
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
    fn ui_frame_edge_status_roundtrip() {
        let full = UiFrame::EdgeStatus {
            edge_id: "air".into(),
            wifi: Some(82),
            latency_ms: Some(15),
        };
        let json = serde_json::to_string(&full).unwrap();
        assert!(json.contains("\"type\":\"edge_status\""));
        assert!(json.contains("\"wifi\":82"));
        assert!(json.contains("\"latency_ms\":15"));
        let parsed: UiFrame = serde_json::from_str(&json).unwrap();
        match parsed {
            UiFrame::EdgeStatus {
                edge_id,
                wifi,
                latency_ms,
            } => {
                assert_eq!(edge_id, "air");
                assert_eq!(wifi, Some(82));
                assert_eq!(latency_ms, Some(15));
            }
            _ => panic!("wrong variant"),
        }

        // Both metrics absent → only edge_id on the wire.
        let empty = UiFrame::EdgeStatus {
            edge_id: "air".into(),
            wifi: None,
            latency_ms: None,
        };
        let json = serde_json::to_string(&empty).unwrap();
        assert!(json.contains("\"edge_id\":\"air\""));
        assert!(!json.contains("wifi"));
        assert!(!json.contains("latency_ms"));
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
    fn edge_to_server_edge_status_roundtrip() {
        let with_wifi = EdgeToServer::EdgeStatus { wifi: Some(73) };
        let json = serde_json::to_string(&with_wifi).unwrap();
        assert!(json.contains("\"type\":\"edge_status\""));
        assert!(json.contains("\"wifi\":73"));
        let parsed: EdgeToServer = serde_json::from_str(&json).unwrap();
        match parsed {
            EdgeToServer::EdgeStatus { wifi } => assert_eq!(wifi, Some(73)),
            _ => panic!("wrong variant"),
        }

        // None should be elided from the wire form.
        let no_wifi = EdgeToServer::EdgeStatus { wifi: None };
        let json = serde_json::to_string(&no_wifi).unwrap();
        assert!(json.contains("\"type\":\"edge_status\""));
        assert!(!json.contains("wifi"));
        let parsed: EdgeToServer = serde_json::from_str(&json).unwrap();
        match parsed {
            EdgeToServer::EdgeStatus { wifi } => assert_eq!(wifi, None),
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

    #[test]
    fn device_cycle_roundtrip() {
        let m1 = Uuid::new_v4();
        let m2 = Uuid::new_v4();
        let cycle = DeviceCycle {
            device_type: "nuimo".into(),
            device_id: "C3:81:DF:4E:FF:6A".into(),
            mapping_ids: vec![m1, m2],
            active_mapping_id: Some(m1),
            cycle_gesture: Some("swipe_up".into()),
        };
        let json = serde_json::to_string(&cycle).unwrap();
        assert!(json.contains("\"device_type\":\"nuimo\""));
        assert!(json.contains("\"cycle_gesture\":\"swipe_up\""));
        let parsed: DeviceCycle = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, cycle);

        // Optional cycle_gesture elided when None.
        let no_gesture = DeviceCycle {
            cycle_gesture: None,
            ..cycle.clone()
        };
        let json = serde_json::to_string(&no_gesture).unwrap();
        assert!(!json.contains("cycle_gesture"));
    }

    #[test]
    fn server_to_edge_device_cycle_patch_roundtrip() {
        let m1 = Uuid::new_v4();
        let msg = ServerToEdge::DeviceCyclePatch {
            cycle: DeviceCycle {
                device_type: "nuimo".into(),
                device_id: "dev-1".into(),
                mapping_ids: vec![m1],
                active_mapping_id: Some(m1),
                cycle_gesture: Some("swipe_up".into()),
            },
            op: PatchOp::Upsert,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"device_cycle_patch\""));
        assert!(json.contains("\"op\":\"upsert\""));
        let parsed: ServerToEdge = serde_json::from_str(&json).unwrap();
        match parsed {
            ServerToEdge::DeviceCyclePatch { cycle, op } => {
                assert_eq!(cycle.device_type, "nuimo");
                assert!(matches!(op, PatchOp::Upsert));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn server_to_edge_switch_active_connection_roundtrip() {
        let m1 = Uuid::new_v4();
        let msg = ServerToEdge::SwitchActiveConnection {
            device_type: "nuimo".into(),
            device_id: "dev-1".into(),
            active_mapping_id: m1,
            service_target_label: Some("Apple Music".into()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"switch_active_connection\""));
        assert!(json.contains("\"service_target_label\":\"Apple Music\""));
        let parsed: ServerToEdge = serde_json::from_str(&json).unwrap();
        match parsed {
            ServerToEdge::SwitchActiveConnection {
                device_type,
                device_id,
                active_mapping_id,
                service_target_label,
            } => {
                assert_eq!(device_type, "nuimo");
                assert_eq!(device_id, "dev-1");
                assert_eq!(active_mapping_id, m1);
                assert_eq!(service_target_label.as_deref(), Some("Apple Music"));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn server_to_edge_switch_active_connection_omits_label_when_none() {
        let msg = ServerToEdge::SwitchActiveConnection {
            device_type: "nuimo".into(),
            device_id: "dev-1".into(),
            active_mapping_id: Uuid::new_v4(),
            service_target_label: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(!json.contains("service_target_label"));
    }

    #[test]
    fn server_to_edge_switch_active_connection_back_compat_no_label() {
        // Older servers that haven't bumped emit the frame without the
        // `service_target_label` field. The receiver must accept it.
        let json = r#"{"type":"switch_active_connection","device_type":"nuimo","device_id":"dev-1","active_mapping_id":"00000000-0000-0000-0000-000000000000"}"#;
        let parsed: ServerToEdge = serde_json::from_str(json).expect("parses without label");
        match parsed {
            ServerToEdge::SwitchActiveConnection {
                service_target_label,
                ..
            } => assert_eq!(service_target_label, None),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn edge_to_server_switch_active_connection_roundtrip() {
        let m1 = Uuid::new_v4();
        let msg = EdgeToServer::SwitchActiveConnection {
            device_type: "nuimo".into(),
            device_id: "dev-1".into(),
            active_mapping_id: m1,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"switch_active_connection\""));
        let parsed: EdgeToServer = serde_json::from_str(&json).unwrap();
        match parsed {
            EdgeToServer::SwitchActiveConnection {
                active_mapping_id, ..
            } => assert_eq!(active_mapping_id, m1),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn server_to_edge_service_state_roundtrip() {
        let msg = ServerToEdge::ServiceState {
            edge_id: "pro".into(),
            service_type: "roon".into(),
            target: "zone-living".into(),
            property: "volume".into(),
            output_id: Some("output-1".into()),
            value: serde_json::json!(47),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"service_state\""));
        assert!(json.contains("\"edge_id\":\"pro\""));
        assert!(json.contains("\"output_id\":\"output-1\""));
        let parsed: ServerToEdge = serde_json::from_str(&json).unwrap();
        match parsed {
            ServerToEdge::ServiceState {
                edge_id,
                service_type,
                target,
                property,
                output_id,
                value,
            } => {
                assert_eq!(edge_id, "pro");
                assert_eq!(service_type, "roon");
                assert_eq!(target, "zone-living");
                assert_eq!(property, "volume");
                assert_eq!(output_id.as_deref(), Some("output-1"));
                assert_eq!(value, serde_json::json!(47));
            }
            _ => panic!("wrong variant"),
        }

        // output_id elided when None.
        let no_output = ServerToEdge::ServiceState {
            edge_id: "pro".into(),
            service_type: "roon".into(),
            target: "z".into(),
            property: "playback".into(),
            output_id: None,
            value: serde_json::json!("playing"),
        };
        let json = serde_json::to_string(&no_output).unwrap();
        assert!(!json.contains("output_id"));
    }

    #[test]
    fn ui_frame_device_cycle_changed_roundtrip() {
        let m1 = Uuid::new_v4();
        let upsert = UiFrame::DeviceCycleChanged {
            device_type: "nuimo".into(),
            device_id: "dev-1".into(),
            op: PatchOp::Upsert,
            cycle: Some(DeviceCycle {
                device_type: "nuimo".into(),
                device_id: "dev-1".into(),
                mapping_ids: vec![m1],
                active_mapping_id: Some(m1),
                cycle_gesture: Some("swipe_up".into()),
            }),
        };
        let json = serde_json::to_string(&upsert).unwrap();
        assert!(json.contains("\"type\":\"device_cycle_changed\""));
        assert!(json.contains("\"op\":\"upsert\""));
        let _: UiFrame = serde_json::from_str(&json).unwrap();

        let delete = UiFrame::DeviceCycleChanged {
            device_type: "nuimo".into(),
            device_id: "dev-1".into(),
            op: PatchOp::Delete,
            cycle: None,
        };
        let json = serde_json::to_string(&delete).unwrap();
        assert!(json.contains("\"op\":\"delete\""));
        assert!(json.contains("\"cycle\":null"));
    }

    #[test]
    fn ui_snapshot_device_cycles_default_empty() {
        // Older server payloads without the device_cycles field still parse.
        let json = r#"{
            "edges": [],
            "service_states": [],
            "device_states": [],
            "mappings": [],
            "glyphs": []
        }"#;
        let snap: UiSnapshot = serde_json::from_str(json).unwrap();
        assert!(snap.device_cycles.is_empty());
    }

    #[test]
    fn edge_config_device_cycles_default_empty() {
        // Older ConfigFull payloads without device_cycles still parse.
        let json = r#"{
            "edge_id": "air",
            "mappings": []
        }"#;
        let cfg: EdgeConfig = serde_json::from_str(json).unwrap();
        assert!(cfg.device_cycles.is_empty());
    }
}
