//! Test suite for Milestone 48: Next-Gen CSS Standards & Modern Transitions Suite
//!
//! Validates:
//! - CSS `@starting-style` parsing (top-level and nested) and cascade resolution.
//! - Scroll-driven animations (`animation-timeline: scroll() | view()` and `animation-range`).
//! - Houdini `@property` custom property registration and continuous interpolation.
//! - CSS View Transitions Module (`view-transition-name`).
//! - Tailwind utilities for Next-Gen CSS.

use strata_style::{
    parse_animation_range, parse_animation_timeline, parse_property_rule_body,
    parse_view_transition_name, AnimationRange, AnimationRangeName, AnimationTimeline,
    CascadeSolver, Color, PropertyRegistry, PropertySyntax, RegisteredProperty,
    ScrollAxis, ScrollTimelineSource, StyleSheet, TailwindEngine,
};

#[test]
fn test_starting_style_parsing_toplevel_and_nested() {
    let css = r#"
        /* Top-level @starting-style */
        @starting-style {
            .modal {
                opacity: 0;
                transform: scale(0.9);
            }
        }

        /* Rule with nested @starting-style */
        .toast {
            opacity: 1;
            transform: translateY(0px);
            @starting-style {
                opacity: 0;
                transform: translateY(20px);
            }
        }
    "#;

    let sheet = StyleSheet::parse(css).expect("Valid CSS stylesheet with @starting-style should parse");

    // Verify top-level @starting-style
    assert_eq!(sheet.starting_styles.len(), 2, "Expected 2 starting style rules (1 top-level, 1 nested)");
    let top_level = &sheet.starting_styles[0];
    assert_eq!(top_level.declarations.len(), 2);
    assert!(top_level.declarations.iter().any(|d| d.property == "opacity" && d.value == "0"));
    assert!(top_level.declarations.iter().any(|d| d.property == "transform" && d.value == "scale(0.9)"));

    // Verify nested @starting-style
    let nested = &sheet.starting_styles[1];
    assert_eq!(nested.declarations.len(), 2);
    assert!(nested.declarations.iter().any(|d| d.property == "opacity" && d.value == "0"));
    assert!(nested.declarations.iter().any(|d| d.property == "transform" && d.value == "translateY(20px)"));

    // Verify the outer .toast rule has its standard declarations intact
    assert_eq!(sheet.rules.len(), 1);
    let toast_rule = &sheet.rules[0];
    assert!(toast_rule.declarations.iter().any(|d| d.property == "opacity" && d.value == "1"));
}

#[test]
fn test_starting_style_cascade_resolution() {
    let css = r#"
        .dialog {
            opacity: 1;
            background-color: #ffffff;
            transition: opacity 300ms ease;
        }

        @starting-style {
            .dialog {
                opacity: 0;
            }
        }
    "#;

    let sheet = StyleSheet::parse(css).unwrap();
    let state = strata_style::PseudoState::default();
    let computed = CascadeSolver::resolve(
        "div",
        &["dialog"],
        None,
        None,
        &[&sheet],
        None,
        &state,
    );
    assert_eq!(computed.opacity, 1.0, "Computed target opacity must be 1.0");

    let starting = computed.starting_style.expect("Starting style must be populated from @starting-style");
    assert_eq!(starting.opacity, 0.0, "Starting style opacity must be 0.0 per @starting-style rule");
    assert_eq!(starting.background_color, Some(Color::rgb(255, 255, 255)), "Inherited/cascaded base properties preserved");
}

#[test]
fn test_scroll_driven_animations_parsing() {
    // 1. scroll() default
    let tl1 = parse_animation_timeline("scroll()");
    assert_eq!(tl1, AnimationTimeline::Scroll(ScrollTimelineSource::Nearest, ScrollAxis::Block));

    // 2. scroll(root inline)
    let tl2 = parse_animation_timeline("scroll(root inline)");
    assert_eq!(tl2, AnimationTimeline::Scroll(ScrollTimelineSource::Root, ScrollAxis::Inline));

    // 3. scroll(self horizontal)
    let tl3 = parse_animation_timeline("scroll(self horizontal)");
    assert_eq!(tl3, AnimationTimeline::Scroll(ScrollTimelineSource::SelfNode, ScrollAxis::Horizontal));

    // 4. view() default and with axis
    let tl4 = parse_animation_timeline("view()");
    assert_eq!(tl4, AnimationTimeline::View(ScrollAxis::Block, 0.0, 0.0));

    let tl5 = parse_animation_timeline("view(inline 20px)");
    assert_eq!(tl5, AnimationTimeline::View(ScrollAxis::Inline, 20.0, 20.0));

    // 5. Named timeline
    let tl6 = parse_animation_timeline("--progress-timeline");
    assert_eq!(tl6, AnimationTimeline::Named("--progress-timeline".to_string()));

    // 6. animation-range parsing
    let r1 = parse_animation_range("cover");
    assert_eq!(r1, AnimationRange { name: AnimationRangeName::Cover, start_offset: 0.0, end_offset: 1.0 });

    let r2 = parse_animation_range("entry 20% 80%");
    assert_eq!(r2, AnimationRange { name: AnimationRangeName::Entry, start_offset: 0.20, end_offset: 0.80 });
}

#[test]
fn test_houdini_property_registration_and_continuous_interpolation() {
    let body = r#"
        syntax: '<color>';
        inherits: false;
        initial-value: #ff0000;
    "#;
    let prop = parse_property_rule_body("--brand-accent", body);
    assert_eq!(prop.name, "--brand-accent");
    assert_eq!(prop.syntax, PropertySyntax::Color);
    assert!(!prop.inherits);
    assert_eq!(prop.initial_value, Some("#ff0000".to_string()));

    let mut registry = PropertyRegistry::new();
    registry.register(prop);
    registry.register(RegisteredProperty {
        name: "--glow-spread".to_string(),
        syntax: PropertySyntax::Length,
        inherits: true,
        initial_value: Some("0px".to_string()),
    });
    registry.register(RegisteredProperty {
        name: "--progress".to_string(),
        syntax: PropertySyntax::Percentage,
        inherits: false,
        initial_value: Some("0%".to_string()),
    });

    // Interpolate Color
    let color_interp = registry.interpolate("--brand-accent", "#000000", "#ffffff", 0.5).unwrap();
    assert!(color_interp.contains("128") || color_interp.contains("127"), "Midpoint of black and white should have ~128 RGB");

    // Interpolate Length
    let length_interp = registry.interpolate("--glow-spread", "10px", "50px", 0.25).unwrap();
    assert_eq!(length_interp, "20px");

    // Interpolate Percentage
    let pct_interp = registry.interpolate("--progress", "10%", "90%", 0.5).unwrap();
    assert_eq!(pct_interp, "50.00%");
}

#[test]
fn test_view_transition_property_parsing() {
    assert_eq!(parse_view_transition_name("none"), None);
    assert_eq!(parse_view_transition_name("card-hero"), Some("card-hero".to_string()));
    assert_eq!(parse_view_transition_name("auto"), Some("auto".to_string()));
}

#[test]
fn test_tailwind_next_gen_utilities() {
    // 1. View transition names
    let style1 = TailwindEngine::resolve_classes("view-transition-[hero-card]");
    assert_eq!(style1.view_transition_name, Some("hero-card".to_string()));

    let style2 = TailwindEngine::resolve_classes("view-transition-none");
    assert_eq!(style2.view_transition_name, None);

    // 2. Scroll-driven animation timelines
    let style3 = TailwindEngine::resolve_classes("timeline-scroll");
    assert_eq!(style3.animation_timeline, AnimationTimeline::Scroll(ScrollTimelineSource::Nearest, ScrollAxis::Block));

    let style4 = TailwindEngine::resolve_classes("timeline-scroll-root");
    assert_eq!(style4.animation_timeline, AnimationTimeline::Scroll(ScrollTimelineSource::Root, ScrollAxis::Block));

    let style5 = TailwindEngine::resolve_classes("timeline-view");
    assert_eq!(style5.animation_timeline, AnimationTimeline::View(ScrollAxis::Block, 0.0, 0.0));

    // 3. Animation ranges
    let style6 = TailwindEngine::resolve_classes("range-cover");
    assert_eq!(style6.animation_range.name, AnimationRangeName::Cover);

    let style7 = TailwindEngine::resolve_classes("range-entry");
    assert_eq!(style7.animation_range.name, AnimationRangeName::Entry);
}
