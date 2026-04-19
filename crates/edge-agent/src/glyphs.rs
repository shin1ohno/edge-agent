//! Parametric glyph renderers kept local to the agent.
//!
//! Named static patterns (play, pause, next, previous, link) are fetched from
//! weave via `ConfigFull.glyphs` and stored in `edge_core::GlyphRegistry`.
//! `volume_bar` stays here because it's computed from a percentage.

use nuimo::Glyph;

/// Volume bar glyph, 0..=9 LEDs lit from the bottom row upward.
///
/// The bar count is the rendered unit of change: above this layer, feedback
/// is dedup'd on `bars` so two Roon updates that round to the same bar count
/// don't trigger a redraw.
pub fn volume_bars(bars: u8) -> Glyph {
    let bars = bars.min(9) as usize;
    let mut rows = String::new();
    for row in 0..9 {
        let from_bottom = 8 - row;
        if from_bottom < bars {
            rows.push_str("    *    ");
        } else {
            rows.push_str("         ");
        }
        if row < 8 {
            rows.push('\n');
        }
    }
    Glyph::from_str(&rows)
}
