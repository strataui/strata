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
        for cls in classes.iter() {
            for single_class in cls.split_whitespace() {
                strata_style::tailwind::TailwindEngine::apply_class_with_state(
                    style,
                    single_class,
                    is_hovered,
                    is_focused,
                    is_dark,
                );
            }
        }
    }

    /// Returns true when the last hover transition was resolved by a paint-only restyle.
    pub fn hover_is_paint_only(&self) -> bool {
        self.hover_paint_only
    }

    /// Returns true when any `hover:` utility on the path mutates box geometry.
    pub(crate) fn path_hover_affects_layout(blueprint: &NodeBlueprint, path: &[usize]) -> bool {
        let mut current = blueprint;
        if Self::node_hover_classes_affect_layout(current) {
            return true;
        }
        for &idx in path {
            let mut flat = Vec::new();
            match current {
                NodeBlueprint::Element { children, .. } | NodeBlueprint::Fragment(children) => {
                    for child in children {
                        Self::flatten_blueprints(child, &mut flat);
                    }
                }
                _ => return false,
            }
            if idx >= flat.len() {
                return false;
            }
            current = flat[idx];
            if Self::node_hover_classes_affect_layout(current) {
                return true;
            }
        }
        false
    }

    pub(crate) fn node_hover_classes_affect_layout(node: &NodeBlueprint) -> bool {
        match node {
            NodeBlueprint::Element { classes, .. } => classes.iter().any(|class| {
                class.split_whitespace().any(|token| {
                    token
                        .strip_prefix("hover:")
                        .is_some_and(|rest| !Self::hover_token_is_paint_only(rest))
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
            "filter-",
            "backdrop-",
            "blur-",
            "brightness-",
            "contrast-",
            "saturate-",
            "hue-rotate-",
            "z-",
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
            return value.contains('#')
                || value.contains("rgb")
                || value.contains("hsl")
                || value.contains("oklch")
                || value.contains("color(");
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
            return value.contains('#')
                || value.contains("rgb")
                || value.contains("hsl")
                || value.contains("oklch");
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
