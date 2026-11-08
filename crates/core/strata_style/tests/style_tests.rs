//! Comprehensive Integration Tests for `strata_style` CSS Engine, Cascade Solver, and Tailwind JIT.

use strata_style::prelude::*;

#[test]
fn test_specificity_calculation_and_ordering() {
    let spec_type = Specificity::TYPE; // (0, 0, 0, 1)
    let spec_class = Specificity::CLASS; // (0, 0, 1, 0)
    let spec_id = Specificity::ID; // (0, 1, 0, 0)
    let spec_inline = Specificity::INLINE; // (1, 0, 0, 0)

    // W3C Level 4 Hierarchy: Inline > ID > Class > Type
    assert!(spec_inline > spec_id);
    assert!(spec_id > spec_class);
    assert!(spec_class > spec_type);

    // Compound addition
    let compound = spec_id + spec_class + spec_class + spec_type;
    assert_eq!(compound, Specificity::new(0, 1, 2, 1));

    // Even 100 classes cannot beat 1 ID
    let hundred_classes = Specificity::new(0, 0, 100, 100);
    assert!(spec_id > hundred_classes);
}

#[test]
fn test_selector_parsing_and_matching() {
    let selector = Selector::parse("button.btn-primary:hover").expect("Failed to parse selector");
    assert_eq!(selector.parts.len(), 3);
    assert_eq!(selector.specificity, Specificity::new(0, 0, 2, 1));

    let mut state = PseudoState::default();
    assert!(!selector.matches("button", &["btn-primary"], None, &state));

    state.is_hovered = true;
    assert!(selector.matches("button", &["btn-primary"], None, &state));
    assert!(!selector.matches("div", &["btn-primary"], None, &state)); // Wrong tag
    assert!(!selector.matches("button", &["other-class"], None, &state)); // Missing class

    // ID selector
    let id_sel = Selector::parse("#main-nav").expect("Failed to parse ID selector");
    assert_eq!(id_sel.specificity, Specificity::new(0, 1, 0, 0));
    assert!(id_sel.matches("div", &[], Some("main-nav"), &state));
    assert!(!id_sel.matches("div", &[], Some("other-id"), &state));
}

#[test]
fn test_css_stylesheet_parser() {
    let css = r#"
        /* Global Reset */
        * {
            margin: 0px;
            padding: 0px;
        }

        /* Container Card */
        .card, div.container {
            display: flex;
            background-color: #1e293b;
            padding: 16px;
            border-radius: 8px;
        }

        /* Button hover with !important */
        button.btn:hover {
            background-color: #3b82f6 !important;
            color: #ffffff;
        }
    "#;

    let sheet = StyleSheet::parse(css).expect("Failed to parse stylesheet");
    assert_eq!(sheet.rules.len(), 3);

    // Rule 1: *
    assert_eq!(sheet.rules[0].selectors[0].raw, "*");
    assert_eq!(sheet.rules[0].declarations.len(), 2);

    // Rule 2: .card, div.container (comma-separated selectors)
    assert_eq!(sheet.rules[1].selectors.len(), 2);
    assert_eq!(sheet.rules[1].selectors[0].raw, ".card");
    assert_eq!(sheet.rules[1].selectors[1].raw, "div.container");
    assert_eq!(sheet.rules[1].declarations.len(), 4);

    // Rule 3: button.btn:hover with !important
    assert_eq!(sheet.rules[2].declarations[0].property, "background-color");
    assert!(sheet.rules[2].declarations[0].important);
}

#[test]
fn test_cascade_solver_and_inheritance() {
    let css = r#"
        div {
            color: #ff0000;
            font-size: 20px;
            padding: 10px;
        }

        .highlight {
            color: #00ff00;
        }

        #special {
            color: #0000ff;
        }

        .override-all {
            color: #ffffff !important;
        }
    "#;

    let sheet = StyleSheet::parse(css).expect("Failed to parse stylesheet");
    let state = PseudoState::default();

    // 1. Tag match only: red color, 20px font
    let style1 = CascadeSolver::resolve(
        "div",
        &[],
        None,
        None,
        &[&sheet],
        None,
        &state,
    );
    assert_eq!(style1.color, Color::new(255, 0, 0, 255));
    assert_eq!(style1.font_size, 20.0);
    assert_eq!(style1.padding.top, 10.0);

    // 2. Class match beats tag: green color
    let style2 = CascadeSolver::resolve(
        "div",
        &["highlight"],
        None,
        None,
        &[&sheet],
        None,
        &state,
    );
    assert_eq!(style2.color, Color::new(0, 255, 0, 255));

    // 3. ID match beats class: blue color
    let style3 = CascadeSolver::resolve(
        "div",
        &["highlight"],
        Some("special"),
        None,
        &[&sheet],
        None,
        &state,
    );
    assert_eq!(style3.color, Color::new(0, 0, 255, 255));

    // 4. !important beats higher specificity ID: white color
    let style4 = CascadeSolver::resolve(
        "div",
        &["highlight", "override-all"],
        Some("special"),
        None,
        &[&sheet],
        None,
        &state,
    );
    assert_eq!(style4.color, Color::new(255, 255, 255, 255));

    // 5. Inheritance: child node inherits typographic color and font-size from parent
    let child_style = CascadeSolver::resolve(
        "span",
        &[],
        None,
        None,
        &[&sheet],
        Some(&style3), // Parent is style3 (blue color, 20px font)
        &state,
    );
    assert_eq!(child_style.color, Color::new(0, 0, 255, 255));
    assert_eq!(child_style.font_size, 20.0);
    assert_eq!(child_style.padding.top, 0.0); // Non-inheritable property remains default
}

#[test]
fn test_tailwind_layout_and_spacing() {
    let classes = "flex flex-col items-center justify-between p-4 px-6 m-2 gap-4";
    let style = TailwindEngine::resolve_classes(classes);

    assert_eq!(style.display, Display::Flex);
    assert_eq!(style.flex_direction, FlexDirection::Column);
    assert_eq!(style.align_items, AlignItems::Center);
    assert_eq!(style.justify_content, JustifyContent::SpaceBetween);

    // Padding: p-4 (16px), px-6 (24px horizontal)
    assert_eq!(style.padding.top, 16.0);
    assert_eq!(style.padding.bottom, 16.0);
    assert_eq!(style.padding.left, 24.0);
    assert_eq!(style.padding.right, 24.0);

    // Margin: m-2 (8px)
    assert_eq!(style.margin.top, 8.0);
    assert_eq!(style.margin.right, 8.0);

    // Gap: gap-4 (16px)
    assert_eq!(style.gap, 16.0);
    assert_eq!(style.gap_row, 16.0);
    assert_eq!(style.gap_col, 16.0);
}

#[test]
fn test_tailwind_colors_and_opacity() {
    let classes = "bg-slate-900 text-white border-blue-500 bg-black/50";
    let style = TailwindEngine::resolve_classes(classes);

    // Final background is black with 50% opacity
    assert_eq!(style.background_color, Some(Color::new(0, 0, 0, 128)));
    assert_eq!(style.color, Color::WHITE);
    assert_eq!(style.border_color, Some(Color::new(59, 130, 246, 255)));
}

#[test]
fn test_tailwind_arbitrary_values() {
    let classes = "w-[420px] h-[300px] bg-[#0f172a] p-[18px] gap-[2rem] rounded-[14px]";
    let style = TailwindEngine::resolve_classes(classes);

    assert_eq!(style.width, Dimension::Px(420.0));
    assert_eq!(style.height, Dimension::Px(300.0));
    assert_eq!(style.background_color, Some(Color::new(15, 23, 42, 255)));
    assert_eq!(style.padding.top, 18.0);
    assert_eq!(style.padding.left, 18.0);
    assert_eq!(style.gap, 32.0); // 2rem = 32px
    assert_eq!(style.border_radius.top_left, 14.0);
}

#[test]
fn test_tailwind_state_modifiers() {
    let classes = "bg-slate-800 hover:bg-blue-600 dark:bg-black text-white";

    // Unhovered, Light Mode
    let normal_style = TailwindEngine::resolve_classes_with_state(classes, false, false, false);
    assert_eq!(normal_style.background_color, Some(Color::new(30, 41, 59, 255))); // slate-800

    // Hovered, Light Mode
    let hovered_style = TailwindEngine::resolve_classes_with_state(classes, true, false, false);
    assert_eq!(hovered_style.background_color, Some(Color::new(37, 99, 235, 255))); // blue-600

    // Dark Mode
    let dark_style = TailwindEngine::resolve_classes_with_state(classes, false, false, true);
    assert_eq!(dark_style.background_color, Some(Color::BLACK));
}

#[test]
fn test_tailwind_grid_system() {
    let classes = "grid grid-cols-3 gap-2";
    let style = TailwindEngine::resolve_classes(classes);

    assert_eq!(style.display, Display::Grid);
    assert_eq!(style.grid_template_columns.len(), 3);
    assert_eq!(style.grid_template_columns[0], GridTrack::Fr(1.0));
    assert_eq!(style.gap, 8.0);
}

#[test]
fn test_tailwind_arbitrary_utf8_and_bracket_safety() {
    // 1. Multibyte UTF-8 within arbitrary values does not panic
    let utf8_classes = "bg-[#12🦀45] w-[calc(100%-20px)]";
    let style = TailwindEngine::resolve_classes(utf8_classes);
    // Invalid hex with emoji gracefully fails to parse without crashing
    assert_eq!(style.background_color, None);
    // Calc width is parsed and interned
    assert!(matches!(style.width, Dimension::Calc(_)));

    // 2. Colons inside brackets do not get confused with modifiers
    let (mod1, rest1) = strata_style::tailwind::split_modifier("bg-[url(https://example.com/asset.png)]");
    assert_eq!(mod1, None);
    assert_eq!(rest1, "bg-[url(https://example.com/asset.png)]");

    // 3. True modifiers with arbitrary values work seamlessly
    let hover_classes = "hover:bg-[rgb(255,0,0)]";
    let unhovered = TailwindEngine::resolve_classes_with_state(hover_classes, false, false, false);
    assert_eq!(unhovered.background_color, None);
    let hovered = TailwindEngine::resolve_classes_with_state(hover_classes, true, false, false);
    assert_eq!(hovered.background_color, Some(Color::new(255, 0, 0, 255)));

    // 4. Forward slash inside arbitrary brackets does not get confused with opacity
    let slash_classes = "w-[calc(100%/2)]";
    let style_slash = TailwindEngine::resolve_classes(slash_classes);
    assert!(matches!(style_slash.width, Dimension::Calc(_)));

    // 5. Opacity on arbitrary color
    let op_classes = "bg-[#00ff00]/50";
    let style_op = TailwindEngine::resolve_classes(op_classes);
    assert_eq!(style_op.background_color, Some(Color::new(0, 255, 0, 128)));

    // 6. Arbitrary units: vw, vh, rem, em
    let units_classes = "w-[50vw] h-[80vh] min-w-[3rem]";
    let style_units = TailwindEngine::resolve_classes(units_classes);
    assert_eq!(style_units.width, Dimension::Vw(50.0));
    assert_eq!(style_units.height, Dimension::Vh(80.0));
    assert_eq!(style_units.min_width, Dimension::Px(48.0));

    // 7. Bracketed arbitrary values with spaces are tokenized intact
    let spaced_classes = "flex w-[calc(100% - 32px)] text-sm";
    let style_spaced = TailwindEngine::resolve_classes(spaced_classes);
    assert_eq!(style_spaced.display, Display::Flex);
    assert!(matches!(style_spaced.width, Dimension::Calc(_)));
    assert_eq!(style_spaced.font_size, 14.0);
}

