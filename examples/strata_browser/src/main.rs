//! Strata Native Web Browser Example Harness.
//!
//! A zero-Chromium desktop web browser harness demonstrating pure-Rust HTML5 parsing,
//! CSS layout, and reactive presentation without WebKit, Blink, or V8 engine overhead.

#![allow(non_snake_case, dead_code, unused_variables, unused_imports)]

mod app;
mod state;

use app::BrowserApp;
use state::BrowserState;
use strata::prelude::*;

fn main() {
    println!("[*] Starting Strata Native Web Browser GUI Harness...");

    let state = BrowserState::new();

    // Check command-line arguments for URL navigation or headless mode
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 && !args[1].starts_with('-') {
        state.navigate(&args[1]);
    }

    let is_headless = args.iter().any(|a| a == "--headless" || a == "--ci");

    let config = WindowConfig {
        title: "Strata Browser (Zero-Chromium Architecture)".to_string(),
        width: 1280,
        height: 800,
        decoration: WindowDecoration::ClientSideDecorations,
        ..Default::default()
    };

    if is_headless {
        println!("[*] Running in headless verification mode...");
        let render_state = state.clone();
        let runtime = strata::launch_headless(
            move || BrowserApp(render_state),
            config.width as f32,
            config.height as f32,
        );
        let cmd_count = runtime.display_list.commands.len();
        println!(
            "[OK] Headless GUI render verified: viewport {}x{}, {} commands presented.",
            runtime.viewport_width,
            runtime.viewport_height,
            cmd_count
        );
        assert!(cmd_count > 0, "Display list must contain rendered commands");
        return;
    }

    #[cfg(target_os = "linux")]
    {
        println!("[+] Connecting to native Linux display server with GTK/Cairo presentation...");
        let render_state = state.clone();
        let _ = strata::launch_gtk(move || BrowserApp(render_state.clone()), config);
    }

    #[cfg(not(target_os = "linux"))]
    {
        let render_state = state.clone();
        strata::app().with_window(config).build(move || BrowserApp(render_state.clone())).run();
    }
}
