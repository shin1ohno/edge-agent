//! Parametric glyph renderers kept local to the agent.
//!
//! Named static patterns (play, pause, next, previous, link) are fetched from
//! weave via `ConfigFull.glyphs` and stored in `edge_core::GlyphRegistry`.
//! `volume_bar` stays here because it's computed from a percentage.

use nuimo::Glyph;

/// Volume bar glyph (0-100%). Matches weave's `volume_bar` registration
/// (builtin = true, empty pattern).
pub fn volume(percentage: u8) -> Glyph {
    let bars = ((percentage as f64 / 100.0) * 9.0).round() as usize;
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
