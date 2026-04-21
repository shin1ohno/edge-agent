//! LAN-local bridge resolution via mDNS (`_hue._tcp.local.`).
//!
//! Hue bridges advertise themselves with a TXT record containing
//! `bridgeid=XXXXXXXXFFFEXXXXXX`. We browse the service type for a bounded
//! window, match the TXT `bridgeid` against the one stored in our token,
//! and return the first matching host. Internet-independent — the fast
//! path when `discovery.meethue.com` is unreachable.

use std::time::{Duration, Instant};

use mdns_sd::{ServiceDaemon, ServiceEvent};

const SERVICE_TYPE: &str = "_hue._tcp.local.";

/// Browse `_hue._tcp.local.` for up to `timeout`, returning the host
/// (IPv4 address as string) of the bridge whose TXT `bridgeid` matches
/// `bridge_id` (case-insensitive). Returns `Ok(None)` on timeout.
pub async fn resolve(bridge_id: &str, timeout: Duration) -> anyhow::Result<Option<String>> {
    let target = bridge_id.to_ascii_lowercase();
    let timeout_task = timeout;
    let target_task = target.clone();

    tokio::task::spawn_blocking(move || resolve_blocking(&target_task, timeout_task)).await?
}

fn resolve_blocking(target: &str, timeout: Duration) -> anyhow::Result<Option<String>> {
    let mdns = ServiceDaemon::new()?;
    let receiver = mdns.browse(SERVICE_TYPE)?;
    let deadline = Instant::now() + timeout;

    let result = loop {
        let remaining = match deadline.checked_duration_since(Instant::now()) {
            Some(d) if !d.is_zero() => d,
            _ => break None,
        };
        match receiver.recv_timeout(remaining) {
            Ok(ServiceEvent::ServiceResolved(info)) => {
                let txt = info
                    .get_properties()
                    .iter()
                    .find(|p| p.key().eq_ignore_ascii_case("bridgeid"))
                    .map(|p| p.val_str().to_ascii_lowercase());
                let matches = txt.as_deref().map(|id| id == target).unwrap_or(false);
                if matches {
                    if let Some(addr) = info.get_addresses_v4().iter().next() {
                        break Some(addr.to_string());
                    }
                }
            }
            Ok(_) => continue,
            Err(_) => break None,
        }
    };

    // Best-effort shutdown; ignore errors (daemon drops anyway when the
    // handle falls out of scope).
    let _ = mdns.shutdown();
    Ok(result)
}
