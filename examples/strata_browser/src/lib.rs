//! Strata Native Web Browser Library.
//!
//! A zero-Chromium web browser built directly on Strata's native layout engine,
//! reactive signal graph, and WebTranspiler HTML pipeline.

#![allow(non_snake_case, dead_code, unused_variables, unused_imports)]

pub mod components;
pub mod network;
pub mod perf;
pub mod sandboxing;
pub mod state;
pub mod tab_worker;

pub use components::{BrowserApp, GoogleNewTab, StatusBar, TabBar, Toolbar, Viewport};
pub use network::{load_page, normalize_target, PageLoadResult};
pub use sandboxing::{
    ProcessStatus, SandboxPolicy, SandboxedJob, SandboxedResponse, SandboxedTabPool,
    SandboxedTabWorker,
};
pub use state::{BookmarkItem, BrowserState, BrowserTab, ContextTarget};
pub use tab_worker::{TabWorkerJob, TabWorkerPool, TabWorkerResponse};
