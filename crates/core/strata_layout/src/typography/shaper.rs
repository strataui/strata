//! OpenType text shaper: GSUB ligatures, GPOS pair kerning, and glyph placement.
//!
//! Evaluates code ligatures (e.g., `===`, `!=`, `=>`), typographic ligatures (`fi`, `fl`),
//! optical pair kerning adjustments, and subpixel glyph advance positioning.

use strata_primitives::geom::LayoutUnit;
use super::font_metrics::FontMetrics;

/// Single positioned glyph resulting from text shaping.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShapedGlyph {
    /// Glyph identifier (index in font face or unicode scalar).
    pub glyph_id: u32,
    /// UTF-8 byte offset in original text corresponding to this glyph cluster.
    pub cluster: u32,
    /// Number of UTF-8 bytes consumed in original source text by this cluster.
    pub source_bytes: u16,
    /// Horizontal advance width to the next glyph origin.
    pub x_advance: LayoutUnit,
    /// Vertical advance (0 in horizontal writing mode).
    pub y_advance: LayoutUnit,
    /// Horizontal subpixel offset relative to glyph origin.
    pub x_offset: LayoutUnit,
    /// Vertical subpixel offset relative to baseline.
    pub y_offset: LayoutUnit,
    /// Whether this glyph was produced by a multi-character ligature substitution.
    pub is_ligature: bool,
    /// Whether this glyph represents a space or non-breaking whitespace.
    pub is_space: bool,
}

/// Known ligature definition with replacement glyph token.
struct LigatureRule {
    pattern: &'static str,
    glyph_id: u32,
    advance_factor: f32,
}

static KNOWN_LIGATURES: &[LigatureRule] = &[
    // 4-character ligatures
    LigatureRule { pattern: "<!--", glyph_id: 0xF001, advance_factor: 2.4 },
    // 3-character code ligatures
    LigatureRule { pattern: "===", glyph_id: 0xF002, advance_factor: 1.8 },
    LigatureRule { pattern: "!==", glyph_id: 0xF003, advance_factor: 1.8 },
    LigatureRule { pattern: "-->", glyph_id: 0xF004, advance_factor: 1.8 },
    LigatureRule { pattern: "...", glyph_id: 0xF005, advance_factor: 1.2 },
    LigatureRule { pattern: "ffi", glyph_id: 0xFB03, advance_factor: 1.4 },
    LigatureRule { pattern: "ffl", glyph_id: 0xFB04, advance_factor: 1.4 },
    // 2-character code & typographic ligatures
    LigatureRule { pattern: "==", glyph_id: 0xF006, advance_factor: 1.3 },
    LigatureRule { pattern: "!=", glyph_id: 0xF007, advance_factor: 1.3 },
    LigatureRule { pattern: "<=", glyph_id: 0xF008, advance_factor: 1.3 },
    LigatureRule { pattern: ">=", glyph_id: 0xF009, advance_factor: 1.3 },
    LigatureRule { pattern: "=>", glyph_id: 0xF00A, advance_factor: 1.3 },
    LigatureRule { pattern: "->", glyph_id: 0xF00B, advance_factor: 1.3 },
    LigatureRule { pattern: "<-", glyph_id: 0xF00C, advance_factor: 1.3 },
    LigatureRule { pattern: "::", glyph_id: 0xF00D, advance_factor: 1.0 },
    LigatureRule { pattern: "fi", glyph_id: 0xFB01, advance_factor: 0.95 },
    LigatureRule { pattern: "fl", glyph_id: 0xFB02, advance_factor: 0.95 },
    LigatureRule { pattern: "ff", glyph_id: 0xFB00, advance_factor: 0.95 },
];

/// Known optical pair kerning adjustment rules (fractions of em).
struct KerningPair {
    first: char,
    second: char,
    delta_em: f32,
}

static KNOWN_KERNING_PAIRS: &[KerningPair] = &[
    KerningPair { first: 'A', second: 'V', delta_em: -0.085 },
    KerningPair { first: 'V', second: 'A', delta_em: -0.085 },
    KerningPair { first: 'A', second: 'W', delta_em: -0.070 },
    KerningPair { first: 'W', second: 'A', delta_em: -0.070 },
    KerningPair { first: 'A', second: 'T', delta_em: -0.075 },
    KerningPair { first: 'T', second: 'A', delta_em: -0.075 },
    KerningPair { first: 'T', second: 'o', delta_em: -0.070 },
    KerningPair { first: 'T', second: 'e', delta_em: -0.065 },
    KerningPair { first: 'W', second: 'e', delta_em: -0.045 },
    KerningPair { first: 'P', second: 'a', delta_em: -0.055 },
    KerningPair { first: 'P', second: 'o', delta_em: -0.050 },
    KerningPair { first: 'Y', second: 'o', delta_em: -0.080 },
    KerningPair { first: 'Y', second: 'a', delta_em: -0.075 },
    KerningPair { first: 'F', second: 'a', delta_em: -0.060 },
    KerningPair { first: 'L', second: 'T', delta_em: -0.065 },
    KerningPair { first: 'L', second: 'V', delta_em: -0.070 },
    KerningPair { first: 'r', second: 'c', delta_em: -0.025 },
    KerningPair { first: 'f', second: 'i', delta_em: -0.020 },
];

/// Pure-Rust OpenType text shaping engine.
#[derive(Debug, Clone, Copy, Default)]
pub struct TextShaper;

impl TextShaper {
    pub fn new() -> Self {
        Self
    }

    /// Shapes a text run into positioned glyphs, applying ligatures and pair kerning.
    pub fn shape(
        &self,
        text: &str,
        metrics: &FontMetrics,
        is_monospace: bool,
    ) -> Vec<ShapedGlyph> {
        if text.is_empty() {
            return Vec::new();
        }

        let mut glyphs = Vec::with_capacity(text.len());
        let bytes = text.as_bytes();
        let mut byte_idx = 0;

        while byte_idx < bytes.len() {
            let slice = &text[byte_idx..];

            // 1. Check for multi-character ligatures if not monospace
            let mut matched_ligature = None;
            if !is_monospace {
                for lig in KNOWN_LIGATURES {
                    if slice.starts_with(lig.pattern) {
                        matched_ligature = Some(lig);
                        break;
                    }
                }
            }

            if let Some(lig) = matched_ligature {
                let x_advance = LayoutUnit::from_f32(metrics.font_size * lig.advance_factor);
                glyphs.push(ShapedGlyph {
                    glyph_id: lig.glyph_id,
                    cluster: byte_idx as u32,
                    source_bytes: lig.pattern.len() as u16,
                    x_advance,
                    y_advance: LayoutUnit::ZERO,
                    x_offset: LayoutUnit::ZERO,
                    y_offset: LayoutUnit::ZERO,
                    is_ligature: true,
                    is_space: false,
                });
                byte_idx += lig.pattern.len();
                continue;
            }

            // 2. Extract next Unicode character
            let ch = match slice.chars().next() {
                Some(c) => c,
                None => break,
            };
            let char_len = ch.len_utf8();

            // 3. Determine base glyph advance
            let base_advance = if is_monospace {
                metrics.font_size * 0.60
            } else {
                Self::character_advance(ch, metrics.font_size)
            };

            let is_space = ch == ' ' || ch == '\t' || ch == '\u{00A0}';
            let glyph_id = ch as u32;

            glyphs.push(ShapedGlyph {
                glyph_id,
                cluster: byte_idx as u32,
                source_bytes: char_len as u16,
                x_advance: LayoutUnit::from_f32(base_advance),
                y_advance: LayoutUnit::ZERO,
                x_offset: LayoutUnit::ZERO,
                y_offset: LayoutUnit::ZERO,
                is_ligature: false,
                is_space,
            });

            byte_idx += char_len;
        }

        // 4. Apply GPOS optical pair kerning if proportional
        if !is_monospace && glyphs.len() > 1 {
            for i in 0..glyphs.len() - 1 {
                let id1 = glyphs[i].glyph_id;
                let id2 = glyphs[i + 1].glyph_id;

                if let (Some(c1), Some(c2)) = (char::from_u32(id1), char::from_u32(id2)) {
                    if let Some(delta) = Self::lookup_kerning(c1, c2) {
                        let adjustment = LayoutUnit::from_f32(metrics.font_size * delta);
                        glyphs[i].x_advance = glyphs[i].x_advance + adjustment;
                    }
                }
            }
        }

        glyphs
    }

    /// Looks up optical kerning adjustment between two characters in fraction of em.
    fn lookup_kerning(c1: char, c2: char) -> Option<f32> {
        for kp in KNOWN_KERNING_PAIRS {
            if kp.first == c1 && kp.second == c2 {
                return Some(kp.delta_em);
            }
        }
        None
    }

    /// Proportional character advance width for typical modern sans-serif typography.
    fn character_advance(ch: char, font_size: f32) -> f32 {
        let em = font_size;

        // CJK Unified Ideographs, Hiragana, Katakana, Hangul: full-width em
        if (ch >= '\u{4E00}' && ch <= '\u{9FFF}')
            || (ch >= '\u{3040}' && ch <= '\u{30FF}')
            || (ch >= '\u{AC00}' && ch <= '\u{D7AF}')
        {
            return em * 1.0;
        }

        // Arabic characters (~0.45 - 0.70 em)
        if (ch >= '\u{0600}' && ch <= '\u{06FF}') || (ch >= '\u{0750}' && ch <= '\u{077F}') {
            return em * 0.52;
        }

        // Hebrew characters (~0.50 - 0.65 em)
        if ch >= '\u{0590}' && ch <= '\u{05FF}' {
            return em * 0.54;
        }

        match ch {
            // Very narrow characters
            'i' | 'l' | '!' | ';' | ':' | '|' | '.' | ',' | '\'' | '`' | '\u{200B}' => em * 0.26,
            'j' | 't' | 'r' | 'f' | '(' | ')' | '[' | ']' | '{' | '}' => em * 0.35,
            ' ' | '\u{00A0}' => em * 0.28,

            // Moderately narrow
            's' | 'c' | 'z' | 'k' | 'v' | 'x' | '-' | '*' => em * 0.46,

            // Average proportional lowercase
            'a' | 'b' | 'd' | 'e' | 'g' | 'h' | 'n' | 'o' | 'p' | 'q' | 'u' | 'y' => em * 0.54,

            // Average proportional uppercase & digits
            '0'..='9' | 'A'..='Z' => {
                match ch {
                    'M' | 'W' => em * 0.82,
                    'I' | 'J' => em * 0.34,
                    'E' | 'F' | 'L' => em * 0.52,
                    _ => em * 0.62,
                }
            }

            // Wide characters
            'w' | 'm' => em * 0.76,
            '@' | '%' => em * 0.85,
            '—' | '–' => em * 0.70,

            // Default proportional advance
            _ => em * 0.52,
        }
    }
}
