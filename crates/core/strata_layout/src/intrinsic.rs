//! Chromium LayoutNG-parity Intrinsic Min/Max Sizing (`ComputeMinMaxSizes`) and shrink-to-fit solver.
//!
//! Evaluates `min-content`, `max-content`, and `fit-content` across text leaves,
//! block containers, flexbox items, and grid tracks before geometric layout resolution.

use crate::box_model::resolve_dimension;
use crate::tree::LayoutNode;
use strata_style::types::{Display, FlexDirection, GridTrack, GridTrackBreadth, Position};

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
        Self {
            min_content,
            max_content,
        }
    }

    pub const fn zero() -> Self {
        Self {
            min_content: 0.0,
            max_content: 0.0,
        }
    }

    pub const fn exact(val: f32) -> Self {
        Self {
            min_content: val,
            max_content: val,
        }
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
///
/// Results are memoized for the duration of one root layout session. Layout
/// nodes never change their style, text, or child identity while a layout pass
/// is running, so a `(node, available parent size)` pair is a stable key and the
/// cache is cleared when the outermost layout call finishes.
pub fn compute_min_max_sizes(node: &LayoutNode, parent_size: Option<f32>) -> MinMaxSizes {
    let _prof = crate::prof::Scope::new(crate::prof::Slot::Intrinsic);
    if let Some(hit) = memo_lookup(node, parent_size) {
        return hit;
    }
    let result = compute_min_max_sizes_uncached(node, parent_size);
    memo_store(node, parent_size, result);
    result
}

fn memo_key(node: &LayoutNode, parent_size: Option<f32>) -> (usize, u32) {
    (
        node as *const LayoutNode as usize,
        parent_size.map(f32::to_bits).unwrap_or(u32::MAX),
    )
}

fn memo_lookup(node: &LayoutNode, parent_size: Option<f32>) -> Option<MinMaxSizes> {
    if !in_session() {
        return None;
    }
    let key = memo_key(node, parent_size);
    MIN_MAX_MEMO.with(|memo| {
        memo.borrow()
            .as_ref()
            .and_then(|map| map.get(&key).copied())
    })
}

fn memo_store(node: &LayoutNode, parent_size: Option<f32>, sizes: MinMaxSizes) {
    if !in_session() {
        return;
    }
    let key = memo_key(node, parent_size);
    MIN_MAX_MEMO.with(|memo| {
        if let Some(map) = memo.borrow_mut().as_mut() {
            map.insert(key, sizes);
        }
    });
}

thread_local! {
    static MIN_MAX_MEMO: std::cell::RefCell<Option<std::collections::HashMap<(usize, u32), MinMaxSizes>>> =
        const { std::cell::RefCell::new(None) };
    static SESSION_DEPTH: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn in_session() -> bool {
    SESSION_DEPTH.with(|depth| depth.get() > 0)
}

/// Opens a layout session, returning true when this call is the outermost one.
pub fn begin_layout_session() -> bool {
    let is_root = SESSION_DEPTH.with(|depth| {
        let current = depth.get();
        depth.set(current + 1);
        current == 0
    });
    if is_root {
        MIN_MAX_MEMO.with(|memo| {
            let mut slot = memo.borrow_mut();
            if let Some(map) = slot.as_mut() {
                map.clear();
            } else {
                *slot = Some(std::collections::HashMap::new());
            }
        });
    }
    is_root
}

/// Closes the layout session and drops memoized intrinsic sizes at the root.
pub fn end_layout_session(is_root: bool) {
    if is_root {
        MIN_MAX_MEMO.with(|memo| {
            if let Some(map) = memo.borrow_mut().as_mut() {
                map.clear();
            }
        });
    }
    SESSION_DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
}

fn compute_min_max_sizes_uncached(node: &LayoutNode, parent_size: Option<f32>) -> MinMaxSizes {
    let avail = parent_size.unwrap_or(0.0);

    // 1. Definite non-percentage width on element containers:
    // Only element containers without direct text content obey explicit CSS width in intrinsic sizing.
    // Per W3C CSS Sizing Level 3 §5.1, percentage widths are treated as auto for intrinsic sizing.
    if node.text.is_none()
        && !matches!(node.style.width, strata_style::types::Dimension::Percent(_))
    {
        if let Some(exact_w) = resolve_dimension(node.style.width, avail) {
            let min_w = resolve_dimension(node.style.min_width, avail);
            let max_w = resolve_dimension(node.style.max_width, avail);
            return MinMaxSizes::exact(exact_w).clamp(min_w, max_w);
        }
    }

    let inline_insets = node.style.padding.left
        + node.style.padding.right
        + node.style.border_width.left
        + node.style.border_width.right;

    let mut result = MinMaxSizes::zero();

    // 2. Leaf text node or container with direct text / inline_items
    if let Some(ref text) = node.text {
        if !text.is_empty() {
            let inline_style = crate::box_model::inline_style_from_computed_style(&node.style);
            result.max_content = measure_inline_text_width(text, &inline_style);
            result.min_content = widest_word_width(text, &inline_style);
        }
    } else if node.children.is_empty() && !node.inline_items.is_empty() {
        let mut max_line = 0.0f32;
        let mut current_line = 0.0f32;
        let mut widest_unit = 0.0f32;
        for item in &node.inline_items {
            match item {
                crate::typography::InlineItem::Text { text, style } => {
                    current_line += measure_inline_text_width(text, style);
                    widest_unit = widest_unit.max(widest_word_width(text, style));
                }
                crate::typography::InlineItem::Atomic {
                    width,
                    margin_left,
                    margin_right,
                    ..
                } => {
                    let unit = width + margin_left + margin_right;
                    current_line += unit;
                    widest_unit = widest_unit.max(unit);
                }
                crate::typography::InlineItem::HardBreak => {
                    max_line = max_line.max(current_line);
                    current_line = 0.0;
                }
            }
        }
        result.max_content = max_line.max(current_line);
        result.min_content = widest_unit;
    }

    // 3. Children contribution
    if !node.children.is_empty() {
        match node.style.display {
            Display::Flex | Display::InlineFlex => {
                if node.style.flex_direction == FlexDirection::Row
                    || node.style.flex_direction == FlexDirection::RowReverse
                {
                    // Flex Row: in-flow items add up along inline axis
                    let mut sum_min = 0.0f32;
                    let mut sum_max = 0.0f32;
                    let in_flow_children: Vec<_> = node
                        .children
                        .iter()
                        .filter(|c| {
                            c.style.display != Display::None
                                && c.style.position != Position::Absolute
                                && c.style.position != Position::Fixed
                        })
                        .collect();
                    let gaps = if in_flow_children.len() > 1 {
                        (in_flow_children.len() - 1) as f32 * node.style.gap_col.max(node.style.gap)
                    } else {
                        0.0
                    };

                    for child in in_flow_children {
                        let child_sizes = compute_min_max_sizes(child, parent_size);
                        let child_margin = child.style.margin.left + child.style.margin.right;
                        sum_min += child_sizes.min_content + child_margin;
                        sum_max += child_sizes.max_content + child_margin;
                    }

                    result.min_content = result.min_content.max(sum_min + gaps);
                    result.max_content = result.max_content.max(sum_max + gaps);
                } else {
                    // Flex Column: in-flow items stack vertically, inline size is maximum of children
                    for child in &node.children {
                        if child.style.display == Display::None
                            || child.style.position == Position::Absolute
                            || child.style.position == Position::Fixed
                        {
                            continue;
                        }
                        let child_sizes = compute_min_max_sizes(child, parent_size);
                        let child_margin = child.style.margin.left + child.style.margin.right;
                        result.min_content = result
                            .min_content
                            .max(child_sizes.min_content + child_margin);
                        result.max_content = result
                            .max_content
                            .max(child_sizes.max_content + child_margin);
                    }
                }
            }
            Display::Grid | Display::InlineGrid => {
                let grid_sizes = compute_grid_min_max_sizes(node, parent_size);
                result.min_content = result.min_content.max(grid_sizes.min_content);
                result.max_content = result.max_content.max(grid_sizes.max_content);
            }
            _ => {
                // Standard Block & Inline Formatting Context Layout:
                // Contiguous inline phrasing children accumulate horizontally along the same line,
                // while block children stack vertically.
                let mut inline_run_max = 0.0f32;
                for child in &node.children {
                    if child.style.display == Display::None
                        || child.style.position == Position::Absolute
                        || child.style.position == Position::Fixed
                    {
                        continue;
                    }
                    let child_sizes = compute_min_max_sizes(child, parent_size);
                    let child_margin = child.style.margin.left + child.style.margin.right;
                    result.min_content = result
                        .min_content
                        .max(child_sizes.min_content + child_margin);

                    if crate::box_model::is_inline_phrasing_node(child) {
                        inline_run_max += child_sizes.max_content + child_margin;
                    } else {
                        result.max_content = result.max_content.max(inline_run_max);
                        inline_run_max = 0.0;
                        result.max_content = result
                            .max_content
                            .max(child_sizes.max_content + child_margin);
                    }
                }
                result.max_content = result.max_content.max(inline_run_max);
            }
        }
    }

    // 4. Add non-content padding and border insets
    result.add_insets(inline_insets);

    // 5. Clamp against min-width and max-width
    let min_w = resolve_dimension(node.style.min_width, avail);
    let max_w = resolve_dimension(node.style.max_width, avail);
    result.clamp(min_w, max_w)
}

/// Recursively computes intrinsic min-content and max-content sizes for a grid container
/// according to W3C CSS Grid Layout Level 2 §12.1.
fn compute_grid_min_max_sizes(node: &LayoutNode, parent_size: Option<f32>) -> MinMaxSizes {
    let in_flow_children: Vec<_> = node
        .children
        .iter()
        .filter(|c| {
            c.style.display != Display::None
                && c.style.position != Position::Absolute
                && c.style.position != Position::Fixed
        })
        .collect();

    if in_flow_children.is_empty() {
        return MinMaxSizes::zero();
    }

    let gap_col = node.style.gap_col.max(node.style.gap);

    if !node.style.grid_template_columns.is_empty() {
        let num_tracks = node.style.grid_template_columns.len();
        let mut track_items: Vec<Vec<&LayoutNode>> = vec![Vec::new(); num_tracks];

        for (idx, child) in in_flow_children.iter().enumerate() {
            let col = if let Some(line) = child.style.grid_column_start {
                if line >= 1 {
                    ((line - 1) as usize) % num_tracks
                } else {
                    idx % num_tracks
                }
            } else {
                idx % num_tracks
            };
            track_items[col].push(child);
        }

        let mut sum_min = 0.0f32;
        let mut sum_max = 0.0f32;

        for (col_idx, track) in node.style.grid_template_columns.iter().enumerate() {
            let items = &track_items[col_idx];
            let mut items_min = 0.0f32;
            let mut items_max = 0.0f32;

            for item in items {
                let s = compute_min_max_sizes(item, parent_size);
                let m = item.style.margin.left + item.style.margin.right;
                items_min = items_min.max(s.min_content + m);
                items_max = items_max.max(s.max_content + m);
            }

            match track {
                GridTrack::Px(px) => {
                    let w = px.max(0.0);
                    sum_min += w;
                    sum_max += w;
                }
                GridTrack::Percent(pct) => {
                    if let Some(avail) = parent_size {
                        let w = ((pct / 100.0) * avail).max(0.0);
                        sum_min += w;
                        sum_max += w;
                    } else {
                        sum_min += items_min;
                        sum_max += items_max;
                    }
                }
                GridTrack::Fr(fr) => {
                    sum_min += items_min;
                    sum_max += items_max.max(items_min * fr);
                }
                GridTrack::Auto => {
                    sum_min += items_min;
                    sum_max += items_max;
                }
                GridTrack::MinContent => {
                    sum_min += items_min;
                    sum_max += items_min;
                }
                GridTrack::MaxContent => {
                    sum_min += items_min;
                    sum_max += items_max;
                }
                GridTrack::MinMax(min_b, max_b) => {
                    let t_min = match min_b {
                        GridTrackBreadth::Px(px) => *px,
                        GridTrackBreadth::MinContent => items_min,
                        GridTrackBreadth::MaxContent => items_max,
                        _ => items_min,
                    };
                    let t_max = match max_b {
                        GridTrackBreadth::Px(px) => *px,
                        GridTrackBreadth::MaxContent => items_max,
                        GridTrackBreadth::MinContent => items_min,
                        GridTrackBreadth::Fr(f) => items_max.max(items_min * f),
                        _ => items_max,
                    };
                    sum_min += t_min;
                    sum_max += t_max.max(t_min);
                }
                GridTrack::Subgrid => {
                    sum_min += items_min;
                    sum_max += items_max;
                }
            }
        }

        let gaps = if num_tracks > 1 {
            (num_tracks - 1) as f32 * gap_col
        } else {
            0.0
        };

        MinMaxSizes::new(sum_min + gaps, sum_max + gaps)
    } else {
        // Auto / single-column fallback
        let mut max_min = 0.0f32;
        let mut max_max = 0.0f32;
        for child in in_flow_children {
            let s = compute_min_max_sizes(child, parent_size);
            let m = child.style.margin.left + child.style.margin.right;
            max_min = max_min.max(s.min_content + m);
            max_max = max_max.max(s.max_content + m);
        }
        MinMaxSizes::new(max_min, max_max)
    }
}

/// Resolves the line-height multiplier for an inline text style.
fn inline_line_height_multiplier(style: &crate::typography::InlineTextStyle) -> f32 {
    match style.line_height {
        Some(line_height) if style.font_size > 0.0 => (line_height / style.font_size).max(0.01),
        _ => 1.2,
    }
}

/// Measures the unbroken inline width of a text run using platform font metrics.
fn measure_inline_text_width(text: &str, style: &crate::typography::InlineTextStyle) -> f32 {
    let (width, _) = crate::text::measure_text_bounds_styled_with_family(
        text,
        style.font_size,
        inline_line_height_multiplier(style),
        None,
        style.is_monospace,
        style.font_family,
        style.font_weight,
        style.font_style,
    );
    let tracking = style.letter_spacing * text.chars().count().saturating_sub(1) as f32;
    width + tracking
}

/// Returns the widest unbreakable word in a text run.
fn widest_word_width(text: &str, style: &crate::typography::InlineTextStyle) -> f32 {
    text.split_whitespace()
        .map(|word| measure_inline_text_width(word, style))
        .fold(0.0f32, f32::max)
}
