//! Lightweight light-state cache. Populated by `list_lights` at startup and
//! kept fresh by the SSE event stream. Used by `PowerToggle` to compute the
//! next `on` value without round-tripping to the bridge.

use std::collections::HashMap;

use tokio::sync::RwLock;

use super::types::{Dimming, Light, OnState};

#[derive(Default)]
pub struct LightCache {
    inner: RwLock<HashMap<String, Light>>,
}

impl LightCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn replace_all(&self, lights: Vec<Light>) {
        let map = lights.into_iter().map(|l| (l.id.clone(), l)).collect();
        *self.inner.write().await = map;
    }

    pub async fn values(&self) -> Vec<Light> {
        self.inner.read().await.values().cloned().collect()
    }

    pub async fn get(&self, id: &str) -> Option<Light> {
        self.inner.read().await.get(id).cloned()
    }

    /// Apply a partial SSE update (on and/or dimming). Returns the merged
    /// `Light` if the light was known; `None` otherwise.
    pub async fn merge_partial(
        &self,
        id: &str,
        on: Option<OnState>,
        dimming: Option<Dimming>,
    ) -> Option<Light> {
        let mut guard = self.inner.write().await;
        let entry = guard.get_mut(id)?;
        if let Some(o) = on {
            entry.on = o;
        }
        if let Some(d) = dimming {
            entry.dimming = Some(d);
        }
        Some(entry.clone())
    }
}
