use strata_benchmark::{
    bench_cold_start, bench_flexbox_layout_10k, bench_signal_propagation_100k,
    BenchmarkReport, BenchmarkResult, BenchmarkSample,
};

#[test]
fn test_benchmark_metrics_calculation() {
    let samples = vec![
        BenchmarkSample { iteration: 0, elapsed_nanos: 10_000 },
        BenchmarkSample { iteration: 1, elapsed_nanos: 20_000 },
        BenchmarkSample { iteration: 2, elapsed_nanos: 30_000 },
        BenchmarkSample { iteration: 3, elapsed_nanos: 40_000 },
        BenchmarkSample { iteration: 4, elapsed_nanos: 50_000 },
    ];

    let result = BenchmarkResult::from_samples("Test Task", &samples, 2.74);
    assert_eq!(result.iterations, 5);
    assert!((result.mean_us - 30.0).abs() < 1e-4);
    assert!((result.min_us - 10.0).abs() < 1e-4);
    assert!((result.max_us - 50.0).abs() < 1e-4);
    assert!(result.throughput_per_sec > 0.0);
    assert_eq!(result.memory_rss_mb, 2.74);
}

#[test]
fn test_cold_start_and_layout_benchmarks() {
    let cold_start = bench_cold_start(5);
    assert!(cold_start.mean_us < 5_000.0, "Cold start should be sub-5ms in debug builds");

    let flex_layout = bench_flexbox_layout_10k(2);
    assert_eq!(flex_layout.iterations, 2);
    assert!(flex_layout.throughput_per_sec > 0.0);
}

#[test]
fn test_signal_propagation_benchmark() {
    let signal_bench = bench_signal_propagation_100k(2);
    assert_eq!(signal_bench.iterations, 2);
    assert!(signal_bench.throughput_per_sec > 0.0);
}

#[test]
fn test_report_markdown_and_json_generation() {
    let sample = BenchmarkSample { iteration: 0, elapsed_nanos: 16_000 };
    let res = BenchmarkResult::from_samples("Engine Boot", &[sample], 2.74);
    let report = BenchmarkReport::new(vec![res]);

    let md = report.to_markdown();
    assert!(md.contains("# Project Strata — Universal Performance Benchmark Report"));
    assert!(md.contains("Engine Boot"));
    assert!(md.contains("Strata (Pure Rust)"));
    assert!(md.contains("Electron v32"));

    let json = report.to_json();
    assert!(json.contains("\"name\":\"Engine Boot\""));
    assert!(json.contains("\"rss_mb\":2.74"));
}
