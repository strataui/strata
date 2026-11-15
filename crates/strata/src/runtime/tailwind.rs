//! Tailwind CSS class resolution and style mutation analysis.

use super::*;

impl AppRuntime {
    /// Full Tailwind CSS class parser supporting hover, focus, and dark mode variants.
    pub fn apply_tailwind_classes(
        style: &mut ComputedStyle,
        classes: &strata_dom::ClassList,
        is_hovered: bool,
        is_focused: bool,
        is_dark: bool,
    ) {
        let state = strata_style::selector::PseudoState {
            is_hovered,
            is_focused,
            is_active: false,
            is_disabled: false,
            is_dark,
        };
        Self::apply_tailwind_classes_with_pseudo_state(style, classes, &state);
    }

    /// Full Tailwind CSS class parser supporting hover, focus, active, disabled, and dark mode variants.
    pub fn apply_tailwind_classes_with_pseudo_state(
        style: &mut ComputedStyle,
        classes: &strata_dom::ClassList,
        state: &strata_style::selector::PseudoState,
    ) {
        let active_count =
            u8::from(state.is_focused) + u8::from(state.is_hovered) + u8::from(state.is_active);
        if active_count <= 1 {
            for cls in classes.iter() {
                for single_class in strata_style::tailwind::tokenize_classes(cls) {
                    strata_style::tailwind::TailwindEngine::apply_class_with_pseudo_state(
                        style,
                        single_class,
                        state,
                    );
                }
            }
        } else {
            Self::apply_base_tailwind_classes(style, classes, state);
            Self::apply_interactive_tailwind_classes(style, classes, state);
        }
    }

    /// Applies only non-interactive (base + `dark:` + `disabled:`) Tailwind classes.
    pub fn apply_base_tailwind_classes(
        style: &mut ComputedStyle,
        classes: &strata_dom::ClassList,
        state: &strata_style::selector::PseudoState,
    ) {
        for cls in classes.iter() {
            for single_class in strata_style::tailwind::tokenize_classes(cls) {
                strata_style::tailwind::TailwindEngine::apply_base_class_with_pseudo_state(
                    style,
                    single_class,
                    state,
                );
            }
        }
    }

    /// Applies only active interactive (`hover:`, `focus:`, `active:`) Tailwind classes.
    pub fn apply_interactive_tailwind_classes(
        style: &mut ComputedStyle,
        classes: &strata_dom::ClassList,
        state: &strata_style::selector::PseudoState,
    ) {
        let active_count =
            u8::from(state.is_focused) + u8::from(state.is_hovered) + u8::from(state.is_active);
        if active_count <= 1 {
            for cls in classes.iter() {
                for single_class in strata_style::tailwind::tokenize_classes(cls) {
                    strata_style::tailwind::TailwindEngine::apply_interactive_class_with_pseudo_state(
                        style,
                        single_class,
                        state,
                    );
                }
            }
            return;
        }
        // CSS/Tailwind LVFHA cascade order: focus -> hover -> active.
        let mut apply_phase = |phase: u8| {
            for cls in classes.iter() {
                for single_class in strata_style::tailwind::tokenize_classes(cls) {
                    let is_active = single_class.contains("active:");
                    let is_hover = single_class.contains("hover:");
                    let is_focus = single_class.contains("focus");
                    let matches_phase = match phase {
                        0 => is_focus && !is_hover && !is_active,
                        1 => is_hover && !is_active,
                        _ => is_active,
                    };
                    if matches_phase {
                        strata_style::tailwind::TailwindEngine::apply_interactive_class_with_pseudo_state(
                            style,
                            single_class,
                            state,
                        );
                    }
                }
            }
        };
        if state.is_focused {
            apply_phase(0);
        }
        if state.is_hovered {
            apply_phase(1);
        }
        if state.is_active {
            apply_phase(2);
        }
    }

    /// Returns true when the last hover transition was resolved by a paint-only restyle.
    pub fn hover_is_paint_only(&self) -> bool {
        self.hover_paint_only
    }

    /// Returns true when any `hover:` utility or `<style>` `:hover` rule on the path mutates box geometry.
    pub(crate) fn path_hover_affects_layout(blueprint: &NodeBlueprint, path: &[usize]) -> bool {
        let mut path_nodes: Vec<&NodeBlueprint> = Vec::with_capacity(path.len() + 1);
        let mut current = blueprint;
        if Self::node_hover_classes_affect_layout(current) {
            return true;
        }
        path_nodes.push(current);
        for &idx in path {
            let mut flat = Vec::new();
            match current {
                NodeBlueprint::Element { children, .. } | NodeBlueprint::Fragment(children) => {
                    for child in children {
                        Self::flatten_blueprints(child, &mut flat);
                    }
                }
                _ => break,
            }
            if idx >= flat.len() {
                break;
            }
            current = flat[idx];
            if Self::node_hover_classes_affect_layout(current) {
                return true;
            }
            path_nodes.push(current);
        }

        let stylesheets = Self::collect_stylesheets_from_blueprint(blueprint);
        if stylesheets.is_empty() {
            return false;
        }

        let compound_has_hover = |comp: &strata_style::selector::CompoundSelector| {
            comp.parts.iter().any(|p| match p {
                strata_style::selector::SelectorPart::PseudoClass(name) => {
                    let n = name.trim().to_ascii_lowercase();
                    n == "hover" || n.contains(":hover")
                }
                _ => false,
            })
        };
        let probe_state = strata_style::selector::PseudoState {
            is_hovered: true,
            is_focused: false,
            is_active: false,
            is_disabled: false,
            is_dark: false,
        };
        let probe_state_dark = strata_style::selector::PseudoState {
            is_hovered: true,
            is_focused: false,
            is_active: false,
            is_disabled: false,
            is_dark: true,
        };

        for bp in &path_nodes {
            if let NodeBlueprint::Element {
                tag,
                classes,
                attributes,
                ..
            } = bp
            {
                if Self::is_style_tag(tag) {
                    continue;
                }
                let tag_str = Self::tag_as_str(tag);
                let (class_vec, id_val, attr_vec) =
                    Self::extract_element_selector_meta(classes, attributes);
                for sheet in &stylesheets {
                    for rule in &sheet.rules {
                        let any_hover_sel_matches = rule.selectors.iter().any(|sel| {
                            if !sel.has_any_hover_pseudo() {
                                return false;
                            }
                            let target_matches = compound_has_hover(&sel.target)
                                && (sel.target.matches_with_context(
                                    tag_str,
                                    &class_vec,
                                    id_val.as_deref(),
                                    &attr_vec,
                                    &probe_state,
                                    &[],
                                    Some(true),
                                    Some(true),
                                    &[],
                                ) || sel.target.matches_with_context(
                                    tag_str,
                                    &class_vec,
                                    id_val.as_deref(),
                                    &attr_vec,
                                    &probe_state_dark,
                                    &[],
                                    Some(true),
                                    Some(true),
                                    &[],
                                ));
                            let step_matches = sel.steps.iter().any(|(_, comp)| {
                                compound_has_hover(comp)
                                    && (comp.matches_with_context(
                                        tag_str,
                                        &class_vec,
                                        id_val.as_deref(),
                                        &attr_vec,
                                        &probe_state,
                                        &[],
                                        Some(true),
                                        Some(true),
                                        &[],
                                    ) || comp.matches_with_context(
                                        tag_str,
                                        &class_vec,
                                        id_val.as_deref(),
                                        &attr_vec,
                                        &probe_state_dark,
                                        &[],
                                        Some(true),
                                        Some(true),
                                        &[],
                                    ))
                            });
                            target_matches || step_matches
                        });
                        if any_hover_sel_matches
                            && rule
                                .declarations
                                .iter()
                                .any(|d| !Self::css_declaration_is_paint_only(&d.property))
                        {
                            return true;
                        }
                    }
                }
            }
        }

        false
    }

    pub(crate) fn css_declaration_is_paint_only(prop: &str) -> bool {
        matches!(
            prop.trim().to_ascii_lowercase().as_str(),
            "background"
                | "background-color"
                | "background-image"
                | "color"
                | "border-color"
                | "border-top-color"
                | "border-right-color"
                | "border-bottom-color"
                | "border-left-color"
                | "border-radius"
                | "border-top-left-radius"
                | "border-top-right-radius"
                | "border-bottom-right-radius"
                | "border-bottom-left-radius"
                | "opacity"
                | "box-shadow"
                | "outline"
                | "outline-color"
                | "outline-offset"
                | "filter"
                | "backdrop-filter"
                | "-webkit-backdrop-filter"
                | "transform"
                | "transform-origin"
                | "cursor"
                | "transition"
                | "transition-property"
                | "transition-duration"
                | "transition-timing-function"
                | "transition-delay"
                | "z-index"
                | "text-decoration"
                | "visibility"
                | "pointer-events"
                | "user-select"
                | "will-change"
                | "mix-blend-mode"
                | "isolation"
                | "clip-path"
        )
    }

    pub(crate) fn node_hover_classes_affect_layout(node: &NodeBlueprint) -> bool {
        match node {
            NodeBlueprint::Element { classes, .. } => classes.iter().any(|class| {
                strata_style::tailwind::tokenize_classes(class)
                    .into_iter()
                    .any(|token| {
                        let mut cur = token;
                        let mut has_hover = false;
                        while let (Some(m), rest) = strata_style::tailwind::split_modifier(cur) {
                            if m == "hover" {
                                has_hover = true;
                            }
                            cur = rest;
                        }
                        has_hover && !Self::hover_token_is_paint_only(cur)
                    })
            }),
            _ => false,
        }
    }

    /// Classifies a hover utility suffix as paint-only.
    ///
    /// The allowlist is deliberately conservative: any token not recognized as
    /// a paint, filter, transform, or interaction property forces a full
    /// relayout so unknown geometry changes always take the safe path.
    pub(crate) fn hover_token_is_paint_only(rest: &str) -> bool {
        if rest.starts_with("bg-") {
            return true;
        }
        if let Some(value) = rest.strip_prefix("text-") {
            return Self::text_utility_is_color(value);
        }
        if let Some(value) = rest.strip_prefix("border-") {
            return Self::border_utility_is_color(value);
        }
        if rest.starts_with("decoration-") {
            return true;
        }
        const PREFIXES: &[&str] = &[
            "opacity-",
            "shadow-",
            "ring-",
            "outline-",
            "fill-",
            "stroke-",
            "cursor-",
            "transition-",
            "duration-",
            "ease-",
            "delay-",
            "animate-",
            "scale-",
            "rotate-",
            "translate-",
            "-translate-",
            "-rotate-",
            "filter-",
            "backdrop-",
            "blur-",
            "brightness-",
            "contrast-",
            "saturate-",
            "hue-rotate-",
            "z-",
            "-z-",
            "pointer-events-",
            "select-",
            "will-change-",
        ];
        if PREFIXES.iter().any(|prefix| rest.starts_with(prefix)) {
            return true;
        }
        matches!(
            rest,
            "underline"
                | "no-underline"
                | "overline"
                | "line-through"
                | "transform"
                | "transform-gpu"
                | "grayscale"
                | "invert"
                | "sepia"
                | "visible"
                | "invisible"
                | "drop-shadow"
                | "shadow"
        )
    }

    pub(crate) fn text_utility_is_color(value: &str) -> bool {
        const NON_COLOR: &[&str] = &[
            "xs", "sm", "base", "lg", "xl", "2xl", "3xl", "4xl", "5xl", "6xl", "7xl", "8xl", "9xl",
            "left", "center", "right", "justify", "start", "end", "wrap", "nowrap", "balance",
            "pretty", "ellipsis", "clip",
        ];
        if NON_COLOR.contains(&value) {
            return false;
        }
        if value.starts_with('[') {
            if value.starts_with("[length:") {
                return false;
            }
            return value.contains('#')
                || value.contains("rgb")
                || value.contains("hsl")
                || value.contains("oklch")
                || value.contains("color(")
                || value.contains("color:")
                || value.contains("var(");
        }
        true
    }

    pub(crate) fn border_utility_is_color(value: &str) -> bool {
        let mut value = value;
        for side in ["t-", "r-", "b-", "l-", "x-", "y-", "s-", "e-"] {
            if let Some(stripped) = value.strip_prefix(side) {
                value = stripped;
                break;
            }
        }
        if value.starts_with('[') {
            if value.starts_with("[length:") {
                return false;
            }
            return value.contains('#')
                || value.contains("rgb")
                || value.contains("hsl")
                || value.contains("oklch")
                || value.contains("color(")
                || value.contains("color:")
                || value.contains("var(");
        }
        if matches!(value, "transparent" | "current" | "black" | "white") {
            return true;
        }
        value
            .rsplit_once('-')
            .map(|(_, shade)| !shade.is_empty() && shade.chars().all(|c| c.is_ascii_digit()))
            .unwrap_or(false)
    }
}
