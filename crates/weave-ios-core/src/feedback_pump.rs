//! LED feedback pump for the iOS edge.
//!
//! Subscribes to the in-process `StateUpdate` broadcast that
//! `EdgeClient::publish_*` writes to whenever the iPad reports new
//! `ios_media` service state, resolves each update into a
//! `FeedbackPlan` via the mapping's `feedback` rules (cached in the
//! routing engine), renders the resulting 9x9 glyph through
//! `nuimo_protocol::build_led_payload`, and hands the byte array to a
//! Swift-implemented `LedFeedbackSink` that issues the actual BLE
//! write through `BleBridge`.
//!
//! Ports the same `FeedbackPlan` semantics the Linux/Mac edge-agent
//! uses (`crates/edge-agent/src/main.rs`), without the `#[cfg(feature
//! = "roon")]` gate or BLE write coupling — iOS always wants
//! feedback and the BLE call goes back through Swift.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use edge_core::RoutingEngine;
use nuimo_protocol::{self as np, VolumeDirection};
use tokio::sync::broadcast;
use weave_contracts::FeedbackRule;

use crate::glyph_registry::GlyphRegistry;

/// State change received from any iOS edge publisher.
///
/// Mirrors `edge-core::StateUpdate` but kept local to avoid leaking
/// edge-core's adapter trait surface across the crate boundary.
#[derive(Debug, Clone)]
pub(crate) struct StateUpdate {
    pub service_type: String,
    pub target: String,
    pub property: String,
    pub value: serde_json::Value,
}

/// Swift-implemented BLE write sink. The pump never drives BLE
/// directly — Swift owns CoreBluetooth — so each rendered frame goes
/// across this callback.
#[uniffi::export(with_foreign)]
pub trait LedFeedbackSink: Send + Sync {
    fn write_led(&self, device_id: String, payload: Vec<u8>);
}

/// Decision tree for "what should be drawn on the LED right now?"
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FeedbackPlan {
    VolumeBar(u8, VolumeDirection),
    NamedGlyph(String),
    /// Single ASCII character rendered from the bundled 5x7 font. Used
    /// for the cycle-switch target hint ("which target did I just
    /// pick?"). Non-ASCII / unsupported chars fall back to `'?'`.
    Letter(char),
    /// Horizontal scroll of an arbitrary string across the 9x9 LED.
    /// Used for now-playing track titles. Non-ASCII chars are
    /// filtered before scrolling; if the filtered string is empty the
    /// renderer shows `'?'` once and stops.
    ScrollText(String),
}

impl FeedbackPlan {
    pub fn resolve(update: &StateUpdate, rules: &[FeedbackRule]) -> Option<Self> {
        if let Some(plan) = Self::from_rules(update, rules) {
            return Some(plan);
        }
        Self::from_default(update)
    }

    /// Mapping-level feedback rules (user-configured per Connection).
    fn from_rules(update: &StateUpdate, rules: &[FeedbackRule]) -> Option<Self> {
        for rule in rules {
            if rule.state != update.property {
                continue;
            }
            match rule.feedback_type.as_str() {
                "glyph" => {
                    let value_key = match &update.value {
                        serde_json::Value::String(s) => s.clone(),
                        _ => continue,
                    };
                    let glyph_name = rule
                        .mapping
                        .as_object()
                        .and_then(|m| m.get(&value_key))
                        .and_then(|v| v.as_str())?;
                    if glyph_name == "volume_bar" {
                        if let Some((bars, dir)) = volume_bar_from_value(&update.value) {
                            return Some(Self::VolumeBar(bars, dir));
                        }
                        continue;
                    }
                    return Some(Self::NamedGlyph(glyph_name.to_string()));
                }
                "volume_bar" => {
                    if let Some((bars, dir)) = volume_bar_from_value(&update.value) {
                        return Some(Self::VolumeBar(bars, dir));
                    }
                }
                "letter" => {
                    // The state's value must be a single-char string.
                    // Multi-char strings would race with `track_scroll` —
                    // explicit single-char gate keeps the two rule
                    // types mutually exclusive.
                    let s = match &update.value {
                        serde_json::Value::String(s) => s,
                        _ => continue,
                    };
                    if let Some(c) = s.chars().next().filter(|_| s.chars().count() == 1) {
                        return Some(Self::Letter(c));
                    }
                }
                "track_scroll" => {
                    // The track-scroll feedback type accepts the
                    // `now_playing` composite — extract the title field.
                    // Keeps iOS NowPlayingObserver's existing publish
                    // shape (no schema bump for a separate `track`
                    // property).
                    if let Some(title) = update
                        .value
                        .as_object()
                        .and_then(|o| o.get("title"))
                        .and_then(|v| v.as_str())
                        .filter(|t| !t.is_empty())
                    {
                        return Some(Self::ScrollText(title.to_string()));
                    }
                }
                _ => continue,
            }
        }
        None
    }

    /// Hardcoded fallback used when no mapping rule covers the update.
    /// Mirrors Mac's defaults so an unconfigured mapping still produces
    /// useful feedback.
    fn from_default(update: &StateUpdate) -> Option<Self> {
        match (update.property.as_str(), &update.value) {
            ("playback", serde_json::Value::String(s)) => match s.as_str() {
                "playing" => Some(Self::NamedGlyph("play".into())),
                "paused" | "stopped" => Some(Self::NamedGlyph("pause".into())),
                _ => None,
            },
            ("volume", _) | ("brightness", _) => {
                volume_bar_from_value(&update.value).map(|(b, d)| Self::VolumeBar(b, d))
            }
            _ => None,
        }
    }

    /// Stable identifier for "what's on the LED right now". The
    /// `FeedbackFilter` dedups on this so two state updates that round
    /// to the same plan don't trigger redundant BLE writes.
    pub fn signature(&self) -> String {
        match self {
            Self::VolumeBar(bars, dir) => {
                let d = match dir {
                    VolumeDirection::BottomUp => "up",
                    VolumeDirection::TopDown => "down",
                };
                format!("vol:{bars}:{d}")
            }
            Self::NamedGlyph(name) => name.clone(),
            Self::Letter(c) => format!("letter:{}", c),
            // ScrollText has its own per-(device, property) cancel
            // path; the dedup signature captures the source string so
            // a second identical scroll request is a no-op while a new
            // title (re-)launches the animation.
            Self::ScrollText(text) => format!("scroll:{}", text),
        }
    }

    /// Render this plan into the 13-byte payload Nuimo's LED
    /// characteristic expects. Returns `None` when the plan references
    /// a glyph that's not in the registry yet — the caller skips the
    /// write rather than blanking the LED.
    ///
    /// `ScrollText` is NOT rendered through this entry point — it
    /// drives its own animation task; calling `render` on it returns
    /// `None`. The dispatch loop branches before reaching here.
    pub async fn render(&self, registry: &GlyphRegistry) -> Option<Vec<u8>> {
        let (glyph, transition, timeout_ms) = match self {
            Self::VolumeBar(bars, dir) => (
                np::volume_bars(*bars, *dir),
                np::DisplayTransition::Immediate,
                3000u32,
            ),
            Self::NamedGlyph(name) => {
                let entry = registry.get(name).await?;
                if entry.builtin {
                    return None; // builtin glyphs are rendered programmatically (e.g. volume_bar)
                }
                (
                    np::Glyph::from_ascii(&entry.pattern),
                    np::DisplayTransition::CrossFade,
                    1000u32,
                )
            }
            Self::Letter(c) => (
                np::char_glyph(*c),
                np::DisplayTransition::CrossFade,
                2000u32,
            ),
            // Animation-driven; emitted frame-by-frame from the
            // dispatch loop, not this single-payload path.
            Self::ScrollText(_) => return None,
        };
        let opts = np::DisplayOptions {
            brightness: 1.0,
            timeout_ms,
            transition,
        };
        Some(np::build_led_payload(&glyph, &opts).to_vec())
    }
}

/// Project a state-update value into a 9-bar fill + direction.
///
/// Recognises a raw 0..=100 number (Hue brightness, our iOS volume) and
/// a Roon-style `{value, min, max, type?}` envelope. Returns `None` for
/// other shapes so the caller can fall through.
fn volume_bar_from_value(value: &serde_json::Value) -> Option<(u8, VolumeDirection)> {
    match value {
        serde_json::Value::Number(_) => {
            let v = value.as_f64()?;
            let ratio = (v / 100.0).clamp(0.0, 1.0);
            let bars = (ratio * 9.0).round() as u8;
            Some((bars, VolumeDirection::BottomUp))
        }
        serde_json::Value::Object(obj) => {
            let value = obj.get("value").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let max = obj.get("max").and_then(|v| v.as_f64()).unwrap_or(100.0);
            let min = obj.get("min").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let vtype = obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
            let is_db = vtype.eq_ignore_ascii_case("db") || (max <= 0.0 && min < 0.0);
            let span = max - min;
            let ratio = if span > 0.0 {
                ((value - min) / span).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let bars = (ratio * 9.0).round() as u8;
            let direction = if is_db {
                VolumeDirection::TopDown
            } else {
                VolumeDirection::BottomUp
            };
            Some((bars, direction))
        }
        _ => None,
    }
}

/// Throttle + dedup for BLE writes.
///
/// Tracking is keyed on `(device_id, property)` so two state updates
/// arriving in the same publish burst (e.g. `playback` then `volume`
/// 1 ms apart from `NowPlayingObserver.publish()`) don't throttle each
/// other — only further updates to the *same* property within the
/// time gap get dropped.
///
/// Throttle: per-(device, property) minimum gap so a continuous rotate
/// doesn't saturate the BLE connection. Dedup: per-(device, property)
/// signature match — same plan as last write means the visible frame
/// won't change, so skip.
pub(crate) struct FeedbackFilter {
    last_at: HashMap<(String, String), Instant>,
    last_sig: HashMap<(String, String), String>,
    min_gap: Duration,
}

impl FeedbackFilter {
    pub fn new() -> Self {
        Self {
            last_at: HashMap::new(),
            last_sig: HashMap::new(),
            min_gap: Duration::from_millis(100),
        }
    }

    pub fn should_render(&mut self, device_id: &str, property: &str, signature: &str) -> bool {
        let key = (device_id.to_string(), property.to_string());
        if self.last_sig.get(&key).map(String::as_str) == Some(signature) {
            return false;
        }
        let now = Instant::now();
        if let Some(prev) = self.last_at.get(&key) {
            if now.duration_since(*prev) < self.min_gap {
                return false;
            }
        }
        self.last_at.insert(key.clone(), now);
        self.last_sig.insert(key, signature.to_string());
        true
    }
}

/// Long-running task: drains the in-process state broadcast, resolves
/// each update through the routing engine, and pushes the rendered
/// frame across the Swift sink.
///
/// `sink` is `Arc<StdMutex<Option<...>>>` so Swift can register the
/// callback after `connect` (the typical lifecycle in EdgeClientHost).
pub(crate) async fn run_feedback_pump(
    mut state_rx: broadcast::Receiver<StateUpdate>,
    engine: Arc<RoutingEngine>,
    glyphs: Arc<GlyphRegistry>,
    sink: Arc<std::sync::Mutex<Option<Arc<dyn LedFeedbackSink>>>>,
) {
    let mut filter = FeedbackFilter::new();
    let mut animations: HashMap<(String, String), tokio::task::JoinHandle<()>> = HashMap::new();
    loop {
        match state_rx.recv().await {
            Ok(update) => {
                dispatch(
                    &update,
                    &engine,
                    &glyphs,
                    &sink,
                    &mut filter,
                    &mut animations,
                )
                .await;
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!(count = n, "feedback pump lagged");
            }
            Err(broadcast::error::RecvError::Closed) => return,
        }
    }
}

async fn dispatch(
    update: &StateUpdate,
    engine: &RoutingEngine,
    glyphs: &GlyphRegistry,
    sink: &std::sync::Mutex<Option<Arc<dyn LedFeedbackSink>>>,
    filter: &mut FeedbackFilter,
    animations: &mut HashMap<(String, String), tokio::task::JoinHandle<()>>,
) {
    let targets = engine
        .feedback_targets_for(&update.service_type, &update.target)
        .await;
    if targets.is_empty() {
        return;
    }

    // Snapshot the sink under the std::sync::Mutex; release the guard
    // before any `.await` so we never hold a sync lock across a suspend
    // point.
    let registered = { sink.lock().expect("led sink mutex").clone() };
    let Some(registered) = registered else {
        // Until Swift registers the callback the pump is a no-op —
        // expected during connection setup before EdgeClientHost
        // hands its bridge across.
        return;
    };

    for (_device_type, device_id, rules) in targets {
        let Some(plan) = FeedbackPlan::resolve(update, &rules) else {
            continue;
        };
        let sig = plan.signature();
        if !filter.should_render(&device_id, &update.property, &sig) {
            continue;
        }

        // Any new state for this (device, property) supersedes a
        // previously-running scroll animation. Abort first so the
        // single-frame paths below don't fight the animation's writes.
        let key = (device_id.clone(), update.property.clone());
        if let Some(prev) = animations.remove(&key) {
            prev.abort();
        }

        match plan {
            FeedbackPlan::ScrollText(text) => {
                let sink_clone = registered.clone();
                let device_id_owned = device_id;
                let handle = tokio::spawn(async move {
                    run_scroll_animation(text, device_id_owned, sink_clone).await;
                });
                animations.insert(key, handle);
            }
            other => {
                if let Some(payload) = other.render(glyphs).await {
                    registered.write_led(device_id, payload);
                }
            }
        }
    }
}

/// Composed wide bitmap for a scrolling string. One row vector per LED
/// row, each cell holds the lit-or-dark state for that column. Wide
/// enough to include `LED_COLS` of left/right padding so the text
/// scrolls in from the right and out to the left.
struct ScrollCanvas {
    rows: [Vec<bool>; np::LED_ROWS],
}

impl ScrollCanvas {
    /// Compose a canvas for `text`. Non-supported chars are filtered
    /// before composition. Returns `None` if the filtered string is
    /// empty.
    fn from_text(text: &str) -> Option<Self> {
        let chars: Vec<char> = text
            .chars()
            .filter(|c| np::char_pattern(*c).is_some())
            .collect();
        if chars.is_empty() {
            return None;
        }

        let char_w = np::FONT_CHAR_WIDTH;
        let gap = 1usize;
        let pad = np::LED_COLS;
        let body_cols = chars.len() * (char_w + gap) - gap;
        let total_cols = pad + body_cols + pad;
        let row_offset = (np::LED_ROWS - np::FONT_CHAR_HEIGHT) / 2;

        let mut rows: [Vec<bool>; np::LED_ROWS] = std::array::from_fn(|_| vec![false; total_cols]);

        let mut col_cursor = pad;
        for c in &chars {
            if let Some(bits) = np::char_bits(*c) {
                for (r_idx, row_bits) in bits.iter().enumerate() {
                    let target_row = row_offset + r_idx;
                    for col_idx in 0..char_w {
                        if row_bits & (1u16 << col_idx) != 0 {
                            rows[target_row][col_cursor + col_idx] = true;
                        }
                    }
                }
            }
            col_cursor += char_w + gap;
        }

        Some(Self { rows })
    }

    fn total_cols(&self) -> usize {
        self.rows[0].len()
    }

    /// Extract a 9-col window starting at `start_col` and pack into a
    /// `Glyph`. Cols beyond canvas width render as dark.
    fn frame(&self, start_col: usize) -> np::Glyph {
        let mut glyph_rows = [0u16; np::LED_ROWS];
        for (r, row) in self.rows.iter().enumerate() {
            let mut bits = 0u16;
            for c in 0..np::LED_COLS {
                let canvas_col = start_col + c;
                if canvas_col < row.len() && row[canvas_col] {
                    bits |= 1u16 << c;
                }
            }
            glyph_rows[r] = bits;
        }
        np::Glyph { rows: glyph_rows }
    }
}

const SCROLL_FRAME_MS: u64 = 120;

async fn run_scroll_animation(text: String, device_id: String, sink: Arc<dyn LedFeedbackSink>) {
    let Some(canvas) = ScrollCanvas::from_text(&text) else {
        // Empty after non-ASCII filter — show '?' once and stop.
        let glyph = np::char_glyph('?');
        let opts = np::DisplayOptions {
            brightness: 1.0,
            timeout_ms: 2000,
            transition: np::DisplayTransition::CrossFade,
        };
        sink.write_led(device_id, np::build_led_payload(&glyph, &opts).to_vec());
        return;
    };

    // Number of windows to render: from start_col=0 (right edge of
    // text just entering the LED) to start_col=total_cols-LED_COLS
    // (left edge fully scrolled past). +1 for the final window.
    let total_frames = canvas
        .total_cols()
        .saturating_sub(np::LED_COLS)
        .saturating_add(1);
    let mut interval = tokio::time::interval(Duration::from_millis(SCROLL_FRAME_MS));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    for frame_idx in 0..total_frames {
        interval.tick().await;
        let glyph = canvas.frame(frame_idx);
        let opts = np::DisplayOptions {
            brightness: 1.0,
            // Slightly longer than the frame interval so the LED
            // doesn't blank between writes; the next write replaces
            // before the timeout expires.
            timeout_ms: 250,
            transition: np::DisplayTransition::Immediate,
        };
        let payload = np::build_led_payload(&glyph, &opts).to_vec();
        sink.write_led(device_id.clone(), payload);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::sync::Mutex as StdMutex;
    use uuid::Uuid;
    use weave_contracts::{Mapping, Route};

    /// Test sink that records every (device_id, payload) call.
    struct RecordingSink {
        captured: StdMutex<Vec<(String, Vec<u8>)>>,
    }

    impl RecordingSink {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                captured: StdMutex::new(Vec::new()),
            })
        }

        fn captured(&self) -> Vec<(String, Vec<u8>)> {
            self.captured.lock().unwrap().clone()
        }
    }

    impl LedFeedbackSink for RecordingSink {
        fn write_led(&self, device_id: String, payload: Vec<u8>) {
            self.captured.lock().unwrap().push((device_id, payload));
        }
    }

    fn ios_media_mapping_with_feedback(device_id: &str, feedback: Vec<FeedbackRule>) -> Mapping {
        Mapping {
            mapping_id: Uuid::new_v4(),
            edge_id: "ipad".into(),
            device_type: "nuimo".into(),
            device_id: device_id.into(),
            service_type: "ios_media".into(),
            service_target: "apple_music".into(),
            routes: vec![Route {
                input: "press".into(),
                intent: "play_pause".into(),
                params: BTreeMap::new(),
            }],
            feedback,
            active: true,
            target_candidates: vec![],
            target_switch_on: None,
        }
    }

    fn glyph_rule_for_playback() -> FeedbackRule {
        FeedbackRule {
            state: "playback".into(),
            feedback_type: "glyph".into(),
            mapping: serde_json::json!({
                "playing": "play",
                "paused": "pause",
                "stopped": "pause",
            }),
        }
    }

    fn volume_bar_rule() -> FeedbackRule {
        FeedbackRule {
            state: "volume".into(),
            feedback_type: "volume_bar".into(),
            mapping: serde_json::json!({}),
        }
    }

    fn track_scroll_rule() -> FeedbackRule {
        FeedbackRule {
            state: "now_playing".into(),
            feedback_type: "track_scroll".into(),
            mapping: serde_json::json!({}),
        }
    }

    fn letter_rule() -> FeedbackRule {
        FeedbackRule {
            state: "target_hint".into(),
            feedback_type: "letter".into(),
            mapping: serde_json::json!({}),
        }
    }

    #[test]
    fn letter_signature_includes_char() {
        assert_eq!(FeedbackPlan::Letter('A').signature(), "letter:A");
        assert_eq!(FeedbackPlan::Letter('?').signature(), "letter:?");
    }

    #[test]
    fn scroll_text_signature_includes_text() {
        let plan = FeedbackPlan::ScrollText("Bohemian".into());
        assert_eq!(plan.signature(), "scroll:Bohemian");
    }

    #[test]
    fn from_rules_track_scroll_extracts_title() {
        let rules = vec![track_scroll_rule()];
        let update = StateUpdate {
            service_type: "ios_media".into(),
            target: "apple_music".into(),
            property: "now_playing".into(),
            value: serde_json::json!({
                "state": "playing",
                "title": "Lateralus",
                "artist": "Tool",
            }),
        };
        let plan = FeedbackPlan::from_rules(&update, &rules).expect("title extracted");
        assert_eq!(plan, FeedbackPlan::ScrollText("Lateralus".into()));
    }

    #[test]
    fn from_rules_track_scroll_skips_when_title_missing() {
        let rules = vec![track_scroll_rule()];
        let update = StateUpdate {
            service_type: "ios_media".into(),
            target: "apple_music".into(),
            property: "now_playing".into(),
            value: serde_json::json!({"state": "stopped"}),
        };
        assert!(FeedbackPlan::from_rules(&update, &rules).is_none());
    }

    #[test]
    fn from_rules_letter_single_char() {
        let rules = vec![letter_rule()];
        let update = StateUpdate {
            service_type: "ios_media".into(),
            target: "x".into(),
            property: "target_hint".into(),
            value: serde_json::json!("A"),
        };
        let plan = FeedbackPlan::from_rules(&update, &rules).expect("letter A");
        assert_eq!(plan, FeedbackPlan::Letter('A'));
    }

    #[test]
    fn from_rules_letter_rejects_multi_char() {
        let rules = vec![letter_rule()];
        let update = StateUpdate {
            service_type: "ios_media".into(),
            target: "x".into(),
            property: "target_hint".into(),
            value: serde_json::json!("AB"),
        };
        assert!(FeedbackPlan::from_rules(&update, &rules).is_none());
    }

    #[test]
    fn scroll_canvas_filters_non_ascii_keeps_supported_chars() {
        // "Hi 日本" → "HI " after filter (' ' is supported, lowercase
        // upcased by char_pattern, "日本" has no font entry).
        let canvas = ScrollCanvas::from_text("Hi 日本").expect("non-empty after filter");
        // Not asserting exact width because of padding details, just
        // that something was composed.
        assert!(canvas.total_cols() > np::LED_COLS * 2);
    }

    #[test]
    fn scroll_canvas_returns_none_for_all_non_ascii() {
        assert!(ScrollCanvas::from_text("日本語").is_none());
        assert!(ScrollCanvas::from_text("").is_none());
    }

    #[test]
    fn scroll_canvas_first_frame_starts_blank_for_pad() {
        let canvas = ScrollCanvas::from_text("A").expect("non-empty");
        let glyph = canvas.frame(0);
        // First LED_COLS columns of the canvas are left padding —
        // every row in this window should be 0.
        assert!(glyph.rows.iter().all(|&r| r == 0));
    }

    #[tokio::test]
    async fn from_rules_glyph_resolves_state_value_to_named_glyph() {
        let rules = vec![glyph_rule_for_playback()];
        let update = StateUpdate {
            service_type: "ios_media".into(),
            target: "apple_music".into(),
            property: "playback".into(),
            value: serde_json::json!("playing"),
        };
        let plan = FeedbackPlan::from_rules(&update, &rules).expect("playing → play");
        assert_eq!(plan, FeedbackPlan::NamedGlyph("play".into()));
    }

    #[tokio::test]
    async fn from_rules_glyph_skips_when_value_is_not_a_string() {
        // The PR #50 / #52 issue: now_playing carries an object, so the
        // glyph rule cannot bind a state value and must not fire.
        let rules = vec![glyph_rule_for_playback()];
        let update = StateUpdate {
            service_type: "ios_media".into(),
            target: "apple_music".into(),
            property: "playback".into(),
            value: serde_json::json!({"nested": "playing"}),
        };
        assert!(FeedbackPlan::from_rules(&update, &rules).is_none());
    }

    #[tokio::test]
    async fn from_rules_volume_bar_uses_numeric_value() {
        let rules = vec![volume_bar_rule()];
        let update = StateUpdate {
            service_type: "ios_media".into(),
            target: "apple_music".into(),
            property: "volume".into(),
            value: serde_json::json!(47.5),
        };
        let plan = FeedbackPlan::from_rules(&update, &rules).expect("47.5 → 4 bars");
        assert!(matches!(
            plan,
            FeedbackPlan::VolumeBar(4, VolumeDirection::BottomUp)
        ));
    }

    #[tokio::test]
    async fn from_default_playback_paused_falls_back_to_pause_glyph() {
        let update = StateUpdate {
            service_type: "ios_media".into(),
            target: "apple_music".into(),
            property: "playback".into(),
            value: serde_json::json!("paused"),
        };
        let plan = FeedbackPlan::from_default(&update).expect("default plan exists");
        assert_eq!(plan, FeedbackPlan::NamedGlyph("pause".into()));
    }

    #[tokio::test]
    async fn signature_dedups_identical_plans() {
        let mut filter = FeedbackFilter::new();
        let plan = FeedbackPlan::NamedGlyph("play".into());
        let sig = plan.signature();
        assert!(filter.should_render("nuimo-1", "playback", &sig));
        // Same property, same signature → no second write.
        assert!(!filter.should_render("nuimo-1", "playback", &sig));
    }

    #[tokio::test]
    async fn signature_per_device_independent() {
        let mut filter = FeedbackFilter::new();
        let sig = "play".to_string();
        assert!(filter.should_render("nuimo-a", "playback", &sig));
        // Different device, same plan → still renders (its own LED).
        assert!(filter.should_render("nuimo-b", "playback", &sig));
    }

    #[tokio::test]
    async fn different_property_does_not_throttle_volume_after_playback() {
        // Regression: NowPlayingObserver.publish() emits playback then
        // volume in rapid succession. With device-only throttling the
        // volume frame got dropped because the playback write was
        // <100 ms ago — LED stayed on the playback glyph and never
        // showed the volume bar. Per-(device, property) keys keep the
        // two tracks independent.
        let mut filter = FeedbackFilter::new();
        assert!(filter.should_render("nuimo-1", "playback", "pause"));
        // Immediate follow-up on a different property — must NOT be
        // throttled by the recent playback write.
        assert!(filter.should_render("nuimo-1", "volume", "vol:5:up"));
    }

    #[tokio::test]
    async fn dispatch_writes_payload_via_sink_for_each_matching_device() {
        let engine = RoutingEngine::new();
        // Two Nuimos paired to the same iPad, both routed at the same
        // (service_type, target).
        engine
            .replace_all(vec![
                ios_media_mapping_with_feedback("nuimo-1", vec![glyph_rule_for_playback()]),
                ios_media_mapping_with_feedback("nuimo-2", vec![glyph_rule_for_playback()]),
            ])
            .await;
        let registry = GlyphRegistry::new();
        registry
            .replace_all(vec![weave_contracts::Glyph {
                name: "play".into(),
                pattern: "    *    \n   ***   \n  *****  ".into(),
                builtin: false,
            }])
            .await;
        let sink: Arc<RecordingSink> = RecordingSink::new();
        let sink_slot: Arc<StdMutex<Option<Arc<dyn LedFeedbackSink>>>> = Arc::new(StdMutex::new(
            Some(sink.clone() as Arc<dyn LedFeedbackSink>),
        ));
        let mut filter = FeedbackFilter::new();
        let update = StateUpdate {
            service_type: "ios_media".into(),
            target: "apple_music".into(),
            property: "playback".into(),
            value: serde_json::json!("playing"),
        };

        dispatch(
            &update,
            &engine,
            &registry,
            &sink_slot,
            &mut filter,
            &mut HashMap::new(),
        )
        .await;

        let captured = sink.captured();
        assert_eq!(captured.len(), 2, "both Nuimos receive the play frame");
        let device_ids: Vec<&str> = captured.iter().map(|(d, _)| d.as_str()).collect();
        assert!(device_ids.contains(&"nuimo-1"));
        assert!(device_ids.contains(&"nuimo-2"));
        // 13-byte LED payload format.
        assert_eq!(captured[0].1.len(), 13);
    }

    #[tokio::test]
    async fn dispatch_skips_when_property_does_not_match_any_rule() {
        let engine = RoutingEngine::new();
        engine
            .replace_all(vec![ios_media_mapping_with_feedback(
                "nuimo-1",
                vec![glyph_rule_for_playback()],
            )])
            .await;
        let registry = GlyphRegistry::new();
        let sink: Arc<RecordingSink> = RecordingSink::new();
        let sink_slot: Arc<StdMutex<Option<Arc<dyn LedFeedbackSink>>>> = Arc::new(StdMutex::new(
            Some(sink.clone() as Arc<dyn LedFeedbackSink>),
        ));
        let mut filter = FeedbackFilter::new();
        // `now_playing` is the property iOS used to publish object-shaped
        // updates under. The pump must not light the LED for these — they
        // exist for UI consumption only.
        let update = StateUpdate {
            service_type: "ios_media".into(),
            target: "apple_music".into(),
            property: "now_playing".into(),
            value: serde_json::json!({"title": "x", "state": "playing"}),
        };

        dispatch(
            &update,
            &engine,
            &registry,
            &sink_slot,
            &mut filter,
            &mut HashMap::new(),
        )
        .await;

        assert!(sink.captured().is_empty());
    }

    #[tokio::test]
    async fn dispatch_no_op_when_sink_unregistered() {
        let engine = RoutingEngine::new();
        engine
            .replace_all(vec![ios_media_mapping_with_feedback(
                "nuimo-1",
                vec![glyph_rule_for_playback()],
            )])
            .await;
        let registry = GlyphRegistry::new();
        let sink_slot: Arc<StdMutex<Option<Arc<dyn LedFeedbackSink>>>> =
            Arc::new(StdMutex::new(None));
        let mut filter = FeedbackFilter::new();
        let update = StateUpdate {
            service_type: "ios_media".into(),
            target: "apple_music".into(),
            property: "playback".into(),
            value: serde_json::json!("playing"),
        };

        // No panic, no error — pump waits for register_led_feedback_callback.
        dispatch(
            &update,
            &engine,
            &registry,
            &sink_slot,
            &mut filter,
            &mut HashMap::new(),
        )
        .await;
    }
}
