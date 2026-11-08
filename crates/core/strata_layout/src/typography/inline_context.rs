//! Inline Formatting Context (IFC), Line Boxes, and Subpixel Vertical Alignment.
//!
//! Organizes shaped glyph runs into line boxes, resolves vertical alignments,
//! and computes final subpixel layout bounds with full Blink LayoutNG parity.

use super::bidi::BidiEngine;
use super::font_metrics::{compute_half_leading, CssStrut, FontHeight, FontMetrics};
use super::line_breaker::{LineBreaker, WhiteSpaceMode};
use super::shaper::{ShapedGlyph, TextShaper};
use crate::exclusions::{ExclusionSpace, LineOpportunity};
use crate::geometry::Rect;
use std::cell::RefCell;
use strata_primitives::geom::LayoutUnit;
use strata_style::types::{Color, FontStyle, FontWeight, TextDecoration};

thread_local! {
    static INLINE_LAYOUT_CACHE: std::cell::RefCell<std::collections::HashMap<u64, InlineLayoutResult>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
    static FONT_METRICS_CACHE: std::cell::RefCell<std::collections::HashMap<(u32, u32, bool, u64), FontMetrics>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// Hashes an inline item stream and option set into a deterministic layout cache key.
fn inline_layout_cache_key(items: &[InlineItem], options: &InlineLayoutOptions) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    // Measurement callbacks can be re-registered at runtime. Mixing the font
    // generation into the key prevents cached geometry from an older
    // measurement backend surviving a rebuild.
    crate::text::font_generation().hash(&mut hasher);
    options.font_size.to_bits().hash(&mut hasher);
    options
        .line_height
        .map(|v| v.to_bits())
        .unwrap_or(u32::MAX)
        .hash(&mut hasher);
    options
        .max_width
        .map(|v| v.to_bits())
        .unwrap_or(u32::MAX)
        .hash(&mut hasher);
    (options.white_space as u8).hash(&mut hasher);
    (options.vertical_align as u8).hash(&mut hasher);
    options.is_monospace.hash(&mut hasher);
    options.bfc_offset_y.to_bits().hash(&mut hasher);
    items.len().hash(&mut hasher);
    for item in items {
        match item {
            InlineItem::Text { text, style } => {
                1u8.hash(&mut hasher);
                text.hash(&mut hasher);
                style.font_family.0.hash(&mut hasher);
                style.font_size.to_bits().hash(&mut hasher);
                (style.font_weight as u8).hash(&mut hasher);
                (style.font_style as u8).hash(&mut hasher);
                style.letter_spacing.to_bits().hash(&mut hasher);
                style
                    .line_height
                    .map(|v| v.to_bits())
                    .unwrap_or(u32::MAX)
                    .hash(&mut hasher);
                style.is_monospace.hash(&mut hasher);
                (style.vertical_align as u8).hash(&mut hasher);
                // Paint-only fields must participate in the key. A cached
                // result carries cloned fragments with a full InlineTextStyle,
                // so a hit with a different color would repaint the first
                // run's styling for the life of the thread.
                style.color.r.hash(&mut hasher);
                style.color.g.hash(&mut hasher);
                style.color.b.hash(&mut hasher);
                style.color.a.hash(&mut hasher);
                match style.background_color {
                    Some(c) => {
                        1u8.hash(&mut hasher);
                        c.r.hash(&mut hasher);
                        c.g.hash(&mut hasher);
                        c.b.hash(&mut hasher);
                        c.a.hash(&mut hasher);
                    }
                    None => 0u8.hash(&mut hasher),
                }
                (style.text_decoration as u8).hash(&mut hasher);
                style.href.hash(&mut hasher);
            }
            InlineItem::Atomic {
                node_index,
                width,
                height,
                margin_left,
                margin_right,
                vertical_align,
            } => {
                2u8.hash(&mut hasher);
                node_index.hash(&mut hasher);
                width.to_bits().hash(&mut hasher);
                height.to_bits().hash(&mut hasher);
                margin_left.to_bits().hash(&mut hasher);
                margin_right.to_bits().hash(&mut hasher);
                (*vertical_align as u8).hash(&mut hasher);
            }
            InlineItem::HardBreak => 3u8.hash(&mut hasher),
        }
    }
    hasher.finish()
}

/// Builds font metrics from the registered platform callback, falling back to the
/// synthetic design-ratio metrics when no platform font backend is active.
fn metrics_for_style(
    font_family: strata_primitives::AtomId,
    font_size: f32,
    is_monospace: bool,
) -> FontMetrics {
    let cache_key = (
        font_family.0,
        font_size.to_bits(),
        is_monospace,
        crate::text::font_generation(),
    );
    if let Some(hit) = FONT_METRICS_CACHE.with(|cache| cache.borrow().get(&cache_key).copied()) {
        return hit;
    }
    if let Some(callback) = crate::text::get_font_metrics() {
        let (ascent, descent, line_gap) = callback(font_family, font_size, is_monospace);
        if ascent > 0.0 && descent >= 0.0 {
            let metrics = FontMetrics {
                ascent: LayoutUnit::from_f32(ascent),
                descent: LayoutUnit::from_f32(descent),
                line_gap: LayoutUnit::from_f32(line_gap.max(0.0)),
                x_height: LayoutUnit::from_f32(ascent * 0.56),
                cap_height: LayoutUnit::from_f32(ascent * 0.78),
                units_per_em: 1000,
                font_size,
            };
            FONT_METRICS_CACHE.with(|cache| {
                let mut map = cache.borrow_mut();
                if map.len() >= 4096 {
                    map.clear();
                }
                map.insert(cache_key, metrics);
            });
            return metrics;
        }
    }

    if is_monospace {
        FontMetrics::default_mono(font_size)
    } else {
        FontMetrics::default_sans(font_size)
    }
}

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
    pub font_family: strata_primitives::AtomId,
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
            font_family: strata_primitives::AtomId::FONT_SANS,
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
    /// Requested font family atom used for glyph advances and paint selection.
    pub font_family: strata_primitives::AtomId,
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

/// Returns true if a token cannot begin a new line per Unicode UAX #14 (Japanese Kinsoku Shori Gyoto Kinsoku).
#[inline]
pub fn is_prohibited_line_start(text: &str) -> bool {
    let mut chars = text.chars();
    match chars.next() {
        Some(c) => super::line_breaker::is_prohibited_start(c),
        None => false,
    }
}

/// Returns true if a token cannot end a line per Unicode UAX #14 (Japanese Kinsoku Shori Gyomatsu Kinsoku).
#[inline]
pub fn is_prohibited_line_end(text: &str) -> bool {
    let trimmed = text.trim_end_matches([' ', '\t']);
    if trimmed.len() != text.len() {
        return false;
    }
    match trimmed.chars().last() {
        Some(c) => super::line_breaker::is_prohibited_end(c),
        None => false,
    }
}

/// Returns true if a soft line break is permitted between two adjacent inline tokens per UAX #14.
#[inline]
fn can_break_between(prev: &InlineToken, next: &InlineToken) -> bool {
    if prev.is_hard_break || next.is_hard_break {
        return true;
    }
    if is_prohibited_line_start(&next.text) || is_prohibited_line_end(&prev.text) {
        return false;
    }
    if let (Some(p_ch), Some(n_ch)) = (prev.text.chars().last(), next.text.chars().next()) {
        if super::line_breaker::can_break_between_chars(p_ch, n_ch) {
            return true;
        }
    }
    prev.can_break_after || next.atomic.is_some()
}

/// Internal intermediate token for breaking and measuring the inline stream.
#[derive(Debug, Clone)]
struct InlineToken {
    item_index: usize,
    text: String,
    style: InlineTextStyle,
    glyphs: Vec<ShapedGlyph>,
    width: f32,
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

        let opt_font = super::opentype::lookup_font_for_style(
            style.font_family,
            style.font_weight,
            style.font_style,
        );

        for run in bidi_runs {
            let run_slice = &text[run.start..run.end];
            let mut run_glyphs = if let Some(ref font) = opt_font {
                self.shaper.shape_with_font(run_slice, font, style.font_size)
            } else {
                self.shaper.shape(run_slice, font_metrics, style.is_monospace)
            };
            if run.is_rtl {
                run_glyphs.reverse();
            }
            shaped.extend(run_glyphs);
        }

        let mut token_width = 0.0f32;
        for g in &shaped {
            token_width += g.x_advance.to_f32();
        }

        if crate::text::get_family_styled_text_measurer().is_some() {
            let (meas_w, _) = crate::text::measure_text_bounds_styled_with_family(
                &text,
                style.font_size,
                1.0,
                None,
                style.is_monospace,
                style.font_family,
                style.font_weight,
                style.font_style,
            );
            if token_width > 0.0 && meas_w > 0.0 {
                let scale = meas_w / token_width;
                for g in &mut shaped {
                    g.x_advance = LayoutUnit::from_f32(g.x_advance.to_f32() * scale);
                }
                token_width = meas_w;
            } else if meas_w > 0.0 {
                token_width = meas_w;
            }
        } else if crate::text::get_family_text_measurer().is_some() {
            let (meas_w, _) = crate::text::measure_text_bounds_with_family(
                &text,
                style.font_size,
                1.0,
                None,
                style.is_monospace,
                style.font_family,
            );
            if token_width > 0.0 && meas_w > 0.0 {
                let scale = meas_w / token_width;
                for g in &mut shaped {
                    g.x_advance = LayoutUnit::from_f32(g.x_advance.to_f32() * scale);
                }
                token_width = meas_w;
            } else if meas_w > 0.0 {
                token_width = meas_w;
            }
        } else if let Some(styled_measurer) = crate::text::get_styled_text_measurer() {
            let (meas_w, _) = styled_measurer(
                &text,
                style.font_size,
                1.0,
                None,
                style.is_monospace,
                style.font_weight,
                style.font_style,
            );
            if token_width > 0.0 && meas_w > 0.0 {
                let scale = meas_w / token_width;
                for g in &mut shaped {
                    g.x_advance = LayoutUnit::from_f32(g.x_advance.to_f32() * scale);
                }
                token_width = meas_w;
            } else if meas_w > 0.0 {
                token_width = meas_w;
            }
        } else if crate::text::get_text_measurer().is_some() {
            let (meas_w, _) = crate::text::measure_text_bounds_ext(
                &text,
                style.font_size,
                1.0,
                None,
                style.is_monospace,
            );
            if token_width > 0.0 && meas_w > 0.0 {
                let scale = meas_w / token_width;
                for g in &mut shaped {
                    g.x_advance = LayoutUnit::from_f32(g.x_advance.to_f32() * scale);
                }
                token_width = meas_w;
            } else if meas_w > 0.0 {
                token_width = meas_w;
            }
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
        let _prof = crate::prof::Scope::new(crate::prof::Slot::Inline);
        let cacheable = options.exclusion_space.is_none();
        let cache_key = if cacheable {
            Some(inline_layout_cache_key(items, options))
        } else {
            None
        };
        if let Some(key) = cache_key {
            let cached = INLINE_LAYOUT_CACHE.with(|cache| {
                let cache: &RefCell<std::collections::HashMap<u64, InlineLayoutResult>> = cache;
                cache.borrow().get(&key).cloned()
            });
            if let Some(result) = cached {
                return result;
            }
        }

        let result = self.layout_inline_stream_inner(items, options);
        if let (Some(key), true) = (cache_key, cacheable) {
            INLINE_LAYOUT_CACHE.with(|cache| {
                let cache: &RefCell<std::collections::HashMap<u64, InlineLayoutResult>> = cache;
                let mut map = cache.borrow_mut();
                if map.len() >= 512 {
                    map.clear();
                }
                map.insert(key, result.clone());
            });
        }
        result
    }

    fn layout_inline_stream_inner(
        &self,
        items: &[InlineItem],
        options: &InlineLayoutOptions,
    ) -> InlineLayoutResult {
        let strut_family = items
            .iter()
            .find_map(|item| match item {
                InlineItem::Text { style, .. } => Some(style.font_family),
                _ => None,
            })
            .unwrap_or(strata_primitives::AtomId::FONT_SANS);
        let strut_metrics =
            metrics_for_style(strut_family, options.font_size, options.is_monospace);
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

                    let font_metrics =
                        metrics_for_style(style.font_family, style.font_size, style.is_monospace);

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
                                    || !tokens.last().is_some_and(|t| t.text.ends_with(' '))
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
                        } else if ('\u{4E00}'..='\u{9FFF}').contains(&ch)
                            || ('\u{3400}'..='\u{4DBF}').contains(&ch)
                            || ('\u{3040}'..='\u{30FF}').contains(&ch)
                            || ('\u{AC00}'..='\u{D7AF}').contains(&ch)
                            || ('\u{3000}'..='\u{303F}').contains(&ch)
                            || ('\u{FF01}'..='\u{FF60}').contains(&ch)
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
                            let can_break_after = !super::line_breaker::is_prohibited_end(ch);
                            let mut cjk_str = String::new();
                            cjk_str.push(ch);
                            tokens.push(self.make_text_token(
                                item_idx,
                                cjk_str,
                                style,
                                &font_metrics,
                                can_break_after,
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
        let allows_soft_wrap =
            options.white_space.allows_soft_wrap() && options.max_width.is_some();

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

                // Skip leading whitespace on newly wrapped line
                if line_tokens.is_empty()
                    && tok.is_whitespace_only
                    && options.white_space.collapses_spaces()
                {
                    token_idx += 1;
                    continue;
                }

                // Group contiguous tokens without a valid break point between them (e.g. "version.[30]")
                let mut cluster_end = token_idx + 1;
                while cluster_end < num_tokens
                    && !tokens[cluster_end].is_hard_break
                    && !can_break_between(&tokens[cluster_end - 1], &tokens[cluster_end])
                {
                    cluster_end += 1;
                }
                let cluster_width: f32 =
                    tokens[token_idx..cluster_end].iter().map(|t| t.width).sum();

                if allows_soft_wrap
                    && !line_tokens.is_empty()
                    && (line_width + cluster_width > opp.available_width + 0.5)
                    && line_tokens
                        .last()
                        .is_none_or(|prev| can_break_between(prev, tok))
                    {
                        break;
                    }

                // If line is empty and unbreakable cluster exceeds available_width:
                // Check if dropping past float gives more width
                if line_tokens.is_empty()
                    && allows_soft_wrap
                    && cluster_width > opp.available_width + 0.5
                    && opp.available_width < container_w
                {
                    if let Some(es) = options.exclusion_space {
                        if let Some(next_bottom) = es.next_float_bottom_after(bfc_y) {
                            current_y = next_bottom - options.bfc_offset_y;
                            break;
                        }
                    }
                }

                for tok in &tokens[token_idx..cluster_end] {
                    line_width += tok.width;
                    line_tokens.push(tok.clone());
                }
                token_idx = cluster_end;
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
            let has_text_tokens = line_tokens
                .iter()
                .any(|tok| tok.atomic.is_none() && !tok.is_whitespace_only);
            let (mut line_max_ascent, mut line_max_descent) = if has_text_tokens {
                (
                    strut.metrics.ascent.to_f32(),
                    strut.metrics.descent.to_f32(),
                )
            } else {
                (0.0f32, 0.0f32)
            };

            for tok in &line_tokens {
                if let Some(ref at) = tok.atomic {
                    line_max_ascent = line_max_ascent.max(at.height);
                } else {
                    let tok_metrics = metrics_for_style(
                        tok.style.font_family,
                        tok.style.font_size,
                        tok.style.is_monospace,
                    );
                    let lh = tok
                        .style
                        .line_height
                        .unwrap_or_else(|| tok_metrics.default_line_height().to_f32());
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
                VerticalAlignment::Top => {
                    -((strut.line_height - strut_metrics.natural_height()).to_f32() / 2.0)
                }
                VerticalAlignment::Bottom => {
                    (strut.line_height - strut_metrics.natural_height()).to_f32() / 2.0
                }
                VerticalAlignment::Middle => {
                    (strut_metrics.x_height.to_f32() / 2.0) - (options.font_size * 0.25)
                }
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

                    // Pango paints the merged fragment as one continuously shaped
                    // string, whose advance is slightly wider than the sum of the
                    // individually measured token widths. Measure the merged run so
                    // the fragment width and the cursor advance match paint.
                    let frag_width = if crate::text::get_family_styled_text_measurer().is_some() {
                        let (measured_w, _) = crate::text::measure_text_bounds_styled_with_family(
                            &merged_text,
                            target_style.font_size,
                            1.0,
                            None,
                            target_style.is_monospace,
                            target_style.font_family,
                            target_style.font_weight,
                            target_style.font_style,
                        );
                        if measured_w > 0.0 {
                            measured_w
                        } else {
                            merged_width
                        }
                    } else {
                        merged_width
                    };
                    // Replace the accumulated token-sum advance with the continuous
                    // width so the next fragment starts at the painted position.
                    cur_x += frag_width - merged_width;

                    let tok_metrics = metrics_for_style(
                        target_style.font_family,
                        target_style.font_size,
                        target_style.is_monospace,
                    );

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
                        bounds: Rect::from_xywh(start_x, frag_y, frag_width, frag_h),
                        text: merged_text,
                        baseline_y: frag_baseline,
                        font_family: target_style.font_family,
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
