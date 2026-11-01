//! Strata TUI: Terminal User Interface Target Backend in Pure Rust.

pub mod borders;
pub mod components;
pub mod graphics;
pub mod quantization;
pub mod screen;
pub mod types;

pub use borders::{draw_box, resolve_border_junction, BorderJunctionStyle, Cardinal};
pub use components::{is_tui_target, no_tui, set_tui_target_active, tui_fallback, tui_only};
pub use graphics::{braille_char_from_dots, render_braille, render_half_block};
pub use quantization::largest_remainder_quantization;
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
