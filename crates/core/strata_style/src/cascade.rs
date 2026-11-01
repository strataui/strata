//! W3C-compliant CSS Cascade Solver and Property Application Engine.

use crate::parser::{parse_color, parse_declarations, parse_dimension, parse_edge_insets_px, Declaration, StyleSheet};
use crate::selector::PseudoState;
use crate::specificity::Specificity;
use crate::types::*;

struct CascadeCandidate<'a> {
    declaration: &'a Declaration,
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
        // Step 1: Base style from inheritance or initial default
        let mut computed = match parent_style {
            Some(parent) => ComputedStyle::inherit_from(parent),
            None => ComputedStyle::default(),
        };

        // Step 2: Collect all matching declarations from stylesheets
        let mut candidates = Vec::new();
        let mut global_order = 0;

        for sheet in stylesheets {
            for rule in &sheet.rules {
                // Find highest specificity among matching selectors for this rule
                let mut best_match: Option<Specificity> = None;
                for selector in &rule.selectors {
                    if selector.matches(tag, classes, id, state) {
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
                        candidates.push(CascadeCandidate {
                            declaration: decl,
                            specificity: spec,
                            source_order: global_order,
                        });
                        global_order += 1;
                    }
                }
            }
        }

        // Step 3: Parse and append inline style declarations
        let inline_declarations = inline_style.map(parse_declarations).unwrap_or_default();
        for decl in &inline_declarations {
            candidates.push(CascadeCandidate {
                declaration: decl,
                specificity: Specificity::INLINE,
                source_order: global_order,
            });
            global_order += 1;
        }

        // Step 4: Sort candidates by cascade priority:
        // (important, specificity, source_order)
        candidates.sort_by(|a, b| {
            a.declaration
                .important
                .cmp(&b.declaration.important)
                .then_with(|| a.specificity.cmp(&b.specificity))
                .then_with(|| a.source_order.cmp(&b.source_order))
        });

        // Step 5: Apply declarations in sorted priority order
        for candidate in candidates {
            Self::apply_declaration(&mut computed, candidate.declaration);
        }

        computed
    }

    /// Applies an individual CSS declaration to a mutable `ComputedStyle`.
    pub fn apply_declaration(style: &mut ComputedStyle, decl: &Declaration) {
        let prop = decl.property.as_str();
        let val = decl.value.as_str();

        match prop {
            "display" => match val {
                "block" => style.display = Display::Block,
                "inline" => style.display = Display::Inline,
                "inline-block" => style.display = Display::InlineBlock,
                "flex" => style.display = Display::Flex,
                "inline-flex" => style.display = Display::InlineFlex,
                "grid" => style.display = Display::Grid,
                "inline-grid" => style.display = Display::InlineGrid,
                "none" => style.display = Display::None,
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
                if let Some(insets) = parse_edge_insets_px(val) {
                    style.margin = insets;
                }
            }
            "margin-top" => {
                if let Some(Dimension::Px(px)) = parse_dimension(val) {
                    style.margin.top = px;
                }
            }
            "margin-right" => {
                if let Some(Dimension::Px(px)) = parse_dimension(val) {
                    style.margin.right = px;
                }
            }
            "margin-bottom" => {
                if let Some(Dimension::Px(px)) = parse_dimension(val) {
                    style.margin.bottom = px;
                }
            }
            "margin-left" => {
                if let Some(Dimension::Px(px)) = parse_dimension(val) {
                    style.margin.left = px;
                }
            }
            "padding" => {
                if let Some(insets) = parse_edge_insets_px(val) {
                    style.padding = insets;
                }
            }
            "padding-top" => {
                if let Some(Dimension::Px(px)) = parse_dimension(val) {
                    style.padding.top = px;
                }
            }
            "padding-right" => {
                if let Some(Dimension::Px(px)) = parse_dimension(val) {
                    style.padding.right = px;
                }
            }
            "padding-bottom" => {
                if let Some(Dimension::Px(px)) = parse_dimension(val) {
                    style.padding.bottom = px;
                }
            }
            "padding-left" => {
                if let Some(Dimension::Px(px)) = parse_dimension(val) {
                    style.padding.left = px;
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
            "gap" => {
                if let Some(Dimension::Px(px)) = parse_dimension(val) {
                    style.gap = px;
                    style.gap_row = px;
                    style.gap_col = px;
                }
            }
            "row-gap" => {
                if let Some(Dimension::Px(px)) = parse_dimension(val) {
                    style.gap_row = px;
                }
            }
            "column-gap" => {
                if let Some(Dimension::Px(px)) = parse_dimension(val) {
                    style.gap_col = px;
                }
            }
            "background-color" | "background" => {
                if let Some(c) = parse_color(val) {
                    style.background_color = Some(c);
                }
            }
            "color" => {
                if let Some(c) = parse_color(val) {
                    style.color = c;
                }
            }
            "border-color" => {
                if let Some(c) = parse_color(val) {
                    style.border_color = Some(c);
                }
            }
            "border-width" => {
                if let Some(insets) = parse_edge_insets_px(val) {
                    style.border_width = insets;
                }
            }
            "border-radius" => {
                if let Some(Dimension::Px(px)) = parse_dimension(val) {
                    style.border_radius = BorderRadius::all(px);
                }
            }
            "opacity" => {
                if let Ok(op) = val.parse::<f32>() {
                    style.opacity = op.clamp(0.0, 1.0);
                }
            }
            "font-size" => {
                if let Some(Dimension::Px(px)) = parse_dimension(val) {
                    style.font_size = px;
                }
            }
            "font-weight" => match val {
                "bold" => style.font_weight = FontWeight::Bold,
                "normal" => style.font_weight = FontWeight::Normal,
                other => {
                    if let Ok(w) = other.parse::<u16>() {
                        style.font_weight = FontWeight::from_u16(w);
                    }
                }
            },
            "text-align" => match val {
                "left" => style.text_align = TextAlign::Left,
                "center" => style.text_align = TextAlign::Center,
                "right" => style.text_align = TextAlign::Right,
                "justify" => style.text_align = TextAlign::Justify,
                _ => {}
            },
            "line-height" => {
                if let Ok(lh) = val.parse::<f32>() {
                    style.line_height = lh;
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
            _ => {}
        }
    }
}
