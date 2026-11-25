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

use crate::diff::{compare_screenshots, ParityMetrics};

/// Self-contained structural HTML parser for parity fixtures (zero external HTML-crate deps).
///
/// Preserves `<style>` blocks, standard HTML tags (`div`, `span`, `h1`..`h6`, `p`, `table`,
/// `thead`, `tbody`, `tr`, `th`, `td`, `ul`, `ol`, `li`, `a`, `section`, `header`, `main`,
/// `footer`, `nav`, `aside`), `class` / `id` / `style` attributes, and common HTML entities so
/// `AppRuntime`'s built-in CSS stylesheet cascade matches Chromium LayoutNG faithfully.
pub fn html_fixture_to_blueprint(html: &str) -> NodeBlueprint {
    fn decode_entities(s: &str) -> String {
        s.replace("&ge;", "≥")
            .replace("&le;", "≤")
            .replace("&amp;", "&")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&#39;", "'")
            .replace("&nbsp;", " ")
    }

    fn tag_from_name(name: &str) -> Tag {
        match name {
            "div" => Tag::Div,
            "span" => Tag::Span,
            "button" => Tag::Button,
            "input" => Tag::Input,
            "select" => Tag::Select,
            "option" => Tag::Option,
            "textarea" => Tag::Textarea,
            "label" => Tag::Label,
            "form" => Tag::Form,
            "header" => Tag::Header,
            "nav" => Tag::Nav,
            "main" => Tag::Main,
            "section" => Tag::Section,
            "aside" => Tag::Aside,
            "footer" => Tag::Footer,
            "h1" => Tag::H1,
            "h2" => Tag::H2,
            "h3" => Tag::H3,
            "h4" => Tag::H4,
            "h5" => Tag::H5,
            "h6" => Tag::H6,
            "p" => Tag::P,
            "ul" => Tag::Ul,
            "ol" => Tag::Ol,
            "li" => Tag::Li,
            "a" => Tag::A,
            "img" => Tag::Img,
            "table" => Tag::Table,
            "thead" => Tag::Thead,
            "tbody" => Tag::Tbody,
            "tr" => Tag::Tr,
            "th" => Tag::Th,
            "td" => Tag::Td,
            "hr" => Tag::Hr,
            "br" => Tag::Br,
            "body" => Tag::Custom("body"),
            _ => Tag::Div,
        }
    }

    fn parse_attrs(raw: &str) -> (Vec<String>, Vec<Attribute>) {
        let mut classes = Vec::new();
        let mut attrs = Vec::new();
        let bytes = raw.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            let key_start = i;
            while i < bytes.len()
                && !bytes[i].is_ascii_whitespace()
                && bytes[i] != b'='
                && bytes[i] != b'/'
            {
                i += 1;
            }
            if i == key_start {
                i += 1;
                continue;
            }
            let key = raw[key_start..i].to_ascii_lowercase();
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            let mut val = String::new();
            if i < bytes.len() && bytes[i] == b'=' {
                i += 1;
                while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
                if i < bytes.len() && (bytes[i] == b'"' || bytes[i] == b'\'') {
                    let q = bytes[i];
                    i += 1;
                    let vs = i;
                    while i < bytes.len() && bytes[i] != q {
                        i += 1;
                    }
                    val = decode_entities(&raw[vs..i]);
                    if i < bytes.len() {
                        i += 1;
                    }
                } else {
                    let vs = i;
                    while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                        i += 1;
                    }
                    val = decode_entities(&raw[vs..i]);
                }
            }
            if key == "class" {
                classes.extend(val.split_whitespace().map(str::to_string));
            } else {
                attrs.push(Attribute::property(key, val));
            }
        }
        (classes, attrs)
    }

    // Collect all <style> blocks from the document
    let mut style_nodes = Vec::new();
    let mut rest = html;
    while let Some(s) = rest.find("<style") {
        let after_open = &rest[s..];
        let Some(gt) = after_open.find('>') else { break };
        let content_start = &after_open[gt + 1..];
        let Some(end) = content_start.find("</style>") else {
            break;
        };
        let css = content_start[..end].trim().to_string();
        if !css.is_empty() {
            style_nodes.push(NodeBlueprint::element(
                Tag::Custom("style"),
                vec![],
                vec![],
                vec![NodeBlueprint::Text(css)],
            ));
        }
        rest = &content_start[end + 8..];
    }

    // Extract <body> attributes and inner HTML
    let (body_classes, body_attrs, body_inner) = if let Some(b) = html.find("<body") {
        let after = &html[b + 5..];
        let gt = after.find('>').unwrap_or(0);
        let (bc, ba) = parse_attrs(&after[..gt]);
        let inner_start = &after[gt + 1..];
        let end = inner_start.find("</body>").unwrap_or(inner_start.len());
        (bc, ba, &inner_start[..end])
    } else {
        (Vec::new(), Vec::new(), html)
    };

    // Stack-based HTML tree builder
    struct Frame {
        tag_name: String,
        tag: Tag,
        classes: Vec<String>,
        attrs: Vec<Attribute>,
        children: Vec<NodeBlueprint>,
    }

    let mut stack: Vec<Frame> = vec![Frame {
        tag_name: "body".to_string(),
        tag: Tag::Custom("body"),
        classes: body_classes,
        attrs: body_attrs,
        children: style_nodes,
    }];

    let bytes = body_inner.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'<' {
            // Skip HTML comments
            if body_inner[i..].starts_with("<!--") {
                if let Some(end) = body_inner[i + 4..].find("-->") {
                    i += 4 + end + 3;
                    continue;
                }
                break;
            }
            let Some(rel_gt) = body_inner[i..].find('>') else {
                break;
            };
            let raw_tag = body_inner[i + 1..i + rel_gt].trim();
            i += rel_gt + 1;

            if let Some(close_name) = raw_tag.strip_prefix('/') {
                let cname = close_name.trim().to_ascii_lowercase();
                if let Some(pos) = stack.iter().rposition(|f| f.tag_name == cname) {
                    if pos > 0 {
                        while stack.len() > pos {
                            let finished = stack.pop().unwrap();
                            let node = NodeBlueprint::element(
                                finished.tag,
                                finished.classes,
                                finished.attrs,
                                finished.children,
                            );
                            stack.last_mut().unwrap().children.push(node);
                        }
                    }
                }
            } else if !raw_tag.is_empty() && !raw_tag.starts_with('!') {
                let self_closing = raw_tag.ends_with('/');
                let trimmed = raw_tag.trim_end_matches('/').trim();
                let mut parts = trimmed.splitn(2, char::is_whitespace);
                let name = parts.next().unwrap_or("div").to_ascii_lowercase();
                let attr_raw = parts.next().unwrap_or("");
                let (classes, attrs) = parse_attrs(attr_raw);
                let tag = tag_from_name(&name);
                let is_void = self_closing
                    || matches!(
                        name.as_str(),
                        "br" | "hr" | "img" | "input" | "meta" | "link"
                    );
                if is_void {
                    stack.last_mut().unwrap().children.push(NodeBlueprint::element(
                        tag,
                        classes,
                        attrs,
                        vec![],
                    ));
                } else {
                    stack.push(Frame {
                        tag_name: name,
                        tag,
                        classes,
                        attrs,
                        children: Vec::new(),
                    });
                }
            }
        } else {
            let start = i;
            while i < bytes.len() && bytes[i] != b'<' {
                i += 1;
            }
            let raw_text = &body_inner[start..i];
            let collapsed: String = raw_text.split_whitespace().collect::<Vec<_>>().join(" ");
            if !collapsed.is_empty() {
                stack
                    .last_mut()
                    .unwrap()
                    .children
                    .push(NodeBlueprint::Text(decode_entities(&collapsed)));
            }
        }
    }

    while stack.len() > 1 {
        let finished = stack.pop().unwrap();
        let node = NodeBlueprint::element(
            finished.tag,
            finished.classes,
            finished.attrs,
            finished.children,
        );
        stack.last_mut().unwrap().children.push(node);
    }

    let root = stack.pop().unwrap();
    let mut root_classes = root.classes;
    root_classes.push("w-full".to_string());
    root_classes.push("min-h-full".to_string());
    NodeBlueprint::element(root.tag, root_classes, root.attrs, root.children)
}

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
            output_dir: PathBuf::from("target/parity_viewport"),
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

    for variable in ["CHROME_BIN", "CHROMIUM_BIN"] {
        if let Ok(path) = std::env::var(variable) {
            if Path::new(&path).exists() {
                return Some(path);
            }
        }
    }

    let candidates = [
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

    let content_blueprint = html_fixture_to_blueprint(html);

    // Encapsulate inside a maximized scrollable viewport container
    let container = NodeBlueprint::element(
        Tag::Div,
        vec!["w-full h-full block overflow-y-auto".to_string()],
        vec![
            Attribute::property("id", "browser-viewport-maximized"),
            Attribute::property("role", "main"),
        ],
        vec![content_blueprint],
    );

    let mut runtime = AppRuntime::new(View { root: container }, width as f32, height as f32, 1.0);
    runtime
        .layout_root
        .compute_layout(width as f32, height as f32);
    runtime.tick_frame();

    if scroll_y > 0.0 {
        // Apply the offset through the deterministic scroll store instead of
        // the kinetic wheel path, so captures are stable across runs.
        runtime
            .scroll_offsets_by_id
            .insert("browser-viewport-maximized".to_string(), (0.0, scroll_y));
        // Parity fixtures may define a dedicated scroll container instead of
        // scrolling the outer viewport. Unknown ids are ignored by the walk.
        runtime
            .scroll_offsets_by_id
            .insert("parity-scroll-root".to_string(), (0.0, scroll_y));
        runtime.tick_frame();
    }

    #[cfg(target_os = "linux")]
    {
        strata_platform::render_to_png(
            &runtime.display_list,
            width as i32,
            height as i32,
            output_png,
        )?;
    }

    #[cfg(not(target_os = "linux"))]
    {
        let mut fb = strata_render::rasterizer::FrameBuffer::new(width as usize, height as usize);
        fb.clear(strata_style::types::Color::rgba(9, 9, 11, 255));
        fb.execute_display_list(&runtime.display_list);
        let img: image::RgbaImage = image::ImageBuffer::from_raw(width, height, fb.pixels)
            .ok_or_else(|| "Failed to create image buffer".to_string())?;
        img.save(output_png)
            .map_err(|e| format!("Failed to save PNG: {}", e))?;
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
        return Err(format!(
            "Chromium exited with non-zero status: {:?}",
            status.code()
        ));
    }

    if !Path::new(output_png).exists() {
        return Err(format!(
            "Chromium did not generate output file at {}",
            output_png
        ));
    }

    Ok(())
}

/// Renders a final, already-injected HTML document in headless Chromium.
///
/// The caller injects the determinism style, freeze script, and scroll offset.
/// A fresh profile directory is used for every invocation so that no cookie,
/// cache, or font state leaks between runs.
pub fn render_chromium_deterministic(
    html: &str,
    width: u32,
    height: u32,
    output_png: &str,
    work_dir: &Path,
    profile_dir: &Path,
    chrome_binary_override: Option<&str>,
) -> Result<(), String> {
    let chrome_bin = find_chromium_binary(chrome_binary_override)
        .ok_or_else(|| "No Google Chrome or Chromium executable found on system".to_string())?;

    fs::create_dir_all(work_dir)
        .map_err(|e| format!("Failed to create Chromium work directory: {}", e))?;

    if profile_dir.exists() {
        fs::remove_dir_all(profile_dir).map_err(|e| {
            format!(
                "Failed to clear Chromium profile {}: {}",
                profile_dir.display(),
                e
            )
        })?;
    }
    fs::create_dir_all(profile_dir)
        .map_err(|e| format!("Failed to create Chromium profile: {}", e))?;

    let html_path = work_dir.join("chromium_input.html");
    fs::write(&html_path, html)
        .map_err(|e| format!("Failed to write temporary HTML for Chromium: {}", e))?;

    let file_url = format!("file://{}", html_path.display());
    let output = Command::new(&chrome_bin)
        .args([
            "--headless=new",
            "--disable-gpu",
            "--no-sandbox",
            "--force-color-profile=srgb",
            "--force-device-scale-factor=1",
            "--font-render-hinting=none",
            "--disable-lcd-text",
            "--hide-scrollbars",
            "--no-first-run",
            "--disable-extensions",
            "--virtual-time-budget=2000",
        ])
        .arg(format!("--window-size={},{}", width, height))
        .arg(format!("--user-data-dir={}", profile_dir.display()))
        .arg(format!("--screenshot={}", output_png))
        .arg(&file_url)
        .output()
        .map_err(|e| format!("Failed to execute Chromium: {}", e))?;

    let _ = fs::remove_dir_all(profile_dir);

    if !output.status.success() {
        return Err(format!(
            "Chromium exited with status {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    if !Path::new(output_png).exists() {
        return Err(format!(
            "Chromium did not generate output file at {}",
            output_png
        ));
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
            let metrics = compare_screenshots(&chromium_path, &strata_path, &heatmap_path)?;

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

    let content_blueprint = html_fixture_to_blueprint(html);
    let container = NodeBlueprint::element(
        Tag::Div,
        vec!["w-full h-full block overflow-y-auto".to_string()],
        vec![
            Attribute::property("id", "browser-viewport-maximized"),
            Attribute::property("role", "main"),
        ],
        vec![content_blueprint],
    );

    let mut runtime = AppRuntime::new(View { root: container }, width as f32, height as f32, 1.0);
    runtime
        .layout_root
        .compute_layout(width as f32, height as f32);
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

        if let Some(bar) = runtime
            .active_scrollbars
            .iter()
            .find(|b| b.id == "browser-viewport-maximized")
        {
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
