//! Strata Native Web Browser Entry Point.
//!
//! A high-performance, multi-tab desktop GUI web browser written in pure Rust that
//! renders web documents directly through Strata's native layout engine and WebTranspiler,
//! completely free of Chromium, WebKit, or JavaScript engine overhead.

#![allow(non_snake_case, dead_code, unused_variables, unused_imports)]

use strata::prelude::*;
use strata_browser::components::BrowserApp;
use strata_browser::state::BrowserState;

fn main() {
    println!("[*] Starting Strata Native Web Browser GUI Application...");

    let state = BrowserState::new();

    // Check if initial URL was passed as command line argument
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 && !args[1].starts_with('-') {
        state.navigate(&args[1]);
    }

    // Configure Native Desktop Window
    let mut config = WindowConfig::default();
    config.title = "Strata Browser (Zero-Chromium Architecture)".to_string();
    config.width = 1280;
    config.height = 800;

    // Check for headless or CI test mode
    if args.iter().any(|a| a == "--headless" || a == "--ci") {
        println!("[*] Running in headless verification mode...");
        let (_runtime, _window, presenter) = strata::launch_window(
            move || BrowserApp(state.clone()),
            config,
        );
        println!(
            "[OK] Headless GUI render verified: {}x{} physical pixels, {} bytes backing buffer.",
            presenter.width,
            presenter.height,
            presenter.backing_buffer.len()
        );
        return;
    }

    #[cfg(target_os = "linux")]
    {
        println!("[+] Connecting to native Linux display server with GTK/Cairo high-fidelity presentation (DISPLAY: {})...", std::env::var("DISPLAY").unwrap_or_else(|_| ":0".to_string()));
        let render_state = state.clone();
        let event_state = state.clone();

        let res = strata::launch_gtk_with_events(
            move || BrowserApp(render_state.clone()),
            config.clone(),
            move |event, _runtime| {
                match event {
                    PlatformEvent::MouseDown { button: MouseButton::Middle, x, y } => {
                        event_state.handle_middle_click(*x, *y)
                    }
                    PlatformEvent::KeyDown { key, .. } => {
                        match key {
                            KeyCode::Enter => {
                                let target = event_state.url_input.get();
                                event_state.navigate(&target);
                                true
                            }
                            KeyCode::Backspace => {
                                let mut current = event_state.url_input.get();
                                if !current.is_empty() {
                                    current.pop();
                                    event_state.url_input.set(current);
                                    true
                                } else {
                                    false
                                }
                            }
                            KeyCode::Escape => {
                                if let Some(tab) = event_state.active_tab() {
                                    event_state.url_input.set(tab.url);
                                    true
                                } else {
                                    false
                                }
                            }
                            _ => false,
                        }
                    }
                    PlatformEvent::ReceivedCharacter(ch) => {
                        if !ch.is_control() {
                            let mut current = event_state.url_input.get();
                            current.push(*ch);
                            event_state.url_input.set(current);
                            true
                        } else {
                            false
                        }
                    }
                    _ => false,
                }
            },
        );

        if let Err(err) = res {
            eprintln!("[!] Failed to initialize GTK desktop window: {}. Falling back to software surface presenter.", err);
            let (_runtime, _window, presenter) = strata::launch_window(
                move || BrowserApp(state.clone()),
                config,
            );
            println!("[OK] Surface presenter initialized: {}x{}.", presenter.width, presenter.height);
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        let (_runtime, _window, presenter) = strata::launch_window(
            move || BrowserApp(state.clone()),
            config,
        );
        println!("[OK] Strata Native Browser surface presenter active: {}x{}.", presenter.width, presenter.height);
    }
}
