//! Automated universal performance benchmark and profiling suite for Project Strata.

#![allow(clippy::field_reassign_with_default)]

pub mod harness;
pub mod metrics;
pub mod report;

pub use harness::{
    bench_cold_start, bench_flexbox_layout_10k, bench_signal_propagation_100k,
    bench_ui_ir_throughput_100k, bench_html_parsing, bench_web_transpiler,
    bench_w3c_flexbox_layout_1k, bench_w3c_grid_layout_1k, bench_display_list_compilation_1k,
    benchmark_task, get_process_memory_rss_mb,
};
pub use metrics::{BenchmarkResult, BenchmarkSample};
pub use report::BenchmarkReport;

/// Runs the complete universal benchmark suite, returning a consolidated report.
pub fn run_full_benchmark_suite(iterations: usize) -> BenchmarkReport {
    let r1 = bench_cold_start(iterations);
    let r2 = bench_w3c_flexbox_layout_1k(iterations);
    let r3 = bench_flexbox_layout_10k(iterations);
    let r4 = bench_w3c_grid_layout_1k(iterations);
    let r5 = bench_display_list_compilation_1k(iterations);
    let r6 = bench_signal_propagation_100k(iterations);
    let r7 = bench_ui_ir_throughput_100k(iterations);
    let r8 = bench_html_parsing(iterations);
    let r9 = bench_web_transpiler(iterations);

    BenchmarkReport::new(vec![r1, r2, r3, r4, r5, r6, r7, r8, r9])
}
