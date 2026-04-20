//! Routing engine: applies a device input primitive against the active
//! mappings to produce zero or more `(service_type, target, intent)` triples.

use std::collections::HashMap;

use tokio::sync::RwLock;
use uuid::Uuid;
use weave_contracts::{Mapping, Route, TargetCandidate};

use super::intent::{InputPrimitive, Intent};

/// A concrete intent destined for a specific service target.
#[derive(Debug, Clone)]
pub struct RoutedIntent {
    pub service_type: String,
    pub service_target: String,
    pub intent: Intent,
}

/// Per-(device_type, device_id) transient state used by target-selection
/// mode. Keyed off the mapping that declared `target_switch_on`. While
/// this struct is in `RoutingEngine::selection`, the device has entered
/// mode: `Rotate` browses `cursor`, `Press` commits, any other input
/// cancels.
#[derive(Debug, Clone)]
pub struct SelectionMode {
    pub mapping_id: Uuid,
    pub edge_id: String,
    pub candidates: Vec<TargetCandidate>,
    pub cursor: usize,
}

impl SelectionMode {
    fn current(&self) -> &TargetCandidate {
        &self.candidates[self.cursor]
    }
    fn advance(&mut self, delta: f64) {
        let n = self.candidates.len();
        if n == 0 {
            return;
        }
        let step = if delta >= 0.0 { 1 } else { n - 1 };
        self.cursor = (self.cursor + step) % n;
    }
}

/// One outcome of routing a single input primitive. `Normal` is the
/// existing "route to zero or more intents" path; the other variants are
/// target-selection side effects that the caller must translate into LED
/// feedback or a server-bound `SwitchTarget` frame.
#[derive(Debug, Clone)]
pub enum RouteOutcome {
    Normal(Vec<RoutedIntent>),
    /// Device just entered selection mode; show `glyph` on the LED.
    EnterSelection {
        edge_id: String,
        mapping_id: Uuid,
        glyph: String,
    },
    /// Device still in selection mode, cursor moved; show `glyph`.
    UpdateSelection {
        mapping_id: Uuid,
        glyph: String,
    },
    /// Device committed the selection; caller sends
    /// `EdgeToServer::SwitchTarget` and optionally clears the LED.
    CommitSelection {
        edge_id: String,
        mapping_id: Uuid,
        service_target: String,
    },
    /// Device exited selection mode without committing (non-rotate/press
    /// input). Caller should clear any lingering LED feedback.
    CancelSelection {
        mapping_id: Uuid,
    },
}

/// Thread-safe registry of currently-active mappings, keyed by
/// `(device_type, device_id)` for O(1) lookup per input event.
#[derive(Default)]
pub struct RoutingEngine {
    by_device: RwLock<HashMap<(String, String), Vec<Mapping>>>,
    selection: RwLock<HashMap<(String, String), SelectionMode>>,
}

impl RoutingEngine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace all mappings with the provided set. Used on `config_full` push.
    pub async fn replace_all(&self, mappings: Vec<Mapping>) {
        let mut by_device: HashMap<(String, String), Vec<Mapping>> = HashMap::new();
        for m in mappings {
            by_device
                .entry((m.device_type.clone(), m.device_id.clone()))
                .or_default()
                .push(m);
        }
        *self.by_device.write().await = by_device;
    }

    /// Insert or replace one mapping, keyed by `mapping_id`. Used on
    /// `config_patch` upsert.
    pub async fn upsert_mapping(&self, mapping: Mapping) {
        let mut guard = self.by_device.write().await;
        // Remove any prior entry with the same mapping_id, regardless of
        // device key (handles device reassignment).
        for list in guard.values_mut() {
            list.retain(|m| m.mapping_id != mapping.mapping_id);
        }
        guard
            .entry((mapping.device_type.clone(), mapping.device_id.clone()))
            .or_default()
            .push(mapping);
    }

    /// Remove any mapping with the given `mapping_id`. Used on
    /// `config_patch` delete.
    pub async fn remove_mapping(&self, id: &uuid::Uuid) {
        let mut guard = self.by_device.write().await;
        for list in guard.values_mut() {
            list.retain(|m| &m.mapping_id != id);
        }
        // Prune empty device buckets so route() stays tight.
        guard.retain(|_, list| !list.is_empty());
    }

    /// Snapshot every mapping currently held, grouped or not. Used to
    /// persist the local cache after an incremental patch.
    pub async fn snapshot(&self) -> Vec<Mapping> {
        self.by_device
            .read()
            .await
            .values()
            .flatten()
            .cloned()
            .collect()
    }

    /// Apply the given input primitive from a specific device, returning every
    /// intent it produces across all matching mappings.
    pub async fn route(
        &self,
        device_type: &str,
        device_id: &str,
        input: &InputPrimitive,
    ) -> Vec<RoutedIntent> {
        let guard = self.by_device.read().await;
        let Some(mappings) = guard.get(&(device_type.to_string(), device_id.to_string())) else {
            return Vec::new();
        };
        route_mappings(mappings, input)
    }

    /// Same dispatch as `route`, but also implements the target-selection
    /// state machine (`Mapping.target_switch_on` + `target_candidates`).
    /// Callers that want on-device target switching should use this
    /// instead of `route`.
    pub async fn route_with_mode(
        &self,
        device_type: &str,
        device_id: &str,
        input: &InputPrimitive,
    ) -> RouteOutcome {
        let key = (device_type.to_string(), device_id.to_string());

        // Already in selection mode for this device: intercept rotate /
        // press / cancel before dispatching to normal routing.
        {
            let mut sel = self.selection.write().await;
            if let Some(mode) = sel.get_mut(&key) {
                match input {
                    InputPrimitive::Rotate { delta } => {
                        mode.advance(*delta);
                        let glyph = mode.current().glyph.clone();
                        let mapping_id = mode.mapping_id;
                        return RouteOutcome::UpdateSelection { mapping_id, glyph };
                    }
                    InputPrimitive::Press => {
                        let mapping_id = mode.mapping_id;
                        let service_target = mode.current().target.clone();
                        let edge_id = mode.edge_id.clone();
                        sel.remove(&key);
                        return RouteOutcome::CommitSelection {
                            edge_id,
                            mapping_id,
                            service_target,
                        };
                    }
                    _ => {
                        let mapping_id = mode.mapping_id;
                        sel.remove(&key);
                        return RouteOutcome::CancelSelection { mapping_id };
                    }
                }
            }
        }

        // Not in mode yet: check whether this input should enter mode on
        // any mapping for this device, else fall through to normal routing.
        let guard = self.by_device.read().await;
        let Some(mappings) = guard.get(&key) else {
            return RouteOutcome::Normal(Vec::new());
        };

        for m in mappings {
            if !m.active {
                continue;
            }
            let Some(switch_on) = m.target_switch_on.as_deref() else {
                continue;
            };
            if m.target_candidates.is_empty() {
                continue;
            }
            if !input.matches_route(switch_on) {
                continue;
            }
            // Enter mode with cursor one step AFTER the current
            // service_target — so swipe_up→press (no rotate) cycles to
            // the next candidate. Rotate still browses from there.
            let current_idx = m
                .target_candidates
                .iter()
                .position(|c| c.target == m.service_target);
            let cursor = match current_idx {
                Some(i) => (i + 1) % m.target_candidates.len(),
                None => 0,
            };
            let mode = SelectionMode {
                mapping_id: m.mapping_id,
                edge_id: m.edge_id.clone(),
                candidates: m.target_candidates.clone(),
                cursor,
            };
            let glyph = mode.current().glyph.clone();
            let mapping_id = mode.mapping_id;
            let edge_id = mode.edge_id.clone();
            drop(guard);
            self.selection.write().await.insert(key, mode);
            return RouteOutcome::EnterSelection {
                edge_id,
                mapping_id,
                glyph,
            };
        }

        RouteOutcome::Normal(route_mappings(mappings, input))
    }
}

fn route_mappings(mappings: &[Mapping], input: &InputPrimitive) -> Vec<RoutedIntent> {
    let mut out = Vec::new();
    for m in mappings {
        if !m.active {
            continue;
        }
        for route in &m.routes {
            if !input.matches_route(&route.input) {
                continue;
            }
            if let Some(intent) = build_intent(route, input) {
                out.push(RoutedIntent {
                    service_type: m.service_type.clone(),
                    service_target: m.service_target.clone(),
                    intent,
                });
                // Only first matching route per mapping fires.
                break;
            }
        }
    }
    out
}

fn build_intent(route: &Route, input: &InputPrimitive) -> Option<Intent> {
    let damping = route
        .params
        .get("damping")
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0);

    match route.intent.as_str() {
        "play" => Some(Intent::Play),
        "pause" => Some(Intent::Pause),
        "play_pause" | "playpause" => Some(Intent::PlayPause),
        "stop" => Some(Intent::Stop),
        "next" => Some(Intent::Next),
        "previous" => Some(Intent::Previous),
        "mute" => Some(Intent::Mute),
        "unmute" => Some(Intent::Unmute),
        "power_toggle" => Some(Intent::PowerToggle),
        "power_on" => Some(Intent::PowerOn),
        "power_off" => Some(Intent::PowerOff),
        "volume_change" => input
            .continuous_value()
            .map(|v| Intent::VolumeChange { delta: v * damping }),
        "volume_set" => route
            .params
            .get("value")
            .and_then(|v| v.as_f64())
            .map(|value| Intent::VolumeSet { value }),
        "seek_relative" => input.continuous_value().map(|v| Intent::SeekRelative {
            seconds: v * damping,
        }),
        "brightness_change" => input
            .continuous_value()
            .map(|v| Intent::BrightnessChange { delta: v * damping }),
        "color_temperature_change" => input
            .continuous_value()
            .map(|v| Intent::ColorTemperatureChange { delta: v * damping }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edge_core::Direction;
    use std::collections::BTreeMap;
    use uuid::Uuid;
    use weave_contracts::Route;

    fn rotate_mapping() -> Mapping {
        Mapping {
            mapping_id: Uuid::new_v4(),
            edge_id: "living-room".into(),
            device_type: "nuimo".into(),
            device_id: "C3:81:DF:4E".into(),
            service_type: "roon".into(),
            service_target: "zone-1".into(),
            routes: vec![Route {
                input: "rotate".into(),
                intent: "volume_change".into(),
                params: BTreeMap::from([("damping".into(), serde_json::json!(80.0))]),
            }],
            feedback: vec![],
            active: true,
            target_candidates: vec![],
            target_switch_on: None,
        }
    }

    #[tokio::test]
    async fn rotate_produces_volume_change_with_damping() {
        let engine = RoutingEngine::new();
        engine.replace_all(vec![rotate_mapping()]).await;

        let out = engine
            .route(
                "nuimo",
                "C3:81:DF:4E",
                &InputPrimitive::Rotate { delta: 0.03 },
            )
            .await;
        assert_eq!(out.len(), 1);
        match &out[0].intent {
            Intent::VolumeChange { delta } => assert!((*delta - 2.4).abs() < 0.001),
            other => panic!("expected VolumeChange, got {:?}", other),
        }
        assert_eq!(out[0].service_type, "roon");
        assert_eq!(out[0].service_target, "zone-1");
    }

    #[tokio::test]
    async fn inactive_mappings_are_skipped() {
        let mut m = rotate_mapping();
        m.active = false;
        let engine = RoutingEngine::new();
        engine.replace_all(vec![m]).await;

        let out = engine
            .route(
                "nuimo",
                "C3:81:DF:4E",
                &InputPrimitive::Rotate { delta: 0.03 },
            )
            .await;
        assert!(out.is_empty());
    }

    #[tokio::test]
    async fn unknown_device_returns_empty() {
        let engine = RoutingEngine::new();
        engine.replace_all(vec![rotate_mapping()]).await;

        let out = engine
            .route("nuimo", "unknown", &InputPrimitive::Rotate { delta: 0.03 })
            .await;
        assert!(out.is_empty());
    }

    fn selection_mapping() -> Mapping {
        let mut m = rotate_mapping();
        m.service_target = "target-A".into();
        m.target_switch_on = Some("swipe_up".into());
        m.target_candidates = vec![
            TargetCandidate {
                target: "target-A".into(),
                label: "A".into(),
                glyph: "glyph-a".into(),
            },
            TargetCandidate {
                target: "target-B".into(),
                label: "B".into(),
                glyph: "glyph-b".into(),
            },
        ];
        m
    }

    #[tokio::test]
    async fn swipe_up_press_cycles_to_next_target_without_rotate() {
        let engine = RoutingEngine::new();
        engine.replace_all(vec![selection_mapping()]).await;

        match engine
            .route_with_mode(
                "nuimo",
                "C3:81:DF:4E",
                &InputPrimitive::Swipe {
                    direction: Direction::Up,
                },
            )
            .await
        {
            RouteOutcome::EnterSelection { glyph, .. } => {
                // Entering mode from target-A should point at target-B.
                assert_eq!(glyph, "glyph-b");
            }
            other => panic!("expected EnterSelection, got {:?}", other),
        }

        match engine
            .route_with_mode("nuimo", "C3:81:DF:4E", &InputPrimitive::Press)
            .await
        {
            RouteOutcome::CommitSelection { service_target, .. } => {
                assert_eq!(service_target, "target-B")
            }
            other => panic!("expected CommitSelection, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn rotate_then_press_commits_advanced_candidate() {
        let engine = RoutingEngine::new();
        engine.replace_all(vec![selection_mapping()]).await;

        // Enter: cursor jumps to target-B (position 1).
        let _ = engine
            .route_with_mode(
                "nuimo",
                "C3:81:DF:4E",
                &InputPrimitive::Swipe {
                    direction: Direction::Up,
                },
            )
            .await;
        // Rotate forward: wraps from B back to A.
        match engine
            .route_with_mode(
                "nuimo",
                "C3:81:DF:4E",
                &InputPrimitive::Rotate { delta: 0.1 },
            )
            .await
        {
            RouteOutcome::UpdateSelection { glyph, .. } => assert_eq!(glyph, "glyph-a"),
            other => panic!("expected UpdateSelection, got {:?}", other),
        }
        match engine
            .route_with_mode("nuimo", "C3:81:DF:4E", &InputPrimitive::Press)
            .await
        {
            RouteOutcome::CommitSelection { service_target, .. } => {
                assert_eq!(service_target, "target-A")
            }
            other => panic!("expected CommitSelection, got {:?}", other),
        }
    }
}
