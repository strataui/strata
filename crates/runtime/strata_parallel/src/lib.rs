//! Strata Parallel: Multi-Threaded Parallel Subtree Reconciliation, Layout & Display List Assembly.

pub mod arena;
pub mod containment;
pub mod dirty_queue;
pub mod engine;

pub use arena::ThreadLocalArena;
pub use containment::{ContainmentClassifier, PartitionBoundary, PartitionId};
pub use dirty_queue::DirtySignalQueue;
pub use engine::{
    translate_draw_command, DisplayListChunk, LayoutConstraints, LayoutPartition, ParallelEngine,
};

pub mod prelude {
    pub use super::containment::{ContainmentClassifier, PartitionBoundary, PartitionId};
    pub use super::dirty_queue::DirtySignalQueue;
    pub use super::engine::{
        translate_draw_command, DisplayListChunk, LayoutConstraints, LayoutPartition,
        ParallelEngine,
    };
}
