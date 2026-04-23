//! WebSocket client to `weave-server` `/ws/edge`.
//!
//! Runs a long-lived reconnect loop: after connect, sends a `Hello` frame,
//! reads frames from the server (updating the `RoutingEngine` and local cache
//! on `ConfigFull`/`ConfigPatch`), and forwards outbound `EdgeToServer`
//! frames produced by adapters.
//!
//! On unreachable server, the agent loads the cached config so local routing
//! keeps working; the reconnect loop retries in the background.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::sync::{broadcast, mpsc};
use tokio_tungstenite::tungstenite::protocol::Message;
use weave_contracts::{EdgeConfig, EdgeToServer, PatchOp, ServerToEdge};

use super::cache;
use super::registry::GlyphRegistry;
use super::routing::RoutingEngine;

const RECONNECT_INITIAL_DELAY: Duration = Duration::from_secs(2);
const RECONNECT_MAX_DELAY: Duration = Duration::from_secs(30);

pub struct WsClient {
    url: String,
    edge_id: String,
    version: String,
    capabilities: Vec<String>,
    engine: Arc<RoutingEngine>,
    glyphs: Arc<GlyphRegistry>,
    cache_path: PathBuf,
    outbox_rx: mpsc::Receiver<EdgeToServer>,
    outbox_tx: mpsc::Sender<EdgeToServer>,
    resync_tx: broadcast::Sender<()>,
}

impl WsClient {
    pub fn new(
        url: String,
        edge_id: String,
        version: String,
        capabilities: Vec<String>,
        engine: Arc<RoutingEngine>,
        glyphs: Arc<GlyphRegistry>,
    ) -> Self {
        let cache_path = cache::default_cache_path(&edge_id);
        let (outbox_tx, outbox_rx) = mpsc::channel(256);
        // Small buffer is fine — subscribers only care about the latest
        // reconnect event; lagged ones can be dropped silently.
        let (resync_tx, _) = broadcast::channel(8);
        Self {
            url,
            edge_id,
            version,
            capabilities,
            engine,
            glyphs,
            cache_path,
            outbox_rx,
            outbox_tx,
            resync_tx,
        }
    }

    /// Get a sender for outbound `EdgeToServer` frames. Clone as many times
    /// as needed; adapters publish state updates via this channel.
    pub fn outbox(&self) -> mpsc::Sender<EdgeToServer> {
        self.outbox_tx.clone()
    }

    /// Clone the `ws/edge` (re)connect broadcaster. Fires once per
    /// successful connect + Hello exchange. State-pumps subscribe to this
    /// and replay the most recent frame per
    /// (service_type, target, property, output_id) key so weave-server
    /// recovers its full snapshot after a restart — otherwise idle zones
    /// / lights that haven't changed since the last connect disappear
    /// from the UI because the adapter's source-side dedup suppresses
    /// re-sends.
    pub fn resync_sender(&self) -> broadcast::Sender<()> {
        self.resync_tx.clone()
    }

    /// Populate the routing engine + glyph registry from the local cache, if
    /// one exists. Call this once at startup before entering `run()`.
    pub async fn prime_from_cache(&self) -> anyhow::Result<()> {
        if let Some(cfg) = cache::load(&self.cache_path).await? {
            tracing::info!(
                mappings = cfg.mappings.len(),
                glyphs = cfg.glyphs.len(),
                path = %self.cache_path.display(),
                "primed routing engine from cache",
            );
            self.engine.replace_all(cfg.mappings).await;
            self.glyphs.replace_all(cfg.glyphs).await;
        }
        Ok(())
    }

    /// Run the reconnect loop. Never returns under normal operation.
    pub async fn run(mut self) {
        let mut delay = RECONNECT_INITIAL_DELAY;
        loop {
            match self.connect_once().await {
                Ok(_) => {
                    tracing::info!("ws session ended cleanly; reconnecting");
                    delay = RECONNECT_INITIAL_DELAY;
                }
                Err(e) => {
                    tracing::warn!(error = %e, delay_secs = delay.as_secs(), "ws session failed");
                }
            }
            tokio::time::sleep(delay).await;
            delay = (delay * 2).min(RECONNECT_MAX_DELAY);
        }
    }

    async fn connect_once(&mut self) -> anyhow::Result<()> {
        let (ws, _) = tokio_tungstenite::connect_async(&self.url).await?;
        tracing::info!(url = %self.url, "ws connected");
        let (mut tx, mut rx) = ws.split();

        let hello = EdgeToServer::Hello {
            edge_id: self.edge_id.clone(),
            version: self.version.clone(),
            capabilities: self.capabilities.clone(),
        };
        tx.send(Message::Text(serde_json::to_string(&hello)?))
            .await?;

        // Fire after the Hello is on the wire so subscribers replay only
        // once the server is ready to accept frames. `Err` here just means
        // no live subscribers yet, which is fine.
        let _ = self.resync_tx.send(());

        loop {
            tokio::select! {
                incoming = rx.next() => {
                    let Some(msg) = incoming else { return Ok(()); };
                    let msg = msg?;
                    match msg {
                        Message::Text(t) => self.handle_server_frame(&t).await?,
                        Message::Binary(_) => continue,
                        Message::Ping(p) => tx.send(Message::Pong(p)).await?,
                        Message::Pong(_) => continue,
                        Message::Close(_) => return Ok(()),
                        Message::Frame(_) => continue,
                    }
                }
                outbound = self.outbox_rx.recv() => {
                    let Some(frame) = outbound else { return Ok(()); };
                    tx.send(Message::Text(serde_json::to_string(&frame)?)).await?;
                }
            }
        }
    }

    async fn handle_server_frame(&self, text: &str) -> anyhow::Result<()> {
        let frame: ServerToEdge = serde_json::from_str(text)?;
        match frame {
            ServerToEdge::ConfigFull { config } => {
                tracing::info!(
                    mappings = config.mappings.len(),
                    glyphs = config.glyphs.len(),
                    edge_id = %config.edge_id,
                    "received config_full",
                );
                self.apply_full(&config).await;
                let _ = cache::save(&self.cache_path, &config).await;
            }
            ServerToEdge::ConfigPatch {
                mapping_id,
                op,
                mapping,
            } => match op {
                PatchOp::Upsert => {
                    if let Some(m) = mapping {
                        tracing::info!(
                            %mapping_id,
                            device = %m.device_id,
                            service = %m.service_type,
                            "config_patch upsert",
                        );
                        self.engine.upsert_mapping(m).await;
                        self.refresh_cache().await;
                    } else {
                        tracing::warn!(%mapping_id, "config_patch upsert without mapping payload; ignoring");
                    }
                }
                PatchOp::Delete => {
                    tracing::info!(%mapping_id, "config_patch delete");
                    self.engine.remove_mapping(&mapping_id).await;
                    self.refresh_cache().await;
                }
            },
            ServerToEdge::TargetSwitch {
                mapping_id,
                service_target,
            } => {
                // Express as an upsert of the current mapping with the new
                // service_target. Cheap since we already have it locally.
                tracing::info!(%mapping_id, %service_target, "target_switch");
                let mut current = self.engine.snapshot().await;
                if let Some(idx) = current.iter().position(|m| m.mapping_id == mapping_id) {
                    current[idx].service_target = service_target;
                    self.engine.upsert_mapping(current.remove(idx)).await;
                    self.refresh_cache().await;
                } else {
                    tracing::warn!(%mapping_id, "target_switch for unknown mapping");
                }
            }
            ServerToEdge::GlyphsUpdate { glyphs } => {
                tracing::info!(count = glyphs.len(), "received glyphs_update");
                self.glyphs.replace_all(glyphs).await;
            }
            ServerToEdge::Ping => {
                // Pong is handled via the outbox channel to avoid tx contention here;
                // fire-and-forget.
                let _ = self.outbox_tx.try_send(EdgeToServer::Pong);
            }
        }
        Ok(())
    }

    async fn apply_full(&self, config: &EdgeConfig) {
        self.engine.replace_all(config.mappings.clone()).await;
        self.glyphs.replace_all(config.glyphs.clone()).await;
    }

    /// Persist a fresh cache after an incremental patch so the agent
    /// comes back up with the latest config even if the server is
    /// unreachable on the next boot.
    async fn refresh_cache(&self) {
        let mappings = self.engine.snapshot().await;
        // The cache stores an EdgeConfig; we need edge_id + current glyphs.
        // Glyphs aren't kept in a cheap-to-read form on the engine, so
        // derive from the last saved cache if present.
        let edge_id = self.edge_id.clone();
        let glyphs = match cache::load(&self.cache_path).await {
            Ok(Some(cfg)) => cfg.glyphs,
            _ => Vec::new(),
        };
        let cfg = EdgeConfig {
            edge_id,
            mappings,
            glyphs,
        };
        if let Err(e) = cache::save(&self.cache_path, &cfg).await {
            tracing::warn!(error = %e, "failed to persist cache after patch");
        }
    }
}
