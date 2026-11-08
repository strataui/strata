//! Strata UI: High-Level Accessible Component Library and Design System.

#![allow(
    non_snake_case,
    dead_code,
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::field_reassign_with_default
)]

pub mod primitives;
pub mod buttons;
pub mod inputs;
pub mod data_display;
pub mod feedback;
pub mod navigation;
pub mod composite;
pub mod canvas;
pub mod scrollbar;
pub mod scroll_area;
pub mod title_bar;
pub mod scene3d;
pub mod gpu_surface;
pub mod menu;

pub mod prelude {
    pub use super::primitives::*;
    pub use super::buttons::*;
    pub use super::inputs::*;
    pub use super::data_display::*;
    pub use super::feedback::*;
    pub use super::navigation::*;
    pub use super::composite::*;
    pub use super::canvas::*;
    pub use super::scrollbar::*;
    pub use super::scroll_area::*;
    pub use super::title_bar::*;
    pub use super::scene3d::{HudCard, SceneViewport};
    pub use super::gpu_surface::*;
    pub use super::menu::*;
    pub use strata_dom::hooks::*;
}

