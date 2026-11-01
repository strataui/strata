//! Strata Query: Pure-Rust Asynchronous Query Caching and Mutation Engine.
//!
//! Provides a zero-dependency equivalent to `tanstack-query` (React Query) and `swr`
//! with automatic request deduplication, Stale-While-Revalidate caching, mutation lifecycle states,
//! and cache invalidation.

pub mod cache;
pub mod mutation;
pub mod query;

pub use cache::QueryClient;
pub use mutation::{use_mutation, MutationResult};
pub use query::{use_query, QueryOptions, QueryResult, QueryStatus};

pub mod prelude {
    pub use crate::cache::QueryClient;
    pub use crate::mutation::{use_mutation, MutationResult};
    pub use crate::query::{use_query, QueryOptions, QueryResult, QueryStatus};
}
