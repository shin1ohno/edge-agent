//! Linux wifi signal-strength reader backed by `/proc/net/wireless`.
//!
//! The kernel exposes a per-interface "link quality" value through this
//! procfs file. Most drivers normalise to 0..=70 (the historical
//! Wireless Extension upper bound), some to 0..=100. We treat 70 as the
//! denominator and clamp, which slightly under-reports drivers that
//! emit 0..=100 but never exceeds 100% — fine for a UI indicator.

pub async fn read() -> Option<u8> {
    let content = tokio::fs::read_to_string("/proc/net/wireless").await.ok()?;
    parse(&content)
}

fn parse(content: &str) -> Option<u8> {
    let mut max_quality: Option<f64> = None;
    // First two lines are the header. Each subsequent non-empty line is
    // one interface; the format is:
    //   <name>: <status> <quality>. <level>. <noise>.  ...
    // (the trailing `.` on quality / level / noise is part of the
    //  procfs format, not a typo.)
    for line in content.lines().skip(2) {
        let after_colon = match line.split_once(':') {
            Some((_, rest)) => rest,
            None => continue,
        };
        let mut fields = after_colon.split_whitespace();
        let _status = match fields.next() {
            Some(s) => s,
            None => continue,
        };
        let quality_str = match fields.next() {
            Some(s) => s,
            None => continue,
        };
        let quality: f64 = match quality_str.trim_end_matches('.').parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        max_quality = Some(max_quality.map_or(quality, |m: f64| m.max(quality)));
    }
    let q = max_quality?;
    let pct = (q / 70.0 * 100.0).clamp(0.0, 100.0);
    Some(pct.round() as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_typical_single_adapter() {
        // Real `/proc/net/wireless` output captured from a healthy
        // connection (wlp4s0 at 62/70 link quality).
        let s =
            "Inter-| sta-|   Quality        |   Discarded packets               | Missed | WE\n\
                 face | tus | link level noise |  nwid  crypt   frag  retry   misc | beacon | 22\n\
                 wlp4s0: 0000   62.  -48.  -256        0      0      0      0    151        0\n";
        assert_eq!(parse(s), Some(89)); // 62/70*100 ≈ 88.57 → 89
    }

    #[test]
    fn returns_none_when_no_adapters() {
        let s = "Inter-| sta-|   Quality        ...\n face | sta | link level noise...\n";
        assert_eq!(parse(s), None);
    }

    #[test]
    fn picks_max_across_multiple_adapters() {
        let s = "header1\nheader2\n\
                 wlan0: 0000   30.  -60.  -256        0      0      0      0    100        0\n\
                 wlan1: 0000   55.  -50.  -256        0      0      0      0    100        0\n";
        assert_eq!(parse(s), Some(79)); // 55/70*100 ≈ 78.57 → 79
    }

    #[test]
    fn clamps_quality_above_seventy() {
        // Some drivers report on a 0..=100 scale; ensure we don't return >100.
        let s =
            "h1\nh2\nwlan0: 0000   95.  -40.  -256        0      0      0      0    100        0\n";
        assert_eq!(parse(s), Some(100));
    }

    #[test]
    fn skips_malformed_lines() {
        let s = "h1\nh2\ngarbage line\nwlan0: 0000   42.  -55.  -256        0\n";
        assert_eq!(parse(s), Some(60)); // 42/70*100 = 60
    }
}
