//! Strata Virtual: High-Scale List & Grid Virtualization Engine.
//!
//! Provides a pure-Rust, zero-dependency equivalent to `tanstack-virtual` and `react-window`
//! rendering 1,000,000+ items with constant $O(1)$ memory usage, dynamic size measurement,
//! and 2D virtual grid slicing.

pub mod grid;
pub mod hook;
pub mod virtualizer;

pub use grid::{VirtualGrid, VirtualGridCell, VirtualGridOptions};
pub use hook::{use_virtualizer, VirtualizerHandle};
pub use virtualizer::{ScrollAlign, VirtualItem, VirtualOptions, Virtualizer};

pub mod prelude {
    pub use crate::grid::{VirtualGrid, VirtualGridCell, VirtualGridOptions};
    pub use crate::hook::{use_virtualizer, VirtualizerHandle};
    pub use crate::virtualizer::{ScrollAlign, VirtualItem, VirtualOptions, Virtualizer};
}
