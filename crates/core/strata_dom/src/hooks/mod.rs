//! Composable reactive interaction, media query, and window control hooks.

pub mod interaction;
pub mod media;
pub mod window;

pub use interaction::{
    dispatch_click_outside, dispatch_click_outside_for_ids, get_scroll_offset,
    has_pending_scroll_commands, scroll_into_view, scroll_to, scroll_to_bottom, scroll_to_top,
    set_live_scroll_offset, snapshot_open_click_outside_ids, take_pending_scroll_commands,
    use_active, use_click_outside, use_focus, use_focus_within, use_hover, use_press,
    ClickOutsideHandle, FocusHandlers, HoverHandlers, PressHandlers, ProgrammaticScrollCommand,
};
pub use media::{
    set_system_color_scheme, set_system_contrast, set_system_reduced_motion,
    set_viewport_dimensions, use_color_scheme, use_contrast, use_media_query, use_reduced_motion,
    ColorScheme, ContrastPreference,
};
pub use window::{
    clear_cursor_override, current_cursor, dispatch_global_keydown, set_current_cursor,
    use_clipboard, use_cursor, use_keyboard_shortcut, ClipboardHandle, CursorIcon, KeyChord,
    ShortcutHandle,
};
