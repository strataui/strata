#![allow(clippy::field_reassign_with_default)]

use strata::prelude::*;
use strata_dom::View;
use strata_platform::{MockEventLoop, MockPlatformWindow, PlatformEvent, SoftwareSurfacePresenter, SwapchainCompositor, SwapchainFormat, WindowConfig};

#[component]
fn TestWindowApp() -> View {
    rsx! {
        <div class="flex flex-col w-full h-full bg-[#0a0a0a]">
            <header class="flex h-[60px] bg-[#1a1a1a]">
                <span class="text-white">"Strata Live Window"</span>
            </header>
            <main class="flex flex-1 p-4">
                <div class="flex w-[200px] h-[100px] bg-[#2a2a2a]">
                    <span>"Content Card"</span>
                </div>
            </main>
        </div>
    }
}

#[test]
fn test_strata_launch_window_initialization_and_first_frame() {
    let mut config = WindowConfig::default();
    config.title = "Strata Test Application".to_string();
    config.width = 1280;
    config.height = 720;

    let (runtime, window, presenter) = strata::launch_window(TestWindowApp, config);

    assert_eq!(window.title(), "Strata Test Application");
    assert_eq!(window.size(), (1280, 720));
    assert_eq!(presenter.width, 1280);
    assert_eq!(presenter.height, 720);
    assert_eq!(presenter.presented_frames, 1);
    assert_eq!(presenter.format, SwapchainFormat::Bgra8Unorm);

    // Verify initial layout & display list
    assert_eq!(runtime.viewport_width, 1280.0);
    assert_eq!(runtime.viewport_height, 720.0);
    assert!(!runtime.display_list.commands.is_empty());
}

#[test]
fn test_app_runtime_step_event_resize_and_scale() {
    let view = TestWindowApp();
    let mut runtime = AppRuntime::new(view, 800.0, 600.0, 1.0);
    assert_eq!(runtime.viewport_width, 800.0);
    assert_eq!(runtime.viewport_height, 600.0);
    assert_eq!(runtime.dpr, 1.0);

    // Resize event
    let resize_event = PlatformEvent::WindowResized { width: 1920, height: 1080 };
    let dirty = runtime.step_event(&resize_event);
    assert!(dirty);
    assert_eq!(runtime.viewport_width, 1920.0);
    assert_eq!(runtime.viewport_height, 1080.0);

    // Scale factor event
    let scale_event = PlatformEvent::ScaleFactorChanged { scale_factor: 2.0 };
    let dirty_scale = runtime.step_event(&scale_event);
    assert!(dirty_scale);
    assert_eq!(runtime.dpr, 2.0);

    // Focus event should not trigger dirty layout
    let focus_event = PlatformEvent::WindowFocused(true);
    assert!(!runtime.step_event(&focus_event));
}

#[test]
fn test_app_runtime_run_with_window_event_loop() {
    let view = TestWindowApp();
    let mut runtime = AppRuntime::new(view, 800.0, 600.0, 1.0);

    let mut window = MockPlatformWindow::new_mock();
    let mut event_loop = MockEventLoop::new();
    let mut compositor = SwapchainCompositor::new(800, 600, 1.0);
    let mut presenter = SoftwareSurfacePresenter::new(800, 600, SwapchainFormat::Bgra8Unorm);

    // Queue simulated OS events
    event_loop.queue_event(PlatformEvent::WindowResized { width: 1024, height: 768 });
    event_loop.queue_event(PlatformEvent::WindowFocused(true));
    event_loop.queue_event(PlatformEvent::WindowCloseRequested);

    runtime.run_with_window(&mut window, &mut event_loop, &mut compositor, &mut presenter);

    // Verify final state
    assert_eq!(runtime.viewport_width, 1024.0);
    assert_eq!(runtime.viewport_height, 768.0);
    assert!(presenter.presented_frames >= 2); // Initial frame + resize frame
}

#[test]
fn test_strata_low_level_app_builder_and_lifecycle_hooks() {
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::Arc;

    let started = Arc::new(AtomicBool::new(false));
    let frame_counter = Arc::new(AtomicU64::new(0));
    let event_intercepted = Arc::new(AtomicBool::new(false));
    let draw_injected = Arc::new(AtomicBool::new(false));
    let exited = Arc::new(AtomicBool::new(false));

    let started_clone = Arc::clone(&started);
    let frame_counter_clone = Arc::clone(&frame_counter);
    let event_intercepted_clone = Arc::clone(&event_intercepted);
    let draw_injected_clone = Arc::clone(&draw_injected);
    let exited_clone = Arc::clone(&exited);

    let mut app = StrataApp::builder()
        .with_window(WindowConfig {
            title: "Low-Level Custom Studio".to_string(),
            width: 1440,
            height: 900,
            resizable: true,
            transparent: false,
            ..Default::default()
        })
        .with_dpr(1.5)
        .on_start(move |_runtime| {
            started_clone.store(true, Ordering::SeqCst);
        })
        .on_event(move |event, _ctrl, _runtime| {
            if matches!(event, PlatformEvent::WindowResized { .. }) {
                event_intercepted_clone.store(true, Ordering::SeqCst);
            }
        })
        .on_draw(move |display_list| {
            draw_injected_clone.store(true, Ordering::SeqCst);
            display_list.push(strata_render::compositor::DrawCommand::DrawRect {
                bounds: strata_layout::Rect::new(0.0, 0.0, 50.0, 50.0),
                color: strata_style::types::Color::rgba(255, 0, 0, 255),
            });
        })
        .on_frame(move |_frame_info, _runtime| {
            frame_counter_clone.fetch_add(1, Ordering::SeqCst);
        })
        .on_exit(move |_runtime| {
            exited_clone.store(true, Ordering::SeqCst);
        })
        .build(TestWindowApp);

    assert_eq!(app.window_config.title, "Low-Level Custom Studio");
    assert_eq!(app.runtime.dpr, 1.5);

    // 1. Test step_frame and on_draw hook
    let dl = app.step_frame();
    assert!(draw_injected.load(Ordering::SeqCst));
    assert!(!dl.commands.is_empty());

    // 2. Test event dispatching and on_event hook
    let resized = PlatformEvent::WindowResized { width: 1600, height: 1000 };
    app.dispatch_event(&resized);
    assert!(event_intercepted.load(Ordering::SeqCst));
    assert_eq!(app.runtime.viewport_width, 1600.0);

    // 3. Test render_to_buffer
    let buffer = app.render_to_buffer().unwrap();
    assert_eq!(buffer.len(), (1440 * 900 * 4) as usize);

    // 4. Test run_headless
    let frames = app.run_headless(5);
    assert_eq!(frames.len(), 5);
    assert!(started.load(Ordering::SeqCst));
    assert_eq!(frame_counter.load(Ordering::SeqCst), 5);
    assert!(exited.load(Ordering::SeqCst));
}

#[test]
fn test_runtime_native_input_focus_and_key_routing() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use strata::prelude::*;

    let url_val = Arc::new(std::sync::Mutex::new("strata://newtab".to_string()));
    let search_val = Arc::new(std::sync::Mutex::new(String::new()));
    let url_blurred = Arc::new(AtomicBool::new(false));
    let search_focused = Arc::new(AtomicBool::new(false));

    let u_val = url_val.clone();
    let s_val = search_val.clone();
    let u_blur = url_blurred.clone();
    let s_foc = search_focused.clone();

    let root_view = rsx! {
        <div class="flex flex-col w-[800px] h-[600px] p-4 gap-4">
            <input
                type="text"
                class="w-[400px] h-9 px-3 bg-neutral-900 text-white"
                value="strata://newtab"
                oninput={move |val: String| {
                    *u_val.lock().unwrap() = val;
                }}
                onblur={move |_| {
                    u_blur.store(true, Ordering::SeqCst);
                }}
            />
            <input
                type="text"
                class="w-[400px] h-8 px-3 bg-neutral-900 text-white"
                placeholder="Search..."
                oninput={move |val: String| {
                    *s_val.lock().unwrap() = val;
                }}
                onfocus={move |_| {
                    s_foc.store(true, Ordering::SeqCst);
                }}
            />
            <button type="button" class="w-24 h-8 bg-blue-600">
                {"Submit"}
            </button>
        </div>
    };

    let mut runtime = AppRuntime::new(root_view, 800.0, 600.0, 1.0);

    // Initially, no input is focused
    assert!(runtime.active_focus.is_none());

    // 1. Click on the first input (inside x: 16..416, y: 16..52)
    let focused_first = runtime.dispatch_pointer_down(50.0, 30.0);
    assert!(focused_first);
    assert!(runtime.active_focus.is_some());
    assert_eq!(runtime.active_focus.as_ref().unwrap().state.text.get(), "strata://newtab");

    // 2. Type '!' into the first input at the clicked caret position
    let key_ev = PlatformEvent::ReceivedCharacter('!');
    assert!(runtime.handle_key_event(&key_ev));
    assert_eq!(*url_val.lock().unwrap(), "str!ata://newtab");

    // Press End key and type '?'
    assert!(runtime.handle_key_event(&PlatformEvent::KeyDown {
        key: strata_platform::KeyCode::End,
        modifiers: strata_platform::Modifiers::default(),
    }));
    assert!(runtime.handle_key_event(&PlatformEvent::ReceivedCharacter('?')));
    assert_eq!(*url_val.lock().unwrap(), "str!ata://newtab?");

    // 3. Click on the second input (inside y: 68..100)
    let focused_second = runtime.dispatch_pointer_down(50.0, 80.0);
    assert!(focused_second);
    assert!(url_blurred.load(Ordering::SeqCst), "First input must have received blur event");
    assert!(search_focused.load(Ordering::SeqCst), "Second input must have received focus event");

    // 4. Type 'rust' into the second input
    for ch in "rust".chars() {
        assert!(runtime.handle_key_event(&PlatformEvent::ReceivedCharacter(ch)));
    }
    assert_eq!(*search_val.lock().unwrap(), "rust");
    // Verify first input value remains unchanged
    assert_eq!(*url_val.lock().unwrap(), "str!ata://newtab?");

    // 5. Test Backspace on second input
    assert!(runtime.handle_key_event(&PlatformEvent::KeyDown {
        key: strata_platform::KeyCode::Backspace,
        modifiers: strata_platform::Modifiers::default(),
    }));
    assert_eq!(*search_val.lock().unwrap(), "rus");

    // 6. Click on page background / whitespace (x: 500.0, y: 500.0)
    let unfocused = runtime.dispatch_pointer_down(500.0, 500.0);
    assert!(unfocused);
    assert!(runtime.active_focus.is_none(), "Active focus must be cleared on outside click");

    // 7. Typing while unfocused must not be consumed
    let stray_key = PlatformEvent::ReceivedCharacter('x');
    assert!(!runtime.handle_key_event(&stray_key), "Keystrokes while unfocused must return false");
}

#[test]
fn test_multi_input_per_field_undo_redo_and_selection() {
    let input_a_val = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let input_b_val = std::sync::Arc::new(std::sync::Mutex::new(String::new()));

    let a_val = input_a_val.clone();
    let b_val = input_b_val.clone();

    let root_view = rsx! {
        <div class="flex flex-col w-[800px] h-[600px] p-4 gap-4">
            <input
                type="text"
                id="field-a"
                class="w-[400px] h-9 px-3 bg-neutral-900 text-white"
                placeholder="Field A"
                oninput={move |val: String| {
                    *a_val.lock().unwrap() = val;
                }}
            />
            <input
                type="text"
                id="field-b"
                class="w-[400px] h-9 px-3 bg-neutral-900 text-white"
                placeholder="Field B"
                oninput={move |val: String| {
                    *b_val.lock().unwrap() = val;
                }}
            />
        </div>
    };

    let mut runtime = AppRuntime::new(root_view, 800.0, 600.0, 1.0);

    // 1. Focus Field A (at y=30)
    assert!(runtime.dispatch_pointer_down(50.0, 30.0));
    assert!(runtime.dispatch_pointer_up(50.0, 30.0));
    assert_eq!(runtime.active_focus.as_ref().unwrap().input_id.as_deref(), Some("field-a"));

    // Type "abc" into Field A
    for ch in "abc".chars() {
        assert!(runtime.handle_key_event(&PlatformEvent::ReceivedCharacter(ch)));
    }
    assert_eq!(*input_a_val.lock().unwrap(), "abc");

    // Undo twice in Field A: "abc" -> "ab" -> "a"
    let ctrl_z = PlatformEvent::KeyDown {
        key: strata_platform::KeyCode::KeyZ,
        modifiers: strata_platform::Modifiers { ctrl: true, ..Default::default() },
    };
    assert!(runtime.handle_key_event(&ctrl_z));
    assert_eq!(*input_a_val.lock().unwrap(), "ab");
    assert!(runtime.handle_key_event(&ctrl_z));
    assert_eq!(*input_a_val.lock().unwrap(), "a");

    // Redo once with Ctrl+Y: "a" -> "ab"
    let ctrl_y = PlatformEvent::KeyDown {
        key: strata_platform::KeyCode::KeyY,
        modifiers: strata_platform::Modifiers { ctrl: true, ..Default::default() },
    };
    assert!(runtime.handle_key_event(&ctrl_y));
    assert_eq!(*input_a_val.lock().unwrap(), "ab");

    // Redo once with Ctrl+Shift+Z: "ab" -> "abc"
    let ctrl_shift_z = PlatformEvent::KeyDown {
        key: strata_platform::KeyCode::KeyZ,
        modifiers: strata_platform::Modifiers { ctrl: true, shift: true, ..Default::default() },
    };
    assert!(runtime.handle_key_event(&ctrl_shift_z));
    assert_eq!(*input_a_val.lock().unwrap(), "abc");

    // 2. Focus Field B (at y=80)
    assert!(runtime.dispatch_pointer_down(50.0, 80.0));
    assert!(runtime.dispatch_pointer_up(50.0, 80.0));
    assert_eq!(runtime.active_focus.as_ref().unwrap().input_id.as_deref(), Some("field-b"));

    // Type "xyz" into Field B
    for ch in "xyz".chars() {
        assert!(runtime.handle_key_event(&PlatformEvent::ReceivedCharacter(ch)));
    }
    assert_eq!(*input_b_val.lock().unwrap(), "xyz");

    // Undo once in Field B: "xyz" -> "xy"
    assert!(runtime.handle_key_event(&ctrl_z));
    assert_eq!(*input_b_val.lock().unwrap(), "xy");

    // 3. Switch back to Field A: verify independent history is preserved
    assert!(runtime.dispatch_pointer_down(50.0, 30.0));
    assert!(runtime.dispatch_pointer_up(50.0, 30.0));
    assert_eq!(runtime.active_focus.as_ref().unwrap().input_id.as_deref(), Some("field-a"));
    assert_eq!(*input_a_val.lock().unwrap(), "abc");

    // Undo once in Field A: "abc" -> "ab"
    assert!(runtime.handle_key_event(&ctrl_z));
    assert_eq!(*input_a_val.lock().unwrap(), "ab");

    // Verify Field B was NOT affected by Field A's undo
    assert_eq!(*input_b_val.lock().unwrap(), "xy");

    // Switch back to Field B and Redo: "xy" -> "xyz"
    assert!(runtime.dispatch_pointer_down(50.0, 80.0));
    assert!(runtime.dispatch_pointer_up(50.0, 80.0));
    assert_eq!(runtime.active_focus.as_ref().unwrap().input_id.as_deref(), Some("field-b"));
    assert!(runtime.handle_key_event(&ctrl_y));
    assert_eq!(*input_b_val.lock().unwrap(), "xyz");

    // 4. Test Mouse Drag Text Selection in Field B
    // Press down at x=35, move to x=55
    assert!(runtime.dispatch_pointer_down(35.0, 80.0));
    assert!(runtime.dispatch_pointer_move(65.0, 80.0));
    assert!(runtime.active_focus.as_ref().unwrap().state.has_selection());
    let (sel_start, sel_end) = runtime.active_focus.as_ref().unwrap().state.selection_range().unwrap();
    assert!(sel_start < sel_end);
    assert!(runtime.dispatch_pointer_up(65.0, 80.0));

    // Delete selection with Backspace
    let backspace = PlatformEvent::KeyDown {
        key: strata_platform::KeyCode::Backspace,
        modifiers: strata_platform::Modifiers::default(),
    };
    assert!(runtime.handle_key_event(&backspace));
    // Verify text is shortened
    assert!(input_b_val.lock().unwrap().len() < 3);

    // Undo deletion with Ctrl+Z
    assert!(runtime.handle_key_event(&ctrl_z));
    assert_eq!(*input_b_val.lock().unwrap(), "xyz");
}

#[test]
fn test_caret_blink_timing_and_mouse_move_isolation() {
    let input_val = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let input_clone = input_val.clone();

    let root_view = rsx! {
        <div class="flex flex-col p-4 w-full h-full bg-black">
            <input
                id="search-input"
                class="w-64 h-10 bg-neutral-900 text-white px-3"
                value=""
                oninput={move |val: String| {
                    *input_clone.lock().unwrap() = val;
                }}
            />
        </div>
    };

    let mut runtime = AppRuntime::new(root_view, 800.0, 600.0, 1.0);

    // 1. Focus input
    assert!(runtime.dispatch_pointer_down(50.0, 20.0));
    assert!(runtime.dispatch_pointer_up(50.0, 20.0));
    assert!(runtime.active_focus.is_some());

    // Record baseline blink epoch
    let baseline_epoch = runtime.caret_blink_epoch;
    std::thread::sleep(std::time::Duration::from_millis(10));

    // 2. Mouse move across the screen should NOT reset caret_blink_epoch
    let mouse_move_event = PlatformEvent::MouseMove { x: 300.0, y: 300.0 };
    assert!(!runtime.dispatch_pointer_move(300.0, 300.0));
    assert!(!runtime.handle_key_event(&mouse_move_event));
    assert_eq!(
        runtime.caret_blink_epoch, baseline_epoch,
        "MouseMove must never reset caret_blink_epoch or interrupt blinking cycle"
    );

    // 3. Active typing (ReceivedCharacter) MUST reset caret_blink_epoch
    assert!(runtime.handle_key_event(&PlatformEvent::ReceivedCharacter('a')));
    let typed_epoch = runtime.caret_blink_epoch;
    assert!(
        typed_epoch > baseline_epoch,
        "Active character insertion must reset caret_blink_epoch to guarantee solid caret"
    );

    std::thread::sleep(std::time::Duration::from_millis(10));

    // 4. Moving mouse after typing still must NOT reset caret_blink_epoch
    assert!(!runtime.dispatch_pointer_move(400.0, 400.0));
    assert!(!runtime.handle_key_event(&PlatformEvent::MouseMove { x: 400.0, y: 400.0 }));
    assert_eq!(
        runtime.caret_blink_epoch, typed_epoch,
        "MouseMove after typing must not alter the standardized delay epoch"
    );

    // 5. Navigation key (ArrowLeft) moves caret and resets blink epoch
    let arrow_left = PlatformEvent::KeyDown {
        key: strata_platform::KeyCode::ArrowLeft,
        modifiers: strata_platform::Modifiers::default(),
    };
    assert!(runtime.handle_key_event(&arrow_left));
    assert!(
        runtime.caret_blink_epoch > typed_epoch,
        "Caret navigation key must reset caret_blink_epoch"
    );
}


