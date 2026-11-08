//! Comprehensive Terminal Raw Mode, ANSI Input Parsing, SGR Mouse, and Event Loop Tests.

use strata_tui::input::{
    parse_next_event, KeyCode, KeyEvent, Modifiers, MouseAction, MouseButton, MouseEvent, TuiEvent,
};
use strata_tui::raw_mode::{enable_raw_mode, get_terminal_size};

#[test]
fn test_terminal_dimensions_query() {
    let (cols, rows) = get_terminal_size();
    assert!(cols > 0, "Terminal cols must be positive");
    assert!(rows > 0, "Terminal rows must be positive");
}

#[test]
fn test_raw_mode_guard_lifecycle() {
    let guard = enable_raw_mode().expect("Failed to initialize raw mode guard");
    // When dropped, guard restores terminal cleanly
    drop(guard);
}

#[test]
fn test_ansi_arrow_and_navigation_key_parsing() {
    // Up arrow
    let (ev, len) = parse_next_event(b"\x1b[A").expect("Parse Up");
    assert_eq!(len, 3);
    assert_eq!(ev, TuiEvent::Key(KeyEvent::new(KeyCode::Up, Modifiers::NONE)));

    // Down arrow
    let (ev, len) = parse_next_event(b"\x1b[B").expect("Parse Down");
    assert_eq!(len, 3);
    assert_eq!(ev, TuiEvent::Key(KeyEvent::new(KeyCode::Down, Modifiers::NONE)));

    // Ctrl + Up: \x1b[1;5A
    let (ev, len) = parse_next_event(b"\x1b[1;5A").expect("Parse Ctrl+Up");
    assert_eq!(len, 6);
    assert_eq!(ev, TuiEvent::Key(KeyEvent::new(KeyCode::Up, Modifiers::CTRL)));

    // Shift + Right: \x1b[1;2C
    let (ev, len) = parse_next_event(b"\x1b[1;2C").expect("Parse Shift+Right");
    assert_eq!(len, 6);
    assert_eq!(ev, TuiEvent::Key(KeyEvent::new(KeyCode::Right, Modifiers::SHIFT)));

    // Alt + Down: \x1b[1;3B
    let (ev, len) = parse_next_event(b"\x1b[1;3B").expect("Parse Alt+Down");
    assert_eq!(len, 6);
    assert_eq!(ev, TuiEvent::Key(KeyEvent::new(KeyCode::Down, Modifiers::ALT)));

    // Home / End
    let (ev, _) = parse_next_event(b"\x1b[H").unwrap();
    assert_eq!(ev, TuiEvent::Key(KeyEvent::new(KeyCode::Home, Modifiers::NONE)));
    let (ev, _) = parse_next_event(b"\x1b[F").unwrap();
    assert_eq!(ev, TuiEvent::Key(KeyEvent::new(KeyCode::End, Modifiers::NONE)));

    // PageUp / PageDown
    let (ev, _) = parse_next_event(b"\x1b[5~").unwrap();
    assert_eq!(ev, TuiEvent::Key(KeyEvent::new(KeyCode::PageUp, Modifiers::NONE)));
    let (ev, _) = parse_next_event(b"\x1b[6~").unwrap();
    assert_eq!(ev, TuiEvent::Key(KeyEvent::new(KeyCode::PageDown, Modifiers::NONE)));

    // Delete / Insert
    let (ev, _) = parse_next_event(b"\x1b[3~").unwrap();
    assert_eq!(ev, TuiEvent::Key(KeyEvent::new(KeyCode::Delete, Modifiers::NONE)));
    let (ev, _) = parse_next_event(b"\x1b[2~").unwrap();
    assert_eq!(ev, TuiEvent::Key(KeyEvent::new(KeyCode::Insert, Modifiers::NONE)));
}

#[test]
fn test_function_keys_and_control_characters() {
    // F1 through F4 via SS3
    let (ev, _) = parse_next_event(b"\x1bOP").unwrap();
    assert_eq!(ev, TuiEvent::Key(KeyEvent::new(KeyCode::F(1), Modifiers::NONE)));
    let (ev, _) = parse_next_event(b"\x1bOQ").unwrap();
    assert_eq!(ev, TuiEvent::Key(KeyEvent::new(KeyCode::F(2), Modifiers::NONE)));

    // F5 and F12 via CSI
    let (ev, _) = parse_next_event(b"\x1b[15~").unwrap();
    assert_eq!(ev, TuiEvent::Key(KeyEvent::new(KeyCode::F(5), Modifiers::NONE)));
    let (ev, _) = parse_next_event(b"\x1b[24~").unwrap();
    assert_eq!(ev, TuiEvent::Key(KeyEvent::new(KeyCode::F(12), Modifiers::NONE)));

    // Control characters
    assert_eq!(parse_next_event(b"\t").unwrap().0, TuiEvent::Key(KeyEvent::new(KeyCode::Tab, Modifiers::NONE)));
    assert_eq!(parse_next_event(b"\n").unwrap().0, TuiEvent::Key(KeyEvent::new(KeyCode::Enter, Modifiers::NONE)));
    assert_eq!(parse_next_event(b"\r").unwrap().0, TuiEvent::Key(KeyEvent::new(KeyCode::Enter, Modifiers::NONE)));
    assert_eq!(parse_next_event(b"\x7f").unwrap().0, TuiEvent::Key(KeyEvent::new(KeyCode::Backspace, Modifiers::NONE)));
    assert_eq!(parse_next_event(b"\x08").unwrap().0, TuiEvent::Key(KeyEvent::new(KeyCode::Backspace, Modifiers::NONE)));

    // Ctrl+C (byte 3)
    assert_eq!(parse_next_event(b"\x03").unwrap().0, TuiEvent::Key(KeyEvent::new(KeyCode::Char('c'), Modifiers::CTRL)));
    // Ctrl+Z (byte 26)
    assert_eq!(parse_next_event(b"\x1a").unwrap().0, TuiEvent::Key(KeyEvent::new(KeyCode::Char('z'), Modifiers::CTRL)));
}

#[test]
fn test_utf8_multibyte_character_parsing() {
    // UTF-8 '€' (3 bytes: E2 82 AC)
    let euro = "€".as_bytes();
    let (ev, len) = parse_next_event(euro).expect("Parse Euro sign");
    assert_eq!(len, 3);
    assert_eq!(ev, TuiEvent::Key(KeyEvent::char('€')));

    // UTF-8 '🦀' (4 bytes: F0 9F A6 80)
    let crab = "🦀".as_bytes();
    let (ev, len) = parse_next_event(crab).expect("Parse Crab emoji");
    assert_eq!(len, 4);
    assert_eq!(ev, TuiEvent::Key(KeyEvent::char('🦀')));
}

#[test]
fn test_mouse_sgr_1006_parsing() {
    // Left button down at col 20, row 10 (1-based in sequence: 21, 11)
    let (ev, len) = parse_next_event(b"\x1b[<0;21;11M").expect("Parse SGR Left Down");
    assert_eq!(len, 11);
    assert_eq!(
        ev,
        TuiEvent::Mouse(MouseEvent {
            button: MouseButton::Left,
            action: MouseAction::Down,
            col: 20,
            row: 10,
            modifiers: Modifiers::NONE,
        })
    );

    // Left button release at col 20, row 10 (lowercase 'm')
    let (ev, _) = parse_next_event(b"\x1b[<0;21;11m").expect("Parse SGR Left Up");
    assert_eq!(
        ev,
        TuiEvent::Mouse(MouseEvent {
            button: MouseButton::None,
            action: MouseAction::Up,
            col: 20,
            row: 10,
            modifiers: Modifiers::NONE,
        })
    );

    // Right button down
    let (ev, _) = parse_next_event(b"\x1b[<2;5;8M").expect("Parse SGR Right Down");
    assert_eq!(
        ev,
        TuiEvent::Mouse(MouseEvent {
            button: MouseButton::Right,
            action: MouseAction::Down,
            col: 4,
            row: 7,
            modifiers: Modifiers::NONE,
        })
    );

    // Scroll Wheel Up (code 64)
    let (ev, _) = parse_next_event(b"\x1b[<64;30;40M").expect("Parse SGR Scroll Up");
    assert_eq!(
        ev,
        TuiEvent::Mouse(MouseEvent {
            button: MouseButton::None,
            action: MouseAction::ScrollUp,
            col: 29,
            row: 39,
            modifiers: Modifiers::NONE,
        })
    );

    // Scroll Wheel Down (code 65)
    let (ev, _) = parse_next_event(b"\x1b[<65;30;40M").expect("Parse SGR Scroll Down");
    assert_eq!(
        ev,
        TuiEvent::Mouse(MouseEvent {
            button: MouseButton::None,
            action: MouseAction::ScrollDown,
            col: 29,
            row: 39,
            modifiers: Modifiers::NONE,
        })
    );

    // Mouse drag with Shift and Ctrl (cb = 0 + 32 (drag) + 4 (shift) + 16 (ctrl) = 52)
    let (ev, _) = parse_next_event(b"\x1b[<52;15;25M").expect("Parse SGR Drag");
    assert_eq!(
        ev,
        TuiEvent::Mouse(MouseEvent {
            button: MouseButton::Left,
            action: MouseAction::Drag,
            col: 14,
            row: 24,
            modifiers: Modifiers {
                shift: true,
                alt: false,
                ctrl: true,
            },
        })
    );
}

#[test]
fn test_bracketed_paste_and_focus_tracking() {
    // Bracketed paste: \x1b[200~Hello Pure Rust TUI!\x1b[201~
    let paste_input = b"\x1b[200~Hello Pure Rust TUI!\x1b[201~";
    let (ev, len) = parse_next_event(paste_input).expect("Parse bracketed paste");
    assert_eq!(len, paste_input.len());
    assert_eq!(ev, TuiEvent::Paste("Hello Pure Rust TUI!".to_string()));

    // Focus Gained (\x1b[I) and Lost (\x1b[O)
    let (ev, len) = parse_next_event(b"\x1b[I").unwrap();
    assert_eq!(len, 3);
    assert_eq!(ev, TuiEvent::FocusGained);

    let (ev, len) = parse_next_event(b"\x1b[O").unwrap();
    assert_eq!(len, 3);
    assert_eq!(ev, TuiEvent::FocusLost);
}

#[test]
fn test_mouse_to_platform_event_translation() {
    let mouse = MouseEvent {
        button: MouseButton::Left,
        action: MouseAction::Down,
        col: 10,
        row: 5,
        modifiers: Modifiers::NONE,
    };
    let event = TuiEvent::Mouse(mouse);
    let platform_ev = event.to_platform_event(8.0, 16.0);
    assert_eq!(
        platform_ev,
        Some(strata_platform::PlatformEvent::MouseDown {
            button: strata_platform::MouseButton::Left,
            x: 80.0,
            y: 80.0,
        })
    );

    let mouse_up = MouseEvent {
        button: MouseButton::Right,
        action: MouseAction::Up,
        col: 4,
        row: 2,
        modifiers: Modifiers::NONE,
    };
    let event_up = TuiEvent::Mouse(mouse_up);
    assert_eq!(
        event_up.to_platform_event(10.0, 20.0),
        Some(strata_platform::PlatformEvent::MouseUp {
            button: strata_platform::MouseButton::Right,
            x: 40.0,
            y: 40.0,
        })
    );

    let mouse_scroll = MouseEvent {
        button: MouseButton::None,
        action: MouseAction::ScrollDown,
        col: 0,
        row: 0,
        modifiers: Modifiers::NONE,
    };
    assert_eq!(
        TuiEvent::Mouse(mouse_scroll).to_platform_event(8.0, 16.0),
        Some(strata_platform::PlatformEvent::Scroll {
            delta_x: 0.0,
            delta_y: 1.0,
            is_momentum: false,
        })
    );

    assert_eq!(
        TuiEvent::Resize(80, 25).to_platform_event(8.0, 16.0),
        Some(strata_platform::PlatformEvent::WindowResized {
            width: 80,
            height: 25,
        })
    );

    assert_eq!(
        TuiEvent::FocusGained.to_platform_event(8.0, 16.0),
        Some(strata_platform::PlatformEvent::WindowFocused(true))
    );
}