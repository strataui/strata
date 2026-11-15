//! Strata Icons: Built-in, zero-runtime-cost vector icon suite for Strata.

#![allow(non_snake_case, dead_code)]

pub mod icons;
pub mod path;
pub mod registry;

pub use icons::{Icon, IconConfig, Icons};
pub use path::{PathCommand, VectorPath};
pub use registry::{all_icons, lookup_icon, IconData, IconName};

pub mod prelude {
    pub use super::icons::{Icon, IconConfig, Icons};
    pub use super::path::{PathCommand, VectorPath};
    pub use super::registry::{all_icons, lookup_icon, IconData, IconName};
}
