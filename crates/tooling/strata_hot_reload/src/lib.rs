//! State-preserving native code hot-reloading orchestrator for Project Strata.

pub mod protocol;
pub mod reloader;
pub mod snapshot;
pub mod watcher;

pub use protocol::HotReloadMessage;
pub use reloader::HotReloader;
pub use snapshot::{
    ComponentSnapshot, FieldAction, FieldMigration, GenerationalFieldValue, HotReloadSnapshot,
    SignalMigration, SignalSnapshot,
};
pub use watcher::FileChangeTracker;
