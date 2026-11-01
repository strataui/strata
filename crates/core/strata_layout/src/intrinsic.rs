//! Chromium LayoutNG-parity Intrinsic Min/Max Sizing (`ComputeMinMaxSizes`) and shrink-to-fit solver.
//!
//! Evaluates `min-content`, `max-content`, and `fit-content` across text leaves,
//! block containers, flexbox items, and grid tracks before geometric layout resolution.

use strata_style::types::{Display, FlexDirection};
use crate::box_model::resolve_dimension;
use crate::tree::LayoutNode;
use crate::typography::{FontMetrics, InlineFormattingContext, InlineLayoutOptions, WhiteSpaceMode};

/// Intrinsic dimension bounds (min-content and max-content inline sizes).
/// Mirrors Chromium's `MinMaxSizes` in `third_party/blink/renderer/core/layout/min_max_sizes.h`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MinMaxSizes {
    /// Narrowest inline size element can take without unhandled overflow.
    pub min_content: f32,
    /// Ideal inline size assuming unconstrained space without soft wrapping.
    pub max_content: f32,
}

impl MinMaxSizes {
    pub const fn new(min_content: f32, max_content: f32) -> Self {
        Self { min_content, max_content }
    }

    pub const fn zero() -> Self {
        Self { min_content: 0.0, max_content: 0.0 }
    }

    pub const fn exact(val: f32) -> Self {
        Self { min_content: val, max_content: val }
    }

    /// Evaluates the standard CSS shrink-to-fit sizing formula:
    /// `ShrinkToFit(A) = min(max_content, max(min_content, available_size))`
    #[inline(always)]
    pub fn shrink_to_fit(&self, available_size: f32) -> f32 {
        let avail = available_size.max(0.0);
        self.max_content.min(self.min_content.max(avail))
    }

    /// Expands both bounds to encompass a target value.
    #[inline(always)]
    pub fn encompass(&mut self, val: f32) {
        self.min_content = self.min_content.max(val);
        self.max_content = self.max_content.max(val);
    }

    /// Adds horizontal non-content insets (padding + borders) to both bounds.
    #[inline(always)]
    pub fn add_insets(&mut self, inline_insets: f32) {
        self.min_content += inline_insets;
        self.max_content += inline_insets;
    }

    /// Clamps bounds against explicit min and max width constraints.
    pub fn clamp(&self, min: Option<f32>, max: Option<f32>) -> Self {
        let mut min_c = self.min_content;
        let mut max_c = self.max_content;

        if let Some(min_val) = min {
            min_c = min_c.max(min_val);
            max_c = max_c.max(min_val);
        }

        if let Some(max_val) = max {
            min_c = min_c.min(max_val);
            max_c = max_c.min(max_val);
        }

        Self {
            min_content: min_c,
            max_content: max_c.max(min_c),
        }
    }
}

/// Recursively computes intrinsic min-content and max-content sizes for a layout node.
pub fn compute_min_max_sizes(node: &LayoutNode, parent_size: Option<f32>) -> MinMaxSizes {
    let avail = parent_size.unwrap_or(0.0);

    // 1. Definite width: If width is explicitly set (px, calc, percent with parent), return exact
    if let Some(exact_w) = resolve_dimension(node.style.width, avail) {
        let min_w = resolve_dimension(node.style.min_width, avail);
        let max_w = resolve_dimension(node.style.max_width, avail);
        return MinMaxSizes::exact(exact_w).clamp(min_w, max_w);
    }

    let inline_insets = node.style.padding.left
        + node.style.padding.right
        + node.style.border_width.left
        + node.style.border_width.right;

    let mut result = MinMaxSizes::zero();

    // 2. Leaf text node or container with direct text
    if let Some(ref text) = node.text {
        if !text.is_empty() {
            let ifc = InlineFormattingContext::new();

            // Max content: unbroken single line with unlimited width
            let max_opt = InlineLayoutOptions {
                font_size: node.style.font_size,
                line_height: Some(node.style.line_height * node.style.font_size),
                max_width: None,
                white_space: WhiteSpaceMode::NoWrap,
                ..Default::default()
            };
            let max_res = ifc.layout_text(text, &max_opt);
            result.max_content = max_res.total_width;

            // Min content: break on every opportunity (narrowest word)
            let mut max_word_w = 0.0f32;
            let metrics = FontMetrics::default_sans(node.style.font_size);
            let shaper = crate::typography::TextShaper::new();

            for word in text.split_whitespace() {
                let glyphs = shaper.shape(word, &metrics, false);
                let word_w: f32 = glyphs.iter().map(|g| g.x_advance.to_f32()).sum();
                if word_w > max_word_w {
                    max_word_w = word_w;
                }
            }
            result.min_content = max_word_w;
        }
    }

    // 3. Children contribution
    if !node.children.is_empty() {
        match node.style.display {
            Display::Flex | Display::InlineFlex => {
                if node.style.flex_direction == FlexDirection::Row || node.style.flex_direction == FlexDirection::RowReverse {
                    // Flex Row: items add up along inline axis
                    let mut sum_min = 0.0f32;
                    let mut sum_max = 0.0f32;
                    let gaps = if node.children.len() > 1 {
                        (node.children.len() - 1) as f32 * node.style.gap_col.max(node.style.gap)
                    } else {
                        0.0
                    };

                    for child in &node.children {
                        let child_sizes = compute_min_max_sizes(child, parent_size);
                        let child_margin = child.style.margin.left + child.style.margin.right;
                        sum_min += child_sizes.min_content + child_margin;
                        sum_max += child_sizes.max_content + child_margin;
                    }

                    result.min_content = result.min_content.max(sum_min + gaps);
                    result.max_content = result.max_content.max(sum_max + gaps);
                } else {
                    // Flex Column: items stack vertically, inline size is maximum of children
                    for child in &node.children {
                        let child_sizes = compute_min_max_sizes(child, parent_size);
                        let child_margin = child.style.margin.left + child.style.margin.right;
                        result.min_content = result.min_content.max(child_sizes.min_content + child_margin);
                        result.max_content = result.max_content.max(child_sizes.max_content + child_margin);
                    }
                }
            }
            _ => {
                // Standard Block Layout: inline size is maximum of in-flow children
                for child in &node.children {
                    let child_sizes = compute_min_max_sizes(child, parent_size);
                    let child_margin = child.style.margin.left + child.style.margin.right;
                    result.min_content = result.min_content.max(child_sizes.min_content + child_margin);
                    result.max_content = result.max_content.max(child_sizes.max_content + child_margin);
                }
            }
        }
    }

    // 4. Add non-content padding and border insets
    result.add_insets(inline_insets);

    // 5. Clamp against min-width and max-width
    let min_w = resolve_dimension(node.style.min_width.clone(), avail);
    let max_w = resolve_dimension(node.style.max_width.clone(), avail);
    result.clamp(min_w, max_w)
}
