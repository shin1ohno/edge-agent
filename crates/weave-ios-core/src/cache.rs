//! Local persistence for mappings + glyph registry on the iPad.
//!
//! Server-side `weave-server` is the authoritative editing surface,
//! but bootstrapping every routing decision off a live `/ws/edge`
//! connection means the iPad does nothing useful at app launch until
//! the server has handshaken back. This module mirrors the latest
//! `ConfigFull` / `ConfigPatch` / `GlyphsUpdate` to JSON files in
//! Application Support so the next launch can hydrate the routing
//! engine + glyph registry from disk before (or even without) talking
//! to the server.
//!
//! Files (relative to the directory the caller passes in):
//! - `mappings.json` — `Vec<weave_contracts::Mapping>` snapshot
//! - `glyphs.json`   — `Vec<weave_contracts::Glyph>` snapshot
//!
//! Writes are atomic via temp-file + rename. Decode errors fall
//! through to "empty + log" so a schema drift doesn't strand the
//! app — the next online connect overwrites the cache with fresh
//! content.

use std::path::Path;

use anyhow::Context;
use weave_contracts::{Glyph, Mapping};

use crate::glyph_registry::GlyphRegistry;
use edge_core::RoutingEngine;

const MAPPINGS_FILE: &str = "mappings.json";
const GLYPHS_FILE: &str = "glyphs.json";

/// Load whatever's on disk into the engine + registry. Missing files
/// or decode failures are non-fatal — the caller continues with an
/// empty engine / registry and waits for the server.
pub(crate) async fn hydrate_from_cache(dir: &Path, engine: &RoutingEngine, glyphs: &GlyphRegistry) {
    match read_json::<Vec<Mapping>>(&dir.join(MAPPINGS_FILE)).await {
        Ok(Some(m)) => {
            let n = m.len();
            engine.replace_all(m).await;
            tracing::info!(mapping_count = n, "cache: mappings hydrated");
        }
        Ok(None) => {}
        Err(e) => tracing::warn!(error = %e, "cache: mappings decode failed; starting empty"),
    }
    match read_json::<Vec<Glyph>>(&dir.join(GLYPHS_FILE)).await {
        Ok(Some(g)) => {
            let n = g.len();
            glyphs.replace_all(g).await;
            tracing::info!(glyph_count = n, "cache: glyphs hydrated");
        }
        Ok(None) => {}
        Err(e) => tracing::warn!(error = %e, "cache: glyphs decode failed; starting empty"),
    }
}

/// Snapshot the current engine + registry state to disk. Per-file
/// failures `tracing::warn!` and otherwise no-op so a transient I/O
/// error doesn't surface to the user — the in-memory state remains
/// authoritative for this run, and the next mutation tries the write
/// again.
pub(crate) async fn persist_cache(dir: &Path, engine: &RoutingEngine, glyphs: &GlyphRegistry) {
    if let Err(e) = tokio::fs::create_dir_all(dir).await {
        tracing::warn!(error = %e, "cache: mkdir failed; skipping persist");
        return;
    }
    let mappings = engine.snapshot().await;
    let glyphs_list = glyphs.snapshot().await;
    if let Err(e) = write_atomic(&dir.join(MAPPINGS_FILE), &mappings).await {
        tracing::warn!(error = %e, "cache: mappings write failed");
    }
    if let Err(e) = write_atomic(&dir.join(GLYPHS_FILE), &glyphs_list).await {
        tracing::warn!(error = %e, "cache: glyphs write failed");
    }
}

async fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> anyhow::Result<Option<T>> {
    match tokio::fs::read_to_string(path).await {
        Ok(s) => Ok(Some(serde_json::from_str(&s).context("decode")?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

async fn write_atomic<T: serde::Serialize>(target: &Path, value: &T) -> anyhow::Result<()> {
    let tmp = target.with_extension("json.tmp");
    let json = serde_json::to_string_pretty(value)?;
    tokio::fs::write(&tmp, json).await?;
    tokio::fs::rename(&tmp, target).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use tempfile::TempDir;
    use uuid::Uuid;
    use weave_contracts::{Mapping, Route};

    fn sample_mapping() -> Mapping {
        Mapping {
            mapping_id: Uuid::new_v4(),
            edge_id: "ipad".into(),
            device_type: "nuimo".into(),
            device_id: "f0dd0533-c396-fd91-5480-ddf761ef1eb0".into(),
            service_type: "ios_media".into(),
            service_target: "apple_music".into(),
            routes: vec![Route {
                input: "press".into(),
                intent: "play_pause".into(),
                params: BTreeMap::new(),
            }],
            feedback: vec![],
            active: true,
            target_candidates: vec![],
            target_switch_on: None,
        }
    }

    fn sample_glyph(name: &str) -> Glyph {
        Glyph {
            name: name.into(),
            pattern: "    *    \n   ***   \n  *****  ".into(),
            builtin: false,
        }
    }

    #[tokio::test]
    async fn hydrate_no_cache_is_silent() {
        let tmp = TempDir::new().unwrap();
        let engine = RoutingEngine::new();
        let glyphs = GlyphRegistry::new();

        hydrate_from_cache(tmp.path(), &engine, &glyphs).await;

        assert!(engine.snapshot().await.is_empty());
        assert_eq!(glyphs.len().await, 0);
    }

    #[tokio::test]
    async fn persist_then_hydrate_round_trips_mappings() {
        let tmp = TempDir::new().unwrap();
        let engine = RoutingEngine::new();
        let glyphs = GlyphRegistry::new();
        let mapping = sample_mapping();
        engine.replace_all(vec![mapping.clone()]).await;

        persist_cache(tmp.path(), &engine, &glyphs).await;

        let fresh_engine = RoutingEngine::new();
        let fresh_glyphs = GlyphRegistry::new();
        hydrate_from_cache(tmp.path(), &fresh_engine, &fresh_glyphs).await;

        let snap = fresh_engine.snapshot().await;
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].mapping_id, mapping.mapping_id);
        assert_eq!(snap[0].service_type, "ios_media");
    }

    #[tokio::test]
    async fn persist_then_hydrate_round_trips_glyphs() {
        let tmp = TempDir::new().unwrap();
        let engine = RoutingEngine::new();
        let glyphs = GlyphRegistry::new();
        glyphs
            .replace_all(vec![sample_glyph("play"), sample_glyph("pause")])
            .await;

        persist_cache(tmp.path(), &engine, &glyphs).await;

        let fresh_glyphs = GlyphRegistry::new();
        hydrate_from_cache(tmp.path(), &RoutingEngine::new(), &fresh_glyphs).await;
        assert_eq!(fresh_glyphs.len().await, 2);
        assert!(fresh_glyphs.get("play").await.is_some());
        assert!(fresh_glyphs.get("pause").await.is_some());
    }

    #[tokio::test]
    async fn corrupted_mappings_json_falls_through_without_blocking_glyphs() {
        let tmp = TempDir::new().unwrap();
        // Write garbage to mappings.json directly.
        tokio::fs::write(tmp.path().join("mappings.json"), b"{not json")
            .await
            .unwrap();
        // Write a valid glyphs.json by persisting a registry first
        // (uses the same atomic write the runtime path takes).
        let glyphs_seed = GlyphRegistry::new();
        glyphs_seed.replace_all(vec![sample_glyph("play")]).await;
        persist_cache(tmp.path(), &RoutingEngine::new(), &glyphs_seed).await;
        // Now overwrite mappings.json again with garbage (persist
        // wrote a fresh mappings.json above; we want the corrupted
        // version on disk for the test).
        tokio::fs::write(tmp.path().join("mappings.json"), b"{not json")
            .await
            .unwrap();

        let engine = RoutingEngine::new();
        let glyphs = GlyphRegistry::new();
        hydrate_from_cache(tmp.path(), &engine, &glyphs).await;

        assert!(
            engine.snapshot().await.is_empty(),
            "garbage mappings → empty engine, no panic"
        );
        assert_eq!(
            glyphs.len().await,
            1,
            "corrupt mappings file does not block glyphs hydration"
        );
    }

    #[tokio::test]
    async fn persist_overwrites_prior_state_and_leaves_no_tmp_file() {
        let tmp = TempDir::new().unwrap();
        let engine = RoutingEngine::new();
        let glyphs = GlyphRegistry::new();

        // First persist with one mapping.
        engine.replace_all(vec![sample_mapping()]).await;
        persist_cache(tmp.path(), &engine, &glyphs).await;

        // Second persist with no mappings — should overwrite, not append.
        engine.replace_all(vec![]).await;
        persist_cache(tmp.path(), &engine, &glyphs).await;

        let fresh = RoutingEngine::new();
        hydrate_from_cache(tmp.path(), &fresh, &GlyphRegistry::new()).await;
        assert!(fresh.snapshot().await.is_empty());

        // Atomic-write tempfiles must not linger.
        let leftovers: Vec<_> = std::fs::read_dir(tmp.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "tempfiles must be renamed, not left around"
        );
    }
}
