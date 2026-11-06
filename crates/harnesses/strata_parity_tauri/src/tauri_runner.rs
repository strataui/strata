//! Tauri 2 WebKitGTK Window Runner and Snapshot Orchestrator.

use crate::fixture::{get_fixture_html, ParityLevel};
use std::fs;
use std::path::Path;
use std::thread;
use std::time::Duration;
use tauri::{WebviewUrl, WebviewWindowBuilder};

/// Runs the Tauri 2 WebKitGTK window on DISPLAY=:1 maximized, takes a screenshot, and exits.
pub fn run_tauri_window(level: ParityLevel, html_fixture_path: &str, screenshot_path: &str) {
    fs::write(html_fixture_path, get_fixture_html(level))
        .expect("Failed to write HTML fixture for Tauri");

    let file_url = format!("file://{}", html_fixture_path);
    let parsed_url: tauri::Url = file_url.parse().expect("Valid file URL");

    let shot_path = screenshot_path.to_string();

    let app = tauri::Builder::default()
        .setup(move |app| {
            let _win = WebviewWindowBuilder::new(app, "tauri-parity", WebviewUrl::External(parsed_url))
                .title("Parity Benchmark - Tauri")
                .inner_size(1920.0, 1080.0)
                .decorations(false)
                .maximized(true)
                .build()?;

            let handle = app.handle().clone();
            thread::spawn(move || {
                println!("[Tauri] Window opened maximized (decorations: false). Waiting 2.0s for WebKitGTK render...");
                thread::sleep(Duration::from_millis(2000));
                println!("[Tauri] Capturing window-scoped snapshot to {}...", shot_path);
                let captured = crate::capture::capture_window_by_title("Parity Benchmark - Tauri", &shot_path);
                println!("[Tauri] Window capture result: {}", captured);
                thread::sleep(Duration::from_millis(500));
                handle.exit(0);
            });

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("Error building Tauri application");

    app.run(|_, _| {});
}

/// Loads a prepared HTML file in a fixed-size Tauri window, captures it, and exits.
///
/// The window is undecorated, non-resizable, and explicitly not maximized. The
/// capture is scoped to the window ID, so a window manager offset does not
/// affect the pixels. A fixed 1500 ms settle period is used as the load signal.
pub fn run_tauri_capture(
    html_fixture_path: &str,
    screenshot_path: &str,
    width: u32,
    height: u32,
    title: &str,
) -> bool {
    let file_url = format!("file://{}", html_fixture_path);
    let parsed_url: tauri::Url = match file_url.parse() {
        Ok(url) => url,
        Err(error) => {
            eprintln!("[Tauri] Invalid file URL {}: {}", file_url, error);
            return false;
        }
    };

    let shot_path = screenshot_path.to_string();
    let window_title = title.to_string();
    let capture_title = window_title.clone();

    let app = match tauri::Builder::default()
        .setup(move |app| {
            let _window =
                WebviewWindowBuilder::new(app, "tauri-parity", WebviewUrl::External(parsed_url))
                    .title(&window_title)
                    .inner_size(width as f64, height as f64)
                    .decorations(false)
                    .resizable(false)
                    .maximized(false)
                    .build()?;

            let handle = app.handle().clone();
            thread::spawn(move || {
                println!(
                    "[Tauri] Fixed window opened at {}x{}; waiting 1500 ms for WebKitGTK render.",
                    width, height
                );
                thread::sleep(Duration::from_millis(1500));
                println!(
                    "[Tauri] Capturing window-scoped snapshot to {}...",
                    shot_path
                );
                let captured = crate::capture::capture_window_by_title(&capture_title, &shot_path);
                println!("[Tauri] Window capture result: {}", captured);
                thread::sleep(Duration::from_millis(250));
                handle.exit(if captured { 0 } else { 1 });
            });

            Ok(())
        })
        .build(tauri::generate_context!())
    {
        Ok(app) => app,
        Err(error) => {
            eprintln!("[Tauri] Failed to build application: {}", error);
            return false;
        }
    };

    app.run(|_, _| {});
    Path::new(screenshot_path).exists()
}
