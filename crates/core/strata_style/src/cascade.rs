//! W3C-compliant CSS Cascade Solver and Property Application Engine.

use crate::parser::{
    parse_angle_rad, parse_animation_delays, parse_animation_directions, parse_animation_durations,
    parse_animation_fill_modes, parse_animation_iteration_counts, parse_animation_names,
    parse_animation_play_states, parse_animation_range, parse_animation_timeline,
    parse_animation_timing_functions, parse_animations, parse_aspect_ratio,
    parse_backface_visibility, parse_border_style, parse_color, parse_cursor, parse_declarations,
    parse_dimension, parse_pointer_events, parse_text_decoration, parse_text_transform,
    parse_transform_list, parse_transform_origin, parse_transform_style,
    parse_transition_behaviors, parse_transition_delays, parse_transition_durations,
    parse_transition_properties, parse_transition_timing_functions, parse_transitions,
    parse_user_select, parse_vertical_align, parse_view_transition_name, parse_visibility,
    parse_white_space, Declaration, StyleSheet,
};
use crate::selector::PseudoState;
use crate::specificity::Specificity;
use crate::types::*;

fn resolve_var<'a>(val: &'a str, style: &ComputedStyle) -> std::borrow::Cow<'a, str> {
    if !val.contains("var(") {
        return std::borrow::Cow::Borrowed(val);
    }
    let mut resolved = val.to_string();
    while let Some(start) = resolved.find("var(") {
        if let Some(end_rel) = resolved[start..].find(')') {
            let end = start + end_rel;
            let (name, fallback) = {
                let inner = resolved[start + 4..end].trim();
                match inner.split_once(',') {
                    Some((n, fb)) => (n.trim().to_string(), fb.trim().to_string()),
                    None => (inner.to_string(), String::new()),
                }
            };
            let key_atom = strata_primitives::atom::intern(&name);
            let replacement_string = style
                .get_custom_property(key_atom)
                .and_then(strata_primitives::atom::resolve)
                .unwrap_or(fallback);
            resolved.replace_range(start..=end, &replacement_string);
        } else {
            break;
        }
    }
    std::borrow::Cow::Owned(resolved)
}

#[inline]
fn resolve_length_px(val: &str, current_font_size: f32, root_font_size: f32) -> Option<f32> {
    match parse_dimension(val) {
        Some(Dimension::Px(px)) => Some(px),
        Some(Dimension::Em(em)) => Some(em * current_font_size),
        Some(Dimension::Rem(rem)) => Some(rem * root_font_size),
        Some(Dimension::Percent(pct)) => Some(current_font_size * (pct / 100.0)),
        _ => None,
    }
}

/// Cascade origins per W3C CSS Cascading and Inheritance Level 4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CascadeOrigin {
    UserAgent,
    User,
    Author,
    Inline,
}

/// An origin-annotated stylesheet for multi-origin cascade resolution.
#[derive(Debug, Clone, Copy)]
pub struct OriginStyleSheet<'a> {
    pub sheet: &'a StyleSheet,
    pub origin: CascadeOrigin,
}

impl<'a> OriginStyleSheet<'a> {
    pub fn new(sheet: &'a StyleSheet, origin: CascadeOrigin) -> Self {
        Self { sheet, origin }
    }

    pub fn user_agent(sheet: &'a StyleSheet) -> Self {
        Self {
            sheet,
            origin: CascadeOrigin::UserAgent,
        }
    }

    pub fn author(sheet: &'a StyleSheet) -> Self {
        Self {
            sheet,
            origin: CascadeOrigin::Author,
        }
    }
}

/// Computes declaration cascade priority rank per W3C Cascade Level 4:
/// Normal declarations: UserAgent (0) < User (1) < Author (2) < Inline (3)
/// Important declarations: Author !important (4) < Inline !important (5) < User !important (6) < UserAgent !important (7)
#[inline]
pub fn cascade_origin_rank(origin: CascadeOrigin, important: bool) -> u8 {
    if !important {
        match origin {
            CascadeOrigin::UserAgent => 0,
            CascadeOrigin::User => 1,
            CascadeOrigin::Author => 2,
            CascadeOrigin::Inline => 3,
        }
    } else {
        match origin {
            CascadeOrigin::Author => 4,
            CascadeOrigin::Inline => 5,
            CascadeOrigin::User => 6,
            CascadeOrigin::UserAgent => 7,
        }
    }
}

struct CascadeCandidate<'a> {
    declaration: &'a Declaration,
    rank: u8,
    layer_priority: u32,
    specificity: Specificity,
    source_order: usize,
}

/// The Strata CSS Cascade Solver.
pub struct CascadeSolver;

impl CascadeSolver {
    /// Resolves all matching CSS rules and inline styles into a final `ComputedStyle`.
    pub fn resolve(
        tag: &str,
        classes: &[&str],
        id: Option<&str>,
        inline_style: Option<&str>,
        stylesheets: &[&StyleSheet],
        parent_style: Option<&ComputedStyle>,
        state: &PseudoState,
    ) -> ComputedStyle {
        let owned_classes: Vec<String> = classes.iter().map(|s| s.to_string()).collect();
        let target = crate::selector::ElementRef {
            tag,
            classes: &owned_classes,
            id,
            attributes: &[],
        };
        Self::resolve_with_hierarchy(
            &target,
            &[],
            &[],
            inline_style,
            stylesheets,
            parent_style,
            state,
        )
    }

    /// Collects and resolves all matching CSS declarations into a sorted list considering hierarchy and specificity.
    pub fn collect_declarations_with_hierarchy(
        target: &crate::selector::ElementRef,
        ancestors: &[crate::selector::ElementRef],
        preceding_siblings: &[crate::selector::ElementRef],
        inline_style: Option<&str>,
        stylesheets: &[&StyleSheet],
        state: &PseudoState,
    ) -> Vec<Declaration> {
        let ua = crate::ua_stylesheet::get_ua_stylesheet();
        let mut origin_sheets = Vec::with_capacity(1 + stylesheets.len());
        origin_sheets.push(OriginStyleSheet::user_agent(ua));
        for s in stylesheets {
            origin_sheets.push(OriginStyleSheet::author(s));
        }

        Self::collect_declarations_with_origins(
            target,
            ancestors,
            preceding_siblings,
            inline_style,
            &origin_sheets,
            state,
        )
    }

    /// Collects and resolves all matching CSS declarations from multi-origin stylesheets.
    pub fn collect_declarations_with_origins(
        target: &crate::selector::ElementRef,
        ancestors: &[crate::selector::ElementRef],
        preceding_siblings: &[crate::selector::ElementRef],
        inline_style: Option<&str>,
        stylesheets: &[OriginStyleSheet],
        state: &PseudoState,
    ) -> Vec<Declaration> {
        let mut candidates = Vec::new();
        let mut global_order = 0;

        for origin_sheet in stylesheets {
            let sheet = origin_sheet.sheet;
            let origin = origin_sheet.origin;

            for rule in &sheet.rules {
                // Find highest specificity among matching selectors for this rule
                let mut best_match: Option<Specificity> = None;
                for selector in &rule.selectors {
                    if selector.matches_hierarchy(target, ancestors, preceding_siblings, state) {
                        match best_match {
                            Some(current) => {
                                if selector.specificity > current {
                                    best_match = Some(selector.specificity);
                                }
                            }
                            None => best_match = Some(selector.specificity),
                        }
                    }
                }

                if let Some(spec) = best_match {
                    for decl in &rule.declarations {
                        let rank = cascade_origin_rank(origin, decl.important);
                        let layer_priority = match (rule.layer_order, decl.important) {
                            (None, false) => u32::MAX, // unlayered normal beats all layered normal
                            (Some(k), false) => k + 1, // later layers beat earlier layers
                            (None, true) => 0, // unlayered !important is beaten by all layered !important
                            (Some(k), true) => u32::MAX.saturating_sub(k), // earlier layers beat later layers
                        };
                        candidates.push(CascadeCandidate {
                            declaration: decl,
                            rank,
                            layer_priority,
                            specificity: spec,
                            source_order: global_order,
                        });
                        global_order += 1;
                    }
                }
            }
        }

        // Parse and append inline style declarations
        let inline_declarations = inline_style.map(parse_declarations).unwrap_or_default();
        for decl in &inline_declarations {
            let rank = cascade_origin_rank(CascadeOrigin::Inline, decl.important);
            let layer_priority = if decl.important { 0 } else { u32::MAX };
            candidates.push(CascadeCandidate {
                declaration: decl,
                rank,
                layer_priority,
                specificity: Specificity::INLINE,
                source_order: global_order,
            });
            global_order += 1;
        }

        // Sort candidates by W3C cascade priority: (rank, layer_priority, specificity, source_order)
        candidates.sort_by(|a, b| {
            a.rank
                .cmp(&b.rank)
                .then_with(|| a.layer_priority.cmp(&b.layer_priority))
                .then_with(|| a.specificity.cmp(&b.specificity))
                .then_with(|| a.source_order.cmp(&b.source_order))
        });

        candidates
            .into_iter()
            .map(|c| c.declaration.clone())
            .collect()
    }

    /// Resolves all matching CSS rules considering hierarchical ancestor and sibling relationships.
    pub fn resolve_with_hierarchy(
        target: &crate::selector::ElementRef,
        ancestors: &[crate::selector::ElementRef],
        preceding_siblings: &[crate::selector::ElementRef],
        inline_style: Option<&str>,
        stylesheets: &[&StyleSheet],
        parent_style: Option<&ComputedStyle>,
        state: &PseudoState,
    ) -> ComputedStyle {
        // Step 1: Base style from inheritance or initial default
        let mut computed = match parent_style {
            Some(parent) => ComputedStyle::inherit_from(parent),
            None => ComputedStyle::default(),
        };

        // Step 2: Collect and apply declarations in sorted priority order
        let declarations = Self::collect_declarations_with_hierarchy(
            target,
            ancestors,
            preceding_siblings,
            inline_style,
            stylesheets,
            state,
        );

        for decl in &declarations {
            Self::apply_declaration_with_parent(&mut computed, decl, parent_style);
        }

        if target.tag.eq_ignore_ascii_case("html") || target.tag.eq_ignore_ascii_case(":root") {
            computed.root_font_size = computed.font_size;
        }

        // Step 3: Evaluate matching @starting-style rules (W3C CSS Transitions Level 2)
        let mut starting_decls = Vec::new();
        for sheet in stylesheets {
            for s_rule in &sheet.starting_styles {
                if s_rule
                    .selectors
                    .iter()
                    .any(|sel| sel.matches_hierarchy(target, ancestors, preceding_siblings, state))
                {
                    starting_decls.extend(s_rule.declarations.clone());
                }
            }
        }

        if !starting_decls.is_empty() {
            let mut starting_computed = computed.clone();
            starting_computed.starting_style = None;
            for decl in &starting_decls {
                Self::apply_declaration_with_parent(&mut starting_computed, decl, parent_style);
            }
            computed.starting_style = Some(Box::new(starting_computed));
        }

        computed
    }

    /// Applies an individual CSS declaration to a mutable `ComputedStyle`.
    pub fn apply_declaration(style: &mut ComputedStyle, decl: &Declaration) {
        Self::apply_declaration_with_parent(style, decl, None);
    }

    /// Applies an individual CSS declaration to a mutable `ComputedStyle`, resolving
    /// CSS `inherit` and `initial` keywords relative to parent style and W3C defaults.
    pub fn apply_declaration_with_parent(
        style: &mut ComputedStyle,
        decl: &Declaration,
        parent: Option<&ComputedStyle>,
    ) {
        let prop = decl.property.as_str();

        // CSS custom properties declaration: --accent-color: #ff0000;
        if prop.starts_with("--") {
            let key = strata_primitives::atom::intern(prop);
            let val = strata_primitives::atom::intern(&decl.value);
            style.set_custom_property(key, val);
            return;
        }

        let resolved_val = resolve_var(&decl.value, style);
        let val = resolved_val.as_ref();

        // W3C CSS Cascading & Inheritance Level 4: Explicit `inherit` keyword
        if val.trim().eq_ignore_ascii_case("inherit") {
            if let Some(p) = parent {
                match prop {
                    "color" => style.color = p.color,
                    "font-size" => style.font_size = p.font_size,
                    "font-family" => style.font_family = p.font_family,
                    "font-weight" => style.font_weight = p.font_weight,
                    "font-style" => style.font_style = p.font_style,
                    "text-align" => style.text_align = p.text_align,
                    "line-height" => style.line_height = p.line_height,
                    "letter-spacing" => style.letter_spacing = p.letter_spacing,
                    "text-decoration" => style.text_decoration = p.text_decoration,
                    "text-transform" => style.text_transform = p.text_transform,
                    "text-overflow" => style.text_overflow = p.text_overflow,
                    "white-space" => style.white_space = p.white_space,
                    "vertical-align" => style.vertical_align = p.vertical_align,
                    "visibility" => style.visibility = p.visibility,
                    "cursor" => style.cursor = p.cursor,
                    "user-select" => style.user_select = p.user_select,
                    "display" => style.display = p.display,
                    "padding" => style.padding = p.padding,
                    "margin" => style.margin = p.margin,
                    "width" => style.width = p.width,
                    "height" => style.height = p.height,
                    "min-width" => style.min_width = p.min_width,
                    "min-height" => style.min_height = p.min_height,
                    "max-width" => style.max_width = p.max_width,
                    "max-height" => style.max_height = p.max_height,
                    "background" | "background-color" => {
                        style.background_color = p.background_color
                    }
                    "background-image" => style.background_gradient = p.background_gradient.clone(),
                    "box-shadow" => style.box_shadow = p.box_shadow.clone(),
                    "backdrop-filter" | "-webkit-backdrop-filter" => {
                        style.backdrop_filter = p.backdrop_filter.clone()
                    }
                    "filter" => style.filter = p.filter.clone(),
                    "clip-path" => style.clip_path = p.clip_path.clone(),
                    "mix-blend-mode" => style.mix_blend_mode = p.mix_blend_mode,
                    "isolation" => style.isolation = p.isolation,
                    "border-color" => style.border_color = p.border_color,
                    "border-width" => style.border_width = p.border_width,
                    "border-radius" => style.border_radius = p.border_radius,
                    "border-collapse" => style.border_collapse = p.border_collapse,
                    "border-spacing" => style.border_spacing = p.border_spacing,
                    "caption-side" => style.caption_side = p.caption_side,
                    "opacity" => style.opacity = p.opacity,
                    "transition" => style.transitions = p.transitions.clone(),
                    "transform" => style.transform = p.transform.clone(),
                    "transform-origin" => style.transform_origin = p.transform_origin,
                    "transform-style" => style.transform_style = p.transform_style,
                    "perspective" => style.perspective = p.perspective,
                    "backface-visibility" => style.backface_visibility = p.backface_visibility,
                    _ => {}
                }
            }
            return;
        }

        // W3C CSS Cascading & Inheritance Level 4: Explicit `initial` keyword
        if val.trim().eq_ignore_ascii_case("initial") {
            let initial = ComputedStyle::default();
            match prop {
                "color" => style.color = initial.color,
                "font-size" => style.font_size = initial.font_size,
                "font-family" => style.font_family = initial.font_family,
                "font-weight" => style.font_weight = initial.font_weight,
                "font-style" => style.font_style = initial.font_style,
                "text-align" => style.text_align = initial.text_align,
                "line-height" => style.line_height = initial.line_height,
                "letter-spacing" => style.letter_spacing = initial.letter_spacing,
                "text-decoration" => style.text_decoration = initial.text_decoration,
                "text-transform" => style.text_transform = initial.text_transform,
                "text-overflow" => style.text_overflow = initial.text_overflow,
                "white-space" => style.white_space = initial.white_space,
                "vertical-align" => style.vertical_align = initial.vertical_align,
                "visibility" => style.visibility = initial.visibility,
                "cursor" => style.cursor = initial.cursor,
                "user-select" => style.user_select = initial.user_select,
                "display" => style.display = initial.display,
                "padding" => style.padding = initial.padding,
                "margin" => style.margin = initial.margin,
                "width" => style.width = initial.width,
                "height" => style.height = initial.height,
                "min-width" => style.min_width = initial.min_width,
                "min-height" => style.min_height = initial.min_height,
                "max-width" => style.max_width = initial.max_width,
                "max-height" => style.max_height = initial.max_height,
                "background" | "background-color" => {
                    style.background_color = initial.background_color
                }
                "background-image" => {
                    style.background_gradient = initial.background_gradient.clone()
                }
                "box-shadow" => style.box_shadow = initial.box_shadow.clone(),
                "backdrop-filter" | "-webkit-backdrop-filter" => {
                    style.backdrop_filter = initial.backdrop_filter.clone()
                }
                "filter" => style.filter = initial.filter.clone(),
                "clip-path" => style.clip_path = initial.clip_path.clone(),
                "mix-blend-mode" => style.mix_blend_mode = initial.mix_blend_mode,
                "isolation" => style.isolation = initial.isolation,
                "border-color" => style.border_color = initial.border_color,
                "border-width" => style.border_width = initial.border_width,
                "border-radius" => style.border_radius = initial.border_radius,
                "border-collapse" => style.border_collapse = initial.border_collapse,
                "border-spacing" => style.border_spacing = initial.border_spacing,
                "caption-side" => style.caption_side = initial.caption_side,
                "opacity" => style.opacity = initial.opacity,
                "transition" => style.transitions = initial.transitions.clone(),
                "transform" => style.transform = initial.transform.clone(),
                "transform-origin" => style.transform_origin = initial.transform_origin,
                "transform-style" => style.transform_style = initial.transform_style,
                "perspective" => style.perspective = initial.perspective,
                "backface-visibility" => style.backface_visibility = initial.backface_visibility,
                _ => {}
            }
            return;
        }

        match prop {
            "display" => match val {
                "block" => style.display = Display::Block,
                "inline" => style.display = Display::Inline,
                "inline-block" => style.display = Display::InlineBlock,
                "list-item" => style.display = Display::ListItem,
                "flex" => style.display = Display::Flex,
                "inline-flex" => style.display = Display::InlineFlex,
                "grid" => style.display = Display::Grid,
                "inline-grid" => style.display = Display::InlineGrid,
                "table" => style.display = Display::Table,
                "inline-table" => style.display = Display::InlineTable,
                "table-row-group" => style.display = Display::TableRowGroup,
                "table-header-group" => style.display = Display::TableHeaderGroup,
                "table-footer-group" => style.display = Display::TableFooterGroup,
                "table-row" => style.display = Display::TableRow,
                "table-cell" => style.display = Display::TableCell,
                "table-caption" => style.display = Display::TableCaption,
                "table-column-group" => style.display = Display::TableColumnGroup,
                "table-column" => style.display = Display::TableColumn,
                "none" => style.display = Display::None,
                _ => {}
            },
            "table-layout" => match val {
                "auto" => style.table_layout = TableLayout::Auto,
                "fixed" => style.table_layout = TableLayout::Fixed,
                _ => {}
            },
            "border-collapse" => match val {
                "collapse" => style.border_collapse = BorderCollapse::Collapse,
                "separate" => style.border_collapse = BorderCollapse::Separate,
                _ => {}
            },
            "border-spacing" => {
                let parts: Vec<&str> = val.split_whitespace().collect();
                if parts.len() == 1 {
                    if let Some(Dimension::Px(px)) = parse_dimension(parts[0]) {
                        style.border_spacing = (px, px);
                    }
                } else if parts.len() >= 2 {
                    if let (Some(Dimension::Px(h)), Some(Dimension::Px(v))) =
                        (parse_dimension(parts[0]), parse_dimension(parts[1]))
                    {
                        style.border_spacing = (h, v);
                    }
                }
            }
            "caption-side" => match val {
                "top" => style.caption_side = CaptionSide::Top,
                "bottom" => style.caption_side = CaptionSide::Bottom,
                _ => {}
            },
            "position" => match val {
                "static" => style.position = Position::Static,
                "relative" => style.position = Position::Relative,
                "absolute" => style.position = Position::Absolute,
                "fixed" => style.position = Position::Fixed,
                "sticky" => style.position = Position::Sticky,
                _ => {}
            },
            "float" => match val {
                "left" => style.float = Float::Left,
                "right" => style.float = Float::Right,
                "none" => style.float = Float::None,
                "inline-start" => style.float = Float::InlineStart,
                "inline-end" => style.float = Float::InlineEnd,
                _ => {}
            },
            "clear" => match val {
                "left" => style.clear = Clear::Left,
                "right" => style.clear = Clear::Right,
                "both" => style.clear = Clear::Both,
                "none" => style.clear = Clear::None,
                "inline-start" => style.clear = Clear::InlineStart,
                "inline-end" => style.clear = Clear::InlineEnd,
                _ => {}
            },
            "width" => {
                if let Some(dim) = parse_dimension(val) {
                    style.width = dim;
                }
            }
            "height" => {
                if let Some(dim) = parse_dimension(val) {
                    style.height = dim;
                }
            }
            "min-width" => {
                if let Some(dim) = parse_dimension(val) {
                    style.min_width = dim;
                }
            }
            "max-width" => {
                if let Some(dim) = parse_dimension(val) {
                    style.max_width = dim;
                }
            }
            "min-height" => {
                if let Some(dim) = parse_dimension(val) {
                    style.min_height = dim;
                }
            }
            "max-height" => {
                if let Some(dim) = parse_dimension(val) {
                    style.max_height = dim;
                }
            }
            "margin" => {
                let parts: Vec<&str> = val.split_whitespace().collect();
                let parse_side = |tok: &str| -> (f32, bool) {
                    if tok.eq_ignore_ascii_case("auto") {
                        (0.0, true)
                    } else if let Some(px) =
                        resolve_length_px(tok, style.font_size, style.root_font_size)
                    {
                        (px, false)
                    } else {
                        (0.0, false)
                    }
                };
                match parts.len() {
                    1 => {
                        let (v, a) = parse_side(parts[0]);
                        style.margin = EdgeInsets::all(v);
                        style.margin_auto_top = a;
                        style.margin_auto_right = a;
                        style.margin_auto_bottom = a;
                        style.margin_auto_left = a;
                    }
                    2 => {
                        let (v_tb, a_tb) = parse_side(parts[0]);
                        let (v_lr, a_lr) = parse_side(parts[1]);
                        style.margin = EdgeInsets::symmetric(v_tb, v_lr);
                        style.margin_auto_top = a_tb;
                        style.margin_auto_bottom = a_tb;
                        style.margin_auto_left = a_lr;
                        style.margin_auto_right = a_lr;
                    }
                    3 => {
                        let (v_t, a_t) = parse_side(parts[0]);
                        let (v_lr, a_lr) = parse_side(parts[1]);
                        let (v_b, a_b) = parse_side(parts[2]);
                        style.margin = EdgeInsets::new(v_t, v_lr, v_b, v_lr);
                        style.margin_auto_top = a_t;
                        style.margin_auto_left = a_lr;
                        style.margin_auto_right = a_lr;
                        style.margin_auto_bottom = a_b;
                    }
                    4 => {
                        let (v_t, a_t) = parse_side(parts[0]);
                        let (v_r, a_r) = parse_side(parts[1]);
                        let (v_b, a_b) = parse_side(parts[2]);
                        let (v_l, a_l) = parse_side(parts[3]);
                        style.margin = EdgeInsets::new(v_t, v_r, v_b, v_l);
                        style.margin_auto_top = a_t;
                        style.margin_auto_right = a_r;
                        style.margin_auto_bottom = a_b;
                        style.margin_auto_left = a_l;
                    }
                    _ => {}
                }
            }
            "margin-top" => {
                if val.trim() == "auto" {
                    style.margin_auto_top = true;
                } else if let Some(px) =
                    resolve_length_px(val, style.font_size, style.root_font_size)
                {
                    style.margin.top = px;
                    style.margin_auto_top = false;
                }
            }
            "margin-right" => {
                if val.trim() == "auto" {
                    style.margin_auto_right = true;
                } else if let Some(px) =
                    resolve_length_px(val, style.font_size, style.root_font_size)
                {
                    style.margin.right = px;
                    style.margin_auto_right = false;
                }
            }
            "margin-bottom" => {
                if val.trim() == "auto" {
                    style.margin_auto_bottom = true;
                } else if let Some(px) =
                    resolve_length_px(val, style.font_size, style.root_font_size)
                {
                    style.margin.bottom = px;
                    style.margin_auto_bottom = false;
                }
            }
            "margin-left" => {
                if val.trim() == "auto" {
                    style.margin_auto_left = true;
                } else if let Some(px) =
                    resolve_length_px(val, style.font_size, style.root_font_size)
                {
                    style.margin.left = px;
                    style.margin_auto_left = false;
                }
            }
            "margin-block" => {
                let parts: Vec<&str> = val.split_whitespace().collect();
                if parts.len() == 1 {
                    if parts[0] == "auto" {
                        style.margin_auto_top = true;
                        style.margin_auto_bottom = true;
                    } else if let Some(px) =
                        resolve_length_px(parts[0], style.font_size, style.root_font_size)
                    {
                        style.margin.top = px;
                        style.margin.bottom = px;
                        style.margin_auto_top = false;
                        style.margin_auto_bottom = false;
                    }
                } else if parts.len() >= 2 {
                    if parts[0] == "auto" {
                        style.margin_auto_top = true;
                    } else if let Some(top) =
                        resolve_length_px(parts[0], style.font_size, style.root_font_size)
                    {
                        style.margin.top = top;
                        style.margin_auto_top = false;
                    }
                    if parts[1] == "auto" {
                        style.margin_auto_bottom = true;
                    } else if let Some(bot) =
                        resolve_length_px(parts[1], style.font_size, style.root_font_size)
                    {
                        style.margin.bottom = bot;
                        style.margin_auto_bottom = false;
                    }
                }
            }
            "margin-block-start" => {
                if val.trim() == "auto" {
                    style.margin_auto_top = true;
                } else if let Some(px) =
                    resolve_length_px(val, style.font_size, style.root_font_size)
                {
                    style.margin.top = px;
                    style.margin_auto_top = false;
                }
            }
            "margin-block-end" => {
                if val.trim() == "auto" {
                    style.margin_auto_bottom = true;
                } else if let Some(px) =
                    resolve_length_px(val, style.font_size, style.root_font_size)
                {
                    style.margin.bottom = px;
                    style.margin_auto_bottom = false;
                }
            }
            "margin-inline" => {
                let parts: Vec<&str> = val.split_whitespace().collect();
                if parts.len() == 1 {
                    if parts[0] == "auto" {
                        style.margin_auto_left = true;
                        style.margin_auto_right = true;
                    } else if let Some(px) =
                        resolve_length_px(parts[0], style.font_size, style.root_font_size)
                    {
                        style.margin.left = px;
                        style.margin.right = px;
                        style.margin_auto_left = false;
                        style.margin_auto_right = false;
                    }
                } else if parts.len() >= 2 {
                    if parts[0] == "auto" {
                        style.margin_auto_left = true;
                    } else if let Some(left) =
                        resolve_length_px(parts[0], style.font_size, style.root_font_size)
                    {
                        style.margin.left = left;
                        style.margin_auto_left = false;
                    }
                    if parts[1] == "auto" {
                        style.margin_auto_right = true;
                    } else if let Some(right) =
                        resolve_length_px(parts[1], style.font_size, style.root_font_size)
                    {
                        style.margin.right = right;
                        style.margin_auto_right = false;
                    }
                }
            }
            "margin-inline-start" => {
                if val.trim() == "auto" {
                    style.margin_auto_left = true;
                } else if let Some(px) =
                    resolve_length_px(val, style.font_size, style.root_font_size)
                {
                    style.margin.left = px;
                    style.margin_auto_left = false;
                }
            }
            "margin-inline-end" => {
                if val.trim() == "auto" {
                    style.margin_auto_right = true;
                } else if let Some(px) =
                    resolve_length_px(val, style.font_size, style.root_font_size)
                {
                    style.margin.right = px;
                    style.margin_auto_right = false;
                }
            }
            "padding" => {
                if let Some(insets) = crate::parser::parse_edge_insets_with_context(
                    val,
                    style.font_size,
                    style.root_font_size,
                ) {
                    style.padding = insets;
                }
            }
            "padding-top" => {
                if let Some(px) = resolve_length_px(val, style.font_size, style.root_font_size) {
                    style.padding.top = px;
                }
            }
            "padding-right" => {
                if let Some(px) = resolve_length_px(val, style.font_size, style.root_font_size) {
                    style.padding.right = px;
                }
            }
            "padding-bottom" => {
                if let Some(px) = resolve_length_px(val, style.font_size, style.root_font_size) {
                    style.padding.bottom = px;
                }
            }
            "padding-left" => {
                if let Some(px) = resolve_length_px(val, style.font_size, style.root_font_size) {
                    style.padding.left = px;
                }
            }
            "padding-block" => {
                let parts: Vec<&str> = val.split_whitespace().collect();
                if parts.len() == 1 {
                    if let Some(px) =
                        resolve_length_px(parts[0], style.font_size, style.root_font_size)
                    {
                        style.padding.top = px;
                        style.padding.bottom = px;
                    }
                } else if parts.len() >= 2 {
                    if let Some(top) =
                        resolve_length_px(parts[0], style.font_size, style.root_font_size)
                    {
                        style.padding.top = top;
                    }
                    if let Some(bot) =
                        resolve_length_px(parts[1], style.font_size, style.root_font_size)
                    {
                        style.padding.bottom = bot;
                    }
                }
            }
            "padding-block-start" => {
                if let Some(px) = resolve_length_px(val, style.font_size, style.root_font_size) {
                    style.padding.top = px;
                }
            }
            "padding-block-end" => {
                if let Some(px) = resolve_length_px(val, style.font_size, style.root_font_size) {
                    style.padding.bottom = px;
                }
            }
            "padding-inline" => {
                let parts: Vec<&str> = val.split_whitespace().collect();
                if parts.len() == 1 {
                    if let Some(px) =
                        resolve_length_px(parts[0], style.font_size, style.root_font_size)
                    {
                        style.padding.left = px;
                        style.padding.right = px;
                    }
                } else if parts.len() >= 2 {
                    if let Some(left) =
                        resolve_length_px(parts[0], style.font_size, style.root_font_size)
                    {
                        style.padding.left = left;
                    }
                    if let Some(right) =
                        resolve_length_px(parts[1], style.font_size, style.root_font_size)
                    {
                        style.padding.right = right;
                    }
                }
            }
            "padding-inline-start" => {
                if let Some(px) = resolve_length_px(val, style.font_size, style.root_font_size) {
                    style.padding.left = px;
                }
            }
            "padding-inline-end" => {
                if let Some(px) = resolve_length_px(val, style.font_size, style.root_font_size) {
                    style.padding.right = px;
                }
            }
            "top" => {
                if let Some(dim) = parse_dimension(val) {
                    style.insets.top = dim;
                }
            }
            "right" => {
                if let Some(dim) = parse_dimension(val) {
                    style.insets.right = dim;
                }
            }
            "bottom" => {
                if let Some(dim) = parse_dimension(val) {
                    style.insets.bottom = dim;
                }
            }
            "left" => {
                if let Some(dim) = parse_dimension(val) {
                    style.insets.left = dim;
                }
            }
            "z-index" => {
                if let Ok(z) = val.parse::<i32>() {
                    style.z_index = Some(z);
                }
            }
            "flex-direction" => match val {
                "row" => style.flex_direction = FlexDirection::Row,
                "row-reverse" => style.flex_direction = FlexDirection::RowReverse,
                "column" => style.flex_direction = FlexDirection::Column,
                "column-reverse" => style.flex_direction = FlexDirection::ColumnReverse,
                _ => {}
            },
            "flex-wrap" => match val {
                "nowrap" => style.flex_wrap = FlexWrap::NoWrap,
                "wrap" => style.flex_wrap = FlexWrap::Wrap,
                "wrap-reverse" => style.flex_wrap = FlexWrap::WrapReverse,
                _ => {}
            },
            "justify-content" => match val {
                "flex-start" | "start" => style.justify_content = JustifyContent::FlexStart,
                "flex-end" | "end" => style.justify_content = JustifyContent::FlexEnd,
                "center" => style.justify_content = JustifyContent::Center,
                "space-between" => style.justify_content = JustifyContent::SpaceBetween,
                "space-around" => style.justify_content = JustifyContent::SpaceAround,
                "space-evenly" => style.justify_content = JustifyContent::SpaceEvenly,
                _ => {}
            },
            "align-items" => match val {
                "stretch" => style.align_items = AlignItems::Stretch,
                "flex-start" | "start" => style.align_items = AlignItems::FlexStart,
                "flex-end" | "end" => style.align_items = AlignItems::FlexEnd,
                "center" => style.align_items = AlignItems::Center,
                "baseline" => style.align_items = AlignItems::Baseline,
                _ => {}
            },
            "align-self" => match val {
                "auto" => style.align_self = AlignSelf::Auto,
                "stretch" => style.align_self = AlignSelf::Stretch,
                "flex-start" | "start" => style.align_self = AlignSelf::FlexStart,
                "flex-end" | "end" => style.align_self = AlignSelf::FlexEnd,
                "center" => style.align_self = AlignSelf::Center,
                "baseline" => style.align_self = AlignSelf::Baseline,
                _ => {}
            },
            "flex-grow" => {
                if let Ok(fg) = val.parse::<f32>() {
                    style.flex_grow = fg;
                }
            }
            "flex-shrink" => {
                if let Ok(fs) = val.parse::<f32>() {
                    style.flex_shrink = fs;
                }
            }
            "flex-basis" => {
                if let Some(dim) = parse_dimension(val) {
                    style.flex_basis = dim;
                }
            }
            "flex" => {
                let parts: Vec<&str> = val.split_whitespace().collect();
                match parts.len() {
                    1 => {
                        let single = parts[0].to_ascii_lowercase();
                        if single == "none" {
                            style.flex_grow = 0.0;
                            style.flex_shrink = 0.0;
                            style.flex_basis = Dimension::Auto;
                        } else if single == "auto" {
                            style.flex_grow = 1.0;
                            style.flex_shrink = 1.0;
                            style.flex_basis = Dimension::Auto;
                        } else if single == "initial" {
                            style.flex_grow = 0.0;
                            style.flex_shrink = 1.0;
                            style.flex_basis = Dimension::Auto;
                        } else if let Ok(num) = single.parse::<f32>() {
                            style.flex_grow = num;
                            style.flex_shrink = 1.0;
                            style.flex_basis = Dimension::Px(0.0);
                        } else if let Some(dim) = parse_dimension(&single) {
                            style.flex_basis = dim;
                        }
                    }
                    2 => {
                        if let (Ok(g), Ok(s)) = (parts[0].parse::<f32>(), parts[1].parse::<f32>()) {
                            style.flex_grow = g;
                            style.flex_shrink = s;
                            style.flex_basis = Dimension::Px(0.0);
                        } else if let (Ok(g), Some(dim)) =
                            (parts[0].parse::<f32>(), parse_dimension(parts[1]))
                        {
                            style.flex_grow = g;
                            style.flex_shrink = 1.0;
                            style.flex_basis = dim;
                        }
                    }
                    3 => {
                        if let Ok(g) = parts[0].parse::<f32>() {
                            style.flex_grow = g;
                        }
                        if let Ok(s) = parts[1].parse::<f32>() {
                            style.flex_shrink = s;
                        }
                        if parts[2].eq_ignore_ascii_case("auto") {
                            style.flex_basis = Dimension::Auto;
                        } else if let Some(dim) = parse_dimension(parts[2]) {
                            style.flex_basis = dim;
                        }
                    }
                    _ => {}
                }
            }
            "gap" => {
                if let Some(px) = resolve_length_px(val, style.font_size, style.root_font_size) {
                    style.gap = px;
                    style.gap_row = px;
                    style.gap_col = px;
                }
            }
            "row-gap" => {
                if let Some(px) = resolve_length_px(val, style.font_size, style.root_font_size) {
                    style.gap_row = px;
                }
            }
            "column-gap" => {
                if let Some(px) = resolve_length_px(val, style.font_size, style.root_font_size) {
                    style.gap_col = px;
                }
            }
            "grid-template-columns" => {
                style.grid_template_columns = crate::parser::parse_grid_tracks(val);
            }
            "grid-template-rows" => {
                style.grid_template_rows = crate::parser::parse_grid_tracks(val);
            }
            "grid-auto-flow" => {
                if let Some(flow) = GridAutoFlow::parse(val) {
                    style.grid_auto_flow = flow;
                }
            }
            "grid-column" => {
                apply_grid_placement_shorthand(
                    val,
                    &mut style.grid_column_start,
                    &mut style.grid_column_span,
                );
            }
            "grid-column-start" => {
                apply_grid_placement_line(
                    val,
                    &mut style.grid_column_start,
                    &mut style.grid_column_span,
                );
            }
            "grid-column-end" => {
                apply_grid_placement_end(val, style.grid_column_start, &mut style.grid_column_span);
            }
            "grid-row" => {
                apply_grid_placement_shorthand(
                    val,
                    &mut style.grid_row_start,
                    &mut style.grid_row_span,
                );
            }
            "grid-row-start" => {
                apply_grid_placement_line(val, &mut style.grid_row_start, &mut style.grid_row_span);
            }
            "grid-row-end" => {
                apply_grid_placement_end(val, style.grid_row_start, &mut style.grid_row_span);
            }
            "background-color" => {
                if let Some(c) = parse_color(val) {
                    style.background_color = Some(c);
                }
            }
            "background" => {
                let trimmed = val.trim();
                if trimmed.contains("gradient") {
                    if let Some(grad) = crate::parser::parse_gradient(trimmed) {
                        style.background_gradient = Some(grad);
                    }
                }
                if let Some(c) = parse_color(trimmed) {
                    style.background_color = Some(c);
                }
            }
            "background-image" => {
                let trimmed = val.trim();
                if trimmed.eq_ignore_ascii_case("none") {
                    style.background_gradient = None;
                } else if let Some(grad) = crate::parser::parse_gradient(trimmed) {
                    style.background_gradient = Some(grad);
                }
            }
            "color" => {
                if let Some(c) = parse_color(val) {
                    style.color = c;
                }
            }
            "border" => {
                if val == "none" || val == "0" {
                    style.border_width = EdgeInsets::ZERO;
                    style.border_style = BorderStyle::None;
                } else {
                    for part in val.split_whitespace() {
                        if let Some(bs) = parse_border_style(part) {
                            style.border_style = bs;
                        } else if let Some(px) =
                            resolve_length_px(part, style.font_size, style.root_font_size)
                        {
                            style.border_width = EdgeInsets::all(px);
                        } else if let Some(c) = parse_color(part) {
                            style.border_color = Some(c);
                        }
                    }
                }
            }
            "border-color" => {
                if let Some(c) = parse_color(val) {
                    style.border_color = Some(c);
                }
            }
            "border-top-color"
            | "border-right-color"
            | "border-bottom-color"
            | "border-left-color" => {
                if let Some(c) = parse_color(val) {
                    style.border_color = Some(c);
                }
            }
            "border-width" => {
                if let Some(insets) = crate::parser::parse_edge_insets_with_context(
                    val,
                    style.font_size,
                    style.root_font_size,
                ) {
                    style.border_width = insets;
                }
            }
            "border-top-width" => {
                if let Some(px) = resolve_length_px(val, style.font_size, style.root_font_size) {
                    style.border_width.top = px;
                }
            }
            "border-right-width" => {
                if let Some(px) = resolve_length_px(val, style.font_size, style.root_font_size) {
                    style.border_width.right = px;
                }
            }
            "border-bottom-width" => {
                if let Some(px) = resolve_length_px(val, style.font_size, style.root_font_size) {
                    style.border_width.bottom = px;
                }
            }
            "border-left-width" => {
                if let Some(px) = resolve_length_px(val, style.font_size, style.root_font_size) {
                    style.border_width.left = px;
                }
            }
            "border-radius" => {
                if let Some(px) = resolve_length_px(val, style.font_size, style.root_font_size) {
                    style.border_radius = BorderRadius::all(px);
                }
            }
            "opacity" => {
                if let Ok(op) = val.parse::<f32>() {
                    style.opacity = op.clamp(0.0, 1.0);
                }
            }
            "box-shadow" => {
                let trimmed = val.trim();
                if trimmed.eq_ignore_ascii_case("none") {
                    style.box_shadow.clear();
                } else {
                    style.box_shadow = crate::parser::parse_box_shadow(trimmed);
                }
            }
            "backdrop-filter" | "-webkit-backdrop-filter" => {
                let trimmed = val.trim();
                if trimmed.eq_ignore_ascii_case("none") {
                    style.backdrop_filter = None;
                } else {
                    style.backdrop_filter = crate::parser::parse_filter_list(trimmed);
                }
            }
            "filter" => {
                let trimmed = val.trim();
                if trimmed.eq_ignore_ascii_case("none") {
                    style.filter = None;
                } else {
                    style.filter = crate::parser::parse_filter_list(trimmed);
                }
            }
            "clip-path" => {
                let trimmed = val.trim();
                if trimmed.eq_ignore_ascii_case("none") {
                    style.clip_path = None;
                } else {
                    style.clip_path = crate::parser::parse_clip_path(trimmed);
                }
            }
            "mix-blend-mode" => {
                if let Some(bm) = BlendMode::parse(val) {
                    style.mix_blend_mode = bm;
                }
            }
            "isolation" => {
                if let Some(iso) = Isolation::parse(val) {
                    style.isolation = iso;
                }
            }
            "font-size" => match val.trim().to_ascii_lowercase().as_str() {
                "xx-small" => style.font_size = 9.0,
                "x-small" => style.font_size = 10.0,
                "small" => style.font_size = 13.0,
                "medium" => style.font_size = 16.0,
                "large" => style.font_size = 18.0,
                "x-large" => style.font_size = 24.0,
                "xx-large" => style.font_size = 32.0,
                "smaller" => style.font_size = (style.font_size * 0.833).max(8.0),
                "larger" => style.font_size *= 1.2,
                _ => {
                    if let Some(dim) = parse_dimension(val) {
                        match dim {
                            Dimension::Px(px) => style.font_size = px,
                            Dimension::Em(em) => style.font_size = (style.font_size * em).max(1.0),
                            Dimension::Rem(rem) => {
                                style.font_size = (style.root_font_size * rem).max(1.0)
                            }
                            Dimension::Percent(pct) => {
                                style.font_size = (style.font_size * (pct / 100.0)).max(1.0)
                            }
                            _ => {}
                        }
                    }
                }
            },
            "font-weight" => match val.trim().to_ascii_lowercase().as_str() {
                "bold" => style.font_weight = FontWeight::Bold,
                "bolder" => style.font_weight = FontWeight::ExtraBold,
                "lighter" => style.font_weight = FontWeight::Light,
                "normal" => style.font_weight = FontWeight::Normal,
                other => {
                    if let Ok(w) = other.parse::<u16>() {
                        style.font_weight = FontWeight::from_u16(w);
                    }
                }
            },
            "font-style" => match val {
                "italic" | "oblique" => style.font_style = FontStyle::Italic,
                "normal" => style.font_style = FontStyle::Normal,
                _ => {}
            },
            "text-align" => match val {
                "left" => style.text_align = TextAlign::Left,
                "center" => style.text_align = TextAlign::Center,
                "right" => style.text_align = TextAlign::Right,
                "justify" => style.text_align = TextAlign::Justify,
                _ => {}
            },
            "line-height" => {
                let trimmed = val.trim().to_ascii_lowercase();
                if trimmed == "normal" {
                    style.line_height = 1.2;
                } else if let Ok(lh) = trimmed.parse::<f32>() {
                    style.line_height = lh;
                } else if let Some(dim) = parse_dimension(&trimmed) {
                    match dim {
                        Dimension::Px(px) => {
                            if style.font_size > 0.0 {
                                style.line_height = px / style.font_size;
                            }
                        }
                        Dimension::Em(em) => style.line_height = em,
                        Dimension::Rem(rem) => {
                            if style.font_size > 0.0 {
                                style.line_height = (rem * 16.0) / style.font_size;
                            }
                        }
                        Dimension::Percent(pct) => style.line_height = pct / 100.0,
                        _ => {}
                    }
                }
            }
            "letter-spacing" => {
                if let Some(Dimension::Px(px)) = parse_dimension(val) {
                    style.letter_spacing = px;
                }
            }
            "overflow" => match val {
                "visible" => {
                    style.overflow_x = Overflow::Visible;
                    style.overflow_y = Overflow::Visible;
                }
                "hidden" => {
                    style.overflow_x = Overflow::Hidden;
                    style.overflow_y = Overflow::Hidden;
                }
                "scroll" => {
                    style.overflow_x = Overflow::Scroll;
                    style.overflow_y = Overflow::Scroll;
                }
                "auto" => {
                    style.overflow_x = Overflow::Auto;
                    style.overflow_y = Overflow::Auto;
                }
                _ => {}
            },
            "overflow-x" => match val {
                "visible" => style.overflow_x = Overflow::Visible,
                "hidden" => style.overflow_x = Overflow::Hidden,
                "scroll" => style.overflow_x = Overflow::Scroll,
                "auto" => style.overflow_x = Overflow::Auto,
                _ => {}
            },
            "overflow-y" => match val {
                "visible" => style.overflow_y = Overflow::Visible,
                "hidden" => style.overflow_y = Overflow::Hidden,
                "scroll" => style.overflow_y = Overflow::Scroll,
                "auto" => style.overflow_y = Overflow::Auto,
                _ => {}
            },
            "box-sizing" => match val {
                "border-box" => style.box_sizing = BoxSizing::BorderBox,
                "content-box" => style.box_sizing = BoxSizing::ContentBox,
                _ => {}
            },
            "font-family" => {
                let trimmed = val.trim_matches('"').trim_matches('\'').trim();
                if !trimmed.is_empty() {
                    style.font_family = strata_primitives::atom::intern(trimmed);
                }
            }
            "text-decoration" | "text-decoration-line" => {
                if let Some(td) = parse_text_decoration(val) {
                    style.text_decoration = td;
                }
            }
            "text-transform" => {
                if let Some(tt) = parse_text_transform(val) {
                    style.text_transform = tt;
                }
            }
            "text-overflow" => match val {
                "ellipsis" => style.text_overflow = TextOverflow::Ellipsis,
                "clip" => style.text_overflow = TextOverflow::Clip,
                _ => {}
            },
            "white-space" => {
                if let Some(ws) = parse_white_space(val) {
                    style.white_space = ws;
                }
            }
            "vertical-align" => {
                if let Some(va) = parse_vertical_align(val) {
                    style.vertical_align = va;
                }
            }
            "cursor" => {
                if let Some(c) = parse_cursor(val) {
                    style.cursor = c;
                }
            }
            "caret-color" => {
                if val.trim().eq_ignore_ascii_case("auto") {
                    style.caret_color = None;
                } else if let Some(c) = parse_color(val) {
                    style.caret_color = Some(c);
                }
            }
            "user-select" => {
                if let Some(us) = parse_user_select(val) {
                    style.user_select = us;
                }
            }
            "pointer-events" => {
                if let Some(pe) = parse_pointer_events(val) {
                    style.pointer_events = pe;
                }
            }
            "visibility" => {
                if let Some(v) = parse_visibility(val) {
                    style.visibility = v;
                }
            }
            "border-style" => {
                if let Some(bs) = parse_border_style(val) {
                    style.border_style = bs;
                }
            }
            "aspect-ratio" => {
                if let Some(ar) = parse_aspect_ratio(val) {
                    style.aspect_ratio = Some(ar);
                }
            }
            "outline" => {
                if val == "none" || val == "0" {
                    style.outline_width = 0.0;
                    style.outline_style = BorderStyle::None;
                } else {
                    for part in val.split_whitespace() {
                        if let Some(bs) = parse_border_style(part) {
                            style.outline_style = bs;
                        } else if let Some(Dimension::Px(px)) = parse_dimension(part) {
                            style.outline_width = px;
                        } else if let Some(c) = parse_color(part) {
                            style.outline_color = Some(c);
                        }
                    }
                }
            }
            "outline-width" => {
                if let Some(Dimension::Px(px)) = parse_dimension(val) {
                    style.outline_width = px;
                }
            }
            "outline-style" => {
                if let Some(bs) = parse_border_style(val) {
                    style.outline_style = bs;
                }
            }
            "outline-color" => {
                if let Some(c) = parse_color(val) {
                    style.outline_color = Some(c);
                }
            }
            "outline-offset" => {
                if let Some(Dimension::Px(px)) = parse_dimension(val) {
                    style.outline_offset = px;
                }
            }
            "contain" => {
                let trimmed = val.trim().to_ascii_lowercase();
                if trimmed == "none" {
                    style.contain = Containment::NONE;
                } else if trimmed == "strict" {
                    style.contain = Containment::STRICT;
                } else if trimmed == "content" {
                    style.contain = Containment::CONTENT;
                } else {
                    let mut c = Containment::NONE;
                    for part in trimmed.split_whitespace() {
                        match part {
                            "size" => c.size = true,
                            "inline-size" => c.inline_size = true,
                            "layout" => c.layout = true,
                            "paint" => c.paint = true,
                            "style" => c.style = true,
                            _ => {}
                        }
                    }
                    style.contain = c;
                }
            }
            "content-visibility" => match val.trim().to_ascii_lowercase().as_str() {
                "auto" => style.content_visibility = ContentVisibility::Auto,
                "hidden" => style.content_visibility = ContentVisibility::Hidden,
                "visible" => style.content_visibility = ContentVisibility::Visible,
                _ => {}
            },
            "contain-intrinsic-size" | "contain-intrinsic-width" | "contain-intrinsic-height" => {
                let parts: Vec<&str> = val.split_whitespace().collect();
                let dims: Vec<Dimension> = parts
                    .iter()
                    .filter(|&&p| p != "auto")
                    .filter_map(|&p| parse_dimension(p))
                    .collect();
                if prop == "contain-intrinsic-width" {
                    if let Some(dim) = dims.first() {
                        style.contain_intrinsic_width = Some(*dim);
                    }
                } else if prop == "contain-intrinsic-height" {
                    if let Some(dim) = dims.first() {
                        style.contain_intrinsic_height = Some(*dim);
                    }
                } else {
                    match dims.len() {
                        1 => {
                            style.contain_intrinsic_width = Some(dims[0]);
                            style.contain_intrinsic_height = Some(dims[0]);
                        }
                        2.. => {
                            style.contain_intrinsic_width = Some(dims[0]);
                            style.contain_intrinsic_height = Some(dims[1]);
                        }
                        _ => {}
                    }
                }
            }
            "transition" => {
                style.transitions = parse_transitions(val);
            }
            "transition-property" => {
                let props = parse_transition_properties(val);
                if style.transitions.is_empty() {
                    style.transitions = props
                        .into_iter()
                        .map(|p| TransitionSpec {
                            property: p,
                            ..Default::default()
                        })
                        .collect();
                } else {
                    let n = props.len();
                    let mut new_transitions = Vec::with_capacity(n);
                    for (i, p) in props.into_iter().enumerate() {
                        let mut spec = style.transitions[i % style.transitions.len()].clone();
                        spec.property = p;
                        new_transitions.push(spec);
                    }
                    style.transitions = new_transitions;
                }
            }
            "transition-duration" => {
                let durations = parse_transition_durations(val);
                if !durations.is_empty() {
                    if style.transitions.is_empty() {
                        style.transitions = durations
                            .into_iter()
                            .map(|d| TransitionSpec {
                                duration_ms: d,
                                ..Default::default()
                            })
                            .collect();
                    } else {
                        let m = durations.len();
                        for (i, spec) in style.transitions.iter_mut().enumerate() {
                            spec.duration_ms = durations[i % m];
                        }
                    }
                }
            }
            "transition-timing-function" => {
                let easings = parse_transition_timing_functions(val);
                if !easings.is_empty() {
                    if style.transitions.is_empty() {
                        style.transitions = easings
                            .into_iter()
                            .map(|e| TransitionSpec {
                                timing_function: e,
                                ..Default::default()
                            })
                            .collect();
                    } else {
                        let m = easings.len();
                        for (i, spec) in style.transitions.iter_mut().enumerate() {
                            spec.timing_function = easings[i % m];
                        }
                    }
                }
            }
            "transition-delay" => {
                let delays = parse_transition_delays(val);
                if !delays.is_empty() {
                    if style.transitions.is_empty() {
                        style.transitions = delays
                            .into_iter()
                            .map(|d| TransitionSpec {
                                delay_ms: d,
                                ..Default::default()
                            })
                            .collect();
                    } else {
                        let m = delays.len();
                        for (i, spec) in style.transitions.iter_mut().enumerate() {
                            spec.delay_ms = delays[i % m];
                        }
                    }
                }
            }
            "transition-behavior" => {
                let behaviors = parse_transition_behaviors(val);
                if !behaviors.is_empty() {
                    if style.transitions.is_empty() {
                        style.transitions = behaviors
                            .into_iter()
                            .map(|b| TransitionSpec {
                                behavior: b,
                                ..Default::default()
                            })
                            .collect();
                    } else {
                        let m = behaviors.len();
                        for (i, spec) in style.transitions.iter_mut().enumerate() {
                            spec.behavior = behaviors[i % m];
                        }
                    }
                }
            }
            "animation" => {
                style.animations = parse_animations(val);
            }
            "animation-name" => {
                let names = parse_animation_names(val);
                if style.animations.is_empty() {
                    style.animations = names
                        .into_iter()
                        .map(|n| AnimationSpec {
                            name: n,
                            ..Default::default()
                        })
                        .collect();
                } else {
                    let n = names.len();
                    let mut new_anims = Vec::with_capacity(n);
                    for (i, name) in names.into_iter().enumerate() {
                        let mut spec = style.animations[i % style.animations.len()].clone();
                        spec.name = name;
                        new_anims.push(spec);
                    }
                    style.animations = new_anims;
                }
            }
            "animation-duration" => {
                let durations = parse_animation_durations(val);
                if !durations.is_empty() {
                    if style.animations.is_empty() {
                        style.animations = durations
                            .into_iter()
                            .map(|d| AnimationSpec {
                                duration_ms: d,
                                ..Default::default()
                            })
                            .collect();
                    } else {
                        let m = durations.len();
                        for (i, spec) in style.animations.iter_mut().enumerate() {
                            spec.duration_ms = durations[i % m];
                        }
                    }
                }
            }
            "animation-timing-function" => {
                let easings = parse_animation_timing_functions(val);
                if !easings.is_empty() {
                    if style.animations.is_empty() {
                        style.animations = easings
                            .into_iter()
                            .map(|e| AnimationSpec {
                                timing_function: e,
                                ..Default::default()
                            })
                            .collect();
                    } else {
                        let m = easings.len();
                        for (i, spec) in style.animations.iter_mut().enumerate() {
                            spec.timing_function = easings[i % m];
                        }
                    }
                }
            }
            "animation-delay" => {
                let delays = parse_animation_delays(val);
                if !delays.is_empty() {
                    if style.animations.is_empty() {
                        style.animations = delays
                            .into_iter()
                            .map(|d| AnimationSpec {
                                delay_ms: d,
                                ..Default::default()
                            })
                            .collect();
                    } else {
                        let m = delays.len();
                        for (i, spec) in style.animations.iter_mut().enumerate() {
                            spec.delay_ms = delays[i % m];
                        }
                    }
                }
            }
            "animation-iteration-count" => {
                let counts = parse_animation_iteration_counts(val);
                if !counts.is_empty() {
                    if style.animations.is_empty() {
                        style.animations = counts
                            .into_iter()
                            .map(|c| AnimationSpec {
                                iteration_count: c,
                                ..Default::default()
                            })
                            .collect();
                    } else {
                        let m = counts.len();
                        for (i, spec) in style.animations.iter_mut().enumerate() {
                            spec.iteration_count = counts[i % m];
                        }
                    }
                }
            }
            "animation-direction" => {
                let directions = parse_animation_directions(val);
                if !directions.is_empty() {
                    if style.animations.is_empty() {
                        style.animations = directions
                            .into_iter()
                            .map(|d| AnimationSpec {
                                direction: d,
                                ..Default::default()
                            })
                            .collect();
                    } else {
                        let m = directions.len();
                        for (i, spec) in style.animations.iter_mut().enumerate() {
                            spec.direction = directions[i % m];
                        }
                    }
                }
            }
            "animation-fill-mode" => {
                let fill_modes = parse_animation_fill_modes(val);
                if !fill_modes.is_empty() {
                    if style.animations.is_empty() {
                        style.animations = fill_modes
                            .into_iter()
                            .map(|f| AnimationSpec {
                                fill_mode: f,
                                ..Default::default()
                            })
                            .collect();
                    } else {
                        let m = fill_modes.len();
                        for (i, spec) in style.animations.iter_mut().enumerate() {
                            spec.fill_mode = fill_modes[i % m];
                        }
                    }
                }
            }
            "animation-play-state" => {
                let play_states = parse_animation_play_states(val);
                if !play_states.is_empty() {
                    if style.animations.is_empty() {
                        style.animations = play_states
                            .into_iter()
                            .map(|ps| AnimationSpec {
                                play_state: ps,
                                ..Default::default()
                            })
                            .collect();
                    } else {
                        let m = play_states.len();
                        for (i, spec) in style.animations.iter_mut().enumerate() {
                            spec.play_state = play_states[i % m];
                        }
                    }
                }
            }
            "animation-timeline" => {
                let timeline = parse_animation_timeline(val);
                style.animation_timeline = timeline.clone();
                if style.animations.is_empty() {
                    style.animations.push(AnimationSpec {
                        timeline,
                        ..Default::default()
                    });
                } else {
                    for spec in style.animations.iter_mut() {
                        spec.timeline = timeline.clone();
                    }
                }
            }
            "animation-range" => {
                let range = parse_animation_range(val);
                style.animation_range = range;
                if style.animations.is_empty() {
                    style.animations.push(AnimationSpec {
                        range,
                        ..Default::default()
                    });
                } else {
                    for spec in style.animations.iter_mut() {
                        spec.range = range;
                    }
                }
            }
            "view-transition-name" => {
                style.view_transition_name = parse_view_transition_name(val);
            }
            "transform" => {
                style.transform = parse_transform_list(val);
            }
            "transform-origin" => {
                if let Some(orig) = parse_transform_origin(val) {
                    style.transform_origin = orig;
                }
            }
            "transform-style" => {
                if let Some(ts) = parse_transform_style(val) {
                    style.transform_style = ts;
                }
            }
            "perspective" => {
                let trimmed = val.trim().to_ascii_lowercase();
                if trimmed == "none" {
                    style.perspective = None;
                } else if let Some(dim) = parse_dimension(&trimmed) {
                    let px = match dim {
                        Dimension::Px(p) => p,
                        Dimension::Rem(rem) => rem * 16.0,
                        Dimension::Em(em) => em * 16.0,
                        _ => 0.0,
                    };
                    style.perspective = if px > 0.0 { Some(px) } else { None };
                }
            }
            "backface-visibility" => {
                if let Some(bv) = parse_backface_visibility(val) {
                    style.backface_visibility = bv;
                }
            }
            "rotate" => {
                if let Some(rad) = parse_angle_rad(val) {
                    style
                        .transform
                        .retain(|op| !matches!(op, TransformOp::Rotate(_)));
                    style.transform.push(TransformOp::Rotate(rad));
                }
            }
            "scale" => {
                let parts: Vec<&str> = val.split_whitespace().collect();
                if let Some(first) = parts.first().and_then(|s| s.parse::<f32>().ok()) {
                    let second = parts
                        .get(1)
                        .and_then(|s| s.parse::<f32>().ok())
                        .unwrap_or(first);
                    style
                        .transform
                        .retain(|op| !matches!(op, TransformOp::Scale(_, _)));
                    style.transform.push(TransformOp::Scale(first, second));
                }
            }
            "translate" => {
                let parts: Vec<&str> = val.split_whitespace().collect();
                if let Some(dx) = parts.first().and_then(|s| parse_dimension(s)) {
                    let dy = parts
                        .get(1)
                        .and_then(|s| parse_dimension(s))
                        .unwrap_or(Dimension::Px(0.0));
                    style
                        .transform
                        .retain(|op| !matches!(op, TransformOp::Translate(_, _)));
                    style.transform.push(TransformOp::Translate(dx, dy));
                }
            }
            _ => {}
        }
    }
}

fn parse_grid_line(value: &str) -> Option<i32> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("auto") {
        return None;
    }
    trimmed.parse::<i32>().ok()
}

fn parse_grid_span(value: &str) -> Option<u32> {
    let mut parts = value.split_whitespace();
    let keyword = parts.next()?;
    if !keyword.eq_ignore_ascii_case("span") {
        return None;
    }
    parts.next()?.parse::<u32>().ok()
}

fn apply_grid_placement_line(value: &str, start: &mut Option<i32>, span: &mut Option<u32>) {
    if let Some(count) = parse_grid_span(value) {
        *span = Some(count);
    } else if value.trim().eq_ignore_ascii_case("auto") {
        *start = None;
    } else if let Some(line) = parse_grid_line(value) {
        *start = Some(line);
    }
}

fn apply_grid_placement_end(value: &str, start: Option<i32>, span: &mut Option<u32>) {
    if let Some(count) = parse_grid_span(value) {
        *span = Some(count);
    } else if let Some(end) = parse_grid_line(value) {
        if let Some(begin) = start {
            if end > begin {
                *span = Some((end - begin) as u32);
            }
        }
    }
}

fn apply_grid_placement_shorthand(value: &str, start: &mut Option<i32>, span: &mut Option<u32>) {
    let parts: Vec<&str> = value
        .split('/')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect();

    match parts.as_slice() {
        [single] => apply_grid_placement_line(single, start, span),
        [first, second] => {
            if let Some(count) = parse_grid_span(first) {
                *span = Some(count);
                if let Some(end) = parse_grid_line(second) {
                    *start = Some(end - count as i32);
                }
                return;
            }

            apply_grid_placement_line(first, start, span);

            if let Some(count) = parse_grid_span(second) {
                *span = Some(count);
            } else if let Some(end) = parse_grid_line(second) {
                if let Some(begin) = *start {
                    if end > begin {
                        *span = Some((end - begin) as u32);
                    }
                }
            }
        }
        _ => {}
    }
}
