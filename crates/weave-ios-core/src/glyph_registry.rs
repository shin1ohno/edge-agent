//! In-memory registry of named LED glyphs pushed by weave-server.
//!
//! Populated from `ServerToEdge::ConfigFull { config.glyphs }` and refreshed
//! by `ServerToEdge::GlyphsUpdate`. Read by the feedback pump to resolve
//! `FeedbackPlan::NamedGlyph(name)` into a renderable 9x9 grid.
//!
//! `volume_bar` and other `builtin: true` entries arrive with an empty
//! `pattern` and a runtime renderer (e.g. `nuimo_protocol::volume_bars`)
//! handles the actual bitmap. The registry surfaces the metadata so the
//! pump knows whether to consult `pattern` or skip the lookup.
//!
//! Lock granularity: a single `RwLock<HashMap>` since updates are rare
//! (one per server reconnect or per GlyphsUpdate frame) and reads are
//! short.

use std::collections::HashMap;

use tokio::sync::RwLock;
use weave_contracts::Glyph;

#[derive(Default)]
pub(crate) struct GlyphRegistry {
    by_name: RwLock<HashMap<String, Glyph>>,
}

impl GlyphRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace every entry. Used on `ConfigFull` (server pushes the
    /// authoritative full set) and on `GlyphsUpdate` (server treats the
    /// payload as the new full set, not a delta).
    pub async fn replace_all(&self, glyphs: Vec<Glyph>) {
        let map: HashMap<String, Glyph> = glyphs.into_iter().map(|g| (g.name.clone(), g)).collect();
        *self.by_name.write().await = map;
    }

    /// Look up a glyph by name. Returns `None` if the registry has no
    /// entry — the pump treats that as "skip rendering" rather than
    /// rendering a blank.
    pub async fn get(&self, name: &str) -> Option<Glyph> {
        self.by_name.read().await.get(name).cloned()
    }

    /// Snapshot every entry as a flat `Vec`. Used by the cache layer
    /// to persist the registry to disk; the order is unspecified
    /// because `replace_all` keys on `name`.
    pub async fn snapshot(&self) -> Vec<Glyph> {
        self.by_name.read().await.values().cloned().collect()
    }

    #[cfg(test)]
    pub async fn len(&self) -> usize {
        self.by_name.read().await.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn glyph(name: &str, pattern: &str) -> Glyph {
        Glyph {
            name: name.into(),
            pattern: pattern.into(),
            builtin: false,
        }
    }

    #[tokio::test]
    async fn replace_all_then_get_returns_pattern() {
        let registry = GlyphRegistry::new();
        registry
            .replace_all(vec![
                glyph("play", "         \n    *    \n   ***   "),
                glyph("pause", "         \n  ** **  "),
            ])
            .await;

        let got = registry.get("play").await.expect("play registered");
        assert_eq!(got.name, "play");
        assert!(got.pattern.contains("***"));
    }

    #[tokio::test]
    async fn replace_all_overwrites_prior_entries() {
        let registry = GlyphRegistry::new();
        registry.replace_all(vec![glyph("play", "v1")]).await;
        registry.replace_all(vec![glyph("pause", "v2")]).await;

        assert!(
            registry.get("play").await.is_none(),
            "replace_all is not a delta — old entries must be cleared"
        );
        assert_eq!(registry.get("pause").await.unwrap().pattern, "v2");
    }

    #[tokio::test]
    async fn get_unknown_name_returns_none() {
        let registry = GlyphRegistry::new();
        registry.replace_all(vec![glyph("play", "*")]).await;
        assert!(registry.get("missing").await.is_none());
    }

    #[tokio::test]
    async fn empty_replace_clears_registry() {
        let registry = GlyphRegistry::new();
        registry.replace_all(vec![glyph("play", "*")]).await;
        registry.replace_all(vec![]).await;
        assert_eq!(registry.len().await, 0);
    }
}
