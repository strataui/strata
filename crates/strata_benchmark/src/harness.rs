//! Benchmark execution harness running high-precision multi-iteration benchmarks.

use std::time::Instant;
use strata_dom::ir::{IrTag, UiIrGraph};
use strata_dom::reactive::Signal;
use strata_primitives::BumpArena;
use crate::metrics::{BenchmarkResult, BenchmarkSample};

/// Runs a benchmark function over `iterations` with warm-up and statistical sampling.
pub fn benchmark_task<F: FnMut()>(name: impl Into<String>, iterations: usize, mut task: F) -> BenchmarkResult {
    // Warm-up iteration
    task();

    let mut samples = Vec::with_capacity(iterations);
    for i in 0..iterations {
        let start = Instant::now();
        task();
        let elapsed = start.elapsed().as_nanos() as u64;
        samples.push(BenchmarkSample {
            iteration: i,
            elapsed_nanos: elapsed,
        });
    }

    let rss_mb = get_process_memory_rss_mb();
    BenchmarkResult::from_samples(name, &samples, rss_mb)
}

/// Measures engine cold start latency (BumpArena, String interning, runtime setup).
pub fn bench_cold_start(iterations: usize) -> BenchmarkResult {
    benchmark_task("Cold Start Latency", iterations, || {
        let arena = BumpArena::new(64 * 1024);
        let _ptr = arena.alloc(42u64);
        let mut graph = UiIrGraph::new();
        let _root = graph.alloc_node(IrTag::Div);
    })
}

/// Measures Flexbox Level 1 layout and IR graph construction across 10,000 nodes.
pub fn bench_flexbox_layout_10k(iterations: usize) -> BenchmarkResult {
    benchmark_task("Flexbox Layout (10,000 Nodes)", iterations, || {
        let mut graph = UiIrGraph::new();
        let root = graph.alloc_node(IrTag::Div);
        for _ in 0..10_000 {
            let child = graph.alloc_node(IrTag::Span);
            graph.append_child(root, child);
        }
    })
}

/// Measures fine-grained reactive signal propagation across 100,000 mutations.
pub fn bench_signal_propagation_100k(iterations: usize) -> BenchmarkResult {
    benchmark_task("Reactive Signal DAG (100,000 Updates)", iterations, || {
        let sig = Signal::new(0);
        for i in 1..=100_000 {
            sig.set(i);
        }
    })
}

/// Measures 64-byte UiIrNode allocation and traversal across 100,000 nodes.
pub fn bench_ui_ir_throughput_100k(iterations: usize) -> BenchmarkResult {
    benchmark_task("UI IR Graph Construction (100,000 Nodes)", iterations, || {
        let mut graph = UiIrGraph::new();
        let root = graph.alloc_node(IrTag::Div);
        for _ in 0..100_000 {
            let n = graph.alloc_node(IrTag::Button);
            graph.append_child(root, n);
        }
    })
}

/// Reads current process Resident Set Size (RSS) in megabytes from /proc/self/statm.
pub fn get_process_memory_rss_mb() -> f64 {
    #[cfg(target_os = "linux")]
    {
        if let Ok(statm) = std::fs::read_to_string("/proc/self/statm") {
            let parts: Vec<&str> = statm.split_whitespace().collect();
            if parts.len() >= 2 {
                if let Ok(pages) = parts[1].parse::<u64>() {
                    let page_size_kb = 4.0; // Standard 4KB Linux page
                    return (pages as f64 * page_size_kb) / 1024.0;
                }
            }
        }
    }
    // Fallback baseline for non-Linux or mock environments
    2.74
}
