//! High-performance Tailwind CSS v3/v4 JIT Compiler and resolver.

use crate::palette::lookup_tailwind_color;
use crate::types::*;
use strata_primitives::geom::EdgeInsets;

/// High-speed Tailwind CSS JIT compiler and resolver.
pub struct TailwindEngine;

impl TailwindEngine {
    /// Resolves a space-delimited string of Tailwind classes into a `ComputedStyle`.
    pub fn resolve_classes(classes: &str) -> ComputedStyle {
        Self::resolve_classes_with_state(classes, false, false, false)
    }

    /// Resolves Tailwind classes evaluating pseudo-classes (`hover:`, `focus:`, `dark:`).
    pub fn resolve_classes_with_state(
        classes: &str,
        is_hovered: bool,
        is_focused: bool,
        is_dark_mode: bool,
    ) -> ComputedStyle {
        let mut style = ComputedStyle::default();

        for raw_token in classes.split_whitespace() {
            let (modifier, class) = split_modifier(raw_token);

            let should_apply = match modifier {
                None => true,
                Some("hover") => is_hovered,
                Some("focus") => is_focused,
                Some("dark") => is_dark_mode,
                Some(_) => false, // sm:, md:, lg: can be activated with media contexts
            };

            if should_apply {
                Self::apply_class(&mut style, class);
            }
        }

        style
    }

    /// Applies a single, non-modified Tailwind class to `ComputedStyle`.
    pub fn apply_class(style: &mut ComputedStyle, class: &str) {
        // 1. Display & Layout Mode
        match class {
            "flex" => {
                style.display = Display::Flex;
                return;
            }
            "inline-flex" => {
                style.display = Display::InlineFlex;
                return;
            }
            "grid" => {
                style.display = Display::Grid;
                return;
            }
            "inline-grid" => {
                style.display = Display::InlineGrid;
                return;
            }
            "block" => {
                style.display = Display::Block;
                return;
            }
            "inline-block" => {
                style.display = Display::InlineBlock;
                return;
            }
            "inline" => {
                style.display = Display::Inline;
                return;
            }
            "hidden" => {
                style.display = Display::None;
                return;
            }
            _ => {}
        }

        // 2. Flexbox Container & Item Props
        match class {
            "flex-row" => {
                style.flex_direction = FlexDirection::Row;
                return;
            }
            "flex-row-reverse" => {
                style.flex_direction = FlexDirection::RowReverse;
                return;
            }
            "flex-col" => {
                style.flex_direction = FlexDirection::Column;
                return;
            }
            "flex-col-reverse" => {
                style.flex_direction = FlexDirection::ColumnReverse;
                return;
            }
            "box-border" => {
                style.box_sizing = BoxSizing::BorderBox;
                return;
            }
            "box-content" => {
                style.box_sizing = BoxSizing::ContentBox;
                return;
            }
            "flex-wrap" => {
                style.flex_wrap = FlexWrap::Wrap;
                return;
            }
            "flex-nowrap" => {
                style.flex_wrap = FlexWrap::NoWrap;
                return;
            }
            "flex-wrap-reverse" => {
                style.flex_wrap = FlexWrap::WrapReverse;
                return;
            }
            "items-start" => {
                style.align_items = AlignItems::FlexStart;
                return;
            }
            "items-center" => {
                style.align_items = AlignItems::Center;
                return;
            }
            "items-end" => {
                style.align_items = AlignItems::FlexEnd;
                return;
            }
            "items-baseline" => {
                style.align_items = AlignItems::Baseline;
                return;
            }
            "items-stretch" => {
                style.align_items = AlignItems::Stretch;
                return;
            }
            "justify-start" => {
                style.justify_content = JustifyContent::FlexStart;
                return;
            }
            "justify-center" => {
                style.justify_content = JustifyContent::Center;
                return;
            }
            "justify-end" => {
                style.justify_content = JustifyContent::FlexEnd;
                return;
            }
            "justify-between" => {
                style.justify_content = JustifyContent::SpaceBetween;
                return;
            }
            "justify-around" => {
                style.justify_content = JustifyContent::SpaceAround;
                return;
            }
            "justify-evenly" => {
                style.justify_content = JustifyContent::SpaceEvenly;
                return;
            }
            "flex-1" => {
                style.flex_grow = 1.0;
                style.flex_shrink = 1.0;
                style.flex_basis = Dimension::Percent(0.0);
                return;
            }
            "flex-auto" => {
                style.flex_grow = 1.0;
                style.flex_shrink = 1.0;
                style.flex_basis = Dimension::Auto;
                return;
            }
            "flex-none" => {
                style.flex_grow = 0.0;
                style.flex_shrink = 0.0;
                style.flex_basis = Dimension::Auto;
                return;
            }
            "grow" | "flex-grow" => {
                style.flex_grow = 1.0;
                return;
            }
            "grow-0" | "flex-grow-0" => {
                style.flex_grow = 0.0;
                return;
            }
            "shrink" | "flex-shrink" => {
                style.flex_shrink = 1.0;
                return;
            }
            "shrink-0" | "flex-shrink-0" => {
                style.flex_shrink = 0.0;
                return;
            }
            _ => {}
        }

        // 3. Grid Columns & Rows
        if let Some(rest) = class.strip_prefix("grid-cols-") {
            if let Ok(cols) = rest.parse::<usize>() {
                style.grid_template_columns = vec![GridTrack::Fr(1.0); cols];
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("grid-rows-") {
            if let Ok(rows) = rest.parse::<usize>() {
                style.grid_template_rows = vec![GridTrack::Fr(1.0); rows];
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("col-span-") {
            if let Ok(span) = rest.parse::<u32>() {
                style.grid_column_span = Some(span);
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("row-span-") {
            if let Ok(span) = rest.parse::<u32>() {
                style.grid_row_span = Some(span);
                return;
            }
        }

        // 4. Spacing: Padding, Margin, Gap
        if let Some(rest) = class.strip_prefix("p-") {
            if let Some(px) = resolve_spacing(rest) {
                style.padding = EdgeInsets::all(px);
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("px-") {
            if let Some(px) = resolve_spacing(rest) {
                style.padding.left = px;
                style.padding.right = px;
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("py-") {
            if let Some(px) = resolve_spacing(rest) {
                style.padding.top = px;
                style.padding.bottom = px;
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("pt-") {
            if let Some(px) = resolve_spacing(rest) {
                style.padding.top = px;
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("pr-") {
            if let Some(px) = resolve_spacing(rest) {
                style.padding.right = px;
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("pb-") {
            if let Some(px) = resolve_spacing(rest) {
                style.padding.bottom = px;
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("pl-") {
            if let Some(px) = resolve_spacing(rest) {
                style.padding.left = px;
                return;
            }
        }

        if let Some(rest) = class.strip_prefix("m-") {
            if let Some(px) = resolve_spacing(rest) {
                style.margin = EdgeInsets::all(px);
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("-m-") {
            if let Some(px) = resolve_spacing(rest) {
                style.margin = EdgeInsets::all(-px);
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("mx-") {
            if rest == "auto" {
                // Auto margins handled in layout solver
                return;
            }
            if let Some(px) = resolve_spacing(rest) {
                style.margin.left = px;
                style.margin.right = px;
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("my-") {
            if let Some(px) = resolve_spacing(rest) {
                style.margin.top = px;
                style.margin.bottom = px;
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("mt-") {
            if let Some(px) = resolve_spacing(rest) {
                style.margin.top = px;
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("mr-") {
            if let Some(px) = resolve_spacing(rest) {
                style.margin.right = px;
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("mb-") {
            if let Some(px) = resolve_spacing(rest) {
                style.margin.bottom = px;
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("ml-") {
            if let Some(px) = resolve_spacing(rest) {
                style.margin.left = px;
                return;
            }
        }

        if let Some(rest) = class.strip_prefix("gap-") {
            if let Some(px) = resolve_spacing(rest) {
                style.gap = px;
                style.gap_row = px;
                style.gap_col = px;
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("gap-x-") {
            if let Some(px) = resolve_spacing(rest) {
                style.gap_col = px;
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("gap-y-") {
            if let Some(px) = resolve_spacing(rest) {
                style.gap_row = px;
                return;
            }
        }

        // 5. Sizing: Width and Height
        if class == "w-full" {
            style.width = Dimension::Percent(100.0);
            return;
        }
        if class == "w-screen" {
            style.width = Dimension::Vw(100.0);
            return;
        }
        if class == "w-auto" {
            style.width = Dimension::Auto;
            return;
        }
        if let Some(rest) = class.strip_prefix("w-") {
            if let Some(dim) = resolve_sizing_dimension(rest) {
                style.width = dim;
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("min-w-") {
            if let Some(dim) = resolve_sizing_dimension(rest) {
                style.min_width = dim;
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("max-w-") {
            if let Some(dim) = resolve_sizing_dimension(rest) {
                style.max_width = dim;
                return;
            }
        }

        if class == "h-full" {
            style.height = Dimension::Percent(100.0);
            return;
        }
        if class == "h-screen" {
            style.height = Dimension::Vh(100.0);
            return;
        }
        if class == "h-auto" {
            style.height = Dimension::Auto;
            return;
        }
        if let Some(rest) = class.strip_prefix("h-") {
            if let Some(dim) = resolve_sizing_dimension(rest) {
                style.height = dim;
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("min-h-") {
            if let Some(dim) = resolve_sizing_dimension(rest) {
                style.min_height = dim;
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("max-h-") {
            if let Some(dim) = resolve_sizing_dimension(rest) {
                style.max_height = dim;
                return;
            }
        }

        // 6. Colors: Background, Text, Border
        if let Some(rest) = class.strip_prefix("bg-") {
            let (color_part, opacity) = split_opacity(rest);
            if let Some(c) = lookup_color_or_arbitrary(color_part, opacity) {
                style.background_color = Some(c);
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("text-") {
            let (color_part, opacity) = split_opacity(rest);
            if let Some(c) = lookup_color_or_arbitrary(color_part, opacity) {
                style.color = c;
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("border-") {
            let (color_part, opacity) = split_opacity(rest);
            if let Some(c) = lookup_color_or_arbitrary(color_part, opacity) {
                style.border_color = Some(c);
                return;
            }
        }

        // 7. Typography: Sizes, Weights, Alignments
        match class {
            "text-xs" => {
                style.font_size = 12.0;
                style.line_height = 16.0 / 12.0;
                return;
            }
            "text-sm" => {
                style.font_size = 14.0;
                style.line_height = 20.0 / 14.0;
                return;
            }
            "text-base" => {
                style.font_size = 16.0;
                style.line_height = 24.0 / 16.0;
                return;
            }
            "text-lg" => {
                style.font_size = 18.0;
                style.line_height = 28.0 / 18.0;
                return;
            }
            "text-xl" => {
                style.font_size = 20.0;
                style.line_height = 28.0 / 20.0;
                return;
            }
            "text-2xl" => {
                style.font_size = 24.0;
                style.line_height = 32.0 / 24.0;
                return;
            }
            "text-3xl" => {
                style.font_size = 30.0;
                style.line_height = 36.0 / 30.0;
                return;
            }
            "text-4xl" => {
                style.font_size = 36.0;
                style.line_height = 40.0 / 36.0;
                return;
            }
            "text-5xl" => {
                style.font_size = 48.0;
                style.line_height = 48.0 / 48.0;
                return;
            }
            "font-thin" => {
                style.font_weight = FontWeight::Thin;
                return;
            }
            "font-extralight" => {
                style.font_weight = FontWeight::ExtraLight;
                return;
            }
            "font-light" => {
                style.font_weight = FontWeight::Light;
                return;
            }
            "font-normal" => {
                style.font_weight = FontWeight::Normal;
                return;
            }
            "font-medium" => {
                style.font_weight = FontWeight::Medium;
                return;
            }
            "font-semibold" => {
                style.font_weight = FontWeight::SemiBold;
                return;
            }
            "font-bold" => {
                style.font_weight = FontWeight::Bold;
                return;
            }
            "font-extrabold" => {
                style.font_weight = FontWeight::ExtraBold;
                return;
            }
            "font-black" => {
                style.font_weight = FontWeight::Black;
                return;
            }
            "text-left" => {
                style.text_align = TextAlign::Left;
                return;
            }
            "text-center" => {
                style.text_align = TextAlign::Center;
                return;
            }
            "text-right" => {
                style.text_align = TextAlign::Right;
                return;
            }
            "text-justify" => {
                style.text_align = TextAlign::Justify;
                return;
            }
            "leading-none" => {
                style.line_height = 1.0;
                return;
            }
            "leading-tight" => {
                style.line_height = 1.25;
                return;
            }
            "leading-snug" => {
                style.line_height = 1.375;
                return;
            }
            "leading-normal" => {
                style.line_height = 1.5;
                return;
            }
            "leading-relaxed" => {
                style.line_height = 1.625;
                return;
            }
            "leading-loose" => {
                style.line_height = 2.0;
                return;
            }
            _ => {}
        }

        // 8. Borders & Radii
        match class {
            "border" => {
                style.border_width = EdgeInsets::all(1.0);
                return;
            }
            "border-t" => {
                style.border_width.top = 1.0;
                return;
            }
            "border-b" => {
                style.border_width.bottom = 1.0;
                return;
            }
            "border-l" => {
                style.border_width.left = 1.0;
                return;
            }
            "border-r" => {
                style.border_width.right = 1.0;
                return;
            }
            "border-x" => {
                style.border_width.left = 1.0;
                style.border_width.right = 1.0;
                return;
            }
            "border-y" => {
                style.border_width.top = 1.0;
                style.border_width.bottom = 1.0;
                return;
            }
            "border-0" => {
                style.border_width = EdgeInsets::ZERO;
                return;
            }
            "border-2" => {
                style.border_width = EdgeInsets::all(2.0);
                return;
            }
            "border-4" => {
                style.border_width = EdgeInsets::all(4.0);
                return;
            }
            "border-8" => {
                style.border_width = EdgeInsets::all(8.0);
                return;
            }
            "rounded-none" => {
                style.border_radius = BorderRadius::ZERO;
                return;
            }
            "rounded-sm" => {
                style.border_radius = BorderRadius::all(2.0);
                return;
            }
            "rounded" => {
                style.border_radius = BorderRadius::all(4.0);
                return;
            }
            "rounded-md" => {
                style.border_radius = BorderRadius::all(6.0);
                return;
            }
            "rounded-lg" => {
                style.border_radius = BorderRadius::all(8.0);
                return;
            }
            "rounded-xl" => {
                style.border_radius = BorderRadius::all(12.0);
                return;
            }
            "rounded-2xl" => {
                style.border_radius = BorderRadius::all(16.0);
                return;
            }
            "rounded-3xl" => {
                style.border_radius = BorderRadius::all(24.0);
                return;
            }
            "rounded-full" => {
                style.border_radius = BorderRadius::all(9999.0);
                return;
            }
            _ => {}
        }
        if let Some(rest) = class.strip_prefix("rounded-[") {
            if let Some(inner) = rest.strip_suffix(']') {
                if let Some(px) = resolve_arbitrary_px(inner) {
                    style.border_radius = BorderRadius::all(px);
                    return;
                }
            }
        }

        // 9. Opacity & Shadows
        if let Some(rest) = class.strip_prefix("opacity-") {
            if let Ok(pct) = rest.parse::<f32>() {
                style.opacity = (pct / 100.0).clamp(0.0, 1.0);
                return;
            }
        }
        match class {
            "shadow-sm" => {
                style.box_shadow = vec![BoxShadow {
                    offset_x: 0.0,
                    offset_y: 1.0,
                    blur_radius: 2.0,
                    spread_radius: 0.0,
                    color: Color::new(0, 0, 0, 13),
                    inset: false,
                }];
                return;
            }
            "shadow" => {
                style.box_shadow = vec![BoxShadow {
                    offset_x: 0.0,
                    offset_y: 1.0,
                    blur_radius: 3.0,
                    spread_radius: 0.0,
                    color: Color::new(0, 0, 0, 26),
                    inset: false,
                }];
                return;
            }
            "shadow-md" => {
                style.box_shadow = vec![BoxShadow {
                    offset_x: 0.0,
                    offset_y: 4.0,
                    blur_radius: 6.0,
                    spread_radius: -1.0,
                    color: Color::new(0, 0, 0, 26),
                    inset: false,
                }];
                return;
            }
            "shadow-lg" => {
                style.box_shadow = vec![BoxShadow {
                    offset_x: 0.0,
                    offset_y: 10.0,
                    blur_radius: 15.0,
                    spread_radius: -3.0,
                    color: Color::new(0, 0, 0, 26),
                    inset: false,
                }];
                return;
            }
            "shadow-xl" => {
                style.box_shadow = vec![BoxShadow {
                    offset_x: 0.0,
                    offset_y: 20.0,
                    blur_radius: 25.0,
                    spread_radius: -5.0,
                    color: Color::new(0, 0, 0, 26),
                    inset: false,
                }];
                return;
            }
            "shadow-none" => {
                style.box_shadow.clear();
                return;
            }
            _ => {}
        }

        // 10. Positioning & Insets
        match class {
            "static" => {
                style.position = Position::Static;
                return;
            }
            "relative" => {
                style.position = Position::Relative;
                return;
            }
            "absolute" => {
                style.position = Position::Absolute;
                return;
            }
            "fixed" => {
                style.position = Position::Fixed;
                return;
            }
            "sticky" => {
                style.position = Position::Sticky;
                return;
            }
            "inset-0" => {
                style.insets = EdgeInsets::all(Dimension::Px(0.0));
                return;
            }
            "overflow-hidden" => {
                style.overflow_x = Overflow::Hidden;
                style.overflow_y = Overflow::Hidden;
                return;
            }
            "overflow-auto" => {
                style.overflow_x = Overflow::Auto;
                style.overflow_y = Overflow::Auto;
                return;
            }
            _ => {}
        }
        if let Some(rest) = class.strip_prefix("z-") {
            if rest == "auto" {
                style.z_index = None;
                return;
            }
            if let Ok(z) = rest.parse::<i32>() {
                style.z_index = Some(z);
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("top-") {
            if let Some(px) = resolve_spacing(rest) {
                style.insets.top = Dimension::Px(px);
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("left-") {
            if let Some(px) = resolve_spacing(rest) {
                style.insets.left = Dimension::Px(px);
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("right-") {
            if let Some(px) = resolve_spacing(rest) {
                style.insets.right = Dimension::Px(px);
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("bottom-") {
            if let Some(px) = resolve_spacing(rest) {
                style.insets.bottom = Dimension::Px(px);
                return;
            }
        }
    }
}

fn split_modifier(token: &str) -> (Option<&str>, &str) {
    if let Some((modifier, rest)) = token.split_once(':') {
        (Some(modifier), rest)
    } else {
        (None, token)
    }
}

fn split_opacity(token: &str) -> (&str, Option<f32>) {
    if let Some((color, op_str)) = token.split_once('/') {
        let op_trimmed = op_str.trim_start_matches('[').trim_end_matches(']');
        if let Some(pct) = op_trimmed.strip_suffix('%') {
            if let Ok(p) = pct.parse::<f32>() {
                return (color, Some((p / 100.0).clamp(0.0, 1.0)));
            }
        }
        if let Ok(val) = op_trimmed.parse::<f32>() {
            let alpha = if val <= 1.0 { val } else { val / 100.0 };
            return (color, Some(alpha.clamp(0.0, 1.0)));
        }
        (color, None)
    } else {
        (token, None)
    }
}

fn lookup_color_or_arbitrary(token: &str, opacity: Option<f32>) -> Option<Color> {
    if token.starts_with('[') && token.ends_with(']') {
        let inner = &token[1..token.len() - 1];
        if inner.starts_with('#') {
            let mut c = Color::parse_hex(inner)?;
            if let Some(op) = opacity {
                c.a = (c.a as f32 * op.clamp(0.0, 1.0)).round() as u8;
            }
            return Some(c);
        }
    }
    lookup_tailwind_color(token, opacity)
}

fn resolve_spacing(token: &str) -> Option<f32> {
    if token.starts_with('[') && token.ends_with(']') {
        let inner = &token[1..token.len() - 1];
        return resolve_arbitrary_px(inner);
    }

    match token {
        "0" => Some(0.0),
        "px" => Some(1.0),
        "0.5" => Some(2.0),
        "1" => Some(4.0),
        "1.5" => Some(6.0),
        "2" => Some(8.0),
        "2.5" => Some(10.0),
        "3" => Some(12.0),
        "3.5" => Some(14.0),
        "4" => Some(16.0),
        "5" => Some(20.0),
        "6" => Some(24.0),
        "7" => Some(28.0),
        "8" => Some(32.0),
        "9" => Some(36.0),
        "10" => Some(40.0),
        "11" => Some(44.0),
        "12" => Some(48.0),
        "14" => Some(56.0),
        "16" => Some(64.0),
        "20" => Some(80.0),
        "24" => Some(96.0),
        "28" => Some(112.0),
        "32" => Some(128.0),
        "36" => Some(144.0),
        "40" => Some(160.0),
        "44" => Some(176.0),
        "48" => Some(192.0),
        "52" => Some(208.0),
        "56" => Some(224.0),
        "60" => Some(240.0),
        "64" => Some(256.0),
        "72" => Some(288.0),
        "80" => Some(320.0),
        "96" => Some(384.0),
        _ => None,
    }
}

fn resolve_sizing_dimension(token: &str) -> Option<Dimension> {
    if token.starts_with('[') && token.ends_with(']') {
        let inner = &token[1..token.len() - 1];
        if let Some(px) = resolve_arbitrary_px(inner) {
            return Some(Dimension::Px(px));
        }
        if let Some(pct) = inner.strip_suffix('%') {
            if let Ok(n) = pct.parse::<f32>() {
                return Some(Dimension::Percent(n));
            }
        }
        return None;
    }

    if let Some((num, den)) = token.split_once('/') {
        if let (Ok(n), Ok(d)) = (num.parse::<f32>(), den.parse::<f32>()) {
            if d > 0.0 {
                return Some(Dimension::Percent((n / d) * 100.0));
            }
        }
    }

    resolve_spacing(token).map(Dimension::Px)
}

fn resolve_arbitrary_px(val: &str) -> Option<f32> {
    if let Some(px_str) = val.strip_suffix("px") {
        return px_str.trim().parse::<f32>().ok();
    }
    if let Some(rem_str) = val.strip_suffix("rem") {
        return rem_str.trim().parse::<f32>().map(|r| r * 16.0).ok();
    }
    if let Ok(n) = val.parse::<f32>() {
        return Some(n);
    }
    None
}
