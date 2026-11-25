//! Strata Parity Harness & Cross-Engine Benchmark Suite.

pub mod capture;
pub mod chromium_parity;
pub mod components;
pub mod determinism;
pub mod diff;
pub mod fixture;
pub mod parity_suite;
pub mod raw_bench;
pub mod sha256;
pub mod strata_runner;
pub mod tauri_runner;

pub use chromium_parity::*;
pub use components::{find_fixture, suite_fixtures, Fixture};
pub use diff::{
    compare_images_detailed, compare_screenshots, DetailedParityMetrics, DifferenceBounds,
    ParityMetrics,
};
pub use fixture::{get_fixture_html, get_fixture_view, ParityLevel};
pub use parity_suite::{run_from_args, SuiteOptions};
pub use sha256::{file_hex_digest, hex_digest};
