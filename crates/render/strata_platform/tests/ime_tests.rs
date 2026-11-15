//! Integration tests for Operating System IME (Input Method Editor) and CJK text composition.

use strata_platform::{
    gtk_reset_ime, gtk_set_ime_cursor_location, gtk_set_ime_surrounding_text, MockPlatformWindow,
    PlatformEvent, PlatformWindow, WindowConfig,
};

#[test]
fn test_ime_platform_event_variants() {
    let preedit = PlatformEvent::ImePreedit {
        text: "nihong".to_string(),
        cursor: Some((6, 6)),
    };
    assert_eq!(
        preedit,
        PlatformEvent::ImePreedit {
            text: "nihong".to_string(),
            cursor: Some((6, 6)),
        }
    );

    let commit = PlatformEvent::ImeCommit("日本語".to_string());
    assert_eq!(commit, PlatformEvent::ImeCommit("日本語".to_string()));

    let del = PlatformEvent::ImeDeleteSurrounding {
        offset: -2,
        n_chars: 4,
    };
    assert_eq!(
        del,
        PlatformEvent::ImeDeleteSurrounding {
            offset: -2,
            n_chars: 4,
        }
    );
}

#[test]
fn test_mock_platform_window_ime_lifecycle() {
    let mut win = MockPlatformWindow::new(WindowConfig::default());
    assert_eq!(win.ime_cursor_location, None);
    assert_eq!(win.ime_surrounding, None);

    // Set caret bounding box for candidate window positioning
    win.set_ime_cursor_location(120, 240, 2, 18);
    assert_eq!(win.ime_cursor_location, Some((120, 240, 2, 18)));

    // Set surrounding text buffer for context-aware prediction
    win.set_ime_surrounding_text("Hello world", 5);
    assert_eq!(win.ime_surrounding, Some(("Hello world".to_string(), 5)));

    // Reset IME session
    win.reset_ime();
    assert_eq!(win.ime_cursor_location, None);
    assert_eq!(win.ime_surrounding, None);
}

#[test]
fn test_gtk_ime_api_safe_execution() {
    // Calling GTK IME helpers safely even without an active GTK window
    gtk_set_ime_cursor_location(100, 200, 2, 16);
    gtk_set_ime_surrounding_text("Beijing Shanghai", 7);
    gtk_reset_ime();
}
