//! `MacosMusicAdapter`: a `ServiceAdapter` that controls the local
//! `Music.app` via AppleScript. The adapter holds no client state — each
//! intent is a one-shot `osascript` invocation. State publishing runs in
//! a background polling task that watches Music.app's player state and
//! emits `now_playing` updates whenever the visible track or playback
//! state changes.

use async_trait::async_trait;
use edge_core::{Intent, ServiceAdapter, StateUpdate};
use tokio::sync::broadcast;

use super::osascript::{intent_to_script, run_script};

pub const SERVICE_TYPE: &str = "macos_music";

/// Empty config — there is exactly one Music.app per host. Reserved for
/// future options (custom poll cadence, optional MediaRemote integration
/// once the private API is wired up).
#[derive(Debug, Clone, Default)]
pub struct MacosMusicConfig;

pub struct MacosMusicAdapter {
    state_tx: broadcast::Sender<StateUpdate>,
}

impl MacosMusicAdapter {
    /// Build the adapter and spawn the now_playing polling task. Failure
    /// of the initial query (Music.app not running yet) is non-fatal —
    /// the polling task will retry on its next tick once the user opens
    /// the app.
    pub async fn start(_config: MacosMusicConfig) -> anyhow::Result<Self> {
        let (state_tx, _) = broadcast::channel(256);
        tokio::spawn(super::now_playing::run(state_tx.clone()));
        tracing::info!("macos_music adapter started");
        Ok(Self { state_tx })
    }
}

#[async_trait]
impl ServiceAdapter for MacosMusicAdapter {
    fn service_type(&self) -> &'static str {
        SERVICE_TYPE
    }

    async fn send_intent(&self, _target: &str, intent: &Intent) -> anyhow::Result<()> {
        let Some(script) = intent_to_script(intent) else {
            tracing::debug!(
                ?intent,
                "intent not applicable to macos_music adapter; skipping"
            );
            return Ok(());
        };
        let stdout = run_script(&script).await?;
        if !stdout.is_empty() {
            tracing::debug!(stdout = %stdout, ?intent, "macos_music intent dispatched");
        }
        Ok(())
    }

    fn subscribe_state(&self) -> broadcast::Receiver<StateUpdate> {
        self.state_tx.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intent_translation_covers_media_keys() {
        // Directly test that each routable intent produces *some* script.
        // The actual osascript invocation is exercised by hardware tests.
        for intent in [
            Intent::Play,
            Intent::Pause,
            Intent::PlayPause,
            Intent::Stop,
            Intent::Next,
            Intent::Previous,
            Intent::VolumeChange { delta: 4.0 },
            Intent::VolumeSet { value: 50.0 },
            Intent::Mute,
            Intent::Unmute,
            Intent::SeekRelative { seconds: 10.0 },
            Intent::SeekAbsolute { seconds: 60.0 },
        ] {
            assert!(
                intent_to_script(&intent).is_some(),
                "expected script for {:?}",
                intent
            );
        }
    }

    #[test]
    fn intent_translation_skips_brightness_and_power() {
        for intent in [
            Intent::BrightnessChange { delta: 0.1 },
            Intent::BrightnessSet { value: 0.5 },
            Intent::ColorTemperatureChange { delta: 0.1 },
            Intent::PowerToggle,
            Intent::PowerOn,
            Intent::PowerOff,
        ] {
            assert!(
                intent_to_script(&intent).is_none(),
                "expected no script for {:?}",
                intent
            );
        }
    }

    #[test]
    fn volume_set_clamps_to_0_100_range() {
        let s = intent_to_script(&Intent::VolumeSet { value: 150.0 }).unwrap();
        assert!(s.contains("100"));
        let s = intent_to_script(&Intent::VolumeSet { value: -10.0 }).unwrap();
        assert!(s.contains(" 0"));
    }
}
