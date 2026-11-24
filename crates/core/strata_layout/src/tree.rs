//! Layout tree node definition and top-level formatting context dispatcher.

use crate::geometry::{LayoutGeometry, Rect};
use std::cell::{Cell, RefCell};
use strata_style::types::{ComputedStyle, Display};

/// SVG stroke line cap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StrokeLineCap {
    #[default]
    Butt,
    Round,
    Square,
}

/// SVG stroke line join.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StrokeLineJoin {
    #[default]
    Miter,
    Round,
    Bevel,
}

/// SVG fill rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FillRule {
    #[default]
    NonZero,
    EvenOdd,
}

/// An SVG `<text>` or `<tspan>` text span positioned within an `<svg>` coordinate system.
#[derive(Debug, Clone, PartialEq)]
pub struct SvgTextData {
    pub content: String,
    pub x: f32,
    pub y: f32,
    pub font_size: f32,
    pub font_weight: strata_style::types::FontWeight,
    pub text_anchor: strata_style::TextAlign,
}

/// An individual SVG vector or text item within a layout node.
#[derive(Debug, Clone, PartialEq)]
pub struct SvgItem {
    pub path_data: String,
    pub text: Option<SvgTextData>,
    pub stroke_color: Option<strata_style::types::Color>,
    pub fill_color: Option<strata_style::types::Color>,
    pub stroke_is_current_color: bool,
    pub fill_is_current_color: bool,
    pub stroke_width: f32,
    pub stroke_linecap: StrokeLineCap,
    pub stroke_linejoin: StrokeLineJoin,
    pub stroke_dasharray: Vec<f32>,
    pub fill_rule: FillRule,
    pub view_box: (f32, f32),
}

impl SvgItem {
    pub fn new(path_data: String, view_box: (f32, f32)) -> Self {
        Self {
            path_data,
            text: None,
            stroke_color: None,
            fill_color: None,
            stroke_is_current_color: false,
            fill_is_current_color: false,
            stroke_width: 1.0,
            stroke_linecap: StrokeLineCap::Butt,
            stroke_linejoin: StrokeLineJoin::Miter,
            stroke_dasharray: Vec::new(),
            fill_rule: FillRule::NonZero,
            view_box,
        }
    }
}

use crate::typography::{InlineItem, LineBox};

/// An encoded raster image carried on a layout node.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageResource {
    pub data: std::sync::Arc<Vec<u8>>,
    pub width: u32,
    pub height: u32,
}

/// Computes a compact 64-bit hash of the sizing and overflow style fields that flexbox/grid
/// passes temporarily override or that affect layout box measurement.
#[inline]
pub(crate) fn layout_style_pin_hash(style: &ComputedStyle) -> u64 {
    use strata_style::types::Dimension;
    #[inline(always)]
    fn dim_bits(d: Dimension) -> u64 {
        match d {
            Dimension::Auto => 0x01,
            Dimension::Px(v) => 0x02 | ((v.to_bits() as u64) << 8),
            Dimension::Percent(v) => 0x03 | ((v.to_bits() as u64) << 8),
            Dimension::Vw(v) => 0x04 | ((v.to_bits() as u64) << 8),
            Dimension::Vh(v) => 0x05 | ((v.to_bits() as u64) << 8),
            Dimension::Rem(v) => 0x06 | ((v.to_bits() as u64) << 8),
            Dimension::Em(v) => 0x07 | ((v.to_bits() as u64) << 8),
            Dimension::Calc(id) => 0x08 | ((id.0 as u64) << 8),
            Dimension::MinContent => 0x09,
            Dimension::MaxContent => 0x0A,
            Dimension::FitContent(v) => 0x0B | ((v.to_bits() as u64) << 8),
        }
    }
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut mix = |v: u64| {
        h ^= v;
        h = h.wrapping_mul(0x100_0000_01b3);
    };
    mix(dim_bits(style.width));
    mix(dim_bits(style.height));
    mix(dim_bits(style.min_width));
    mix(dim_bits(style.max_width));
    mix(dim_bits(style.min_height));
    mix(dim_bits(style.max_height));
    mix(dim_bits(style.flex_basis));
    mix(style.flex_grow.to_bits() as u64);
    mix(style.flex_shrink.to_bits() as u64);
    mix(((style.overflow_x as u64) << 16)
        | ((style.overflow_y as u64) << 8)
        | ((style.white_space as u64) << 4)
        | (style.text_overflow as u64));
    h
}

/// Compact layout result cache for O(1) subtree re-entry during multi-pass formatting.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LayoutCacheEntry {
    pub available_w: f32,
    pub available_h: f32,
    pub result_w: f32,
    pub result_h: f32,
    pub origin_x: f32,
    pub origin_y: f32,
    pub style_pin_hash: u64,
    pub is_valid: bool,
}

impl LayoutCacheEntry {
    #[inline]
    pub const fn invalid() -> Self {
        Self {
            available_w: 0.0,
            available_h: 0.0,
            result_w: 0.0,
            result_h: 0.0,
            origin_x: 0.0,
            origin_y: 0.0,
            style_pin_hash: 0,
            is_valid: false,
        }
    }
}

/// Cached flexbox/grid measurement probe result (`(available_w, available_h, style_pin_hash) ->
/// (result_w, result_h, baseline_offset)`), allowing clean subtrees to skip Pass-1 / Pass-2
/// measurement traversals in O(1) without disturbing their final Pass-4 child geometry.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MeasureCacheEntry {
    pub available_w: f32,
    pub available_h: f32,
    pub style_pin_hash: u64,
    pub result_w: f32,
    pub result_h: f32,
    pub baseline_offset: f32,
    pub is_valid: bool,
}

impl MeasureCacheEntry {
    #[inline]
    pub const fn invalid() -> Self {
        Self {
            available_w: 0.0,
            available_h: 0.0,
            style_pin_hash: 0,
            result_w: 0.0,
            result_h: 0.0,
            baseline_offset: 0.0,
            is_valid: false,
        }
    }
}

/// Lock-free, 2-slot persistent intrinsic [`crate::intrinsic::MinMaxSizes`] cache stored inline
/// on every [`LayoutNode`].
///
/// Slot 0 caches `parent_size == None`, and Slot 1 caches the most recent `parent_size == Some(w)`.
/// Cleared whenever this node or any descendant is invalidated, eliminating full-tree text shaping
/// and intrinsic min/max re-traversal on unchanged reconciled subtrees across frames.
pub struct IntrinsicCacheCell {
    keys: [std::sync::atomic::AtomicU64; 2],
    vals: [std::sync::atomic::AtomicU64; 2],
}

impl IntrinsicCacheCell {
    #[inline]
    pub const fn empty() -> Self {
        Self {
            keys: [
                std::sync::atomic::AtomicU64::new(0),
                std::sync::atomic::AtomicU64::new(0),
            ],
            vals: [
                std::sync::atomic::AtomicU64::new(0),
                std::sync::atomic::AtomicU64::new(0),
            ],
        }
    }

    #[inline]
    fn encode_key(parent_size: Option<f32>) -> (usize, u64) {
        match parent_size {
            None => (0, 1),
            Some(w) => (1, 2u64.wrapping_add(w.to_bits() as u64)),
        }
    }

    #[inline]
    pub fn get(&self, parent_size: Option<f32>) -> Option<crate::intrinsic::MinMaxSizes> {
        let (slot, expected_key) = Self::encode_key(parent_size);
        let k = self.keys[slot].load(std::sync::atomic::Ordering::Relaxed);
        if k == expected_key {
            let v = self.vals[slot].load(std::sync::atomic::Ordering::Relaxed);
            let min_c = f32::from_bits(v as u32);
            let max_c = f32::from_bits((v >> 32) as u32);
            Some(crate::intrinsic::MinMaxSizes::new(min_c, max_c))
        } else {
            None
        }
    }

    #[inline]
    pub fn set(&self, parent_size: Option<f32>, sizes: crate::intrinsic::MinMaxSizes) {
        let (slot, key) = Self::encode_key(parent_size);
        let packed = (sizes.min_content.to_bits() as u64) | ((sizes.max_content.to_bits() as u64) << 32);
        self.vals[slot].store(packed, std::sync::atomic::Ordering::Relaxed);
        self.keys[slot].store(key, std::sync::atomic::Ordering::Relaxed);
    }

    #[inline]
    pub fn clear(&self) {
        self.keys[0].store(0, std::sync::atomic::Ordering::Relaxed);
        self.keys[1].store(0, std::sync::atomic::Ordering::Relaxed);
    }
}

impl Clone for IntrinsicCacheCell {
    fn clone(&self) -> Self {
        Self {
            keys: [
                std::sync::atomic::AtomicU64::new(
                    self.keys[0].load(std::sync::atomic::Ordering::Relaxed),
                ),
                std::sync::atomic::AtomicU64::new(
                    self.keys[1].load(std::sync::atomic::Ordering::Relaxed),
                ),
            ],
            vals: [
                std::sync::atomic::AtomicU64::new(
                    self.vals[0].load(std::sync::atomic::Ordering::Relaxed),
                ),
                std::sync::atomic::AtomicU64::new(
                    self.vals[1].load(std::sync::atomic::Ordering::Relaxed),
                ),
            ],
        }
    }
}

impl PartialEq for IntrinsicCacheCell {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl std::fmt::Debug for IntrinsicCacheCell {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IntrinsicCacheCell").finish_non_exhaustive()
    }
}

/// Maximum allowable call-stack recursion depth for layout tree formatting.
/// Bounded to 512 frames to guarantee that total call-stack consumption stays
/// safely bounded within default OS and test-runner stack budgets (2 MB) across all
/// platforms and unoptimized debug builds while supporting deep enterprise DOM trees.
pub const MAX_LAYOUT_DEPTH: usize = 512;

/// Lightweight Copy-context passed through layout formatting contexts to track recursion budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutDepthGuard {
    depth: usize,
    max_depth: usize,
}

impl LayoutDepthGuard {
    /// Constructs a root guard with default MAX_LAYOUT_DEPTH.
    #[inline]
    pub const fn root() -> Self {
        Self {
            depth: 0,
            max_depth: MAX_LAYOUT_DEPTH,
        }
    }

    /// Constructs a guard with a custom maximum depth limit (e.g. for testing).
    #[inline]
    pub const fn with_max_depth(max_depth: usize) -> Self {
        Self {
            depth: 0,
            max_depth,
        }
    }

    /// Advances the guard into a child node frame. Returns `None` if the depth budget is exhausted.
    #[inline]
    pub fn enter(&self) -> Option<Self> {
        if self.depth >= self.max_depth {
            None
        } else {
            Some(Self {
                depth: self.depth + 1,
                max_depth: self.max_depth,
            })
        }
    }

    #[inline]
    pub fn current_depth(&self) -> usize {
        self.depth
    }

    #[inline]
    pub fn max_depth(&self) -> usize {
        self.max_depth
    }

    #[inline]
    pub fn is_exhausted(&self) -> bool {
        self.depth >= self.max_depth
    }
}

type LayoutOverflowCallback = Box<dyn Fn(usize, usize) + Send + Sync>;

thread_local! {
    static LAYOUT_OVERFLOW_CALLBACK: RefCell<Option<LayoutOverflowCallback>> = const { RefCell::new(None) };
    static LAST_LAYOUT_OVERFLOW: Cell<Option<(usize, usize)>> = const { Cell::new(None) };
}

/// Registers an event callback triggered when layout depth exceeds MAX_LAYOUT_DEPTH.
pub fn set_on_layout_overflow<F: Fn(usize, usize) + Send + Sync + 'static>(cb: Option<F>) {
    LAYOUT_OVERFLOW_CALLBACK.with(|c| {
        *c.borrow_mut() = cb.map(|f| Box::new(f) as LayoutOverflowCallback);
    });
}

/// Returns the last recorded layout depth overflow as (depth, max_depth), if any.
pub fn last_layout_overflow() -> Option<(usize, usize)> {
    LAST_LAYOUT_OVERFLOW.with(|c| c.get())
}

/// Clears any recorded layout depth overflow.
pub fn clear_layout_overflow() {
    LAST_LAYOUT_OVERFLOW.with(|c| c.set(None));
}

/// Records a layout depth overflow occurrence, updates diagnostics, and fires the registered callback.
pub fn record_layout_overflow(depth: usize, max_depth: usize) {
    LAST_LAYOUT_OVERFLOW.with(|c| c.set(Some((depth, max_depth))));
    LAYOUT_OVERFLOW_CALLBACK.with(|cb| {
        if let Some(ref f) = *cb.borrow() {
            f(depth, max_depth);
        }
    });
}

/// Placement of an anchored top-layer portal or floating element relative to its anchor box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AnchorPlacement {
    Bottom,
    BottomStart,
    #[default]
    BottomEnd,
    Top,
    TopStart,
    TopEnd,
    Left,
    LeftStart,
    LeftEnd,
    Right,
    RightStart,
    RightEnd,
}

impl AnchorPlacement {
    /// Parses a CSS/Floating-UI placement string such as `"bottom-end"`, `"top-start"`, `"bottom"`, etc.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "bottom" | "block-end" => Some(Self::Bottom),
            "bottom-start" | "bottom_start" | "bottom-left" => Some(Self::BottomStart),
            "bottom-end" | "bottom_end" | "bottom-right" => Some(Self::BottomEnd),
            "top" | "block-start" => Some(Self::Top),
            "top-start" | "top_start" | "top-left" => Some(Self::TopStart),
            "top-end" | "top_end" | "top-right" => Some(Self::TopEnd),
            "left" | "inline-start" => Some(Self::Left),
            "left-start" | "left_start" | "left-top" => Some(Self::LeftStart),
            "left-end" | "left_end" | "left-bottom" => Some(Self::LeftEnd),
            "right" | "inline-end" => Some(Self::Right),
            "right-start" | "right_start" | "right-top" => Some(Self::RightStart),
            "right-end" | "right_end" | "right-bottom" => Some(Self::RightEnd),
            _ => None,
        }
    }

    /// Canonical kebab-case string representation.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bottom => "bottom",
            Self::BottomStart => "bottom-start",
            Self::BottomEnd => "bottom-end",
            Self::Top => "top",
            Self::TopStart => "top-start",
            Self::TopEnd => "top-end",
            Self::Left => "left",
            Self::LeftStart => "left-start",
            Self::LeftEnd => "left-end",
            Self::Right => "right",
            Self::RightStart => "right-start",
            Self::RightEnd => "right-end",
        }
    }

    /// Returns the opposite primary-axis placement preserving cross-axis alignment (`bottom-end <-> top-end`).
    pub const fn opposite(self) -> Self {
        match self {
            Self::Bottom => Self::Top,
            Self::BottomStart => Self::TopStart,
            Self::BottomEnd => Self::TopEnd,
            Self::Top => Self::Bottom,
            Self::TopStart => Self::BottomStart,
            Self::TopEnd => Self::BottomEnd,
            Self::Left => Self::Right,
            Self::LeftStart => Self::RightStart,
            Self::LeftEnd => Self::RightEnd,
            Self::Right => Self::Left,
            Self::RightStart => Self::LeftStart,
            Self::RightEnd => Self::LeftEnd,
        }
    }

    #[inline]
    fn raw_coords(
        self,
        anchor: crate::geometry::Rect,
        floating_w: f32,
        floating_h: f32,
        offset: f32,
    ) -> (f32, f32) {
        match self {
            Self::Bottom => (
                anchor.x + (anchor.width - floating_w) * 0.5,
                anchor.y + anchor.height + offset,
            ),
            Self::BottomStart => (anchor.x, anchor.y + anchor.height + offset),
            Self::BottomEnd => (
                anchor.x + anchor.width - floating_w,
                anchor.y + anchor.height + offset,
            ),
            Self::Top => (
                anchor.x + (anchor.width - floating_w) * 0.5,
                anchor.y - floating_h - offset,
            ),
            Self::TopStart => (anchor.x, anchor.y - floating_h - offset),
            Self::TopEnd => (
                anchor.x + anchor.width - floating_w,
                anchor.y - floating_h - offset,
            ),
            Self::Left => (
                anchor.x - floating_w - offset,
                anchor.y + (anchor.height - floating_h) * 0.5,
            ),
            Self::LeftStart => (anchor.x - floating_w - offset, anchor.y),
            Self::LeftEnd => (
                anchor.x - floating_w - offset,
                anchor.y + anchor.height - floating_h,
            ),
            Self::Right => (
                anchor.x + anchor.width + offset,
                anchor.y + (anchor.height - floating_h) * 0.5,
            ),
            Self::RightStart => (anchor.x + anchor.width + offset, anchor.y),
            Self::RightEnd => (
                anchor.x + anchor.width + offset,
                anchor.y + anchor.height - floating_h,
            ),
        }
    }
}

/// Computes the viewport-aware anchored `(x, y, resolved_placement)` for a floating/top-layer element,
/// performing automatic primary-axis collision flipping (`bottom-end <-> top-end`, `left <-> right`)
/// and cross-axis viewport shift clamping.
#[allow(clippy::too_many_arguments)]
pub fn solve_anchored_position(
    anchor: crate::geometry::Rect,
    floating_w: f32,
    floating_h: f32,
    placement: AnchorPlacement,
    offset: f32,
    flip: bool,
    shift: bool,
    padding: f32,
    viewport: crate::geometry::Rect,
) -> (f32, f32, AnchorPlacement) {
    let min_x = viewport.x + padding;
    let min_y = viewport.y + padding;
    let max_x = (viewport.x + viewport.width - padding).max(min_x);
    let max_y = (viewport.y + viewport.height - padding).max(min_y);

    let axis_score = |cand: AnchorPlacement, cx: f32, cy: f32| -> f32 {
        let left_ov = (min_x - cx).max(0.0);
        let right_ov = (cx + floating_w - max_x).max(0.0);
        let top_ov = (min_y - cy).max(0.0);
        let bottom_ov = (cy + floating_h - max_y).max(0.0);
        if !shift {
            return left_ov + right_ov + top_ov + bottom_ov;
        }
        // When shift is enabled, cross-axis overflow (e.g. horizontal clamp on Bottom*) is resolved
        // cleanly by viewport clamping unless another primary/cardinal side fits with zero overflow.
        let cross_fits = left_ov <= 0.001 && right_ov <= 0.001;
        let vert_fits = top_ov <= 0.001 && bottom_ov <= 0.001;
        match cand {
            AnchorPlacement::Bottom
            | AnchorPlacement::BottomStart
            | AnchorPlacement::BottomEnd
            | AnchorPlacement::Top
            | AnchorPlacement::TopStart
            | AnchorPlacement::TopEnd => {
                let main_ov = top_ov + bottom_ov;
                if main_ov <= 0.001 && cross_fits {
                    0.0
                } else {
                    main_ov
                }
            }
            AnchorPlacement::Left
            | AnchorPlacement::LeftStart
            | AnchorPlacement::LeftEnd
            | AnchorPlacement::Right
            | AnchorPlacement::RightStart
            | AnchorPlacement::RightEnd => {
                let main_ov = left_ov + right_ov;
                if main_ov <= 0.001 && vert_fits {
                    0.0
                } else {
                    main_ov + (top_ov + bottom_ov) * 0.25
                }
            }
        }
    };

    let candidates: [AnchorPlacement; 6] = if shift {
        match placement {
            AnchorPlacement::BottomStart => [
                AnchorPlacement::BottomStart,
                AnchorPlacement::TopStart,
                AnchorPlacement::RightStart,
                AnchorPlacement::LeftStart,
                AnchorPlacement::BottomEnd,
                AnchorPlacement::TopEnd,
            ],
            AnchorPlacement::BottomEnd => [
                AnchorPlacement::BottomEnd,
                AnchorPlacement::TopEnd,
                AnchorPlacement::LeftEnd,
                AnchorPlacement::RightEnd,
                AnchorPlacement::BottomStart,
                AnchorPlacement::TopStart,
            ],
            AnchorPlacement::Bottom => [
                AnchorPlacement::Bottom,
                AnchorPlacement::Top,
                AnchorPlacement::Right,
                AnchorPlacement::Left,
                AnchorPlacement::BottomStart,
                AnchorPlacement::BottomEnd,
            ],
            AnchorPlacement::TopStart => [
                AnchorPlacement::TopStart,
                AnchorPlacement::BottomStart,
                AnchorPlacement::RightStart,
                AnchorPlacement::LeftStart,
                AnchorPlacement::TopEnd,
                AnchorPlacement::BottomEnd,
            ],
            AnchorPlacement::TopEnd => [
                AnchorPlacement::TopEnd,
                AnchorPlacement::BottomEnd,
                AnchorPlacement::LeftEnd,
                AnchorPlacement::RightEnd,
                AnchorPlacement::TopStart,
                AnchorPlacement::BottomStart,
            ],
            AnchorPlacement::Top => [
                AnchorPlacement::Top,
                AnchorPlacement::Bottom,
                AnchorPlacement::Right,
                AnchorPlacement::Left,
                AnchorPlacement::TopStart,
                AnchorPlacement::TopEnd,
            ],
            AnchorPlacement::RightStart => [
                AnchorPlacement::RightStart,
                AnchorPlacement::LeftStart,
                AnchorPlacement::BottomStart,
                AnchorPlacement::TopStart,
                AnchorPlacement::RightEnd,
                AnchorPlacement::LeftEnd,
            ],
            AnchorPlacement::RightEnd => [
                AnchorPlacement::RightEnd,
                AnchorPlacement::LeftEnd,
                AnchorPlacement::BottomEnd,
                AnchorPlacement::TopEnd,
                AnchorPlacement::RightStart,
                AnchorPlacement::LeftStart,
            ],
            AnchorPlacement::Right => [
                AnchorPlacement::Right,
                AnchorPlacement::Left,
                AnchorPlacement::Bottom,
                AnchorPlacement::Top,
                AnchorPlacement::RightStart,
                AnchorPlacement::RightEnd,
            ],
            AnchorPlacement::LeftStart => [
                AnchorPlacement::LeftStart,
                AnchorPlacement::RightStart,
                AnchorPlacement::BottomStart,
                AnchorPlacement::TopStart,
                AnchorPlacement::LeftEnd,
                AnchorPlacement::RightEnd,
            ],
            AnchorPlacement::LeftEnd => [
                AnchorPlacement::LeftEnd,
                AnchorPlacement::RightEnd,
                AnchorPlacement::BottomEnd,
                AnchorPlacement::TopEnd,
                AnchorPlacement::LeftStart,
                AnchorPlacement::RightStart,
            ],
            AnchorPlacement::Left => [
                AnchorPlacement::Left,
                AnchorPlacement::Right,
                AnchorPlacement::Bottom,
                AnchorPlacement::Top,
                AnchorPlacement::LeftStart,
                AnchorPlacement::LeftEnd,
            ],
        }
    } else {
        match placement {
            AnchorPlacement::BottomStart => [
                AnchorPlacement::BottomStart,
                AnchorPlacement::TopStart,
                AnchorPlacement::BottomEnd,
                AnchorPlacement::TopEnd,
                AnchorPlacement::RightStart,
                AnchorPlacement::LeftStart,
            ],
            AnchorPlacement::BottomEnd => [
                AnchorPlacement::BottomEnd,
                AnchorPlacement::TopEnd,
                AnchorPlacement::BottomStart,
                AnchorPlacement::TopStart,
                AnchorPlacement::LeftEnd,
                AnchorPlacement::RightEnd,
            ],
            AnchorPlacement::Bottom => [
                AnchorPlacement::Bottom,
                AnchorPlacement::Top,
                AnchorPlacement::BottomStart,
                AnchorPlacement::BottomEnd,
                AnchorPlacement::Right,
                AnchorPlacement::Left,
            ],
            AnchorPlacement::TopStart => [
                AnchorPlacement::TopStart,
                AnchorPlacement::BottomStart,
                AnchorPlacement::TopEnd,
                AnchorPlacement::BottomEnd,
                AnchorPlacement::RightStart,
                AnchorPlacement::LeftStart,
            ],
            AnchorPlacement::TopEnd => [
                AnchorPlacement::TopEnd,
                AnchorPlacement::BottomEnd,
                AnchorPlacement::TopStart,
                AnchorPlacement::BottomStart,
                AnchorPlacement::LeftEnd,
                AnchorPlacement::RightEnd,
            ],
            AnchorPlacement::Top => [
                AnchorPlacement::Top,
                AnchorPlacement::Bottom,
                AnchorPlacement::TopStart,
                AnchorPlacement::TopEnd,
                AnchorPlacement::Right,
                AnchorPlacement::Left,
            ],
            AnchorPlacement::RightStart => [
                AnchorPlacement::RightStart,
                AnchorPlacement::LeftStart,
                AnchorPlacement::RightEnd,
                AnchorPlacement::LeftEnd,
                AnchorPlacement::BottomStart,
                AnchorPlacement::TopStart,
            ],
            AnchorPlacement::RightEnd => [
                AnchorPlacement::RightEnd,
                AnchorPlacement::LeftEnd,
                AnchorPlacement::RightStart,
                AnchorPlacement::LeftStart,
                AnchorPlacement::BottomEnd,
                AnchorPlacement::TopEnd,
            ],
            AnchorPlacement::Right => [
                AnchorPlacement::Right,
                AnchorPlacement::Left,
                AnchorPlacement::RightStart,
                AnchorPlacement::RightEnd,
                AnchorPlacement::Bottom,
                AnchorPlacement::Top,
            ],
            AnchorPlacement::LeftStart => [
                AnchorPlacement::LeftStart,
                AnchorPlacement::RightStart,
                AnchorPlacement::LeftEnd,
                AnchorPlacement::RightEnd,
                AnchorPlacement::BottomStart,
                AnchorPlacement::TopStart,
            ],
            AnchorPlacement::LeftEnd => [
                AnchorPlacement::LeftEnd,
                AnchorPlacement::RightEnd,
                AnchorPlacement::LeftStart,
                AnchorPlacement::RightStart,
                AnchorPlacement::BottomEnd,
                AnchorPlacement::TopEnd,
            ],
            AnchorPlacement::Left => [
                AnchorPlacement::Left,
                AnchorPlacement::Right,
                AnchorPlacement::LeftStart,
                AnchorPlacement::LeftEnd,
                AnchorPlacement::Bottom,
                AnchorPlacement::Top,
            ],
        }
    };

    let mut chosen = placement;
    let (mut x, mut y) = chosen.raw_coords(anchor, floating_w, floating_h, offset);

    if flip {
        let eval_len = if shift { 4 } else { candidates.len() };
        let mut best_score = axis_score(chosen, x, y);
        if best_score > 0.001 {
            for &cand in &candidates[1..eval_len] {
                let (cx, cy) = cand.raw_coords(anchor, floating_w, floating_h, offset);
                let score = axis_score(cand, cx, cy);
                if score + 0.001 < best_score {
                    best_score = score;
                    chosen = cand;
                    x = cx;
                    y = cy;
                    if best_score <= 0.001 {
                        break;
                    }
                }
            }
        }
    }

    if shift {
        let upper_x = (max_x - floating_w).max(min_x);
        let upper_y = (max_y - floating_h).max(min_y);
        x = x.clamp(min_x, upper_x);
        y = y.clamp(min_y, upper_y);
    }

    (x, y, chosen)
}

/// Configuration attaching a top-layer portal or CSS anchor-positioned box to a trigger element.
#[derive(Debug, Clone, PartialEq)]
pub struct PortalAnchorSpec {
    pub anchor_id: String,
    pub placement: AnchorPlacement,
    pub offset: f32,
    pub flip: bool,
    pub shift: bool,
    pub viewport_padding: f32,
    pub match_width: bool,
    pub min_width_from_anchor: bool,
}

impl PortalAnchorSpec {
    pub fn new(anchor_id: impl Into<String>, placement: AnchorPlacement) -> Self {
        Self {
            anchor_id: anchor_id.into(),
            placement,
            offset: 4.0,
            flip: true,
            shift: true,
            viewport_padding: 8.0,
            match_width: false,
            min_width_from_anchor: false,
        }
    }
}

/// Thread-safe type-erased handle to an embedded 3D scene (`Arc<Scene3D>`) carried on a layout node
/// so the 2D CSS compositor can emit `DrawViewport3D` at the exact stacking-context paint order.
#[derive(Clone)]
pub struct ViewportSceneHandle(pub std::sync::Arc<dyn std::any::Any + Send + Sync>);

impl std::fmt::Debug for ViewportSceneHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("ViewportSceneHandle")
            .field(&"<scene_3d>")
            .finish()
    }
}

impl PartialEq for ViewportSceneHandle {
    fn eq(&self, other: &Self) -> bool {
        std::sync::Arc::ptr_eq(&self.0, &other.0)
    }
}

/// Subtree hit-testing bitflags stored on [`LayoutNode::subtree_hit_flags`].
pub const SUBTREE_HAS_TOP_LAYER: u8 = 1 << 0;
pub const SUBTREE_HAS_HOISTED: u8 = 1 << 1;
pub const SUBTREE_HAS_TRANSFORM: u8 = 1 << 2;
pub const SUBTREE_META_VALID: u8 = 1 << 3;
/// Cached summary bit indicating whether any descendant (up to an explicit pixel-height boundary)
/// depends on parent height (`has_height_dependent_descendant`).
pub const SUBTREE_HAS_HEIGHT_DEP_DESCENDANT: u8 = 1 << 4;

/// A node participating in the layout tree.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutNode {
    pub style: ComputedStyle,
    pub geometry: LayoutGeometry,
    pub children: Vec<LayoutNode>,
    pub text: Option<String>,
    pub svg_path: Option<String>,
    pub svg_items: Vec<SvgItem>,
    pub image: Option<ImageResource>,
    pub scene_3d: Option<ViewportSceneHandle>,
    pub inline_items: Vec<InlineItem>,
    pub line_boxes: Vec<LineBox>,
    pub layout_cache: LayoutCacheEntry,
    /// Two-slot measurement probe cache used by Flexbox Pass 1 & Pass 2 to skip clean subtrees
    /// in O(1) without clobbering their final Pass-4 child geometry.
    pub measure_cache: [MeasureCacheEntry; 2],
    /// Persistent per-node intrinsic min/max sizing cache cleared only on subtree invalidation.
    pub intrinsic_cache: IntrinsicCacheCell,
    /// Content-box size used the last time this node formatted its descendants.
    pub last_child_layout_content: Option<(f32, f32)>,
    /// True when this node's descendants have been laid out for the current style/tree state and
    /// neither `self` nor any descendant has been invalidated since.
    pub children_layout_clean: bool,
    /// Last preserved box parameters `(x, y, border_w, border_h, resolve_positioned)` formatted by
    /// `layout_children_inner_guarded`, enabling O(1) skip + translate when unchanged.
    pub last_preserved_box: Option<(f32, f32, f32, f32, bool)>,
    /// Table border grid matrix for collapsed border rendering (CSS 2.1 §17.6.2).
    pub table_grid: Option<crate::table::TableBorderGrid>,
    /// Unique 64-bit entity identifier associating this layout node with its blueprint element.
    pub entity_id: strata_primitives::EntityId,
    /// True when one or more CSS `transition-*` interpolations are currently in-flight on this node.
    pub active_transitions: bool,
    /// DOM `id` or `data-anchor-id` attribute used for live anchor lookup.
    pub element_id: Option<String>,
    /// CSS `anchor-name` (e.g. `--card-menu-trigger`) declared on this element.
    pub anchor_name: Option<String>,
    /// True when this element renders in the root Top Layer (`<Portal>` / `create_portal`),
    /// escaping all ancestor `overflow`, `transform`, `filter`, and stacking-context boundaries.
    pub is_top_layer: bool,
    /// Live anchor positioning specification resolved against post-scroll trigger bounds.
    pub portal_anchor: Option<Box<PortalAnchorSpec>>,
    /// Bitflags summarizing descendants (`SUBTREE_HAS_*`, `SUBTREE_META_VALID`).
    pub subtree_hit_flags: u8,
    /// Conservative axis-aligned bounding box covering this node and in-flow descendants.
    pub subtree_bounds: Rect,
    /// Expanded border-box width locked during collapse/expand transitions so children avoid 0px squeeze.
    pub transition_layout_width_px: Option<f32>,
    /// Expanded border-box height locked during collapse/expand transitions so children avoid 0px squeeze.
    pub transition_layout_height_px: Option<f32>,
}

impl LayoutNode {
    /// Constructs a leaf layout node with specified computed styles.
    pub fn new(style: ComputedStyle) -> Self {
        Self {
            style,
            geometry: LayoutGeometry::default(),
            children: Vec::new(),
            text: None,
            svg_path: None,
            svg_items: Vec::new(),
            image: None,
            scene_3d: None,
            inline_items: Vec::new(),
            line_boxes: Vec::new(),
            layout_cache: LayoutCacheEntry::invalid(),
            measure_cache: [MeasureCacheEntry::invalid(); 2],
            intrinsic_cache: IntrinsicCacheCell::empty(),
            last_child_layout_content: None,
            children_layout_clean: false,
            last_preserved_box: None,
            table_grid: None,
            entity_id: strata_primitives::EntityId::NONE,
            active_transitions: false,
            element_id: None,
            anchor_name: None,
            is_top_layer: false,
            portal_anchor: None,
            subtree_hit_flags: 0,
            subtree_bounds: Rect::new(0.0, 0.0, 0.0, 0.0),
            transition_layout_width_px: None,
            transition_layout_height_px: None,
        }
    }

    /// Constructs a layout node with child nodes.
    pub fn with_children(style: ComputedStyle, children: Vec<LayoutNode>) -> Self {
        let mut node = Self::new(style);
        node.children = children;
        node
    }

    /// Constructs a text leaf node with computed styles and string content.
    pub fn with_text(style: ComputedStyle, text: String) -> Self {
        let mut node = Self::new(style);
        node.text = Some(text);
        node
    }

    /// Constructs an inline phrasing container with an inline item stream.
    pub fn with_inline_items(style: ComputedStyle, inline_items: Vec<InlineItem>) -> Self {
        let mut node = Self::new(style);
        node.inline_items = inline_items;
        node
    }

    /// Constructs an SVG vector path leaf node with computed styles and path string.
    pub fn with_svg_path(style: ComputedStyle, svg_path: String) -> Self {
        let mut node = Self::new(style);
        node.svg_path = Some(svg_path.clone());
        node.svg_items = vec![SvgItem {
            path_data: svg_path,
            stroke_color: None,
            fill_color: None,
            stroke_is_current_color: true,
            fill_is_current_color: false,
            stroke_width: 2.0,
            stroke_linecap: StrokeLineCap::Round,
            stroke_linejoin: StrokeLineJoin::Round,
            stroke_dasharray: Vec::new(),
            fill_rule: FillRule::NonZero,
            view_box: (24.0, 24.0),
            text: None,
        }];
        node
    }

    /// Constructs an SVG layout node with multiple styled vector items and viewBox.
    pub fn with_svg_items(style: ComputedStyle, svg_items: Vec<SvgItem>) -> Self {
        let first_path = svg_items.first().map(|i| i.path_data.clone());
        let mut node = Self::new(style);
        node.svg_path = first_path;
        node.svg_items = svg_items;
        node
    }

    /// Constructs a replaced raster image leaf node with intrinsic dimensions.
    pub fn with_image(style: ComputedStyle, image: ImageResource) -> Self {
        let mut node = Self::new(style);
        node.image = Some(image);
        node
    }

    /// Returns `true` if this node or any of its descendants currently has an active CSS transition.
    pub fn has_active_transitions(&self) -> bool {
        if self.active_transitions {
            return true;
        }
        self.children.iter().any(|c| c.has_active_transitions())
    }

    /// Inner content-box width override used during collapse/expand transitions so children
    /// keep their expanded layout width without squeezing to zero mid-animation.
    #[inline]
    pub fn transition_content_width(&self) -> Option<f32> {
        let inset_h = self.style.padding.left
            + self.style.padding.right
            + self.style.border_width.left
            + self.style.border_width.right;
        self.transition_layout_width_px
            .map(|w| (w - inset_h).max(0.0))
    }

    /// Inner content-box height override used during collapse/expand transitions.
    #[inline]
    pub fn transition_content_height(&self) -> Option<f32> {
        let inset_v = self.style.padding.top
            + self.style.padding.bottom
            + self.style.border_width.top
            + self.style.border_width.bottom;
        self.transition_layout_height_px
            .map(|h| (h - inset_v).max(0.0))
    }

    /// Attaches a 3D viewport scene handle to this layout node.
    pub fn with_scene_3d(mut self, handle: ViewportSceneHandle) -> Self {
        self.scene_3d = Some(handle);
        self
    }

    /// Mutably sets or clears the 3D viewport scene handle on this layout node.
    pub fn set_scene_3d(&mut self, handle: Option<ViewportSceneHandle>) {
        self.scene_3d = handle;
    }

    /// Sets the EntityId on this layout node.
    pub fn with_entity_id(mut self, entity_id: strata_primitives::EntityId) -> Self {
        self.entity_id = entity_id;
        self
    }

    /// Mutably assigns the EntityId on this layout node.
    pub fn set_entity_id(&mut self, entity_id: strata_primitives::EntityId) {
        self.entity_id = entity_id;
    }

    /// Recursively locates a layout node by its EntityId.
    pub fn find_by_entity_id(&self, target_id: strata_primitives::EntityId) -> Option<&LayoutNode> {
        if !target_id.is_valid() {
            return None;
        }
        if self.entity_id == target_id {
            return Some(self);
        }
        for child in &self.children {
            if let Some(found) = child.find_by_entity_id(target_id) {
                return Some(found);
            }
        }
        None
    }

    /// Recursively locates a mutable layout node by its EntityId.
    pub fn find_by_entity_id_mut(
        &mut self,
        target_id: strata_primitives::EntityId,
    ) -> Option<&mut LayoutNode> {
        if !target_id.is_valid() {
            return None;
        }
        if self.entity_id == target_id {
            return Some(self);
        }
        for child in &mut self.children {
            if let Some(found) = child.find_by_entity_id_mut(target_id) {
                return Some(found);
            }
        }
        None
    }

    /// Appends a child layout node.
    pub fn add_child(&mut self, child: LayoutNode) {
        self.children.push(child);
    }

    /// Evaluates if this layout node establishes an atomic CSS stacking context within `parent_display`.
    #[inline]
    pub fn establishes_stacking_context_in(
        &self,
        parent_display: Option<Display>,
        is_root: bool,
    ) -> bool {
        self.style
            .establishes_stacking_context_in(parent_display, is_root)
    }

    /// Translates this node's geometry and all descendants by (dx, dy) using an iterative heap stack (O(1) call stack).
    pub fn translate(&mut self, dx: f32, dy: f32) {
        if dx == 0.0 && dy == 0.0 {
            return;
        }

        let mut stack: Vec<&mut LayoutNode> = vec![self];
        while let Some(node) = stack.pop() {
            node.geometry.translate(dx, dy);
            node.subtree_bounds.x += dx;
            node.subtree_bounds.y += dy;
            node.layout_cache.origin_x += dx;
            node.layout_cache.origin_y += dy;
            if let Some(ref mut pb) = node.last_preserved_box {
                pb.0 += dx;
                pb.1 += dy;
            }
            for line in &mut node.line_boxes {
                line.bounds.x += dx;
                line.bounds.y += dy;
                for frag in &mut line.text_fragments {
                    frag.bounds.x += dx;
                    frag.bounds.y += dy;
                    frag.baseline_y += dy;
                }
                for atomic in &mut line.atomic_fragments {
                    atomic.bounds.x += dx;
                    atomic.bounds.y += dy;
                }
            }
            if let Some(ref mut grid) = node.table_grid {
                grid.translate(dx, dy);
            }
            for child in &mut node.children {
                stack.push(child);
            }
        }
    }

    /// Translates descendants of a scroll container by `(dx, dy)` while honoring CSS containing-block
    /// escape semantics:
    /// - `Position::Fixed` subtrees do not scroll unless an ancestor at or inside the scroll container
    ///   established a fixed containing block (`transform`, `filter`, `backdrop-filter`).
    /// - `Position::Absolute` subtrees do not scroll unless an ancestor at or inside the scroll container
    ///   established an absolute containing block (`position != Static` or fixed CB).
    pub fn translate_scrolled(
        &mut self,
        dx: f32,
        dy: f32,
        parent_is_abs_cb: bool,
        parent_is_fixed_cb: bool,
    ) {
        if dx == 0.0 && dy == 0.0 {
            return;
        }

        let mut stack: Vec<(&mut LayoutNode, bool, bool)> =
            vec![(self, parent_is_abs_cb, parent_is_fixed_cb)];
        while let Some((node, abs_cb_inside, fixed_cb_inside)) = stack.pop() {
            if node.is_top_layer {
                continue;
            }
            match node.style.position {
                strata_style::types::Position::Fixed if !fixed_cb_inside => {
                    continue;
                }
                strata_style::types::Position::Absolute if !abs_cb_inside => {
                    continue;
                }
                _ => {}
            }

            node.geometry.translate(dx, dy);
            node.subtree_bounds.x += dx;
            node.subtree_bounds.y += dy;
            node.layout_cache.origin_x += dx;
            node.layout_cache.origin_y += dy;
            if let Some(ref mut pb) = node.last_preserved_box {
                pb.0 += dx;
                pb.1 += dy;
            }
            for line in &mut node.line_boxes {
                line.bounds.x += dx;
                line.bounds.y += dy;
                for frag in &mut line.text_fragments {
                    frag.bounds.x += dx;
                    frag.bounds.y += dy;
                    frag.baseline_y += dy;
                }
                for atomic in &mut line.atomic_fragments {
                    atomic.bounds.x += dx;
                    atomic.bounds.y += dy;
                }
            }
            if let Some(ref mut grid) = node.table_grid {
                grid.translate(dx, dy);
            }

            let child_fixed_cb = fixed_cb_inside || node.style.establishes_fixed_containing_block();
            let child_abs_cb = abs_cb_inside
                || node.style.position != strata_style::types::Position::Static
                || child_fixed_cb;

            for child in &mut node.children {
                stack.push((child, child_abs_cb, child_fixed_cb));
            }
        }
    }

    /// Formats the children of this node within its already-resolved content box.
    pub fn layout_children(&mut self) {
        self.layout_children_guarded(LayoutDepthGuard::root());
    }

    /// Formats the children of this node with an active depth guard.
    pub fn layout_children_guarded(&mut self, guard: LayoutDepthGuard) {
        self.layout_children_inner_guarded(guard, true);
    }

    pub(crate) fn layout_children_inner_guarded(
        &mut self,
        guard: LayoutDepthGuard,
        resolve_positioned: bool,
    ) {
        if (self.children.is_empty() && self.inline_items.is_empty()) || guard.is_exhausted() {
            return;
        }

        let content_w = self.geometry.content_box.width;
        let content_h = self.geometry.content_box.height;
        let preserved_geom = self.geometry;

        // O(1) clean subtree fast path: when neither `self` nor any descendant was invalidated
        // since the last layout and the preserved box + content dimensions match, simply translate
        // descendants if the box origin moved instead of re-solving the entire subtree.
        if self.children_layout_clean {
            if let Some((prev_x, prev_y, prev_w, prev_h, prev_pos)) = self.last_preserved_box {
                if prev_pos == resolve_positioned
                    && (prev_w - preserved_geom.border_box.width).abs() < 0.05
                    && (prev_h - preserved_geom.border_box.height).abs() < 0.05
                    && self.last_child_layout_content.is_some_and(|(cw, ch)| {
                        (cw - content_w).abs() < 0.05 && (ch - content_h).abs() < 0.05
                    })
                {
                    let dx = preserved_geom.border_box.x - prev_x;
                    let dy = preserved_geom.border_box.y - prev_y;
                    if dx.abs() > 0.001 || dy.abs() > 0.001 {
                        self.subtree_bounds.x += dx;
                        self.subtree_bounds.y += dy;
                        for line in &mut self.line_boxes {
                            line.bounds.x += dx;
                            line.bounds.y += dy;
                            for frag in &mut line.text_fragments {
                                frag.bounds.x += dx;
                                frag.bounds.y += dy;
                                frag.baseline_y += dy;
                            }
                            for at in &mut line.atomic_fragments {
                                at.bounds.x += dx;
                                at.bounds.y += dy;
                            }
                        }
                        for child in &mut self.children {
                            child.translate(dx, dy);
                        }
                        self.last_preserved_box = Some((
                            preserved_geom.border_box.x,
                            preserved_geom.border_box.y,
                            prev_w,
                            prev_h,
                            prev_pos,
                        ));
                    }
                    return;
                }
            }
        }

        self.subtree_hit_flags &= !SUBTREE_META_VALID;

        let saved_w = self.style.width;
        let saved_h = self.style.height;
        let saved_min_w = self.style.min_width;
        let saved_max_w = self.style.max_width;
        let saved_min_h = self.style.min_height;
        let saved_max_h = self.style.max_height;

        let pin_w = preserved_geom.border_box.width > 0.0
            || matches!(saved_w, strata_style::types::Dimension::Percent(_));
        let pin_h = preserved_geom.border_box.height > 0.0
            || matches!(saved_h, strata_style::types::Dimension::Percent(_));

        let container_avail_w = if pin_w {
            self.style.width = strata_style::types::Dimension::Px(preserved_geom.border_box.width);
            self.style.min_width = strata_style::types::Dimension::Auto;
            self.style.max_width = strata_style::types::Dimension::Auto;
            preserved_geom.border_box.width
        } else {
            preserved_geom.border_box.width + self.style.margin.left + self.style.margin.right
        };

        let container_avail_h = if pin_h {
            let pinned_h = if saved_h == strata_style::types::Dimension::Auto {
                crate::box_model::clamp_dimension_with_node(
                    preserved_geom.border_box.height,
                    saved_min_h,
                    saved_max_h,
                    content_h.max(preserved_geom.border_box.height),
                    self,
                )
            } else {
                preserved_geom.border_box.height
            };
            self.style.height = strata_style::types::Dimension::Px(pinned_h);
            self.style.min_height = strata_style::types::Dimension::Auto;
            self.style.max_height = strata_style::types::Dimension::Auto;
            pinned_h
        } else {
            preserved_geom.border_box.height + self.style.margin.top + self.style.margin.bottom
        };

        match self.style.display {
            Display::Flex | Display::InlineFlex => {
                let definite_cross = if content_h > 0.0 && !pin_h {
                    Some(content_h)
                } else {
                    None
                };
                crate::flexbox::solve_flexbox_with_definite_cross_guarded(
                    self,
                    container_avail_w,
                    container_avail_h,
                    definite_cross,
                    guard,
                );
            }
            Display::Grid | Display::InlineGrid => {
                crate::grid::solve_grid_guarded(self, container_avail_w, container_avail_h, guard);
            }
            Display::Table | Display::InlineTable => {
                crate::table::solve_table_guarded(
                    self,
                    container_avail_w,
                    container_avail_h,
                    guard,
                );
            }
            Display::Block
            | Display::ListItem
            | Display::Inline
            | Display::InlineBlock
            | Display::TableRowGroup
            | Display::TableHeaderGroup
            | Display::TableFooterGroup
            | Display::TableRow
            | Display::TableCell
            | Display::TableCaption
            | Display::TableColumnGroup
            | Display::TableColumn => {
                let block_h = crate::box_model::solve_block_children_with_exclusions_guarded(
                    self, content_w, content_h, None, 0.0, guard,
                );
                if saved_h == strata_style::types::Dimension::Auto {
                    self.geometry.border_box.height = block_h;
                }
            }
            Display::None => {}
        }

        self.style.width = saved_w;
        self.style.height = saved_h;
        self.style.min_width = saved_min_w;
        self.style.max_width = saved_max_w;
        self.style.min_height = saved_min_h;
        self.style.max_height = saved_max_h;

        let clamp_avail_h = preserved_geom.border_box.height.max(content_h);
        if self.style.height == strata_style::types::Dimension::Auto {
            self.geometry.border_box.height = crate::box_model::clamp_dimension_with_node(
                self.geometry.border_box.height,
                saved_min_h,
                saved_max_h,
                clamp_avail_h,
                self,
            );
        }

        let clips_y = self.style.overflow_y != strata_style::types::Overflow::Visible;
        let final_h = if self.style.height == strata_style::types::Dimension::Auto && !clips_y {
            let unclipped = self
                .geometry
                .border_box
                .height
                .max(preserved_geom.border_box.height);
            crate::box_model::clamp_dimension_with_node(
                unclipped,
                saved_min_h,
                saved_max_h,
                clamp_avail_h,
                self,
            )
        } else if preserved_geom.border_box.height > 0.0 {
            if self.style.height == strata_style::types::Dimension::Auto {
                crate::box_model::clamp_dimension_with_node(
                    preserved_geom.border_box.height,
                    saved_min_h,
                    saved_max_h,
                    clamp_avail_h,
                    self,
                )
            } else {
                preserved_geom.border_box.height
            }
        } else {
            self.geometry.border_box.height
        };

        let clips_x = self.style.overflow_x != strata_style::types::Overflow::Visible;
        let final_w = if self.style.width == strata_style::types::Dimension::Auto && !clips_x {
            self.geometry
                .border_box
                .width
                .max(preserved_geom.border_box.width)
        } else if preserved_geom.border_box.width > 0.0 {
            preserved_geom.border_box.width
        } else {
            self.geometry.border_box.width
        };

        self.geometry = LayoutGeometry::from_border_box(
            crate::geometry::Rect::new(
                preserved_geom.border_box.x,
                preserved_geom.border_box.y,
                final_w,
                final_h,
            ),
            self.style.padding,
            self.style.border_width,
            self.style.margin,
        );

        if resolve_positioned && self.style.position != strata_style::types::Position::Static {
            crate::positioning::solve_positioning_guarded(self, content_w, content_h, guard);
        }
        self.geometry.snap_to_pixel();
        self.update_scroll_metrics();
        self.last_child_layout_content = Some((content_w, content_h));
        self.children_layout_clean = true;
        self.last_preserved_box = Some((
            self.geometry.border_box.x,
            self.geometry.border_box.y,
            self.geometry.border_box.width,
            self.geometry.border_box.height,
            resolve_positioned,
        ));
        self.layout_cache.is_valid = false;
    }

    /// Computes layout geometry for this node and all descendants given available bounds.
    pub fn compute_layout(&mut self, available_width: f32, available_height: f32) {
        let session_root = crate::intrinsic::begin_layout_session();
        let _guard = crate::prof::LayoutGuard::new();
        let guard = LayoutDepthGuard::root();
        self.compute_layout_guarded(available_width, available_height, None, 0.0, guard);
        crate::intrinsic::end_layout_session(session_root);
        self.refresh_subtree_hit_metadata();
    }

    /// Computes layout geometry passing an active exclusion space for float obstructions.
    pub fn compute_layout_with_exclusions(
        &mut self,
        available_width: f32,
        available_height: f32,
        exclusions: Option<&mut crate::exclusions::ExclusionSpace>,
        bfc_offset_y: f32,
    ) {
        let guard = LayoutDepthGuard::root();
        self.compute_layout_guarded(
            available_width,
            available_height,
            exclusions,
            bfc_offset_y,
            guard,
        );
        self.refresh_subtree_hit_metadata();
    }

    /// Recomputes `subtree_hit_flags` and `subtree_bounds` bottom-up across the layout tree,
    /// skipping clean subtrees whose `SUBTREE_META_VALID` flag is already up to date.
    pub fn refresh_subtree_hit_metadata(&mut self) {
        self.refresh_subtree_hit_metadata_inner(None, true);
    }

    #[inline]
    fn is_self_height_dependent(&self) -> bool {
        use strata_style::types::{Dimension, Position};
        matches!(self.style.height, Dimension::Percent(_))
            || matches!(self.style.min_height, Dimension::Percent(_))
            || matches!(self.style.max_height, Dimension::Percent(_))
            || matches!(self.style.flex_basis, Dimension::Percent(_))
            || (matches!(self.style.position, Position::Absolute | Position::Fixed)
                && (!matches!(self.style.insets.bottom, Dimension::Auto)
                    || matches!(self.style.insets.top, Dimension::Percent(_))))
    }

    fn refresh_subtree_hit_metadata_inner(
        &mut self,
        parent_display: Option<strata_style::types::Display>,
        is_root: bool,
    ) {
        let self_hoisted = !is_root
            && (self.style.position != strata_style::types::Position::Static
                || self.establishes_stacking_context_in(parent_display, false));
        if (self.subtree_hit_flags & SUBTREE_META_VALID) != 0
            && (!self_hoisted || (self.subtree_hit_flags & SUBTREE_HAS_HOISTED) != 0)
        {
            return;
        }

        if matches!(self.style.display, strata_style::types::Display::None) {
            self.subtree_hit_flags = SUBTREE_META_VALID;
            self.subtree_bounds = Rect::new(0.0, 0.0, 0.0, 0.0);
            return;
        }

        let mut flags = SUBTREE_META_VALID;
        if self.is_top_layer {
            flags |= SUBTREE_HAS_TOP_LAYER;
        }
        if self.style.has_transform() {
            flags |= SUBTREE_HAS_TRANSFORM;
        }
        if self_hoisted {
            flags |= SUBTREE_HAS_HOISTED;
        }

        let own = self.geometry.border_box;
        let mut min_x = own.x;
        let mut min_y = own.y;
        let mut max_x = own.x + own.width.max(0.0);
        let mut max_y = own.y + own.height.max(0.0);
        let mut has_bounds = own.width > 0.0 || own.height > 0.0 || self.children.is_empty();

        let union_rect = |r: Rect,
                          min_x: &mut f32,
                          min_y: &mut f32,
                          max_x: &mut f32,
                          max_y: &mut f32,
                          has_bounds: &mut bool| {
            let rx1 = r.x + r.width.max(0.0);
            let ry1 = r.y + r.height.max(0.0);
            if *has_bounds {
                *min_x = min_x.min(r.x);
                *min_y = min_y.min(r.y);
                *max_x = max_x.max(rx1);
                *max_y = max_y.max(ry1);
            } else {
                *min_x = r.x;
                *min_y = r.y;
                *max_x = rx1;
                *max_y = ry1;
                *has_bounds = true;
            }
        };

        for line in &self.line_boxes {
            if line.bounds.width > 0.0 || line.bounds.height > 0.0 {
                union_rect(
                    line.bounds,
                    &mut min_x,
                    &mut min_y,
                    &mut max_x,
                    &mut max_y,
                    &mut has_bounds,
                );
            }
        }

        let self_display = Some(self.style.display);
        for child in &mut self.children {
            child.refresh_subtree_hit_metadata_inner(self_display, false);
            flags |= child.subtree_hit_flags & !SUBTREE_HAS_HEIGHT_DEP_DESCENDANT;
            if child.is_self_height_dependent()
                || (!matches!(child.style.height, strata_style::types::Dimension::Px(_))
                    && (child.subtree_hit_flags & SUBTREE_HAS_HEIGHT_DEP_DESCENDANT) != 0)
            {
                flags |= SUBTREE_HAS_HEIGHT_DEP_DESCENDANT;
            }
            if matches!(child.style.display, strata_style::types::Display::None)
                || child.is_top_layer
                || child.style.position != strata_style::types::Position::Static
                || child.establishes_stacking_context_in(self_display, false)
            {
                continue;
            }
            let cb = child.subtree_bounds;
            if cb.width > 0.0 || cb.height > 0.0 || (child.children.is_empty() && (cb.x != 0.0 || cb.y != 0.0)) {
                union_rect(
                    cb,
                    &mut min_x,
                    &mut min_y,
                    &mut max_x,
                    &mut max_y,
                    &mut has_bounds,
                );
            }
        }

        if has_bounds {
            if self.style.overflow_x != strata_style::types::Overflow::Visible {
                let clip_x0 = own.x;
                let clip_x1 = own.x + own.width.max(0.0);
                min_x = min_x.max(clip_x0).min(clip_x1);
                max_x = max_x.min(clip_x1).max(min_x);
            }
            if self.style.overflow_y != strata_style::types::Overflow::Visible {
                let clip_y0 = own.y;
                let clip_y1 = own.y + own.height.max(0.0);
                min_y = min_y.max(clip_y0).min(clip_y1);
                max_y = max_y.min(clip_y1).max(min_y);
            }
            self.subtree_bounds = Rect::new(min_x, min_y, max_x - min_x, max_y - min_y);
        } else {
            self.subtree_bounds = own;
        }
        self.subtree_hit_flags = flags;
    }

    #[inline]
    pub(crate) fn is_height_independent_for_cache(&self) -> bool {
        self.style.height == strata_style::types::Dimension::Auto
            && self.style.flex_basis == strata_style::types::Dimension::Auto
            && self.style.flex_grow == 0.0
            && !matches!(
                self.style.min_height,
                strata_style::types::Dimension::Percent(_)
            )
            && !matches!(
                self.style.max_height,
                strata_style::types::Dimension::Percent(_)
            )
            && !(matches!(
                self.style.position,
                strata_style::types::Position::Absolute | strata_style::types::Position::Fixed
            ) && (!matches!(
                self.style.insets.bottom,
                strata_style::types::Dimension::Auto
            ) || matches!(
                self.style.insets.top,
                strata_style::types::Dimension::Percent(_)
            )))
            && !self.has_height_dependent_descendant_fast()
    }

    /// Looks up a cached measurement probe `(result_w, result_h, baseline_offset)` for a clean
    /// subtree without modifying `self.geometry` or descending into children.
    #[inline]
    pub(crate) fn lookup_measure_cache(
        &self,
        available_w: f32,
        available_h: f32,
        style_pin_hash: u64,
    ) -> Option<(f32, f32, f32)> {
        if !self.children_layout_clean {
            return None;
        }
        for slot in &self.measure_cache {
            if slot.is_valid
                && slot.style_pin_hash == style_pin_hash
                && (available_w - slot.available_w).abs() < 0.01
                && ((available_h - slot.available_h).abs() < 0.01
                    || self.is_height_independent_for_cache())
            {
                return Some((slot.result_w, slot.result_h, slot.baseline_offset));
            }
        }
        None
    }

    /// Stores a measurement probe result in slot `0` (Pass 1) or `1` (Pass 2).
    #[inline]
    pub(crate) fn store_measure_cache(
        &mut self,
        slot_idx: usize,
        available_w: f32,
        available_h: f32,
        style_pin_hash: u64,
        result_w: f32,
        result_h: f32,
        baseline_offset: f32,
    ) {
        self.measure_cache[slot_idx.min(1)] = MeasureCacheEntry {
            available_w,
            available_h,
            style_pin_hash,
            result_w,
            result_h,
            baseline_offset,
            is_valid: true,
        };
    }

    /// Computes layout geometry passing an active exclusion space and depth guard.
    pub fn compute_layout_guarded(
        &mut self,
        available_width: f32,
        available_height: f32,
        exclusions: Option<&mut crate::exclusions::ExclusionSpace>,
        bfc_offset_y: f32,
        guard: LayoutDepthGuard,
    ) {
        let _guard = crate::prof::LayoutGuard::new();
        crate::prof::observe_width(self.children.len());

        let available_width = if available_width.is_finite() && available_width >= 0.0 {
            available_width.min(1e7)
        } else {
            0.0
        };
        let available_height = if available_height.is_finite() && available_height >= 0.0 {
            available_height.min(1e7)
        } else {
            0.0
        };

        if self.style.display == Display::None {
            self.geometry = LayoutGeometry::default();
            self.layout_cache.is_valid = false;
            self.children_layout_clean = false;
            self.subtree_hit_flags &= !SUBTREE_META_VALID;
            return;
        }

        if guard.is_exhausted() {
            record_layout_overflow(guard.current_depth(), guard.max_depth());
            let padding = self.style.padding;
            let border = self.style.border_width;
            let margin = self.style.margin;

            let w = crate::box_model::resolve_dimension_with_node(
                self.style.width,
                available_width,
                self,
            )
            .unwrap_or(available_width)
            .max(0.0);
            let h = crate::box_model::resolve_dimension(self.style.height, available_height)
                .unwrap_or_else(|| {
                    if self.text.is_some() {
                        self.style.font_size * self.style.line_height
                    } else {
                        0.0
                    }
                })
                .max(0.0);

            self.geometry = LayoutGeometry::from_border_box(
                crate::geometry::Rect::new(
                    self.geometry.border_box.x,
                    self.geometry.border_box.y,
                    w,
                    h,
                ),
                padding,
                border,
                margin,
            );
            self.geometry.snap_to_pixel();
            self.layout_cache.is_valid = false;
            self.children_layout_clean = false;
            self.subtree_hit_flags &= !SUBTREE_META_VALID;
            return;
        }

        let is_unconstrained_root = exclusions
            .as_ref()
            .is_none_or(|e| e.exclusions().is_empty());
        let style_pin_hash = layout_style_pin_hash(&self.style);

        // Subtree layout cache hit: skip re-traversal if available dimensions & style pins match.
        // Only evaluate `is_height_independent_for_cache()` when `available_w` and `style_pin_hash`
        // already match and `available_h` differs (avoiding unnecessary subtree walks).
        if is_unconstrained_root
            && self.layout_cache.is_valid
            && self.layout_cache.style_pin_hash == style_pin_hash
            && (available_width - self.layout_cache.available_w).abs() < 0.01
            && ((available_height - self.layout_cache.available_h).abs() < 0.01
                || self.is_height_independent_for_cache())
        {
            let dx = self.geometry.border_box.x - self.layout_cache.origin_x;
            let dy = self.geometry.border_box.y - self.layout_cache.origin_y;
            if dx.abs() > 0.001 || dy.abs() > 0.001 {
                self.geometry.padding_box.x += dx;
                self.geometry.padding_box.y += dy;
                self.geometry.content_box.x += dx;
                self.geometry.content_box.y += dy;
                self.geometry.margin_box.x += dx;
                self.geometry.margin_box.y += dy;
                self.subtree_bounds.x += dx;
                self.subtree_bounds.y += dy;
                if let Some(ref mut pb) = self.last_preserved_box {
                    pb.0 += dx;
                    pb.1 += dy;
                }
                if let Some(ref mut grid) = self.table_grid {
                    grid.translate(dx, dy);
                }
                for line in &mut self.line_boxes {
                    line.bounds.x += dx;
                    line.bounds.y += dy;
                    for frag in &mut line.text_fragments {
                        frag.bounds.x += dx;
                        frag.bounds.y += dy;
                        frag.baseline_y += dy;
                    }
                    for at in &mut line.atomic_fragments {
                        at.bounds.x += dx;
                        at.bounds.y += dy;
                    }
                }
                for child in &mut self.children {
                    child.translate(dx, dy);
                }
                self.layout_cache.origin_x = self.geometry.border_box.x;
                self.layout_cache.origin_y = self.geometry.border_box.y;
            }
            return;
        }

        self.subtree_hit_flags &= !SUBTREE_META_VALID;

        // Dispatch according to container display formatting context
        match self.style.display {
            Display::Flex | Display::InlineFlex => {
                crate::flexbox::solve_flexbox_guarded(
                    self,
                    available_width,
                    available_height,
                    guard,
                );
            }
            Display::Grid | Display::InlineGrid => {
                crate::grid::solve_grid_guarded(self, available_width, available_height, guard);
            }
            Display::Table | Display::InlineTable => {
                crate::table::solve_table_guarded(self, available_width, available_height, guard);
            }
            Display::Block
            | Display::ListItem
            | Display::Inline
            | Display::InlineBlock
            | Display::TableRowGroup
            | Display::TableHeaderGroup
            | Display::TableFooterGroup
            | Display::TableRow
            | Display::TableCell
            | Display::TableCaption
            | Display::TableColumnGroup
            | Display::TableColumn => {
                crate::box_model::solve_block_with_exclusions_guarded(
                    self,
                    available_width,
                    available_height,
                    exclusions,
                    bfc_offset_y,
                    guard,
                );
            }
            Display::None => unreachable!(),
        }

        // Resolve relative and out-of-flow absolute/fixed positioned children
        crate::positioning::solve_positioning_guarded(
            self,
            available_width,
            available_height,
            guard,
        );

        // Subpixel snapping and geometry sanitization
        self.geometry.snap_to_pixel();
        self.geometry.sanitize();

        // Update scrollable overflow metrics
        self.update_scroll_metrics();

        if !self.children.is_empty() || !self.inline_items.is_empty() || self.text.is_some() {
            self.last_child_layout_content = Some((
                self.geometry.content_box.width,
                self.geometry.content_box.height,
            ));
        }
        self.children_layout_clean = true;
        self.last_preserved_box = None;

        // Record layout result in cache for subsequent multi-pass probes
        if is_unconstrained_root {
            self.layout_cache = LayoutCacheEntry {
                available_w: available_width,
                available_h: available_height,
                result_w: self.geometry.border_box.width,
                result_h: self.geometry.border_box.height,
                origin_x: self.geometry.border_box.x,
                origin_y: self.geometry.border_box.y,
                style_pin_hash,
                is_valid: true,
            };
        }
    }

    #[inline]
    fn has_height_dependent_descendant_fast(&self) -> bool {
        if (self.subtree_hit_flags & SUBTREE_META_VALID) != 0 {
            return (self.subtree_hit_flags & SUBTREE_HAS_HEIGHT_DEP_DESCENDANT) != 0;
        }
        self.has_height_dependent_descendant()
    }

    /// Returns true if any descendant (up to an explicit pixel-height boundary) depends on parent height.
    fn has_height_dependent_descendant(&self) -> bool {
        use strata_style::types::{Dimension, Position};
        let mut stack: Vec<&LayoutNode> = self.children.iter().collect();
        while let Some(node) = stack.pop() {
            if matches!(node.style.height, Dimension::Percent(_))
                || matches!(node.style.min_height, Dimension::Percent(_))
                || matches!(node.style.max_height, Dimension::Percent(_))
                || matches!(node.style.flex_basis, Dimension::Percent(_))
            {
                return true;
            }
            if matches!(node.style.position, Position::Absolute | Position::Fixed)
                && (!matches!(node.style.insets.bottom, Dimension::Auto)
                    || matches!(node.style.insets.top, Dimension::Percent(_)))
            {
                return true;
            }
            if !matches!(node.style.height, Dimension::Px(_)) {
                for grand in &node.children {
                    stack.push(grand);
                }
            }
        }
        false
    }

    /// Invalidates all layout, measurement, intrinsic sizing, and hit-metadata caches strictly on
    /// this node without disturbing clean child subtrees.
    #[inline]
    pub fn invalidate_self_caches(&mut self) {
        self.layout_cache.is_valid = false;
        self.measure_cache[0].is_valid = false;
        self.measure_cache[1].is_valid = false;
        self.intrinsic_cache.clear();
        self.last_child_layout_content = None;
        self.children_layout_clean = false;
        self.last_preserved_box = None;
        self.subtree_hit_flags &= !SUBTREE_META_VALID;
    }

    /// Invalidates layout cache on this node and all descendants using an iterative heap stack.
    pub fn invalidate_cache(&mut self) {
        let mut stack: Vec<&mut LayoutNode> = vec![self];
        while let Some(node) = stack.pop() {
            node.invalidate_self_caches();
            for child in &mut node.children {
                stack.push(child);
            }
        }
    }

    /// Returns true if this node forms an isolated layout boundary (W3C CSS Containment Level 2 §3.1).
    ///
    /// An element is a layout boundary if internal descendant changes can never affect
    /// the layout of ancestor or sibling elements outside this boundary:
    /// 1. Explicit CSS `contain: strict` or `contain: content`
    /// 2. Explicit CSS `contain: layout` combined with `contain: size` or definite pixel dimensions
    /// 3. CSS `content-visibility: auto` or `hidden` with definite pixel dimensions
    /// 4. Out-of-flow positioned boxes with clipping and definite pixel dimensions
    pub fn is_layout_boundary(&self) -> bool {
        // 1. Explicit contain: strict or contain: content
        if self.style.contain == strata_style::types::Containment::STRICT
            || self.style.contain == strata_style::types::Containment::CONTENT
        {
            return true;
        }

        // 2. Explicit contain: layout with contain: size
        if self.style.contain.layout && self.style.contain.size {
            return true;
        }

        // 3. Explicit contain: layout with definite width and height
        if self.style.contain.layout && self.has_definite_dimensions() {
            return true;
        }

        // 4. Content-visibility containment with definite dimensions
        if self.style.content_visibility != strata_style::types::ContentVisibility::Visible
            && self.has_definite_dimensions()
        {
            return true;
        }

        // 5. Out-of-flow positioned boxes with clipping and definite dimensions
        if matches!(
            self.style.position,
            strata_style::types::Position::Absolute | strata_style::types::Position::Fixed
        ) && self.has_definite_dimensions()
            && self.style.overflow_x != strata_style::types::Overflow::Visible
            && self.style.overflow_y != strata_style::types::Overflow::Visible
        {
            return true;
        }

        false
    }

    /// Checks if both width and height are definite non-auto pixel dimensions.
    #[inline]
    pub fn has_definite_dimensions(&self) -> bool {
        matches!(self.style.width, strata_style::types::Dimension::Px(_))
            && matches!(self.style.height, strata_style::types::Dimension::Px(_))
    }

    /// Recursively searches for the nearest ancestor layout boundary containing the given index path.
    /// Returns the prefix of `path` that terminates at the nearest layout boundary container.
    pub fn find_layout_boundary_path<'a>(&self, path: &'a [usize]) -> Option<&'a [usize]> {
        let mut best_boundary_len = if self.is_layout_boundary() {
            Some(0)
        } else {
            None
        };

        let mut curr = self;
        for (i, &child_idx) in path.iter().enumerate() {
            if child_idx >= curr.children.len() {
                break;
            }
            curr = &curr.children[child_idx];
            if curr.is_layout_boundary() {
                best_boundary_len = Some(i + 1);
            }
        }

        best_boundary_len.map(|len| &path[..len])
    }

    /// Recomputes layout for the subtree rooted at `path` within its containing layout boundary.
    ///
    /// If an ancestor layout boundary exists along `path`, layout is recomputed ONLY for that
    /// boundary container and its descendants, guaranteeing zero re-layouts on the rest of the tree.
    /// Returns `true` if a layout boundary was found and laid out locally, or `false` if root
    /// layout recomputation is required.
    pub fn compute_subgrid_or_boundary_layout(&mut self, path: &[usize]) -> bool {
        let boundary_len = match self.find_layout_boundary_path(path) {
            Some(subpath) => subpath.len(),
            None => return false,
        };

        if boundary_len == 0 {
            // Root itself is the layout boundary
            self.layout_children();
            return true;
        }

        // Navigate to the boundary node mutably
        let mut curr = self;
        for &idx in &path[..boundary_len] {
            if idx >= curr.children.len() {
                return false;
            }
            curr = &mut curr.children[idx];
        }

        // Recompute layout strictly within the isolated boundary
        curr.layout_children();
        true
    }

    /// Invalidates layout cache only for nodes along `path` under the nearest layout boundary.
    pub fn invalidate_layout_boundary(&mut self, path: &[usize]) -> bool {
        let boundary_len = match self.find_layout_boundary_path(path) {
            Some(subpath) => subpath.len(),
            None => {
                self.invalidate_cache();
                return false;
            }
        };

        let mut curr = self;
        for &idx in &path[..boundary_len] {
            if idx >= curr.children.len() {
                return false;
            }
            curr = &mut curr.children[idx];
        }

        curr.invalidate_cache();
        true
    }

    /// Recursively calculates the index path to the layout node with matching EntityId.
    pub fn find_path_by_entity_id(
        &self,
        target_id: strata_primitives::EntityId,
    ) -> Option<Vec<usize>> {
        if !target_id.is_valid() {
            return None;
        }
        let mut path = Vec::new();
        if self.find_path_by_entity_id_internal(target_id, &mut path) {
            Some(path)
        } else {
            None
        }
    }

    fn find_path_by_entity_id_internal(
        &self,
        target_id: strata_primitives::EntityId,
        path: &mut Vec<usize>,
    ) -> bool {
        if self.entity_id == target_id {
            return true;
        }
        for (i, child) in self.children.iter().enumerate() {
            path.push(i);
            if child.find_path_by_entity_id_internal(target_id, path) {
                return true;
            }
            path.pop();
        }
        false
    }

    /// Invalidates layout cache on self and all ancestors leading down to `path`.
    pub fn invalidate_ancestors(&mut self, path: &[usize]) {
        self.invalidate_self_caches();
        let mut curr = self;
        for &idx in path {
            if idx >= curr.children.len() {
                break;
            }
            curr = &mut curr.children[idx];
            curr.invalidate_self_caches();
        }
    }

    /// Evaluates child geometry to compute `scroll_bounds` and `has_scroll` for this container.
    pub fn update_scroll_metrics(&mut self) {
        let origin_x = self.geometry.padding_box.x;
        let origin_y = self.geometry.padding_box.y;
        let mut max_extent_x = self.geometry.content_box.width;
        let mut max_extent_y = self.geometry.content_box.height;

        for child in &self.children {
            if child.style.display == strata_style::types::Display::None
                || child.style.position == strata_style::types::Position::Fixed
            {
                continue;
            }
            let child_end_x = child.geometry.border_box.max_x() - origin_x;
            let child_end_y = child.geometry.border_box.max_y() - origin_y;
            if child_end_x.is_finite() && child_end_x > max_extent_x {
                max_extent_x = child_end_x;
            }
            if child_end_y.is_finite() && child_end_y > max_extent_y {
                max_extent_y = child_end_y;
            }
        }

        for line in &self.line_boxes {
            let line_end_x = line.bounds.max_x() - origin_x;
            let line_end_y = line.bounds.max_y() - origin_y;
            if line_end_x.is_finite() && line_end_x > max_extent_x {
                max_extent_x = line_end_x;
            }
            if line_end_y.is_finite() && line_end_y > max_extent_y {
                max_extent_y = line_end_y;
            }
        }

        self.geometry.scroll_bounds = crate::geometry::Size::new(max_extent_x, max_extent_y);

        let has_scroll_x = match self.style.overflow_x {
            strata_style::types::Overflow::Scroll => true,
            strata_style::types::Overflow::Auto => max_extent_x > self.geometry.padding_box.width,
            _ => false,
        };
        let has_scroll_y = match self.style.overflow_y {
            strata_style::types::Overflow::Scroll => true,
            strata_style::types::Overflow::Auto => max_extent_y > self.geometry.padding_box.height,
            _ => false,
        };
        self.geometry.has_scroll = (has_scroll_x, has_scroll_y);
        self.geometry.scroll_offset = self
            .geometry
            .clamp_scroll_offset(self.geometry.scroll_offset);
    }
}

impl Drop for LayoutNode {
    fn drop(&mut self) {
        // Drain descendants iteratively onto a heap stack to prevent recursion stack overflow
        // during deallocation of deeply nested trees (e.g. 10,000+ levels).
        let mut stack = std::mem::take(&mut self.children);
        while let Some(mut node) = stack.pop() {
            if !node.children.is_empty() {
                let grandchildren = std::mem::take(&mut node.children);
                stack.extend(grandchildren);
            }
        }
    }
}
