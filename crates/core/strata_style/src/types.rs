//! Core CSS style types, layout enums, and ComputedStyle representation.

pub use strata_primitives::color::Rgba;
pub use strata_primitives::geom::EdgeInsets;

pub type Color = Rgba;

/// Box layout display model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Display {
    #[default]
    Block,
    Inline,
    InlineBlock,
    Flex,
    InlineFlex,
    Grid,
    InlineGrid,
    None,
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
}

impl Dimension {
    /// Resolves the dimension into concrete pixels given a context.
    pub fn resolve_px(&self, parent_size: f32, root_font_size: f32, current_font_size: f32) -> Option<f32> {
        match *self {
            Self::Auto => None,
            Self::Px(px) => Some(px),
            Self::Percent(pct) => Some(parent_size * (pct / 100.0)),
            Self::Vw(vw) => Some(vw), // Handled by viewport context in layout
            Self::Vh(vh) => Some(vh),
            Self::Rem(rem) => Some(rem * root_font_size),
            Self::Em(em) => Some(em * current_font_size),
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

/// CSS Grid track sizing definition.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GridTrack {
    Auto,
    Px(f32),
    Percent(f32),
    Fr(f32),
    MinContent,
    MaxContent,
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
        Self { top_left, top_right, bottom_right, bottom_left }
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

/// Fully computed and resolved styles for an element node.
#[derive(Debug, Clone, PartialEq)]
pub struct ComputedStyle {
    // Layout Display & Box Model
    pub display: Display,
    pub position: Position,
    pub box_sizing: BoxSizing,
    pub width: Dimension,
    pub height: Dimension,
    pub min_width: Dimension,
    pub max_width: Dimension,
    pub min_height: Dimension,
    pub max_height: Dimension,
    pub margin: EdgeInsets<f32>,
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
    pub grid_column_start: Option<i32>,
    pub grid_column_span: Option<u32>,
    pub grid_row_start: Option<i32>,
    pub grid_row_span: Option<u32>,

    // Visuals & Backgrounds
    pub background_color: Option<Color>,
    pub border_color: Option<Color>,
    pub border_width: EdgeInsets<f32>,
    pub border_radius: BorderRadius,
    pub opacity: f32,
    pub box_shadow: Vec<BoxShadow>,
    pub overflow_x: Overflow,
    pub overflow_y: Overflow,

    // Typography (Inheritable)
    pub color: Color,
    pub font_size: f32,
    pub font_weight: FontWeight,
    pub text_align: TextAlign,
    pub line_height: f32,
    pub letter_spacing: f32,
}

impl Default for ComputedStyle {
    fn default() -> Self {
        Self {
            display: Display::Block,
            position: Position::Static,
            box_sizing: BoxSizing::BorderBox,
            width: Dimension::Auto,
            height: Dimension::Auto,
            min_width: Dimension::Auto,
            max_width: Dimension::Auto,
            min_height: Dimension::Auto,
            max_height: Dimension::Auto,
            margin: EdgeInsets::ZERO,
            padding: EdgeInsets::ZERO,
            insets: EdgeInsets::new(Dimension::Auto, Dimension::Auto, Dimension::Auto, Dimension::Auto),
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
            grid_column_start: None,
            grid_column_span: None,
            grid_row_start: None,
            grid_row_span: None,

            background_color: None,
            border_color: None,
            border_width: EdgeInsets::ZERO,
            border_radius: BorderRadius::ZERO,
            opacity: 1.0,
            box_shadow: Vec::new(),
            overflow_x: Overflow::Visible,
            overflow_y: Overflow::Visible,

            color: Color::BLACK,
            font_size: 16.0,
            font_weight: FontWeight::Normal,
            text_align: TextAlign::Left,
            line_height: 1.5,
            letter_spacing: 0.0,
        }
    }
}

impl ComputedStyle {
    /// Constructs an unstyled node inheriting inheritable properties from parent.
    pub fn inherit_from(parent: &Self) -> Self {
        let mut child = Self::default();
        // Inheritable properties per W3C CSS Cascading and Inheritance Level 4:
        child.color = parent.color;
        child.font_size = parent.font_size;
        child.font_weight = parent.font_weight;
        child.text_align = parent.text_align;
        child.line_height = parent.line_height;
        child.letter_spacing = parent.letter_spacing;
        child
    }
}
