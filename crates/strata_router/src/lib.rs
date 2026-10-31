//! Strata Router: Universal Type-Safe Routing Engine.
//!
//! Provides a pure-Rust declarative routing engine equivalent to `react-router` and `tanstack-router`
//! with parameterized path matching, query string parsing, history stack management, and reactive navigation hooks.

pub mod components;
pub mod context;
pub mod history;
pub mod matcher;

pub use components::{render_routes, Link, Navigate, RouteEntry};
pub use context::{
    init_router, use_location, use_navigate, use_params, use_router, Navigator, RouterContext,
};
pub use history::{HistoryAction, Location, MemoryHistory};
pub use matcher::{match_path, normalize_path, parse_query_string, RouteMatch};

pub mod prelude {
    pub use crate::components::{render_routes, Link, Navigate, RouteEntry};
    pub use crate::context::{
        init_router, use_location, use_navigate, use_params, use_router, Navigator, RouterContext,
    };
    pub use crate::history::{HistoryAction, Location, MemoryHistory};
    pub use crate::matcher::{match_path, parse_query_string, RouteMatch};
}
