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

    // Configure Native Desktop Window with CSD Frameless mode
    let mut config = WindowConfig::default();
    config.title = "Strata Browser (Zero-Chromium Architecture)".to_string();
    config.width = 1280;
    config.height = 800;
    config.decoration = WindowDecoration::ClientSideDecorations;

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
                    PlatformEvent::KeyDown { key, modifiers } => {
                        // Chromium Browser Shortcut Keybinds
                        if modifiers.ctrl {
                            match key {
                                KeyCode::KeyT => {
                                    event_state.new_tab("strata://newtab");
                                    return true;
                                }
                                KeyCode::KeyW => {
                                    if let Some(tab) = event_state.active_tab() {
                                        event_state.close_tab(tab.id);
                                        return true;
                                    }
                                }
                                KeyCode::KeyR => {
                                    event_state.reload();
                                    return true;
                                }
                                KeyCode::KeyL => {
                                    event_state.start_editing_omnibox();
                                    return true;
                                }
                                KeyCode::KeyA => {
                                    event_state.omnibox_selected.set(true);
                                    return true;
                                }
                                KeyCode::KeyC => {
                                    let text = event_state.url_input.get();
                                    strata::gtk_set_clipboard_text(&text);
                                    return true;
                                }
                                KeyCode::KeyX => {
                                    let text = event_state.url_input.get();
                                    strata::gtk_set_clipboard_text(&text);
                                    event_state.url_input.set(String::new());
                                    event_state.omnibox_selected.set(false);
                                    return true;
                                }
                                KeyCode::KeyV => {
                                    if let Some(clip) = strata::gtk_get_clipboard_text() {
                                        if event_state.omnibox_selected.get() {
                                            event_state.url_input.set(clip);
                                            event_state.omnibox_selected.set(false);
                                        } else {
                                            let mut cur = event_state.url_input.get();
                                            cur.push_str(&clip);
                                            event_state.url_input.set(cur);
                                        }
                                        return true;
                                    }
                                }
                                _ => {}
                            }
                        }

                        if modifiers.alt {
                            match key {
                                KeyCode::KeyD => {
                                    event_state.start_editing_omnibox();
                                    return true;
                                }
                                KeyCode::ArrowLeft => {
                                    event_state.go_back();
                                    return true;
                                }
                                KeyCode::ArrowRight => {
                                    event_state.go_forward();
                                    return true;
                                }
                                _ => {}
                            }
                        }

                        match key {
                            KeyCode::F5 => {
                                event_state.reload();
                                true
                            }
                            KeyCode::Enter => {
                                event_state.commit_omnibox();
                                true
                            }
                            KeyCode::Backspace => {
                                event_state.handle_backspace()
                            }
                            KeyCode::Delete => {
                                if event_state.omnibox_selected.get() {
                                    event_state.url_input.set(String::new());
                                    event_state.omnibox_selected.set(false);
                                    true
                                } else {
                                    false
                                }
                            }
                            KeyCode::Escape => {
                                event_state.cancel_editing_omnibox();
                                true
                            }
                            _ => false,
                        }
                    }
                    PlatformEvent::ReceivedCharacter(ch) => {
                        if !ch.is_control() {
                            event_state.handle_key_input(*ch);
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
