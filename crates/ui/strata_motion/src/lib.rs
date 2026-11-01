//! Strata Motion: Pure-Rust Numerical Spring Physics & Animation Engine.
//!
//! Provides a zero-dependency, GPU-framerate animation pipeline equivalent to Framer Motion and React Spring.
//! Includes analytical harmonic oscillators, cubic bezier easings, reactive motion values, and AnimatePresence exit coordinators.

pub mod motion_value;
pub mod presence;
pub mod spring;
pub mod tween;

pub use motion_value::{use_motion_value, MotionValue};
pub use presence::{AnimatePresenceState, PresenceItem};
pub use spring::{SpringConfig, SpringState};
pub use tween::{interpolate, Easing};

pub mod prelude {
    pub use crate::motion_value::{use_motion_value, MotionValue};
    pub use crate::presence::{AnimatePresenceState, PresenceItem};
    pub use crate::spring::{SpringConfig, SpringState};
    pub use crate::tween::{interpolate, Easing};
}
