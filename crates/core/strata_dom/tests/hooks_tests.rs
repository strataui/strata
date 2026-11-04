//! Unit tests for Reactive Interaction & Pseudo-Class Hooks Architecture.

use std::cell::Cell;
use std::rc::Rc;

use strata_dom::hooks::*;
use strata_dom::reactive::Signal;

#[test]
fn test_use_hover() {
    let (is_hovered, handlers) = use_hover();
    assert!(!is_hovered.get(), "Initial hover state should be false");

    // Enter element
    (handlers.onmouseenter)();
    assert!(is_hovered.get(), "Hover state should be true after mouseenter");

    // Leave element
    (handlers.onmouseleave)();
    assert!(!is_hovered.get(), "Hover state should be false after mouseleave");

    // Attributes verification
    let attrs = handlers.attributes();
    assert_eq!(attrs.len(), 2, "HoverHandlers should emit 2 DOM attributes");
}

#[test]
fn test_use_focus_and_focus_within() {
    let (is_focused, handlers) = use_focus();
    assert!(!is_focused.get(), "Initial focus state should be false");

    (handlers.onfocus)();
    assert!(is_focused.get(), "Focus state should be true after onfocus");

    (handlers.onblur)();
    assert!(!is_focused.get(), "Focus state should be false after onblur");

    // Focus Within (nested subtree tracking)
    let (has_focus, within_handlers) = use_focus_within();
    assert!(!has_focus.get());

    // First child focused
    (within_handlers.onfocus)();
    assert!(has_focus.get());

    // Second child focused (depth = 2)
    (within_handlers.onfocus)();
    assert!(has_focus.get());

    // First child blurred (depth = 1)
    (within_handlers.onblur)();
    assert!(has_focus.get(), "has_focus should stay true when nested child remains focused");

    // Second child blurred (depth = 0)
    (within_handlers.onblur)();
    assert!(!has_focus.get(), "has_focus should revert to false when all children blur");
}

#[test]
fn test_use_active_and_press() {
    let (is_active, handlers) = use_active();
    assert!(!is_active.get());

    (handlers.onmousedown)();
    assert!(is_active.get());

    (handlers.onmouseup)();
    assert!(!is_active.get());

    // Pointer leaves element while pressed down: cancels active
    (handlers.onmousedown)();
    assert!(is_active.get());
    (handlers.onmouseleave)();
    assert!(!is_active.get(), "Active state should cancel when pointer leaves");

    // Test use_press alias
    let (is_pressed, press_handlers) = use_press();
    assert!(!is_pressed.get());
    (press_handlers.onmousedown)();
    assert!(is_pressed.get());
}

#[test]
fn test_use_click_outside() {
    let is_open = Signal::new(true);
    let dismiss_count = Rc::new(Cell::new(0usize));

    let handle = {
        let count = dismiss_count.clone();
        use_click_outside(is_open.clone(), move || {
            count.set(count.get() + 1);
        })
    };

    // Click outside while open: triggers dismiss
    let triggered = dispatch_click_outside();
    assert!(triggered);
    assert_eq!(dismiss_count.get(), 1);

    // Close popup
    is_open.set(false);
    let triggered_closed = dispatch_click_outside();
    assert!(!triggered_closed);
    assert_eq!(dismiss_count.get(), 1, "Should not trigger dismiss when closed");

    // Drop handle and verify deregistration
    drop(handle);
    is_open.set(true);
    let triggered_dropped = dispatch_click_outside();
    assert!(!triggered_dropped, "Should not trigger dismiss after handle drop");
}

#[test]
fn test_use_media_query_and_system_preferences() {
    set_viewport_dimensions(1920.0, 1080.0);
    set_system_color_scheme(ColorScheme::Dark);
    set_system_reduced_motion(false);

    let is_desktop = use_media_query("(min-width: 1024px)");
    let is_mobile = use_media_query("(max-width: 640px)");
    let is_dark = use_media_query("(prefers-color-scheme: dark)");

    assert!(is_desktop.get(), "1920px should match min-width 1024px");
    assert!(!is_mobile.get(), "1920px should not match max-width 640px");
    assert!(is_dark.get(), "Should match dark color scheme");

    // Resize viewport to mobile width
    set_viewport_dimensions(375.0, 812.0);
    assert!(!is_desktop.get(), "375px should not match min-width 1024px");
    assert!(is_mobile.get(), "375px should match max-width 640px");

    // Toggle color scheme
    set_system_color_scheme(ColorScheme::Light);
    assert!(!is_dark.get(), "Light scheme should not match dark query");
    assert_eq!(use_color_scheme().get(), ColorScheme::Light);

    // Reduced motion preference
    assert!(!use_reduced_motion().get());
    set_system_reduced_motion(true);
    assert!(use_reduced_motion().get());

    // Contrast preference
    assert_eq!(use_contrast().get(), ContrastPreference::Normal);
    set_system_contrast(ContrastPreference::High);
    assert_eq!(use_contrast().get(), ContrastPreference::High);
}

#[test]
fn test_window_cursor_keyboard_shortcut_and_clipboard() {
    // Cursor hook
    let is_hovered = Signal::new(false);
    use_cursor(CursorIcon::Pointer, is_hovered.clone());

    assert_eq!(current_cursor(), CursorIcon::Default);
    is_hovered.set(true);
    assert_eq!(current_cursor(), CursorIcon::Pointer);
    is_hovered.set(false);
    assert_eq!(current_cursor(), CursorIcon::Default);

    // Manual cursor override
    set_current_cursor(CursorIcon::ColResize);
    assert_eq!(current_cursor(), CursorIcon::ColResize);
    clear_cursor_override();
    assert_eq!(current_cursor(), CursorIcon::Default);

    // Keyboard shortcut
    let shortcut_invoked = Rc::new(Cell::new(false));
    let handle = {
        let inv = shortcut_invoked.clone();
        use_keyboard_shortcut("Ctrl+Shift+P", move || {
            inv.set(true);
        })
    };

    // Non-matching shortcut
    assert!(!dispatch_global_keydown("P", false, false, false, false));
    assert!(!shortcut_invoked.get());

    // Matching shortcut
    assert!(dispatch_global_keydown("P", true, true, false, false));
    assert!(shortcut_invoked.get());

    // Unregister shortcut
    drop(handle);
    shortcut_invoked.set(false);
    assert!(!dispatch_global_keydown("P", true, true, false, false));
    assert!(!shortcut_invoked.get());

    // Clipboard hook
    let clipboard = use_clipboard();
    assert!(clipboard.text.get().is_empty());

    (clipboard.copy)("Copy test string");
    assert_eq!(clipboard.text.get(), "Copy test string");
    assert_eq!((clipboard.read_text)(), "Copy test string");
}
