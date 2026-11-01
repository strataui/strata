//! Inline Formatting Context (IFC), Line Boxes, and Subpixel Vertical Alignment.
//!
//! Organizes shaped glyph runs into line boxes, resolves vertical alignments,
//! and computes final subpixel layout bounds with full Blink LayoutNG parity.

use strata_primitives::geom::{LayoutUnit, Rect};
use super::bidi::BidiEngine;
use super::font_metrics::{compute_half_leading, CssStrut, FontHeight, FontMetrics};
use super::line_breaker::{LineBreaker, TextLine, WhiteSpaceMode};
use super::shaper::{ShapedGlyph, TextShaper};

/// CSS vertical-align property values for inline formatting context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VerticalAlignment {
    /// Align the baseline of the element with the baseline of its parent box.
    #[default]
    Baseline,
    /// Align the top of the element with the top of the entire line box.
    Top,
    /// Align the bottom of the element with the bottom of the entire line box.
    Bottom,
    /// Align the vertical midpoint of the element with baseline + half x-height.
    Middle,
    /// Subscript: Lower the baseline by ~20% of font size.
    Sub,
    /// Superscript: Raise the baseline by ~35% of font size.
    Super,
    /// Align the top of the element with the top of the parent element's font.
    TextTop,
    /// Align the bottom of the element with the bottom of the parent element's font.
    TextBottom,
}

/// A positioned inline line box representing a single rendered text line.
#[derive(Debug, Clone, PartialEq)]
pub struct LineBox {
    /// Bounding rectangle in container coordinate space (x, y, width, height).
    pub bounds: Rect<f32>,
    /// Distance from the top of the line box to the dominant baseline.
    pub baseline: f32,
    /// Ascent and descent bounds encompassing all inline items on this line.
    pub metrics: FontHeight,
    /// Positioned glyphs on this line.
    pub glyphs: Vec<ShapedGlyph>,
    /// Whether this line was terminated by a hard newline.
    pub is_hard_break: bool,
}

/// Options configuring inline text layout.
#[derive(Debug, Clone, Copy)]
pub struct InlineLayoutOptions {
    /// Font size in CSS pixels.
    pub font_size: f32,
    /// Explicit CSS line-height in pixels, or None for 'normal' (~1.2x).
    pub line_height: Option<f32>,
    /// Container max-width constraint for wrapping.
    pub max_width: Option<f32>,
    /// Whitespace handling mode.
    pub white_space: WhiteSpaceMode,
    /// Vertical alignment mode.
    pub vertical_align: VerticalAlignment,
    /// Whether font is monospace (disables ligatures and kerning).
    pub is_monospace: bool,
}

impl Default for InlineLayoutOptions {
    fn default() -> Self {
        Self {
            font_size: 16.0,
            line_height: None,
            max_width: None,
            white_space: WhiteSpaceMode::Normal,
            vertical_align: VerticalAlignment::Baseline,
            is_monospace: false,
        }
    }
}

/// Complete measurement and geometry result of an inline layout pass.
#[derive(Debug, Clone, PartialEq)]
pub struct InlineLayoutResult {
    /// The formatted line boxes.
    pub lines: Vec<LineBox>,
    /// Maximum bounding width across all line boxes.
    pub total_width: f32,
    /// Total vertical height of all stacked line boxes.
    pub total_height: f32,
    /// Line box count.
    pub line_count: usize,
}

/// High-performance Inline Formatting Context engine.
#[derive(Debug, Clone, Default)]
pub struct InlineFormattingContext {
    bidi: BidiEngine,
    shaper: TextShaper,
    line_breaker: LineBreaker,
}

impl InlineFormattingContext {
    pub fn new() -> Self {
        Self {
            bidi: BidiEngine::new(),
            shaper: TextShaper::new(),
            line_breaker: LineBreaker::new(),
        }
    }

    /// Formats input text into positioned line boxes, honoring metrics, BiDi, and line breaks.
    pub fn layout_text(&self, text: &str, options: &InlineLayoutOptions) -> InlineLayoutResult {
        if text.is_empty() {
            let metrics = if options.is_monospace {
                FontMetrics::default_mono(options.font_size)
            } else {
                FontMetrics::default_sans(options.font_size)
            };
            let strut = CssStrut::new(&metrics, options.line_height);
            let height = strut.line_height.to_f32();
            return InlineLayoutResult {
                lines: Vec::new(),
                total_width: 0.0,
                total_height: height,
                line_count: 0,
            };
        }

        // 1. Resolve font metrics and CSS strut
        let font_metrics = if options.is_monospace {
            FontMetrics::default_mono(options.font_size)
        } else {
            FontMetrics::default_sans(options.font_size)
        };
        let strut = CssStrut::new(&font_metrics, options.line_height);

        // 2. Unicode Bidirectional decomposition
        let bidi_runs = self.bidi.resolve_runs(text);

        // 3. Shape glyphs for each visual run
        let mut visual_glyphs = Vec::new();
        for run in &bidi_runs {
            let run_text = &text[run.start..run.end];
            let mut shaped = self.shaper.shape(run_text, &font_metrics, options.is_monospace);

            // If RTL, reverse glyph order inside the run
            if run.is_rtl {
                shaped.reverse();
            }
            visual_glyphs.extend(shaped);
        }

        // 4. Line breaking pass
        let max_w_unit = options.max_width.map(LayoutUnit::from_f32);
        let text_lines: Vec<TextLine> = self.line_breaker.break_lines(
            &visual_glyphs,
            max_w_unit,
            options.white_space,
        );

        // 5. Construct Line Boxes
        let mut line_boxes = Vec::with_capacity(text_lines.len());
        let mut current_y = 0.0f32;
        let mut max_width = 0.0f32;

        let line_height_unit = strut.line_height;
        let line_box_h = line_height_unit.to_f32();

        // Baseline offset from line top: half_leading_above + ascent
        let (leading_above, _) = compute_half_leading(
            line_height_unit,
            font_metrics.ascent,
            font_metrics.descent,
        );
        let base_baseline_offset = (font_metrics.ascent + leading_above).to_f32();

        for text_line in text_lines {
            let line_w = text_line.content_width.to_f32();
            if line_w > max_width {
                max_width = line_w;
            }

            // Adjust baseline according to vertical-align
            let vertical_shift = match options.vertical_align {
                VerticalAlignment::Baseline => 0.0,
                VerticalAlignment::Top => -(leading_above.to_f32()),
                VerticalAlignment::Bottom => leading_above.to_f32(),
                VerticalAlignment::Middle => (font_metrics.x_height.to_f32() / 2.0) - (options.font_size * 0.25),
                VerticalAlignment::Sub => options.font_size * 0.20,
                VerticalAlignment::Super => -(options.font_size * 0.35),
                VerticalAlignment::TextTop => 0.0,
                VerticalAlignment::TextBottom => 0.0,
            };

            let line_baseline = base_baseline_offset + vertical_shift;

            let bounds = Rect::from_xywh(0.0, current_y, line_w, line_box_h);

            line_boxes.push(LineBox {
                bounds,
                baseline: line_baseline,
                metrics: strut.metrics,
                glyphs: text_line.glyphs,
                is_hard_break: text_line.is_hard_break,
            });

            current_y += line_box_h;
        }

        let total_lines = line_boxes.len();
        let total_height = if total_lines == 0 { line_box_h } else { current_y };

        InlineLayoutResult {
            lines: line_boxes,
            total_width: max_width,
            total_height,
            line_count: total_lines,
        }
    }
}
