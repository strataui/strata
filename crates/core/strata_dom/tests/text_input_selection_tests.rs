//! Exhaustive tests for text input mouse selection, hit testing, and keyboard editing contracts.

use strata_dom::text_input::{InputKey, InputKeyResult, InputModifiers, TextInputState};

#[test]
fn test_mouse_selection_and_drag() {
    let mut input = TextInputState::with_text("Hello World from Strata");
    assert_eq!(input.cursor_index.get(), 23);
    assert!(!input.has_selection());

    // Click at index 6 ('W')
    input.start_selection(6);
    assert_eq!(input.cursor_index.get(), 6);
    assert_eq!(input.selection_anchor.get(), Some(6));
    assert!(!input.has_selection());

    // Drag to index 11 (after 'd')
    input.update_selection_drag(11);
    assert_eq!(input.cursor_index.get(), 11);
    assert!(input.has_selection());
    assert_eq!(input.selection_range(), Some((6, 11)));
    assert_eq!(input.selected_text(), Some("World".to_string()));

    // Typing a char replaces selection
    let res = input.handle_key(InputKey::Char('R'), InputModifiers::default(), None);
    assert_eq!(res, InputKeyResult::Handled);
    assert_eq!(input.text.get(), "Hello R from Strata");
    assert_eq!(input.cursor_index.get(), 7);
    assert!(!input.has_selection());
}

#[test]
fn test_double_click_word_selection() {
    let input = TextInputState::with_text("Quantum layout engine");

    // Double-click on 'a' in 'layout' (index 10)
    input.select_word_at_index(10);
    assert_eq!(input.selection_range(), Some((8, 14)));
    assert_eq!(input.selected_text(), Some("layout".to_string()));

    // Double-click on first word 'Quantum' (index 2)
    input.select_word_at_index(2);
    assert_eq!(input.selection_range(), Some((0, 7)));
    assert_eq!(input.selected_text(), Some("Quantum".to_string()));

    // Double-click on last word 'engine' (index 17)
    input.select_word_at_index(17);
    assert_eq!(input.selection_range(), Some((15, 21)));
    assert_eq!(input.selected_text(), Some("engine".to_string()));
}

#[test]
fn test_triple_click_select_all() {
    let input = TextInputState::with_text("Select all this content");
    input.select_all();
    assert_eq!(input.selection_range(), Some((0, 23)));
    assert_eq!(
        input.selected_text(),
        Some("Select all this content".to_string())
    );
}

#[test]
fn test_select_line_at_index_middle_line() {
    let input = TextInputState::with_text("alpha\nbeta\ngamma");
    input.select_line_at_index(7);
    assert_eq!(input.selection_range(), Some((6, 10)));
    assert_eq!(input.selected_text(), Some("beta".to_string()));
}

#[test]
fn test_select_line_at_index_first_line() {
    let input = TextInputState::with_text("alpha\nbeta\ngamma");
    input.select_line_at_index(2);
    assert_eq!(input.selection_range(), Some((0, 5)));
    assert_eq!(input.selected_text(), Some("alpha".to_string()));
}

#[test]
fn test_select_line_at_index_last_line() {
    let input = TextInputState::with_text("alpha\nbeta\ngamma");
    input.select_line_at_index(13);
    assert_eq!(input.selection_range(), Some((11, 16)));
    assert_eq!(input.selected_text(), Some("gamma".to_string()));
}

#[test]
fn test_select_line_at_index_single_line() {
    let input = TextInputState::with_text("hello world");
    input.select_line_at_index(4);
    assert_eq!(input.selection_range(), Some((0, 11)));
    assert_eq!(input.selected_text(), Some("hello world".to_string()));
}

#[test]
fn test_select_line_at_index_out_of_range() {
    let input = TextInputState::with_text("alpha\nbeta\ngamma");
    input.select_line_at_index(999);
    assert_eq!(input.selection_range(), Some((11, 16)));
    assert_eq!(input.selected_text(), Some("gamma".to_string()));
}

#[test]
fn test_char_index_at_offset_calculation() {
    let text = "HelloWorld";
    let font_size = 14.0;
    // char_w is approx 14.0 * 0.55 = 7.7
    assert_eq!(
        TextInputState::calculate_char_index_at_offset(text, 0.0, font_size),
        0
    );
    assert_eq!(
        TextInputState::calculate_char_index_at_offset(text, 7.7, font_size),
        1
    );
    assert_eq!(
        TextInputState::calculate_char_index_at_offset(text, 77.0, font_size),
        10
    );
    assert_eq!(
        TextInputState::calculate_char_index_at_offset(text, 200.0, font_size),
        10
    );
}

#[test]
fn test_shift_arrows_selection_extension() {
    let mut input = TextInputState::with_text("Engine");
    input.cursor_index.set(2);

    let shift = InputModifiers {
        shift: true,
        ..Default::default()
    };

    // Shift + Right -> selects 'g'
    input.handle_key(InputKey::Right, shift, None);
    assert_eq!(input.selection_range(), Some((2, 3)));
    assert_eq!(input.selected_text(), Some("g".to_string()));

    // Shift + Right -> extends to 'i'
    input.handle_key(InputKey::Right, shift, None);
    assert_eq!(input.selection_range(), Some((2, 4)));
    assert_eq!(input.selected_text(), Some("gi".to_string()));

    // Shift + Home -> extends to start
    input.handle_key(InputKey::Home, shift, None);
    assert_eq!(input.selection_range(), Some((0, 2)));
    assert_eq!(input.selected_text(), Some("En".to_string()));
}

#[test]
fn test_ctrl_word_jumping_and_deletion() {
    let mut input = TextInputState::with_text("alpha beta gamma");
    input.cursor_index.set(16);

    let ctrl = InputModifiers {
        ctrl: true,
        ..Default::default()
    };

    // Ctrl + Left -> jump before 'gamma'
    input.handle_key(InputKey::Left, ctrl, None);
    assert_eq!(input.cursor_index.get(), 11);

    // Ctrl + Backspace -> deletes 'beta '
    input.handle_key(InputKey::Backspace, ctrl, None);
    assert_eq!(input.text.get(), "alpha gamma");
    assert_eq!(input.cursor_index.get(), 6);

    // Ctrl + Z -> undoes deletion
    input.handle_key(InputKey::Z, ctrl, None);
    assert_eq!(input.text.get(), "alpha beta gamma");
}
