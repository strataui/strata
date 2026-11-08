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

        for raw_token in tokenize_classes(classes) {
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

    /// Applies a single Tailwind class token evaluating pseudo-classes (`hover:`, `focus:`, `dark:`).
    pub fn apply_class_with_state(
        style: &mut ComputedStyle,
        token: &str,
        is_hovered: bool,
        is_focused: bool,
        is_dark_mode: bool,
    ) {
        let (modifier, class) = split_modifier(token);
        let should_apply = match modifier {
            None => true,
            Some("hover") => is_hovered,
            Some("focus") => is_focused,
            Some("dark") => is_dark_mode,
            Some(_) => false,
        };
        if should_apply {
            Self::apply_class(style, class);
        }
    }

    /// Applies a single, non-modified Tailwind class to `ComputedStyle`.
    pub fn apply_class(style: &mut ComputedStyle, class: &str) {
        // 1. Display & Layout Mode
        match class {
            "flex" => {
                style.display = Display::Flex;
                style.flex_direction = FlexDirection::Row;
                return;
            }
            "inline-flex" => {
                style.display = Display::InlineFlex;
                style.flex_direction = FlexDirection::Row;
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
            "table" => {
                style.display = Display::Table;
                return;
            }
            "inline-table" => {
                style.display = Display::InlineTable;
                return;
            }
            "table-caption" => {
                style.display = Display::TableCaption;
                return;
            }
            "table-cell" => {
                style.display = Display::TableCell;
                return;
            }
            "table-column" => {
                style.display = Display::TableColumn;
                return;
            }
            "table-column-group" => {
                style.display = Display::TableColumnGroup;
                return;
            }
            "table-footer-group" => {
                style.display = Display::TableFooterGroup;
                return;
            }
            "table-header-group" => {
                style.display = Display::TableHeaderGroup;
                return;
            }
            "table-row-group" => {
                style.display = Display::TableRowGroup;
                return;
            }
            "table-row" => {
                style.display = Display::TableRow;
                return;
            }
            "table-auto" => {
                style.table_layout = TableLayout::Auto;
                return;
            }
            "table-fixed" => {
                style.table_layout = TableLayout::Fixed;
                return;
            }
            "border-collapse" => {
                style.border_collapse = BorderCollapse::Collapse;
                return;
            }
            "border-separate" => {
                style.border_collapse = BorderCollapse::Separate;
                return;
            }
            "caption-top" => {
                style.caption_side = CaptionSide::Top;
                return;
            }
            "caption-bottom" => {
                style.caption_side = CaptionSide::Bottom;
                return;
            }
            "hidden" => {
                style.display = Display::None;
                return;
            }
            "content-auto" => {
                style.content_visibility = ContentVisibility::Auto;
                return;
            }
            "content-visible" => {
                style.content_visibility = ContentVisibility::Visible;
                return;
            }
            "content-hidden" => {
                style.content_visibility = ContentVisibility::Hidden;
                return;
            }
            "contain-none" => {
                style.contain = Containment::NONE;
                return;
            }
            "contain-strict" => {
                style.contain = Containment::STRICT;
                return;
            }
            "contain-content" => {
                style.contain = Containment::CONTENT;
                return;
            }
            "contain-size" => {
                style.contain.size = true;
                return;
            }
            "contain-inline-size" => {
                style.contain.inline_size = true;
                return;
            }
            "contain-layout" => {
                style.contain.layout = true;
                return;
            }
            "contain-paint" => {
                style.contain.paint = true;
                return;
            }
            "contain-style" => {
                style.contain.style = true;
                return;
            }
            _ => {}
        }

        // 2. Float and Clear Props
        match class {
            "float-left" => {
                style.float = Float::Left;
                return;
            }
            "float-right" => {
                style.float = Float::Right;
                return;
            }
            "float-none" => {
                style.float = Float::None;
                return;
            }
            "clear-left" => {
                style.clear = Clear::Left;
                return;
            }
            "clear-right" => {
                style.clear = Clear::Right;
                return;
            }
            "clear-both" => {
                style.clear = Clear::Both;
                return;
            }
            "clear-none" => {
                style.clear = Clear::None;
                return;
            }
            _ => {}
        }

        // 3. Flexbox Container & Item Props
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
                style.colspan = span;
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("row-span-") {
            if let Ok(span) = rest.parse::<u32>() {
                style.grid_row_span = Some(span);
                style.rowspan = span;
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

        if class == "m-auto" {
            style.margin_auto_top = true;
            style.margin_auto_right = true;
            style.margin_auto_bottom = true;
            style.margin_auto_left = true;
            return;
        }
        if class == "mx-auto" {
            style.margin_auto_left = true;
            style.margin_auto_right = true;
            return;
        }
        if class == "my-auto" {
            style.margin_auto_top = true;
            style.margin_auto_bottom = true;
            return;
        }
        if class == "ml-auto" {
            style.margin_auto_left = true;
            return;
        }
        if class == "mr-auto" {
            style.margin_auto_right = true;
            return;
        }
        if class == "mt-auto" {
            style.margin_auto_top = true;
            return;
        }
        if class == "mb-auto" {
            style.margin_auto_bottom = true;
            return;
        }

        if let Some(rest) = class.strip_prefix("m-") {
            if let Some(px) = resolve_spacing(rest) {
                style.margin = EdgeInsets::all(px);
                style.margin_auto_top = false;
                style.margin_auto_right = false;
                style.margin_auto_bottom = false;
                style.margin_auto_left = false;
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("-m-") {
            if let Some(px) = resolve_spacing(rest) {
                style.margin = EdgeInsets::all(-px);
                style.margin_auto_top = false;
                style.margin_auto_right = false;
                style.margin_auto_bottom = false;
                style.margin_auto_left = false;
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("mx-") {
            if let Some(px) = resolve_spacing(rest) {
                style.margin.left = px;
                style.margin.right = px;
                style.margin_auto_left = false;
                style.margin_auto_right = false;
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("my-") {
            if let Some(px) = resolve_spacing(rest) {
                style.margin.top = px;
                style.margin.bottom = px;
                style.margin_auto_top = false;
                style.margin_auto_bottom = false;
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("mt-") {
            if let Some(px) = resolve_spacing(rest) {
                style.margin.top = px;
                style.margin_auto_top = false;
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("mr-") {
            if let Some(px) = resolve_spacing(rest) {
                style.margin.right = px;
                style.margin_auto_right = false;
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("mb-") {
            if let Some(px) = resolve_spacing(rest) {
                style.margin.bottom = px;
                style.margin_auto_bottom = false;
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("ml-") {
            if let Some(px) = resolve_spacing(rest) {
                style.margin.left = px;
                style.margin_auto_left = false;
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
        if let Some(rest) = class.strip_prefix("space-x-") {
            if let Some(px) = resolve_spacing(rest) {
                style.gap_col = px;
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("space-y-") {
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
            if let Some(inner) = rest.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                if let Some(px_str) = inner.strip_suffix("px") {
                    if let Ok(px) = px_str.parse::<f32>() {
                        style.font_size = px;
                        style.line_height = 1.35;
                        return;
                    }
                } else if let Some(rem_str) = inner.strip_suffix("rem") {
                    if let Ok(rem) = rem_str.parse::<f32>() {
                        style.font_size = rem * 16.0;
                        style.line_height = 1.35;
                        return;
                    }
                }
            }
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
                style.line_height = 1.0;
                return;
            }
            "text-6xl" => {
                style.font_size = 60.0;
                style.line_height = 1.0;
                return;
            }
            "text-7xl" => {
                style.font_size = 72.0;
                style.line_height = 1.0;
                return;
            }
            "text-8xl" => {
                style.font_size = 96.0;
                style.line_height = 1.0;
                return;
            }
            "text-9xl" => {
                style.font_size = 128.0;
                style.line_height = 1.0;
                return;
            }
            "font-sans" => {
                style.font_family = strata_primitives::AtomId::FONT_SANS;
                return;
            }
            "font-serif" => {
                style.font_family = strata_primitives::AtomId::FONT_SERIF;
                return;
            }
            "font-mono" => {
                style.font_family = strata_primitives::AtomId::FONT_MONO;
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
            "whitespace-normal" => {
                style.white_space = WhiteSpace::Normal;
                return;
            }
            "whitespace-nowrap" => {
                style.white_space = WhiteSpace::NoWrap;
                return;
            }
            "whitespace-pre" => {
                style.white_space = WhiteSpace::Pre;
                return;
            }
            "whitespace-pre-line" => {
                style.white_space = WhiteSpace::PreLine;
                return;
            }
            "whitespace-pre-wrap" => {
                style.white_space = WhiteSpace::PreWrap;
                return;
            }
            "truncate" => {
                style.overflow_x = Overflow::Hidden;
                style.overflow_y = Overflow::Hidden;
                style.white_space = WhiteSpace::NoWrap;
                style.text_overflow = TextOverflow::Ellipsis;
                return;
            }
            "cursor-auto" | "cursor-default" => {
                style.cursor = Cursor::Default;
                return;
            }
            "cursor-pointer" => {
                style.cursor = Cursor::Pointer;
                return;
            }
            "cursor-text" => {
                style.cursor = Cursor::Text;
                return;
            }
            "cursor-move" => {
                style.cursor = Cursor::Move;
                return;
            }
            "cursor-not-allowed" => {
                style.cursor = Cursor::NotAllowed;
                return;
            }
            "cursor-grab" => {
                style.cursor = Cursor::Grab;
                return;
            }
            "cursor-grabbing" => {
                style.cursor = Cursor::Grabbing;
                return;
            }
            "cursor-col-resize" => {
                style.cursor = Cursor::ColResize;
                return;
            }
            "cursor-row-resize" => {
                style.cursor = Cursor::RowResize;
                return;
            }
            "cursor-crosshair" => {
                style.cursor = Cursor::Crosshair;
                return;
            }
            "cursor-wait" => {
                style.cursor = Cursor::Wait;
                return;
            }
            "cursor-help" => {
                style.cursor = Cursor::Help;
                return;
            }
            "cursor-none" => {
                style.cursor = Cursor::None;
                return;
            }
            _ => {}
        }

        // Caret Color (CSS caret-color)
        if let Some(rest) = class.strip_prefix("caret-") {
            if rest == "auto" {
                style.caret_color = None;
                return;
            }
            if rest == "transparent" {
                style.caret_color = Some(Color::rgba(0, 0, 0, 0));
                return;
            }
            if rest == "current" || rest == "inherit" {
                style.caret_color = Some(style.color);
                return;
            }
            let (color_part, opacity) = split_opacity(rest);
            if let Some(c) = lookup_color_or_arbitrary(color_part, opacity) {
                style.caret_color = Some(c);
                return;
            }
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
            "border-t-0" => {
                style.border_width.top = 0.0;
                return;
            }
            "border-b-0" => {
                style.border_width.bottom = 0.0;
                return;
            }
            "border-l-0" => {
                style.border_width.left = 0.0;
                return;
            }
            "border-r-0" => {
                style.border_width.right = 0.0;
                return;
            }
            "border-2" => {
                style.border_width = EdgeInsets::all(2.0);
                return;
            }
            "border-t-2" => {
                style.border_width.top = 2.0;
                return;
            }
            "border-b-2" => {
                style.border_width.bottom = 2.0;
                return;
            }
            "border-l-2" => {
                style.border_width.left = 2.0;
                return;
            }
            "border-r-2" => {
                style.border_width.right = 2.0;
                return;
            }
            "border-4" => {
                style.border_width = EdgeInsets::all(4.0);
                return;
            }
            "border-t-4" => {
                style.border_width.top = 4.0;
                return;
            }
            "border-b-4" => {
                style.border_width.bottom = 4.0;
                return;
            }
            "border-l-4" => {
                style.border_width.left = 4.0;
                return;
            }
            "border-r-4" => {
                style.border_width.right = 4.0;
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

        if let Some(rest) = class.strip_prefix("border-spacing-x-") {
            if let Some(px) = resolve_spacing(rest) {
                style.border_spacing.0 = px;
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("border-spacing-y-") {
            if let Some(px) = resolve_spacing(rest) {
                style.border_spacing.1 = px;
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("border-spacing-") {
            if let Some(px) = resolve_spacing(rest) {
                style.border_spacing = (px, px);
                return;
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
            "inset-auto" => {
                style.insets = EdgeInsets::all(Dimension::Auto);
                return;
            }
            "inset-x-auto" => {
                style.insets.left = Dimension::Auto;
                style.insets.right = Dimension::Auto;
                return;
            }
            "inset-y-auto" => {
                style.insets.top = Dimension::Auto;
                style.insets.bottom = Dimension::Auto;
                return;
            }
            "top-auto" => {
                style.insets.top = Dimension::Auto;
                return;
            }
            "bottom-auto" => {
                style.insets.bottom = Dimension::Auto;
                return;
            }
            "left-auto" => {
                style.insets.left = Dimension::Auto;
                return;
            }
            "right-auto" => {
                style.insets.right = Dimension::Auto;
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
            "overflow-scroll" => {
                style.overflow_x = Overflow::Scroll;
                style.overflow_y = Overflow::Scroll;
                return;
            }
            "overflow-visible" => {
                style.overflow_x = Overflow::Visible;
                style.overflow_y = Overflow::Visible;
                return;
            }
            "overflow-x-auto" => {
                style.overflow_x = Overflow::Auto;
                return;
            }
            "overflow-y-auto" => {
                style.overflow_y = Overflow::Auto;
                return;
            }
            "overflow-x-hidden" => {
                style.overflow_x = Overflow::Hidden;
                return;
            }
            "overflow-y-hidden" => {
                style.overflow_y = Overflow::Hidden;
                return;
            }
            "overflow-x-scroll" => {
                style.overflow_x = Overflow::Scroll;
                return;
            }
            "overflow-y-scroll" => {
                style.overflow_y = Overflow::Scroll;
                return;
            }
            "overflow-x-visible" => {
                style.overflow_x = Overflow::Visible;
                return;
            }
            "overflow-y-visible" => {
                style.overflow_y = Overflow::Visible;
                return;
            }
            _ => {}
        }
        if let Some(rest) = class.strip_prefix("z-") {
            if rest == "auto" {
                style.z_index = None;
                return;
            }
            let num_str = extract_bracket_content(rest).unwrap_or(rest);
            if let Ok(z) = num_str.parse::<i32>() {
                style.z_index = Some(z);
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("-z-") {
            let num_str = extract_bracket_content(rest).unwrap_or(rest);
            if let Ok(z) = num_str.parse::<i32>() {
                style.z_index = Some(-z);
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("inset-x-") {
            if let Some(px) = resolve_spacing(rest) {
                style.insets.left = Dimension::Px(px);
                style.insets.right = Dimension::Px(px);
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("-inset-x-") {
            if let Some(px) = resolve_spacing(rest) {
                style.insets.left = Dimension::Px(-px);
                style.insets.right = Dimension::Px(-px);
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("inset-x-") {
            if let Some(dim) = resolve_sizing_dimension(rest) {
                style.insets.left = dim;
                style.insets.right = dim;
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("-inset-x-") {
            if let Some(dim) = resolve_sizing_dimension(rest) {
                let neg = match dim {
                    Dimension::Px(px) => Dimension::Px(-px),
                    Dimension::Percent(pct) => Dimension::Percent(-pct),
                    other => other,
                };
                style.insets.left = neg;
                style.insets.right = neg;
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("inset-y-") {
            if let Some(dim) = resolve_sizing_dimension(rest) {
                style.insets.top = dim;
                style.insets.bottom = dim;
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("-inset-y-") {
            if let Some(dim) = resolve_sizing_dimension(rest) {
                let neg = match dim {
                    Dimension::Px(px) => Dimension::Px(-px),
                    Dimension::Percent(pct) => Dimension::Percent(-pct),
                    other => other,
                };
                style.insets.top = neg;
                style.insets.bottom = neg;
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("inset-") {
            if let Some(dim) = resolve_sizing_dimension(rest) {
                style.insets = EdgeInsets::all(dim);
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("-inset-") {
            if let Some(dim) = resolve_sizing_dimension(rest) {
                let neg = match dim {
                    Dimension::Px(px) => Dimension::Px(-px),
                    Dimension::Percent(pct) => Dimension::Percent(-pct),
                    other => other,
                };
                style.insets = EdgeInsets::all(neg);
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("top-") {
            if let Some(dim) = resolve_sizing_dimension(rest) {
                style.insets.top = dim;
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("-top-") {
            if let Some(dim) = resolve_sizing_dimension(rest) {
                let neg = match dim {
                    Dimension::Px(px) => Dimension::Px(-px),
                    Dimension::Percent(pct) => Dimension::Percent(-pct),
                    other => other,
                };
                style.insets.top = neg;
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("left-") {
            if let Some(dim) = resolve_sizing_dimension(rest) {
                style.insets.left = dim;
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("-left-") {
            if let Some(dim) = resolve_sizing_dimension(rest) {
                let neg = match dim {
                    Dimension::Px(px) => Dimension::Px(-px),
                    Dimension::Percent(pct) => Dimension::Percent(-pct),
                    other => other,
                };
                style.insets.left = neg;
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("right-") {
            if let Some(dim) = resolve_sizing_dimension(rest) {
                style.insets.right = dim;
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("-right-") {
            if let Some(dim) = resolve_sizing_dimension(rest) {
                let neg = match dim {
                    Dimension::Px(px) => Dimension::Px(-px),
                    Dimension::Percent(pct) => Dimension::Percent(-pct),
                    other => other,
                };
                style.insets.right = neg;
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("bottom-") {
            if let Some(dim) = resolve_sizing_dimension(rest) {
                style.insets.bottom = dim;
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("-bottom-") {
            if let Some(dim) = resolve_sizing_dimension(rest) {
                let neg = match dim {
                    Dimension::Px(px) => Dimension::Px(-px),
                    Dimension::Percent(pct) => Dimension::Percent(-pct),
                    other => other,
                };
                style.insets.bottom = neg;
                return;
            }
        }

        // Transitions & Animations (Tailwind v3/v4)
        if class == "transition" {
            let default_easing = TransitionEasing::CubicBezier(0.4, 0.0, 0.2, 1.0);
            style.transitions = vec![
                TransitionSpec {
                    property: TransitionProperty::Color,
                    duration_ms: 150.0,
                    timing_function: default_easing,
                    delay_ms: 0.0,
                    behavior: TransitionBehavior::Normal,
                },
                TransitionSpec {
                    property: TransitionProperty::BackgroundColor,
                    duration_ms: 150.0,
                    timing_function: default_easing,
                    delay_ms: 0.0,
                    behavior: TransitionBehavior::Normal,
                },
                TransitionSpec {
                    property: TransitionProperty::BorderColor,
                    duration_ms: 150.0,
                    timing_function: default_easing,
                    delay_ms: 0.0,
                    behavior: TransitionBehavior::Normal,
                },
                TransitionSpec {
                    property: TransitionProperty::Opacity,
                    duration_ms: 150.0,
                    timing_function: default_easing,
                    delay_ms: 0.0,
                    behavior: TransitionBehavior::Normal,
                },
                TransitionSpec {
                    property: TransitionProperty::BoxShadow,
                    duration_ms: 150.0,
                    timing_function: default_easing,
                    delay_ms: 0.0,
                    behavior: TransitionBehavior::Normal,
                },
                TransitionSpec {
                    property: TransitionProperty::Transform,
                    duration_ms: 150.0,
                    timing_function: default_easing,
                    delay_ms: 0.0,
                    behavior: TransitionBehavior::Normal,
                },
            ];
            return;
        }
        if class == "transition-all" {
            style.transitions = vec![TransitionSpec {
                property: TransitionProperty::All,
                duration_ms: 150.0,
                timing_function: TransitionEasing::CubicBezier(0.4, 0.0, 0.2, 1.0),
                delay_ms: 0.0,
                behavior: TransitionBehavior::Normal,
            }];
            return;
        }
        if class == "transition-colors" {
            let default_easing = TransitionEasing::CubicBezier(0.4, 0.0, 0.2, 1.0);
            style.transitions = vec![
                TransitionSpec {
                    property: TransitionProperty::Color,
                    duration_ms: 150.0,
                    timing_function: default_easing,
                    delay_ms: 0.0,
                    behavior: TransitionBehavior::Normal,
                },
                TransitionSpec {
                    property: TransitionProperty::BackgroundColor,
                    duration_ms: 150.0,
                    timing_function: default_easing,
                    delay_ms: 0.0,
                    behavior: TransitionBehavior::Normal,
                },
                TransitionSpec {
                    property: TransitionProperty::BorderColor,
                    duration_ms: 150.0,
                    timing_function: default_easing,
                    delay_ms: 0.0,
                    behavior: TransitionBehavior::Normal,
                },
            ];
            return;
        }
        if class == "transition-opacity" {
            style.transitions = vec![TransitionSpec {
                property: TransitionProperty::Opacity,
                duration_ms: 150.0,
                timing_function: TransitionEasing::CubicBezier(0.4, 0.0, 0.2, 1.0),
                delay_ms: 0.0,
                behavior: TransitionBehavior::Normal,
            }];
            return;
        }
        if class == "transition-shadow" {
            style.transitions = vec![TransitionSpec {
                property: TransitionProperty::BoxShadow,
                duration_ms: 150.0,
                timing_function: TransitionEasing::CubicBezier(0.4, 0.0, 0.2, 1.0),
                delay_ms: 0.0,
                behavior: TransitionBehavior::Normal,
            }];
            return;
        }
        if class == "transition-transform" {
            style.transitions = vec![TransitionSpec {
                property: TransitionProperty::Transform,
                duration_ms: 150.0,
                timing_function: TransitionEasing::CubicBezier(0.4, 0.0, 0.2, 1.0),
                delay_ms: 0.0,
                behavior: TransitionBehavior::Normal,
            }];
            return;
        }
        if class == "transition-none" {
            style.transitions = vec![TransitionSpec {
                property: TransitionProperty::None,
                duration_ms: 0.0,
                ..Default::default()
            }];
            return;
        }

        if let Some(dur_str) = class.strip_prefix("duration-") {
            let dur_ms = match dur_str {
                "0" => Some(0.0),
                "75" => Some(75.0),
                "100" => Some(100.0),
                "150" => Some(150.0),
                "200" => Some(200.0),
                "300" => Some(300.0),
                "500" => Some(500.0),
                "700" => Some(700.0),
                "1000" => Some(1000.0),
                custom => {
                    let inner = extract_bracket_content(custom).unwrap_or(custom);
                    crate::parser::parse_time_ms(inner)
                }
            };
            if let Some(d) = dur_ms {
                if style.transitions.is_empty() {
                    style.transitions = vec![TransitionSpec {
                        duration_ms: d,
                        ..Default::default()
                    }];
                } else {
                    for spec in &mut style.transitions {
                        spec.duration_ms = d;
                    }
                }
                return;
            }
        }

        if let Some(ease_str) = class.strip_prefix("ease-") {
            let easing = match ease_str {
                "linear" => Some(TransitionEasing::Linear),
                "in" => Some(TransitionEasing::CubicBezier(0.4, 0.0, 1.0, 1.0)),
                "out" => Some(TransitionEasing::CubicBezier(0.0, 0.0, 0.2, 1.0)),
                "in-out" => Some(TransitionEasing::CubicBezier(0.4, 0.0, 0.2, 1.0)),
                "initial" => Some(TransitionEasing::Ease),
                custom => {
                    let inner = extract_bracket_content(custom).unwrap_or(custom);
                    TransitionEasing::parse(inner)
                }
            };
            if let Some(e) = easing {
                if style.transitions.is_empty() {
                    style.transitions = vec![TransitionSpec {
                        timing_function: e,
                        ..Default::default()
                    }];
                } else {
                    for spec in &mut style.transitions {
                        spec.timing_function = e;
                    }
                }
                return;
            }
        }

        if let Some(delay_str) = class.strip_prefix("delay-") {
            let delay_ms = match delay_str {
                "75" => Some(75.0),
                "100" => Some(100.0),
                "150" => Some(150.0),
                "200" => Some(200.0),
                "300" => Some(300.0),
                "500" => Some(500.0),
                "700" => Some(700.0),
                "1000" => Some(1000.0),
                custom => {
                    let inner = extract_bracket_content(custom).unwrap_or(custom);
                    crate::parser::parse_time_ms(inner)
                }
            };
            if let Some(del) = delay_ms {
                if style.transitions.is_empty() {
                    style.transitions = vec![TransitionSpec {
                        delay_ms: del,
                        ..Default::default()
                    }];
                } else {
                    for spec in &mut style.transitions {
                        spec.delay_ms = del;
                    }
                }
                return;
            }
        }

        // Animations (Tailwind v3/v4)
        if class == "animate-none" {
            style.animations.clear();
            return;
        }
        if class == "animate-spin" {
            style.animations = vec![AnimationSpec {
                name: "spin".to_string(),
                duration_ms: 1000.0,
                timing_function: TransitionEasing::Linear,
                delay_ms: 0.0,
                iteration_count: AnimationIterationCount::Infinite,
                direction: AnimationDirection::Normal,
                fill_mode: AnimationFillMode::None,
                play_state: AnimationPlayState::Running,
                ..Default::default()
            }];
            return;
        }
        if class == "animate-ping" {
            style.animations = vec![AnimationSpec {
                name: "ping".to_string(),
                duration_ms: 1000.0,
                timing_function: TransitionEasing::CubicBezier(0.0, 0.0, 0.2, 1.0),
                delay_ms: 0.0,
                iteration_count: AnimationIterationCount::Infinite,
                direction: AnimationDirection::Normal,
                fill_mode: AnimationFillMode::None,
                play_state: AnimationPlayState::Running,
                ..Default::default()
            }];
            return;
        }
        if class == "animate-pulse" {
            style.animations = vec![AnimationSpec {
                name: "pulse".to_string(),
                duration_ms: 2000.0,
                timing_function: TransitionEasing::CubicBezier(0.4, 0.0, 0.6, 1.0),
                delay_ms: 0.0,
                iteration_count: AnimationIterationCount::Infinite,
                direction: AnimationDirection::Normal,
                fill_mode: AnimationFillMode::None,
                play_state: AnimationPlayState::Running,
                ..Default::default()
            }];
            return;
        }
        if class == "animate-bounce" {
            style.animations = vec![AnimationSpec {
                name: "bounce".to_string(),
                duration_ms: 1000.0,
                timing_function: TransitionEasing::Ease,
                delay_ms: 0.0,
                iteration_count: AnimationIterationCount::Infinite,
                direction: AnimationDirection::Normal,
                fill_mode: AnimationFillMode::None,
                play_state: AnimationPlayState::Running,
                ..Default::default()
            }];
            return;
        }
        if let Some(rest) = class.strip_prefix("animate-") {
            if let Some(inner) = extract_bracket_content(rest) {
                let normalized = inner.replace('_', " ");
                let parsed = crate::parser::parse_animations(&normalized);
                if !parsed.is_empty() {
                    style.animations = parsed;
                    return;
                }
            } else {
                style.animations = vec![AnimationSpec {
                    name: rest.to_string(),
                    duration_ms: 1000.0,
                    timing_function: TransitionEasing::Ease,
                    delay_ms: 0.0,
                    iteration_count: AnimationIterationCount::Count(1.0),
                    direction: AnimationDirection::Normal,
                    fill_mode: AnimationFillMode::None,
                    play_state: AnimationPlayState::Running,
                    timeline: AnimationTimeline::Time,
                    range: AnimationRange::default(),
                }];
                return;
            }
        }

        // View Transitions Module Level 1 & 2
        if class == "view-transition-none" {
            style.view_transition_name = None;
            return;
        }
        if let Some(rest) = class.strip_prefix("view-transition-") {
            let name = extract_bracket_content(rest).unwrap_or(rest);
            style.view_transition_name = Some(name.to_string());
            return;
        }

        // Scroll-Driven Animation Timelines (W3C Level 1)
        if class == "timeline-none" {
            style.animation_timeline = AnimationTimeline::Named("none".to_string());
            return;
        }
        if class == "timeline-scroll" {
            style.animation_timeline =
                AnimationTimeline::Scroll(ScrollTimelineSource::Nearest, ScrollAxis::Block);
            return;
        }
        if class == "timeline-scroll-root" {
            style.animation_timeline =
                AnimationTimeline::Scroll(ScrollTimelineSource::Root, ScrollAxis::Block);
            return;
        }
        if class == "timeline-scroll-inline" {
            style.animation_timeline =
                AnimationTimeline::Scroll(ScrollTimelineSource::Nearest, ScrollAxis::Inline);
            return;
        }
        if class == "timeline-view" {
            style.animation_timeline = AnimationTimeline::View(ScrollAxis::Block, 0.0, 0.0);
            return;
        }
        if let Some(rest) = class.strip_prefix("timeline-") {
            let name = extract_bracket_content(rest).unwrap_or(rest);
            style.animation_timeline = crate::parser::parse_animation_timeline(name);
            return;
        }

        // Animation Range Anchors
        if class == "range-cover" {
            style.animation_range = AnimationRange {
                name: AnimationRangeName::Cover,
                start_offset: 0.0,
                end_offset: 1.0,
            };
            return;
        }
        if class == "range-contain" {
            style.animation_range = AnimationRange {
                name: AnimationRangeName::Contain,
                start_offset: 0.0,
                end_offset: 1.0,
            };
            return;
        }
        if class == "range-entry" {
            style.animation_range = AnimationRange {
                name: AnimationRangeName::Entry,
                start_offset: 0.0,
                end_offset: 1.0,
            };
            return;
        }
        if class == "range-exit" {
            style.animation_range = AnimationRange {
                name: AnimationRangeName::Exit,
                start_offset: 0.0,
                end_offset: 1.0,
            };
            return;
        }
        if class == "range-normal" {
            style.animation_range = AnimationRange::default();
            return;
        }

        // 2D & 3D Transforms (Tailwind v3/v4)
        if let Some(rest) = class.strip_prefix("scale-") {
            let factor = match rest {
                "0" => Some(0.0),
                "50" => Some(0.5),
                "75" => Some(0.75),
                "90" => Some(0.9),
                "95" => Some(0.95),
                "100" => Some(1.0),
                "105" => Some(1.05),
                "110" => Some(1.10),
                "125" => Some(1.25),
                "150" => Some(1.5),
                custom => {
                    let inner = extract_bracket_content(custom).unwrap_or(custom);
                    if let Some(pct) = inner.strip_suffix('%') {
                        pct.parse::<f32>().map(|p| p / 100.0).ok().filter(|f| f.is_finite())
                    } else {
                        inner.parse::<f32>().ok().filter(|f| f.is_finite())
                    }
                }
            };
            if let Some(f) = factor {
                style
                    .transform
                    .retain(|op| !matches!(op, TransformOp::Scale(_, _)));
                style.transform.push(TransformOp::Scale(f, f));
                return;
            }
        }

        if let Some(rest) = class.strip_prefix("rotate-") {
            let angle_rad = match rest {
                "0" => Some(0.0),
                "1" => Some(1.0f32.to_radians()),
                "2" => Some(2.0f32.to_radians()),
                "3" => Some(3.0f32.to_radians()),
                "6" => Some(6.0f32.to_radians()),
                "12" => Some(12.0f32.to_radians()),
                "45" => Some(45.0f32.to_radians()),
                "90" => Some(90.0f32.to_radians()),
                "180" => Some(180.0f32.to_radians()),
                custom => {
                    let inner = extract_bracket_content(custom).unwrap_or(custom);
                    crate::parser::parse_angle_rad(inner).filter(|f| f.is_finite())
                }
            };
            if let Some(rad) = angle_rad {
                style
                    .transform
                    .retain(|op| !matches!(op, TransformOp::Rotate(_)));
                style.transform.push(TransformOp::Rotate(rad));
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("-rotate-") {
            let angle_rad = match rest {
                "1" => Some(-1.0f32.to_radians()),
                "2" => Some(-2.0f32.to_radians()),
                "3" => Some(-3.0f32.to_radians()),
                "6" => Some(-6.0f32.to_radians()),
                "12" => Some(-12.0f32.to_radians()),
                "45" => Some(-45.0f32.to_radians()),
                "90" => Some(-90.0f32.to_radians()),
                "180" => Some(-180.0f32.to_radians()),
                custom => {
                    let inner = extract_bracket_content(custom).unwrap_or(custom);
                    crate::parser::parse_angle_rad(inner).filter(|f| f.is_finite()).map(|r| -r)
                }
            };
            if let Some(rad) = angle_rad {
                style
                    .transform
                    .retain(|op| !matches!(op, TransformOp::Rotate(_)));
                style.transform.push(TransformOp::Rotate(rad));
                return;
            }
        }

        if let Some(rest) = class.strip_prefix("translate-x-") {
            let dim = match rest {
                "full" => Some(Dimension::Percent(100.0)),
                "1/2" => Some(Dimension::Percent(50.0)),
                "1/3" => Some(Dimension::Percent(33.333)),
                "2/3" => Some(Dimension::Percent(66.666)),
                "1/4" => Some(Dimension::Percent(25.0)),
                "3/4" => Some(Dimension::Percent(75.0)),
                other => resolve_sizing_dimension(other),
            };
            if let Some(d) = dim {
                style
                    .transform
                    .push(TransformOp::Translate(d, Dimension::Px(0.0)));
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("-translate-x-") {
            let dim = match rest {
                "full" => Some(Dimension::Percent(-100.0)),
                "1/2" => Some(Dimension::Percent(-50.0)),
                "1/3" => Some(Dimension::Percent(-33.333)),
                "2/3" => Some(Dimension::Percent(-66.666)),
                "1/4" => Some(Dimension::Percent(-25.0)),
                "3/4" => Some(Dimension::Percent(-75.0)),
                other => resolve_spacing(other).map(|px| Dimension::Px(-px)),
            };
            if let Some(d) = dim {
                style
                    .transform
                    .push(TransformOp::Translate(d, Dimension::Px(0.0)));
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("translate-y-") {
            let dim = match rest {
                "full" => Some(Dimension::Percent(100.0)),
                "1/2" => Some(Dimension::Percent(50.0)),
                "1/3" => Some(Dimension::Percent(33.333)),
                "2/3" => Some(Dimension::Percent(66.666)),
                "1/4" => Some(Dimension::Percent(25.0)),
                "3/4" => Some(Dimension::Percent(75.0)),
                other => resolve_sizing_dimension(other),
            };
            if let Some(d) = dim {
                style
                    .transform
                    .push(TransformOp::Translate(Dimension::Px(0.0), d));
                return;
            }
        }
        if let Some(rest) = class.strip_prefix("-translate-y-") {
            let dim = match rest {
                "full" => Some(Dimension::Percent(-100.0)),
                "1/2" => Some(Dimension::Percent(-50.0)),
                "1/3" => Some(Dimension::Percent(-33.333)),
                "2/3" => Some(Dimension::Percent(-66.666)),
                "1/4" => Some(Dimension::Percent(-25.0)),
                "3/4" => Some(Dimension::Percent(-75.0)),
                other => resolve_spacing(other).map(|px| Dimension::Px(-px)),
            };
            if let Some(d) = dim {
                style
                    .transform
                    .push(TransformOp::Translate(Dimension::Px(0.0), d));
                return;
            }
        }

        if let Some(rest) = class.strip_prefix("origin-") {
            let origin = match rest {
                "center" => Some(TransformOrigin {
                    x: Dimension::Percent(50.0),
                    y: Dimension::Percent(50.0),
                    z: 0.0,
                }),
                "top" => Some(TransformOrigin {
                    x: Dimension::Percent(50.0),
                    y: Dimension::Percent(0.0),
                    z: 0.0,
                }),
                "top-right" => Some(TransformOrigin {
                    x: Dimension::Percent(100.0),
                    y: Dimension::Percent(0.0),
                    z: 0.0,
                }),
                "right" => Some(TransformOrigin {
                    x: Dimension::Percent(100.0),
                    y: Dimension::Percent(50.0),
                    z: 0.0,
                }),
                "bottom-right" => Some(TransformOrigin {
                    x: Dimension::Percent(100.0),
                    y: Dimension::Percent(100.0),
                    z: 0.0,
                }),
                "bottom" => Some(TransformOrigin {
                    x: Dimension::Percent(50.0),
                    y: Dimension::Percent(100.0),
                    z: 0.0,
                }),
                "bottom-left" => Some(TransformOrigin {
                    x: Dimension::Percent(0.0),
                    y: Dimension::Percent(100.0),
                    z: 0.0,
                }),
                "left" => Some(TransformOrigin {
                    x: Dimension::Percent(0.0),
                    y: Dimension::Percent(50.0),
                    z: 0.0,
                }),
                "top-left" => Some(TransformOrigin {
                    x: Dimension::Percent(0.0),
                    y: Dimension::Percent(0.0),
                    z: 0.0,
                }),
                _ => None,
            };
            if let Some(o) = origin {
                style.transform_origin = o;
                return;
            }
        }

        // 7. Visual Effects: Blend modes, Isolation, Filters, and Backdrop Filters
        if resolve_visual_effects(class, style) {
        }
    }
}

fn resolve_visual_effects(class: &str, style: &mut ComputedStyle) -> bool {
    // 1. Isolation
    if class == "isolate" {
        style.isolation = Isolation::Isolate;
        return true;
    }
    if class == "isolation-auto" {
        style.isolation = Isolation::Auto;
        return true;
    }

    // 2. Mix Blend Modes
    if let Some(rest) = class.strip_prefix("mix-blend-") {
        if let Some(bm) = BlendMode::parse(rest) {
            style.mix_blend_mode = bm;
            return true;
        } else if rest == "plus-lighter" {
            style.mix_blend_mode = BlendMode::Lighten;
            return true;
        }
    }

    // 3. Filter enable / disable
    if class == "filter-none" {
        style.filter = None;
        return true;
    }
    if class == "filter" {
        if style.filter.is_none() {
            style.filter = Some(crate::effects::BackdropFilter::new(Vec::new()));
        }
        return true;
    }

    // 4. Blur filters
    if class == "blur-none" {
        add_filter_op(&mut style.filter, crate::effects::FilterOp::Blur(0.0));
        return true;
    }
    if class == "blur-sm" {
        add_filter_op(&mut style.filter, crate::effects::FilterOp::Blur(4.0));
        return true;
    }
    if class == "blur" {
        add_filter_op(&mut style.filter, crate::effects::FilterOp::Blur(8.0));
        return true;
    }
    if class == "blur-md" {
        add_filter_op(&mut style.filter, crate::effects::FilterOp::Blur(12.0));
        return true;
    }
    if class == "blur-lg" {
        add_filter_op(&mut style.filter, crate::effects::FilterOp::Blur(16.0));
        return true;
    }
    if class == "blur-xl" {
        add_filter_op(&mut style.filter, crate::effects::FilterOp::Blur(24.0));
        return true;
    }
    if class == "blur-2xl" {
        add_filter_op(&mut style.filter, crate::effects::FilterOp::Blur(40.0));
        return true;
    }
    if class == "blur-3xl" {
        add_filter_op(&mut style.filter, crate::effects::FilterOp::Blur(64.0));
        return true;
    }
    if let Some(rest) = class.strip_prefix("blur-[") {
        if let Some(inner) = rest.strip_suffix(']') {
            if let Some(px) = resolve_pixel_length(inner) {
                add_filter_op(&mut style.filter, crate::effects::FilterOp::Blur(px));
                return true;
            }
        }
    }

    // 5. Brightness filters
    if let Some(rest) = class.strip_prefix("brightness-") {
        let val = match rest {
            "0" => Some(0.0),
            "50" => Some(0.5),
            "75" => Some(0.75),
            "90" => Some(0.9),
            "95" => Some(0.95),
            "100" => Some(1.0),
            "105" => Some(1.05),
            "110" => Some(1.10),
            "125" => Some(1.25),
            "150" => Some(1.5),
            "200" => Some(2.0),
            other => {
                if let Some(inner) = extract_bracket_content(other) {
                    inner.parse::<f32>().ok().filter(|f| f.is_finite())
                } else {
                    None
                }
            }
        };
        if let Some(b) = val {
            add_filter_op(&mut style.filter, crate::effects::FilterOp::Brightness(b));
            return true;
        }
    }

    // 6. Contrast filters
    if let Some(rest) = class.strip_prefix("contrast-") {
        let val = match rest {
            "0" => Some(0.0),
            "50" => Some(0.5),
            "75" => Some(0.75),
            "100" => Some(1.0),
            "125" => Some(1.25),
            "150" => Some(1.5),
            "200" => Some(2.0),
            other => {
                if let Some(inner) = extract_bracket_content(other) {
                    inner.parse::<f32>().ok().filter(|f| f.is_finite())
                } else {
                    None
                }
            }
        };
        if let Some(c) = val {
            add_filter_op(&mut style.filter, crate::effects::FilterOp::Contrast(c));
            return true;
        }
    }

    // 7. Grayscale
    if class == "grayscale" {
        add_filter_op(&mut style.filter, crate::effects::FilterOp::Grayscale(1.0));
        return true;
    }
    if class == "grayscale-0" {
        add_filter_op(&mut style.filter, crate::effects::FilterOp::Grayscale(0.0));
        return true;
    }

    // 8. Invert
    if class == "invert" {
        add_filter_op(&mut style.filter, crate::effects::FilterOp::Invert(1.0));
        return true;
    }
    if class == "invert-0" {
        add_filter_op(&mut style.filter, crate::effects::FilterOp::Invert(0.0));
        return true;
    }

    // 9. Sepia
    if class == "sepia" {
        add_filter_op(&mut style.filter, crate::effects::FilterOp::Sepia(1.0));
        return true;
    }
    if class == "sepia-0" {
        add_filter_op(&mut style.filter, crate::effects::FilterOp::Sepia(0.0));
        return true;
    }

    // 10. Backdrop filters
    if class == "backdrop-filter-none" {
        style.backdrop_filter = None;
        return true;
    }
    if class == "backdrop-filter" {
        if style.backdrop_filter.is_none() {
            style.backdrop_filter = Some(crate::effects::BackdropFilter::new(Vec::new()));
        }
        return true;
    }
    if class == "backdrop-blur-none" {
        add_filter_op(
            &mut style.backdrop_filter,
            crate::effects::FilterOp::Blur(0.0),
        );
        return true;
    }
    if class == "backdrop-blur-sm" {
        add_filter_op(
            &mut style.backdrop_filter,
            crate::effects::FilterOp::Blur(4.0),
        );
        return true;
    }
    if class == "backdrop-blur" {
        add_filter_op(
            &mut style.backdrop_filter,
            crate::effects::FilterOp::Blur(8.0),
        );
        return true;
    }
    if class == "backdrop-blur-md" {
        add_filter_op(
            &mut style.backdrop_filter,
            crate::effects::FilterOp::Blur(12.0),
        );
        return true;
    }
    if class == "backdrop-blur-lg" {
        add_filter_op(
            &mut style.backdrop_filter,
            crate::effects::FilterOp::Blur(16.0),
        );
        return true;
    }
    if class == "backdrop-blur-xl" {
        add_filter_op(
            &mut style.backdrop_filter,
            crate::effects::FilterOp::Blur(24.0),
        );
        return true;
    }
    if class == "backdrop-blur-2xl" {
        add_filter_op(
            &mut style.backdrop_filter,
            crate::effects::FilterOp::Blur(40.0),
        );
        return true;
    }
    if class == "backdrop-blur-3xl" {
        add_filter_op(
            &mut style.backdrop_filter,
            crate::effects::FilterOp::Blur(64.0),
        );
        return true;
    }
    if let Some(rest) = class.strip_prefix("backdrop-blur-[") {
        if let Some(inner) = rest.strip_suffix(']') {
            if let Some(px) = resolve_pixel_length(inner) {
                add_filter_op(
                    &mut style.backdrop_filter,
                    crate::effects::FilterOp::Blur(px),
                );
                return true;
            }
        }
    }

    // 11. Arbitrary clip-path
    if let Some(rest) = class.strip_prefix("clip-path-[") {
        if let Some(inner) = rest.strip_suffix(']') {
            let normalized = inner.replace('_', " ");
            style.clip_path = crate::parser::parse_clip_path(&normalized);
            return true;
        }
    }

    false
}

fn add_filter_op(
    target: &mut Option<crate::effects::BackdropFilter>,
    op: crate::effects::FilterOp,
) {
    if let Some(filter) = target.as_mut() {
        filter.operations.push(op);
    } else {
        *target = Some(crate::effects::BackdropFilter::new(vec![op]));
    }
}

fn resolve_pixel_length(val: &str) -> Option<f32> {
    if let Some(px) = val.strip_suffix("px") {
        px.trim().parse::<f32>().ok().filter(|f| f.is_finite())
    } else if let Some(rem) = val.strip_suffix("rem") {
        rem.trim().parse::<f32>().ok().filter(|f| f.is_finite()).map(|r| r * 16.0)
    } else {
        val.trim().parse::<f32>().ok().filter(|f| f.is_finite())
    }
}

/// Safely extracts the inner content of a bracketed arbitrary value `[...]`.
/// Returns `None` if the token does not start with `[` or end with `]`.
#[inline]
pub fn extract_bracket_content(token: &str) -> Option<&str> {
    token.strip_prefix('[')?.strip_suffix(']')
}

/// Tokenizes a Tailwind classes string by whitespace, preserving bracketed expressions `[...]`.
pub fn tokenize_classes(classes: &str) -> Vec<&str> {
    let mut tokens = Vec::new();
    let mut bracket_depth = 0;
    let mut start = None;

    for (idx, ch) in classes.char_indices() {
        if ch == '[' {
            bracket_depth += 1;
        } else if ch == ']' && bracket_depth > 0 {
            bracket_depth -= 1;
        }

        if ch.is_whitespace() && bracket_depth == 0 {
            if let Some(s) = start {
                tokens.push(&classes[s..idx]);
                start = None;
            }
        } else if start.is_none() {
            start = Some(idx);
        }
    }

    if let Some(s) = start {
        tokens.push(&classes[s..]);
    }

    tokens
}

/// Splits a modifier prefix (`hover:`, `dark:`, `sm:`) from a class token,
/// ignoring colons located within arbitrary value brackets `[...]`.
pub fn split_modifier(token: &str) -> (Option<&str>, &str) {
    let mut bracket_depth = 0;
    for (idx, ch) in token.char_indices() {
        match ch {
            '[' => bracket_depth += 1,
            ']' => {
                if bracket_depth > 0 {
                    bracket_depth -= 1;
                }
            }
            ':' if bracket_depth == 0 => {
                let modifier = &token[..idx];
                let rest = &token[idx + 1..];
                return (Some(modifier), rest);
            }
            _ => {}
        }
    }
    (None, token)
}

/// Splits an opacity suffix (`/50`, `/[0.75]`) from a class token,
/// ignoring forward slashes located within arbitrary value brackets `[...]`.
fn split_opacity(token: &str) -> (&str, Option<f32>) {
    let mut bracket_depth = 0;
    let mut split_idx = None;
    for (idx, ch) in token.char_indices() {
        match ch {
            '[' => bracket_depth += 1,
            ']' => {
                if bracket_depth > 0 {
                    bracket_depth -= 1;
                }
            }
            '/' if bracket_depth == 0 => {
                split_idx = Some(idx);
                break;
            }
            _ => {}
        }
    }

    if let Some(idx) = split_idx {
        let color = &token[..idx];
        let op_str = &token[idx + 1..];
        let op_trimmed = extract_bracket_content(op_str).unwrap_or(op_str);
        if let Some(pct) = op_trimmed.strip_suffix('%') {
            if let Ok(p) = pct.parse::<f32>() {
                if p.is_finite() {
                    return (color, Some((p / 100.0).clamp(0.0, 1.0)));
                }
            }
        }
        if let Ok(val) = op_trimmed.parse::<f32>() {
            if val.is_finite() {
                let alpha = if val <= 1.0 { val } else { val / 100.0 };
                return (color, Some(alpha.clamp(0.0, 1.0)));
            }
        }
        (color, None)
    } else {
        (token, None)
    }
}

fn lookup_color_or_arbitrary(token: &str, opacity: Option<f32>) -> Option<Color> {
    if let Some(inner) = extract_bracket_content(token) {
        let clean = inner.trim();
        let normalized = clean.replace('_', " ");
        if let Some(mut c) = crate::parser::parse_color(&normalized) {
            if let Some(op) = opacity {
                c.a = (c.a as f32 * op.clamp(0.0, 1.0)).round() as u8;
            }
            return Some(c);
        }
    }
    lookup_tailwind_color(token, opacity)
}

fn resolve_spacing(token: &str) -> Option<f32> {
    if let Some(inner) = extract_bracket_content(token) {
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
    match token {
        "full" => return Some(Dimension::Percent(100.0)),
        "screen" => return Some(Dimension::Vh(100.0)),
        "none" => return Some(Dimension::Auto),
        "min" => return Some(Dimension::MinContent),
        "max" => return Some(Dimension::MaxContent),
        "fit" => return Some(Dimension::FitContent(0.0)),
        "0" => return Some(Dimension::Px(0.0)),
        "xs" => return Some(Dimension::Px(320.0)),
        "sm" => return Some(Dimension::Px(384.0)),
        "md" => return Some(Dimension::Px(448.0)),
        "lg" => return Some(Dimension::Px(512.0)),
        "xl" => return Some(Dimension::Px(576.0)),
        "2xl" => return Some(Dimension::Px(672.0)),
        "3xl" => return Some(Dimension::Px(768.0)),
        "4xl" => return Some(Dimension::Px(896.0)),
        "5xl" => return Some(Dimension::Px(1024.0)),
        "6xl" => return Some(Dimension::Px(1152.0)),
        "7xl" => return Some(Dimension::Px(1280.0)),
        _ => {}
    }

    if let Some(inner) = extract_bracket_content(token) {
        if let Some(px_str) = inner.strip_suffix("px") {
            if let Ok(n) = px_str.trim().parse::<f32>() {
                if n.is_finite() {
                    return Some(Dimension::Px(n));
                }
            }
        }
        if let Some(pct) = inner.strip_suffix('%') {
            if let Ok(n) = pct.trim().parse::<f32>() {
                if n.is_finite() {
                    return Some(Dimension::Percent(n));
                }
            }
        }
        if let Some(rem) = inner.strip_suffix("rem") {
            if let Ok(n) = rem.trim().parse::<f32>() {
                if n.is_finite() {
                    return Some(Dimension::Px(n * 16.0));
                }
            }
        }
        if let Some(em) = inner.strip_suffix("em") {
            if let Ok(n) = em.trim().parse::<f32>() {
                if n.is_finite() {
                    return Some(Dimension::Px(n * 16.0));
                }
            }
        }
        if let Some(vw) = inner.strip_suffix("vw") {
            if let Ok(n) = vw.trim().parse::<f32>() {
                if n.is_finite() {
                    return Some(Dimension::Vw(n));
                }
            }
        }
        if let Some(vh) = inner.strip_suffix("vh") {
            if let Ok(n) = vh.trim().parse::<f32>() {
                if n.is_finite() {
                    return Some(Dimension::Vh(n));
                }
            }
        }
        let calc_str = inner.replace('_', " ");
        if calc_str.starts_with("calc(") || calc_str.contains('+') || calc_str.contains('-') || calc_str.contains('*') || calc_str.contains('/') {
            if let Ok(expr) = crate::calc::parse_calc(&calc_str) {
                let id = crate::calc::intern_calc(expr);
                return Some(Dimension::Calc(id));
            }
        }
        if let Some(px) = resolve_arbitrary_px(inner) {
            return Some(Dimension::Px(px));
        }
        return None;
    }

    if let Some((num, den)) = token.split_once('/') {
        if let (Ok(n), Ok(d)) = (num.parse::<f32>(), den.parse::<f32>()) {
            if d > 0.0 && n.is_finite() && d.is_finite() {
                return Some(Dimension::Percent((n / d) * 100.0));
            }
        }
    }

    resolve_spacing(token).map(Dimension::Px)
}

fn resolve_arbitrary_px(val: &str) -> Option<f32> {
    let clean = val.trim();
    if let Some(px_str) = clean.strip_suffix("px") {
        return px_str.trim().parse::<f32>().ok().filter(|f| f.is_finite());
    }
    if let Some(rem_str) = clean.strip_suffix("rem") {
        return rem_str.trim().parse::<f32>().ok().filter(|f| f.is_finite()).map(|r| r * 16.0);
    }
    if let Some(em_str) = clean.strip_suffix("em") {
        return em_str.trim().parse::<f32>().ok().filter(|f| f.is_finite()).map(|e| e * 16.0);
    }
    if let Ok(n) = clean.parse::<f32>() {
        if n.is_finite() {
            return Some(n);
        }
    }
    None
}
