//! Strata Native Web Browser Library.
//!
//! A zero-Chromium web browser built directly on Strata's native layout engine,
//! reactive signal graph, and WebTranspiler HTML pipeline.

#![allow(non_snake_case, dead_code, unused_variables, unused_imports)]

pub mod components;
pub mod mock_pages;
pub mod state;

pub use components::{BrowserApp, GoogleNewTab, StatusBar, TabBar, Toolbar, Viewport};
pub use mock_pages::resolve_page;
pub use state::{BrowserState, BrowserTab};
