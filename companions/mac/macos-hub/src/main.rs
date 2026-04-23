mod command_handler;
mod config;
#[cfg(target_os = "macos")]
mod core_audio;
#[cfg(not(target_os = "macos"))]
mod core_audio_stub;
#[cfg(target_os = "macos")]
mod media_keys;
#[cfg(not(target_os = "macos"))]
mod media_keys_stub;
mod mqtt;

#[cfg(target_os = "macos")]
use crate::core_audio as audio;

#[cfg(not(target_os = "macos"))]
use crate::core_audio_stub as audio;

#[cfg(not(target_os = "macos"))]
use crate::media_keys_stub as media_keys;

use std::path::PathBuf;
use std::time::Duration;

use tokio::signal;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let config_path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("macos-hub.toml"));

    let config = config::Config::load(&config_path)?;
    tracing::info!(
        "Config loaded: edge_id={}, mqtt={}:{}",
        config.macos.edge_id,
        config.mqtt.host,
        config.mqtt.port,
    );

    if !media_keys::is_accessibility_trusted() {
        tracing::warn!(
            "Accessibility permission NOT granted — CGEventPost will silently drop \
             media-key events. Grant permission in System Settings → Privacy & \
             Security → Accessibility (add the binary or the terminal running it). \
             Volume and output switching do not require this; media keys do."
        );
    } else {
        tracing::info!("Accessibility permission OK");
    }

    #[cfg(target_os = "macos")]
    tracing::info!(
        "Default audio output at startup: {}",
        audio::describe_default_output()
    );
    #[cfg(target_os = "macos")]
    match audio::list_outputs() {
        Ok(devs) => {
            tracing::info!("All output devices ({}):", devs.len());
            for d in &devs {
                tracing::info!(
                    "  id={} uid={:?} name={:?} transport=0x{:08x} is_airplay={}",
                    d.id, d.uid, d.name, d.transport_type, d.is_airplay
                );
            }
        }
        Err(e) => tracing::warn!("list_outputs error: {}", e),
    }

    // MQTT bridge
    let bridge = mqtt::MqttBridge::new(&config.mqtt);
    let (client, mut command_rx) = bridge.start().await?;
    tracing::info!(
        "MQTT connected to {}:{}",
        config.mqtt.host,
        config.mqtt.port
    );

    let edge_id = config.macos.edge_id.clone();
    let periodic_interval = Duration::from_secs(config.macos.periodic_publish_interval_secs);

    // Initial state publish.
    publish_full_state(&client, &edge_id).await;

    // Command-handling task.
    let cmd_client = client.clone();
    let cmd_edge_id = edge_id.clone();
    tokio::spawn(async move {
        let mut mute_restore: Option<u8> = None;
        while let Some((topic, payload)) = command_rx.recv().await {
            tracing::info!("handling command: topic={} payload={}", topic, payload);
            match command_handler::handle_command(&topic, &payload, &mut mute_restore).await {
                Ok(fx) => {
                    tracing::info!(
                        "command handled: volume_changed={} output_changed={}",
                        fx.volume_changed,
                        fx.output_changed
                    );
                    if fx.volume_changed {
                        if let Err(e) = republish_volume(&cmd_client, &cmd_edge_id).await {
                            tracing::warn!("republish volume error: {}", e);
                        }
                    }
                    if fx.output_changed {
                        if let Err(e) = republish_output(&cmd_client, &cmd_edge_id).await {
                            tracing::warn!("republish output error: {}", e);
                        }
                    }
                }
                Err(e) => tracing::warn!("command error on {}: {}", topic, e),
            }
        }
        tracing::warn!("command_rx closed; handler task exiting");
    });

    // Periodic re-publish of volume + output_device. Catches external changes
    // (user moves the menubar slider, plugs in AirPods, etc.).
    let periodic_client = client.clone();
    let periodic_edge_id = edge_id.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(periodic_interval);
        tick.tick().await; // consume immediate tick
        loop {
            tick.tick().await;
            if let Err(e) = republish_volume(&periodic_client, &periodic_edge_id).await {
                tracing::debug!("periodic volume error: {}", e);
            }
            if let Err(e) = republish_output(&periodic_client, &periodic_edge_id).await {
                tracing::debug!("periodic output error: {}", e);
            }
        }
    });

    signal::ctrl_c().await?;
    tracing::info!("shutting down");
    Ok(())
}

async fn publish_full_state(client: &rumqttc::AsyncClient, edge_id: &str) {
    if let Err(e) = republish_volume(client, edge_id).await {
        tracing::warn!("initial volume publish: {}", e);
    }
    if let Err(e) = republish_output(client, edge_id).await {
        tracing::warn!("initial output publish: {}", e);
    }
    if let Err(e) = republish_available_outputs(client, edge_id).await {
        tracing::warn!("initial available_outputs publish: {}", e);
    }
    if let Err(e) = mqtt::publish_playback_active(client, edge_id, None).await {
        tracing::warn!("initial playback_active publish: {}", e);
    }
}

async fn republish_volume(
    client: &rumqttc::AsyncClient,
    edge_id: &str,
) -> anyhow::Result<()> {
    let vol_f = audio::get_system_volume()?;
    let level = (vol_f * 100.0).round().clamp(0.0, 100.0) as u8;
    mqtt::publish_volume(client, edge_id, level).await
}

async fn republish_output(
    client: &rumqttc::AsyncClient,
    edge_id: &str,
) -> anyhow::Result<()> {
    let default_id = audio::get_default_output()?;
    let outputs = audio::list_outputs()?;
    if let Some(current) = outputs.iter().find(|d| d.id == default_id) {
        mqtt::publish_output_device(client, edge_id, current).await?;
    }
    Ok(())
}

async fn republish_available_outputs(
    client: &rumqttc::AsyncClient,
    edge_id: &str,
) -> anyhow::Result<()> {
    let outputs = audio::list_outputs()?;
    mqtt::publish_available_outputs(client, edge_id, &outputs).await
}
