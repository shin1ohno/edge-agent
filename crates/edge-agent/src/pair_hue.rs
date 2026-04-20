//! `edge-agent pair-hue [--host <ip>] [--out <path>] [--timeout <secs>]`
//!
//! One-shot helper: discover a bridge (or use `--host`), guide the user
//! through the link-button press, and persist the credentials so the main
//! runtime can pick them up.

use std::path::PathBuf;
use std::time::Duration;

use adapter_hue::{discover, pair, DiscoveredBridge};

use crate::hue_token::{save, HueToken};

pub async fn run(args: &[String]) -> anyhow::Result<()> {
    let mut host: Option<String> = None;
    let mut out: Option<PathBuf> = None;
    let mut timeout_secs: u64 = 60;

    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--host" => host = it.next().cloned(),
            "--out" => out = it.next().map(PathBuf::from),
            "--timeout" => {
                timeout_secs = it
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--timeout needs a value"))?
                    .parse()?;
            }
            "--help" | "-h" => {
                print_help();
                return Ok(());
            }
            other => anyhow::bail!("unknown argument: {other}"),
        }
    }

    let host = match host {
        Some(h) => h,
        None => pick_bridge().await?,
    };
    let out = out.unwrap_or_else(default_token_path);

    tracing::info!(%host, "starting pair flow; press the link button on your bridge within {}s", timeout_secs);
    let creds = pair(
        &host,
        "edge-agent#pair-hue",
        Duration::from_secs(timeout_secs),
    )
    .await?;

    let token = HueToken {
        host: creds.host,
        app_key: creds.app_key,
        client_key: creds.client_key,
    };
    save(&out, &token)?;
    tracing::info!(path = %out.display(), "hue token saved");
    println!("Paired successfully. Token written to {}", out.display());
    Ok(())
}

async fn pick_bridge() -> anyhow::Result<String> {
    let bridges = discover().await?;
    match bridges.as_slice() {
        [] => anyhow::bail!("no Hue bridges discovered via N-UPnP; pass --host explicitly"),
        [b] => {
            tracing::info!(id = %b.id, host = %b.host, "using sole discovered bridge");
            Ok(b.host.clone())
        }
        many => {
            let labels: Vec<_> = many
                .iter()
                .map(|b: &DiscoveredBridge| format!("  {} ({})", b.host, b.id))
                .collect();
            anyhow::bail!(
                "multiple bridges found; pass --host explicitly:\n{}",
                labels.join("\n")
            );
        }
    }
}

fn default_token_path() -> PathBuf {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("state")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("edge-agent").join("hue-token.json")
}

fn print_help() {
    println!(
        "Usage: edge-agent pair-hue [--host <ip>] [--out <path>] [--timeout <secs>]

Discovers a Hue bridge (N-UPnP) or uses --host, then polls the API until you
press the link button on the bridge. On success, writes a JSON token to
--out (default ~/.local/state/edge-agent/hue-token.json)."
    );
}
