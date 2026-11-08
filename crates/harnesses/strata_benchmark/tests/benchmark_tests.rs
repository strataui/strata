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
    let cold_start = bench_cold_start(3);
    assert_eq!(cold_start.name, "Cold Start to First Frame Presentation");
    assert!(cold_start.mean_us < 120_000.0, "Cold start should be < 120ms in debug builds under test load, got {} us", cold_start.mean_us);

    let flex_layout = bench_flexbox_layout_10k(2);
    assert_eq!(flex_layout.name, "W3C Flexbox Layout (10,000 Items)");
    assert_eq!(flex_layout.iterations, 2);
    assert!(flex_layout.throughput_per_sec > 0.0);

    let grid_layout = strata_benchmark::bench_w3c_grid_layout_1k(2);
    assert_eq!(grid_layout.iterations, 2);
    assert!(grid_layout.throughput_per_sec > 0.0);

    let dl_bench = strata_benchmark::bench_display_list_compilation_1k(2);
    assert_eq!(dl_bench.iterations, 2);
    assert!(dl_bench.throughput_per_sec > 0.0);
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
    assert!(md.contains("Empirical Runtime Summary"));
    assert!(md.contains("Host Operating System:"));
    assert!(md.contains("Measurement Integrity:"));
    // Ensure no fake competitor claims exist in output
    assert!(!md.contains("Electron v32"));
    assert!(!md.contains("Tauri v2"));

    let json = report.to_json();
    assert!(json.contains("\"name\":\"Engine Boot\""));
    assert!(json.contains("\"rss_mb\":2.74"));
}

#[test]
fn test_wikipedia_corpus_benchmarks() {
    let parse_res = strata_benchmark::bench_html_parsing(2);
    assert_eq!(parse_res.name, "HTML Semantic Parsing (Wikipedia Article)");
    assert_eq!(parse_res.iterations, 2);
    assert!(parse_res.throughput_per_sec > 0.0);

    let transpile_res = strata_benchmark::bench_web_transpiler(2);
    assert_eq!(transpile_res.name, "WebTranspiler Blueprint Gen (Wikipedia Article)");
    assert_eq!(transpile_res.iterations, 2);
    assert!(transpile_res.throughput_per_sec > 0.0);
}
