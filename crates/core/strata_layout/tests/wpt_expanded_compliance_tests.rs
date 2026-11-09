//! Comprehensive Integration Tests for W3C Official Test Vectors & Layout Regression.
//!
//! Validates GOAL-68: Expand Internal 14-Test WPT Regression File to Official W3C Test Vectors.
//! Executes >1,000 official W3C layout test vectors across:
//! - CSS Flexible Box Layout Module Level 1 (flex-grow, flex-shrink, multi-line wrap, cross-axis alignment)
//! - CSS Grid Layout Module Level 2 (fractional tracks, negative lines, subgrid inheritance, auto-flow)
//! - CSS 2.1 Box Model (margin collapsing, border-box, padding)
//! - CSS Sizing Level 3 (fit-content shrink-to-fit formula, min-content, max-content)
//! - CSS Position Level 3 (absolute insets, relative containing block)

use strata_layout::wpt::InternalLayoutRegressionSuite;

#[test]
fn test_official_w3c_layout_suite_executes_over_one_thousand_vectors() {
    let results = InternalLayoutRegressionSuite::run_official_w3c_suite();

    assert!(
        results.len() >= 1000,
        "WPT suite must execute over 1,000 test vectors, actual: {}",
        results.len()
    );

    let passed_count = results.iter().filter(|v| v.passed).count();
    let compliance = (passed_count as f32 / results.len() as f32) * 100.0;

    assert!(
        compliance >= 98.0,
        "W3C official compliance must exceed 98.0%, actual: {:.2}% ({}/{} passed)",
        compliance,
        passed_count,
        results.len()
    );

    let max_dev = results
        .iter()
        .map(|v| v.deviation_px)
        .fold(0.0f32, |acc, d| acc.max(d));

    assert!(
        max_dev <= 0.1,
        "Maximum deviation across all W3C vectors must be <= 0.1px, actual: {}px",
        max_dev
    );
}

#[test]
fn test_official_w3c_compliance_percentage_helper() {
    let pct = InternalLayoutRegressionSuite::official_w3c_compliance_percentage();
    assert!(
        pct >= 98.0,
        "Helper must report >= 98.0% compliance, actual: {:.2}%",
        pct
    );
}

#[test]
fn test_core_baseline_fourteen_vectors_remain_backward_compatible() {
    let baseline = InternalLayoutRegressionSuite::run_all();
    assert_eq!(baseline.len(), 14);
    assert_eq!(InternalLayoutRegressionSuite::compliance_percentage(), 100.0);
}
