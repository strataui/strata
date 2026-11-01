use strata_dom::NodeBlueprint;
use strata_primitives::color::Rgba;
use strata_tui::prelude::*;

#[test]
fn test_largest_remainder_quantization_exact_sums() {
    // 3 equal shares across 80 columns: 80 / 3 = 26.666...
    // Largest remainder will allocate 27, 27, 26 = 80
    let tracks = largest_remainder_quantization(&[1.0, 1.0, 1.0], 80);
    assert_eq!(tracks.len(), 3);
    assert_eq!(tracks.iter().sum::<u16>(), 80);
    assert_eq!(tracks, vec![27, 27, 26]);

    // Complex fractional shares across 100 columns
    let shares = vec![1.2, 3.4, 0.8, 2.5, 4.1];
    let tracks2 = largest_remainder_quantization(&shares, 100);
    assert_eq!(tracks2.len(), 5);
    assert_eq!(tracks2.iter().sum::<u16>(), 100);

    // Unequal 30% / 70% split
    let tracks3 = largest_remainder_quantization(&[3.0, 7.0], 10);
    assert_eq!(tracks3, vec![3, 7]);
    assert_eq!(tracks3.iter().sum::<u16>(), 10);

    // Zero / empty edge cases
    assert_eq!(largest_remainder_quantization(&[], 50), Vec::<u16>::new());
    assert_eq!(largest_remainder_quantization(&[1.0, 2.0], 0), vec![0, 0]);
}

#[test]
fn test_border_junction_resolution_and_box_drawing() {
    use strata_tui::borders::cardinal::*;

    // Rounded Corners
    assert_eq!(resolve_border_junction(SOUTH | EAST, BorderJunctionStyle::Rounded), '╭');
    assert_eq!(resolve_border_junction(SOUTH | WEST, BorderJunctionStyle::Rounded), '╮');
    assert_eq!(resolve_border_junction(NORTH | EAST, BorderJunctionStyle::Rounded), '╰');
    assert_eq!(resolve_border_junction(NORTH | WEST, BorderJunctionStyle::Rounded), '╯');
    assert_eq!(resolve_border_junction(EAST | WEST, BorderJunctionStyle::Rounded), '─');
    assert_eq!(resolve_border_junction(NORTH | SOUTH, BorderJunctionStyle::Rounded), '│');
    assert_eq!(resolve_border_junction(NORTH | SOUTH | EAST | WEST, BorderJunctionStyle::Rounded), '┼');

    // Double Border Corners
    assert_eq!(resolve_border_junction(SOUTH | EAST, BorderJunctionStyle::Double), '╔');
    assert_eq!(resolve_border_junction(SOUTH | WEST, BorderJunctionStyle::Double), '╗');
    assert_eq!(resolve_border_junction(NORTH | EAST, BorderJunctionStyle::Double), '╚');
    assert_eq!(resolve_border_junction(NORTH | WEST, BorderJunctionStyle::Double), '╝');

    // Box drawing test on screen
    let mut screen = TerminalScreen::new(10, 6);
    draw_box(
        &mut screen,
        1,
        1,
        6,
        4,
        BorderJunctionStyle::Rounded,
        Color24::WHITE,
        Color24::BLACK,
        0,
    );

    assert_eq!(screen.cell(1, 1).unwrap().ch, '╭');
    assert_eq!(screen.cell(6, 1).unwrap().ch, '╮');
    assert_eq!(screen.cell(1, 4).unwrap().ch, '╰');
    assert_eq!(screen.cell(6, 4).unwrap().ch, '╯');
    assert_eq!(screen.cell(2, 1).unwrap().ch, '─');
    assert_eq!(screen.cell(1, 2).unwrap().ch, '│');
}

#[test]
fn test_terminal_screen_delta_ansi_flushing() {
    let mut screen = TerminalScreen::new(20, 5);

    // Initial draw
    screen.put_str(
        0,
        0,
        "Hello TUI",
        Color24::Rgb(0, 255, 0),
        Color24::BLACK,
        AttrBits::BOLD,
        0,
    );

    let frame1 = screen.render_delta_ansi();
    assert!(frame1.contains("\x1b[?2025h")); // Synchronized start
    assert!(frame1.contains("\x1b[?2025l")); // Synchronized end
    assert!(frame1.contains("\x1b[38;2;0;255;0m")); // TrueColor green
    assert!(frame1.contains("Hello TUI"));

    // Second render tick without changes: MUST emit ZERO content modifications!
    let frame2 = screen.render_delta_ansi();
    assert_eq!(frame2, "\x1b[?2025h\x1b[0m\x1b[?2025l");
}

#[test]
fn test_half_block_and_braille_graphics_degradation() {
    let mut screen = TerminalScreen::new(10, 5);

    // 2x2 image: top red, bottom blue
    let red = Rgba::rgb(255, 0, 0);
    let blue = Rgba::rgb(0, 0, 255);
    let pixels = vec![red, red, blue, blue];

    render_half_block(&pixels, 2, 2, &mut screen, 0, 0, 0);

    let cell = screen.cell(0, 0).unwrap();
    assert_eq!(cell.ch, '▀');
    assert_eq!(cell.fg, Color24::Rgb(255, 0, 0));
    assert_eq!(cell.bg, Color24::Rgb(0, 0, 255));

    // Braille: All 8 dots active -> U+28FF
    let full_dots = [true; 8];
    let ch = strata_tui::braille_char_from_dots(full_dots);
    assert_eq!(ch, '\u{28FF}');
}

#[test]
fn test_conditional_tui_components() {
    use strata_dom::View;

    let gui_view = View::new(NodeBlueprint::Element {
        tag: strata_dom::Tag::Div,
        classes: vec![],
        attributes: vec![],
        children: vec![],
    });
    let tui_view = View::new(NodeBlueprint::Text("Terminal Mode".into()));

    // Mode A: GUI active
    set_tui_target_active(false);
    assert!(!is_tui_target());
    let res_gui = tui_fallback(gui_view.clone(), tui_view.clone());
    match res_gui.root {
        NodeBlueprint::Element { .. } => {}
        _ => panic!("Expected GUI element"),
    }

    let no_tui_res = no_tui(vec![NodeBlueprint::Text("Visible on GUI".into())]);
    assert!(matches!(no_tui_res.root, NodeBlueprint::Fragment(_)));

    let tui_only_res = tui_only(vec![NodeBlueprint::Text("Hidden on GUI".into())]);
    assert!(matches!(tui_only_res.root, NodeBlueprint::Empty));

    // Mode B: TUI active
    set_tui_target_active(true);
    assert!(is_tui_target());
    let res_tui = tui_fallback(gui_view, tui_view);
    match res_tui.root {
        NodeBlueprint::Text(s) => assert_eq!(s, "Terminal Mode"),
        _ => panic!("Expected TUI text"),
    }

    let no_tui_res2 = no_tui(vec![NodeBlueprint::Text("Hidden on TUI".into())]);
    assert!(matches!(no_tui_res2.root, NodeBlueprint::Empty));

    let tui_only_res2 = tui_only(vec![NodeBlueprint::Text("Visible on TUI".into())]);
    assert!(matches!(tui_only_res2.root, NodeBlueprint::Fragment(_)));

    // Reset back
    set_tui_target_active(false);
}
