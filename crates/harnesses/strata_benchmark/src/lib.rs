//! Automated universal performance benchmark and profiling suite for Project Strata.

pub mod harness;
pub mod metrics;
pub mod report;

pub use harness::{
    bench_cold_start, bench_flexbox_layout_10k, bench_signal_propagation_100k,
    bench_ui_ir_throughput_100k, bench_html_parsing, bench_web_transpiler,
    benchmark_task, get_process_memory_rss_mb,
};
pub use metrics::{BenchmarkResult, BenchmarkSample};
pub use report::BenchmarkReport;

/// Runs the complete universal benchmark suite, returning a consolidated report.
pub fn run_full_benchmark_suite(iterations: usize) -> BenchmarkReport {
    let r1 = bench_cold_start(iterations);
    let r2 = bench_flexbox_layout_10k(iterations);
    let r3 = bench_signal_propagation_100k(iterations);
    let r4 = bench_ui_ir_throughput_100k(iterations);
    let r5 = bench_html_parsing(iterations);
    let r6 = bench_web_transpiler(iterations);

    BenchmarkReport::new(vec![r1, r2, r3, r4, r5, r6])
}
