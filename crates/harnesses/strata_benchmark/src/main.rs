//! CLI binary runner for Project Strata benchmarks.

use strata_benchmark::run_full_benchmark_suite;

fn main() {
    println!("Project Strata — Universal Performance Benchmark Suite v0.1.0");
    println!("─────────────────────────────────────────────────────────────");

    let report = run_full_benchmark_suite(10);
    println!("{}", report.to_markdown());
}
