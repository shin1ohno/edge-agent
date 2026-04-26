//! Service adapter for iOS-hosted media playback.
//!
//! Bridges `edge-core::Intent` → Swift, where `IosMediaDispatcher` invokes
//! `MPRemoteCommandCenter` on whichever app currently owns the iOS Now
//! Playing session (Music.app, Spotify, Podcasts, …).
//!
//! iOS sandboxes system-volume, mute, and brightness operations away from
//! third-party apps. Those intents are rejected at the adapter boundary so
//! the failure shows up cleanly in the Web UI's Live Console — the user
//! sees `ios_media: unsupported on iOS (volume_change)` instead of a
//! silent no-op on the device.

use std::sync::Arc;

use async_trait::async_trait;
use edge_core::{Intent, ServiceAdapter, StateUpdate};
use thiserror::Error;
use tokio::sync::broadcast;

/// Subset of `edge-core::Intent` that an iOS edge can dispatch via
/// `MPRemoteCommandCenter`. Volume, mute, brightness, and power intents
/// are intentionally absent — they are rejected by the adapter with
/// `IosMediaError::Unsupported` before reaching Swift.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum IosMediaIntent {
    Play,
    Pause,
    PlayPause,
    Stop,
    Next,
    Previous,
    SeekRelative { seconds: f64 },
    SeekAbsolute { seconds: f64 },
}

/// Coarse playback-state classification used by `NowPlayingInfo`. Mirrors
/// the meaningful values of `MPMusicPlaybackState`; transient values
/// (`.interrupted`, `.seekingForward`, `.seekingBackward`) collapse into
/// `Playing` since the UI doesn't differentiate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum PlaybackState {
    Stopped,
    Playing,
    Paused,
}

/// Snapshot of what's currently playing in Apple Music on the iPad.
/// Forwarded to weave-server over `EdgeToServer::State` with
/// `service_type = "ios_media"`, `property = "now_playing"`.
///
/// Optional fields collapse to `null` in the JSON value when the
/// underlying `MPMediaItem` did not provide them (no metadata, no
/// queue, etc.).
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct NowPlayingInfo {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub duration_seconds: Option<f64>,
    pub position_seconds: f64,
    pub state: PlaybackState,
}

/// Errors returned across the FFI to Swift dispatchers, and back from the
/// adapter into edge-core's command-result reporting path.
#[derive(Debug, Error, uniffi::Error)]
pub enum IosMediaError {
    /// The Intent variant has no iOS equivalent — typically volume,
    /// mute, brightness, or power.
    #[error("ios_media: unsupported on iOS ({variant})")]
    Unsupported { variant: String },
    /// Swift dispatcher signalled failure (no Now Playing app, command
    /// disabled, etc.).
    #[error("ios_media: dispatch failed: {message}")]
    DispatchFailed { message: String },
}

/// Swift-implemented dispatcher for `IosMediaIntent`s. Implementations
/// invoke `MPRemoteCommandCenter` on the iOS app side.
#[uniffi::export(with_foreign)]
pub trait IosMediaCallback: Send + Sync {
    fn handle_intent(&self, intent: IosMediaIntent) -> Result<(), IosMediaError>;
}

/// Concrete `ServiceAdapter` registered for `service_type = "ios_media"`.
/// Constructed once per `EdgeClient` lifetime; dispatch goes through the
/// `IosMediaCallback` provided by Swift.
pub(crate) struct IosMediaAdapter {
    callback: Arc<dyn IosMediaCallback>,
    state_tx: broadcast::Sender<StateUpdate>,
}

impl IosMediaAdapter {
    pub fn new(callback: Arc<dyn IosMediaCallback>) -> Self {
        let (state_tx, _) = broadcast::channel(16);
        Self { callback, state_tx }
    }

    /// Map a transport-class `Intent` into `IosMediaIntent`. Returns
    /// `None` for variants the iOS sandbox cannot satisfy — those are
    /// surfaced as `IosMediaError::Unsupported` upstream.
    fn map_intent(intent: &Intent) -> Option<IosMediaIntent> {
        match intent {
            Intent::Play => Some(IosMediaIntent::Play),
            Intent::Pause => Some(IosMediaIntent::Pause),
            Intent::PlayPause => Some(IosMediaIntent::PlayPause),
            Intent::Stop => Some(IosMediaIntent::Stop),
            Intent::Next => Some(IosMediaIntent::Next),
            Intent::Previous => Some(IosMediaIntent::Previous),
            Intent::SeekRelative { seconds } => {
                Some(IosMediaIntent::SeekRelative { seconds: *seconds })
            }
            Intent::SeekAbsolute { seconds } => {
                Some(IosMediaIntent::SeekAbsolute { seconds: *seconds })
            }
            _ => None,
        }
    }

    fn variant_name(intent: &Intent) -> &'static str {
        match intent {
            Intent::Play => "play",
            Intent::Pause => "pause",
            Intent::PlayPause => "play_pause",
            Intent::Stop => "stop",
            Intent::Next => "next",
            Intent::Previous => "previous",
            Intent::VolumeChange { .. } => "volume_change",
            Intent::VolumeSet { .. } => "volume_set",
            Intent::Mute => "mute",
            Intent::Unmute => "unmute",
            Intent::SeekRelative { .. } => "seek_relative",
            Intent::SeekAbsolute { .. } => "seek_absolute",
            Intent::BrightnessChange { .. } => "brightness_change",
            Intent::BrightnessSet { .. } => "brightness_set",
            Intent::ColorTemperatureChange { .. } => "color_temperature_change",
            Intent::PowerToggle => "power_toggle",
            Intent::PowerOn => "power_on",
            Intent::PowerOff => "power_off",
        }
    }
}

#[async_trait]
impl ServiceAdapter for IosMediaAdapter {
    fn service_type(&self) -> &'static str {
        "ios_media"
    }

    async fn send_intent(&self, _target: &str, intent: &Intent) -> anyhow::Result<()> {
        let Some(media_intent) = Self::map_intent(intent) else {
            let variant = Self::variant_name(intent).to_string();
            return Err(anyhow::anyhow!(IosMediaError::Unsupported { variant }));
        };
        // The Swift dispatcher's MPRemoteCommandCenter calls are
        // synchronous and brief; calling a sync callback from this async
        // method does not stall the tokio runtime in practice.
        self.callback
            .handle_intent(media_intent)
            .map_err(|e| anyhow::anyhow!(e))
    }

    fn subscribe_state(&self) -> broadcast::Receiver<StateUpdate> {
        self.state_tx.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct RecordingCallback {
        captured: Mutex<Vec<IosMediaIntent>>,
        respond_with: Mutex<Option<IosMediaError>>,
    }

    impl RecordingCallback {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                captured: Mutex::new(Vec::new()),
                respond_with: Mutex::new(None),
            })
        }

        fn fail_next(self: &Arc<Self>, err: IosMediaError) {
            *self.respond_with.lock().unwrap() = Some(err);
        }

        fn captured(&self) -> Vec<IosMediaIntent> {
            self.captured.lock().unwrap().clone()
        }
    }

    impl IosMediaCallback for RecordingCallback {
        fn handle_intent(&self, intent: IosMediaIntent) -> Result<(), IosMediaError> {
            self.captured.lock().unwrap().push(intent);
            if let Some(err) = self.respond_with.lock().unwrap().take() {
                Err(err)
            } else {
                Ok(())
            }
        }
    }

    #[tokio::test]
    async fn play_pause_intent_reaches_callback() {
        let callback = RecordingCallback::new();
        let adapter = IosMediaAdapter::new(callback.clone());

        adapter
            .send_intent("default", &Intent::PlayPause)
            .await
            .expect("PlayPause must dispatch");

        assert_eq!(callback.captured(), vec![IosMediaIntent::PlayPause]);
    }

    #[tokio::test]
    async fn next_and_previous_dispatch() {
        let callback = RecordingCallback::new();
        let adapter = IosMediaAdapter::new(callback.clone());

        adapter.send_intent("default", &Intent::Next).await.unwrap();
        adapter
            .send_intent("default", &Intent::Previous)
            .await
            .unwrap();

        assert_eq!(
            callback.captured(),
            vec![IosMediaIntent::Next, IosMediaIntent::Previous]
        );
    }

    #[tokio::test]
    async fn seek_relative_carries_seconds() {
        let callback = RecordingCallback::new();
        let adapter = IosMediaAdapter::new(callback.clone());

        adapter
            .send_intent("default", &Intent::SeekRelative { seconds: 12.5 })
            .await
            .unwrap();

        assert_eq!(
            callback.captured(),
            vec![IosMediaIntent::SeekRelative { seconds: 12.5 }]
        );
    }

    #[tokio::test]
    async fn volume_change_returns_unsupported_without_calling_swift() {
        let callback = RecordingCallback::new();
        let adapter = IosMediaAdapter::new(callback.clone());

        let err = adapter
            .send_intent("default", &Intent::VolumeChange { delta: 5.0 })
            .await
            .expect_err("volume must be rejected");
        let msg = format!("{err}");
        assert!(msg.contains("unsupported"));
        assert!(msg.contains("volume_change"));
        assert!(callback.captured().is_empty(), "Swift must not see volume");
    }

    #[tokio::test]
    async fn brightness_and_power_are_unsupported() {
        let callback = RecordingCallback::new();
        let adapter = IosMediaAdapter::new(callback.clone());

        for intent in [
            Intent::BrightnessSet { value: 50.0 },
            Intent::PowerToggle,
            Intent::Mute,
        ] {
            let err = adapter
                .send_intent("default", &intent)
                .await
                .expect_err("non-transport intent must be rejected");
            assert!(format!("{err}").contains("unsupported"));
        }
        assert!(callback.captured().is_empty());
    }

    #[tokio::test]
    async fn dispatch_error_from_swift_is_propagated() {
        let callback = RecordingCallback::new();
        callback.fail_next(IosMediaError::DispatchFailed {
            message: "no Now Playing app".into(),
        });
        let adapter = IosMediaAdapter::new(callback.clone());

        let err = adapter
            .send_intent("default", &Intent::Play)
            .await
            .expect_err("dispatch error must propagate");
        assert!(format!("{err}").contains("no Now Playing app"));
    }

    #[test]
    fn service_type_is_ios_media() {
        let callback = RecordingCallback::new();
        let adapter = IosMediaAdapter::new(callback);
        assert_eq!(adapter.service_type(), "ios_media");
    }
}
