//! Parametric glyph renderers kept local to the agent.
//!
//! Named static patterns (play, pause, next, previous, link) are fetched from
//! weave via `ConfigFull.glyphs` and stored in `edge_core::GlyphRegistry`.
//! `volume_bar` stays here because it's computed from a percentage.

use nuimo::Glyph;

/// Fill direction for the volume bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VolumeDirection {
    /// 0-bars = empty bottom; 9-bars = full, bottom-up fill. Used for
    /// linear volumes (0..=max).
    BottomUp,
    /// 0-bars = empty; 9-bars = full, top-down fill (topmost row lit
    /// first). Used for dB-style volumes where the max value is 0 and
    /// negative values reduce loudness — the top LED represents "at 0
    /// dB", and fewer dots below mean more attenuation.
    TopDown,
}

/// Volume bar glyph, 0..=9 LEDs lit. Direction controls whether the bar
/// fills from the bottom up or the top down.
///
/// The bar count + direction is the rendered unit of change; above this
/// layer the feedback filter dedups on the tuple so two Roon updates that
/// round to the same (bars, direction) don't trigger a redraw.
pub fn volume_bars(bars: u8, direction: VolumeDirection) -> Glyph {
    let bars = bars.min(9) as usize;
    let mut rows = String::new();
    for row in 0..9 {
        let lit = match direction {
            VolumeDirection::BottomUp => (8 - row) < bars,
            VolumeDirection::TopDown => row < bars,
        };
        if lit {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn render(bars: u8, dir: VolumeDirection) -> String {
        let mut rows = String::new();
        for row in 0..9 {
            let lit = match dir {
                VolumeDirection::BottomUp => (8 - row) < bars.min(9) as usize,
                VolumeDirection::TopDown => row < bars.min(9) as usize,
            };
            rows.push(if lit { '*' } else { '.' });
            if row < 8 {
                rows.push('\n');
            }
        }
        rows
    }

    #[test]
    fn bottom_up_three_lights_bottom_rows() {
        assert_eq!(
            render(3, VolumeDirection::BottomUp),
            ".\n.\n.\n.\n.\n.\n*\n*\n*"
        );
        // Sanity: the returned Glyph is identical to the corresponding
        // hand-built one (relies on nuimo::Glyph's PartialEq impl).
        assert_eq!(
            volume_bars(3, VolumeDirection::BottomUp),
            nuimo::Glyph::from_str(
                "         \n         \n         \n         \n         \n         \n    *    \n    *    \n    *    "
            )
        );
    }

    #[test]
    fn top_down_three_lights_top_rows() {
        assert_eq!(
            render(3, VolumeDirection::TopDown),
            "*\n*\n*\n.\n.\n.\n.\n.\n."
        );
        assert_eq!(
            volume_bars(3, VolumeDirection::TopDown),
            nuimo::Glyph::from_str(
                "    *    \n    *    \n    *    \n         \n         \n         \n         \n         \n         "
            )
        );
    }

    #[test]
    fn nine_is_full_either_direction() {
        assert_eq!(
            volume_bars(9, VolumeDirection::BottomUp),
            volume_bars(9, VolumeDirection::TopDown)
        );
    }

    #[test]
    fn zero_is_empty_either_direction() {
        assert_eq!(
            volume_bars(0, VolumeDirection::BottomUp),
            volume_bars(0, VolumeDirection::TopDown)
        );
    }
}
