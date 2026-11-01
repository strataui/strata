//! Unicode Line Breaking Algorithm (UAX#14) and CSS white-space handling.
//!
//! Provides opportunistic break detection, whitespace collapsing, and line boundary segmentation.

use strata_primitives::geom::LayoutUnit;
use super::shaper::ShapedGlyph;

/// CSS white-space property mode controlling whitespace collapsing and wrapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WhiteSpaceMode {
    /// Collapses consecutive whitespace; wraps on line breaks and when line exceeds width.
    #[default]
    Normal,
    /// Collapses consecutive whitespace; suppresses all soft line wrapping.
    NoWrap,
    /// Preserves all whitespace and newlines; suppresses soft line wrapping.
    Pre,
    /// Preserves all whitespace and newlines; wraps lines when exceeding container width.
    PreWrap,
    /// Collapses consecutive whitespace; preserves explicit newlines; wraps on line bounds.
    PreLine,
}

impl WhiteSpaceMode {
    /// Whether consecutive whitespace characters are collapsed into a single space.
    #[inline(always)]
    pub fn collapses_spaces(self) -> bool {
        matches!(self, WhiteSpaceMode::Normal | WhiteSpaceMode::NoWrap | WhiteSpaceMode::PreLine)
    }

    /// Whether soft wrapping is allowed when line exceeds container width.
    #[inline(always)]
    pub fn allows_soft_wrap(self) -> bool {
        matches!(self, WhiteSpaceMode::Normal | WhiteSpaceMode::PreWrap | WhiteSpaceMode::PreLine)
    }

    /// Whether explicit newline characters (\n) force a line break.
    #[inline(always)]
    pub fn preserves_newlines(self) -> bool {
        matches!(self, WhiteSpaceMode::Pre | WhiteSpaceMode::PreWrap | WhiteSpaceMode::PreLine)
    }
}

/// A segmented line of shaped glyphs resulting from the line breaking pass.
#[derive(Debug, Clone, PartialEq)]
pub struct TextLine {
    /// The shaped glyphs placed on this line.
    pub glyphs: Vec<ShapedGlyph>,
    /// Total horizontal advance width of the line excluding trailing whitespace.
    pub content_width: LayoutUnit,
    /// Advance width of trailing whitespace.
    pub trailing_space_width: LayoutUnit,
    /// Whether this line ended with an explicit mandatory break (\n).
    pub is_hard_break: bool,
}

/// Unicode UAX#14 Line Breaker.
#[derive(Debug, Clone, Default)]
pub struct LineBreaker;

impl LineBreaker {
    pub fn new() -> Self {
        Self
    }

    /// Breaks a stream of shaped glyphs into visual lines fitting within `max_width`.
    pub fn break_lines(
        &self,
        glyphs: &[ShapedGlyph],
        max_width: Option<LayoutUnit>,
        mode: WhiteSpaceMode,
    ) -> Vec<TextLine> {
        if glyphs.is_empty() {
            return Vec::new();
        }

        let max_w = max_width.unwrap_or(LayoutUnit(i32::MAX));
        let allows_soft_wrap = mode.allows_soft_wrap() && max_width.is_some();
        let collapses_spaces = mode.collapses_spaces();

        let mut lines = Vec::new();
        let mut current_glyphs = Vec::new();
        let mut current_width = LayoutUnit::ZERO;

        let mut last_break_idx = None;

        let mut prev_was_space = false;
        let mut idx = 0;

        while idx < glyphs.len() {
            let glyph = glyphs[idx];

            // 1. Check for hard newline character (\n is glyph_id 10)
            if glyph.glyph_id == 10 || glyph.glyph_id == 13 {
                let (content_w, trailing_w) = Self::calculate_widths(&current_glyphs);
                lines.push(TextLine {
                    glyphs: std::mem::take(&mut current_glyphs),
                    content_width: content_w,
                    trailing_space_width: trailing_w,
                    is_hard_break: true,
                });
                current_width = LayoutUnit::ZERO;
                last_break_idx = None;
                prev_was_space = false;
                idx += 1;
                continue;
            }

            // 2. Whitespace collapsing
            if collapses_spaces && glyph.is_space {
                if prev_was_space || current_glyphs.is_empty() {
                    // Skip redundant whitespace
                    idx += 1;
                    continue;
                }
                prev_was_space = true;
            } else {
                prev_was_space = false;
            }

            // 3. Check for line overflow and break opportunity
            let advance = glyph.x_advance;
            let fits = current_width + advance <= max_w;

            if !fits && allows_soft_wrap && !current_glyphs.is_empty() {
                // If we have a recorded break opportunity on this line, break there
                if let Some(break_pos) = last_break_idx {
                    let remainder: Vec<ShapedGlyph> = current_glyphs.drain(break_pos..).collect();
                    let (content_w, trailing_w) = Self::calculate_widths(&current_glyphs);

                    lines.push(TextLine {
                        glyphs: std::mem::take(&mut current_glyphs),
                        content_width: content_w,
                        trailing_space_width: trailing_w,
                        is_hard_break: false,
                    });

                    // Resume with remainder + current glyph
                    current_glyphs = remainder;
                    current_width = current_glyphs.iter().map(|g| g.x_advance).fold(LayoutUnit::ZERO, |a, b| a + b);
                    last_break_idx = None;
                } else {
                    // Forced emergency break on overflow (overflow-wrap: anywhere)
                    let (content_w, trailing_w) = Self::calculate_widths(&current_glyphs);
                    lines.push(TextLine {
                        glyphs: std::mem::take(&mut current_glyphs),
                        content_width: content_w,
                        trailing_space_width: trailing_w,
                        is_hard_break: false,
                    });
                    current_width = LayoutUnit::ZERO;
                    last_break_idx = None;
                }
            }

            // Record soft break opportunities (after spaces or hyphens)
            if glyph.is_space || glyph.glyph_id == 45 /* '-' */ || glyph.glyph_id == 0x200B /* ZWSP */ {
                last_break_idx = Some(current_glyphs.len() + 1);
            }

            current_width = current_width + advance;
            current_glyphs.push(glyph);
            idx += 1;
        }

        // Flush remaining line
        if !current_glyphs.is_empty() {
            let (content_w, trailing_w) = Self::calculate_widths(&current_glyphs);
            lines.push(TextLine {
                glyphs: current_glyphs,
                content_width: content_w,
                trailing_space_width: trailing_w,
                is_hard_break: false,
            });
        }

        lines
    }

    /// Computes content width and trailing space width for a set of glyphs.
    fn calculate_widths(glyphs: &[ShapedGlyph]) -> (LayoutUnit, LayoutUnit) {
        if glyphs.is_empty() {
            return (LayoutUnit::ZERO, LayoutUnit::ZERO);
        }

        let mut trailing_w = LayoutUnit::ZERO;

        for g in glyphs.iter().rev() {
            if g.is_space {
                trailing_w = trailing_w + g.x_advance;
            } else {
                break;
            }
        }

        let total_w = glyphs.iter().map(|g| g.x_advance).fold(LayoutUnit::ZERO, |a, b| a + b);
        let content_w = total_w - trailing_w;

        (content_w, trailing_w)
    }
}
