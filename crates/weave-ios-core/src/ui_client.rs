//! `UiClient` — WebSocket `/ws/ui` + REST `/api/...` client for the iOS app.
//!
//! Frames arrive from `/ws/ui` as JSON and are forwarded to Swift verbatim
//! via [`UiEventSink::on_frame_json`]. Swift side decodes with `Codable`.
//! REST calls return JSON strings Swift also decodes with `Codable`. This
//! avoids mirroring the entire `weave-contracts` type surface through
//! UniFFI records.
//!
//! Note: avoid writing `/api/*` (with a literal asterisk) in doc comments —
//! when UniFFI copies the doc text into the generated Swift `/** ... */`
//! block, Swift's nested-comment rules see `/*` and open an inner block
//! that never closes, producing a compile error.

use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::sync::{mpsc, Mutex};
use tokio_tungstenite::tungstenite::protocol::Message;

use crate::WeaveError;

/// Swift-implemented callback. All `/ws/ui` frames arrive here as JSON
/// strings; Swift decodes via `Codable` types that mirror
/// `weave_contracts::UiFrame`.
///
/// Uses `with_foreign` so Swift conforms to the trait and the instance can
/// be held as `Arc<dyn UiEventSink>` for the WS loop's lifetime.
#[uniffi::export(with_foreign)]
pub trait UiEventSink: Send + Sync {
    fn on_frame_json(&self, json: String);
    fn on_connection_changed(&self, connected: bool);
}

#[derive(uniffi::Object)]
pub struct UiClient {
    base_url: String,
    http: reqwest::Client,
    shutdown_tx: Mutex<Option<mpsc::Sender<()>>>,
}

#[uniffi::export(async_runtime = "tokio")]
impl UiClient {
    /// Connect to weave-server. `server_url` must include scheme + port,
    /// e.g. `"http://pro.home.local:3100"`. The same origin serves
    /// `/ws/ui` and the REST API; we derive `ws://` / `wss://` variants.
    ///
    /// The WebSocket loop is spawned on the tokio runtime UniFFI hosts for
    /// this crate; callers hold the returned `Arc` and invoke
    /// [`UiClient::shutdown`] before dropping to stop the loop cleanly.
    #[uniffi::constructor]
    pub async fn connect(
        server_url: String,
        sink: Arc<dyn UiEventSink>,
    ) -> Result<Arc<Self>, WeaveError> {
        let base = normalize_base(&server_url)?;

        let http = reqwest::Client::builder()
            .user_agent(concat!("weave-ios/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| WeaveError::Network {
                message: e.to_string(),
            })?;

        let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>(1);

        let ws_url = derive_ws_url(&base)?;
        tokio::spawn(run_ws_loop(ws_url, sink, shutdown_rx));

        Ok(Arc::new(Self {
            base_url: base,
            http,
            shutdown_tx: Mutex::new(Some(shutdown_tx)),
        }))
    }

    /// Signal the WS loop to exit and release its sink reference. Safe to
    /// call multiple times.
    pub async fn shutdown(&self) {
        let tx = self.shutdown_tx.lock().await.take();
        if let Some(tx) = tx {
            let _ = tx.send(()).await;
        }
    }

    // ----- REST: mappings ---------------------------------------------------

    pub async fn list_mappings_json(&self) -> Result<String, WeaveError> {
        self.get_text("/api/mappings").await
    }

    pub async fn get_mapping_json(&self, id: String) -> Result<String, WeaveError> {
        self.get_text(&format!("/api/mappings/{id}")).await
    }

    pub async fn create_mapping(&self, mapping_json: String) -> Result<String, WeaveError> {
        self.post_json("/api/mappings", &mapping_json).await
    }

    pub async fn update_mapping(
        &self,
        id: String,
        mapping_json: String,
    ) -> Result<String, WeaveError> {
        self.put_json(&format!("/api/mappings/{id}"), &mapping_json)
            .await
    }

    pub async fn delete_mapping(&self, id: String) -> Result<(), WeaveError> {
        self.delete(&format!("/api/mappings/{id}")).await
    }

    pub async fn switch_target(
        &self,
        id: String,
        service_target: String,
    ) -> Result<String, WeaveError> {
        let body = serde_json::json!({ "service_target": service_target }).to_string();
        self.post_json(&format!("/api/mappings/{id}/target"), &body)
            .await
    }

    // ----- REST: glyphs -----------------------------------------------------

    pub async fn list_glyphs_json(&self) -> Result<String, WeaveError> {
        self.get_text("/api/glyphs").await
    }

    pub async fn upsert_glyph(&self, name: String, glyph_json: String) -> Result<String, WeaveError> {
        self.put_json(&format!("/api/glyphs/{name}"), &glyph_json)
            .await
    }

    pub async fn delete_glyph(&self, name: String) -> Result<(), WeaveError> {
        self.delete(&format!("/api/glyphs/{name}")).await
    }
}

// ----- Internal HTTP helpers ----------------------------------------------

impl UiClient {
    async fn get_text(&self, path: &str) -> Result<String, WeaveError> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self.http.get(&url).send().await.map_err(net)?;
        raise_for_status(&resp)?;
        resp.text().await.map_err(net)
    }

    async fn post_json(&self, path: &str, body: &str) -> Result<String, WeaveError> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .http
            .post(&url)
            .header("content-type", "application/json")
            .body(body.to_owned())
            .send()
            .await
            .map_err(net)?;
        raise_for_status(&resp)?;
        resp.text().await.map_err(net)
    }

    async fn put_json(&self, path: &str, body: &str) -> Result<String, WeaveError> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .http
            .put(&url)
            .header("content-type", "application/json")
            .body(body.to_owned())
            .send()
            .await
            .map_err(net)?;
        raise_for_status(&resp)?;
        resp.text().await.map_err(net)
    }

    async fn delete(&self, path: &str) -> Result<(), WeaveError> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self.http.delete(&url).send().await.map_err(net)?;
        raise_for_status(&resp)?;
        Ok(())
    }
}

fn net(e: reqwest::Error) -> WeaveError {
    WeaveError::Network {
        message: e.to_string(),
    }
}

fn raise_for_status(resp: &reqwest::Response) -> Result<(), WeaveError> {
    if resp.status().is_success() {
        Ok(())
    } else {
        Err(WeaveError::Http {
            status: resp.status().as_u16(),
            message: resp.status().canonical_reason().unwrap_or("").to_string(),
        })
    }
}

// ----- URL normalization --------------------------------------------------

fn normalize_base(url: &str) -> Result<String, WeaveError> {
    let trimmed = url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err(WeaveError::Network {
            message: "empty server URL".into(),
        });
    }
    if !(trimmed.starts_with("http://")
        || trimmed.starts_with("https://")
        || trimmed.starts_with("ws://")
        || trimmed.starts_with("wss://"))
    {
        return Err(WeaveError::Network {
            message: format!("server URL must have a scheme: {trimmed}"),
        });
    }
    // Normalize ws[s] → http[s] for the REST base URL.
    let base = trimmed
        .replacen("ws://", "http://", 1)
        .replacen("wss://", "https://", 1);
    Ok(base)
}

fn derive_ws_url(http_base: &str) -> Result<String, WeaveError> {
    let ws_base = http_base
        .replacen("http://", "ws://", 1)
        .replacen("https://", "wss://", 1);
    Ok(format!("{ws_base}/ws/ui"))
}

// ----- WebSocket loop -----------------------------------------------------

async fn run_ws_loop(
    url: String,
    sink: Arc<dyn UiEventSink>,
    mut shutdown_rx: mpsc::Receiver<()>,
) {
    let mut backoff = Duration::from_millis(500);
    let max_backoff = Duration::from_secs(15);

    loop {
        tokio::select! {
            _ = shutdown_rx.recv() => return,
            res = tokio_tungstenite::connect_async(&url) => {
                match res {
                    Ok((mut ws, _resp)) => {
                        tracing::info!(url = %url, "ws/ui connected");
                        sink.on_connection_changed(true);
                        backoff = Duration::from_millis(500);

                        loop {
                            tokio::select! {
                                _ = shutdown_rx.recv() => {
                                    let _ = ws.send(Message::Close(None)).await;
                                    sink.on_connection_changed(false);
                                    return;
                                }
                                msg = ws.next() => {
                                    match msg {
                                        Some(Ok(Message::Text(text))) => {
                                            sink.on_frame_json(text.to_string());
                                        }
                                        Some(Ok(Message::Binary(_))) => {
                                            // Ignore; /ws/ui speaks JSON text only.
                                        }
                                        Some(Ok(Message::Ping(p))) => {
                                            let _ = ws.send(Message::Pong(p)).await;
                                        }
                                        Some(Ok(_)) => {} // Pong, Frame
                                        Some(Err(e)) => {
                                            tracing::warn!(error = %e, "ws/ui read error");
                                            break;
                                        }
                                        None => break,
                                    }
                                }
                            }
                        }
                        sink.on_connection_changed(false);
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, url = %url, "ws/ui connect failed");
                    }
                }
            }
        }

        // Backoff before retry. Interruptible by shutdown.
        tokio::select! {
            _ = shutdown_rx.recv() => return,
            _ = tokio::time::sleep(backoff) => {
                backoff = (backoff * 2).min(max_backoff);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_strips_trailing_slash_and_passes_http() {
        assert_eq!(
            normalize_base("http://host:3100/").unwrap(),
            "http://host:3100"
        );
        assert_eq!(
            normalize_base("http://host:3100").unwrap(),
            "http://host:3100"
        );
    }

    #[test]
    fn normalize_rewrites_ws_to_http() {
        assert_eq!(
            normalize_base("ws://host:3100").unwrap(),
            "http://host:3100"
        );
        assert_eq!(
            normalize_base("wss://host/").unwrap(),
            "https://host"
        );
    }

    #[test]
    fn normalize_rejects_missing_scheme() {
        assert!(matches!(
            normalize_base("host:3100"),
            Err(WeaveError::Network { .. })
        ));
    }

    #[test]
    fn derive_ws_appends_ws_ui_path() {
        assert_eq!(
            derive_ws_url("http://host:3100").unwrap(),
            "ws://host:3100/ws/ui"
        );
        assert_eq!(
            derive_ws_url("https://host").unwrap(),
            "wss://host/ws/ui"
        );
    }
}
