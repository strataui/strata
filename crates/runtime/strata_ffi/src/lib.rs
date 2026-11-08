//! # Strata Universal C-FFI
//!
//! Foreign Function Interface providing stable C ABI symbols for embedding
//! the Strata reactive UI engine inside Python, Swift, Kotlin/Android, Go, Node.js, and C/C++.

#![allow(clippy::missing_safety_doc)]

pub mod app;
pub mod bridge;
pub mod signal;

pub use app::*;
pub use bridge::*;
pub use signal::*;
