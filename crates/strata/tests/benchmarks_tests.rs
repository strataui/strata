//! Integration tests verifying Strata's runtime performance and memory benchmarks.

use strata::benchmarks::BenchmarkHarness;

#[test]
fn test_strata_engine_performance_benchmarks() {
    let report = BenchmarkHarness::run();
    println!("{}", report.summary());

    // Cold start < 8ms in release, < 300ms in unoptimized debug build under heavy test load
    let max_cold_start = if cfg!(debug_assertions) { 300_000 } else { 8_000 };
    assert!(
        report.cold_start_duration_us < max_cold_start,
        "Cold start took {} µs, exceeding threshold",
        report.cold_start_duration_us
    );

    // 10,000 nodes layout < 50ms (50,000 us)
    assert!(
        report.layout_10k_nodes_duration_us < 50_000,
        "10k-node layout took {} µs, exceeding 50ms threshold",
        report.layout_10k_nodes_duration_us
    );

    // Frame reconcile < 2ms (2000 us)
    assert!(
        report.frame_reconcile_duration_us < 2_000,
        "Frame reconcile took {} µs, exceeding 2ms threshold",
        report.frame_reconcile_duration_us
    );

    // Baseline memory RSS < 50MB (even in unoptimized debug build)
    assert!(
        report.memory_rss_bytes < 50 * 1024 * 1024,
        "Process RSS memory was {} bytes, exceeding 50MB",
        report.memory_rss_bytes
    );

    assert!(report.is_passing());
}
