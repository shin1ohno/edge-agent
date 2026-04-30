//! 5x7 ASCII bitmap font for the Nuimo 9x9 LED.
//!
//! The font is intentionally minimal: uppercase `A-Z`, digits `0-9`,
//! and a blank for space — enough to render a single-letter target
//! hint or a scrolling track-name preview. Lowercase callers should
//! pre-uppercase via `char::to_ascii_uppercase` because we don't
//! ship a separate lowercase set.
//!
//! Glyphs are stored as 7 strings of 5 chars each (`*` = on, anything
//! else = off). `char_glyph` centres the 5x7 pattern in the 9x9
//! `Glyph` grid (col offset 2, row offset 1). `char_bits` exposes
//! the same data as 5 packed bits per row for callers that need the
//! raw bitmap (the scroll-text renderer composes wider canvases out
//! of these).

use crate::{Glyph, LED_COLS, LED_ROWS};

/// Width of a font cell in columns.
pub const FONT_CHAR_WIDTH: usize = 5;

/// Height of a font cell in rows.
pub const FONT_CHAR_HEIGHT: usize = 7;

/// Look up the 5x7 ASCII-art pattern for `c` (uppercased internally).
/// Returns `None` for characters outside `[A-Z0-9 ]`.
pub fn char_pattern(c: char) -> Option<[&'static str; FONT_CHAR_HEIGHT]> {
    let upper = c.to_ascii_uppercase();
    Some(match upper {
        ' ' => [".....", ".....", ".....", ".....", ".....", ".....", "....."],
        'A' => [".***.", "*...*", "*...*", "*****", "*...*", "*...*", "*...*"],
        'B' => ["****.", "*...*", "*...*", "****.", "*...*", "*...*", "****."],
        'C' => [".****", "*....", "*....", "*....", "*....", "*....", ".****"],
        'D' => ["****.", "*...*", "*...*", "*...*", "*...*", "*...*", "****."],
        'E' => ["*****", "*....", "*....", "****.", "*....", "*....", "*****"],
        'F' => ["*****", "*....", "*....", "****.", "*....", "*....", "*...."],
        'G' => [".****", "*....", "*....", "*..**", "*...*", "*...*", ".****"],
        'H' => ["*...*", "*...*", "*...*", "*****", "*...*", "*...*", "*...*"],
        'I' => ["*****", "..*..", "..*..", "..*..", "..*..", "..*..", "*****"],
        'J' => ["..***", "....*", "....*", "....*", "....*", "*...*", ".***."],
        'K' => ["*...*", "*..*.", "*.*..", "**...", "*.*..", "*..*.", "*...*"],
        'L' => ["*....", "*....", "*....", "*....", "*....", "*....", "*****"],
        'M' => ["*...*", "**.**", "*.*.*", "*...*", "*...*", "*...*", "*...*"],
        'N' => ["*...*", "**..*", "*.*.*", "*..**", "*...*", "*...*", "*...*"],
        'O' => [".***.", "*...*", "*...*", "*...*", "*...*", "*...*", ".***."],
        'P' => ["****.", "*...*", "*...*", "****.", "*....", "*....", "*...."],
        'Q' => [".***.", "*...*", "*...*", "*...*", "*.*.*", "*..*.", ".**.*"],
        'R' => ["****.", "*...*", "*...*", "****.", "*.*..", "*..*.", "*...*"],
        'S' => [".****", "*....", "*....", ".***.", "....*", "....*", "****."],
        'T' => ["*****", "..*..", "..*..", "..*..", "..*..", "..*..", "..*.."],
        'U' => ["*...*", "*...*", "*...*", "*...*", "*...*", "*...*", ".***."],
        'V' => ["*...*", "*...*", "*...*", "*...*", "*...*", ".*.*.", "..*.."],
        'W' => ["*...*", "*...*", "*...*", "*...*", "*.*.*", "**.**", "*...*"],
        'X' => ["*...*", "*...*", ".*.*.", "..*..", ".*.*.", "*...*", "*...*"],
        'Y' => ["*...*", "*...*", ".*.*.", "..*..", "..*..", "..*..", "..*.."],
        'Z' => ["*****", "....*", "...*.", "..*..", ".*...", "*....", "*****"],
        '0' => [".***.", "*...*", "*..**", "*.*.*", "**..*", "*...*", ".***."],
        '1' => ["..*..", ".**..", "..*..", "..*..", "..*..", "..*..", ".***."],
        '2' => [".***.", "*...*", "....*", "...*.", "..*..", ".*...", "*****"],
        '3' => ["*****", "....*", "...*.", "..**.", "....*", "*...*", ".***."],
        '4' => ["...*.", "..**.", ".*.*.", "*..*.", "*****", "...*.", "...*."],
        '5' => ["*****", "*....", "****.", "....*", "....*", "*...*", ".***."],
        '6' => [".***.", "*....", "*....", "****.", "*...*", "*...*", ".***."],
        '7' => ["*****", "....*", "...*.", "..*..", ".*...", ".*...", ".*..."],
        '8' => [".***.", "*...*", "*...*", ".***.", "*...*", "*...*", ".***."],
        '9' => [".***.", "*...*", "*...*", ".****", "....*", "....*", ".***."],
        '?' => [".***.", "*...*", "....*", "...*.", "..*..", ".....", "..*.."],
        _ => return None,
    })
}

/// Same data as `char_pattern` but as 5 packed bits per row (bit 0 =
/// leftmost column). Useful for callers that need to slide multiple
/// characters across a scroll buffer without parsing strings each
/// frame.
pub fn char_bits(c: char) -> Option<[u16; FONT_CHAR_HEIGHT]> {
    let pattern = char_pattern(c)?;
    let mut rows = [0u16; FONT_CHAR_HEIGHT];
    for (r, line) in pattern.iter().enumerate() {
        let mut bits = 0u16;
        for (col, ch) in line.chars().enumerate() {
            if ch == '*' {
                bits |= 1 << col;
            }
        }
        rows[r] = bits;
    }
    Some(rows)
}

/// Render `c` centred in the 9x9 LED grid. Characters outside the
/// supported set fall back to `?`.
pub fn char_glyph(c: char) -> Glyph {
    let pattern = char_pattern(c).or_else(|| char_pattern('?')).unwrap();
    let row_offset = (LED_ROWS - FONT_CHAR_HEIGHT) / 2; // 1
    let col_offset = (LED_COLS - FONT_CHAR_WIDTH) / 2; // 2

    let mut rows = [0u16; LED_ROWS];
    for (r, line) in pattern.iter().enumerate() {
        let mut row_bits = 0u16;
        for (col, ch) in line.chars().enumerate() {
            if ch == '*' {
                row_bits |= 1 << (col_offset + col);
            }
        }
        rows[row_offset + r] = row_bits;
    }
    Glyph { rows }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_renders_at_5x7_centred_in_9x9() {
        let glyph = char_glyph('A');
        // Row 0 + row 8 are padding, rows 1..=7 hold the 5x7 cell.
        assert_eq!(glyph.rows[0], 0);
        assert_eq!(glyph.rows[8], 0);
        // Row 1 is `.***.` shifted right by col_offset=2, so cols 3..=5 lit.
        assert_eq!(glyph.rows[1], 0b00_0111_000);
        // Row 4 is `*****` (full 5x7 row).
        assert_eq!(glyph.rows[4], 0b00_1111_100);
    }

    #[test]
    fn lowercase_renders_via_uppercase() {
        assert_eq!(char_glyph('a'), char_glyph('A'));
        assert_eq!(char_glyph('z'), char_glyph('Z'));
    }

    #[test]
    fn unsupported_falls_back_to_question_mark() {
        assert_eq!(char_glyph('@'), char_glyph('?'));
    }

    #[test]
    fn space_is_blank() {
        let glyph = char_glyph(' ');
        assert!(glyph.rows.iter().all(|&r| r == 0));
    }

    #[test]
    fn char_bits_round_trips_through_char_pattern() {
        let pattern = char_pattern('A').unwrap();
        let bits = char_bits('A').unwrap();
        for (r, line) in pattern.iter().enumerate() {
            let mut expected = 0u16;
            for (c, ch) in line.chars().enumerate() {
                if ch == '*' {
                    expected |= 1 << c;
                }
            }
            assert_eq!(bits[r], expected, "row {r}");
        }
    }

    #[test]
    fn all_supported_chars_have_correct_dimensions() {
        for c in ('A'..='Z').chain('0'..='9').chain([' ', '?']) {
            let pattern = char_pattern(c).unwrap_or_else(|| panic!("missing char {c}"));
            assert_eq!(pattern.len(), FONT_CHAR_HEIGHT);
            for line in &pattern {
                assert_eq!(line.len(), FONT_CHAR_WIDTH, "char {c} row width");
            }
        }
    }
}
