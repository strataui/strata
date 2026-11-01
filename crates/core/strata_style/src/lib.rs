//! Strata Style: CSS spec parser, specificity calculator, cascade solver, and Tailwind JIT compiler.

pub mod types;
pub mod specificity;
pub mod selector;
pub mod parser;
pub mod cascade;
pub mod palette;
pub mod tailwind;

pub use types::*;
pub use specificity::Specificity;
pub use selector::{PseudoState, Selector, SelectorPart};
pub use parser::{
    parse_color, parse_declarations, parse_dimension, parse_edge_insets_px, Declaration, StyleRule,
    StyleSheet,
};
pub use cascade::CascadeSolver;
pub use palette::lookup_tailwind_color;
pub use tailwind::TailwindEngine;

pub mod prelude {
    pub use crate::types::*;
    pub use crate::specificity::Specificity;
    pub use crate::selector::{PseudoState, Selector, SelectorPart};
    pub use crate::parser::{Declaration, StyleRule, StyleSheet};
    pub use crate::cascade::CascadeSolver;
    pub use crate::palette::lookup_tailwind_color;
    pub use crate::tailwind::TailwindEngine;
}
