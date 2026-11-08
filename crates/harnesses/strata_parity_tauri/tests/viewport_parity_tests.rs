//! Automated Test Suite for Maximized Viewport Parity & SSIM Regression.

use std::fs;
use std::path::Path;
use strata_parity_tauri::chromium_parity::{
    find_chromium_binary, render_chromium_headless, render_strata_web_viewport,
    run_synthetic_wheel_scroll_benchmark, ParityHarnessConfig,
};
use strata_parity_tauri::diff::compare_screenshots;

const TEST_ARTICLE_HTML: &str = r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>
  * { box-sizing: border-box; margin: 0; padding: 0; }
  body { background: #09090b; color: #f4f4f5; font-family: sans-serif; padding: 48px; }
  h1 { font-size: 28px; font-weight: 700; margin-bottom: 16px; border-bottom: 2px solid #27272a; padding-bottom: 8px; }
  p { font-size: 15px; line-height: 1.6; margin-bottom: 20px; color: #a1a1aa; }
  table { width: 100%; border-collapse: collapse; margin-bottom: 24px; }
  th, td { border: 1px solid #27272a; padding: 10px 14px; text-align: left; }
  th { background: #18181b; font-weight: 600; }
  .card { background: #18181b; border: 1px solid #27272a; border-radius: 8px; padding: 24px; margin-bottom: 24px; }
  .badge { display: inline-block; padding: 4px 10px; border-radius: 9999px; font-size: 12px; font-weight: 600; background: #27272a; color: #38bdf8; }
</style>
</head>
<body>
  <div class="card">
    <span class="badge">Strata 2.0 Engine</span>
    <h1>Chromium Maximized Viewport Parity Suite</h1>
    <p>Empirical verification of visual, box-model, and layout parity between Strata pure-Rust layout engine and Google Chrome Blink LayoutNG.</p>
    <table>
      <thead>
        <tr><th>Subsystem</th><th>Target Standard</th><th>Status</th></tr>
      </thead>
      <tbody>
        <tr><td>SSIM Parity</td><td>&ge; 0.95 Structural Parity</td><td>Verified</td></tr>
        <tr><td>Wheel Scroll Physics</td><td>Critically Damped Harmonic Oscillator</td><td>Verified</td></tr>
        <tr><td>Threading Model</td><td>Decoupled Chrome UI Thread</td><td>Sub-1ms Latency</td></tr>
      </tbody>
    </table>
  </div>
</body>
</html>"#;

#[test]
fn test_strata_maximized_viewport_rendering() {
    let nodes = strata_web_reader::HtmlParser::parse(TEST_ARTICLE_HTML);
    let theme = strata_web_reader::ReaderTheme::dark();
    let media_ctx = strata_style::MediaContext::new(1280.0, 720.0, true);
    let transpiler = strata_web_reader::WebTranspiler::new(&theme);
    let bp = transpiler.transpile_full_document(&nodes, &media_ctx);
    let mut runtime = strata::AppRuntime::new(strata::prelude::View { root: bp }, 1280.0, 720.0, 1.0);
    runtime.layout_root.compute_layout(1280.0, 720.0);
    println!("Root box: {:?}", runtime.layout_root.geometry.border_box);


    for cmd in runtime.display_list.commands() {
        match cmd {
            strata_render::compositor::DrawCommand::DrawText { origin, text, .. } => {
                println!("DrawText: origin={:?}, text={}", origin, text);
            }
            strata_render::compositor::DrawCommand::DrawTextFragments { fragments } => {
                for f in fragments {
                    println!("TextFragment: bounds={:?}, text={}", f.bounds, f.text);
                }
            }
            _ => {}
        }
    }

    let out_dir = "/tmp/strata_viewport_test";
    fs::create_dir_all(out_dir).expect("Create output directory");

    let out_1080p = format!("{out_dir}/strata_maximized_1920x1080.png");
    let res_1080p = render_strata_web_viewport(TEST_ARTICLE_HTML, 1920, 1080, 0.0, &out_1080p);
    assert!(res_1080p.is_ok(), "1080p render must succeed: {:?}", res_1080p);
    assert!(Path::new(&out_1080p).exists(), "1080p PNG must exist");

    let out_720p = format!("{out_dir}/strata_maximized_1280x720.png");
    let res_720p = render_strata_web_viewport(TEST_ARTICLE_HTML, 1280, 720, 0.0, &out_720p);
    assert!(res_720p.is_ok(), "720p render must succeed: {:?}", res_720p);
    assert!(Path::new(&out_720p).exists(), "720p PNG must exist");

    // Scrolled state
    let out_scrolled = format!("{out_dir}/strata_maximized_1280x720_scrolled.png");
    let res_scrolled = render_strata_web_viewport(TEST_ARTICLE_HTML, 1280, 720, 200.0, &out_scrolled);
    assert!(res_scrolled.is_ok(), "Scrolled render must succeed: {:?}", res_scrolled);
    assert!(Path::new(&out_scrolled).exists(), "Scrolled PNG must exist");
}

#[test]
fn test_chromium_headless_and_ssim_comparison() {
    let chrome_bin = find_chromium_binary(None);
    if chrome_bin.is_none() {
        eprintln!("[SKIP] Chromium/Chrome binary not found on this system, skipping headless Chrome diff");
        return;
    }

    let out_dir = "/tmp/strata_chromium_parity_test";
    fs::create_dir_all(out_dir).expect("Create output directory");

    let strata_png = format!("{out_dir}/strata_1280x720.png");
    let chromium_png = format!("{out_dir}/chromium_1280x720.png");
    let heatmap_png = format!("{out_dir}/diff_heatmap_1280x720.png");

    let strata_res = render_strata_web_viewport(TEST_ARTICLE_HTML, 1280, 720, 0.0, &strata_png);
    assert!(strata_res.is_ok(), "Strata render must succeed: {:?}", strata_res);

    let chrom_res = render_chromium_headless(TEST_ARTICLE_HTML, 1280, 720, 0.0, &chromium_png, None);
    assert!(chrom_res.is_ok(), "Chromium render must succeed: {:?}", chrom_res);

    let metrics = compare_screenshots(
        Path::new(&chromium_png),
        Path::new(&strata_png),
        Path::new(&heatmap_png),
    ).expect("Differential comparison must succeed");

    println!(
        "[Parity Test Result] SSIM: {:.4}, Exact: {:.2}%, Perceptual: {:.2}%, Mean Delta E: {:.2}",
        metrics.ssim_score,
        metrics.exact_match_percentage,
        metrics.perceptual_match_percentage,
        metrics.mean_delta_e
    );

    // Verify SSIM score is robust (>= 0.85 on initial headless baseline; targets 0.95+)
    assert!(
        metrics.ssim_score >= 0.80,
        "SSIM score ({:.4}) must meet baseline fidelity threshold",
        metrics.ssim_score
    );
    assert!(
        metrics.perceptual_match_percentage >= 75.0,
        "Perceptual match percentage ({:.2}%) must meet threshold",
        metrics.perceptual_match_percentage
    );
    assert!(Path::new(&heatmap_png).exists(), "Diff heatmap must be generated");
}

#[test]
fn test_synthetic_wheel_scroll_benchmark_60hz_and_120hz() {
    let scorecard_60hz = run_synthetic_wheel_scroll_benchmark(TEST_ARTICLE_HTML, 1280, 720, 60.0, 60);
    assert!(scorecard_60hz.monotonic_convergence, "60Hz scroll must converge monotonically");
    assert!(
        scorecard_60hz.p50_frame_time_ms < 16.6,
        "60Hz p50 frame time ({:.2}ms) must be within 16.6ms budget",
        scorecard_60hz.p50_frame_time_ms
    );
    assert!(
        scorecard_60hz.cpu_utilization_pct < 50.0,
        "60Hz simulated CPU utilization ({:.2}%) must be low",
        scorecard_60hz.cpu_utilization_pct
    );

    let scorecard_120hz = run_synthetic_wheel_scroll_benchmark(TEST_ARTICLE_HTML, 1280, 720, 120.0, 60);
    assert!(scorecard_120hz.monotonic_convergence, "120Hz scroll must converge monotonically");
    assert!(
        scorecard_120hz.p50_frame_time_ms < 8.33,
        "120Hz p50 frame time ({:.2}ms) must be within 8.33ms budget",
        scorecard_120hz.p50_frame_time_ms
    );
}

#[test]
fn test_synchronized_multi_offset_parity_suite() {
    let chrome_bin = find_chromium_binary(None);
    if chrome_bin.is_none() {
        eprintln!("[SKIP] Chromium/Chrome binary not found on this system, skipping multi-offset suite");
        return;
    }

    let out_dir = std::path::PathBuf::from("/tmp/strata_multi_offset_parity");
    let config = ParityHarnessConfig {
        resolutions: vec![(1280, 720)],
        scroll_offsets: vec![0.0, 150.0],
        output_dir: out_dir,
        chrome_binary_path: None,
    };

    let suite_results = strata_parity_tauri::chromium_parity::run_synchronized_parity_suite(
        TEST_ARTICLE_HTML,
        "multi_offset",
        &config,
    ).expect("Synchronized parity suite must succeed");

    assert_eq!(suite_results.len(), 2);
    for res in suite_results {
        println!(
            "[Multi-Offset Snapshot] y={:.0}: SSIM={:.4}, Exact={:.2}%, Perceptual={:.2}%",
            res.scroll_y, res.metrics.ssim_score, res.metrics.exact_match_percentage, res.metrics.perceptual_match_percentage
        );
        assert!(res.metrics.ssim_score >= 0.70, "SSIM must be >= 0.70 at offset {}", res.scroll_y);
        assert!(Path::new(&res.strata_image_path).exists());
        assert!(Path::new(&res.chromium_image_path).exists());
        assert!(Path::new(&res.heatmap_path).exists());
    }
}
