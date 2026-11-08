//! Three-engine visual regression suite.
//!
//! Each fixture is captured by headless Chromium, Tauri (WebKitGTK on Linux),
//! and the Strata native renderer at a fixed 1280x720 viewport. Chromium is the
//! visual ground truth. The Chromium-vs-Strata comparison is the pass gate;
//! Tauri is an informational control because it is not Chromium.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use image::GenericImageView;

use crate::chromium_parity::{find_chromium_binary, render_chromium_deterministic};
use crate::components::{
    find_fixture, suite_fixtures, Fixture, DEFAULT_PERCEPTUAL_THRESHOLD, DEFAULT_SSIM_THRESHOLD,
};
use crate::determinism::injected_html;
use crate::diff::{compare_images_detailed, DetailedParityMetrics};
use crate::sha256;

/// Fixed capture width in logical pixels.
pub const VIEWPORT_WIDTH: u32 = 1280;
/// Fixed capture height in logical pixels.
pub const VIEWPORT_HEIGHT: u32 = 720;

const ENGINE_NOTE: &str =
    "Chromium is the visual ground truth. Tauri on Linux runs WebKitGTK, not Chromium; Tauri comparisons are informational.";

#[derive(Debug, Clone, Copy)]
pub struct Thresholds {
    pub ssim: f64,
    pub perceptual: f64,
}

impl Thresholds {
    fn fixture_defaults(fixture: &Fixture) -> Self {
        Self {
            ssim: fixture.ssim_threshold,
            perceptual: fixture.perceptual_threshold,
        }
    }
}

/// Parsed command line options for the parity suite.
#[derive(Debug, Clone)]
pub struct SuiteOptions {
    pub fixture_names: Vec<String>,
    pub single_run: bool,
    pub threshold_override: Option<Thresholds>,
    pub base_dir: PathBuf,
    pub chrome_binary: Option<String>,
}

impl SuiteOptions {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut fixture_names = Vec::new();
        let mut single_run = false;
        let mut threshold_override = None;
        let mut chrome_binary = None;
        let mut index = 1usize;

        while index < args.len() {
            let arg = &args[index];
            if arg == "--single-run" {
                single_run = true;
                index += 1;
                continue;
            }
            if let Some(value) = arg.strip_prefix("--fixture=") {
                fixture_names.push(value.to_string());
                index += 1;
                continue;
            }
            if arg == "--fixture" {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "--fixture requires a fixture name".to_string())?;
                fixture_names.push(value.clone());
                index += 2;
                continue;
            }
            if let Some(value) = arg.strip_prefix("--thresholds=") {
                threshold_override = Some(parse_thresholds(value)?);
                index += 1;
                continue;
            }
            if arg == "--thresholds" {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "--thresholds requires <ssim>,<perceptual>".to_string())?;
                threshold_override = Some(parse_thresholds(value)?);
                index += 2;
                continue;
            }
            if arg == "--chrome" || arg == "--chrome-binary" {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "--chrome requires a binary path".to_string())?;
                chrome_binary = Some(value.clone());
                index += 2;
                continue;
            }
            index += 1;
        }

        if fixture_names.is_empty() {
            fixture_names = suite_fixtures()
                .iter()
                .map(|fixture| fixture.name.to_string())
                .collect();
        }

        Ok(Self {
            fixture_names,
            single_run,
            threshold_override,
            base_dir: workspace_root().join("target/parity"),
            chrome_binary,
        })
    }
}

fn parse_thresholds(value: &str) -> Result<Thresholds, String> {
    let mut parts = value.split(',');
    let ssim = parts
        .next()
        .ok_or_else(|| "--thresholds requires <ssim>,<perceptual>".to_string())?
        .trim()
        .parse::<f64>()
        .map_err(|e| format!("Invalid SSIM threshold '{}': {}", value, e))?;
    let perceptual = parts
        .next()
        .ok_or_else(|| "--thresholds requires <ssim>,<perceptual>".to_string())?
        .trim()
        .parse::<f64>()
        .map_err(|e| format!("Invalid perceptual threshold '{}': {}", value, e))?;
    if parts.next().is_some() {
        return Err("--thresholds accepts exactly two values: <ssim>,<perceptual>".to_string());
    }
    if !(0.0..=1.0).contains(&ssim) {
        return Err("SSIM threshold must be between 0.0 and 1.0".to_string());
    }
    if !(0.0..=100.0).contains(&perceptual) {
        return Err("Perceptual threshold must be between 0.0 and 100.0".to_string());
    }
    Ok(Thresholds { ssim, perceptual })
}

fn workspace_root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .ancestors()
        .nth(3)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| manifest.to_path_buf())
}

#[derive(Debug, Clone)]
struct CaptureRun {
    label: String,
    chromium_png: PathBuf,
    strata_png: PathBuf,
    tauri_png: PathBuf,
    diff_chromium_strata: PathBuf,
    diff_chromium_tauri: PathBuf,
    chromium_hash: Option<String>,
    strata_hash: Option<String>,
    tauri_hash: Option<String>,
    chromium_error: Option<String>,
    strata_error: Option<String>,
    tauri_error: Option<String>,
    tauri_capture_note: Option<String>,
    chromium_strata: Option<DetailedParityMetrics>,
    chromium_tauri: Option<DetailedParityMetrics>,
    chromium_strata_error: Option<String>,
    chromium_tauri_error: Option<String>,
}

impl CaptureRun {
    fn new(dir: PathBuf, label: &str) -> Self {
        Self {
            label: label.to_string(),
            chromium_png: dir.join("chromium.png"),
            strata_png: dir.join("strata.png"),
            tauri_png: dir.join("tauri.png"),
            diff_chromium_strata: dir.join("diff_chromium_strata.png"),
            diff_chromium_tauri: dir.join("diff_chromium_tauri.png"),
            chromium_hash: None,
            strata_hash: None,
            tauri_hash: None,
            chromium_error: None,
            strata_error: None,
            tauri_error: None,
            tauri_capture_note: None,
            chromium_strata: None,
            chromium_tauri: None,
            chromium_strata_error: None,
            chromium_tauri_error: None,
        }
    }

    fn chromium_ready(&self) -> bool {
        self.chromium_hash.is_some()
    }

    fn strata_ready(&self) -> bool {
        self.strata_hash.is_some()
    }

    fn tauri_ready(&self) -> bool {
        self.tauri_hash.is_some()
    }
}

#[derive(Debug)]
struct FixtureResult {
    fixture: Fixture,
    thresholds: Thresholds,
    override_active: bool,
    run_a: CaptureRun,
    run_b: Option<CaptureRun>,
    determinism_chromium: String,
    determinism_strata: String,
    determinism_tauri: String,
    metric_pass: bool,
    gate_pass: bool,
    tauri_pass: bool,
}

struct EngineLabels {
    chromium: String,
    tauri: String,
    strata: String,
}

/// Runs the parity suite from raw process arguments.
///
/// Returns `Ok(true)` when every in-scope fixture passes, `Ok(false)` when the
/// pass gate or determinism check fails, and `Err` for configuration errors.
pub fn run_from_args(args: &[String]) -> Result<bool, String> {
    let options = SuiteOptions::parse(args)?;

    let chrome = find_chromium_binary(options.chrome_binary.as_deref())
        .ok_or_else(|| "No Google Chrome or Chromium executable found on system".to_string())?;
    let labels = EngineLabels {
        chromium: chromium_label(&chrome),
        tauri: tauri_label(),
        strata: strata_label(),
    };

    let mut selected = Vec::new();
    for name in &options.fixture_names {
        let fixture = find_fixture(name)
            .ok_or_else(|| format!("Unknown fixture '{}'. Use --list-fixtures.", name))?;
        selected.push(fixture);
    }
    if selected.is_empty() {
        return Err("No fixtures selected".to_string());
    }

    fs::create_dir_all(&options.base_dir)
        .map_err(|e| format!("Failed to create output directory: {}", e))?;
    let profile_root = options.base_dir.join("chrome-profile");
    fs::create_dir_all(&profile_root)
        .map_err(|e| format!("Failed to create Chromium profile root: {}", e))?;

    print_header(&options, &labels);

    if let Some(thresholds) = options.threshold_override {
        println!(
            "[WARN] THRESHOLD OVERRIDE ACTIVE: SSIM >= {:.4}, perceptual >= {:.2}%",
            thresholds.ssim, thresholds.perceptual
        );
        println!(
            "[WARN] Fixture defaults are SSIM >= {:.4}, perceptual >= {:.2}%. The override replaces them for this run.",
            DEFAULT_SSIM_THRESHOLD, DEFAULT_PERCEPTUAL_THRESHOLD
        );
    }

    if options.single_run {
        println!(
            "[INFO] --single-run: the second capture set and determinism comparison are skipped."
        );
    }

    let mut results = Vec::new();
    for fixture in selected {
        let result = run_fixture(&fixture, &options, &profile_root, &labels)?;
        results.push(result);
    }

    print_summary(&results);
    let overall = results.iter().all(|result| result.gate_pass);
    if overall {
        println!(
            "\n[OK] Parity gate passed for {} fixture(s).",
            results.len()
        );
    } else {
        println!(
            "\n[FAIL] Parity gate failed for {} of {} fixture(s).",
            results.iter().filter(|result| !result.gate_pass).count(),
            results.len()
        );
    }
    println!("[OUTPUT] {}", options.base_dir.display());
    Ok(overall)
}

fn print_header(options: &SuiteOptions, labels: &EngineLabels) {
    println!("=============================================================");
    println!("  Strata Three-Engine Visual Parity Suite");
    println!("=============================================================");
    println!("[ENGINE] Chromium: {}", labels.chromium);
    println!("[ENGINE] Tauri:    {}", labels.tauri);
    println!("[ENGINE] Strata:   {}", labels.strata);
    println!(
        "[VIEWPORT] {}x{}, device scale factor 1.0",
        VIEWPORT_WIDTH, VIEWPORT_HEIGHT
    );
    println!(
        "[DEFAULT THRESHOLDS] SSIM >= {:.4}, perceptual >= {:.2}%",
        DEFAULT_SSIM_THRESHOLD, DEFAULT_PERCEPTUAL_THRESHOLD
    );
    println!("[OUTPUT] {}", options.base_dir.display());
    println!("[NOTE] {}", ENGINE_NOTE);
}

fn chromium_label(binary: &str) -> String {
    let version = Command::new(binary)
        .arg("--version")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unknown version".to_string());
    format!("{} [{}]", version, binary)
}

fn webkit_version() -> String {
    for package in ["webkit2gtk-4.1", "webkit2gtk-4.0"] {
        let output = Command::new("pkg-config")
            .args(["--modversion", package])
            .output();
        if let Ok(output) = output {
            if output.status.success() {
                let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !version.is_empty() {
                    return version;
                }
            }
        }
    }
    "unavailable".to_string()
}

fn tauri_label() -> String {
    format!("Tauri 2 on WebKitGTK {} (not Chromium)", webkit_version())
}

fn strata_label() -> String {
    format!("Strata native engine {}", env!("CARGO_PKG_VERSION"))
}

fn run_fixture(
    fixture: &Fixture,
    options: &SuiteOptions,
    profile_root: &Path,
    labels: &EngineLabels,
) -> Result<FixtureResult, String> {
    let fixture_dir = options.base_dir.join(fixture.name);
    if fixture_dir.exists() {
        fs::remove_dir_all(&fixture_dir).map_err(|e| {
            format!(
                "Failed to clear previous fixture output {}: {}",
                fixture_dir.display(),
                e
            )
        })?;
    }
    fs::create_dir_all(&fixture_dir)
        .map_err(|e| format!("Failed to create fixture output directory: {}", e))?;

    let injected = injected_html(fixture.html, fixture.scroll_y);
    let effective_thresholds = options
        .threshold_override
        .unwrap_or_else(|| Thresholds::fixture_defaults(fixture));

    let run_a = capture_run(
        fixture,
        &injected,
        &fixture_dir.join("run_a"),
        "run_a",
        profile_root,
        options,
    )?;

    let run_b = if options.single_run {
        None
    } else {
        Some(capture_run(
            fixture,
            &injected,
            &fixture_dir.join("run_b"),
            "run_b",
            profile_root,
            options,
        )?)
    };

    let (determinism_chromium, determinism_strata, determinism_tauri) =
        determinism_report(&run_a, run_b.as_ref(), options.single_run);

    let metric_pass = run_a
        .chromium_strata
        .as_ref()
        .map(|metrics| metrics.passes(effective_thresholds.ssim, effective_thresholds.perceptual))
        .unwrap_or(false);
    let tauri_pass = run_a
        .chromium_tauri
        .as_ref()
        .map(|metrics| metrics.passes(effective_thresholds.ssim, effective_thresholds.perceptual))
        .unwrap_or(false);

    let determinism_ok = if options.single_run {
        true
    } else {
        determinism_chromium == "DETERMINISTIC" && determinism_strata == "DETERMINISTIC"
    };
    let captures_ok = run_a.chromium_ready() && run_a.strata_ready();
    let gate_pass = if fixture.in_scope {
        metric_pass && determinism_ok && captures_ok
    } else {
        determinism_ok && captures_ok
    };

    let result = FixtureResult {
        fixture: fixture.clone(),
        thresholds: effective_thresholds,
        override_active: options.threshold_override.is_some(),
        run_a,
        run_b,
        determinism_chromium,
        determinism_strata,
        determinism_tauri,
        metric_pass,
        gate_pass,
        tauri_pass,
    };

    copy_canonical_artifacts(&result, &fixture_dir)?;
    write_hashes(&result, &fixture_dir)?;
    write_report(&result, &fixture_dir, labels)?;
    print_fixture(&result);

    Ok(result)
}

fn capture_run(
    fixture: &Fixture,
    injected: &str,
    run_dir: &Path,
    run_label: &str,
    profile_root: &Path,
    options: &SuiteOptions,
) -> Result<CaptureRun, String> {
    fs::create_dir_all(run_dir)
        .map_err(|e| format!("Failed to create capture directory: {}", e))?;
    let input_path = run_dir.join("input.html");
    fs::write(&input_path, injected)
        .map_err(|e| format!("Failed to write injected fixture input: {}", e))?;

    let mut run = CaptureRun::new(run_dir.to_path_buf(), run_label);

    match render_chromium_deterministic(
        injected,
        VIEWPORT_WIDTH,
        VIEWPORT_HEIGHT,
        &run.chromium_png.display().to_string(),
        run_dir,
        &profile_root.join(format!("{}-{}", fixture.name, run_label)),
        options.chrome_binary.as_deref(),
    ) {
        Ok(()) => match validate_capture(
            &run.chromium_png,
            VIEWPORT_WIDTH,
            VIEWPORT_HEIGHT,
            "Chromium",
        ) {
            Ok(()) => run.chromium_hash = sha256::file_hex_digest(&run.chromium_png).ok(),
            Err(error) => run.chromium_error = Some(error),
        },
        Err(error) => run.chromium_error = Some(error),
    }

    match capture_strata(&input_path, &run.strata_png, fixture.scroll_y) {
        Ok(()) => {
            match validate_capture(&run.strata_png, VIEWPORT_WIDTH, VIEWPORT_HEIGHT, "Strata") {
                Ok(()) => run.strata_hash = sha256::file_hex_digest(&run.strata_png).ok(),
                Err(error) => run.strata_error = Some(error),
            }
        }
        Err(error) => run.strata_error = Some(error),
    }

    match capture_tauri(&input_path, &run.tauri_png, fixture.name, run_label) {
        Ok(()) => match normalize_tauri_capture(&run.tauri_png, VIEWPORT_WIDTH, VIEWPORT_HEIGHT) {
            Ok(note) => {
                run.tauri_capture_note = note;
                match validate_capture(&run.tauri_png, VIEWPORT_WIDTH, VIEWPORT_HEIGHT, "Tauri") {
                    Ok(()) => run.tauri_hash = sha256::file_hex_digest(&run.tauri_png).ok(),
                    Err(error) => run.tauri_error = Some(error),
                }
            }
            Err(error) => run.tauri_error = Some(error),
        },
        Err(error) => run.tauri_error = Some(error),
    }

    if run.chromium_hash.is_some() && run.strata_hash.is_some() {
        match compare_images_detailed(
            &run.chromium_png,
            &run.strata_png,
            &run.diff_chromium_strata,
        ) {
            Ok(metrics) => run.chromium_strata = Some(metrics),
            Err(error) => run.chromium_strata_error = Some(error),
        }
    } else {
        run.chromium_strata_error =
            Some("Chromium and Strata captures are both required for this comparison".to_string());
    }

    if run.chromium_hash.is_some() && run.tauri_hash.is_some() {
        match compare_images_detailed(&run.chromium_png, &run.tauri_png, &run.diff_chromium_tauri) {
            Ok(metrics) => run.chromium_tauri = Some(metrics),
            Err(error) => run.chromium_tauri_error = Some(error),
        }
    } else {
        run.chromium_tauri_error =
            Some("Chromium and Tauri captures are both required for this comparison".to_string());
    }

    Ok(run)
}

fn capture_strata(input_path: &Path, output_png: &Path, scroll_y: f32) -> Result<(), String> {
    let exe =
        env::current_exe().map_err(|e| format!("Failed to resolve current executable: {}", e))?;
    let mut command = Command::new(exe);
    command
        .arg("--capture-strata")
        .arg(input_path)
        .arg(output_png)
        .arg(VIEWPORT_WIDTH.to_string())
        .arg(VIEWPORT_HEIGHT.to_string())
        .arg(format!("{scroll_y}"));
    let output = run_child_with_timeout(&mut command, Duration::from_secs(30))?;
    if !output.status.success() {
        return Err(format!(
            "Strata capture process exited with status {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

fn capture_tauri(
    input_path: &Path,
    output_png: &Path,
    fixture_name: &str,
    run_label: &str,
) -> Result<(), String> {
    let exe =
        env::current_exe().map_err(|e| format!("Failed to resolve current executable: {}", e))?;
    let title = format!("Strata Parity Tauri {} {}", fixture_name, run_label);
    let mut command = Command::new(exe);
    command
        .arg("--capture-tauri")
        .arg(input_path)
        .arg(output_png)
        .arg(VIEWPORT_WIDTH.to_string())
        .arg(VIEWPORT_HEIGHT.to_string())
        .arg(&title);
    let output = run_child_with_timeout(&mut command, Duration::from_secs(30))?;
    if !output.status.success() {
        return Err(format!(
            "Tauri capture process exited with status {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

fn run_child_with_timeout(command: &mut Command, timeout: Duration) -> Result<Output, String> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn capture process: {}", e))?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                return child
                    .wait_with_output()
                    .map_err(|e| format!("Failed to read capture process output: {}", e));
            }
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!(
                        "Capture process timed out after {} seconds",
                        timeout.as_secs()
                    ));
                }
                thread::sleep(Duration::from_millis(50));
            }
            Err(error) => {
                let _ = child.kill();
                return Err(format!("Failed to poll capture process: {}", error));
            }
        }
    }
}

fn normalize_tauri_capture(
    path: &Path,
    expected_width: u32,
    expected_height: u32,
) -> Result<Option<String>, String> {
    let image = image::open(path).map_err(|e| {
        format!(
            "Tauri capture is not a valid image at {}: {}",
            path.display(),
            e
        )
    })?;
    let (width, height) = image.dimensions();
    if width == expected_width && height == expected_height {
        return Ok(None);
    }
    if width < expected_width || height < expected_height {
        return Err(format!(
            "Tauri capture is {}x{} (expected {}x{})",
            width, height, expected_width, expected_height
        ));
    }

    let cropped = image.crop_imm(0, 0, expected_width, expected_height);
    cropped.save(path).map_err(|e| {
        format!(
            "Failed to normalize Tauri capture {}: {}",
            path.display(),
            e
        )
    })?;
    Ok(Some(format!(
        "Cropped {}x{} window capture to top-left {}x{} (removed {}x{} window-manager padding)",
        width,
        height,
        expected_width,
        expected_height,
        width - expected_width,
        height - expected_height
    )))
}

fn validate_capture(
    path: &Path,
    expected_width: u32,
    expected_height: u32,
    engine: &str,
) -> Result<(), String> {
    let metadata = fs::metadata(path)
        .map_err(|e| format!("{} capture missing at {}: {}", engine, path.display(), e))?;
    if metadata.len() == 0 {
        return Err(format!("{} capture is empty at {}", engine, path.display()));
    }
    let image = image::open(path).map_err(|e| {
        format!(
            "{} capture is not a valid image at {}: {}",
            engine,
            path.display(),
            e
        )
    })?;
    let (width, height) = image.dimensions();
    if width != expected_width || height != expected_height {
        return Err(format!(
            "{} capture is {}x{} (expected {}x{})",
            engine, width, height, expected_width, expected_height
        ));
    }
    Ok(())
}

fn determinism_report(
    run_a: &CaptureRun,
    run_b: Option<&CaptureRun>,
    single_run: bool,
) -> (String, String, String) {
    if single_run {
        return (
            "SKIPPED".to_string(),
            "SKIPPED".to_string(),
            "SKIPPED".to_string(),
        );
    }
    let run_b = match run_b {
        Some(run) => run,
        None => {
            return (
                "UNAVAILABLE".to_string(),
                "UNAVAILABLE".to_string(),
                "UNAVAILABLE".to_string(),
            )
        }
    };

    let chromium = compare_hashes(&run_a.chromium_hash, &run_b.chromium_hash);
    let strata = compare_hashes(&run_a.strata_hash, &run_b.strata_hash);
    let tauri = compare_hashes(&run_a.tauri_hash, &run_b.tauri_hash);
    (chromium, strata, tauri)
}

fn compare_hashes(left: &Option<String>, right: &Option<String>) -> String {
    match (left, right) {
        (Some(left), Some(right)) => {
            if left == right {
                "DETERMINISTIC".to_string()
            } else {
                "NONDETERMINISTIC".to_string()
            }
        }
        _ => "UNAVAILABLE".to_string(),
    }
}

fn copy_canonical_artifacts(result: &FixtureResult, fixture_dir: &Path) -> Result<(), String> {
    copy_optional(
        result
            .run_a
            .chromium_hash
            .as_ref()
            .map(|_| result.run_a.chromium_png.as_path()),
        &fixture_dir.join("chromium.png"),
    )?;
    copy_optional(
        result
            .run_a
            .strata_hash
            .as_ref()
            .map(|_| result.run_a.strata_png.as_path()),
        &fixture_dir.join("strata.png"),
    )?;
    copy_optional(
        result
            .run_a
            .tauri_hash
            .as_ref()
            .map(|_| result.run_a.tauri_png.as_path()),
        &fixture_dir.join("tauri.png"),
    )?;
    copy_optional(
        result
            .run_a
            .chromium_strata
            .as_ref()
            .map(|_| result.run_a.diff_chromium_strata.as_path()),
        &fixture_dir.join("diff_chromium_strata.png"),
    )?;
    copy_optional(
        result
            .run_a
            .chromium_tauri
            .as_ref()
            .map(|_| result.run_a.diff_chromium_tauri.as_path()),
        &fixture_dir.join("diff_chromium_tauri.png"),
    )?;
    Ok(())
}

fn copy_optional(source: Option<&Path>, destination: &Path) -> Result<(), String> {
    match source {
        Some(source) => fs::copy(source, destination).map(|_| ()).map_err(|e| {
            format!(
                "Failed to copy {} to {}: {}",
                source.display(),
                destination.display(),
                e
            )
        }),
        None => {
            if destination.exists() {
                fs::remove_file(destination).map_err(|e| {
                    format!(
                        "Failed to remove stale artifact {}: {}",
                        destination.display(),
                        e
                    )
                })?;
            }
            Ok(())
        }
    }
}

fn write_hashes(result: &FixtureResult, fixture_dir: &Path) -> Result<(), String> {
    let mut out = String::new();
    out.push_str(&format!("fixture: {}\n", result.fixture.name));
    write_run_hashes(&mut out, &result.run_a);
    if let Some(run_b) = &result.run_b {
        write_run_hashes(&mut out, run_b);
    }
    out.push_str(&format!(
        "determinism chromium: {}\n",
        result.determinism_chromium
    ));
    out.push_str(&format!(
        "determinism strata: {}\n",
        result.determinism_strata
    ));
    out.push_str(&format!(
        "determinism tauri: {}\n",
        result.determinism_tauri
    ));
    out.push_str(&format!(
        "gate: {}\n",
        if result.gate_pass { "PASS" } else { "FAIL" }
    ));
    fs::write(fixture_dir.join("hashes.txt"), out)
        .map_err(|e| format!("Failed to write hashes.txt: {}", e))
}

fn write_run_hashes(out: &mut String, run: &CaptureRun) {
    out.push_str(&format!(
        "{} chromium: {}\n",
        run.label,
        run.chromium_hash.as_deref().unwrap_or("UNAVAILABLE")
    ));
    out.push_str(&format!(
        "{} strata: {}\n",
        run.label,
        run.strata_hash.as_deref().unwrap_or("UNAVAILABLE")
    ));
    out.push_str(&format!(
        "{} tauri: {}\n",
        run.label,
        run.tauri_hash.as_deref().unwrap_or("UNAVAILABLE")
    ));
}

fn write_report(
    result: &FixtureResult,
    fixture_dir: &Path,
    labels: &EngineLabels,
) -> Result<(), String> {
    let mut out = String::new();
    out.push_str("{\n");
    out.push_str("  \"fixture\": ");
    write_json_string(&mut out, result.fixture.name);
    out.push_str(",\n  \"description\": ");
    write_json_string(&mut out, result.fixture.description);
    out.push_str(&format!(
        ",\n  \"in_scope\": {},\n",
        result.fixture.in_scope
    ));
    out.push_str("  \"known_gap\": ");
    write_optional_string(&mut out, result.fixture.known_gap);
    out.push_str(",\n");
    out.push_str(&format!(
        "  \"viewport\": {{\"width\": {}, \"height\": {}, \"device_scale_factor\": 1.0}},\n",
        VIEWPORT_WIDTH, VIEWPORT_HEIGHT
    ));
    out.push_str("  \"engine_labels\": {\n");
    out.push_str("    \"chromium\": ");
    write_json_string(&mut out, &labels.chromium);
    out.push_str(",\n    \"tauri\": ");
    write_json_string(&mut out, &labels.tauri);
    out.push_str(",\n    \"strata\": ");
    write_json_string(&mut out, &labels.strata);
    out.push_str("\n  },\n");
    out.push_str("  \"note\": ");
    write_json_string(&mut out, ENGINE_NOTE);
    out.push_str(",\n");
    out.push_str(&format!(
        "  \"thresholds\": {{\"fixture_ssim\": {:.6}, \"fixture_perceptual_percent\": {:.6}, \"effective_ssim\": {:.6}, \"effective_perceptual_percent\": {:.6}, \"override\": {}}},\n",
        result.fixture.ssim_threshold,
        result.fixture.perceptual_threshold,
        result.thresholds.ssim,
        result.thresholds.perceptual,
        result.override_active
    ));
    out.push_str("  \"artifacts\": {\n");
    out.push_str("    \"directory\": ");
    write_json_string(&mut out, &fixture_dir.display().to_string());
    out.push_str(",\n    \"chromium\": ");
    write_optional_path(
        &mut out,
        optional_ready_path(&result.run_a.chromium_png, result.run_a.chromium_ready()),
    );
    out.push_str(",\n    \"tauri\": ");
    write_optional_path(
        &mut out,
        optional_ready_path(&result.run_a.tauri_png, result.run_a.tauri_ready()),
    );
    out.push_str(",\n    \"strata\": ");
    write_optional_path(
        &mut out,
        optional_ready_path(&result.run_a.strata_png, result.run_a.strata_ready()),
    );
    out.push_str(",\n    \"diff_chromium_strata\": ");
    write_optional_path(
        &mut out,
        optional_ready_path(
            &result.run_a.diff_chromium_strata,
            result.run_a.chromium_strata.is_some(),
        ),
    );
    out.push_str(",\n    \"diff_chromium_tauri\": ");
    write_optional_path(
        &mut out,
        optional_ready_path(
            &result.run_a.diff_chromium_tauri,
            result.run_a.chromium_tauri.is_some(),
        ),
    );
    out.push_str("\n  },\n");

    out.push_str("  \"sha256\": {\n    \"run_a\": ");
    write_run_hashes_json(&mut out, &result.run_a);
    out.push_str(",\n    \"run_b\": ");
    match &result.run_b {
        Some(run_b) => write_run_hashes_json(&mut out, run_b),
        None => out.push_str("null"),
    }
    out.push_str("\n  },\n");

    out.push_str("  \"comparisons\": {\n    \"run_a\": ");
    write_run_comparisons_json(&mut out, &result.run_a, result.thresholds);
    out.push_str(",\n    \"run_b\": ");
    match &result.run_b {
        Some(run_b) => write_run_comparisons_json(&mut out, run_b, result.thresholds),
        None => out.push_str("null"),
    }
    out.push_str("\n  },\n");

    out.push_str("  \"determinism\": {\n");
    out.push_str(&format!(
        "    \"chromium\": \"{}\",\n",
        result.determinism_chromium
    ));
    out.push_str(&format!(
        "    \"strata\": \"{}\",\n",
        result.determinism_strata
    ));
    out.push_str(&format!(
        "    \"tauri\": \"{}\",\n",
        result.determinism_tauri
    ));
    let determinism_gate = if result.determinism_chromium == "DETERMINISTIC"
        && result.determinism_strata == "DETERMINISTIC"
    {
        "PASS"
    } else if result.determinism_chromium == "SKIPPED" && result.determinism_strata == "SKIPPED" {
        "SKIPPED"
    } else {
        "FAIL"
    };
    out.push_str(&format!("    \"gate\": \"{}\"\n  }},\n", determinism_gate));

    out.push_str("  \"chromium_vs_strata\": ");
    write_optional_metrics(
        &mut out,
        result.run_a.chromium_strata.as_ref(),
        result.thresholds,
    );
    out.push_str(",\n  \"chromium_vs_tauri\": ");
    match (
        result.run_a.chromium_tauri.as_ref(),
        &result.run_a.tauri_error,
    ) {
        (Some(metrics), _) => write_metrics(&mut out, metrics, result.thresholds),
        (None, Some(error)) => {
            out.push_str("{\"available\": false, \"error\": ");
            write_json_string(&mut out, error);
            out.push('}');
        }
        (None, None) => {
            out.push_str("{\"available\": false, \"error\": \"Tauri comparison unavailable\"}")
        }
    }
    out.push_str(",\n");
    out.push_str(&format!(
        "  \"tauri_available\": {},\n",
        result.run_a.tauri_ready()
    ));
    out.push_str("  \"tauri_error\": ");
    match &result.run_a.tauri_error {
        Some(error) => write_json_string(&mut out, error),
        None => out.push_str("null"),
    }
    out.push_str(",\n  \"tauri_capture_normalization\": ");
    write_optional_string(&mut out, result.run_a.tauri_capture_note.as_deref());
    out.push_str(",\n");
    out.push_str(&format!("  \"passed\": {},\n", result.gate_pass));
    out.push_str("  \"gate\": \"chromium_vs_strata\"\n");
    out.push_str("}\n");

    fs::write(fixture_dir.join("report.json"), out)
        .map_err(|e| format!("Failed to write report.json: {}", e))
}

fn optional_ready_path(path: &Path, ready: bool) -> Option<&Path> {
    if ready {
        Some(path)
    } else {
        None
    }
}

fn write_optional_path(out: &mut String, path: Option<&Path>) {
    match path {
        Some(path) => write_json_string(out, &path.display().to_string()),
        None => out.push_str("null"),
    }
}

fn write_run_hashes_json(out: &mut String, run: &CaptureRun) {
    out.push_str("{\n");
    out.push_str("      \"chromium_sha256\": ");
    write_optional_string(out, run.chromium_hash.as_deref());
    out.push_str(",\n      \"strata_sha256\": ");
    write_optional_string(out, run.strata_hash.as_deref());
    out.push_str(",\n      \"tauri_sha256\": ");
    write_optional_string(out, run.tauri_hash.as_deref());
    out.push_str("\n    }");
}

fn write_run_comparisons_json(out: &mut String, run: &CaptureRun, thresholds: Thresholds) {
    out.push_str("{\n      \"chromium_vs_strata\": ");
    write_optional_metrics(out, run.chromium_strata.as_ref(), thresholds);
    out.push_str(",\n      \"chromium_vs_tauri\": ");
    write_optional_metrics(out, run.chromium_tauri.as_ref(), thresholds);
    out.push_str(",\n      \"errors\": {\n        \"chromium\": ");
    write_optional_string(out, run.chromium_error.as_deref());
    out.push_str(",\n        \"strata\": ");
    write_optional_string(out, run.strata_error.as_deref());
    out.push_str(",\n        \"tauri\": ");
    write_optional_string(out, run.tauri_error.as_deref());
    out.push_str(",\n        \"chromium_vs_strata\": ");
    write_optional_string(out, run.chromium_strata_error.as_deref());
    out.push_str(",\n        \"chromium_vs_tauri\": ");
    write_optional_string(out, run.chromium_tauri_error.as_deref());
    out.push_str("\n      }\n    }");
}

fn write_optional_metrics(
    out: &mut String,
    metrics: Option<&DetailedParityMetrics>,
    thresholds: Thresholds,
) {
    match metrics {
        Some(metrics) => write_metrics(out, metrics, thresholds),
        None => out.push_str("null"),
    }
}

fn write_metrics(out: &mut String, metrics: &DetailedParityMetrics, thresholds: Thresholds) {
    out.push_str(&format!(
        "{{\"width\": {}, \"height\": {}, \"total_pixels\": {}, \"exact_match_percentage\": {:.6}, \"perceptual_match_percentage\": {:.6}, \"ssim\": {:.6}, \"mean_delta_e\": {:.6}, \"max_delta_e\": {:.6}, \"significant_diff_pixels\": {}, \"significant_diff_bbox\": ",
        metrics.width,
        metrics.height,
        metrics.total_pixels,
        metrics.exact_match_percentage,
        metrics.perceptual_match_percentage,
        metrics.ssim_score,
        metrics.mean_delta_e,
        metrics.max_delta_e,
        metrics.significant_diff_pixels
    ));
    match metrics.significant_diff_bounds {
        Some(bounds) => out.push_str(&format!(
            "{{\"x\": {}, \"y\": {}, \"width\": {}, \"height\": {}}}",
            bounds.x, bounds.y, bounds.width, bounds.height
        )),
        None => out.push_str("null"),
    }
    out.push_str(&format!(
        ", \"passed\": {}, \"diff\": ",
        metrics.passes(thresholds.ssim, thresholds.perceptual)
    ));
    write_json_string(out, &metrics.diff_path);
    out.push('}');
}

fn write_optional_string(out: &mut String, value: Option<&str>) {
    match value {
        Some(value) => write_json_string(out, value),
        None => out.push_str("null"),
    }
}

fn write_json_string(out: &mut String, value: &str) {
    out.push('"');
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            character if character.is_control() => {
                out.push_str(&format!("\\u{:04x}", character as u32));
            }
            character => out.push(character),
        }
    }
    out.push('"');
}

fn print_fixture(result: &FixtureResult) {
    let scope = if result.fixture.in_scope {
        "in scope"
    } else {
        "out of scope"
    };
    println!("\n[Fixture] {} ({})", result.fixture.name, scope);
    println!("  {}", result.fixture.description);
    if let Some(known_gap) = result.fixture.known_gap {
        println!("  [KNOWN GAP] {}", known_gap);
    }
    print_run_hashes(&result.run_a);
    if let Some(run_b) = &result.run_b {
        print_run_hashes(run_b);
    }
    if let Some(metrics) = &result.run_a.chromium_strata {
        println!(
            "  Chromium vs Strata: SSIM {:.4}, exact {:.2}%, perceptual {:.2}%, mean dE {:.2}, max dE {:.2}, significant {} px, bbox {}",
            metrics.ssim_score,
            metrics.exact_match_percentage,
            metrics.perceptual_match_percentage,
            metrics.mean_delta_e,
            metrics.max_delta_e,
            metrics.significant_diff_pixels,
            format_bounds(metrics)
        );
    } else if let Some(error) = &result.run_a.chromium_strata_error {
        println!("  Chromium vs Strata: UNAVAILABLE ({})", error);
    }
    if let Some(metrics) = &result.run_a.chromium_tauri {
        println!(
            "  Chromium vs Tauri:  SSIM {:.4}, exact {:.2}%, perceptual {:.2}%, mean dE {:.2}, max dE {:.2}, significant {} px, bbox {}",
            metrics.ssim_score,
            metrics.exact_match_percentage,
            metrics.perceptual_match_percentage,
            metrics.mean_delta_e,
            metrics.max_delta_e,
            metrics.significant_diff_pixels,
            format_bounds(metrics)
        );
    } else if let Some(error) = &result.run_a.tauri_error {
        println!("  Chromium vs Tauri:  UNAVAILABLE ({})", error);
    }
    if let Some(note) = &result.run_a.tauri_capture_note {
        println!("  [Tauri] {}", note);
    }
    println!(
        "  Determinism: chromium {} | strata {} | tauri {}",
        result.determinism_chromium, result.determinism_strata, result.determinism_tauri
    );
    println!(
        "  Result: {} (metrics {}, deterministic {}, captures {})",
        if result.gate_pass {
            "[OK] PASS"
        } else {
            "[FAIL]"
        },
        if result.metric_pass { "pass" } else { "fail" },
        if result.determinism_chromium == "DETERMINISTIC"
            && result.determinism_strata == "DETERMINISTIC"
            || result.determinism_chromium == "SKIPPED"
        {
            "yes"
        } else {
            "no"
        },
        if result.run_a.chromium_ready() && result.run_a.strata_ready() {
            "ok"
        } else {
            "missing"
        }
    );
    if !result.fixture.in_scope && !result.metric_pass {
        println!(
            "  [WARN] Out-of-scope fixture did not meet thresholds; it is excluded from the gate."
        );
    }
    if !result.tauri_pass && result.run_a.tauri_ready() {
        println!("  [WARN] Tauri control did not meet thresholds; Tauri is informational only.");
    }
}

fn print_run_hashes(run: &CaptureRun) {
    println!(
        "  {} chromium {}",
        run.label,
        run.chromium_hash.as_deref().unwrap_or("UNAVAILABLE")
    );
    println!(
        "  {} strata   {}",
        run.label,
        run.strata_hash.as_deref().unwrap_or("UNAVAILABLE")
    );
    println!(
        "  {} tauri    {}",
        run.label,
        run.tauri_hash.as_deref().unwrap_or("UNAVAILABLE")
    );
}

fn format_bounds(metrics: &DetailedParityMetrics) -> String {
    match metrics.significant_diff_bounds {
        Some(bounds) => format!(
            "x={} y={} {}x{}",
            bounds.x, bounds.y, bounds.width, bounds.height
        ),
        None => "none".to_string(),
    }
}

fn print_summary(results: &[FixtureResult]) {
    println!("\n==========================================================================================");
    println!("  THREE-ENGINE PARITY SUMMARY");
    println!("==========================================================================================");
    println!(
        "{:<24} {:<11} {:>9} {:>12} {:>9} {:>12} {:<18} {:<6}",
        "Fixture",
        "In scope",
        "SSIM C/S",
        "Perceptual",
        "SSIM C/T",
        "Perceptual",
        "Determinism",
        "Result"
    );
    println!("------------------------------------------------------------------------------------------");
    for result in results {
        let (ssim_cs, perceptual_cs) = result
            .run_a
            .chromium_strata
            .as_ref()
            .map(|metrics| {
                (
                    format!("{:.4}", metrics.ssim_score),
                    format!("{:.2}%", metrics.perceptual_match_percentage),
                )
            })
            .unwrap_or_else(|| ("UNAVAILABLE".to_string(), "UNAVAILABLE".to_string()));
        let (ssim_ct, perceptual_ct) = result
            .run_a
            .chromium_tauri
            .as_ref()
            .map(|metrics| {
                (
                    format!("{:.4}", metrics.ssim_score),
                    format!("{:.2}%", metrics.perceptual_match_percentage),
                )
            })
            .unwrap_or_else(|| ("UNAVAILABLE".to_string(), "UNAVAILABLE".to_string()));
        let determinism = format!(
            "{} / {}",
            result.determinism_chromium, result.determinism_strata
        );
        println!(
            "{:<24} {:<11} {:>9} {:>12} {:>9} {:>12} {:<18} {:<6}",
            result.fixture.name,
            if result.fixture.in_scope { "yes" } else { "no" },
            ssim_cs,
            perceptual_cs,
            ssim_ct,
            perceptual_ct,
            determinism,
            if result.gate_pass { "PASS" } else { "FAIL" }
        );
    }
    println!("==========================================================================================");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn parses_fixture_and_threshold_overrides() {
        let options = SuiteOptions::parse(&args(&[
            "strata_parity_tauri",
            "--parity-suite",
            "--fixture",
            "grid_tracks",
            "--thresholds",
            "0.9,90",
            "--single-run",
        ]))
        .expect("parse options");
        assert_eq!(options.fixture_names, vec!["grid_tracks".to_string()]);
        assert!(options.single_run);
        let thresholds = options.threshold_override.expect("override");
        assert!((thresholds.ssim - 0.9).abs() < f64::EPSILON);
        assert!((thresholds.perceptual - 90.0).abs() < f64::EPSILON);
    }

    #[test]
    fn rejects_malformed_thresholds() {
        assert!(parse_thresholds("1.0").is_err());
        assert!(parse_thresholds("1.5,90").is_err());
        assert!(parse_thresholds("0.9,101").is_err());
        assert!(parse_thresholds("0.9,90,10").is_err());
        assert!(parse_thresholds("abc,90").is_err());
    }

    #[test]
    fn hashes_compare_as_expected() {
        assert_eq!(
            compare_hashes(&Some("a".to_string()), &Some("a".to_string())),
            "DETERMINISTIC"
        );
        assert_eq!(
            compare_hashes(&Some("a".to_string()), &Some("b".to_string())),
            "NONDETERMINISTIC"
        );
        assert_eq!(compare_hashes(&None, &Some("b".to_string())), "UNAVAILABLE");
    }
}
