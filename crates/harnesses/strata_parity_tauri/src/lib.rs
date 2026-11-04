//! Strata Parity Harness & Cross-Engine Benchmark Suite.

pub mod capture;
pub mod chromium_parity;
pub mod diff;
pub mod fixture;
pub mod raw_bench;
pub mod strata_runner;
pub mod tauri_runner;

pub use chromium_parity::*;
pub use diff::{compare_screenshots, ParityMetrics};
pub use fixture::{get_fixture_html, get_fixture_view, ParityLevel};
