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
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::protocol::Message;
use weave_contracts::{EdgeConfig, EdgeToServer, ServerToEdge};

use crate::cache;
use crate::routing::RoutingEngine;

const RECONNECT_INITIAL_DELAY: Duration = Duration::from_secs(2);
const RECONNECT_MAX_DELAY: Duration = Duration::from_secs(30);

pub struct WsClient {
    url: String,
    edge_id: String,
    version: String,
    capabilities: Vec<String>,
    engine: Arc<RoutingEngine>,
    cache_path: PathBuf,
    outbox_rx: mpsc::Receiver<EdgeToServer>,
    outbox_tx: mpsc::Sender<EdgeToServer>,
}

impl WsClient {
    pub fn new(
        url: String,
        edge_id: String,
        version: String,
        capabilities: Vec<String>,
        engine: Arc<RoutingEngine>,
    ) -> Self {
        let cache_path = cache::default_cache_path(&edge_id);
        let (outbox_tx, outbox_rx) = mpsc::channel(256);
        Self {
            url,
            edge_id,
            version,
            capabilities,
            engine,
            cache_path,
            outbox_rx,
            outbox_tx,
        }
    }

    /// Get a sender for outbound `EdgeToServer` frames. Clone as many times
    /// as needed; adapters publish state updates via this channel.
    pub fn outbox(&self) -> mpsc::Sender<EdgeToServer> {
        self.outbox_tx.clone()
    }

    /// Populate the routing engine from the local cache, if one exists.
    /// Call this once at startup before entering `run()`.
    pub async fn prime_from_cache(&self) -> anyhow::Result<()> {
        if let Some(cfg) = cache::load(&self.cache_path).await? {
            tracing::info!(
                mappings = cfg.mappings.len(),
                path = %self.cache_path.display(),
                "primed routing engine from cache",
            );
            self.engine.replace_all(cfg.mappings).await;
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
                    edge_id = %config.edge_id,
                    "received config_full",
                );
                self.apply_full(&config).await;
                let _ = cache::save(&self.cache_path, &config).await;
            }
            ServerToEdge::ConfigPatch {
                mapping_id,
                op,
                mapping: _,
            } => {
                // Phase 1 scope: request a full reload on any patch. Fine-grained
                // patching lands in Phase 3 once the routing engine exposes it.
                tracing::info!(?op, %mapping_id, "config_patch received; full reload on next connect");
            }
            ServerToEdge::TargetSwitch {
                mapping_id,
                service_target,
            } => {
                tracing::info!(%mapping_id, %service_target, "target_switch (phase 3 will apply inline)");
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
    }
}
