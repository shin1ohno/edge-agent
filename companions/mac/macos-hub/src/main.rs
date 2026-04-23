mod command_handler;
mod config;
#[cfg(target_os = "macos")]
mod core_audio;
#[cfg(target_os = "macos")]
mod media_keys;
mod mqtt;

use std::path::PathBuf;
use std::time::Duration;

use tokio::signal;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,macos_hub=debug".into()),
        )
        .init();

    let config_path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("./macos-hub.toml"));

    let config = config::Config::load(&config_path)?;
    tracing::info!(
        "Config loaded: mqtt={}:{} edge_id={}",
        config.mqtt.host,
        config.mqtt.port,
        config.macos.edge_id
    );

    let mqtt_bridge = mqtt::MqttBridge::new(&config.mqtt);
    let (mqtt_client, mut command_rx) = mqtt_bridge.start().await?;
    tracing::info!(
        "MQTT connected to {}:{}",
        config.mqtt.host,
        config.mqtt.port
    );

    // Initial state publish.
    #[cfg(target_os = "macos")]
    publish_full_state(&mqtt_client, &config.macos.edge_id).await;

    // Periodic state re-publisher.
    let interval_secs = config.macos.periodic_publish_interval_secs.max(1);
    let client_for_timer = mqtt_client.clone();
    let edge_id_for_timer = config.macos.edge_id.clone();
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(interval_secs));
        // Skip the immediate first tick; initial publish already happened.
        ticker.tick().await;
        loop {
            ticker.tick().await;
            #[cfg(target_os = "macos")]
            publish_periodic_state(&client_for_timer, &edge_id_for_timer).await;
            #[cfg(not(target_os = "macos"))]
            {
                let _ = (&client_for_timer, &edge_id_for_timer);
            }
        }
    });

    // Command handler.
    let client_for_cmd = mqtt_client.clone();
    let edge_id_for_cmd = config.macos.edge_id.clone();
    tokio::spawn(async move {
        while let Some((topic, payload)) = command_rx.recv().await {
            #[cfg(target_os = "macos")]
            {
                if let Err(e) =
                    command_handler::handle(&client_for_cmd, &edge_id_for_cmd, &topic, &payload)
                        .await
                {
                    tracing::warn!("command error ({}): {}", topic, e);
                }
            }
            #[cfg(not(target_os = "macos"))]
            {
                let _ = (&client_for_cmd, &edge_id_for_cmd, topic, payload);
                tracing::warn!("command ignored: not running on macOS");
            }
        }
    });

    signal::ctrl_c().await?;
    tracing::info!("Shutting down...");
    Ok(())
}

#[cfg(target_os = "macos")]
async fn publish_full_state(client: &rumqttc::AsyncClient, edge_id: &str) {
    match core_audio::list_outputs() {
        Ok(outputs) => {
            if let Err(e) = mqtt::publish_available_outputs(client, edge_id, &outputs).await {
                tracing::warn!("publish_available_outputs: {}", e);
            }
        }
        Err(e) => tracing::warn!("list_outputs failed: {}", e),
    }

    match core_audio::get_default_output_device() {
        Ok(dev) => {
            if let Err(e) = mqtt::publish_output_device(client, edge_id, &dev).await {
                tracing::warn!("publish_output_device: {}", e);
            }
        }
        Err(e) => {
            tracing::warn!("get_default_output_device failed: {}", e);
            if let Err(e) = mqtt::publish_output_device_unknown(client, edge_id).await {
                tracing::warn!("publish_output_device_unknown: {}", e);
            }
        }
    }

    match core_audio::get_system_volume() {
        Ok(v) => {
            if let Err(e) = mqtt::publish_volume(client, edge_id, v).await {
                tracing::warn!("publish_volume: {}", e);
            }
        }
        Err(e) => tracing::warn!("get_system_volume failed: {}", e),
    }

    if let Err(e) = mqtt::publish_playback_active(client, edge_id, None).await {
        tracing::warn!("publish_playback_active: {}", e);
    }
}

#[cfg(target_os = "macos")]
async fn publish_periodic_state(client: &rumqttc::AsyncClient, edge_id: &str) {
    if let Ok(v) = core_audio::get_system_volume() {
        if let Err(e) = mqtt::publish_volume(client, edge_id, v).await {
            tracing::warn!("periodic publish_volume: {}", e);
        }
    }
    if let Ok(dev) = core_audio::get_default_output_device() {
        if let Err(e) = mqtt::publish_output_device(client, edge_id, &dev).await {
            tracing::warn!("periodic publish_output_device: {}", e);
        }
    }
}
