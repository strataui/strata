use strata_style::prelude::*;

#[test]
fn test_css_custom_properties_and_var_resolution() {
    let css = r#"
        :root {
            --primary: #3b82f6;
            --secondary: #10b981;
            --base-font: "Inter, sans-serif";
            --accent-border: 2px;
        }
        .card {
            background-color: var(--primary);
            color: var(--secondary);
            font-family: var(--base-font);
            border-width: var(--accent-border);
            outline-color: var(--undefined-color, #ef4444);
        }
    "#;

    let sheet = StyleSheet::parse(css).expect("Failed to parse stylesheet");
    let state = PseudoState::default();

    let root_style = CascadeSolver::resolve(":root", &[], None, None, &[&sheet], None, &state);
    assert_eq!(root_style.get_custom_property(strata_primitives::atom::intern("--primary")).and_then(strata_primitives::atom::resolve), Some("#3b82f6".to_string()));
    assert_eq!(root_style.get_custom_property(strata_primitives::atom::intern("--secondary")).and_then(strata_primitives::atom::resolve), Some("#10b981".to_string()));

    let card_style = CascadeSolver::resolve(
        "div",
        &["card"],
        None,
        None,
        &[&sheet],
        Some(&root_style),
        &state,
    );

    assert_eq!(card_style.background_color, Some(Color::rgb(59, 130, 246)));
    assert_eq!(card_style.color, Color::rgb(16, 185, 129));
    assert_eq!(card_style.font_family, strata_primitives::atom::intern("Inter, sans-serif"));
    assert_eq!(card_style.border_width.top, 2.0);
    assert_eq!(card_style.border_width.left, 2.0);
    assert_eq!(card_style.outline_color, Some(Color::rgb(239, 68, 68)));
}

#[test]
fn test_w3c_inheritance_rules() {
    let parent_css = r#"
        .parent {
            color: #1e293b;
            font-family: "Fira Code";
            font-size: 14px;
            font-weight: 600;
            font-style: italic;
            text-align: right;
            line-height: 1.75;
            letter-spacing: 0.5px;
            text-decoration: underline;
            text-transform: uppercase;
            text-overflow: ellipsis;
            white-space: nowrap;
            cursor: pointer;
            user-select: none;
            visibility: visible;
            background-color: #ffffff;
            width: 500px;
            margin: 20px;
            padding: 15px;
            border-width: 4px;
        }
    "#;

    let sheet = StyleSheet::parse(parent_css).expect("Failed to parse parent CSS");
    let state = PseudoState::default();

    let parent_style = CascadeSolver::resolve(
        "div",
        &["parent"],
        None,
        None,
        &[&sheet],
        None,
        &state,
    );

    let child_style = CascadeSolver::resolve(
        "span",
        &[],
        None,
        None,
        &[],
        Some(&parent_style),
        &state,
    );

    // Inherited properties
    assert_eq!(child_style.color, Color::rgb(30, 41, 59));
    assert_eq!(child_style.font_family, strata_primitives::atom::intern("Fira Code"));
    assert_eq!(child_style.font_size, 14.0);
    assert_eq!(child_style.font_weight, FontWeight::SemiBold);
    assert_eq!(child_style.font_style, FontStyle::Italic);
    assert_eq!(child_style.text_align, TextAlign::Right);
    assert_eq!(child_style.line_height, 1.75);
    assert_eq!(child_style.letter_spacing, 0.5);
    assert_eq!(child_style.text_decoration, TextDecoration::Underline);
    assert_eq!(child_style.text_transform, TextTransform::Uppercase);
    assert_eq!(child_style.text_overflow, TextOverflow::Ellipsis);
    assert_eq!(child_style.white_space, WhiteSpace::NoWrap);
    assert_eq!(child_style.cursor, Cursor::Pointer);
    assert_eq!(child_style.user_select, UserSelect::None);
    assert_eq!(child_style.visibility, Visibility::Visible);

    // Non-inherited properties retain default values
    assert_eq!(child_style.background_color, None);
    assert_eq!(child_style.width, Dimension::Auto);
    assert_eq!(child_style.margin, EdgeInsets::ZERO);
    assert_eq!(child_style.padding, EdgeInsets::ZERO);
    assert_eq!(child_style.border_width, EdgeInsets::ZERO);
}

#[test]
fn test_shorthand_properties_border_and_outline() {
    let css = r#"
        .box-bordered {
            border: 2px dashed #059669;
            outline: 3px solid #dc2626;
            outline-offset: 4px;
            aspect-ratio: 16 / 9;
        }
        .box-reset {
            border: none;
            outline: none;
        }
    "#;

    let sheet = StyleSheet::parse(css).expect("Failed to parse box CSS");
    let state = PseudoState::default();

    let style = CascadeSolver::resolve(
        "div",
        &["box-bordered"],
        None,
        None,
        &[&sheet],
        None,
        &state,
    );

    assert_eq!(style.border_width.top, 2.0);
    assert_eq!(style.border_width.right, 2.0);
    assert_eq!(style.border_width.bottom, 2.0);
    assert_eq!(style.border_width.left, 2.0);
    assert_eq!(style.border_style, BorderStyle::Dashed);
    assert_eq!(style.border_color, Some(Color::rgb(5, 150, 105)));

    assert_eq!(style.outline_width, 3.0);
    assert_eq!(style.outline_style, BorderStyle::Solid);
    assert_eq!(style.outline_color, Some(Color::rgb(220, 38, 38)));
    assert_eq!(style.outline_offset, 4.0);
    assert_eq!(style.aspect_ratio, Some(16.0 / 9.0));

    let reset_style = CascadeSolver::resolve(
        "div",
        &["box-reset"],
        None,
        None,
        &[&sheet],
        None,
        &state,
    );

    assert_eq!(reset_style.border_width, EdgeInsets::ZERO);
    assert_eq!(reset_style.border_style, BorderStyle::None);
    assert_eq!(reset_style.outline_width, 0.0);
    assert_eq!(reset_style.outline_style, BorderStyle::None);
}

#[test]
fn test_typography_and_interaction_spec() {
    let css = r#"
        .hero-title {
            text-transform: capitalize;
            text-decoration: line-through;
            vertical-align: super;
            white-space: pre-wrap;
            cursor: grab;
            user-select: all;
            pointer-events: none;
            visibility: collapse;
        }
    "#;

    let sheet = StyleSheet::parse(css).expect("Failed to parse hero CSS");
    let state = PseudoState::default();

    let style = CascadeSolver::resolve(
        "h1",
        &["hero-title"],
        None,
        None,
        &[&sheet],
        None,
        &state,
    );

    assert_eq!(style.text_transform, TextTransform::Capitalize);
    assert_eq!(style.text_decoration, TextDecoration::LineThrough);
    assert_eq!(style.vertical_align, VerticalAlign::Super);
    assert_eq!(style.white_space, WhiteSpace::PreWrap);
    assert_eq!(style.cursor, Cursor::Grab);
    assert_eq!(style.user_select, UserSelect::All);
    assert_eq!(style.pointer_events, PointerEvents::None);
    assert_eq!(style.visibility, Visibility::Collapse);
}

#[test]
fn test_cascade_hierarchy_important_and_specificity() {
    let css = r#"
        div {
            color: #111111;
            font-size: 12px;
        }
        .text-blue {
            color: #0000ff;
        }
        #hero-text {
            color: #00ff00;
            font-size: 24px !important;
        }
    "#;

    let sheet = StyleSheet::parse(css).expect("Failed to parse hierarchy CSS");
    let state = PseudoState::default();

    // ID selector beats class selector
    let style = CascadeSolver::resolve(
        "div",
        &["text-blue"],
        Some("hero-text"),
        Some("color: #ff0000; font-size: 18px;"),
        &[&sheet],
        None,
        &state,
    );

    // Inline style overrides ID for color (inline specificity > ID specificity)
    assert_eq!(style.color, Color::rgb(255, 0, 0));
    // But #hero-text font-size has !important, so it overrides inline style font-size: 18px
    assert_eq!(style.font_size, 24.0);
}

#[test]
fn test_nested_var_resolution_and_multi_tier_custom_property_tree() {
    let root_css = r#"
        :root {
            --brand-primary: #6366f1;
            --brand-text: #ffffff;
            --card-padding: 24px;
        }
        .dark-theme {
            --brand-primary: #4338ca;
            --brand-text: #e0e7ff;
        }
    "#;

    let sheet = StyleSheet::parse(root_css).expect("Failed to parse theme CSS");
    let state = PseudoState::default();

    // Level 1: Root
    let root = CascadeSolver::resolve(":root", &[], None, None, &[&sheet], None, &state);

    // Level 2: Dark Theme Container (overrides --brand-primary)
    let container = CascadeSolver::resolve(
        "div",
        &["dark-theme"],
        None,
        None,
        &[&sheet],
        Some(&root),
        &state,
    );
    assert_eq!(container.get_custom_property(strata_primitives::atom::intern("--brand-primary")).and_then(strata_primitives::atom::resolve), Some("#4338ca".to_string()));

    // Level 3: Card Button inheriting dark theme variables
    let button_css = r#"
        .btn {
            background-color: var(--brand-primary);
            color: var(--brand-text);
            padding: var(--card-padding);
        }
    "#;
    let button_sheet = StyleSheet::parse(button_css).expect("Failed to parse button CSS");

    let button = CascadeSolver::resolve(
        "button",
        &["btn"],
        None,
        None,
        &[&button_sheet],
        Some(&container),
        &state,
    );

    assert_eq!(button.background_color, Some(Color::rgb(67, 56, 202)));
    assert_eq!(button.color, Color::rgb(224, 231, 255));
    assert_eq!(button.padding.top, 24.0);
}

#[test]
fn test_float_and_clear_resolution() {
    let css = r#"
        .infobox {
            float: right;
            clear: none;
        }
        .figure-left {
            float: left;
        }
        .footer-clear {
            clear: both;
        }
    "#;
    let sheet = StyleSheet::parse(css).expect("Failed to parse float/clear stylesheet");
    let state = PseudoState::default();

    let infobox = CascadeSolver::resolve("div", &["infobox"], None, None, &[&sheet], None, &state);
    assert_eq!(infobox.float, Float::Right);
    assert_eq!(infobox.clear, Clear::None);

    let figure = CascadeSolver::resolve("div", &["figure-left"], None, None, &[&sheet], None, &state);
    assert_eq!(figure.float, Float::Left);

    let footer = CascadeSolver::resolve("div", &["footer-clear"], None, None, &[&sheet], None, &state);
    assert_eq!(footer.clear, Clear::Both);

    // Test Tailwind utilities
    let mut tw_style = ComputedStyle::default();
    TailwindEngine::apply_class(&mut tw_style, "float-left");
    assert_eq!(tw_style.float, Float::Left);
    TailwindEngine::apply_class(&mut tw_style, "clear-both");
    assert_eq!(tw_style.clear, Clear::Both);
}
