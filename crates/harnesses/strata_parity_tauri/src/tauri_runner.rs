//! Tauri 2 WebKitGTK Window Runner and Snapshot Orchestrator.

use std::fs;
use std::thread;
use std::time::Duration;
use tauri::{WebviewUrl, WebviewWindowBuilder};
use crate::fixture::{get_fixture_html, ParityLevel};

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
