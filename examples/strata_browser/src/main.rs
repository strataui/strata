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
    let config = WindowConfig {
        title: "Strata Browser (Zero-Chromium Architecture)".to_string(),
        width: 1280,
        height: 800,
        decoration: WindowDecoration::ClientSideDecorations,
        ..Default::default()
    };

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

        // Initialize Generalized Shortcut Manager
        let mut shortcuts = strata_platform::shortcuts::ShortcutManager::new();

        // DevTools Suite Shortcuts
        let s = state.clone();
        shortcuts.register("F12", move || {
            s.toggle_devtools();
            true
        });

        let s = state.clone();
        shortcuts.register("Ctrl+Shift+I", move || {
            s.toggle_devtools();
            true
        });

        let s = state.clone();
        shortcuts.register("Ctrl+Shift+J", move || {
            s.open_devtools(strata_browser::state::DevToolsTab::Console);
            true
        });

        let s = state.clone();
        shortcuts.register("Ctrl+Shift+C", move || {
            s.toggle_inspect();
            true
        });

        // Tab & Navigation Shortcuts
        let s = state.clone();
        shortcuts.register("Ctrl+T", move || {
            s.new_tab("strata://newtab");
            true
        });

        let s = state.clone();
        shortcuts.register("Ctrl+W", move || {
            if let Some(tab) = s.active_tab() {
                s.close_tab(tab.id);
                true
            } else {
                false
            }
        });

        let s = state.clone();
        shortcuts.register("Ctrl+R", move || {
            s.reload();
            true
        });

        let s = state.clone();
        shortcuts.register("F5", move || {
            s.reload();
            true
        });

        let s = state.clone();
        shortcuts.register("Ctrl+L", move || {
            s.start_editing_omnibox();
            true
        });

        let s = state.clone();
        shortcuts.register("Alt+D", move || {
            s.start_editing_omnibox();
            true
        });

        let s = state.clone();
        shortcuts.register("Alt+Left", move || {
            s.go_back();
            true
        });

        let s = state.clone();
        shortcuts.register("Alt+Right", move || {
            s.go_forward();
            true
        });

        let s = state.clone();
        shortcuts.register("Ctrl+Shift+T", move || {
            s.reopen_closed_tab()
        });

        let s = state.clone();
        shortcuts.register("Ctrl+D", move || {
            s.open_bookmark_editor(None);
            true
        });

        let res = strata::launch_gtk_with_events(
            move || BrowserApp(render_state.clone()),
            config.clone(),
            move |event, _runtime| {
                // First attempt shortcut dispatch via generalized ShortcutManager
                if shortcuts.dispatch(event) {
                    return true;
                }

                match event {
                    PlatformEvent::WindowMaximized(max) => {
                        event_state.is_window_maximized.set(*max);
                        true
                    }
                    PlatformEvent::DoubleClick { button: MouseButton::Left, .. } => {
                        #[cfg(target_os = "linux")]
                        {
                            let is_max = strata_platform::gtk_is_maximized();
                            event_state.is_window_maximized.set(is_max);
                        }
                        true
                    }
                    PlatformEvent::MouseDown { button: MouseButton::Left, x, y } => {
                        event_state.handle_global_click(*x, *y)
                    }
                    PlatformEvent::MouseDown { button: MouseButton::Middle, x, y } => {
                        event_state.handle_middle_click(*x, *y)
                    }
                    PlatformEvent::KeyDown { key, .. } => {
                        if *key == KeyCode::Escape {
                            if event_state.is_context_menu_open.get() {
                                event_state.close_context_menu();
                                return true;
                            }
                            if event_state.bookmark_editor.get().is_some() {
                                event_state.close_bookmark_editor();
                                return true;
                            }
                            if event_state.is_settings_modal_open.get() {
                                event_state.close_settings();
                                return true;
                            }
                            if event_state.is_kebab_menu_open.get() {
                                event_state.close_kebab_menu();
                                return true;
                            }
                        }
                        false
                    }
                    PlatformEvent::AnimationTick => {
                        event_state.poll_worker_responses()
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
        let (mut runtime, mut window, mut presenter) = strata::launch_window(
            move || BrowserApp(state.clone()),
            config,
        );
        println!("[OK] Strata Native Browser surface presenter active: {}x{}.", presenter.width, presenter.height);
        let mut compositor = strata_render::SwapchainCompositor::new(presenter.width, presenter.height, 1.0);
        let mut event_loop = strata_platform::MockEventLoop::new();
        runtime.run_with_window(&mut window, &mut event_loop, &mut compositor, &mut presenter);
    }
}
