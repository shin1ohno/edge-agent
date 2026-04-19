//! Service adapter trait. Each concrete adapter (Roon, Hue, ...) implements
//! this to accept intents and publish state updates.

use async_trait::async_trait;
use tokio::sync::broadcast;

use crate::intent::Intent;

/// State update from an external service. The edge-agent forwards these
/// over WebSocket to the config-server for fan-out to the Web UI, and
/// can also drive local device feedback (LED glyphs, etc.).
#[derive(Debug, Clone)]
pub struct StateUpdate {
    pub service_type: String,
    pub target: String,
    pub property: String,
    pub output_id: Option<String>,
    pub value: serde_json::Value,
}

#[async_trait]
pub trait ServiceAdapter: Send + Sync {
    /// Stable identifier used to match against `Mapping.service_type`.
    fn service_type(&self) -> &'static str;

    /// Send an intent to a specific target (zone_id, group_id, ...).
    async fn send_intent(&self, target: &str, intent: &Intent) -> anyhow::Result<()>;

    /// Subscribe to state updates from this adapter.
    fn subscribe_state(&self) -> broadcast::Receiver<StateUpdate>;
}
