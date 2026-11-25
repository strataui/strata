//! Standards-compliant CSS serializer for `strata_style::ComputedStyle` over the C ABI.

use std::fmt::Write as _;

use strata_style::types::{
    AlignItems, Color, ComputedStyle, Dimension, Display, FlexDirection, FontWeight, JustifyContent,
    Overflow, Position,
};

fn format_number(v: f32) -> String {
    if (v - v.round()).abs() < 1e-4 {
        format!("{}", v.round() as i64)
    } else {
        let s = format!("{v:.4}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

/// Formats a [`Dimension`] into a valid W3C CSS length token.
pub fn format_css_dimension(dim: Dimension) -> Option<String> {
    match dim {
        Dimension::Auto => None,
        Dimension::Px(px) => Some(format!("{}px", format_number(px))),
        Dimension::Percent(pct) => Some(format!("{}%", format_number(pct))),
        Dimension::Vw(vw) => Some(format!("{}vw", format_number(vw))),
        Dimension::Vh(vh) => Some(format!("{}vh", format_number(vh))),
        Dimension::Rem(rem) => Some(format!("{}rem", format_number(rem))),
        Dimension::Em(em) => Some(format!("{}em", format_number(em))),
        Dimension::Calc(_) => Some("auto".to_string()),
        Dimension::MinContent => Some("min-content".to_string()),
        Dimension::MaxContent => Some("max-content".to_string()),
        Dimension::FitContent(px) => Some(format!("fit-content({}px)", format_number(px))),
    }
}

/// Formats a [`Color`] into a valid CSS `rgba(r, g, b, a)` string with normalized alpha `[0, 1]`.
pub fn format_css_color(c: Color) -> String {
    if c.a == 255 {
        format!("rgba({}, {}, {}, 1)", c.r, c.g, c.b)
    } else if c.a == 0 {
        format!("rgba({}, {}, {}, 0)", c.r, c.g, c.b)
    } else {
        let alpha = (c.a as f32) / 255.0;
        let alpha_s = format!("{alpha:.3}");
        let trimmed = alpha_s.trim_end_matches('0').trim_end_matches('.');
        format!("rgba({}, {}, {}, {})", c.r, c.g, c.b, trimmed)
    }
}

fn format_display(d: Display) -> Option<&'static str> {
    match d {
        Display::Block => None,
        Display::Inline => Some("inline"),
        Display::InlineBlock => Some("inline-block"),
        Display::ListItem => Some("list-item"),
        Display::Flex => Some("flex"),
        Display::InlineFlex => Some("inline-flex"),
        Display::Grid => Some("grid"),
        Display::InlineGrid => Some("inline-grid"),
        Display::Table => Some("table"),
        Display::InlineTable => Some("inline-table"),
        Display::TableRowGroup => Some("table-row-group"),
        Display::TableHeaderGroup => Some("table-header-group"),
        Display::TableFooterGroup => Some("table-footer-group"),
        Display::TableRow => Some("table-row"),
        Display::TableCell => Some("table-cell"),
        Display::TableCaption => Some("table-caption"),
        Display::TableColumnGroup => Some("table-column-group"),
        Display::TableColumn => Some("table-column"),
        Display::None => Some("none"),
    }
}

fn format_position(p: Position) -> Option<&'static str> {
    match p {
        Position::Static => None,
        Position::Relative => Some("relative"),
        Position::Absolute => Some("absolute"),
        Position::Fixed => Some("fixed"),
        Position::Sticky => Some("sticky"),
    }
}

fn format_flex_dir(d: FlexDirection) -> Option<&'static str> {
    match d {
        FlexDirection::Row => None,
        FlexDirection::RowReverse => Some("row-reverse"),
        FlexDirection::Column => Some("column"),
        FlexDirection::ColumnReverse => Some("column-reverse"),
    }
}

fn format_justify(j: JustifyContent) -> Option<&'static str> {
    match j {
        JustifyContent::FlexStart => None,
        JustifyContent::FlexEnd => Some("flex-end"),
        JustifyContent::Center => Some("center"),
        JustifyContent::SpaceBetween => Some("space-between"),
        JustifyContent::SpaceAround => Some("space-around"),
        JustifyContent::SpaceEvenly => Some("space-evenly"),
    }
}

fn format_align(a: AlignItems) -> Option<&'static str> {
    match a {
        AlignItems::Stretch => None,
        AlignItems::FlexStart => Some("flex-start"),
        AlignItems::FlexEnd => Some("flex-end"),
        AlignItems::Center => Some("center"),
        AlignItems::Baseline => Some("baseline"),
    }
}

fn format_overflow(o: Overflow) -> Option<&'static str> {
    match o {
        Overflow::Visible => None,
        Overflow::Hidden => Some("hidden"),
        Overflow::Scroll => Some("scroll"),
        Overflow::Auto => Some("auto"),
    }
}

/// Serializes a resolved [`ComputedStyle`] into standards-compliant inline CSS declarations.
pub fn serialize_computed_style_css(style: &ComputedStyle) -> String {
    let defaults = ComputedStyle::default();
    let mut out = String::new();

    if let Some(d) = format_display(style.display) {
        let _ = write!(out, "display: {d}; ");
    }
    if let Some(p) = format_position(style.position) {
        let _ = write!(out, "position: {p}; ");
    }
    if let Some(fd) = format_flex_dir(style.flex_direction) {
        let _ = write!(out, "flex-direction: {fd}; ");
    }
    if let Some(jc) = format_justify(style.justify_content) {
        let _ = write!(out, "justify-content: {jc}; ");
    }
    if let Some(ai) = format_align(style.align_items) {
        let _ = write!(out, "align-items: {ai}; ");
    }
    if style.gap > 0.0 {
        let _ = write!(out, "gap: {}px; ", format_number(style.gap));
    }
    if let Some(w) = format_css_dimension(style.width) {
        let _ = write!(out, "width: {w}; ");
    }
    if let Some(h) = format_css_dimension(style.height) {
        let _ = write!(out, "height: {h}; ");
    }
    if let Some(mw) = format_css_dimension(style.min_width) {
        let _ = write!(out, "min-width: {mw}; ");
    }
    if let Some(xw) = format_css_dimension(style.max_width) {
        let _ = write!(out, "max-width: {xw}; ");
    }
    if let Some(mh) = format_css_dimension(style.min_height) {
        let _ = write!(out, "min-height: {mh}; ");
    }
    if let Some(xh) = format_css_dimension(style.max_height) {
        let _ = write!(out, "max-height: {xh}; ");
    }
    if style.padding.top != 0.0
        || style.padding.right != 0.0
        || style.padding.bottom != 0.0
        || style.padding.left != 0.0
    {
        let _ = write!(
            out,
            "padding: {}px {}px {}px {}px; ",
            format_number(style.padding.top),
            format_number(style.padding.right),
            format_number(style.padding.bottom),
            format_number(style.padding.left),
        );
    }
    if style.margin.top != 0.0
        || style.margin.right != 0.0
        || style.margin.bottom != 0.0
        || style.margin.left != 0.0
    {
        let _ = write!(
            out,
            "margin: {}px {}px {}px {}px; ",
            format_number(style.margin.top),
            format_number(style.margin.right),
            format_number(style.margin.bottom),
            format_number(style.margin.left),
        );
    }
    if style.border_radius.top_left > 0.0
        || style.border_radius.top_right > 0.0
        || style.border_radius.bottom_right > 0.0
        || style.border_radius.bottom_left > 0.0
    {
        let _ = write!(
            out,
            "border-radius: {}px {}px {}px {}px; ",
            format_number(style.border_radius.top_left),
            format_number(style.border_radius.top_right),
            format_number(style.border_radius.bottom_right),
            format_number(style.border_radius.bottom_left),
        );
    }
    if style.border_width.top > 0.0
        || style.border_width.right > 0.0
        || style.border_width.bottom > 0.0
        || style.border_width.left > 0.0
    {
        let _ = write!(
            out,
            "border-width: {}px {}px {}px {}px; ",
            format_number(style.border_width.top),
            format_number(style.border_width.right),
            format_number(style.border_width.bottom),
            format_number(style.border_width.left),
        );
    }
    if let Some(bc) = style.border_color {
        let _ = write!(out, "border-color: {}; ", format_css_color(bc));
    }
    if let Some(bg) = style.background_color {
        let _ = write!(out, "background-color: {}; ", format_css_color(bg));
    }
    let _ = write!(out, "color: {}; ", format_css_color(style.color));
    if (style.font_size - defaults.font_size).abs() > 1e-4 {
        let _ = write!(out, "font-size: {}px; ", format_number(style.font_size));
    }
    if style.font_weight != FontWeight::Normal {
        let _ = write!(out, "font-weight: {}; ", style.font_weight as u16);
    }
    if (style.opacity - 1.0).abs() > 1e-4 {
        let _ = write!(out, "opacity: {}; ", format_number(style.opacity));
    }
    if let Some(ox) = format_overflow(style.overflow_x) {
        let _ = write!(out, "overflow-x: {ox}; ");
    }
    if let Some(oy) = format_overflow(style.overflow_y) {
        let _ = write!(out, "overflow-y: {oy}; ");
    }
    if let Some(z) = style.z_index {
        let _ = write!(out, "z-index: {z}; ");
    }

    out.trim_end().to_string()
}
