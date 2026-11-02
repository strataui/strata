use strata_dom::{
    InputKey, InputKeyResult, InputModifiers, TextInputState,
};

#[test]
fn test_text_input_cursor_navigation_and_selection() {
    let mut input = TextInputState::with_text("Hello World");
    assert_eq!(input.cursor_index.get(), 11);
    assert!(!input.has_selection());

    // Move left 5 characters without selection
    for _ in 0..5 {
        input.handle_key(InputKey::Left, InputModifiers::default(), None);
    }
    assert_eq!(input.cursor_index.get(), 6);
    assert!(!input.has_selection());

    // Select backwards 5 characters with Shift+Left
    let shift = InputModifiers { shift: true, ..Default::default() };
    for _ in 0..5 {
        input.handle_key(InputKey::Left, shift, None);
    }
    assert_eq!(input.cursor_index.get(), 1);
    assert_eq!(input.selection_range(), Some((1, 6)));
    assert_eq!(input.selected_text(), Some("ello ".to_string()));

    // Select all with Ctrl+A
    let ctrl = InputModifiers { ctrl: true, ..Default::default() };
    input.handle_key(InputKey::A, ctrl, None);
    assert_eq!(input.selection_range(), Some((0, 11)));
    assert_eq!(input.selected_text(), Some("Hello World".to_string()));
}

#[test]
fn test_text_input_editing_and_undo_redo() {
    let mut input = TextInputState::with_text("Hello");

    // Insert text
    input.handle_key(InputKey::Char('!'), InputModifiers::default(), None);
    assert_eq!(input.text.get(), "Hello!");

    // Backspace
    input.handle_key(InputKey::Backspace, InputModifiers::default(), None);
    assert_eq!(input.text.get(), "Hello");

    // Undo
    let ctrl = InputModifiers { ctrl: true, ..Default::default() };
    input.handle_key(InputKey::Z, ctrl, None);
    assert_eq!(input.text.get(), "Hello!");

    // Undo again
    input.handle_key(InputKey::Z, ctrl, None);
    assert_eq!(input.text.get(), "Hello");

    // Redo
    input.handle_key(InputKey::Y, ctrl, None);
    assert_eq!(input.text.get(), "Hello!");
}

#[test]
fn test_text_input_clipboard_cut_copy_paste() {
    let mut input = TextInputState::with_text("Strata Browser");

    // Select "Browser"
    input.selection_anchor.set(Some(7));
    input.cursor_index.set(14);
    assert_eq!(input.selected_text(), Some("Browser".to_string()));

    // Copy (Ctrl+C)
    let ctrl = InputModifiers { ctrl: true, ..Default::default() };
    let res = input.handle_key(InputKey::C, ctrl, None);
    assert_eq!(res, InputKeyResult::CopyToClipboard("Browser".to_string()));
    assert_eq!(input.text.get(), "Strata Browser");

    // Cut (Ctrl+X)
    let res = input.handle_key(InputKey::X, ctrl, None);
    assert_eq!(res, InputKeyResult::CopyToClipboard("Browser".to_string()));
    assert_eq!(input.text.get(), "Strata ");

    // Paste "Engine" (Ctrl+V)
    let res = input.handle_key(InputKey::V, ctrl, Some("Engine"));
    assert_eq!(res, InputKeyResult::Handled);
    assert_eq!(input.text.get(), "Strata Engine");
}
