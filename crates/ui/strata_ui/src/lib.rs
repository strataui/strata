//! Strata UI: High-Level Accessible Component Library and Design System.
//!
//! # Examples
//!
//! ```rust
//! use strata_ui::buttons::ButtonVariant;
//!
//! let variant = ButtonVariant::default();
//! assert_eq!(variant, ButtonVariant::Primary);
//! ```

#![allow(
    non_snake_case,
    dead_code,
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::field_reassign_with_default
)]

pub mod buttons;
pub mod canvas;
pub mod composite;
pub mod data_display;
pub mod feedback;
pub mod gpu_surface;
pub mod inputs;
pub mod menu;
pub mod navigation;
pub mod primitives;
pub mod scene3d;
pub mod scroll_area;
pub mod scrollbar;
pub mod title_bar;

pub mod prelude {
    pub use super::buttons::*;
    pub use super::canvas::*;
    pub use super::composite::*;
    pub use super::data_display::*;
    pub use super::feedback::*;
    pub use super::gpu_surface::*;
    pub use super::inputs::*;
    pub use super::menu::*;
    pub use super::navigation::*;
    pub use super::primitives::*;
    pub use super::scene3d::{HudCard, SceneViewport};
    pub use super::scroll_area::*;
    pub use super::scrollbar::*;
    pub use super::title_bar::*;
    pub use strata_dom::hooks::*;
}
