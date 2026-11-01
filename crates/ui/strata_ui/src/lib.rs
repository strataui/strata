//! Strata UI: High-Level Accessible Component Library and Design System.

#![allow(non_snake_case, dead_code)]

pub mod primitives;
pub mod buttons;
pub mod inputs;
pub mod data_display;
pub mod feedback;
pub mod navigation;
pub mod composite;

pub mod prelude {
    pub use super::primitives::*;
    pub use super::buttons::*;
    pub use super::inputs::*;
    pub use super::data_display::*;
    pub use super::feedback::*;
    pub use super::navigation::*;
    pub use super::composite::*;
}
