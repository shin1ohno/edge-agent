//! In-memory glyph registry populated from `ConfigFull`.
//!
//! Consumers look up patterns by name (e.g. "play", "pause", "volume_bar").
//! `builtin` glyphs carry an empty pattern; the consumer is expected to
//! render them programmatically (`volume_bar` scales with percentage).

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;
use weave_contracts::Glyph;

#[derive(Default, Clone)]
pub struct GlyphRegistry {
    inner: Arc<RwLock<HashMap<String, Glyph>>>,
}

impl GlyphRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn replace_all(&self, glyphs: Vec<Glyph>) {
        let map: HashMap<String, Glyph> = glyphs.into_iter().map(|g| (g.name.clone(), g)).collect();
        *self.inner.write().await = map;
    }

    pub async fn get(&self, name: &str) -> Option<Glyph> {
        self.inner.read().await.get(name).cloned()
    }

    pub async fn len(&self) -> usize {
        self.inner.read().await.len()
    }

    pub async fn is_empty(&self) -> bool {
        self.inner.read().await.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn replace_all_overwrites() {
        let r = GlyphRegistry::new();
        assert!(r.is_empty().await);
        r.replace_all(vec![Glyph {
            name: "play".into(),
            pattern: "*".into(),
            builtin: false,
        }])
        .await;
        assert_eq!(r.len().await, 1);
        assert_eq!(r.get("play").await.unwrap().pattern, "*");

        r.replace_all(vec![Glyph {
            name: "pause".into(),
            pattern: "**".into(),
            builtin: false,
        }])
        .await;
        assert_eq!(r.len().await, 1);
        assert!(r.get("play").await.is_none());
    }
}
