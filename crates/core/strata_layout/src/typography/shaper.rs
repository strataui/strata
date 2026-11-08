//! OpenType text shaper: GSUB ligatures, GPOS pair kerning, and glyph placement.
//!
//! Evaluates code ligatures (e.g., `===`, `!=`, `=>`), typographic ligatures (`fi`, `fl`),
//! optical pair kerning adjustments, and subpixel glyph advance positioning.

use super::font_metrics::FontMetrics;
use strata_primitives::geom::LayoutUnit;

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
    /// Whether this glyph represents a nonspacing combining mark (zero advance, anchors to base).
    pub is_mark: bool,
}

/// Classification of OpenType ligature rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LigatureKind {
    /// Standard typographic ligatures (`liga`).
    Standard,
    /// Contextual programming ligatures (`calt`).
    Contextual,
    /// Discretionary ligatures (`dlig`).
    Discretionary,
    /// Complex Indic script conjuncts.
    Indic,
}

/// OpenType GSUB/GPOS feature tags configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenTypeFeatures {
    /// Standard ligatures (`liga`, `clig`).
    pub ligatures: bool,
    /// Contextual alternates and programming code ligatures (`calt`).
    pub contextual: bool,
    /// Discretionary typographic ligatures (`dlig`, e.g. `st`, `ct`).
    pub discretionary: bool,
    /// Small capitals (`smcp`).
    pub small_caps: bool,
}

impl Default for OpenTypeFeatures {
    fn default() -> Self {
        Self {
            ligatures: true,
            contextual: true,
            discretionary: false,
            small_caps: false,
        }
    }
}

/// Maps ASCII lowercase characters to Unicode small-cap equivalents for `smcp` OpenType feature.
pub fn apply_small_caps(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            'a' => '\u{1D00}',
            'b' => '\u{1D04}',
            'c' => '\u{1D05}',
            'd' => '\u{1D07}',
            'e' => '\u{1D08}',
            'f' => '\u{A730}',
            'g' => '\u{0262}',
            'h' => '\u{029C}',
            'i' => '\u{026A}',
            'j' => '\u{1D0A}',
            'k' => '\u{1D0B}',
            'l' => '\u{029F}',
            'm' => '\u{1D0D}',
            'n' => '\u{0274}',
            'o' => '\u{1D0F}',
            'p' => '\u{1D18}',
            'q' => '\u{01EB}',
            'r' => '\u{0280}',
            's' => '\u{A731}',
            't' => '\u{1D1B}',
            'u' => '\u{1D1C}',
            'v' => '\u{1D20}',
            'w' => '\u{1D21}',
            'x' => 'x',
            'y' => '\u{028F}',
            'z' => '\u{1D22}',
            _ => c,
        })
        .collect()
}

/// Known ligature definition with replacement glyph token.
struct LigatureRule {
    pattern: &'static str,
    glyph_id: u32,
    advance_factor: f32,
    kind: LigatureKind,
}

static KNOWN_LIGATURES: &[LigatureRule] = &[
    // 4-character ligatures
    LigatureRule {
        pattern: "<!--",
        glyph_id: 0xF001,
        advance_factor: 2.4,
        kind: LigatureKind::Contextual,
    },
    // 3-character code ligatures
    LigatureRule {
        pattern: "===",
        glyph_id: 0xF002,
        advance_factor: 1.8,
        kind: LigatureKind::Contextual,
    },
    LigatureRule {
        pattern: "!==",
        glyph_id: 0xF003,
        advance_factor: 1.8,
        kind: LigatureKind::Contextual,
    },
    LigatureRule {
        pattern: "-->",
        glyph_id: 0xF004,
        advance_factor: 1.8,
        kind: LigatureKind::Contextual,
    },
    LigatureRule {
        pattern: "...",
        glyph_id: 0xF005,
        advance_factor: 1.2,
        kind: LigatureKind::Contextual,
    },
    LigatureRule {
        pattern: "ffi",
        glyph_id: 0xFB03,
        advance_factor: 1.4,
        kind: LigatureKind::Standard,
    },
    LigatureRule {
        pattern: "ffl",
        glyph_id: 0xFB04,
        advance_factor: 1.4,
        kind: LigatureKind::Standard,
    },
    // 2-character code & typographic ligatures
    LigatureRule {
        pattern: "==",
        glyph_id: 0xF006,
        advance_factor: 1.3,
        kind: LigatureKind::Contextual,
    },
    LigatureRule {
        pattern: "!=",
        glyph_id: 0xF007,
        advance_factor: 1.3,
        kind: LigatureKind::Contextual,
    },
    LigatureRule {
        pattern: "<=",
        glyph_id: 0xF008,
        advance_factor: 1.3,
        kind: LigatureKind::Contextual,
    },
    LigatureRule {
        pattern: ">=",
        glyph_id: 0xF009,
        advance_factor: 1.3,
        kind: LigatureKind::Contextual,
    },
    LigatureRule {
        pattern: "=>",
        glyph_id: 0xF00A,
        advance_factor: 1.3,
        kind: LigatureKind::Contextual,
    },
    LigatureRule {
        pattern: "->",
        glyph_id: 0xF00B,
        advance_factor: 1.3,
        kind: LigatureKind::Contextual,
    },
    LigatureRule {
        pattern: "<-",
        glyph_id: 0xF00C,
        advance_factor: 1.3,
        kind: LigatureKind::Contextual,
    },
    LigatureRule {
        pattern: "::",
        glyph_id: 0xF00D,
        advance_factor: 1.0,
        kind: LigatureKind::Contextual,
    },
    LigatureRule {
        pattern: "fi",
        glyph_id: 0xFB01,
        advance_factor: 0.95,
        kind: LigatureKind::Standard,
    },
    LigatureRule {
        pattern: "fl",
        glyph_id: 0xFB02,
        advance_factor: 0.95,
        kind: LigatureKind::Standard,
    },
    LigatureRule {
        pattern: "ff",
        glyph_id: 0xFB00,
        advance_factor: 0.95,
        kind: LigatureKind::Standard,
    },
    // Discretionary ligatures (dlig)
    LigatureRule {
        pattern: "st",
        glyph_id: 0xFB06,
        advance_factor: 1.0,
        kind: LigatureKind::Discretionary,
    },
    LigatureRule {
        pattern: "ct",
        glyph_id: 0xF020,
        advance_factor: 1.0,
        kind: LigatureKind::Discretionary,
    },
    LigatureRule {
        pattern: "sp",
        glyph_id: 0xF021,
        advance_factor: 1.0,
        kind: LigatureKind::Discretionary,
    },
    LigatureRule {
        pattern: "th",
        glyph_id: 0xF022,
        advance_factor: 1.0,
        kind: LigatureKind::Discretionary,
    },
    // Extended code operator ligatures
    LigatureRule {
        pattern: "||=",
        glyph_id: 0xF010,
        advance_factor: 1.8,
        kind: LigatureKind::Contextual,
    },
    LigatureRule {
        pattern: "&&=",
        glyph_id: 0xF011,
        advance_factor: 1.8,
        kind: LigatureKind::Contextual,
    },
    LigatureRule {
        pattern: "??=",
        glyph_id: 0xF012,
        advance_factor: 1.8,
        kind: LigatureKind::Contextual,
    },
    LigatureRule {
        pattern: ":=",
        glyph_id: 0xF013,
        advance_factor: 1.2,
        kind: LigatureKind::Contextual,
    },
    LigatureRule {
        pattern: "+=",
        glyph_id: 0xF014,
        advance_factor: 1.2,
        kind: LigatureKind::Contextual,
    },
    LigatureRule {
        pattern: "-=",
        glyph_id: 0xF015,
        advance_factor: 1.2,
        kind: LigatureKind::Contextual,
    },
    LigatureRule {
        pattern: "*=",
        glyph_id: 0xF016,
        advance_factor: 1.2,
        kind: LigatureKind::Contextual,
    },
    LigatureRule {
        pattern: "/=",
        glyph_id: 0xF017,
        advance_factor: 1.2,
        kind: LigatureKind::Contextual,
    },
    // Indic complex script conjuncts (consonant + virama + consonant)
    LigatureRule {
        pattern: "\u{0915}\u{094D}\u{0937}", // ksha (क्ष)
        glyph_id: 0xF050,
        advance_factor: 1.1,
        kind: LigatureKind::Indic,
    },
    LigatureRule {
        pattern: "\u{0924}\u{094D}\u{0930}", // tra (त्र)
        glyph_id: 0xF051,
        advance_factor: 1.0,
        kind: LigatureKind::Indic,
    },
    LigatureRule {
        pattern: "\u{091C}\u{094D}\u{091E}", // gya (ज्ञ)
        glyph_id: 0xF052,
        advance_factor: 1.1,
        kind: LigatureKind::Indic,
    },
    LigatureRule {
        pattern: "\u{0936}\u{094D}\u{0930}", // shra (श्र)
        glyph_id: 0xF053,
        advance_factor: 1.1,
        kind: LigatureKind::Indic,
    },
    LigatureRule {
        pattern: "\u{0924}\u{094D}\u{0924}", // tta (त्त)
        glyph_id: 0xF054,
        advance_factor: 1.0,
        kind: LigatureKind::Indic,
    },
    LigatureRule {
        pattern: "\u{0926}\u{094D}\u{0935}", // dva (द्व)
        glyph_id: 0xF055,
        advance_factor: 1.0,
        kind: LigatureKind::Indic,
    },
    LigatureRule {
        pattern: "\u{0926}\u{094D}\u{0927}", // ddha (द्ध)
        glyph_id: 0xF056,
        advance_factor: 1.0,
        kind: LigatureKind::Indic,
    },
    LigatureRule {
        pattern: "\u{0938}\u{094D}\u{0924}\u{094D}\u{0930}", // stra (स्त्र)
        glyph_id: 0xF057,
        advance_factor: 1.2,
        kind: LigatureKind::Indic,
    },
    LigatureRule {
        pattern: "\u{0915}\u{094D}\u{0930}", // kra (क्र)
        glyph_id: 0xF058,
        advance_factor: 1.0,
        kind: LigatureKind::Indic,
    },
    LigatureRule {
        pattern: "\u{092A}\u{094D}\u{0930}", // pra (प्र)
        glyph_id: 0xF059,
        advance_factor: 1.0,
        kind: LigatureKind::Indic,
    },
    LigatureRule {
        pattern: "\u{0926}\u{094D}\u{092F}", // dya (द्य)
        glyph_id: 0xF05A,
        advance_factor: 1.0,
        kind: LigatureKind::Indic,
    },
    LigatureRule {
        pattern: "\u{0937}\u{094D}\u{091F}", // shta (ष्ट)
        glyph_id: 0xF05B,
        advance_factor: 1.0,
        kind: LigatureKind::Indic,
    },
    LigatureRule {
        pattern: "\u{0937}\u{094D}\u{0920}", // shtha (ष्ठ)
        glyph_id: 0xF05C,
        advance_factor: 1.0,
        kind: LigatureKind::Indic,
    },
];

/// Known optical pair kerning adjustment rules (fractions of em).
struct KerningPair {
    first: char,
    second: char,
    delta_em: f32,
}

static KNOWN_KERNING_PAIRS: &[KerningPair] = &[
    KerningPair {
        first: 'A',
        second: 'V',
        delta_em: -0.085,
    },
    KerningPair {
        first: 'V',
        second: 'A',
        delta_em: -0.085,
    },
    KerningPair {
        first: 'A',
        second: 'W',
        delta_em: -0.070,
    },
    KerningPair {
        first: 'W',
        second: 'A',
        delta_em: -0.070,
    },
    KerningPair {
        first: 'A',
        second: 'T',
        delta_em: -0.075,
    },
    KerningPair {
        first: 'T',
        second: 'A',
        delta_em: -0.075,
    },
    KerningPair {
        first: 'T',
        second: 'o',
        delta_em: -0.070,
    },
    KerningPair {
        first: 'T',
        second: 'e',
        delta_em: -0.065,
    },
    KerningPair {
        first: 'W',
        second: 'e',
        delta_em: -0.045,
    },
    KerningPair {
        first: 'P',
        second: 'a',
        delta_em: -0.055,
    },
    KerningPair {
        first: 'P',
        second: 'o',
        delta_em: -0.050,
    },
    KerningPair {
        first: 'Y',
        second: 'o',
        delta_em: -0.080,
    },
    KerningPair {
        first: 'Y',
        second: 'a',
        delta_em: -0.075,
    },
    KerningPair {
        first: 'F',
        second: 'a',
        delta_em: -0.060,
    },
    KerningPair {
        first: 'L',
        second: 'T',
        delta_em: -0.065,
    },
    KerningPair {
        first: 'L',
        second: 'V',
        delta_em: -0.070,
    },
    KerningPair {
        first: 'r',
        second: 'c',
        delta_em: -0.025,
    },
    KerningPair {
        first: 'f',
        second: 'i',
        delta_em: -0.020,
    },
];

/// Pure-Rust OpenType text shaping engine.
#[derive(Debug, Clone, Copy, Default)]
pub struct TextShaper;

impl TextShaper {
    pub fn new() -> Self {
        Self
    }

    /// Shapes text using authentic OpenType glyph advances, kerning, and configured feature tags from an OpenTypeFont.
    pub fn shape_with_font_and_features(
        &self,
        text: &str,
        font: &super::opentype::OpenTypeFont,
        font_size: f32,
        features: OpenTypeFeatures,
    ) -> Vec<ShapedGlyph> {
        if text.is_empty() {
            return Vec::new();
        }

        let transformed_text: std::borrow::Cow<'_, str> = if features.small_caps {
            std::borrow::Cow::Owned(apply_small_caps(text))
        } else {
            std::borrow::Cow::Borrowed(text)
        };
        let input_text = transformed_text.as_ref();

        let bytes = input_text.as_bytes();
        let mut glyphs = Vec::with_capacity(input_text.len());
        let mut byte_idx = 0;

        while byte_idx < bytes.len() {
            let slice = &input_text[byte_idx..];

            // 1. Check for multi-character ligatures based on active OpenType features
            let mut matched_ligature = None;
            for lig in KNOWN_LIGATURES {
                let is_active = match lig.kind {
                    LigatureKind::Standard => features.ligatures,
                    LigatureKind::Contextual => features.contextual,
                    LigatureKind::Discretionary => features.discretionary,
                    LigatureKind::Indic => true,
                };
                if is_active && slice.starts_with(lig.pattern) {
                    matched_ligature = Some(lig);
                    break;
                }
            }

            if let Some(lig) = matched_ligature {
                let advance = if let Some(first_ch) = char::from_u32(lig.glyph_id) {
                    let gid = font.glyph_id(first_ch);
                    if gid > 0 {
                        font.glyph_advance(gid, font_size)
                    } else {
                        font_size * lig.advance_factor
                    }
                } else {
                    font_size * lig.advance_factor
                };

                glyphs.push(ShapedGlyph {
                    glyph_id: lig.glyph_id,
                    cluster: byte_idx as u32,
                    source_bytes: lig.pattern.len() as u16,
                    x_advance: LayoutUnit::from_f32(advance),
                    y_advance: LayoutUnit::ZERO,
                    x_offset: LayoutUnit::ZERO,
                    y_offset: LayoutUnit::ZERO,
                    is_ligature: true,
                    is_space: false,
                    is_mark: false,
                });
                byte_idx += lig.pattern.len();
                continue;
            }

            let ch = match slice.chars().next() {
                Some(c) => c,
                None => break,
            };
            let char_len = ch.len_utf8();

            if is_combining_mark(ch) {
                let y_offset = compute_mark_offset(ch, font_size);
                glyphs.push(ShapedGlyph {
                    glyph_id: ch as u32,
                    cluster: byte_idx as u32,
                    source_bytes: char_len as u16,
                    x_advance: LayoutUnit::ZERO,
                    y_advance: LayoutUnit::ZERO,
                    x_offset: LayoutUnit::ZERO,
                    y_offset,
                    is_ligature: false,
                    is_space: false,
                    is_mark: true,
                });
                byte_idx += char_len;
                continue;
            }

            let gid = font.glyph_id(ch);
            let advance = font.glyph_advance(gid, font_size);
            let is_space = ch == ' ' || ch == '\t' || ch == '\u{00A0}';

            glyphs.push(ShapedGlyph {
                glyph_id: gid as u32,
                cluster: byte_idx as u32,
                source_bytes: char_len as u16,
                x_advance: LayoutUnit::from_f32(advance),
                y_advance: LayoutUnit::ZERO,
                x_offset: LayoutUnit::ZERO,
                y_offset: LayoutUnit::ZERO,
                is_ligature: false,
                is_space,
                is_mark: false,
            });

            byte_idx += char_len;
        }

        if glyphs.len() > 1 {
            for i in 0..glyphs.len() - 1 {
                let g1 = glyphs[i].glyph_id as u16;
                let g2 = glyphs[i + 1].glyph_id as u16;
                let kern_delta = font.pair_kerning(g1, g2, font_size);
                if kern_delta != 0.0 {
                    glyphs[i].x_advance = glyphs[i].x_advance + LayoutUnit::from_f32(kern_delta);
                }
            }
        }

        glyphs
    }

    /// Shapes text using authentic OpenType glyph advances and kerning from an OpenTypeFont using default features.
    #[inline]
    pub fn shape_with_font(
        &self,
        text: &str,
        font: &super::opentype::OpenTypeFont,
        font_size: f32,
    ) -> Vec<ShapedGlyph> {
        self.shape_with_font_and_features(text, font, font_size, OpenTypeFeatures::default())
    }

    /// Shapes a text run into positioned glyphs, applying ligatures and pair kerning.
    pub fn shape(&self, text: &str, metrics: &FontMetrics, is_monospace: bool) -> Vec<ShapedGlyph> {
        if text.is_empty() {
            return Vec::new();
        }

        // Complex script pass: Contextual Arabic cursive shaping & multi-glyph ligatures
        if !is_monospace
            && text
                .chars()
                .any(|c| ('\u{0600}'..='\u{06FF}').contains(&c) || ('\u{0750}'..='\u{077F}').contains(&c))
        {
            return self.shape_arabic_run(text, metrics);
        }

        // Complex script pass: Indic Universal Shaping Engine (USE) conjuncts & pre-base vowel reordering
        if !is_monospace && text.chars().any(is_indic_char) {
            return self.shape_indic_run(text, metrics);
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
                    is_mark: false,
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

            // 3. Handle nonspacing combining marks (diacritics, vowels, tone marks)
            if is_combining_mark(ch) {
                let y_offset = compute_mark_offset(ch, metrics.font_size);
                glyphs.push(ShapedGlyph {
                    glyph_id: ch as u32,
                    cluster: byte_idx as u32,
                    source_bytes: char_len as u16,
                    x_advance: LayoutUnit::ZERO,
                    y_advance: LayoutUnit::ZERO,
                    x_offset: LayoutUnit::ZERO,
                    y_offset,
                    is_ligature: false,
                    is_space: false,
                    is_mark: true,
                });
                byte_idx += char_len;
                continue;
            }

            // 4. Determine base glyph advance
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
                is_mark: false,
            });

            byte_idx += char_len;
        }

        // 5. Apply GPOS optical pair kerning if proportional
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

    /// Shapes an Arabic script text run with contextual cursive joining and Lam-Alef ligatures.
    fn shape_arabic_run(&self, text: &str, metrics: &FontMetrics) -> Vec<ShapedGlyph> {
        let chars: Vec<(usize, char)> = text.char_indices().collect();
        let n = chars.len();
        let mut glyphs = Vec::with_capacity(n);
        let mut i = 0;

        while i < n {
            let (byte_idx, c) = chars[i];
            let c_len = c.len_utf8();

            // Multi-glyph Arabic ligatures (e.g. Allah, Akbar, Bismillah, Muhammad)
            let slice = &text[byte_idx..];
            let mut matched_multi_lig = None;
            for lig in ARABIC_MULTI_LIGATURES {
                if slice.starts_with(lig.pattern) {
                    matched_multi_lig = Some(lig);
                    break;
                }
            }
            if let Some(lig) = matched_multi_lig {
                let advance = LayoutUnit::from_f32(metrics.font_size * lig.advance_factor);
                glyphs.push(ShapedGlyph {
                    glyph_id: lig.glyph_id,
                    cluster: byte_idx as u32,
                    source_bytes: lig.pattern.len() as u16,
                    x_advance: advance,
                    y_advance: LayoutUnit::ZERO,
                    x_offset: LayoutUnit::ZERO,
                    y_offset: LayoutUnit::ZERO,
                    is_ligature: true,
                    is_space: false,
                    is_mark: false,
                });
                let pat_chars = lig.pattern.chars().count();
                i += pat_chars;
                continue;
            }

            // Nonspacing combining vowels (Harakat/Tashkeel) stack with zero advance
            if is_combining_mark(c) {
                let y_offset = compute_mark_offset(c, metrics.font_size);
                glyphs.push(ShapedGlyph {
                    glyph_id: c as u32,
                    cluster: byte_idx as u32,
                    source_bytes: c_len as u16,
                    x_advance: LayoutUnit::ZERO,
                    y_advance: LayoutUnit::ZERO,
                    x_offset: LayoutUnit::ZERO,
                    y_offset,
                    is_ligature: false,
                    is_space: false,
                    is_mark: true,
                });
                i += 1;
                continue;
            }

            let join_type = arabic_joining_type(c);
            if join_type == ArabicJoiningType::NonJoining {
                let base_advance = Self::character_advance(c, metrics.font_size);
                let is_space = c == ' ' || c == '\t' || c == '\u{00A0}';
                glyphs.push(ShapedGlyph {
                    glyph_id: c as u32,
                    cluster: byte_idx as u32,
                    source_bytes: c_len as u16,
                    x_advance: LayoutUnit::from_f32(base_advance),
                    y_advance: LayoutUnit::ZERO,
                    x_offset: LayoutUnit::ZERO,
                    y_offset: LayoutUnit::ZERO,
                    is_ligature: false,
                    is_space,
                    is_mark: false,
                });
                i += 1;
                continue;
            }

            // Inspect preceding non-transparent character
            let mut prev_joins = false;
            let mut p = i;
            while p > 0 {
                p -= 1;
                let prev_c = chars[p].1;
                let prev_jt = arabic_joining_type(prev_c);
                if prev_jt != ArabicJoiningType::Transparent {
                    if prev_jt == ArabicJoiningType::Dual || prev_jt == ArabicJoiningType::Left {
                        prev_joins = true;
                    }
                    break;
                }
            }

            // Check for Lam-Alef mandatory ligature
            if c == '\u{0644}' {
                let mut next_idx = None;
                let mut k = i + 1;
                while k < n {
                    let next_c = chars[k].1;
                    let next_jt = arabic_joining_type(next_c);
                    if next_jt != ArabicJoiningType::Transparent {
                        next_idx = Some((k, next_c));
                        break;
                    }
                    k += 1;
                }

                if let Some((k, next_c)) = next_idx {
                    if let Some(lig_glyph) = check_lam_alef_ligature(c, next_c, prev_joins) {
                        let total_bytes = (chars[k].0 + next_c.len_utf8()) - byte_idx;
                        let advance = LayoutUnit::from_f32(metrics.font_size * 0.75);
                        glyphs.push(ShapedGlyph {
                            glyph_id: lig_glyph,
                            cluster: byte_idx as u32,
                            source_bytes: total_bytes as u16,
                            x_advance: advance,
                            y_advance: LayoutUnit::ZERO,
                            x_offset: LayoutUnit::ZERO,
                            y_offset: LayoutUnit::ZERO,
                            is_ligature: true,
                            is_space: false,
                            is_mark: false,
                        });
                        i = k + 1;
                        continue;
                    }
                }
            }

            // Inspect succeeding non-transparent character
            let mut next_joins = false;
            let mut k = i + 1;
            while k < n {
                let next_c = chars[k].1;
                let next_jt = arabic_joining_type(next_c);
                if next_jt != ArabicJoiningType::Transparent {
                    if next_jt == ArabicJoiningType::Dual || next_jt == ArabicJoiningType::Right {
                        next_joins = true;
                    }
                    break;
                }
                k += 1;
            }

            let form = match (prev_joins, next_joins) {
                (true, true) if join_type == ArabicJoiningType::Dual => ArabicForm::Medial,
                (true, _) => ArabicForm::Final,
                (_, true) if join_type == ArabicJoiningType::Dual => ArabicForm::Initial,
                _ => ArabicForm::Isolated,
            };

            let glyph_id = map_arabic_form(c, form);
            let advance = LayoutUnit::from_f32(Self::character_advance(c, metrics.font_size));

            glyphs.push(ShapedGlyph {
                glyph_id,
                cluster: byte_idx as u32,
                source_bytes: c_len as u16,
                x_advance: advance,
                y_advance: LayoutUnit::ZERO,
                x_offset: LayoutUnit::ZERO,
                y_offset: LayoutUnit::ZERO,
                is_ligature: false,
                is_space: false,
                is_mark: false,
            });

            i += 1;
        }

        glyphs
    }

    /// Shapes an Indic script text run according to Universal Shaping Engine (USE) rules:
    /// akhand conjuncts, half-forms, Reph handling, and pre-base vowel reordering.
    fn shape_indic_run(&self, text: &str, metrics: &FontMetrics) -> Vec<ShapedGlyph> {
        let chars: Vec<(usize, char)> = text.char_indices().collect();
        let n = chars.len();
        let mut glyphs = Vec::with_capacity(n);
        let mut i = 0;

        while i < n {
            let (byte_idx, c) = chars[i];

            // 1. Check for known Indic conjunct ligatures starting at current position (e.g. क्ष, त्र, ज्ञ, श्र, त्त, द्व, etc.)
            let slice = &text[byte_idx..];
            let mut matched_conjunct = None;
            for lig in KNOWN_LIGATURES {
                if lig.kind == LigatureKind::Indic && slice.starts_with(lig.pattern) {
                    matched_conjunct = Some(lig);
                    break;
                }
            }

            if let Some(lig) = matched_conjunct {
                let lig_char_count = lig.pattern.chars().count();
                let next_idx = i + lig_char_count;

                // Check if followed by a pre-base vowel sign that must reorder before this conjunct
                if next_idx < n && is_indic_pre_base_vowel(chars[next_idx].1) {
                    let (v_byte_idx, v_c) = chars[next_idx];
                    let v_len = v_c.len_utf8();
                    // Reorder pre-base vowel: emit pre-base vowel first
                    glyphs.push(ShapedGlyph {
                        glyph_id: v_c as u32,
                        cluster: v_byte_idx as u32,
                        source_bytes: v_len as u16,
                        x_advance: LayoutUnit::from_f32(metrics.font_size * 0.25),
                        y_advance: LayoutUnit::ZERO,
                        x_offset: LayoutUnit::ZERO,
                        y_offset: LayoutUnit::ZERO,
                        is_ligature: false,
                        is_space: false,
                        is_mark: true,
                    });

                    // Then emit the conjunct
                    glyphs.push(ShapedGlyph {
                        glyph_id: lig.glyph_id,
                        cluster: byte_idx as u32,
                        source_bytes: lig.pattern.len() as u16,
                        x_advance: LayoutUnit::from_f32(metrics.font_size * lig.advance_factor),
                        y_advance: LayoutUnit::ZERO,
                        x_offset: LayoutUnit::ZERO,
                        y_offset: LayoutUnit::ZERO,
                        is_ligature: true,
                        is_space: false,
                        is_mark: false,
                    });

                    i = next_idx + 1;
                    continue;
                } else {
                    glyphs.push(ShapedGlyph {
                        glyph_id: lig.glyph_id,
                        cluster: byte_idx as u32,
                        source_bytes: lig.pattern.len() as u16,
                        x_advance: LayoutUnit::from_f32(metrics.font_size * lig.advance_factor),
                        y_advance: LayoutUnit::ZERO,
                        x_offset: LayoutUnit::ZERO,
                        y_offset: LayoutUnit::ZERO,
                        is_ligature: true,
                        is_space: false,
                        is_mark: false,
                    });
                    i = next_idx;
                    continue;
                }
            }

            // 2. Check for Reph: Ra + Virama followed by a base consonant
            // In Devanagari: 'र' (U+0930) + Virama (U+094D) + Consonant
            if (c == '\u{0930}' || c == '\u{09B0}' || c == '\u{0A30}' || c == '\u{0AB0}')
                && i + 1 < n
                && is_indic_virama(chars[i + 1].1)
                && i + 2 < n
                && is_indic_consonant(chars[i + 2].1)
            {
                let base_consonant_idx = i + 2;
                let (base_byte_idx, base_c) = chars[base_consonant_idx];
                let base_len = base_c.len_utf8();
                let next_after_base = base_consonant_idx + 1;

                // Check if base consonant is followed by a pre-base vowel sign
                if next_after_base < n && is_indic_pre_base_vowel(chars[next_after_base].1) {
                    let (v_byte, v_c) = chars[next_after_base];
                    // Emit pre-base vowel
                    glyphs.push(ShapedGlyph {
                        glyph_id: v_c as u32,
                        cluster: v_byte as u32,
                        source_bytes: v_c.len_utf8() as u16,
                        x_advance: LayoutUnit::from_f32(metrics.font_size * 0.25),
                        y_advance: LayoutUnit::ZERO,
                        x_offset: LayoutUnit::ZERO,
                        y_offset: LayoutUnit::ZERO,
                        is_ligature: false,
                        is_space: false,
                        is_mark: true,
                    });
                }

                // Emit base consonant
                let base_advance = Self::character_advance(base_c, metrics.font_size);
                glyphs.push(ShapedGlyph {
                    glyph_id: base_c as u32,
                    cluster: base_byte_idx as u32,
                    source_bytes: base_len as u16,
                    x_advance: LayoutUnit::from_f32(base_advance),
                    y_advance: LayoutUnit::ZERO,
                    x_offset: LayoutUnit::ZERO,
                    y_offset: LayoutUnit::ZERO,
                    is_ligature: false,
                    is_space: false,
                    is_mark: false,
                });

                // Emit Reph mark above base consonant
                glyphs.push(ShapedGlyph {
                    glyph_id: 0x094D, // Virama / Reph mark
                    cluster: byte_idx as u32,
                    source_bytes: (chars[i + 1].0 + chars[i + 1].1.len_utf8() - byte_idx) as u16,
                    x_advance: LayoutUnit::ZERO,
                    y_advance: LayoutUnit::ZERO,
                    x_offset: LayoutUnit::ZERO,
                    y_offset: LayoutUnit::from_f32(metrics.font_size * 0.40),
                    is_ligature: true,
                    is_space: false,
                    is_mark: true,
                });

                if next_after_base < n && is_indic_pre_base_vowel(chars[next_after_base].1) {
                    i = next_after_base + 1;
                } else {
                    i = base_consonant_idx + 1;
                }
                continue;
            }

            // 3. Check for standard Consonant + Pre-Base Vowel Sign Reordering
            if is_indic_consonant(c) {
                // Look ahead for pre-base vowel sign
                let mut pre_base_idx = None;
                if i + 1 < n && is_indic_pre_base_vowel(chars[i + 1].1) {
                    pre_base_idx = Some(i + 1);
                } else if i + 2 < n && is_indic_virama(chars[i + 1].1) && is_indic_consonant(chars[i + 2].1) {
                    // Consonant + Virama + Consonant (half form + base)
                    if i + 3 < n && is_indic_pre_base_vowel(chars[i + 3].1) {
                        pre_base_idx = Some(i + 3);
                    }
                }

                if let Some(pb_idx) = pre_base_idx {
                    let (pb_byte, pb_c) = chars[pb_idx];
                    // USE rule: Pre-base vowel sign is reordered to precede the consonant cluster visually
                    glyphs.push(ShapedGlyph {
                        glyph_id: pb_c as u32,
                        cluster: pb_byte as u32,
                        source_bytes: pb_c.len_utf8() as u16,
                        x_advance: LayoutUnit::from_f32(metrics.font_size * 0.25),
                        y_advance: LayoutUnit::ZERO,
                        x_offset: LayoutUnit::ZERO,
                        y_offset: LayoutUnit::ZERO,
                        is_ligature: false,
                        is_space: false,
                        is_mark: true,
                    });

                    // Emit consonant cluster up to pb_idx
                    for &(k_byte, k_c) in &chars[i..pb_idx] {
                        let is_v = is_indic_virama(k_c);
                        let adv = if is_v {
                            LayoutUnit::ZERO
                        } else {
                            LayoutUnit::from_f32(Self::character_advance(k_c, metrics.font_size))
                        };
                        glyphs.push(ShapedGlyph {
                            glyph_id: k_c as u32,
                            cluster: k_byte as u32,
                            source_bytes: k_c.len_utf8() as u16,
                            x_advance: adv,
                            y_advance: LayoutUnit::ZERO,
                            x_offset: LayoutUnit::ZERO,
                            y_offset: LayoutUnit::ZERO,
                            is_ligature: false,
                            is_space: false,
                            is_mark: is_v,
                        });
                    }

                    i = pb_idx + 1;
                    continue;
                }
            }

            // 4. Combining marks and generic characters
            let c_len = c.len_utf8();
            if is_combining_mark(c) {
                let y_offset = compute_mark_offset(c, metrics.font_size);
                glyphs.push(ShapedGlyph {
                    glyph_id: c as u32,
                    cluster: byte_idx as u32,
                    source_bytes: c_len as u16,
                    x_advance: LayoutUnit::ZERO,
                    y_advance: LayoutUnit::ZERO,
                    x_offset: LayoutUnit::ZERO,
                    y_offset,
                    is_ligature: false,
                    is_space: false,
                    is_mark: true,
                });
                i += 1;
                continue;
            }

            let base_advance = Self::character_advance(c, metrics.font_size);
            let is_space = c == ' ' || c == '\t' || c == '\u{00A0}';
            glyphs.push(ShapedGlyph {
                glyph_id: c as u32,
                cluster: byte_idx as u32,
                source_bytes: c_len as u16,
                x_advance: LayoutUnit::from_f32(base_advance),
                y_advance: LayoutUnit::ZERO,
                x_offset: LayoutUnit::ZERO,
                y_offset: LayoutUnit::ZERO,
                is_ligature: false,
                is_space,
                is_mark: false,
            });
            i += 1;
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
        if ('\u{4E00}'..='\u{9FFF}').contains(&ch)
            || ('\u{3040}'..='\u{30FF}').contains(&ch)
            || ('\u{AC00}'..='\u{D7AF}').contains(&ch)
        {
            return em * 1.0;
        }

        // Arabic characters (~0.45 - 0.70 em)
        if ('\u{0600}'..='\u{06FF}').contains(&ch) || ('\u{0750}'..='\u{077F}').contains(&ch) {
            return em * 0.52;
        }

        // Hebrew characters (~0.50 - 0.65 em)
        if ('\u{0590}'..='\u{05FF}').contains(&ch) {
            return em * 0.54;
        }

        match ch {
            // Very narrow characters
            'i' | 'l' | 'j' => em * 0.25,
            '\'' | '`' | '\u{200B}' => em * 0.22,
            '.' | ',' | ';' | ':' | '!' | '|' => em * 0.26,
            ' ' | '\u{00A0}' => em * 0.26,

            // Moderately narrow characters
            'f' | 't' => em * 0.35,
            'r' => em * 0.41,
            'c' | 's' | 'z' => em * 0.48,
            'v' | 'y' => em * 0.50,
            'k' | 'x' => em * 0.52,

            // Average proportional lowercase
            'a' | 'e' => em * 0.56,
            'g' => em * 0.55,
            'o' => em * 0.60,
            'b' | 'd' | 'h' | 'n' | 'p' | 'q' | 'u' => em * 0.61,

            // Wide lowercase
            'w' => em * 0.78,
            'm' => em * 0.93,

            // Uppercase characters
            'I' | 'J' => em * 0.28,
            'E' | 'F' | 'K' | 'L' | 'Y' | 'Z' => em * 0.54,
            'S' | 'T' | 'X' => em * 0.56,
            'P' | 'V' => em * 0.60,
            'R' => em * 0.62,
            'A' | 'B' | 'C' => em * 0.64,
            'D' | 'G' | 'U' => em * 0.73,
            'H' | 'N' => em * 0.75,
            'O' | 'Q' => em * 0.78,
            'M' | 'W' => em * 0.92,

            // Digits (tabular modern font metrics)
            '0'..='9' => em * 0.57,

            // Punctuation and symbols
            '(' | ')' | '[' | ']' => em * 0.30,
            '{' | '}' => em * 0.35,
            '-' | '*' | '?' => em * 0.33,
            '"' => em * 0.40,
            '@' | '%' => em * 0.85,
            '—' | '–' => em * 0.70,

            // Default fallback proportional advance
            _ => em * 0.55,
        }
    }
}

/// Identifies nonspacing combining marks (diacritics, vowels, tone marks) that stack vertically.
pub fn is_combining_mark(c: char) -> bool {
    matches!(
        c,
        '\u{0300}'..='\u{036F}'
            | '\u{0483}'..='\u{0489}'
            | '\u{0591}'..='\u{05BD}'
            | '\u{05BF}'
            | '\u{05C1}'..='\u{05C2}'
            | '\u{05C4}'..='\u{05C5}'
            | '\u{05C7}'
            | '\u{0610}'..='\u{061A}'
            | '\u{064B}'..='\u{065F}'
            | '\u{0670}'
            | '\u{06D6}'..='\u{06DC}'
            | '\u{06DF}'..='\u{06E4}'
            | '\u{06E7}'..='\u{06E8}'
            | '\u{06EA}'..='\u{06ED}'
            | '\u{0901}'..='\u{0903}'
            | '\u{093A}'
            | '\u{093C}'
            | '\u{0941}'..='\u{0948}'
            | '\u{094D}'
            | '\u{0951}'..='\u{0954}'
            | '\u{0962}'..='\u{0963}'
            | '\u{0E31}'
            | '\u{0E34}'..='\u{0E3A}'
            | '\u{0E47}'..='\u{0E4E}'
    )
}

/// Computes the vertical subpixel offset for combining marks relative to the baseline.
pub fn compute_mark_offset(c: char, font_size: f32) -> LayoutUnit {
    let is_above = matches!(
        c,
        '\u{0300}'..='\u{0314}'
            | '\u{0340}'..='\u{0344}'
            | '\u{064E}'
            | '\u{064F}'
            | '\u{0651}'
            | '\u{0652}'
            | '\u{0653}'..='\u{0658}'
            | '\u{0670}'
            | '\u{0E48}'..='\u{0E4B}'
            | '\u{0E31}'
            | '\u{0E34}'..='\u{0E37}'
            | '\u{0E47}'
            | '\u{0E4C}'..='\u{0E4E}'
            | '\u{0901}'..='\u{0902}'
    );

    let is_below = matches!(
        c,
        '\u{0315}'..='\u{0333}'
            | '\u{05B0}'..='\u{05BC}'
            | '\u{0650}'
            | '\u{064D}'
            | '\u{0E38}'..='\u{0E39}'
            | '\u{0941}'..='\u{0944}'
    );

    if is_above {
        LayoutUnit::from_f32(font_size * 0.35)
    } else if is_below {
        LayoutUnit::from_f32(-font_size * 0.25)
    } else {
        LayoutUnit::ZERO
    }
}

struct ArabicMultiLigature {
    pattern: &'static str,
    glyph_id: u32,
    advance_factor: f32,
}

static ARABIC_MULTI_LIGATURES: &[ArabicMultiLigature] = &[
    // Bismillah ar-Rahman ar-Rahim: ﷽
    ArabicMultiLigature {
        pattern: "\u{0628}\u{0633}\u{0645}\u{0020}\u{0627}\u{0644}\u{0644}\u{0647}\u{0020}\u{0627}\u{0644}\u{0631}\u{062D}\u{0645}\u{0646}\u{0020}\u{0627}\u{0644}\u{0631}\u{062D}\u{064A}\u{0645}",
        glyph_id: 0xFDFD,
        advance_factor: 2.5,
    },
    // Sallallahu Alayhi Wasallam: ﷺ
    ArabicMultiLigature {
        pattern: "\u{0635}\u{0644}\u{0649}\u{0020}\u{0627}\u{0644}\u{0644}\u{0647}\u{0020}\u{0639}\u{0644}\u{064A}\u{0647}\u{0020}\u{0648}\u{0633}\u{0644}\u{0645}",
        glyph_id: 0xFDFA,
        advance_factor: 1.8,
    },
    // Jalla Jalalouhou: ﷻ
    ArabicMultiLigature {
        pattern: "\u{062C}\u{0644}\u{0020}\u{062C}\u{0644}\u{0627}\u{0644}\u{0647}",
        glyph_id: 0xFDFB,
        advance_factor: 1.5,
    },
    // Allah with Tashkeel: اَللّٰه or اللّٰه or الله
    ArabicMultiLigature {
        pattern: "\u{0627}\u{0644}\u{0644}\u{0651}\u{0670}\u{0647}",
        glyph_id: 0xFDF2,
        advance_factor: 1.2,
    },
    ArabicMultiLigature {
        pattern: "\u{0627}\u{0644}\u{0644}\u{0647}", // Allah (الله)
        glyph_id: 0xFDF2,
        advance_factor: 1.2,
    },
    ArabicMultiLigature {
        pattern: "\u{0644}\u{0644}\u{0647}", // Lillah (لله)
        glyph_id: 0xFDF2,
        advance_factor: 1.0,
    },
    // Akbar: أكبر
    ArabicMultiLigature {
        pattern: "\u{0623}\u{0643}\u{0628}\u{0631}",
        glyph_id: 0xFDF3,
        advance_factor: 1.2,
    },
    ArabicMultiLigature {
        pattern: "\u{0627}\u{0643}\u{0628}\u{0631}",
        glyph_id: 0xFDF3,
        advance_factor: 1.2,
    },
    // Muhammad: محمد
    ArabicMultiLigature {
        pattern: "\u{0645}\u{062D}\u{0645}\u{062F}",
        glyph_id: 0xFDF4,
        advance_factor: 1.2,
    },
    // Rasul: رسول
    ArabicMultiLigature {
        pattern: "\u{0631}\u{0633}\u{0648}\u{0644}",
        glyph_id: 0xFDF6,
        advance_factor: 1.1,
    },
    // Alayhi: عليه
    ArabicMultiLigature {
        pattern: "\u{0639}\u{0644}\u{064A}\u{0647}",
        glyph_id: 0xFDF7,
        advance_factor: 1.0,
    },
    // Wasallam: وسلم
    ArabicMultiLigature {
        pattern: "\u{0648}\u{0633}\u{0644}\u{0645}",
        glyph_id: 0xFDF8,
        advance_factor: 1.0,
    },
    // Salla: صلى
    ArabicMultiLigature {
        pattern: "\u{0635}\u{0644}\u{0649}",
        glyph_id: 0xFDF9,
        advance_factor: 0.9,
    },
];

/// Returns true if a character belongs to an Indic script block.
pub fn is_indic_char(c: char) -> bool {
    matches!(
        c,
        '\u{0900}'..='\u{097F}' // Devanagari
        | '\u{0980}'..='\u{09FF}' // Bengali
        | '\u{0A00}'..='\u{0A7F}' // Gurmukhi
        | '\u{0A80}'..='\u{0AFF}' // Gujarati
        | '\u{0B00}'..='\u{0B7F}' // Oriya
        | '\u{0B80}'..='\u{0BFF}' // Tamil
        | '\u{0C00}'..='\u{0C7F}' // Telugu
        | '\u{0C80}'..='\u{0CFF}' // Kannada
        | '\u{0D00}'..='\u{0D7F}' // Malayalam
    )
}

/// Returns true if a character is an Indic Virama (Halant) explicit vowel suppressor.
pub fn is_indic_virama(c: char) -> bool {
    matches!(
        c,
        '\u{094D}' // Devanagari Virama
        | '\u{09CD}' // Bengali Virama
        | '\u{0A4D}' // Gurmukhi Virama
        | '\u{0ACD}' // Gujarati Virama
        | '\u{0B4D}' // Oriya Virama
        | '\u{0BCD}' // Tamil Virama
        | '\u{0C4D}' // Telugu Virama
        | '\u{0CCD}' // Kannada Virama
        | '\u{0D4D}' // Malayalam Virama
    )
}

/// Returns true if an Indic vowel sign is pre-base and must reorder before the consonant cluster under USE rules.
pub fn is_indic_pre_base_vowel(c: char) -> bool {
    matches!(
        c,
        '\u{093F}' // Devanagari Vowel Sign I (ि)
        | '\u{09BF}' // Bengali Vowel Sign I (ি)
        | '\u{09C7}' // Bengali Vowel Sign E (ে)
        | '\u{09C8}' // Bengali Vowel Sign AI (ৈ)
        | '\u{0A3F}' // Gurmukhi Vowel Sign I (ਿ)
        | '\u{0ABF}' // Gujarati Vowel Sign I (િ)
        | '\u{0B3F}' // Oriya Vowel Sign I (ି)
        | '\u{0BC6}' // Tamil Vowel Sign E (ெ)
        | '\u{0BC7}' // Tamil Vowel Sign EE (ே)
        | '\u{0BC8}' // Tamil Vowel Sign AI (ை)
        | '\u{0D3F}' // Malayalam Vowel Sign I (ി)
        | '\u{0D46}' // Malayalam Vowel Sign E (െ)
        | '\u{0D47}' // Malayalam Vowel Sign EE (ே)
        | '\u{0D48}' // Malayalam Vowel Sign AI (ൈ)
    )
}

/// Returns true if a character is an Indic base consonant.
pub fn is_indic_consonant(c: char) -> bool {
    matches!(
        c,
        '\u{0915}'..='\u{0939}' | '\u{0958}'..='\u{095F}' // Devanagari
        | '\u{0995}'..='\u{09B9}' | '\u{09DC}'..='\u{09DF}' // Bengali
        | '\u{0A15}'..='\u{0A39}' // Gurmukhi
        | '\u{0A95}'..='\u{0AB9}' // Gujarati
        | '\u{0B15}'..='\u{0B39}' // Oriya
        | '\u{0B95}'..='\u{0BB9}' // Tamil
        | '\u{0C15}'..='\u{0C39}' // Telugu
        | '\u{0C95}'..='\u{0CB9}' // Kannada
        | '\u{0D15}'..='\u{0D39}' // Malayalam
    )
}

/// Unicode Arabic joining types according to Unicode Standard Section 9.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArabicJoiningType {
    Dual,
    Right,
    Left,
    NonJoining,
    Transparent,
}

/// Contextual glyph form variants for Arabic script.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArabicForm {
    Isolated,
    Initial,
    Medial,
    Final,
}

fn arabic_joining_type(c: char) -> ArabicJoiningType {
    if is_combining_mark(c) {
        return ArabicJoiningType::Transparent;
    }
    match c {
        '\u{0621}' => ArabicJoiningType::NonJoining,
        '\u{0622}' | '\u{0623}' | '\u{0624}' | '\u{0625}' | '\u{0627}'
        | '\u{0629}'
        | '\u{062F}' | '\u{0630}'
        | '\u{0631}' | '\u{0632}'
        | '\u{0648}'
        | '\u{0649}'
        | '\u{0671}'..='\u{0673}'
        | '\u{0675}'..='\u{0677}'
        | '\u{0688}'..='\u{0699}'
        | '\u{06C0}'..='\u{06CB}'
        | '\u{06CD}'..='\u{06CF}' => ArabicJoiningType::Right,
        '\u{0640}' => ArabicJoiningType::Dual,
        '\u{0626}'
        | '\u{0628}' | '\u{062A}' | '\u{062B}'
        | '\u{062C}' | '\u{062D}' | '\u{062E}'
        | '\u{0633}' | '\u{0634}'
        | '\u{0635}' | '\u{0636}'
        | '\u{0637}' | '\u{0638}'
        | '\u{0639}' | '\u{063A}'
        | '\u{0641}' | '\u{0642}'
        | '\u{0643}' | '\u{0644}'
        | '\u{0645}' | '\u{0646}'
        | '\u{0647}'
        | '\u{064A}'
        | '\u{0678}'..='\u{0687}'
        | '\u{069A}'..='\u{06BF}'
        | '\u{06CC}'
        | '\u{06D0}'..='\u{06D3}' => ArabicJoiningType::Dual,
        _ => ArabicJoiningType::NonJoining,
    }
}

fn map_arabic_form(c: char, form: ArabicForm) -> u32 {
    match c {
        '\u{0627}' => match form {
            ArabicForm::Final => 0xFE8E,
            _ => 0xFE8D,
        },
        '\u{0622}' => match form {
            ArabicForm::Final => 0xFE82,
            _ => 0xFE81,
        },
        '\u{0623}' => match form {
            ArabicForm::Final => 0xFE84,
            _ => 0xFE83,
        },
        '\u{0624}' => match form {
            ArabicForm::Final => 0xFE86,
            _ => 0xFE85,
        },
        '\u{0625}' => match form {
            ArabicForm::Final => 0xFE88,
            _ => 0xFE87,
        },
        '\u{0626}' => match form {
            ArabicForm::Isolated => 0xFE89,
            ArabicForm::Final => 0xFE8A,
            ArabicForm::Initial => 0xFE8B,
            ArabicForm::Medial => 0xFE8C,
        },
        '\u{0628}' => match form {
            ArabicForm::Isolated => 0xFE8F,
            ArabicForm::Final => 0xFE90,
            ArabicForm::Initial => 0xFE91,
            ArabicForm::Medial => 0xFE92,
        },
        '\u{0629}' => match form {
            ArabicForm::Final => 0xFE94,
            _ => 0xFE93,
        },
        '\u{062A}' => match form {
            ArabicForm::Isolated => 0xFE95,
            ArabicForm::Final => 0xFE96,
            ArabicForm::Initial => 0xFE97,
            ArabicForm::Medial => 0xFE98,
        },
        '\u{062B}' => match form {
            ArabicForm::Isolated => 0xFE99,
            ArabicForm::Final => 0xFE9A,
            ArabicForm::Initial => 0xFE9B,
            ArabicForm::Medial => 0xFE9C,
        },
        '\u{062C}' => match form {
            ArabicForm::Isolated => 0xFE9D,
            ArabicForm::Final => 0xFE9E,
            ArabicForm::Initial => 0xFE9F,
            ArabicForm::Medial => 0xFEA0,
        },
        '\u{062D}' => match form {
            ArabicForm::Isolated => 0xFEA1,
            ArabicForm::Final => 0xFEA2,
            ArabicForm::Initial => 0xFEA3,
            ArabicForm::Medial => 0xFEA4,
        },
        '\u{062E}' => match form {
            ArabicForm::Isolated => 0xFEA5,
            ArabicForm::Final => 0xFEA6,
            ArabicForm::Initial => 0xFEA7,
            ArabicForm::Medial => 0xFEA8,
        },
        '\u{062F}' => match form {
            ArabicForm::Final => 0xFEAA,
            _ => 0xFEA9,
        },
        '\u{0630}' => match form {
            ArabicForm::Final => 0xFEAC,
            _ => 0xFEAB,
        },
        '\u{0631}' => match form {
            ArabicForm::Final => 0xFEAE,
            _ => 0xFEAD,
        },
        '\u{0632}' => match form {
            ArabicForm::Final => 0xFEB0,
            _ => 0xFEAF,
        },
        '\u{0633}' => match form {
            ArabicForm::Isolated => 0xFEB1,
            ArabicForm::Final => 0xFEB2,
            ArabicForm::Initial => 0xFEB3,
            ArabicForm::Medial => 0xFEB4,
        },
        '\u{0634}' => match form {
            ArabicForm::Isolated => 0xFEB5,
            ArabicForm::Final => 0xFEB6,
            ArabicForm::Initial => 0xFEB7,
            ArabicForm::Medial => 0xFEB8,
        },
        '\u{0635}' => match form {
            ArabicForm::Isolated => 0xFEB9,
            ArabicForm::Final => 0xFEBA,
            ArabicForm::Initial => 0xFEBB,
            ArabicForm::Medial => 0xFEBC,
        },
        '\u{0636}' => match form {
            ArabicForm::Isolated => 0xFEBD,
            ArabicForm::Final => 0xFEBE,
            ArabicForm::Initial => 0xFEBF,
            ArabicForm::Medial => 0xFEC0,
        },
        '\u{0637}' => match form {
            ArabicForm::Isolated => 0xFEC1,
            ArabicForm::Final => 0xFEC2,
            ArabicForm::Initial => 0xFEC3,
            ArabicForm::Medial => 0xFEC4,
        },
        '\u{0638}' => match form {
            ArabicForm::Isolated => 0xFEC5,
            ArabicForm::Final => 0xFEC6,
            ArabicForm::Initial => 0xFEC7,
            ArabicForm::Medial => 0xFEC8,
        },
        '\u{0639}' => match form {
            ArabicForm::Isolated => 0xFEC9,
            ArabicForm::Final => 0xFECA,
            ArabicForm::Initial => 0xFECB,
            ArabicForm::Medial => 0xFECC,
        },
        '\u{063A}' => match form {
            ArabicForm::Isolated => 0xFECD,
            ArabicForm::Final => 0xFECE,
            ArabicForm::Initial => 0xFECF,
            ArabicForm::Medial => 0xFED0,
        },
        '\u{0641}' => match form {
            ArabicForm::Isolated => 0xFED1,
            ArabicForm::Final => 0xFED2,
            ArabicForm::Initial => 0xFED3,
            ArabicForm::Medial => 0xFED4,
        },
        '\u{0642}' => match form {
            ArabicForm::Isolated => 0xFED5,
            ArabicForm::Final => 0xFED6,
            ArabicForm::Initial => 0xFED7,
            ArabicForm::Medial => 0xFED8,
        },
        '\u{0643}' => match form {
            ArabicForm::Isolated => 0xFED9,
            ArabicForm::Final => 0xFEDA,
            ArabicForm::Initial => 0xFEDB,
            ArabicForm::Medial => 0xFEDC,
        },
        '\u{0644}' => match form {
            ArabicForm::Isolated => 0xFEDD,
            ArabicForm::Final => 0xFEDE,
            ArabicForm::Initial => 0xFEDF,
            ArabicForm::Medial => 0xFEE0,
        },
        '\u{0645}' => match form {
            ArabicForm::Isolated => 0xFEE1,
            ArabicForm::Final => 0xFEE2,
            ArabicForm::Initial => 0xFEE3,
            ArabicForm::Medial => 0xFEE4,
        },
        '\u{0646}' => match form {
            ArabicForm::Isolated => 0xFEE5,
            ArabicForm::Final => 0xFEE6,
            ArabicForm::Initial => 0xFEE7,
            ArabicForm::Medial => 0xFEE8,
        },
        '\u{0647}' => match form {
            ArabicForm::Isolated => 0xFEE9,
            ArabicForm::Final => 0xFEEA,
            ArabicForm::Initial => 0xFEEB,
            ArabicForm::Medial => 0xFEEC,
        },
        '\u{0648}' => match form {
            ArabicForm::Final => 0xFEEE,
            _ => 0xFEED,
        },
        '\u{0649}' => match form {
            ArabicForm::Final => 0xFEF0,
            _ => 0xFEEF,
        },
        '\u{064A}' => match form {
            ArabicForm::Isolated => 0xFEF1,
            ArabicForm::Final => 0xFEF2,
            ArabicForm::Initial => 0xFEF3,
            ArabicForm::Medial => 0xFEF4,
        },
        _ => c as u32,
    }
}

fn check_lam_alef_ligature(c_lam: char, c_alef: char, joins_before: bool) -> Option<u32> {
    if c_lam != '\u{0644}' {
        return None;
    }
    match c_alef {
        '\u{0622}' => Some(if joins_before { 0xFEF6 } else { 0xFEF5 }),
        '\u{0623}' => Some(if joins_before { 0xFEF8 } else { 0xFEF7 }),
        '\u{0625}' => Some(if joins_before { 0xFEFA } else { 0xFEF9 }),
        '\u{0627}' => Some(if joins_before { 0xFEFC } else { 0xFEFB }),
        _ => None,
    }
}
