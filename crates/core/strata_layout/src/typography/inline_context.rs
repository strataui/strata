//! Inline Formatting Context (IFC), Line Boxes, and Subpixel Vertical Alignment.
//!
//! Organizes shaped glyph runs into line boxes, resolves vertical alignments,
//! and computes final subpixel layout bounds with full Blink LayoutNG parity.

use strata_primitives::geom::LayoutUnit;
use crate::geometry::Rect;
use crate::exclusions::{ExclusionSpace, LineOpportunity};
use strata_style::types::{Color, FontStyle, FontWeight, TextDecoration};
use super::bidi::BidiEngine;
use super::font_metrics::{compute_half_leading, CssStrut, FontHeight, FontMetrics};
use super::line_breaker::{LineBreaker, WhiteSpaceMode};
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

/// Typographic styling applied to an individual inline text run within an IFC.
#[derive(Debug, Clone, PartialEq)]
pub struct InlineTextStyle {
    pub font_family: String,
    pub font_size: f32,
    pub font_weight: FontWeight,
    pub font_style: FontStyle,
    pub color: Color,
    pub background_color: Option<Color>,
    pub text_decoration: TextDecoration,
    pub vertical_align: VerticalAlignment,
    pub letter_spacing: f32,
    pub line_height: Option<f32>,
    pub is_monospace: bool,
    pub href: Option<String>,
}

impl Default for InlineTextStyle {
    fn default() -> Self {
        Self {
            font_family: "system-ui, sans-serif".to_string(),
            font_size: 16.0,
            font_weight: FontWeight::Normal,
            font_style: FontStyle::Normal,
            color: Color::new(0, 0, 0, 255),
            background_color: None,
            text_decoration: TextDecoration::None,
            vertical_align: VerticalAlignment::Baseline,
            letter_spacing: 0.0,
            line_height: None,
            is_monospace: false,
            href: None,
        }
    }
}

/// An individual item in an Inline Formatting Context stream.
#[derive(Debug, Clone, PartialEq)]
pub enum InlineItem {
    /// Text content with specific typographic styling.
    Text {
        text: String,
        style: InlineTextStyle,
    },
    /// An atomic inline element (e.g. <img>, inline <button>, <input>).
    Atomic {
        node_index: usize,
        width: f32,
        height: f32,
        margin_left: f32,
        margin_right: f32,
        vertical_align: VerticalAlignment,
    },
    /// Forced hard line break (<br>).
    HardBreak,
}

impl InlineItem {
    pub fn text(text: impl Into<String>, style: InlineTextStyle) -> Self {
        Self::Text {
            text: text.into(),
            style,
        }
    }

    pub fn atomic(
        node_index: usize,
        width: f32,
        height: f32,
        margin_left: f32,
        margin_right: f32,
        vertical_align: VerticalAlignment,
    ) -> Self {
        Self::Atomic {
            node_index,
            width,
            height,
            margin_left,
            margin_right,
            vertical_align,
        }
    }

    pub fn hard_break() -> Self {
        Self::HardBreak
    }
}

/// A visual fragment of an inline text run positioned on a specific line box.
#[derive(Debug, Clone, PartialEq)]
pub struct TextFragment {
    /// Bounding rectangle in container coordinate space (x, y, width, height).
    pub bounds: Rect,
    /// Substring of the originating inline item positioned on this line.
    pub text: String,
    /// Baseline vertical offset from the container top.
    pub baseline_y: f32,
    /// Typographic style applied to this fragment.
    pub style: InlineTextStyle,
    /// Originating inline item index in the IFC stream.
    pub item_index: usize,
}

/// A positioned atomic inline fragment on a line box.
#[derive(Debug, Clone, PartialEq)]
pub struct AtomicFragment {
    /// Bounding rectangle in container coordinate space.
    pub bounds: Rect,
    /// Child node index in the parent's children array.
    pub node_index: usize,
}

/// A positioned inline line box representing a single rendered text line.
#[derive(Debug, Clone, PartialEq)]
pub struct LineBox {
    /// Bounding rectangle in container coordinate space (x, y, width, height).
    pub bounds: Rect,
    /// Distance from the top of the line box to the dominant baseline.
    pub baseline: f32,
    /// Ascent and descent bounds encompassing all inline items on this line.
    pub metrics: FontHeight,
    /// Positioned glyphs on this line.
    pub glyphs: Vec<ShapedGlyph>,
    /// Visual text fragments positioned on this line.
    pub text_fragments: Vec<TextFragment>,
    /// Positioned atomic inline boxes on this line.
    pub atomic_fragments: Vec<AtomicFragment>,
    /// Whether this line was terminated by a hard newline.
    pub is_hard_break: bool,
}

/// Options configuring inline text layout.
#[derive(Debug, Clone, Copy)]
pub struct InlineLayoutOptions<'a> {
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
    /// Active exclusion space for float obstructions.
    pub exclusion_space: Option<&'a ExclusionSpace>,
    /// Container vertical origin offset within the BFC for float alignment.
    pub bfc_offset_y: f32,
}

impl<'a> Default for InlineLayoutOptions<'a> {
    fn default() -> Self {
        Self {
            font_size: 16.0,
            line_height: None,
            max_width: None,
            white_space: WhiteSpaceMode::Normal,
            vertical_align: VerticalAlignment::Baseline,
            is_monospace: false,
            exclusion_space: None,
            bfc_offset_y: 0.0,
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

/// Internal intermediate token for breaking and measuring the inline stream.
#[derive(Debug, Clone)]
struct InlineToken {
    item_index: usize,
    text: String,
    style: InlineTextStyle,
    glyphs: Vec<ShapedGlyph>,
    width: f32,
    #[allow(dead_code)]
    can_break_after: bool,
    is_hard_break: bool,
    is_whitespace_only: bool,
    atomic: Option<AtomicInlineData>,
}

#[derive(Debug, Clone)]
struct AtomicInlineData {
    node_index: usize,
    width: f32,
    height: f32,
    margin_left: f32,
    margin_right: f32,
    #[allow(dead_code)]
    vertical_align: VerticalAlignment,
}

/// High-performance Inline Formatting Context engine.
#[derive(Debug, Clone, Default)]
pub struct InlineFormattingContext {
    bidi: BidiEngine,
    shaper: TextShaper,
    #[allow(dead_code)]
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

    /// Helper to emit a shaped text token.
    fn make_text_token(
        &self,
        item_index: usize,
        text: String,
        style: &InlineTextStyle,
        font_metrics: &FontMetrics,
        can_break_after: bool,
    ) -> InlineToken {
        let is_whitespace_only = text.chars().all(|c| c.is_whitespace() || c == '\u{00A0}');
        let bidi_runs = self.bidi.resolve_runs(&text);
        let mut shaped = Vec::new();

        let weight_multiplier = match style.font_weight {
            FontWeight::Bold | FontWeight::ExtraBold | FontWeight::Black => 1.05,
            FontWeight::SemiBold => 1.025,
            _ => 1.0,
        };

        for run in bidi_runs {
            let run_slice = &text[run.start..run.end];
            let mut run_glyphs = self.shaper.shape(run_slice, font_metrics, style.is_monospace);
            if run.is_rtl {
                run_glyphs.reverse();
            }
            shaped.extend(run_glyphs);
        }

        let mut token_width = 0.0f32;
        for g in &mut shaped {
            if weight_multiplier != 1.0 {
                g.x_advance = LayoutUnit::from_f32(g.x_advance.to_f32() * weight_multiplier);
            }
            token_width += g.x_advance.to_f32();
        }

        InlineToken {
            item_index,
            text,
            style: style.clone(),
            glyphs: shaped,
            width: token_width,
            can_break_after,
            is_hard_break: false,
            is_whitespace_only,
            atomic: None,
        }
    }

    /// Lays out a heterogeneous stream of inline phrasing items (text runs, atomic boxes, hard breaks).
    pub fn layout_inline_stream(
        &self,
        items: &[InlineItem],
        options: &InlineLayoutOptions,
    ) -> InlineLayoutResult {
        let strut_metrics = if options.is_monospace {
            FontMetrics::default_mono(options.font_size)
        } else {
            FontMetrics::default_sans(options.font_size)
        };
        let strut = CssStrut::new(&strut_metrics, options.line_height);

        if items.is_empty() {
            let height = strut.line_height.to_f32();
            return InlineLayoutResult {
                lines: Vec::new(),
                total_width: 0.0,
                total_height: height,
                line_count: 0,
            };
        }

        // 1. Tokenize stream into break units
        let mut tokens: Vec<InlineToken> = Vec::new();
        for (item_idx, item) in items.iter().enumerate() {
            match item {
                InlineItem::HardBreak => {
                    tokens.push(InlineToken {
                        item_index: item_idx,
                        text: String::new(),
                        style: InlineTextStyle::default(),
                        glyphs: Vec::new(),
                        width: 0.0,
                        can_break_after: true,
                        is_hard_break: true,
                        is_whitespace_only: false,
                        atomic: None,
                    });
                }
                InlineItem::Atomic {
                    node_index,
                    width,
                    height,
                    margin_left,
                    margin_right,
                    vertical_align,
                } => {
                    tokens.push(InlineToken {
                        item_index: item_idx,
                        text: String::new(),
                        style: InlineTextStyle::default(),
                        glyphs: Vec::new(),
                        width: margin_left + width + margin_right,
                        can_break_after: true,
                        is_hard_break: false,
                        is_whitespace_only: false,
                        atomic: Some(AtomicInlineData {
                            node_index: *node_index,
                            width: *width,
                            height: *height,
                            margin_left: *margin_left,
                            margin_right: *margin_right,
                            vertical_align: *vertical_align,
                        }),
                    });
                }
                InlineItem::Text { text, style } => {
                    if text.is_empty() {
                        continue;
                    }

                    let font_metrics = if style.is_monospace {
                        FontMetrics::default_mono(style.font_size)
                    } else {
                        FontMetrics::default_sans(style.font_size)
                    };

                    let mut current_word = String::new();
                    for ch in text.chars() {
                        if ch == '\n' {
                            if !current_word.is_empty() {
                                tokens.push(self.make_text_token(
                                    item_idx,
                                    std::mem::take(&mut current_word),
                                    style,
                                    &font_metrics,
                                    true,
                                ));
                            }
                            tokens.push(InlineToken {
                                item_index: item_idx,
                                text: String::new(),
                                style: style.clone(),
                                glyphs: Vec::new(),
                                width: 0.0,
                                can_break_after: true,
                                is_hard_break: true,
                                is_whitespace_only: false,
                                atomic: None,
                            });
                        } else if ch == ' ' || ch == '\t' {
                            if !current_word.is_empty() {
                                current_word.push(ch);
                                tokens.push(self.make_text_token(
                                    item_idx,
                                    std::mem::take(&mut current_word),
                                    style,
                                    &font_metrics,
                                    true,
                                ));
                            } else {
                                if !options.white_space.collapses_spaces()
                                    || tokens.is_empty()
                                    || !tokens.last().map_or(false, |t| t.text.ends_with(' '))
                                {
                                    current_word.push(ch);
                                    tokens.push(self.make_text_token(
                                        item_idx,
                                        std::mem::take(&mut current_word),
                                        style,
                                        &font_metrics,
                                        true,
                                    ));
                                }
                            }
                        } else if ch == '-' || ch == '\u{2010}' || ch == '\u{00AD}' {
                            current_word.push(ch);
                            tokens.push(self.make_text_token(
                                item_idx,
                                std::mem::take(&mut current_word),
                                style,
                                &font_metrics,
                                true,
                            ));
                        } else if ch == '\u{200B}' {
                            if !current_word.is_empty() {
                                tokens.push(self.make_text_token(
                                    item_idx,
                                    std::mem::take(&mut current_word),
                                    style,
                                    &font_metrics,
                                    true,
                                ));
                            }
                        } else if (ch >= '\u{4E00}' && ch <= '\u{9FFF}')
                            || (ch >= '\u{3040}' && ch <= '\u{30FF}')
                            || (ch >= '\u{AC00}' && ch <= '\u{D7AF}')
                        {
                            if !current_word.is_empty() {
                                tokens.push(self.make_text_token(
                                    item_idx,
                                    std::mem::take(&mut current_word),
                                    style,
                                    &font_metrics,
                                    true,
                                ));
                            }
                            let mut cjk_str = String::new();
                            cjk_str.push(ch);
                            tokens.push(self.make_text_token(
                                item_idx,
                                cjk_str,
                                style,
                                &font_metrics,
                                true,
                            ));
                        } else {
                            current_word.push(ch);
                        }
                    }

                    if !current_word.is_empty() {
                        tokens.push(self.make_text_token(
                            item_idx,
                            current_word,
                            style,
                            &font_metrics,
                            false,
                        ));
                    }
                }
            }
        }

        if tokens.is_empty() {
            let height = strut.line_height.to_f32();
            return InlineLayoutResult {
                lines: Vec::new(),
                total_width: 0.0,
                total_height: height,
                line_count: 0,
            };
        }

        // 2. Line breaking and assembly pass with dynamic exclusion opportunities
        let container_w = options.max_width.unwrap_or(f32::MAX);
        let allows_soft_wrap = options.white_space.allows_soft_wrap() && options.max_width.is_some();

        let mut line_boxes: Vec<LineBox> = Vec::new();
        let mut current_y = 0.0f32;
        let mut max_total_width = 0.0f32;

        let mut token_idx = 0;
        let num_tokens = tokens.len();

        while token_idx < num_tokens {
            let line_start_idx = token_idx;
            let mut line_tokens: Vec<InlineToken> = Vec::new();
            let mut line_width = 0.0f32;
            let mut is_hard_break = false;

            // Estimate line height for exclusion query
            let est_line_height = strut.line_height.to_f32();
            let bfc_y = options.bfc_offset_y + current_y;

            let opp = if let Some(es) = options.exclusion_space {
                es.line_opportunity(bfc_y, est_line_height, container_w)
            } else {
                LineOpportunity {
                    start_x: 0.0,
                    available_width: container_w,
                    left_float_edge: 0.0,
                    right_float_edge: container_w,
                }
            };

            // If available width is too narrow (< 20px) and floats exist, drop down past the float
            if opp.available_width < 20.0 && container_w > 50.0 {
                if let Some(es) = options.exclusion_space {
                    if let Some(next_bottom) = es.next_float_bottom_after(bfc_y) {
                        current_y = next_bottom - options.bfc_offset_y;
                        continue;
                    }
                }
            }

            while token_idx < num_tokens {
                let tok = &tokens[token_idx];

                if tok.is_hard_break {
                    is_hard_break = true;
                    token_idx += 1;
                    break;
                }

                if allows_soft_wrap
                    && !line_tokens.is_empty()
                    && (line_width + tok.width > opp.available_width)
                {
                    // Soft wrap opportunity reached for this line
                    break;
                }

                // If line is empty and single token exceeds available_width:
                // Check if dropping past float gives more width
                if line_tokens.is_empty()
                    && allows_soft_wrap
                    && tok.width > opp.available_width
                    && opp.available_width < container_w
                {
                    if let Some(es) = options.exclusion_space {
                        if let Some(next_bottom) = es.next_float_bottom_after(bfc_y) {
                            current_y = next_bottom - options.bfc_offset_y;
                            break;
                        }
                    }
                }

                // Skip leading whitespace on newly wrapped line
                if line_tokens.is_empty()
                    && tok.is_whitespace_only
                    && options.white_space.collapses_spaces()
                {
                    token_idx += 1;
                    continue;
                }

                line_width += tok.width;
                line_tokens.push(tok.clone());
                token_idx += 1;
            }

            // If we broke because of float drop-down on empty line, retry
            if line_tokens.is_empty() && !is_hard_break {
                if token_idx == line_start_idx {
                    // Force consume at least one token to avoid infinite loop
                    line_tokens.push(tokens[token_idx].clone());
                    token_idx += 1;
                } else {
                    continue;
                }
            }

            // Assemble LineBox for this line
            let has_text_tokens = line_tokens.iter().any(|tok| tok.atomic.is_none() && !tok.is_whitespace_only);
            let (mut line_max_ascent, mut line_max_descent) = if has_text_tokens {
                (strut.metrics.ascent.to_f32(), strut.metrics.descent.to_f32())
            } else {
                (0.0f32, 0.0f32)
            };

            for tok in &line_tokens {
                if let Some(ref at) = tok.atomic {
                    line_max_ascent = line_max_ascent.max(at.height);
                } else {
                    let tok_metrics = if tok.style.is_monospace {
                        FontMetrics::default_mono(tok.style.font_size)
                    } else {
                        FontMetrics::default_sans(tok.style.font_size)
                    };
                    let lh = tok.style.line_height.unwrap_or_else(|| {
                        tok_metrics.default_line_height().to_f32()
                    });
                    let (la, lb) = compute_half_leading(
                        LayoutUnit::from_f32(lh),
                        tok_metrics.ascent,
                        tok_metrics.descent,
                    );
                    let a = (tok_metrics.ascent + la).to_f32();
                    let d = (tok_metrics.descent + lb).to_f32();
                    line_max_ascent = line_max_ascent.max(a);
                    line_max_descent = line_max_descent.max(d);
                }
            }

            let actual_line_height = line_max_ascent + line_max_descent;
            let base_vertical_shift = match options.vertical_align {
                VerticalAlignment::Baseline => 0.0,
                VerticalAlignment::Top => -((strut.line_height - strut_metrics.natural_height()).to_f32() / 2.0),
                VerticalAlignment::Bottom => (strut.line_height - strut_metrics.natural_height()).to_f32() / 2.0,
                VerticalAlignment::Middle => (strut_metrics.x_height.to_f32() / 2.0) - (options.font_size * 0.25),
                VerticalAlignment::Sub => options.font_size * 0.20,
                VerticalAlignment::Super => -(options.font_size * 0.35),
                VerticalAlignment::TextTop => 0.0,
                VerticalAlignment::TextBottom => 0.0,
            };
            let line_baseline = line_max_ascent + base_vertical_shift;

            let mut text_fragments = Vec::new();
            let mut atomic_fragments = Vec::new();
            let mut line_glyphs = Vec::new();
            let mut cur_x = opp.start_x;

            let mut idx = 0;
            while idx < line_tokens.len() {
                let tok = &line_tokens[idx];
                if let Some(ref at) = tok.atomic {
                    let ax = cur_x + at.margin_left;
                    let ay = current_y + (line_baseline - at.height);
                    atomic_fragments.push(AtomicFragment {
                        bounds: Rect::from_xywh(ax, ay, at.width, at.height),
                        node_index: at.node_index,
                    });
                    cur_x += at.margin_left + at.width + at.margin_right;
                    idx += 1;
                } else {
                    let target_item = tok.item_index;
                    let target_style = tok.style.clone();
                    let mut merged_text = String::new();
                    let mut merged_width = 0.0f32;
                    let start_x = cur_x;

                    while idx < line_tokens.len() {
                        let t = &line_tokens[idx];
                        if t.atomic.is_some()
                            || t.item_index != target_item
                            || t.style != target_style
                        {
                            break;
                        }
                        merged_text.push_str(&t.text);
                        merged_width += t.width;
                        line_glyphs.extend_from_slice(&t.glyphs);
                        cur_x += t.width;
                        idx += 1;
                    }

                    let tok_metrics = if target_style.is_monospace {
                        FontMetrics::default_mono(target_style.font_size)
                    } else {
                        FontMetrics::default_sans(target_style.font_size)
                    };

                    let vertical_shift = match target_style.vertical_align {
                        VerticalAlignment::Baseline => 0.0,
                        VerticalAlignment::Top => -(line_max_ascent - tok_metrics.ascent.to_f32()),
                        VerticalAlignment::Bottom => {
                            line_max_descent - tok_metrics.descent.to_f32()
                        }
                        VerticalAlignment::Middle => {
                            (tok_metrics.x_height.to_f32() / 2.0) - (target_style.font_size * 0.25)
                        }
                        VerticalAlignment::Sub => target_style.font_size * 0.20,
                        VerticalAlignment::Super => -(target_style.font_size * 0.35),
                        VerticalAlignment::TextTop => 0.0,
                        VerticalAlignment::TextBottom => 0.0,
                    };

                    let frag_baseline = current_y + line_baseline + vertical_shift;
                    let frag_y = frag_baseline - tok_metrics.ascent.to_f32();
                    let frag_h = (tok_metrics.ascent + tok_metrics.descent).to_f32();

                    text_fragments.push(TextFragment {
                        bounds: Rect::from_xywh(start_x, frag_y, merged_width, frag_h),
                        text: merged_text,
                        baseline_y: frag_baseline,
                        style: target_style,
                        item_index: target_item,
                    });
                }
            }

            let line_box_width = cur_x - opp.start_x;
            if cur_x > max_total_width {
                max_total_width = cur_x;
            }

            line_boxes.push(LineBox {
                bounds: Rect::from_xywh(opp.start_x, current_y, line_box_width, actual_line_height),
                baseline: line_baseline,
                metrics: FontHeight::new(
                    LayoutUnit::from_f32(line_max_ascent),
                    LayoutUnit::from_f32(line_max_descent),
                ),
                glyphs: line_glyphs,
                text_fragments,
                atomic_fragments,
                is_hard_break,
            });

            current_y += actual_line_height;
        }

        let total_lines = line_boxes.len();
        let total_height = if total_lines == 0 {
            strut.line_height.to_f32()
        } else {
            current_y
        };

        InlineLayoutResult {
            lines: line_boxes,
            total_width: max_total_width,
            total_height,
            line_count: total_lines,
        }
    }

    /// Formats input text into positioned line boxes, honoring metrics, BiDi, and line breaks.
    pub fn layout_text(&self, text: &str, options: &InlineLayoutOptions) -> InlineLayoutResult {
        let style = InlineTextStyle {
            font_size: options.font_size,
            vertical_align: options.vertical_align,
            line_height: options.line_height,
            is_monospace: options.is_monospace,
            ..Default::default()
        };
        let item = InlineItem::Text {
            text: text.to_string(),
            style,
        };
        self.layout_inline_stream(&[item], options)
    }
}
