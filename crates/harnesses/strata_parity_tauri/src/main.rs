//! Strata three-engine visual parity harness.
//!
//! Chromium is the visual ground truth. Tauri on Linux is WebKitGTK, not
//! Chromium, and the Strata renderer is the native engine under test.

use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

use strata_parity_tauri::fixture::ParityLevel;
use strata_parity_tauri::{
    chromium_parity, components, diff, parity_suite, raw_bench, strata_runner, tauri_runner,
};

fn get_level_from_args(args: &[String]) -> ParityLevel {
    if let Some(pos) = args.iter().position(|a| a == "--level") {
        if let Some(val) = args.get(pos + 1) {
            if let Ok(num) = val.parse::<u32>() {
                return ParityLevel::from_u32(num);
            }
        }
    }
    ParityLevel::Level1BoxModel
}

fn level_index(level: ParityLevel) -> u32 {
    match level {
        ParityLevel::Level1BoxModel => 1,
        ParityLevel::Level2RoundedCorner => 2,
        ParityLevel::Level3Typography => 3,
        ParityLevel::Level4FlexRow => 4,
        ParityLevel::Level5FullDashboard => 5,
        ParityLevel::Level6CssGrid2D => 6,
        ParityLevel::Level7TelemetryConsole => 7,
    }
}

fn required_arg(args: &[String], index: usize, flag: &str) -> String {
    match args.get(index) {
        Some(value) => value.clone(),
        None => {
            eprintln!("[ERROR] {} requires more arguments", flag);
            std::process::exit(2);
        }
    }
}

fn required_u32(args: &[String], index: usize, flag: &str) -> u32 {
    let value = required_arg(args, index, flag);
    match value.parse::<u32>() {
        Ok(number) => number,
        Err(error) => {
            eprintln!(
                "[ERROR] {} expected an integer, got '{}': {}",
                flag, value, error
            );
            std::process::exit(2);
        }
    }
}

fn required_f32(args: &[String], index: usize, flag: &str) -> f32 {
    let value = required_arg(args, index, flag);
    match value.parse::<f32>() {
        Ok(number) => number,
        Err(error) => {
            eprintln!(
                "[ERROR] {} expected a number, got '{}': {}",
                flag, value, error
            );
            std::process::exit(2);
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if let Some(position) = args.iter().position(|arg| arg == "--capture-strata") {
        let html_path = required_arg(&args, position + 1, "--capture-strata");
        let output_png = required_arg(&args, position + 2, "--capture-strata");
        let width = required_u32(&args, position + 3, "--capture-strata");
        let height = required_u32(&args, position + 4, "--capture-strata");
        let scroll_y = required_f32(&args, position + 5, "--capture-strata");
        let html = match fs::read_to_string(&html_path) {
            Ok(html) => html,
            Err(error) => {
                eprintln!("[ERROR] Failed to read {}: {}", html_path, error);
                std::process::exit(1);
            }
        };
        match chromium_parity::render_strata_web_viewport(
            &html,
            width,
            height,
            scroll_y,
            &output_png,
        ) {
            Ok(()) => return,
            Err(error) => {
                eprintln!("[ERROR] Strata capture failed: {}", error);
                std::process::exit(1);
            }
        }
    }

    if let Some(position) = args.iter().position(|arg| arg == "--capture-tauri") {
        let html_path = required_arg(&args, position + 1, "--capture-tauri");
        let output_png = required_arg(&args, position + 2, "--capture-tauri");
        let width = required_u32(&args, position + 3, "--capture-tauri");
        let height = required_u32(&args, position + 4, "--capture-tauri");
        let title = args
            .get(position + 5)
            .cloned()
            .unwrap_or_else(|| "Strata Parity Tauri".to_string());
        let captured =
            tauri_runner::run_tauri_capture(&html_path, &output_png, width, height, &title);
        if !captured {
            std::process::exit(1);
        }
        return;
    }

    if args.iter().any(|arg| arg == "--list-fixtures") {
        println!(
            "{:<24} {:<9} {:<7} {:<9} {:<8} Description",
            "Name", "In scope", "SSIM", "Perceptual", "Scroll"
        );
        println!("----------------------------------------------------------------------------------------------------");
        for fixture in components::suite_fixtures() {
            println!(
                "{:<24} {:<9} {:<7.4} {:<9.2} {:<8.1} {}",
                fixture.name,
                if fixture.in_scope { "yes" } else { "no" },
                fixture.ssim_threshold,
                fixture.perceptual_threshold,
                fixture.scroll_y,
                fixture.description
            );
        }
        return;
    }

    let wants_suite = args.iter().any(|arg| arg == "--parity-suite")
        || args
            .iter()
            .any(|arg| arg == "--fixture" || arg.starts_with("--fixture="));
    if wants_suite {
        match parity_suite::run_from_args(&args) {
            Ok(true) => return,
            Ok(false) => std::process::exit(1),
            Err(error) => {
                eprintln!("[ERROR] {}", error);
                std::process::exit(2);
            }
        }
    }

    let level = get_level_from_args(&args);
    let level_num = level_index(level);

    if args.contains(&"--bench-raw".to_string()) {
        raw_bench::run_raw_benchmarks();
        return;
    }

    if args.contains(&"--bench-damage".to_string()) {
        raw_bench::run_damage_benchmarks();
        return;
    }

    if args.contains(&"--chromium-parity".to_string()) {
        println!("=============================================================");
        println!("  Strata vs Headless Chromium Maximized Viewport Parity      ");
        println!("=============================================================");
        let config = chromium_parity::ParityHarnessConfig::default();
        let sample_html = r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>
  * { box-sizing: border-box; margin: 0; padding: 0; }
  body { background: #09090b; color: #f4f4f5; font-family: sans-serif; padding: 48px; }
  h1 { font-size: 32px; font-weight: 700; margin-bottom: 16px; border-bottom: 2px solid #27272a; padding-bottom: 8px; }
  p { font-size: 16px; line-height: 1.6; margin-bottom: 24px; color: #a1a1aa; }
  table { width: 100%; border-collapse: collapse; margin-bottom: 32px; }
  th, td { border: 1px solid #27272a; padding: 12px 16px; text-align: left; }
  th { background: #18181b; font-weight: 600; }
  .card { background: #18181b; border: 1px solid #27272a; border-radius: 8px; padding: 24px; margin-bottom: 24px; }
</style>
</head>
<body>
  <div class="card">
    <h1>Strata Maximized Web Viewport Engine</h1>
    <p>This benchmark measures visual and layout parity between Strata pure-Rust layout engine and Chromium Blink LayoutNG.</p>
    <table>
      <thead>
        <tr><th>Metric</th><th>Target</th><th>Engine</th></tr>
      </thead>
      <tbody>
        <tr><td>SSIM Score</td><td>&ge; 0.95</td><td>Strata vs Blink</td></tr>
        <tr><td>Rendering Latency</td><td>&le; 16.6ms (60 FPS)</td><td>Zero-Chromium Native</td></tr>
        <tr><td>Kinetic Scrolling</td><td>Critically Damped Harmonic</td><td>Smooth Wheel Physics</td></tr>
      </tbody>
    </table>
  </div>
</body>
</html>"#;
        match chromium_parity::run_synchronized_parity_suite(
            sample_html,
            "viewport_parity",
            &config,
        ) {
            Ok(results) => {
                for r in results {
                    println!("[Parity Result] {}x{} (y={:.0}): SSIM = {:.4}, Exact = {:.2}%, Perceptual = {:.2}%, Mean Delta E = {:.2}",
                        r.width, r.height, r.scroll_y,
                        r.metrics.ssim_score,
                        r.metrics.exact_match_percentage,
                        r.metrics.perceptual_match_percentage,
                        r.metrics.mean_delta_e
                    );
                }
            }
            Err(e) => eprintln!("[!] Parity suite failed: {}", e),
        }
        return;
    }

    if args.contains(&"--scrolling-bench".to_string()) {
        println!("=============================================================");
        println!("  Strata Kinetic Wheel Scrolling & Telemetry Benchmark       ");
        println!("=============================================================");
        let sample_html =
            "<div style='height: 10000px;'><h1 style='padding: 24px;'>Long Document</h1></div>";
        let scorecard_60hz = chromium_parity::run_synthetic_wheel_scroll_benchmark(
            sample_html,
            1920,
            1080,
            60.0,
            120,
        );
        let scorecard_120hz = chromium_parity::run_synthetic_wheel_scroll_benchmark(
            sample_html,
            1920,
            1080,
            120.0,
            120,
        );
        println!("[60Hz Telemetry] p50: {:.2}ms, p95: {:.2}ms, p99: {:.2}ms, Dropped: {}, Monotonic: {}, CPU: {:.2}%",
            scorecard_60hz.p50_frame_time_ms, scorecard_60hz.p95_frame_time_ms, scorecard_60hz.p99_frame_time_ms,
            scorecard_60hz.dropped_frames, scorecard_60hz.monotonic_convergence, scorecard_60hz.cpu_utilization_pct);
        println!("[120Hz Telemetry] p50: {:.2}ms, p95: {:.2}ms, p99: {:.2}ms, Dropped: {}, Monotonic: {}, CPU: {:.2}%",
            scorecard_120hz.p50_frame_time_ms, scorecard_120hz.p95_frame_time_ms, scorecard_120hz.p99_frame_time_ms,
            scorecard_120hz.dropped_frames, scorecard_120hz.monotonic_convergence, scorecard_120hz.cpu_utilization_pct);
        return;
    }

    if args.contains(&"--tauri-only".to_string()) {
        let html_path = format!("/tmp/parity_tauri_fixture_l{}.html", level_num);
        let tauri_shot = format!("/tmp/tauri_rendered_l{}.png", level_num);
        println!(
            "[Execution] Starting Tauri 2 WebKitGTK window for Level {}...",
            level_num
        );
        tauri_runner::run_tauri_window(level, &html_path, &tauri_shot);
        return;
    }

    if args.contains(&"--strata-only".to_string()) {
        let strata_shot = format!("/tmp/strata_rendered_l{}.png", level_num);
        let strata_fb = format!("/tmp/strata_framebuffer_l{}.png", level_num);
        println!(
            "[Execution] Starting Strata Native GTK window for Level {}...",
            level_num
        );
        strata_runner::run_strata_window(level, &strata_shot);
        strata_runner::render_strata_software_framebuffer(level, 1920, 1080, &strata_fb);
        return;
    }

    if args.contains(&"--diff-only".to_string()) {
        let tauri_shot = format!("/tmp/tauri_rendered_l{}.png", level_num);
        let strata_shot = format!("/tmp/strata_rendered_l{}.png", level_num);
        let heatmap_path = format!("/tmp/parity_diff_heatmap_l{}.png", level_num);
        run_diff_analysis_for_level(level_num, &tauri_shot, &strata_shot, &heatmap_path);
        return;
    }

    if args.contains(&"--suite".to_string()) {
        println!("=============================================================");
        println!("  Strata vs Tauri 2 Progressive Parity Benchmark Suite       ");
        println!("=============================================================");

        let levels = [
            ParityLevel::Level1BoxModel,
            ParityLevel::Level2RoundedCorner,
            ParityLevel::Level3Typography,
            ParityLevel::Level4FlexRow,
            ParityLevel::Level5FullDashboard,
            ParityLevel::Level6CssGrid2D,
            ParityLevel::Level7TelemetryConsole,
        ];

        let mut results = Vec::new();

        for l in levels {
            if let Some(metrics) = run_parity_benchmark(l) {
                results.push((l, metrics));
            }
        }

        println!("\n=========================================================================================");
        println!("  PROGRESSIVE VISUAL PARITY SUITE SUMMARY REPORT");
        println!("=========================================================================================");
        println!(
            "{:<8} | {:<28} | {:<12} | {:<14} | {:<14} | {:<10}",
            "Level", "Stage", "Resolution", "Exact Match", "Perceptual", "SSIM"
        );
        println!("-----------------------------------------------------------------------------------------");
        for (l, m) in &results {
            println!(
                "{:<8} | {:<28} | {:<5}x{:<6} | {:>6.2}%        | {:>6.2}%        | {:>8.5}",
                format!("L{}", level_index(*l)),
                l.title().split(':').nth(1).unwrap_or(l.title()).trim(),
                m.width,
                m.height,
                m.exact_match_percentage,
                m.perceptual_match_percentage,
                m.ssim_score,
            );
        }
        println!("=========================================================================================");
        return;
    }

    // Single level execution (defaults to Level 1 if unspecified)
    run_parity_benchmark(level);
}

fn run_parity_benchmark(level: ParityLevel) -> Option<diff::ParityMetrics> {
    let level_num = level_index(level);

    println!("\n=============================================================");
    println!("  Parity Benchmark - {}", level.title());
    println!("=============================================================");

    let current_exe = env::current_exe().expect("Current executable path");

    // Stage 1: Launch Tauri window in isolated process
    println!("\n[Stage 1/3] Executing Tauri 2 (WebKitGTK) window maximized...");
    let tauri_status = Command::new(&current_exe)
        .arg("--tauri-only")
        .arg("--level")
        .arg(level_num.to_string())
        .status()
        .expect("Failed to execute Tauri child process");

    if !tauri_status.success() {
        eprintln!(
            "[!] Tauri process failed with exit code: {:?}",
            tauri_status.code()
        );
    }

    // Stage 2: Launch Strata window in isolated process
    println!("\n[Stage 2/3] Executing Strata Native window maximized...");
    let strata_status = Command::new(&current_exe)
        .arg("--strata-only")
        .arg("--level")
        .arg(level_num.to_string())
        .status()
        .expect("Failed to execute Strata child process");

    if !strata_status.success() {
        eprintln!(
            "[!] Strata process failed with exit code: {:?}",
            strata_status.code()
        );
    }

    // Stage 3: Differential Analysis
    println!("\n[Stage 3/3] Performing Differential Pixel & Perceptual SSIM Analysis...");
    let tauri_shot = format!("/tmp/tauri_rendered_l{}.png", level_num);
    let strata_shot = format!("/tmp/strata_rendered_l{}.png", level_num);
    let heatmap_path = format!("/tmp/parity_diff_heatmap_l{}.png", level_num);

    run_diff_analysis_for_level(level_num, &tauri_shot, &strata_shot, &heatmap_path)
}

fn run_diff_analysis_for_level(
    level_num: u32,
    tauri_shot: &str,
    strata_shot: &str,
    heatmap_path: &str,
) -> Option<diff::ParityMetrics> {
    let path_tauri = Path::new(tauri_shot);
    let path_strata = Path::new(strata_shot);
    let path_heatmap = Path::new(heatmap_path);

    if !path_tauri.exists() {
        eprintln!("[!] Tauri screenshot not found at {}", path_tauri.display());
        return None;
    }
    if !path_strata.exists() {
        eprintln!(
            "[!] Strata screenshot not found at {}",
            path_strata.display()
        );
        return None;
    }

    match diff::compare_screenshots(path_tauri, path_strata, path_heatmap) {
        Ok(metrics) => {
            println!("-------------------------------------------------------------");
            println!("  PARITY BENCHMARK REPORT - Level {}", level_num);
            println!("-------------------------------------------------------------");
            println!(
                "  Resolution:             {}x{} ({} pixels)",
                metrics.width, metrics.height, metrics.total_pixels
            );
            println!(
                "  Exact Match Pixels:     {} ({:.2}%)",
                metrics.exact_match_pixels, metrics.exact_match_percentage
            );
            println!(
                "  Perceptual Match:       {} ({:.2}%)",
                metrics.perceptual_match_pixels, metrics.perceptual_match_percentage
            );
            println!("  SSIM Similarity Score:  {:.5}", metrics.ssim_score);
            println!("  Mean Delta E (Color):   {:.4}", metrics.mean_delta_e);
            println!("  Max Delta E (Color):    {:.4}", metrics.max_delta_e);
            println!("  Diff Heatmap Artifact:  {}", metrics.heatmap_path);
            println!("-------------------------------------------------------------");
            Some(metrics)
        }
        Err(e) => {
            eprintln!("[!] Comparison failed: {}", e);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use strata_parity_tauri::fixture;

    #[test]
    fn test_level5_layout_geometry_parity() {
        let view = fixture::get_fixture_view(fixture::ParityLevel::Level5FullDashboard);
        let mut layout_root = strata::AppRuntime::build_layout_tree(&view.root);
        layout_root.compute_layout(1920.0, 993.0);

        // Header node
        let header = &layout_root.children[0];
        assert_eq!(header.geometry.border_box.y, 32.0);
        assert_eq!(header.geometry.border_box.height, 49.0);

        // Grid node
        let grid = &layout_root.children[1];
        assert_eq!(grid.geometry.border_box.y, 113.0);

        // Card 1
        let card1 = &grid.children[0];
        assert_eq!(card1.geometry.border_box.y, 113.0);
        assert_eq!(card1.geometry.border_box.height, 134.0);

        // Metric value "1,024"
        let val1 = &card1.children[1];
        assert_eq!(val1.geometry.border_box.y, 166.0);
        assert_eq!(val1.geometry.border_box.height, 36.0);
    }

    #[test]
    fn test_strata_raw_benchmarks() {
        let timing = strata_parity_tauri::raw_bench::measure_strata_level(
            fixture::ParityLevel::Level1BoxModel,
            5,
        );
        assert!(timing.total_us > 0);
    }
}
