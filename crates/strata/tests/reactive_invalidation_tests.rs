//! Integration tests for GOAL-91: Fine-grained reactive signal DOM invalidation,
//! zero-rebuild keyboard navigation, and in-place dynamic text synchronization.

use strata::prelude::*;
use strata_dom::reactive::{has_dirty_signals, mark_dirty, take_dirty_signals, Signal};
use strata_platform::{KeyCode, Modifiers, PlatformEvent};

#[component]
fn TextInputHarness() -> View {
    let initial_val = "Hello Strata Reactive Engine".to_string();
    rsx! {
        <div class="p-4 bg-neutral-900">
            <input
                type="text"
                value={initial_val}
                placeholder="Type here..."
                class="w-64 px-2 py-1 text-sm bg-neutral-800 text-white outline-none"
            />
        </div>
    }
}

#[test]
fn test_keyboard_navigation_returns_redraw_only() {
    let mut runtime = AppRuntime::new(TextInputHarness(), 800.0, 600.0, 1.0);

    // Initial click to focus the text input
    let action = runtime.dispatch_pointer_down_action(30.0, 24.0);
    assert_eq!(
        action,
        PointerDownAction::FocusChanged,
        "first click should focus the input element"
    );
    assert!(runtime.active_focus.is_some());

    // Navigation keys must return RedrawOnly (zero component / layout tree rebuild)
    let nav_keys = [
        KeyCode::ArrowLeft,
        KeyCode::ArrowRight,
        KeyCode::Home,
        KeyCode::End,
    ];

    for key in nav_keys {
        let event = PlatformEvent::KeyDown {
            key,
            modifiers: Modifiers::default(),
        };
        let result = runtime.dispatch_key_event(&event);
        assert_eq!(
            result,
            KeyDispatchResult::RedrawOnly,
            "navigation key {:?} must result in RedrawOnly",
            key
        );
        assert!(result.needs_redraw());
        assert!(!result.needs_rebuild());
    }

    // Text character insertion must return NeedsRebuild
    let char_event = PlatformEvent::ReceivedCharacter('a');
    let edit_result = runtime.dispatch_key_event(&char_event);
    assert_eq!(
        edit_result,
        KeyDispatchResult::NeedsRebuild,
        "character insertion must return NeedsRebuild"
    );
    assert!(edit_result.needs_rebuild());
    assert!(edit_result.needs_redraw());

    // Backspace must return NeedsRebuild
    let backspace_event = PlatformEvent::KeyDown {
        key: KeyCode::Backspace,
        modifiers: Modifiers::default(),
    };
    let bs_result = runtime.dispatch_key_event(&backspace_event);
    assert_eq!(
        bs_result,
        KeyDispatchResult::NeedsRebuild,
        "backspace must return NeedsRebuild"
    );
    assert!(bs_result.needs_rebuild());
}

#[test]
fn test_pointer_actions_in_focused_input() {
    let mut runtime = AppRuntime::new(TextInputHarness(), 800.0, 600.0, 1.0);

    // Initial focus
    let first_click = runtime.dispatch_pointer_down_action(30.0, 24.0);
    assert_eq!(first_click, PointerDownAction::FocusChanged);
    let _ = runtime.dispatch_pointer_up_action(30.0, 24.0);

    // Second click inside already focused input must only move caret
    let second_click = runtime.dispatch_pointer_down_action(50.0, 24.0);
    assert_eq!(
        second_click,
        PointerDownAction::CaretMoved,
        "clicking within already focused input must return CaretMoved"
    );
    assert!(second_click.needs_redraw());
    assert!(!second_click.needs_rebuild());

    // Mouse drag across input selects text
    let drag_move = runtime.dispatch_pointer_move_action(70.0, 24.0);
    assert_eq!(
        drag_move,
        PointerMoveAction::SelectionDragged,
        "dragging pointer while focused must return SelectionDragged"
    );
    assert!(drag_move.needs_redraw());
    assert!(!drag_move.needs_rebuild());

    // Releasing pointer ends selection
    let drag_up = runtime.dispatch_pointer_up_action(70.0, 24.0);
    assert_eq!(
        drag_up,
        PointerUpAction::SelectionEnded,
        "releasing pointer after drag must return SelectionEnded"
    );
    assert!(drag_up.needs_redraw());
    assert!(!drag_up.needs_rebuild());
}

#[test]
fn test_reactive_dirty_signal_lifecycle() {
    // Reset dirty signal state
    let _ = take_dirty_signals();
    assert!(!has_dirty_signals());

    mark_dirty();
    assert!(has_dirty_signals());
    assert!(take_dirty_signals());
    assert!(!has_dirty_signals());

    // Creating a signal and updating it sets dirty signals flag
    let count = Signal::new(10);
    assert_eq!(count.get(), 10);
    let _ = take_dirty_signals();

    count.set(20);
    assert_eq!(count.get(), 20);
    assert!(
        has_dirty_signals(),
        "updating signal value must notify and mark dirty_signals flag"
    );
    assert!(take_dirty_signals());
    assert!(!has_dirty_signals());
}

#[test]
fn test_in_place_dynamic_text_sync_without_rebuild() {
    let count = Signal::new(42);
    let count_read = count.clone();

    let view = rsx! {
        <div class="p-2 flex flex-row">
            <span class="text-sm font-sans">{count_read}</span>
        </div>
    };

    let mut runtime = AppRuntime::new(view, 400.0, 200.0, 1.0);

    // Verify dynamic text binding was discovered and registered
    assert_eq!(
        runtime.dynamic_text_bindings.len(),
        1,
        "dynamic text node must register a DynamicTextBinding"
    );
    assert_eq!(runtime.dynamic_text_bindings[0].last_value, "42");

    // Clear dirty signals
    let _ = take_dirty_signals();

    // Mutate the signal value
    count.set(1337);
    assert!(has_dirty_signals());

    // In-place synchronization
    let updated = runtime.sync_dynamic_text();
    assert!(updated, "sync_dynamic_text must return true when text mutated");
    assert_eq!(runtime.dynamic_text_bindings[0].last_value, "1337");

    // Second sync without mutations must return false
    let no_update = runtime.sync_dynamic_text();
    assert!(!no_update, "subsequent sync without changes must return false");
}
