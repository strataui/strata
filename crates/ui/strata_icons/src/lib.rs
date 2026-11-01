//! Strata Icons: Built-in, zero-runtime-cost vector icon suite for Strata.

#![allow(non_snake_case, dead_code)]

pub mod path;
pub mod registry;
pub mod icons;

pub use path::{PathCommand, VectorPath};
pub use registry::{lookup_icon, all_icons, IconData, IconName};
pub use icons::{Icon, IconConfig, Icons};

pub mod prelude {
    pub use super::path::{PathCommand, VectorPath};
    pub use super::registry::{lookup_icon, all_icons, IconData, IconName};
    pub use super::icons::{Icon, IconConfig, Icons};
}
