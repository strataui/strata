//! Strata TUI: Terminal User Interface Target Backend in Pure Rust.

#![allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::field_reassign_with_default
)]

pub mod borders;
pub mod components;
pub mod graphics;
pub mod quantization;
pub mod screen;
pub mod types;
pub mod raw_mode;
pub mod input;
pub mod event_loop;
pub mod kitty;
pub mod sixel;

pub use borders::{draw_box, resolve_border_junction, BorderJunctionStyle, Cardinal};
pub use components::{is_tui_target, no_tui, set_tui_target_active, tui_fallback, tui_only};
pub use event_loop::TuiApp;
pub use graphics::{braille_char_from_dots, render_braille, render_half_block};
pub use input::{
    parse_next_event, KeyCode, KeyEvent, Modifiers, MouseAction, MouseButton, MouseEvent,
    TuiEvent, TuiEventReader,
};
pub use kitty::{delete_kitty_images, encode_kitty_png, encode_kitty_rgba, KittyImageOptions};
pub use sixel::{encode_sixel, SixelOptions};
pub use quantization::largest_remainder_quantization;
pub use raw_mode::{enable_raw_mode, get_terminal_size, restore_terminal, RawModeGuard};
pub use screen::TerminalScreen;
pub use types::{AttrBits, CharCell, Color24};

pub mod prelude {
    pub use super::borders::{draw_box, resolve_border_junction, BorderJunctionStyle, Cardinal};
    pub use super::components::{
        is_tui_target, no_tui, set_tui_target_active, tui_fallback, tui_only,
    };
    pub use super::graphics::{render_braille, render_half_block};
    pub use super::quantization::largest_remainder_quantization;
    pub use super::screen::TerminalScreen;
    pub use super::types::{AttrBits, CharCell, Color24};
}
