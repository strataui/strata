//! Chromium Headless vs Strata Maximized Viewport Parity Harness & SSIM Suite.
//!
//! Renders the isolated web surface maximized at standard resolutions (1920x1080, 1440x900, 1280x720)
//! across multiple vertical scroll positions (y = 0, y = 1000, y = 2500, y = 5000), captures synchronized
//! snapshots against headless Chromium (Blink LayoutNG), computes SSIM parity and perceptual diff heatmaps,
//! and benchmarks kinetic wheel scrolling at 60Hz and 120Hz.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use strata::prelude::*;
use strata_dom::{Attribute, NodeBlueprint, Tag};
use strata_web_reader::{ReaderTheme, WebTranspiler};

use crate::diff::{compare_screenshots, ParityMetrics};

/// Configuration for the maximized viewport parity harness.
#[derive(Debug, Clone)]
pub struct ParityHarnessConfig {
    pub resolutions: Vec<(u32, u32)>,
    pub scroll_offsets: Vec<f32>,
    pub output_dir: PathBuf,
    pub chrome_binary_path: Option<String>,
}

impl Default for ParityHarnessConfig {
    fn default() -> Self {
        Self {
            resolutions: vec![
                (1920, 1080), // Standard 1080p Desktop Maximized
                (1440, 900),  // Standard Laptop Display
                (1280, 720),  // Compact Widescreen Viewport
            ],
            scroll_offsets: vec![
                0.0,    // Top of page (Lead Section / Above Fold)
                1000.0, // Section 1-2 (TOC & Early Sections)
                2500.0, // Mid-page (Syntax & Complex Tables)
                5000.0, // Deep-scroll (Large Code Blocks & References)
            ],
            output_dir: PathBuf::from("/home/usr/.gemini/antigravity/brain/06b8b22c-7cfc-4e3e-a49d-ac2dcee1cc71/parity_artifacts"),
            chrome_binary_path: None,
        }
    }
}

/// Result of a single synchronized snapshot comparison.
#[derive(Debug, Clone)]
pub struct ParitySnapshotResult {
    pub width: u32,
    pub height: u32,
    pub scroll_y: f32,
    pub strata_image_path: String,
    pub chromium_image_path: String,
    pub heatmap_path: String,
    pub metrics: ParityMetrics,
}

/// Telemetry metrics captured during synthetic scrolling benchmarks.
#[derive(Debug, Clone)]
pub struct ScrollTelemetryScorecard {
    pub target_hz: f32,
    pub total_frames: usize,
    pub dropped_frames: usize,
    pub p50_frame_time_ms: f32,
    pub p95_frame_time_ms: f32,
    pub p99_frame_time_ms: f32,
    pub max_frame_time_ms: f32,
    pub cpu_utilization_pct: f32,
    pub monotonic_convergence: bool,
}

/// Locates the Google Chrome or Chromium executable on the current system.
pub fn find_chromium_binary(override_path: Option<&str>) -> Option<String> {
    if let Some(path) = override_path {
        if Path::new(path).exists() {
            return Some(path.to_string());
        }
    }

    let candidates = [
        "/home/usr/.local/bin/google-chrome",
        "/usr/bin/google-chrome",
        "/usr/bin/google-chrome-stable",
        "/usr/bin/chromium",
        "/usr/bin/chromium-browser",
        "/snap/bin/chromium",
    ];

    for candidate in candidates {
        if Path::new(candidate).exists() {
            return Some(candidate.to_string());
        }
    }

    None
}

/// Renders the inner web viewport component maximized in Strata at the specified resolution and scroll position.
pub fn render_strata_web_viewport(
    html: &str,
    width: u32,
    height: u32,
    scroll_y: f32,
    output_png: &str,
) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    strata_platform::init_pango_text_measurement();

    let theme = ReaderTheme::dark();
    let nodes = strata_web_reader::HtmlParser::parse(html);
    let media_ctx = strata_style::MediaContext {
        viewport_width: width as f32,
        viewport_height: height as f32,
        is_dark: true,
    };
    let transpiler = WebTranspiler::new(&theme);
    let content_blueprint = transpiler.transpile_full_document(&nodes, &media_ctx);

    // Encapsulate inside a maximized scrollable viewport container
    let container = NodeBlueprint::element(
        Tag::Div,
        vec![
            "w-full h-full block overflow-y-auto".to_string(),
        ],
        vec![
            Attribute::property("id", "browser-viewport-maximized"),
            Attribute::property("role", "main"),
        ],
        vec![content_blueprint],
    );

    let mut runtime = AppRuntime::new(View { root: container }, width as f32, height as f32, 1.0);
    runtime.layout_root.compute_layout(width as f32, height as f32);
    runtime.tick_frame();

    if scroll_y > 0.0 {
        let center_x = width as f32 / 2.0;
        let center_y = height as f32 / 2.0;
        runtime.dispatch_scroll(center_x, center_y, 0.0, scroll_y);
        runtime.tick_frame();
    }

    #[cfg(target_os = "linux")]
    {
        strata_platform::render_to_png(&runtime.display_list, width as i32, height as i32, output_png)?;
    }

    #[cfg(not(target_os = "linux"))]
    {
        let mut fb = strata_render::rasterizer::FrameBuffer::new(width as usize, height as usize);
        fb.clear(strata_style::types::Color::rgba(9, 9, 11, 255));
        fb.execute_display_list(&runtime.display_list);
        let img: image::RgbaImage = image::ImageBuffer::from_raw(width, height, fb.pixels)
            .ok_or_else(|| "Failed to create image buffer".to_string())?;
        img.save(output_png).map_err(|e| format!("Failed to save PNG: {}", e))?;
    }

    Ok(())
}

/// Renders the HTML document in headless Chromium at the specified resolution and scroll position.
pub fn render_chromium_headless(
    html: &str,
    width: u32,
    height: u32,
    scroll_y: f32,
    output_png: &str,
    chrome_binary_override: Option<&str>,
) -> Result<(), String> {
    let chrome_bin = find_chromium_binary(chrome_binary_override)
        .ok_or_else(|| "No Google Chrome or Chromium executable found on system".to_string())?;

    // Injected wrapper ensuring dark theme and exact scroll position
    let mut modified_html = html.to_string();
    let scroll_script = format!(
        r#"<script>
            document.addEventListener('DOMContentLoaded', function() {{
                window.scrollTo(0, {scroll_y});
                setTimeout(function() {{ window.scrollTo(0, {scroll_y}); }}, 50);
            }});
        </script>"#
    );

    if let Some(pos) = modified_html.rfind("</body>") {
        modified_html.insert_str(pos, &scroll_script);
    } else {
        modified_html.push_str(&scroll_script);
    }

    let temp_html_path = format!("/tmp/chromium_parity_target_{width}x{height}_{scroll_y}.html");
    fs::write(&temp_html_path, &modified_html)
        .map_err(|e| format!("Failed to write temporary HTML for Chromium: {}", e))?;

    let file_url = format!("file://{}", temp_html_path);

    let status = Command::new(&chrome_bin)
        .args([
            "--headless=new",
            "--disable-gpu",
            "--no-sandbox",
            "--force-color-profile=srgb",
            "--hide-scrollbars",
            &format!("--window-size={},{}", width, height),
            &format!("--screenshot={}", output_png),
            &file_url,
        ])
        .status()
        .map_err(|e| format!("Failed to execute Chromium: {}", e))?;

    let _ = fs::remove_file(&temp_html_path);

    if !status.success() {
        return Err(format!("Chromium exited with non-zero status: {:?}", status.code()));
    }

    if !Path::new(output_png).exists() {
        return Err(format!("Chromium did not generate output file at {}", output_png));
    }

    Ok(())
}

/// Executes the synchronized parity comparison suite across resolutions and scroll offsets.
pub fn run_synchronized_parity_suite(
    html: &str,
    label: &str,
    config: &ParityHarnessConfig,
) -> Result<Vec<ParitySnapshotResult>, String> {
    fs::create_dir_all(&config.output_dir)
        .map_err(|e| format!("Failed to create output directory: {}", e))?;

    let mut results = Vec::new();

    for &(width, height) in &config.resolutions {
        for &scroll_y in &config.scroll_offsets {
            let tag = format!("{label}_{width}x{height}_y{scroll_y:.0}");
            let strata_path = config.output_dir.join(format!("{tag}_strata.png"));
            let chromium_path = config.output_dir.join(format!("{tag}_chromium.png"));
            let heatmap_path = config.output_dir.join(format!("{tag}_diff_heatmap.png"));

            let strata_str = strata_path.to_str().unwrap().to_string();
            let chromium_str = chromium_path.to_str().unwrap().to_string();
            let heatmap_str = heatmap_path.to_str().unwrap().to_string();

            // Render Strata Maximized Viewport
            render_strata_web_viewport(html, width, height, scroll_y, &strata_str)?;

            // Render Headless Chromium
            render_chromium_headless(
                html,
                width,
                height,
                scroll_y,
                &chromium_str,
                config.chrome_binary_path.as_deref(),
            )?;

            // Perform SSIM and Perceptual Color Differential Analysis
            let metrics = compare_screenshots(
                &chromium_path,
                &strata_path,
                &heatmap_path,
            )?;

            results.push(ParitySnapshotResult {
                width,
                height,
                scroll_y,
                strata_image_path: strata_str,
                chromium_image_path: chromium_str,
                heatmap_path: heatmap_str,
                metrics,
            });
        }
    }

    Ok(results)
}

/// Simulates synthetic wheel scroll streams at 60Hz and 120Hz to benchmark scroll performance under load.
pub fn run_synthetic_wheel_scroll_benchmark(
    html: &str,
    width: u32,
    height: u32,
    target_hz: f32,
    total_frames: usize,
) -> ScrollTelemetryScorecard {
    #[cfg(target_os = "linux")]
    strata_platform::init_pango_text_measurement();

    let theme = ReaderTheme::dark();
    let nodes = strata_web_reader::HtmlParser::parse(html);
    let media_ctx = strata_style::MediaContext {
        viewport_width: width as f32,
        viewport_height: height as f32,
        is_dark: true,
    };
    let transpiler = WebTranspiler::new(&theme);
    let content_blueprint = transpiler.transpile_full_document(&nodes, &media_ctx);
    let container = NodeBlueprint::element(
        Tag::Div,
        vec![
            "w-full h-full block overflow-y-auto".to_string(),
        ],
        vec![
            Attribute::property("id", "browser-viewport-maximized"),
            Attribute::property("role", "main"),
        ],
        vec![content_blueprint],
    );

    let mut runtime = AppRuntime::new(View { root: container }, width as f32, height as f32, 1.0);
    runtime.layout_root.compute_layout(width as f32, height as f32);
    runtime.tick_frame();

    let target_frame_budget_ms = 1000.0 / target_hz;
    let mut frame_times_ms = Vec::with_capacity(total_frames);
    let mut dropped_frames = 0;
    let mut previous_scroll_y = 0.0f32;
    let mut monotonic_convergence = true;

    let center_x = width as f32 / 2.0;
    let center_y = height as f32 / 2.0;

    for i in 0..total_frames {
        let t_start = Instant::now();

        // Simulate periodic wheel notches every 4 frames
        if i % 4 == 0 {
            runtime.dispatch_scroll(center_x, center_y, 0.0, 110.0);
        }

        // Advance smooth scroll physics & tick frame
        runtime.tick_frame();

        let elapsed_ms = t_start.elapsed().as_secs_f32() * 1000.0;
        frame_times_ms.push(elapsed_ms);

        if elapsed_ms > target_frame_budget_ms {
            dropped_frames += 1;
        }

        if let Some(bar) = runtime.active_scrollbars.iter().find(|b| b.id == "browser-viewport-maximized") {
            if bar.scroll_y < previous_scroll_y {
                // Monotonic downward progression check
                monotonic_convergence = false;
            }
            previous_scroll_y = bar.scroll_y;
        }
    }

    frame_times_ms.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let p50 = frame_times_ms[(total_frames as f32 * 0.50) as usize];
    let p95 = frame_times_ms[(total_frames as f32 * 0.95) as usize];
    let p99 = frame_times_ms[(total_frames as f32 * 0.99) as usize];
    let max = *frame_times_ms.last().unwrap_or(&0.0);

    // CPU utilization proxy based on active time vs frame budget
    let total_active_time: f32 = frame_times_ms.iter().sum();
    let total_budget_time: f32 = target_frame_budget_ms * total_frames as f32;
    let cpu_utilization_pct = (total_active_time / total_budget_time) * 100.0;

    ScrollTelemetryScorecard {
        target_hz,
        total_frames,
        dropped_frames,
        p50_frame_time_ms: p50,
        p95_frame_time_ms: p95,
        p99_frame_time_ms: p99,
        max_frame_time_ms: max,
        cpu_utilization_pct,
        monotonic_convergence,
    }
}
