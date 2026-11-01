//! Statistical metrics collector for high-precision micro-benchmarking.

/// A single execution timing sample.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BenchmarkSample {
    pub iteration: usize,
    pub elapsed_nanos: u64,
}

/// Aggregated statistical benchmark result across multiple iterations.
#[derive(Debug, Clone, PartialEq)]
pub struct BenchmarkResult {
    pub name: String,
    pub iterations: usize,
    pub mean_us: f64,
    pub min_us: f64,
    pub max_us: f64,
    pub p95_us: f64,
    pub p99_us: f64,
    pub throughput_per_sec: f64,
    pub memory_rss_mb: f64,
}

impl BenchmarkResult {
    /// Computes summary statistics from raw timing samples.
    pub fn from_samples(name: impl Into<String>, samples: &[BenchmarkSample], memory_rss_mb: f64) -> Self {
        let n = samples.len().max(1);
        let mut nanos: Vec<u64> = samples.iter().map(|s| s.elapsed_nanos).collect();
        nanos.sort_unstable();

        let sum_nanos: u64 = nanos.iter().sum();
        let mean_nanos = sum_nanos as f64 / n as f64;
        let min_nanos = *nanos.first().unwrap_or(&0);
        let max_nanos = *nanos.last().unwrap_or(&0);

        let p95_idx = ((n as f64 * 0.95).ceil() as usize).saturating_sub(1).min(n - 1);
        let p99_idx = ((n as f64 * 0.99).ceil() as usize).saturating_sub(1).min(n - 1);

        let p95_nanos = nanos[p95_idx];
        let p99_nanos = nanos[p99_idx];

        let mean_us = mean_nanos / 1_000.0;
        let min_us = min_nanos as f64 / 1_000.0;
        let max_us = max_nanos as f64 / 1_000.0;
        let p95_us = p95_nanos as f64 / 1_000.0;
        let p99_us = p99_nanos as f64 / 1_000.0;

        let throughput = if mean_nanos > 0.0 {
            1_000_000_000.0 / mean_nanos
        } else {
            0.0
        };

        Self {
            name: name.into(),
            iterations: n,
            mean_us,
            min_us,
            max_us,
            p95_us,
            p99_us,
            throughput_per_sec: throughput,
            memory_rss_mb,
        }
    }
}
