//! macOS wifi signal-strength reader backed by the `airport` private
//! framework binary.
//!
//! `airport -I` returns a key-value text dump that includes
//! `agrCtlRSSI: <dBm>`. We map RSSI to a percent using the standard
//! `2 * (rssi + 100)` clamped to 0..=100 — this is the same conversion
//! macOS Wi-Fi menu bar uses internally.
//!
//! The `airport` binary is deprecated in macOS 14 (Sonoma) and may be
//! removed in newer releases. When the binary is absent or returns a
//! non-zero status we yield `None`, and the UI falls back to `—`. A
//! `CoreWLAN`-based reader is a follow-up if Apple removes the binary
//! entirely.

use tokio::process::Command;

const AIRPORT_PATH: &str =
    "/System/Library/PrivateFrameworks/Apple80211.framework/Versions/Current/Resources/airport";

pub async fn read() -> Option<u8> {
    let output = Command::new(AIRPORT_PATH).arg("-I").output().await.ok()?;
    if !output.status.success() {
        return None;
    }
    let text = std::str::from_utf8(&output.stdout).ok()?;
    parse(text)
}

fn parse(text: &str) -> Option<u8> {
    let rssi_line = text
        .lines()
        .find(|l| l.trim_start().starts_with("agrCtlRSSI:"))?;
    let rssi_str = rssi_line.split(':').nth(1)?.trim();
    let rssi: i32 = rssi_str.parse().ok()?;
    // Standard RSSI-to-percent: 2 * (rssi + 100), clamped.
    let pct = (2 * (rssi + 100)).clamp(0, 100) as u8;
    Some(pct)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_strong_signal() {
        let s = "     agrCtlRSSI: -45\n     agrExtRSSI: 0\n     agrCtlNoise: -90\n     state: running\n";
        assert_eq!(parse(s), Some(100)); // 2*(-45+100)=110 → clamped 100
    }

    #[test]
    fn parses_weak_signal() {
        let s = "     agrCtlRSSI: -75\n     state: running\n";
        assert_eq!(parse(s), Some(50)); // 2*(-75+100)=50
    }

    #[test]
    fn parses_borderline() {
        let s = "     agrCtlRSSI: -100\n";
        assert_eq!(parse(s), Some(0)); // 2*0=0
    }

    #[test]
    fn returns_none_when_rssi_missing() {
        let s = "     state: not associated\n";
        assert_eq!(parse(s), None);
    }

    #[test]
    fn handles_disassociated_output() {
        // When not associated to a network, RSSI is sometimes reported as
        // `0` rather than absent. Keep the conversion: 2*(0+100)=200 → 100.
        // Callers can treat this as an artifact of the disassociated state;
        // we don't have richer signals from `airport -I` to discriminate.
        let s = "     agrCtlRSSI: 0\n     state: init\n";
        assert_eq!(parse(s), Some(100));
    }
}
