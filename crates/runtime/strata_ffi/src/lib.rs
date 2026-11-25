//! # Strata Universal C99 FFI
//!
//! Production Foreign Function Interface providing panic-safe C ABI symbols for embedding
//! the Strata reactive UI engine inside C, C++, Python (`ctypes`/`cffi`), Swift, Kotlin/JNI,
//! Go (`cgo`), and Node-API hosts.

#![allow(clippy::missing_safety_doc)]

pub mod app;
pub mod bridge;
pub mod dom;
pub mod error;
pub mod signal;
pub mod style;

pub use app::*;
pub use bridge::*;
pub use dom::*;
pub use error::*;
pub use signal::*;
pub use style::{format_css_color, format_css_dimension, serialize_computed_style_css};
