use strata::prelude::*;
use strata_dom::View;
use strata_platform::{
    CursorIcon, KeyCode, Modifiers, MouseButton, PlatformEvent, SingleInstanceLock,
    SingleInstanceStatus,
};
use std::sync::{Arc, Mutex};

#[component]
fn UserSearchInputFixture() -> View {
    let query_str = "".to_string();
    rsx! {
        <div class="p-2.5 pb-1.5 bg-[#1f1f1d] border-b border-[#3a3a36]">
            <div class="w-full relative flex items-center bg-[#232321] border border-[#3a3a36] rounded-lg px-3 py-2 cursor-text">
                <svg class="w-3.5 h-3.5 text-[#98958c] mr-2.5 shrink-0" viewBox="0 0 24 24" fill="none">
                    <circle cx="11" cy="11" r="8" stroke="currentColor" stroke-width="2" />
                    <line x1="21" y1="21" x2="16.65" y2="16.65" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
                </svg>
                <input
                    type="text"
                    value={query_str.clone()}
                    placeholder="Search engines, tools, circuits..."
                    class="w-full bg-transparent text-xs text-[#ece9e1] placeholder-[#98958c] outline-none font-sans"
                />
            </div>
        </div>
    }
}

#[component]
fn InteractiveButtonAndPointerFixture() -> View {
    rsx! {
        <div class="flex flex-col p-4 w-[600px] h-[400px]">
            <button
                class="px-4 py-2 bg-blue-600 text-white rounded cursor-pointer"
                on_click={|_| {}}
            >
                <span>"Action Button"</span>
            </button>
            <div class="mt-4 p-4 bg-gray-800 cursor-text">
                <input type="text" value="editable text" class="w-full" />
            </div>
            <div class="mt-4 p-4 bg-gray-900">
                <span>"Regular Text Area"</span>
            </div>
        </div>
    }
}

#[test]
fn test_search_input_container_click_and_caret_focus() {
    let view = UserSearchInputFixture();
    let mut runtime = AppRuntime::new(view, 800.0, 200.0, 1.0);

    assert!(runtime.active_focus.is_none());

    // Click inside the outer container (padding area near the SVG icon)
    let click_ev = PlatformEvent::MouseDown {
        x: 18.0,
        y: 18.0,
        button: MouseButton::Left,
    };
    let rebuild = runtime.step_event(&click_ev);
    assert!(rebuild);
    assert!(runtime.active_focus.is_some(), "Clicking cursor-text container should focus child input");

    // Check display list includes caret draw rect when focused
    runtime.compile_display_list();
    let has_caret = runtime.display_list.commands.iter().any(|cmd| {
        if let strata_render::compositor::DrawCommand::DrawRect { bounds, .. } = cmd {
            // Caret width is 1.5px
            bounds.width <= 2.0 && bounds.height > 8.0
        } else {
            false
        }
    });
    assert!(has_caret, "Focused input must render a vertical caret rect");
}

#[test]
fn test_search_input_full_keyboard_editing_and_shortcuts() {
    let view = UserSearchInputFixture();
    let mut runtime = AppRuntime::new(view, 800.0, 200.0, 1.0);

    // Focus input
    runtime.step_event(&PlatformEvent::MouseDown {
        x: 40.0,
        y: 18.0,
        button: MouseButton::Left,
    });
    assert!(runtime.active_focus.is_some());

    // Type "circuits"
    for ch in "circuits".chars() {
        runtime.step_event(&PlatformEvent::ReceivedCharacter(ch));
    }
    assert_eq!(
        runtime.active_focus.as_ref().unwrap().state.text.get(),
        "circuits"
    );

    // Left arrow navigation (move left 3 times)
    let left_key = PlatformEvent::KeyDown {
        key: KeyCode::ArrowLeft,
        modifiers: Modifiers::default(),
    };
    runtime.step_event(&left_key);
    runtime.step_event(&left_key);
    runtime.step_event(&left_key);

    let active = runtime.active_focus.as_ref().unwrap();
    assert_eq!(active.state.cursor_index.get(), 5);
    assert_eq!(active.state.selection_anchor.get(), None);

    // Shift + Left Arrow (range selection of 2 characters: "cu")
    let shift_left = PlatformEvent::KeyDown {
        key: KeyCode::ArrowLeft,
        modifiers: Modifiers {
            shift: true,
            ctrl: false,
            alt: false,
            meta: false,
        },
    };
    runtime.step_event(&shift_left);
    runtime.step_event(&shift_left);

    let active = runtime.active_focus.as_ref().unwrap();
    assert_eq!(active.state.cursor_index.get(), 3);
    assert_eq!(active.state.selection_anchor.get(), Some(5));

    // Ctrl + A (Select All)
    let ctrl_a = PlatformEvent::KeyDown {
        key: KeyCode::KeyA,
        modifiers: Modifiers {
            shift: false,
            ctrl: true,
            alt: false,
            meta: false,
        },
    };
    runtime.step_event(&ctrl_a);
    let active = runtime.active_focus.as_ref().unwrap();
    assert_eq!(active.state.cursor_index.get(), 8);
    assert_eq!(active.state.selection_anchor.get(), Some(0));

    // Ctrl + C (Copy)
    let ctrl_c = PlatformEvent::KeyDown {
        key: KeyCode::KeyC,
        modifiers: Modifiers {
            shift: false,
            ctrl: true,
            alt: false,
            meta: false,
        },
    };
    runtime.step_event(&ctrl_c);

    // Right Arrow to clear selection and move to end
    let right_key = PlatformEvent::KeyDown {
        key: KeyCode::ArrowRight,
        modifiers: Modifiers::default(),
    };
    runtime.step_event(&right_key);

    // Ctrl + V (Paste) -> "circuitscircuits"
    let ctrl_v = PlatformEvent::KeyDown {
        key: KeyCode::KeyV,
        modifiers: Modifiers {
            shift: false,
            ctrl: true,
            alt: false,
            meta: false,
        },
    };
    runtime.step_event(&ctrl_v);
    assert_eq!(
        runtime.active_focus.as_ref().unwrap().state.text.get(),
        "circuitscircuits"
    );

    // Ctrl + Z (Undo)
    let ctrl_z = PlatformEvent::KeyDown {
        key: KeyCode::KeyZ,
        modifiers: Modifiers {
            shift: false,
            ctrl: true,
            alt: false,
            meta: false,
        },
    };
    runtime.step_event(&ctrl_z);
    assert_eq!(
        runtime.active_focus.as_ref().unwrap().state.text.get(),
        "circuits"
    );

    // Ctrl + Y (Redo)
    let ctrl_y = PlatformEvent::KeyDown {
        key: KeyCode::KeyY,
        modifiers: Modifiers {
            shift: false,
            ctrl: true,
            alt: false,
            meta: false,
        },
    };
    runtime.step_event(&ctrl_y);
    assert_eq!(
        runtime.active_focus.as_ref().unwrap().state.text.get(),
        "circuitscircuits"
    );

    // Ctrl + Backspace (Delete word backward)
    let ctrl_backspace = PlatformEvent::KeyDown {
        key: KeyCode::Backspace,
        modifiers: Modifiers {
            shift: false,
            ctrl: true,
            alt: false,
            meta: false,
        },
    };
    runtime.step_event(&ctrl_backspace);
    assert_eq!(
        runtime.active_focus.as_ref().unwrap().state.text.get(),
        ""
    );
}

#[test]
fn test_automatic_mouse_cursor_switching() {
    let view = InteractiveButtonAndPointerFixture();
    let runtime = AppRuntime::new(view, 800.0, 600.0, 1.0);

    // Initially cursor is Default
    assert_eq!(runtime.current_cursor, CursorIcon::Default);

    // Hover over action button (y=20)
    let cur_btn = AppRuntime::hit_test_cursor(&runtime.root_blueprint, &runtime.layout_root, 30.0, 20.0);
    assert_eq!(cur_btn, CursorIcon::Pointer, "Hovering button should resolve CursorIcon::Pointer");

    // Hover over input container (y=80)
    let cur_input = AppRuntime::hit_test_cursor(&runtime.root_blueprint, &runtime.layout_root, 30.0, 80.0);
    assert_eq!(cur_input, CursorIcon::Text, "Hovering cursor-text container should resolve CursorIcon::Text");

    // Hover over normal text area (y=160)
    let cur_normal = AppRuntime::hit_test_cursor(&runtime.root_blueprint, &runtime.layout_root, 30.0, 160.0);
    assert_eq!(cur_normal, CursorIcon::Default, "Hovering standard container should resolve CursorIcon::Default");
}

#[test]
fn test_single_instance_lock_mutex_and_ipc_forwarding() {
    let test_app_id = "com.strata.test_single_instance_ipc";
    let received_args = Arc::new(Mutex::new(Vec::new()));
    let received_args_clone = received_args.clone();

    // Primary instance acquires the lock
    let status1 = SingleInstanceLock::acquire(test_app_id, &["--main".to_string()], move |args| {
        let mut guard = received_args_clone.lock().unwrap();
        *guard = args;
    });

    assert!(status1.is_ok());
    let _lock = match status1.unwrap() {
        SingleInstanceStatus::Primary(lock) => lock,
        SingleInstanceStatus::SecondaryExited => panic!("First instance must be Primary"),
    };

    // Secondary instance launches with new arguments
    let secondary_args = vec!["--focus".to_string(), "--url".to_string(), "https://rust-lang.org".to_string()];
    let status2 = SingleInstanceLock::acquire(test_app_id, &secondary_args, |_| {});

    assert!(status2.is_ok());
    match status2.unwrap() {
        SingleInstanceStatus::SecondaryExited => {
            // Expected: secondary forwards args and signals exit
        }
        SingleInstanceStatus::Primary(_) => panic!("Second instance must not become Primary"),
    }

    // Give IPC listener thread a moment to process the socket payload
    std::thread::sleep(std::time::Duration::from_millis(50));

    let args = received_args.lock().unwrap().clone();
    assert_eq!(args, secondary_args, "Primary instance must receive forwarded arguments from secondary process");
}
