//! Hue bridge host resolution with DHCP resilience.
//!
//! Strategy (cheapest → most expensive):
//!   1. Probe the stored host via unauthenticated `GET /api/config`
//!   2. Browse mDNS `_hue._tcp.local.` for a bridgeid match
//!   3. Query the Philips cloud at `discovery.meethue.com`
//!
//! When the stored token's `bridge_id` is `None` (migration from an older
//! version), the cloud path will accept a single-bridge result as the one
//! to adopt, but refuses to auto-pick among multiple bridges — those
//! environments need an explicit re-pair so we don't silently latch onto
//! the wrong device.

use std::path::Path;
use std::time::Duration;

use super::{api, discovery, mdns};
use crate::hue_token::{self, HueToken};

const PROBE_TIMEOUT: Duration = Duration::from_secs(2);
const MDNS_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolveSource {
    StoredHost,
    Mdns,
    Cloud,
}

/// Resolve `token` to a reachable bridge host. On success, mutates
/// `token.host` and/or `token.bridge_id` if they changed, and persists the
/// new token atomically via `hue_token::save(token_path, ...)`.
pub async fn resolve_bridge(
    token: &mut HueToken,
    token_path: &Path,
) -> anyhow::Result<ResolveSource> {
    // Stage 1: probe stored host. If OK and bridgeid confirms identity
    // (or we had no bridgeid yet, adopt what we learned), we're done —
    // this is the fast path on every restart where DHCP didn't change.
    match api::fetch_bridge_config(&token.host, PROBE_TIMEOUT).await {
        Ok(cfg) => {
            let observed = cfg.bridge_id.to_ascii_lowercase();
            match token.bridge_id.as_deref().map(str::to_ascii_lowercase) {
                Some(known) if known == observed => {
                    return Ok(ResolveSource::StoredHost);
                }
                Some(known) => {
                    tracing::warn!(
                        known = %known,
                        observed = %observed,
                        host = %token.host,
                        "stored host responds as a different bridge; falling through to discovery",
                    );
                }
                None => {
                    token.bridge_id = Some(observed);
                    persist(token, token_path)?;
                    return Ok(ResolveSource::StoredHost);
                }
            }
        }
        Err(e) => {
            tracing::debug!(
                error = %e,
                host = %token.host,
                "stored host probe failed; falling through to mDNS",
            );
        }
    }

    // From here on we must have a known bridge_id — otherwise the only
    // safe migration path was "stored host responds with the id we adopt",
    // which already returned above.
    let Some(known_id) = token.bridge_id.clone() else {
        return try_cloud_migration(token, token_path).await;
    };

    // Stage 2: mDNS.
    match mdns::resolve(&known_id, MDNS_TIMEOUT).await {
        Ok(Some(host)) => {
            tracing::info!(host = %host, bridge_id = %known_id, "hue bridge resolved via mdns");
            token.host = host;
            persist(token, token_path)?;
            return Ok(ResolveSource::Mdns);
        }
        Ok(None) => {
            tracing::debug!("mdns found no matching bridge; falling through to cloud");
        }
        Err(e) => {
            tracing::debug!(error = %e, "mdns browse failed; falling through to cloud");
        }
    }

    // Stage 3: Philips cloud.
    let bridges = discovery::discover().await?;
    let known_lc = known_id.to_ascii_lowercase();
    let Some(found) = bridges
        .iter()
        .find(|b| b.id.eq_ignore_ascii_case(&known_lc))
    else {
        anyhow::bail!(
            "bridge {known_id} not found via mdns or cloud (found {} candidates)",
            bridges.len()
        );
    };

    tracing::info!(host = %found.host, bridge_id = %found.id, "hue bridge resolved via cloud");
    token.host = found.host.clone();
    persist(token, token_path)?;
    Ok(ResolveSource::Cloud)
}

async fn try_cloud_migration(
    token: &mut HueToken,
    token_path: &Path,
) -> anyhow::Result<ResolveSource> {
    let bridges = discovery::discover().await?;
    match bridges.as_slice() {
        [] => anyhow::bail!(
            "token has no bridge_id and cloud discovery returned no bridges; cannot resolve",
        ),
        [only] => {
            tracing::info!(
                host = %only.host,
                bridge_id = %only.id,
                "adopting sole discovered bridge during migration",
            );
            token.host = only.host.clone();
            token.bridge_id = Some(only.id.to_ascii_lowercase());
            persist(token, token_path)?;
            Ok(ResolveSource::Cloud)
        }
        many => anyhow::bail!(
            "token has no bridge_id and cloud returned {} candidates; re-run `edge-agent pair-hue` to pick one explicitly",
            many.len(),
        ),
    }
}

fn persist(token: &HueToken, path: &Path) -> anyhow::Result<()> {
    hue_token::save(path, token).map_err(|e| {
        anyhow::anyhow!(
            "resolved bridge but failed to persist token to {}: {e}",
            path.display()
        )
    })
}
