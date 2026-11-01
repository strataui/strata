//! Strata Store: Lightweight Global State Store with Slice Subscriptions and WAL Persistence.
//!
//! Provides a pure-Rust, zero-runtime-overhead equivalent to Zustand and Redux Toolkit.
//! Supports atomic signals, fine-grained selector slices, bounded time-travel undo/redo,
//! and automatic Bitcask WAL persistence.

pub mod history;
pub mod persist;
pub mod store;

pub use history::HistoryStore;
pub use persist::{create_persistent_store, PersistOptions};
pub use store::{create_store, Store};

pub mod prelude {
    pub use crate::history::HistoryStore;
    pub use crate::persist::{create_persistent_store, PersistOptions};
    pub use crate::store::{create_store, Store};
}
