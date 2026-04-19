//! Roon service adapter backed by `roon-api`.
//!
//! Registers as a Roon Extension, waits for `CorePaired`, subscribes to zone
//! events, publishes `StateUpdate`s on a broadcast channel, and translates
//! edge-core `Intent`s into Roon `Transport` calls.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use edge_core::{Intent, ServiceAdapter, StateUpdate};
use roon_api::{
    ControlAction, FileTokenStore, MuteAction, RoonClient, RoonClientBuilder, RoonEvent, SeekMode,
    Transport, VolumeMode, Zone, ZoneEvent,
};
use tokio::sync::{broadcast, Mutex};

pub const SERVICE_TYPE: &str = "roon";

#[derive(Debug, Clone)]
pub struct RoonConfig {
    pub extension_id: String,
    pub display_name: String,
    pub display_version: String,
    pub publisher: String,
    pub email: String,
    /// If both set, connect directly; otherwise start SOOD discovery.
    pub host: Option<String>,
    pub port: Option<u16>,
    pub token_path: PathBuf,
}

pub struct RoonAdapter {
    transport: Arc<Mutex<Option<Transport>>>,
    state_tx: broadcast::Sender<StateUpdate>,
    /// `zone_id -> primary output_id`. `Transport::change_volume` /
    /// `mute` require output_id, but mappings commonly target zone_id
    /// (matches Roon's "zone" mental model and the MQTT topic layout).
    /// Populated from zone state as it arrives.
    zone_to_output: Arc<Mutex<HashMap<String, String>>>,
    // Kept to preserve the RoonClient lifetime — its dropped state would
    // eventually close the event broadcast.
    _client: Arc<RoonClient>,
}

impl RoonAdapter {
    pub async fn start(config: RoonConfig) -> anyhow::Result<Self> {
        let client = RoonClientBuilder::new(
            &config.extension_id,
            &config.display_name,
            &config.display_version,
            &config.publisher,
            &config.email,
        )
        .token_store(FileTokenStore::new(&config.token_path))
        .require_transport()
        .build()?;
        let client = Arc::new(client);

        let events = client.events();

        match (config.host.as_deref(), config.port) {
            (Some(host), Some(port)) => {
                tracing::info!(%host, port, "connecting to Roon core directly");
                client.connect(host, port).await?;
            }
            _ => {
                tracing::info!("starting Roon SOOD discovery");
                client.start_discovery().await?;
            }
        }

        let transport_slot: Arc<Mutex<Option<Transport>>> = Arc::new(Mutex::new(None));
        let zone_to_output: Arc<Mutex<HashMap<String, String>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let (state_tx, _) = broadcast::channel(256);

        tokio::spawn(drive_events(
            events,
            transport_slot.clone(),
            zone_to_output.clone(),
            state_tx.clone(),
        ));

        Ok(Self {
            transport: transport_slot,
            state_tx,
            zone_to_output,
            _client: client,
        })
    }

    /// Resolve a caller-supplied target into an output_id for volume / mute
    /// intents. If the target is already a known output_id we return it
    /// unchanged; if it's a zone_id we substitute that zone's primary
    /// output. Falls back to the raw target so pre-existing output-id
    /// mappings still work.
    async fn output_for_volume(&self, target: &str) -> String {
        let map = self.zone_to_output.lock().await;
        map.get(target).cloned().unwrap_or_else(|| target.to_string())
    }
}

#[async_trait]
impl ServiceAdapter for RoonAdapter {
    fn service_type(&self) -> &'static str {
        SERVICE_TYPE
    }

    async fn send_intent(&self, target: &str, intent: &Intent) -> anyhow::Result<()> {
        let guard = self.transport.lock().await;
        let Some(t) = guard.as_ref() else {
            anyhow::bail!("roon transport not yet available (core unpaired)");
        };
        match intent {
            Intent::Play => t.control(target, ControlAction::Play).await?,
            Intent::Pause => t.control(target, ControlAction::Pause).await?,
            Intent::PlayPause => t.control(target, ControlAction::PlayPause).await?,
            Intent::Stop => t.control(target, ControlAction::Stop).await?,
            Intent::Next => t.control(target, ControlAction::Next).await?,
            Intent::Previous => t.control(target, ControlAction::Previous).await?,
            Intent::VolumeChange { delta } => {
                let output = self.output_for_volume(target).await;
                t.change_volume(&output, VolumeMode::Relative, *delta).await?
            }
            Intent::VolumeSet { value } => {
                let output = self.output_for_volume(target).await;
                t.change_volume(&output, VolumeMode::Absolute, *value).await?
            }
            Intent::Mute => {
                let output = self.output_for_volume(target).await;
                t.mute(&output, MuteAction::Mute).await?
            }
            Intent::Unmute => {
                let output = self.output_for_volume(target).await;
                t.mute(&output, MuteAction::Unmute).await?
            }
            Intent::SeekRelative { seconds } => {
                t.seek(target, SeekMode::Relative, *seconds as i64).await?
            }
            Intent::SeekAbsolute { seconds } => {
                t.seek(target, SeekMode::Absolute, *seconds as i64).await?
            }
            other => {
                tracing::debug!(?other, "intent not applicable to roon adapter; skipping");
            }
        }
        Ok(())
    }

    fn subscribe_state(&self) -> broadcast::Receiver<StateUpdate> {
        self.state_tx.subscribe()
    }
}

async fn drive_events(
    mut events: broadcast::Receiver<RoonEvent>,
    transport_slot: Arc<Mutex<Option<Transport>>>,
    zone_to_output: Arc<Mutex<HashMap<String, String>>>,
    state_tx: broadcast::Sender<StateUpdate>,
) {
    loop {
        match events.recv().await {
            Ok(RoonEvent::CorePaired(core)) => {
                tracing::info!(
                    core_id = core.core_id(),
                    display_name = core.display_name(),
                    "paired with Roon core"
                );
                let transport = core.transport();
                match transport.subscribe_zones().await {
                    Ok(zone_rx) => {
                        *transport_slot.lock().await = Some(transport);
                        tokio::spawn(drive_zones(
                            zone_rx,
                            zone_to_output.clone(),
                            state_tx.clone(),
                        ));
                    }
                    Err(e) => tracing::error!(error = %e, "failed to subscribe to zones"),
                }
            }
            Ok(RoonEvent::CoreLost { core_id }) => {
                tracing::warn!(%core_id, "lost Roon core");
                *transport_slot.lock().await = None;
            }
            Ok(RoonEvent::CoreUnpaired { core_id }) => {
                tracing::warn!(%core_id, "unpaired from Roon core");
                *transport_slot.lock().await = None;
            }
            Ok(RoonEvent::CoreFound { core_id, .. }) => {
                tracing::info!(%core_id, "found Roon core");
            }
            Err(broadcast::error::RecvError::Closed) => break,
            Err(broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!(skipped = n, "Roon event lag");
            }
        }
    }
}

async fn drive_zones(
    mut zone_rx: tokio::sync::mpsc::Receiver<ZoneEvent>,
    zone_to_output: Arc<Mutex<HashMap<String, String>>>,
    state_tx: broadcast::Sender<StateUpdate>,
) {
    let mut last_seek_publish = std::time::Instant::now() - std::time::Duration::from_secs(10);
    let seek_throttle = std::time::Duration::from_secs(1);
    // Source-side distinct: Roon republishes the full zone on every
    // ZoneEvent::Changed, including properties that didn't actually change.
    // Suppressing unchanged values here keeps downstream subscribers
    // (edge-agent feedback, Web UI) free of rebound renders.
    let mut last_sent: HashMap<StateKey, serde_json::Value> = HashMap::new();

    while let Some(event) = zone_rx.recv().await {
        match event {
            ZoneEvent::Initial(zones) | ZoneEvent::Added(zones) | ZoneEvent::Changed(zones) => {
                {
                    let mut map = zone_to_output.lock().await;
                    for zone in &zones {
                        if let Some(primary) = zone.outputs.first() {
                            map.insert(zone.zone_id.clone(), primary.output_id.clone());
                        }
                    }
                }
                for zone in &zones {
                    publish_zone(&state_tx, zone, &mut last_sent);
                }
            }
            ZoneEvent::Seeked(_) => {
                if last_seek_publish.elapsed() >= seek_throttle {
                    last_seek_publish = std::time::Instant::now();
                    // seek payload not mapped to a first-class StateUpdate yet.
                }
            }
            ZoneEvent::Removed(zone_ids) => {
                let mut map = zone_to_output.lock().await;
                for id in &zone_ids {
                    map.remove(id);
                }
                last_sent.retain(|k, _| !zone_ids.contains(&k.target));
            }
        }
    }
}

#[derive(Hash, Eq, PartialEq, Clone)]
struct StateKey {
    target: String,
    property: String,
    output_id: Option<String>,
}

fn send_if_changed(
    tx: &broadcast::Sender<StateUpdate>,
    last_sent: &mut HashMap<StateKey, serde_json::Value>,
    update: StateUpdate,
) {
    let key = StateKey {
        target: update.target.clone(),
        property: update.property.clone(),
        output_id: update.output_id.clone(),
    };
    if last_sent.get(&key) == Some(&update.value) {
        return;
    }
    last_sent.insert(key, update.value.clone());
    let _ = tx.send(update);
}

fn publish_zone(
    tx: &broadcast::Sender<StateUpdate>,
    zone: &Zone,
    last_sent: &mut HashMap<StateKey, serde_json::Value>,
) {
    // Zone summary: Web UI reads display_name here to label cards.
    let summary = serde_json::json!({
        "display_name": zone.display_name,
        "state": zone.state,
    });
    send_if_changed(
        tx,
        last_sent,
        StateUpdate {
            service_type: SERVICE_TYPE.into(),
            target: zone.zone_id.clone(),
            property: "zone".into(),
            output_id: None,
            value: summary,
        },
    );

    let playback = serde_json::to_value(zone.state).unwrap_or(serde_json::Value::Null);
    send_if_changed(
        tx,
        last_sent,
        StateUpdate {
            service_type: SERVICE_TYPE.into(),
            target: zone.zone_id.clone(),
            property: "playback".into(),
            output_id: None,
            value: playback,
        },
    );

    if let Some(np) = &zone.now_playing {
        let now_playing = serde_json::to_value(np).unwrap_or(serde_json::Value::Null);
        send_if_changed(
            tx,
            last_sent,
            StateUpdate {
                service_type: SERVICE_TYPE.into(),
                target: zone.zone_id.clone(),
                property: "now_playing".into(),
                output_id: None,
                value: now_playing,
            },
        );
    }

    for output in &zone.outputs {
        if let Some(vol) = &output.volume {
            let volume = serde_json::to_value(vol).unwrap_or(serde_json::Value::Null);
            send_if_changed(
                tx,
                last_sent,
                StateUpdate {
                    service_type: SERVICE_TYPE.into(),
                    target: zone.zone_id.clone(),
                    property: "volume".into(),
                    output_id: Some(output.output_id.clone()),
                    value: volume,
                },
            );
        }
    }
}
