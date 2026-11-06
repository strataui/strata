//! Opt-in performance instrumentation for the browser pipeline.
//!
//! All output is silent unless `STRATA_PERF_LOG` is set in the environment.

use std::sync::OnceLock;
use std::time::Instant;

/// Whether performance logging is enabled for this process.
pub fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("STRATA_PERF_LOG").is_some())
}

/// Logs a measured duration with a stable `[perf]` prefix when enabled.
pub fn log_ms(label: &str, start: Instant, detail: impl std::fmt::Display) {
    if enabled() {
        eprintln!(
            "[perf] {}: {:.3} ms ({})",
            label,
            start.elapsed().as_secs_f64() * 1000.0,
            detail
        );
    }
}
