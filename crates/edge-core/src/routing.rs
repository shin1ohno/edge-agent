//! Routing engine: applies a device input primitive against the active
//! mappings to produce zero or more `(service_type, target, intent)` triples.

use std::collections::HashMap;

use tokio::sync::RwLock;
use weave_contracts::{Mapping, Route};

use crate::intent::{InputPrimitive, Intent};

/// A concrete intent destined for a specific service target.
#[derive(Debug, Clone)]
pub struct RoutedIntent {
    pub service_type: String,
    pub service_target: String,
    pub intent: Intent,
}

/// Thread-safe registry of currently-active mappings, keyed by
/// `(device_type, device_id)` for O(1) lookup per input event.
#[derive(Default)]
pub struct RoutingEngine {
    by_device: RwLock<HashMap<(String, String), Vec<Mapping>>>,
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
}
