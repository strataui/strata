//! Strata Style: CSS spec parser, specificity calculator, cascade solver, and Tailwind JIT compiler.

pub mod types;
pub mod specificity;
pub mod selector;
pub mod parser;
pub mod cascade;
pub mod palette;
pub mod tailwind;
pub mod calc;
pub mod keyframes;
pub mod effects;

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
pub use calc::{get_calc, intern_calc, parse_calc, CalcContext, CalcExpr, CalcId};
pub use keyframes::{CubicBezier, Keyframe, KeyframeAnimation, KeyframeProperty};
pub use effects::{BackdropFilter, ClipPath, FilterOp};

pub mod prelude {
    pub use crate::types::*;
    pub use crate::specificity::Specificity;
    pub use crate::selector::{PseudoState, Selector, SelectorPart};
    pub use crate::parser::{Declaration, StyleRule, StyleSheet};
    pub use crate::cascade::CascadeSolver;
    pub use crate::palette::lookup_tailwind_color;
    pub use crate::tailwind::TailwindEngine;
    pub use crate::calc::{get_calc, intern_calc, parse_calc, CalcContext, CalcExpr, CalcId};
    pub use crate::keyframes::{CubicBezier, Keyframe, KeyframeAnimation, KeyframeProperty};
    pub use crate::effects::{BackdropFilter, ClipPath, FilterOp};
}
