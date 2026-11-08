//! Core CSS style types, layout enums, and ComputedStyle representation.

use std::sync::Arc;
pub use strata_primitives::color::Rgba;
pub use strata_primitives::geom::{EdgeInsets, Matrix4x4};
pub use strata_primitives::AtomId;

pub type Color = Rgba;
pub type CustomPropertyMap = Arc<Vec<(AtomId, AtomId)>>;

/// Box layout display model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Display {
    #[default]
    Block,
    Inline,
    InlineBlock,
    ListItem,
    Flex,
    InlineFlex,
    Grid,
    InlineGrid,
    Table,
    InlineTable,
    TableRowGroup,
    TableHeaderGroup,
    TableFooterGroup,
    TableRow,
    TableCell,
    TableCaption,
    TableColumnGroup,
    TableColumn,
    None,
}

/// CSS table layout algorithm mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TableLayout {
    #[default]
    Auto,
    Fixed,
}

/// CSS border collapse model for tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BorderCollapse {
    #[default]
    Separate,
    Collapse,
}

/// CSS table caption placement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CaptionSide {
    #[default]
    Top,
    Bottom,
}

/// Positioning scheme in CSS box layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Position {
    #[default]
    Static,
    Relative,
    Absolute,
    Fixed,
    Sticky,
}

/// CSS float property defining element lateral drift.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Float {
    #[default]
    None,
    Left,
    Right,
    InlineStart,
    InlineEnd,
}

/// CSS clear property defining clearance below floating boxes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Clear {
    #[default]
    None,
    Left,
    Right,
    Both,
    InlineStart,
    InlineEnd,
}

/// CSS Length and Dimension units.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Dimension {
    #[default]
    Auto,
    Px(f32),
    Percent(f32),
    Vw(f32),
    Vh(f32),
    Rem(f32),
    Em(f32),
    Calc(crate::calc::CalcId),
    MinContent,
    MaxContent,
    FitContent(f32),
}

impl Dimension {
    /// Creates a Dimension variant from an AST calc expression.
    pub fn calc(expr: crate::calc::CalcExpr) -> Self {
        Self::Calc(crate::calc::intern_calc(expr))
    }

    /// Resolves the dimension into concrete pixels given a context.
    pub fn resolve_px(
        &self,
        parent_size: f32,
        root_font_size: f32,
        current_font_size: f32,
    ) -> Option<f32> {
        match *self {
            Self::Auto => None,
            Self::Px(px) => Some(px),
            Self::Percent(pct) => Some(parent_size * (pct / 100.0)),
            Self::Vw(vw) => Some(vw), // Handled by viewport context in layout
            Self::Vh(vh) => Some(vh),
            Self::Rem(rem) => Some(rem * root_font_size),
            Self::Em(em) => Some(em * current_font_size),
            Self::Calc(id) => crate::calc::get_calc(id).map(|expr| {
                let ctx = crate::calc::CalcContext {
                    parent_size,
                    root_font_size,
                    current_font_size,
                    viewport_width: 1920.0,
                    viewport_height: 1080.0,
                };
                expr.eval(&ctx)
            }),
            Self::MinContent | Self::MaxContent | Self::FitContent(_) => None,
        }
    }
}

/// Box sizing model (CSS box-sizing).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BoxSizing {
    #[default]
    BorderBox,
    ContentBox,
}

/// Flex container main axis direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FlexDirection {
    #[default]
    Row,
    RowReverse,
    Column,
    ColumnReverse,
}

/// Flex line wrapping behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FlexWrap {
    #[default]
    NoWrap,
    Wrap,
    WrapReverse,
}

/// Main-axis alignment within flex containers and grid tracks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JustifyContent {
    #[default]
    FlexStart,
    FlexEnd,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

/// Cross-axis alignment for items in flex/grid containers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AlignItems {
    #[default]
    Stretch,
    FlexStart,
    FlexEnd,
    Center,
    Baseline,
}

/// Self-alignment on the cross-axis overriding container `align-items`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AlignSelf {
    #[default]
    Auto,
    Stretch,
    FlexStart,
    FlexEnd,
    Center,
    Baseline,
}

/// Individual track sizing breadth used in CSS minmax() functions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GridTrackBreadth {
    Auto,
    Px(f32),
    Percent(f32),
    Fr(f32),
    MinContent,
    MaxContent,
}

/// CSS Grid track sizing definition (W3C CSS Grid Layout Level 2).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GridTrack {
    Auto,
    Px(f32),
    Percent(f32),
    Fr(f32),
    MinContent,
    MaxContent,
    Subgrid,
    MinMax(GridTrackBreadth, GridTrackBreadth),
}

/// CSS grid-auto-flow property (W3C CSS Grid Level 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GridAutoFlow {
    #[default]
    Row,
    Column,
    RowDense,
    ColumnDense,
}

impl GridAutoFlow {
    pub fn is_row(&self) -> bool {
        matches!(self, Self::Row | Self::RowDense)
    }

    pub fn is_dense(&self) -> bool {
        matches!(self, Self::RowDense | Self::ColumnDense)
    }

    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim().to_ascii_lowercase();
        let tokens: Vec<&str> = s.split_whitespace().collect();
        match tokens.as_slice() {
            ["row"] => Some(Self::Row),
            ["column"] => Some(Self::Column),
            ["dense"] => Some(Self::RowDense),
            ["row", "dense"] | ["dense", "row"] => Some(Self::RowDense),
            ["column", "dense"] | ["dense", "column"] => Some(Self::ColumnDense),
            _ => None,
        }
    }
}

/// 4-corner border radius definition.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BorderRadius {
    pub top_left: f32,
    pub top_right: f32,
    pub bottom_right: f32,
    pub bottom_left: f32,
}

impl BorderRadius {
    pub const ZERO: Self = Self::all(0.0);

    pub const fn new(top_left: f32, top_right: f32, bottom_right: f32, bottom_left: f32) -> Self {
        Self {
            top_left,
            top_right,
            bottom_right,
            bottom_left,
        }
    }

    pub const fn all(radius: f32) -> Self {
        Self {
            top_left: radius,
            top_right: radius,
            bottom_right: radius,
            bottom_left: radius,
        }
    }
}

/// Standard W3C CSS blend modes for mix-blend-mode and background-blend-mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BlendMode {
    #[default]
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
}

impl BlendMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Multiply => "multiply",
            Self::Screen => "screen",
            Self::Overlay => "overlay",
            Self::Darken => "darken",
            Self::Lighten => "lighten",
            Self::ColorDodge => "color-dodge",
            Self::ColorBurn => "color-burn",
            Self::HardLight => "hard-light",
            Self::SoftLight => "soft-light",
            Self::Difference => "difference",
            Self::Exclusion => "exclusion",
            Self::Hue => "hue",
            Self::Saturation => "saturation",
            Self::Color => "color",
            Self::Luminosity => "luminosity",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "normal" => Some(Self::Normal),
            "multiply" => Some(Self::Multiply),
            "screen" => Some(Self::Screen),
            "overlay" => Some(Self::Overlay),
            "darken" => Some(Self::Darken),
            "lighten" => Some(Self::Lighten),
            "color-dodge" => Some(Self::ColorDodge),
            "color-burn" => Some(Self::ColorBurn),
            "hard-light" => Some(Self::HardLight),
            "soft-light" => Some(Self::SoftLight),
            "difference" => Some(Self::Difference),
            "exclusion" => Some(Self::Exclusion),
            "hue" => Some(Self::Hue),
            "saturation" => Some(Self::Saturation),
            "color" => Some(Self::Color),
            "luminosity" => Some(Self::Luminosity),
            _ => None,
        }
    }

    /// Returns true if this blend mode evaluates color channels independently.
    #[inline]
    pub fn is_separable(&self) -> bool {
        !matches!(
            self,
            Self::Hue | Self::Saturation | Self::Color | Self::Luminosity
        )
    }
}

/// CSS isolation property determining stacking group containment for blend modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Isolation {
    #[default]
    Auto,
    Isolate,
}

impl Isolation {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Isolate => "isolate",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(Self::Auto),
            "isolate" => Some(Self::Isolate),
            _ => None,
        }
    }
}

/// CSS box-shadow effect definition.
#[derive(Debug, Clone, PartialEq)]
pub struct BoxShadow {
    pub offset_x: f32,
    pub offset_y: f32,
    pub blur_radius: f32,
    pub spread_radius: f32,
    pub color: Color,
    pub inset: bool,
}

/// A color stop in a CSS gradient definition.
#[derive(Debug, Clone, PartialEq)]
pub struct ColorStop {
    pub color: Color,
    /// Stop position as a fraction [0.0, 1.0], or None for automatic distribution.
    pub position: Option<f32>,
}

/// CSS Gradient background specification.
#[derive(Debug, Clone, PartialEq)]
pub enum Gradient {
    Linear {
        /// Gradient orientation in degrees (0 = to top, 90 = to right, 180 = to bottom).
        angle_deg: f32,
        stops: Vec<ColorStop>,
    },
    Radial {
        /// Center origin fraction (cx, cy), typically (0.5, 0.5).
        center: (f32, f32),
        stops: Vec<ColorStop>,
    },
    Conic {
        angle_deg: f32,
        center: (f32, f32),
        stops: Vec<ColorStop>,
    },
}

/// Content overflow clipping behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Overflow {
    #[default]
    Visible,
    Hidden,
    Scroll,
    Auto,
}

/// Typographic font weight.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FontWeight {
    Thin = 100,
    ExtraLight = 200,
    Light = 300,
    #[default]
    Normal = 400,
    Medium = 500,
    SemiBold = 600,
    Bold = 700,
    ExtraBold = 800,
    Black = 900,
}

impl FontWeight {
    pub fn from_u16(weight: u16) -> Self {
        match weight {
            0..=150 => Self::Thin,
            151..=250 => Self::ExtraLight,
            251..=350 => Self::Light,
            351..=450 => Self::Normal,
            451..=550 => Self::Medium,
            551..=650 => Self::SemiBold,
            651..=750 => Self::Bold,
            751..=850 => Self::ExtraBold,
            _ => Self::Black,
        }
    }
}

/// Text horizontal alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
    Justify,
}

/// Font style (normal, italic, oblique).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FontStyle {
    #[default]
    Normal,
    Italic,
    Oblique,
}

/// Text overflow behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextOverflow {
    #[default]
    Clip,
    Ellipsis,
}

/// Text decoration line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextDecoration {
    #[default]
    None,
    Underline,
    LineThrough,
    Overline,
}

/// Text capitalization transform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextTransform {
    #[default]
    None,
    Uppercase,
    Lowercase,
    Capitalize,
}

/// Whitespace handling behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WhiteSpace {
    #[default]
    Normal,
    NoWrap,
    Pre,
    PreWrap,
    PreLine,
}

/// Vertical alignment within line box or table cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VerticalAlign {
    #[default]
    Baseline,
    Top,
    Middle,
    Bottom,
    TextTop,
    TextBottom,
    Sub,
    Super,
}

/// Mouse pointer cursor style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Cursor {
    #[default]
    Default,
    Pointer,
    Text,
    Move,
    NotAllowed,
    Crosshair,
    Grab,
    Grabbing,
    ColResize,
    RowResize,
    Wait,
    Help,
    None,
}

/// Text selection behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UserSelect {
    #[default]
    Auto,
    None,
    Text,
    All,
}

/// Pointer event pass-through behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PointerEvents {
    #[default]
    Auto,
    None,
}

/// Element visibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Visibility {
    #[default]
    Visible,
    Hidden,
    Collapse,
}

/// Border and outline stroke style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BorderStyle {
    #[default]
    Solid,
    Dashed,
    Dotted,
    Double,
    None,
    Hidden,
}

/// Content visibility optimization (W3C CSS Containment Module Level 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ContentVisibility {
    #[default]
    Visible,
    Auto,
    Hidden,
}

/// CSS Containment specification (W3C CSS Containment Module Level 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Containment {
    pub size: bool,
    pub inline_size: bool,
    pub layout: bool,
    pub paint: bool,
    pub style: bool,
}

impl Containment {
    pub const NONE: Self = Self {
        size: false,
        inline_size: false,
        layout: false,
        paint: false,
        style: false,
    };

    pub const STRICT: Self = Self {
        size: true,
        inline_size: false,
        layout: true,
        paint: true,
        style: false,
    };

    pub const CONTENT: Self = Self {
        size: false,
        inline_size: false,
        layout: true,
        paint: true,
        style: false,
    };

    #[inline]
    pub fn contains_layout(&self) -> bool {
        self.layout
    }

    #[inline]
    pub fn contains_size(&self) -> bool {
        self.size
    }

    #[inline]
    pub fn contains_paint(&self) -> bool {
        self.paint
    }
}


/// Stepped timing function transition position (W3C CSS Easing Functions Level 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StepPosition {
    JumpStart,
    #[default]
    JumpEnd,
    JumpNone,
    JumpBoth,
    Start,
    End,
}

/// Animation and transition timing function (easing curve).
#[derive(Debug, Clone, Copy, PartialEq)]
#[derive(Default)]
pub enum TransitionEasing {
    Linear,
    #[default]
    Ease,
    EaseIn,
    EaseOut,
    EaseInOut,
    StepStart,
    StepEnd,
    Steps(u32, StepPosition),
    CubicBezier(f32, f32, f32, f32),
}


impl TransitionEasing {
    /// Evaluates the easing function at normalized progression `t` in `[0.0, 1.0]`.
    pub fn evaluate(&self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Linear => t,
            Self::Ease => cubic_bezier_solve(t, 0.25, 0.1, 0.25, 1.0),
            Self::EaseIn => cubic_bezier_solve(t, 0.42, 0.0, 1.0, 1.0),
            Self::EaseOut => cubic_bezier_solve(t, 0.0, 0.0, 0.58, 1.0),
            Self::EaseInOut => cubic_bezier_solve(t, 0.42, 0.0, 0.58, 1.0),
            Self::StepStart => {
                if t > 0.0 {
                    1.0
                } else {
                    0.0
                }
            }
            Self::StepEnd => {
                if t >= 1.0 {
                    1.0
                } else {
                    0.0
                }
            }
            Self::Steps(n, pos) => evaluate_steps(t, *n, *pos),
            Self::CubicBezier(x1, y1, x2, y2) => cubic_bezier_solve(t, *x1, *y1, *x2, *y2),
        }
    }

    /// Solves the easing function at normalized progression `t` in `[0.0, 1.0]`.
    #[inline(always)]
    pub fn solve(&self, t: f32) -> f32 {
        self.evaluate(t)
    }

    /// Parses a CSS timing function string into a `TransitionEasing`.
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        let lower = s.to_ascii_lowercase();
        match lower.as_str() {
            "linear" => return Some(Self::Linear),
            "ease" => return Some(Self::Ease),
            "ease-in" => return Some(Self::EaseIn),
            "ease-out" => return Some(Self::EaseOut),
            "ease-in-out" => return Some(Self::EaseInOut),
            "step-start" => return Some(Self::StepStart),
            "step-end" => return Some(Self::StepEnd),
            _ => {}
        }

        if lower.starts_with("cubic-bezier(") && lower.ends_with(')') {
            let inner = &s[13..s.len() - 1];
            let parts: Vec<&str> = inner.split(',').collect();
            if parts.len() == 4 {
                if let (Ok(x1), Ok(y1), Ok(x2), Ok(y2)) = (
                    parts[0].trim().parse::<f32>(),
                    parts[1].trim().parse::<f32>(),
                    parts[2].trim().parse::<f32>(),
                    parts[3].trim().parse::<f32>(),
                ) {
                    return Some(Self::CubicBezier(x1, y1, x2, y2));
                }
            }
        }

        if lower.starts_with("steps(") && lower.ends_with(')') {
            let inner = &s[6..s.len() - 1];
            let parts: Vec<&str> = inner.split(',').collect();
            if parts.len() == 1 {
                if let Ok(n) = parts[0].trim().parse::<u32>() {
                    return Some(Self::Steps(n, StepPosition::End));
                }
            } else if parts.len() == 2 {
                if let Ok(n) = parts[0].trim().parse::<u32>() {
                    let pos = match parts[1].trim().to_ascii_lowercase().as_str() {
                        "jump-start" | "start" => StepPosition::JumpStart,
                        "jump-end" | "end" => StepPosition::JumpEnd,
                        "jump-none" => StepPosition::JumpNone,
                        "jump-both" => StepPosition::JumpBoth,
                        _ => StepPosition::End,
                    };
                    return Some(Self::Steps(n, pos));
                }
            }
        }

        None
    }
}

/// Solves a cubic Bézier curve for parameter `x` using De Casteljau's algorithm and root finding.
#[inline(always)]
pub fn cubic_bezier_solve(x: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    crate::bezier::cubic_bezier_solve(x, x1, y1, x2, y2)
}

/// Evaluates stepped progress for `steps(n, position)`.
pub fn evaluate_steps(t: f32, n: u32, pos: StepPosition) -> f32 {
    let n = n.max(1) as f32;
    let t = t.clamp(0.0, 1.0);
    match pos {
        StepPosition::JumpStart | StepPosition::Start => ((t * n).floor() + 1.0) / n,
        StepPosition::JumpEnd | StepPosition::End => (t * n).floor() / n,
        StepPosition::JumpNone => {
            if n <= 1.0 {
                t
            } else {
                (t * (n - 1.0)).floor() / (n - 1.0)
            }
        }
        StepPosition::JumpBoth => ((t * (n + 1.0)).floor() + 1.0) / (n + 1.0),
    }
    .clamp(0.0, 1.0)
}

/// Identifies an individual animatable CSS property target for transitions.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum TransitionProperty {
    #[default]
    All,
    None,
    Color,
    BackgroundColor,
    Opacity,
    Transform,
    Width,
    Height,
    Margin,
    Padding,
    BorderColor,
    BorderWidth,
    BorderRadius,
    BoxShadow,
    Filter,
    BackdropFilter,
    ClipPath,
    MixBlendMode,
    Isolation,
    ViewTransitionName,
    Custom(String),
}

impl TransitionProperty {
    /// Parses a property identifier string into a `TransitionProperty`.
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "all" => Self::All,
            "none" => Self::None,
            "color" => Self::Color,
            "background-color" | "background" => Self::BackgroundColor,
            "opacity" => Self::Opacity,
            "transform" => Self::Transform,
            "width" => Self::Width,
            "height" => Self::Height,
            "margin" => Self::Margin,
            "padding" => Self::Padding,
            "border-color" => Self::BorderColor,
            "border-width" => Self::BorderWidth,
            "border-radius" => Self::BorderRadius,
            "box-shadow" => Self::BoxShadow,
            "filter" => Self::Filter,
            "backdrop-filter" => Self::BackdropFilter,
            "clip-path" => Self::ClipPath,
            "mix-blend-mode" => Self::MixBlendMode,
            "isolation" => Self::Isolation,
            "view-transition-name" => Self::ViewTransitionName,
            other => Self::Custom(other.to_string()),
        }
    }

    /// Evaluates if this transition property matches a specific target CSS property.
    pub fn matches(&self, target: &str) -> bool {
        match self {
            Self::All => true,
            Self::None => false,
            Self::Color => target == "color",
            Self::BackgroundColor => target == "background-color" || target == "background",
            Self::Opacity => target == "opacity",
            Self::Transform => target == "transform",
            Self::Width => target == "width",
            Self::Height => target == "height",
            Self::Margin => target.starts_with("margin"),
            Self::Padding => target.starts_with("padding"),
            Self::BorderColor => target.starts_with("border-color"),
            Self::BorderWidth => target.starts_with("border-width"),
            Self::BorderRadius => target.starts_with("border-radius"),
            Self::BoxShadow => target == "box-shadow",
            Self::Filter => target == "filter",
            Self::BackdropFilter => target == "backdrop-filter",
            Self::ClipPath => target == "clip-path",
            Self::MixBlendMode => target == "mix-blend-mode",
            Self::Isolation => target == "isolation",
            Self::ViewTransitionName => target == "view-transition-name",
            Self::Custom(c) => c.eq_ignore_ascii_case(target),
        }
    }

    /// Returns the canonical CSS property name string.
    pub fn as_str(&self) -> &str {
        match self {
            Self::All => "all",
            Self::None => "none",
            Self::Color => "color",
            Self::BackgroundColor => "background-color",
            Self::Opacity => "opacity",
            Self::Transform => "transform",
            Self::Width => "width",
            Self::Height => "height",
            Self::Margin => "margin",
            Self::Padding => "padding",
            Self::BorderColor => "border-color",
            Self::BorderWidth => "border-width",
            Self::BorderRadius => "border-radius",
            Self::BoxShadow => "box-shadow",
            Self::Filter => "filter",
            Self::BackdropFilter => "backdrop-filter",
            Self::ClipPath => "clip-path",
            Self::MixBlendMode => "mix-blend-mode",
            Self::Isolation => "isolation",
            Self::ViewTransitionName => "view-transition-name",
            Self::Custom(s) => s.as_str(),
        }
    }
}

/// Transition behavior for discrete animatable properties (W3C CSS Transitions Level 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TransitionBehavior {
    #[default]
    Normal,
    AllowDiscrete,
}

impl TransitionBehavior {
    /// Parses a CSS transition-behavior string.
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "normal" => Some(Self::Normal),
            "allow-discrete" => Some(Self::AllowDiscrete),
            _ => None,
        }
    }
}

/// Complete transition descriptor specifying timing, delay, easing, and behavior.
#[derive(Debug, Clone, PartialEq)]
pub struct TransitionSpec {
    pub property: TransitionProperty,
    pub duration_ms: f32,
    pub timing_function: TransitionEasing,
    pub delay_ms: f32,
    pub behavior: TransitionBehavior,
}

impl Default for TransitionSpec {
    fn default() -> Self {
        Self {
            property: TransitionProperty::All,
            duration_ms: 0.0,
            timing_function: TransitionEasing::Ease,
            delay_ms: 0.0,
            behavior: TransitionBehavior::Normal,
        }
    }
}

/// CSS animation-direction property (W3C CSS Animations Level 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AnimationDirection {
    #[default]
    Normal,
    Reverse,
    Alternate,
    AlternateReverse,
}

impl AnimationDirection {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "normal" => Some(Self::Normal),
            "reverse" => Some(Self::Reverse),
            "alternate" => Some(Self::Alternate),
            "alternate-reverse" => Some(Self::AlternateReverse),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Reverse => "reverse",
            Self::Alternate => "alternate",
            Self::AlternateReverse => "alternate-reverse",
        }
    }
}

/// CSS animation-fill-mode property (W3C CSS Animations Level 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AnimationFillMode {
    #[default]
    None,
    Forwards,
    Backwards,
    Both,
}

impl AnimationFillMode {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "none" => Some(Self::None),
            "forwards" => Some(Self::Forwards),
            "backwards" => Some(Self::Backwards),
            "both" => Some(Self::Both),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Forwards => "forwards",
            Self::Backwards => "backwards",
            Self::Both => "both",
        }
    }
}

/// CSS animation-play-state property (W3C CSS Animations Level 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AnimationPlayState {
    #[default]
    Running,
    Paused,
}

impl AnimationPlayState {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "running" => Some(Self::Running),
            "paused" => Some(Self::Paused),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Paused => "paused",
        }
    }
}

/// CSS animation-iteration-count property (W3C CSS Animations Level 1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AnimationIterationCount {
    Infinite,
    Count(f32),
}

impl Default for AnimationIterationCount {
    fn default() -> Self {
        Self::Count(1.0)
    }
}

impl AnimationIterationCount {
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        if s.eq_ignore_ascii_case("infinite") {
            Some(Self::Infinite)
        } else if let Ok(n) = s.parse::<f32>() {
            if n >= 0.0 {
                Some(Self::Count(n))
            } else {
                None
            }
        } else {
            None
        }
    }
}

/// Target scroll axis for scroll-driven animations (W3C CSS Scroll-Driven Animations Level 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScrollAxis {
    #[default]
    Block,
    Inline,
    Vertical,
    Horizontal,
}

/// Target scroller source container for `scroll()` functional timeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScrollTimelineSource {
    #[default]
    Nearest,
    Root,
    SelfNode,
}

/// CSS animation timeline descriptor per W3C Scroll-Driven Animations Level 1.
#[derive(Debug, Clone, PartialEq)]
#[derive(Default)]
pub enum AnimationTimeline {
    /// Traditional wall-clock time timeline.
    #[default]
    Time,
    /// Container scroll-driven timeline with source scroller and axis.
    Scroll(ScrollTimelineSource, ScrollAxis),
    /// Element visibility in scrollport view timeline with axis and start/end insets.
    View(ScrollAxis, f32, f32),
    /// Named timeline referencing a @scroll-timeline or scroll-timeline-name.
    Named(String),
}


/// Named range anchor for scroll-driven animations per W3C specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AnimationRangeName {
    #[default]
    Normal,
    Cover,
    Contain,
    Entry,
    Exit,
}

/// Start and end range boundaries for scroll-driven animations.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnimationRange {
    pub name: AnimationRangeName,
    pub start_offset: f32,
    pub end_offset: f32,
}

impl Default for AnimationRange {
    fn default() -> Self {
        Self {
            name: AnimationRangeName::Normal,
            start_offset: 0.0,
            end_offset: 1.0,
        }
    }
}

/// Complete CSS animation descriptor specifying name, timing, delay, iterations, direction, fill mode, timeline, and range.
#[derive(Debug, Clone, PartialEq)]
pub struct AnimationSpec {
    pub name: String,
    pub duration_ms: f32,
    pub timing_function: TransitionEasing,
    pub delay_ms: f32,
    pub iteration_count: AnimationIterationCount,
    pub direction: AnimationDirection,
    pub fill_mode: AnimationFillMode,
    pub play_state: AnimationPlayState,
    pub timeline: AnimationTimeline,
    pub range: AnimationRange,
}

impl Default for AnimationSpec {
    fn default() -> Self {
        Self {
            name: String::new(),
            duration_ms: 0.0,
            timing_function: TransitionEasing::Ease,
            delay_ms: 0.0,
            iteration_count: AnimationIterationCount::Count(1.0),
            direction: AnimationDirection::Normal,
            fill_mode: AnimationFillMode::None,
            play_state: AnimationPlayState::Running,
            timeline: AnimationTimeline::Time,
            range: AnimationRange::default(),
        }
    }
}

/// An individual CSS 2D or 3D transform operation.
#[derive(Debug, Clone, PartialEq)]
pub enum TransformOp {
    Translate(Dimension, Dimension),
    Translate3d(Dimension, Dimension, Dimension),
    Scale(f32, f32),
    Scale3d(f32, f32, f32),
    Rotate(f32), // Radians (around Z)
    RotateX(f32),
    RotateY(f32),
    RotateZ(f32),
    Rotate3d(f32, f32, f32, f32), // x, y, z, rad
    Skew(f32, f32),               // Radians (ax, ay)
    Perspective(f32),             // Pixels
    Matrix([f32; 6]),
    Matrix3d([f32; 16]),
}

impl TransformOp {
    /// Computes the 4x4 matrix for this individual transform operation given element size.
    pub fn to_matrix(&self, width: f32, height: f32) -> Matrix4x4 {
        match self {
            Self::Translate(dx, dy) => {
                let tx = match dx {
                    Dimension::Px(px) => *px,
                    Dimension::Percent(pct) => width * (pct / 100.0),
                    Dimension::Rem(rem) => rem * 16.0,
                    Dimension::Em(em) => em * 16.0,
                    _ => 0.0,
                };
                let ty = match dy {
                    Dimension::Px(px) => *px,
                    Dimension::Percent(pct) => height * (pct / 100.0),
                    Dimension::Rem(rem) => rem * 16.0,
                    Dimension::Em(em) => em * 16.0,
                    _ => 0.0,
                };
                Matrix4x4::translation(tx, ty, 0.0)
            }
            Self::Translate3d(dx, dy, dz) => {
                let tx = match dx {
                    Dimension::Px(px) => *px,
                    Dimension::Percent(pct) => width * (pct / 100.0),
                    Dimension::Rem(rem) => rem * 16.0,
                    Dimension::Em(em) => em * 16.0,
                    _ => 0.0,
                };
                let ty = match dy {
                    Dimension::Px(px) => *px,
                    Dimension::Percent(pct) => height * (pct / 100.0),
                    Dimension::Rem(rem) => rem * 16.0,
                    Dimension::Em(em) => em * 16.0,
                    _ => 0.0,
                };
                let tz = match dz {
                    Dimension::Px(px) => *px,
                    Dimension::Rem(rem) => rem * 16.0,
                    Dimension::Em(em) => em * 16.0,
                    _ => 0.0,
                };
                Matrix4x4::translation(tx, ty, tz)
            }
            Self::Scale(sx, sy) => Matrix4x4::scaling(*sx, *sy, 1.0),
            Self::Scale3d(sx, sy, sz) => Matrix4x4::scaling(*sx, *sy, *sz),
            Self::Rotate(rad) | Self::RotateZ(rad) => Matrix4x4::rotation_z(*rad),
            Self::RotateX(rad) => Matrix4x4::rotation_x(*rad),
            Self::RotateY(rad) => Matrix4x4::rotation_y(*rad),
            Self::Rotate3d(x, y, z, rad) => Matrix4x4::rotation_axis(*x, *y, *z, *rad),
            Self::Skew(ax, ay) => Matrix4x4::skew(*ax, *ay),
            Self::Perspective(d) => Matrix4x4::perspective(*d),
            Self::Matrix(m) => Matrix4x4::from_2d(m[0], m[1], m[2], m[3], m[4], m[5]),
            Self::Matrix3d(m) => Matrix4x4::from_array(*m),
        }
    }

    /// Returns the identity equivalent for this operation type per CSS Transitions.
    pub fn to_identity(&self) -> Self {
        match self {
            Self::Translate(..) => Self::Translate(Dimension::Px(0.0), Dimension::Px(0.0)),
            Self::Translate3d(..) => {
                Self::Translate3d(Dimension::Px(0.0), Dimension::Px(0.0), Dimension::Px(0.0))
            }
            Self::Scale(..) => Self::Scale(1.0, 1.0),
            Self::Scale3d(..) => Self::Scale3d(1.0, 1.0, 1.0),
            Self::Rotate(..) => Self::Rotate(0.0),
            Self::RotateX(..) => Self::RotateX(0.0),
            Self::RotateY(..) => Self::RotateY(0.0),
            Self::RotateZ(..) => Self::RotateZ(0.0),
            Self::Rotate3d(x, y, z, _) => Self::Rotate3d(*x, *y, *z, 0.0),
            Self::Skew(..) => Self::Skew(0.0, 0.0),
            Self::Perspective(d) => Self::Perspective(*d),
            Self::Matrix(..) => Self::Matrix([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]),
            Self::Matrix3d(..) => Self::Matrix3d([
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ]),
        }
    }
}

/// The origin point around which transforms are applied.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TransformOrigin {
    pub x: Dimension,
    pub y: Dimension,
    pub z: f32,
}

impl Default for TransformOrigin {
    fn default() -> Self {
        Self {
            x: Dimension::Percent(50.0),
            y: Dimension::Percent(50.0),
            z: 0.0,
        }
    }
}

/// CSS transform-style property defining 3D rendering context hierarchy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TransformStyle {
    #[default]
    Flat,
    Preserve3d,
}

/// CSS backface-visibility property determining if reverse side of 3D element is rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BackfaceVisibility {
    #[default]
    Visible,
    Hidden,
}

/// Computes the composite 4x4 matrix for a list of transform operations relative to `origin`.
pub fn compute_transform_matrix(
    ops: &[TransformOp],
    width: f32,
    height: f32,
    origin: &TransformOrigin,
) -> Matrix4x4 {
    if ops.is_empty() {
        return Matrix4x4::identity();
    }

    let ox = match origin.x {
        Dimension::Px(px) => px,
        Dimension::Percent(pct) => width * (pct / 100.0),
        Dimension::Rem(rem) => rem * 16.0,
        Dimension::Em(em) => em * 16.0,
        _ => width * 0.5,
    };
    let oy = match origin.y {
        Dimension::Px(px) => px,
        Dimension::Percent(pct) => height * (pct / 100.0),
        Dimension::Rem(rem) => rem * 16.0,
        Dimension::Em(em) => em * 16.0,
        _ => height * 0.5,
    };
    let oz = origin.z;

    let t_origin = Matrix4x4::translation(ox, oy, oz);
    let t_origin_inv = Matrix4x4::translation(-ox, -oy, -oz);

    let mut combined_ops = Matrix4x4::identity();
    for op in ops {
        combined_ops = combined_ops.multiply(&op.to_matrix(width, height));
    }

    t_origin.multiply(&combined_ops).multiply(&t_origin_inv)
}

/// Fully computed and resolved styles for an element node.
#[derive(Debug, Clone, PartialEq)]
pub struct ComputedStyle {
    // Layout Display & Box Model
    pub display: Display,
    pub position: Position,
    pub float: Float,
    pub clear: Clear,
    pub box_sizing: BoxSizing,
    pub content_visibility: ContentVisibility,
    pub contain: Containment,
    pub contain_intrinsic_width: Option<Dimension>,
    pub contain_intrinsic_height: Option<Dimension>,
    pub width: Dimension,
    pub height: Dimension,
    pub min_width: Dimension,
    pub max_width: Dimension,
    pub min_height: Dimension,
    pub max_height: Dimension,
    pub margin: EdgeInsets<f32>,
    pub margin_auto_top: bool,
    pub margin_auto_right: bool,
    pub margin_auto_bottom: bool,
    pub margin_auto_left: bool,
    pub padding: EdgeInsets<f32>,
    pub insets: EdgeInsets<Dimension>, // top, right, bottom, left
    pub z_index: Option<i32>,

    // Flexbox
    pub flex_direction: FlexDirection,
    pub flex_wrap: FlexWrap,
    pub justify_content: JustifyContent,
    pub align_items: AlignItems,
    pub align_self: AlignSelf,
    pub flex_grow: f32,
    pub flex_shrink: f32,
    pub flex_basis: Dimension,
    pub gap: f32,
    pub gap_row: f32,
    pub gap_col: f32,

    // CSS Grid
    pub grid_template_columns: Vec<GridTrack>,
    pub grid_template_rows: Vec<GridTrack>,
    pub grid_auto_flow: GridAutoFlow,
    pub grid_column_start: Option<i32>,
    pub grid_column_span: Option<u32>,
    pub grid_row_start: Option<i32>,
    pub grid_row_span: Option<u32>,

    // CSS Table
    pub table_layout: TableLayout,
    pub border_collapse: BorderCollapse,
    pub border_spacing: (f32, f32),
    pub caption_side: CaptionSide,
    pub colspan: u32,
    pub rowspan: u32,

    // Visuals & Backgrounds
    pub background_color: Option<Color>,
    pub background_gradient: Option<Gradient>,
    pub border_color: Option<Color>,
    pub border_width: EdgeInsets<f32>,
    pub border_radius: BorderRadius,
    pub opacity: f32,
    pub box_shadow: Vec<BoxShadow>,
    pub overflow_x: Overflow,
    pub overflow_y: Overflow,

    // Typography (Inheritable)
    pub color: Color,
    pub font_family: AtomId,
    pub font_size: f32,
    pub root_font_size: f32,
    pub font_weight: FontWeight,
    pub font_style: FontStyle,
    pub text_align: TextAlign,
    pub line_height: f32,
    pub letter_spacing: f32,
    pub text_decoration: TextDecoration,
    pub text_transform: TextTransform,
    pub text_overflow: TextOverflow,
    pub white_space: WhiteSpace,
    pub vertical_align: VerticalAlign,

    // Interaction & Behavior (Inheritable)
    pub cursor: Cursor,
    pub user_select: UserSelect,
    pub pointer_events: PointerEvents,
    pub visibility: Visibility,
    pub caret_color: Option<Color>,

    // Border & Outline Styles
    pub border_style: BorderStyle,
    pub outline_width: f32,
    pub outline_style: BorderStyle,
    pub outline_color: Option<Color>,
    pub outline_offset: f32,

    // Aspect Ratio & Layout
    pub aspect_ratio: Option<f32>,

    // Visual Effects
    pub backdrop_filter: Option<crate::effects::BackdropFilter>,
    pub filter: Option<crate::effects::BackdropFilter>,
    pub clip_path: Option<crate::effects::ClipPath>,
    pub mix_blend_mode: BlendMode,
    pub isolation: Isolation,

    // Transitions (W3C CSS Transitions Level 1 & 2)
    pub transitions: Vec<TransitionSpec>,

    // Animations (W3C CSS Animations Level 1 & 2)
    pub animations: Vec<AnimationSpec>,

    // CSS 2D & 3D Transforms (Level 1 & Level 2)
    pub transform: Vec<TransformOp>,
    pub transform_origin: TransformOrigin,
    pub transform_style: TransformStyle,
    pub perspective: Option<f32>,
    pub perspective_origin: (Dimension, Dimension),
    pub backface_visibility: BackfaceVisibility,

    // CSS Custom Properties (Inheritable - Copy-On-Write Arc)
    pub custom_properties: Option<CustomPropertyMap>,

    // Next-Gen CSS Standards (Milestone 48)
    pub starting_style: Option<Box<ComputedStyle>>,
    pub view_transition_name: Option<String>,
    pub animation_timeline: AnimationTimeline,
    pub animation_range: AnimationRange,
}

impl Default for ComputedStyle {
    fn default() -> Self {
        Self {
            display: Display::Block,
            position: Position::Static,
            float: Float::None,
            clear: Clear::None,
            box_sizing: BoxSizing::BorderBox,
            content_visibility: ContentVisibility::Visible,
            contain: Containment::NONE,
            contain_intrinsic_width: None,
            contain_intrinsic_height: None,
            width: Dimension::Auto,
            height: Dimension::Auto,
            min_width: Dimension::Auto,
            max_width: Dimension::Auto,
            min_height: Dimension::Auto,
            max_height: Dimension::Auto,
            margin: EdgeInsets::ZERO,
            margin_auto_top: false,
            margin_auto_right: false,
            margin_auto_bottom: false,
            margin_auto_left: false,
            padding: EdgeInsets::ZERO,
            insets: EdgeInsets::new(
                Dimension::Auto,
                Dimension::Auto,
                Dimension::Auto,
                Dimension::Auto,
            ),
            z_index: None,

            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::NoWrap,
            justify_content: JustifyContent::FlexStart,
            align_items: AlignItems::Stretch,
            align_self: AlignSelf::Auto,
            flex_grow: 0.0,
            flex_shrink: 1.0,
            flex_basis: Dimension::Auto,
            gap: 0.0,
            gap_row: 0.0,
            gap_col: 0.0,

            grid_template_columns: Vec::new(),
            grid_template_rows: Vec::new(),
            grid_auto_flow: GridAutoFlow::Row,
            grid_column_start: None,
            grid_column_span: None,
            grid_row_start: None,
            grid_row_span: None,

            table_layout: TableLayout::Auto,
            border_collapse: BorderCollapse::Separate,
            border_spacing: (0.0, 0.0),
            caption_side: CaptionSide::Top,
            colspan: 1,
            rowspan: 1,

            background_color: None,
            background_gradient: None,
            border_color: None,
            border_width: EdgeInsets::ZERO,
            border_radius: BorderRadius::ZERO,
            opacity: 1.0,
            box_shadow: Vec::new(),
            overflow_x: Overflow::Visible,
            overflow_y: Overflow::Visible,

            color: Color::BLACK,
            font_family: AtomId::FONT_SANS,
            font_size: 16.0,
            root_font_size: 16.0,
            font_weight: FontWeight::Normal,
            font_style: FontStyle::Normal,
            text_align: TextAlign::Left,
            line_height: 1.2,
            letter_spacing: 0.0,
            text_decoration: TextDecoration::None,
            text_transform: TextTransform::None,
            text_overflow: TextOverflow::Clip,
            white_space: WhiteSpace::Normal,
            vertical_align: VerticalAlign::Baseline,

            cursor: Cursor::Default,
            user_select: UserSelect::Auto,
            pointer_events: PointerEvents::Auto,
            visibility: Visibility::Visible,
            caret_color: None,

            border_style: BorderStyle::Solid,
            outline_width: 0.0,
            outline_style: BorderStyle::None,
            outline_color: None,
            outline_offset: 0.0,

            aspect_ratio: None,

            backdrop_filter: None,
            filter: None,
            clip_path: None,
            mix_blend_mode: BlendMode::Normal,
            isolation: Isolation::Auto,
            transitions: Vec::new(),
            animations: Vec::new(),

            transform: Vec::new(),
            transform_origin: TransformOrigin::default(),
            transform_style: TransformStyle::default(),
            perspective: None,
            perspective_origin: (Dimension::Percent(50.0), Dimension::Percent(50.0)),
            backface_visibility: BackfaceVisibility::default(),

            custom_properties: None,

            starting_style: None,
            view_transition_name: None,
            animation_timeline: AnimationTimeline::Time,
            animation_range: AnimationRange::default(),
        }
    }
}

impl ComputedStyle {
    /// Returns true if this style defines any animations.
    #[inline(always)]
    pub fn has_animations(&self) -> bool {
        !self.animations.is_empty()
    }

    /// Returns true if this style defines any 2D or 3D transform operations.
    #[inline(always)]
    pub fn has_transform(&self) -> bool {
        !self.transform.is_empty()
    }

    /// Returns true if this style defines any visual filters, backdrop filters, clip-paths, or blend modes.
    #[inline(always)]
    pub fn has_visual_effects(&self) -> bool {
        self.mix_blend_mode != BlendMode::Normal
            || self.isolation == Isolation::Isolate
            || self.filter.as_ref().is_some_and(|f| !f.is_empty())
            || self
                .backdrop_filter
                .as_ref()
                .is_some_and(|b| !b.is_empty())
            || self.clip_path.as_ref().is_some_and(|c| !c.is_none())
    }

    /// Computes the composite 4x4 matrix for this element's transform given layout width and height.
    #[inline(always)]
    pub fn compute_transform_matrix(&self, width: f32, height: f32) -> Matrix4x4 {
        compute_transform_matrix(&self.transform, width, height, &self.transform_origin)
    }

    /// Evaluates if this element establishes an isolated stacking context per W3C CSS 2.1 Appendix E and CSS Compositing Level 1.
    pub fn establishes_stacking_context(&self) -> bool {
        self.has_transform()
            || self.opacity < 1.0
            || self.z_index.is_some()
            || self.filter.is_some()
            || self.backdrop_filter.is_some()
            || self.clip_path.is_some()
            || self.mix_blend_mode != BlendMode::Normal
            || self.isolation == Isolation::Isolate
    }
    /// Constructs an unstyled node inheriting inheritable properties from parent with zero heap allocations.
    pub fn inherit_from(parent: &Self) -> Self {
        // Inheritable properties per W3C CSS Cascading and Inheritance Level 4:
        Self {
            color: parent.color,
            font_family: parent.font_family,
            font_size: parent.font_size,
            root_font_size: parent.root_font_size,
            font_weight: parent.font_weight,
            font_style: parent.font_style,
            text_align: parent.text_align,
            line_height: parent.line_height,
            letter_spacing: parent.letter_spacing,
            text_decoration: parent.text_decoration,
            text_transform: parent.text_transform,
            text_overflow: parent.text_overflow,
            white_space: parent.white_space,
            cursor: parent.cursor,
            user_select: parent.user_select,
            visibility: parent.visibility,
            caret_color: parent.caret_color,
            border_collapse: parent.border_collapse,
            border_spacing: parent.border_spacing,
            caption_side: parent.caption_side,
            custom_properties: parent.custom_properties.clone(),
            ..Self::default()
        }
    }

    /// Looks up a CSS custom property by AtomId.
    pub fn get_custom_property(&self, key: AtomId) -> Option<AtomId> {
        let map = self.custom_properties.as_ref()?;
        for (k, v) in map.iter().rev() {
            if *k == key {
                return Some(*v);
            }
        }
        None
    }

    /// Sets or overrides a CSS custom property using Copy-On-Write semantics.
    pub fn set_custom_property(&mut self, key: AtomId, value: AtomId) {
        let mut list = match &self.custom_properties {
            Some(arc) => (**arc).clone(),
            None => Vec::with_capacity(4),
        };
        if let Some(pos) = list.iter().position(|(k, _)| *k == key) {
            list[pos].1 = value;
        } else {
            list.push((key, value));
        }
        self.custom_properties = Some(Arc::new(list));
    }
}
