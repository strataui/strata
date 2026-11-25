//! C ABI bindings for Strata application lifecycle, `AppRuntime` execution, input dispatch,
//! checked RGBA framebuffer export, PNG snapshot export, and native window launching.

use std::ffi::CStr;
use std::os::raw::c_char;

use strata::runtime::AppRuntime;
use strata_platform::{KeyCode, Modifiers, PlatformEvent, WindowConfig};

use crate::dom::StrataViewHandle;
use crate::error::{
    clear_last_error_internal, ffi_guard, ffi_guard_i32, ffi_guard_ptr, set_last_error,
    StrataStatus,
};
use crate::signal::take_ffi_signals_dirty;

/// Framebuffer geometry metadata returned by `strata_runtime_framebuffer_info`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StrataFramebufferInfo {
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub byte_len: usize,
}

/// Configuration options passed to `strata_launch_native_window`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct StrataWindowOptions {
    pub title: *const c_char,
    pub width: u32,
    pub height: u32,
    pub resizable: bool,
    /// When `true`, runs Strata's deterministic headless pump instead of opening an OS window.
    pub headless: bool,
}

/// Opaque wrapper for a Strata application instance.
pub struct StrataAppHandle {
    pub is_running: bool,
    pub windows: Vec<*mut StrataWindowHandle>,
}

/// Opaque wrapper for a Strata window instance with frame buffer presentation sink.
pub struct StrataWindowHandle {
    pub app: *mut StrataAppHandle,
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub is_visible: bool,
    pub frame_buffer: strata_render::rasterizer::FrameBuffer,
    pub runtime: *mut AppRuntime,
}

static VERSION_CSTR: &[u8] = concat!(env!("CARGO_PKG_VERSION"), "\0").as_bytes();

/// Returns the Strata FFI engine version as a null-terminated UTF-8 C string.
#[no_mangle]
pub extern "C" fn strata_version() -> *const c_char {
    VERSION_CSTR.as_ptr() as *const c_char
}

/// Creates a new Strata application handle.
#[no_mangle]
pub extern "C" fn strata_app_create() -> *mut StrataAppHandle {
    ffi_guard_ptr(|| {
        clear_last_error_internal();
        Box::into_raw(Box::new(StrataAppHandle {
            is_running: true,
            windows: Vec::new(),
        }))
    })
}

/// Destroys a Strata application handle and releases attached window handles.
#[no_mangle]
pub unsafe extern "C" fn strata_app_destroy(app: *mut StrataAppHandle) {
    if !app.is_null() {
        let mut app_box = Box::from_raw(app);
        for win in app_box.windows.drain(..) {
            if !win.is_null() {
                (*win).app = std::ptr::null_mut();
                let _ = Box::from_raw(win);
            }
        }
    }
}

/// Returns `1` if the application is currently running, `0` otherwise.
#[no_mangle]
pub unsafe extern "C" fn strata_app_is_running(app: *const StrataAppHandle) -> i32 {
    if app.is_null() {
        0
    } else if (*app).is_running {
        1
    } else {
        0
    }
}

/// Creates a new window handle attached to `app`.
#[no_mangle]
pub unsafe extern "C" fn strata_window_create(
    app: *mut StrataAppHandle,
    title: *const c_char,
    width: u32,
    height: u32,
) -> *mut StrataWindowHandle {
    ffi_guard_ptr(|| {
        let title_str = if title.is_null() {
            "Strata".to_string()
        } else {
            CStr::from_ptr(title)
                .to_str()
                .unwrap_or("Strata")
                .to_string()
        };

        let win = Box::new(StrataWindowHandle {
            app,
            title: title_str,
            width,
            height,
            is_visible: true,
            frame_buffer: strata_render::rasterizer::FrameBuffer::new(
                width as usize,
                height as usize,
            ),
            runtime: std::ptr::null_mut(),
        });
        let win_ptr = Box::into_raw(win);

        if !app.is_null() {
            (*app).windows.push(win_ptr);
        }

        clear_last_error_internal();
        win_ptr
    })
}

/// Associates an active [`AppRuntime`] with the window handle for layout and rendering.
#[no_mangle]
pub unsafe extern "C" fn strata_window_set_runtime(
    win: *mut StrataWindowHandle,
    runtime: *mut AppRuntime,
) -> i32 {
    ffi_guard_i32(|| {
        if win.is_null() {
            set_last_error("strata_window_set_runtime: window handle is NULL");
            return -1;
        }
        (*win).runtime = runtime;
        clear_last_error_internal();
        0
    })
}

/// Creates an [`AppRuntime`] from a raw Rust [`strata_dom::View`] pointer (legacy signature).
#[no_mangle]
pub unsafe extern "C" fn strata_runtime_create(
    view: *mut strata_dom::View,
    width: f32,
    height: f32,
    dpr: f32,
) -> *mut AppRuntime {
    ffi_guard_ptr(|| {
        if view.is_null() {
            set_last_error("strata_runtime_create: view pointer is NULL");
            return std::ptr::null_mut();
        }
        let view_clone = (*view).clone();
        let runtime = Box::new(AppRuntime::new(
            view_clone,
            width.max(1.0),
            height.max(1.0),
            dpr.max(0.5),
        ));
        clear_last_error_internal();
        Box::into_raw(runtime)
    })
}

/// Creates an [`AppRuntime`] from an opaque [`StrataViewHandle`] built via the C DOM builders.
#[no_mangle]
pub unsafe extern "C" fn strata_runtime_create_from_view(
    view: *const StrataViewHandle,
    width: f32,
    height: f32,
    dpr: f32,
) -> *mut AppRuntime {
    ffi_guard_ptr(|| {
        if view.is_null() {
            set_last_error("strata_runtime_create_from_view: view handle is NULL");
            return std::ptr::null_mut();
        }
        let view_clone = (*view).view.clone();
        let runtime = Box::new(AppRuntime::new(
            view_clone,
            width.max(1.0),
            height.max(1.0),
            dpr.max(0.5),
        ));
        clear_last_error_internal();
        Box::into_raw(runtime)
    })
}

/// Replaces the root [`View`] of a running [`AppRuntime`] in-place while preserving scroll,
/// focus, and text input buffers.
#[no_mangle]
pub unsafe extern "C" fn strata_runtime_update_view(
    runtime: *mut AppRuntime,
    view: *const StrataViewHandle,
) -> StrataStatus {
    ffi_guard(|| {
        if runtime.is_null() || view.is_null() {
            set_last_error("strata_runtime_update_view: runtime or view pointer is NULL");
            return StrataStatus::NullPointer;
        }
        let rt = &mut *runtime;
        rt.update_view((*view).view.clone());
        rt.mark_paint_dirty();
        clear_last_error_internal();
        StrataStatus::Ok
    })
}

/// Resizes the [`AppRuntime`] viewport, recomputes layout, and fires ResizeObservers.
#[no_mangle]
pub unsafe extern "C" fn strata_runtime_resize(
    runtime: *mut AppRuntime,
    width: f32,
    height: f32,
    dpr: f32,
) -> StrataStatus {
    ffi_guard(|| {
        if runtime.is_null() {
            set_last_error("strata_runtime_resize: runtime pointer is NULL");
            return StrataStatus::NullPointer;
        }
        if width <= 0.0 || height <= 0.0 || dpr <= 0.0 {
            set_last_error("strata_runtime_resize: width, height, and dpr must be positive");
            return StrataStatus::InvalidArgument;
        }
        let rt = &mut *runtime;
        rt.viewport_width = width;
        rt.viewport_height = height;
        rt.dpr = dpr;
        rt.unapply_scroll_offsets();
        rt.layout_root.compute_layout(width, height);
        AppRuntime::sync_layout_to_ir(&rt.layout_root, &mut rt.ir_graph);
        rt.apply_viewport_clamps();
        rt.apply_scroll_offsets();
        rt.spatial_index.rebuild(&rt.root_blueprint, &rt.layout_root);
        let _ = rt.dispatch_resize_observers();
        rt.mark_paint_dirty();
        clear_last_error_internal();
        StrataStatus::Ok
    })
}

/// Advances animations, smooth scrolling, timers, and reactive FFI/DOM signals.
/// Writes `true` into `*out_dirty` (if non-NULL) when a new frame should be rendered.
#[no_mangle]
pub unsafe extern "C" fn strata_runtime_tick(
    runtime: *mut AppRuntime,
    out_dirty: *mut bool,
) -> StrataStatus {
    ffi_guard(|| {
        if runtime.is_null() {
            set_last_error("strata_runtime_tick: runtime pointer is NULL");
            return StrataStatus::NullPointer;
        }
        let rt = &mut *runtime;
        let ffi_dirty = take_ffi_signals_dirty();
        let dom_dirty = strata_dom::reactive::take_dirty_signals();
        let dynamic_synced = if ffi_dirty || dom_dirty {
            rt.sync_dynamic_text()
        } else {
            false
        };
        let anim_dirty = rt.poll_animations();
        let scroll_dirty = rt.poll_smooth_scroll();
        let caret_dirty = rt.poll_caret_blink();
        let tooltip_dirty = rt.poll_hover_tooltip();
        let timers_dirty = strata::js::web::tick_timers() > 0;

        let dirty = ffi_dirty
            || dom_dirty
            || dynamic_synced
            || anim_dirty
            || scroll_dirty
            || caret_dirty
            || tooltip_dirty
            || timers_dirty
            || rt.is_paint_dirty();
        if dirty {
            rt.mark_paint_dirty();
        }
        if !out_dirty.is_null() {
            *out_dirty = dirty;
        }
        clear_last_error_internal();
        StrataStatus::Ok
    })
}

/// Dispatches a pointer-move event into `runtime` (updating hover states and drag selection).
#[no_mangle]
pub unsafe extern "C" fn strata_runtime_pointer_move(
    runtime: *mut AppRuntime,
    x: f32,
    y: f32,
    out_dirty: *mut bool,
) -> StrataStatus {
    ffi_guard(|| {
        if runtime.is_null() {
            set_last_error("strata_runtime_pointer_move: runtime pointer is NULL");
            return StrataStatus::NullPointer;
        }
        let rt = &mut *runtime;
        rt.last_mouse_pos = (x, y);
        let action = rt.dispatch_pointer_move_action(x, y);
        let hover_changed = rt.update_hover(x, y);
        let dirty = action.needs_redraw() || hover_changed;
        if dirty {
            rt.mark_paint_dirty();
        }
        if !out_dirty.is_null() {
            *out_dirty = dirty;
        }
        clear_last_error_internal();
        StrataStatus::Ok
    })
}

/// Dispatches a pointer-down and primary/secondary click event at `(x, y)`.
/// `button`: `0` = Left, `1` = Middle, `2` = Right.
#[no_mangle]
pub unsafe extern "C" fn strata_runtime_pointer_down(
    runtime: *mut AppRuntime,
    x: f32,
    y: f32,
    button: i16,
    out_dirty: *mut bool,
) -> StrataStatus {
    ffi_guard(|| {
        if runtime.is_null() {
            set_last_error("strata_runtime_pointer_down: runtime pointer is NULL");
            return StrataStatus::NullPointer;
        }
        let rt = &mut *runtime;
        rt.last_mouse_pos = (x, y);
        if button == 0 {
            rt.is_mouse_down = true;
        }
        let action = rt.dispatch_pointer_down_action_with_button_and_modifiers(
            x,
            y,
            button,
            Default::default(),
        );
        let clicked = rt.dispatch_mouse_click(x, y, button, 1);
        let dynamic_synced = if take_ffi_signals_dirty() || strata_dom::reactive::take_dirty_signals() {
            rt.sync_dynamic_text()
        } else {
            false
        };
        let dirty = action.needs_redraw() || clicked || dynamic_synced;
        if dirty {
            rt.mark_paint_dirty();
        }
        if !out_dirty.is_null() {
            *out_dirty = dirty;
        }
        clear_last_error_internal();
        StrataStatus::Ok
    })
}

/// Dispatches a pointer-up event at `(x, y)`.
#[no_mangle]
pub unsafe extern "C" fn strata_runtime_pointer_up(
    runtime: *mut AppRuntime,
    x: f32,
    y: f32,
    button: i16,
    out_dirty: *mut bool,
) -> StrataStatus {
    ffi_guard(|| {
        if runtime.is_null() {
            set_last_error("strata_runtime_pointer_up: runtime pointer is NULL");
            return StrataStatus::NullPointer;
        }
        let rt = &mut *runtime;
        rt.last_mouse_pos = (x, y);
        if button == 0 {
            rt.is_mouse_down = false;
        }
        let action = rt.dispatch_pointer_up_action_with_button(x, y, button);
        let dirty = action.needs_redraw();
        if dirty {
            rt.mark_paint_dirty();
        }
        if !out_dirty.is_null() {
            *out_dirty = dirty;
        }
        clear_last_error_internal();
        StrataStatus::Ok
    })
}

/// Dispatches a scroll/wheel impulse at `(x, y)`.
#[no_mangle]
pub unsafe extern "C" fn strata_runtime_wheel(
    runtime: *mut AppRuntime,
    x: f32,
    y: f32,
    delta_x: f32,
    delta_y: f32,
    out_dirty: *mut bool,
) -> StrataStatus {
    ffi_guard(|| {
        if runtime.is_null() {
            set_last_error("strata_runtime_wheel: runtime pointer is NULL");
            return StrataStatus::NullPointer;
        }
        let rt = &mut *runtime;
        rt.last_mouse_pos = (x, y);
        let (handled, scrolled) = rt.dispatch_smooth_scroll_action(x, y, delta_x, delta_y);
        let dirty = handled || scrolled;
        if dirty {
            rt.mark_paint_dirty();
        }
        if !out_dirty.is_null() {
            *out_dirty = dirty;
        }
        clear_last_error_internal();
        StrataStatus::Ok
    })
}

fn decode_modifiers(bits: u32) -> Modifiers {
    Modifiers {
        shift: (bits & 0x01) != 0,
        ctrl: (bits & 0x02) != 0,
        alt: (bits & 0x04) != 0,
        meta: (bits & 0x08) != 0,
    }
}

fn parse_key_code(name: &str) -> KeyCode {
    match name {
        "Enter" => KeyCode::Enter,
        "Escape" => KeyCode::Escape,
        "Tab" => KeyCode::Tab,
        "Space" | " " => KeyCode::Space,
        "Backspace" => KeyCode::Backspace,
        "Delete" => KeyCode::Delete,
        "ArrowUp" => KeyCode::ArrowUp,
        "ArrowDown" => KeyCode::ArrowDown,
        "ArrowLeft" => KeyCode::ArrowLeft,
        "ArrowRight" => KeyCode::ArrowRight,
        "Home" => KeyCode::Home,
        "End" => KeyCode::End,
        "PageUp" => KeyCode::PageUp,
        "PageDown" => KeyCode::PageDown,
        "F1" => KeyCode::F1,
        "F2" => KeyCode::F2,
        "F3" => KeyCode::F3,
        "F4" => KeyCode::F4,
        "F5" => KeyCode::F5,
        "F6" => KeyCode::F6,
        "F7" => KeyCode::F7,
        "F8" => KeyCode::F8,
        "F9" => KeyCode::F9,
        "F10" => KeyCode::F10,
        "F11" => KeyCode::F11,
        "F12" => KeyCode::F12,
        "a" | "A" | "KeyA" => KeyCode::KeyA,
        "b" | "B" | "KeyB" => KeyCode::KeyB,
        "c" | "C" | "KeyC" => KeyCode::KeyC,
        "d" | "D" | "KeyD" => KeyCode::KeyD,
        "e" | "E" | "KeyE" => KeyCode::KeyE,
        "f" | "F" | "KeyF" => KeyCode::KeyF,
        "g" | "G" | "KeyG" => KeyCode::KeyG,
        "h" | "H" | "KeyH" => KeyCode::KeyH,
        "i" | "I" | "KeyI" => KeyCode::KeyI,
        "j" | "J" | "KeyJ" => KeyCode::KeyJ,
        "k" | "K" | "KeyK" => KeyCode::KeyK,
        "l" | "L" | "KeyL" => KeyCode::KeyL,
        "m" | "M" | "KeyM" => KeyCode::KeyM,
        "n" | "N" | "KeyN" => KeyCode::KeyN,
        "o" | "O" | "KeyO" => KeyCode::KeyO,
        "p" | "P" | "KeyP" => KeyCode::KeyP,
        "q" | "Q" | "KeyQ" => KeyCode::KeyQ,
        "r" | "R" | "KeyR" => KeyCode::KeyR,
        "s" | "S" | "KeyS" => KeyCode::KeyS,
        "t" | "T" | "KeyT" => KeyCode::KeyT,
        "u" | "U" | "KeyU" => KeyCode::KeyU,
        "v" | "V" | "KeyV" => KeyCode::KeyV,
        "w" | "W" | "KeyW" => KeyCode::KeyW,
        "x" | "X" | "KeyX" => KeyCode::KeyX,
        "y" | "Y" | "KeyY" => KeyCode::KeyY,
        "z" | "Z" | "KeyZ" => KeyCode::KeyZ,
        "0" | "Digit0" => KeyCode::Digit0,
        "1" | "Digit1" => KeyCode::Digit1,
        "2" | "Digit2" => KeyCode::Digit2,
        "3" | "Digit3" => KeyCode::Digit3,
        "4" | "Digit4" => KeyCode::Digit4,
        "5" | "Digit5" => KeyCode::Digit5,
        "6" | "Digit6" => KeyCode::Digit6,
        "7" | "Digit7" => KeyCode::Digit7,
        "8" | "Digit8" => KeyCode::Digit8,
        "9" | "Digit9" => KeyCode::Digit9,
        "/" | "Slash" => KeyCode::Slash,
        "," | "Comma" => KeyCode::Comma,
        "." | "Period" => KeyCode::Period,
        "-" | "Minus" => KeyCode::Minus,
        "=" | "Equals" => KeyCode::Equals,
        "[" | "BracketLeft" => KeyCode::BracketLeft,
        "]" | "BracketRight" => KeyCode::BracketRight,
        ";" | "Semicolon" => KeyCode::Semicolon,
        "`" | "Backquote" => KeyCode::Backquote,
        _ => KeyCode::Unknown,
    }
}

/// Dispatches a keyboard key-down event (`modifiers_bits`: bit 0=Shift, 1=Ctrl, 2=Alt, 3=Meta).
#[no_mangle]
pub unsafe extern "C" fn strata_runtime_key_down(
    runtime: *mut AppRuntime,
    key_name: *const c_char,
    modifiers_bits: u32,
    out_dirty: *mut bool,
) -> StrataStatus {
    ffi_guard(|| {
        if runtime.is_null() || key_name.is_null() {
            set_last_error("strata_runtime_key_down: runtime or key_name pointer is NULL");
            return StrataStatus::NullPointer;
        }
        let key_str = match CStr::from_ptr(key_name).to_str() {
            Ok(s) => s,
            Err(_) => {
                set_last_error("strata_runtime_key_down: key_name is invalid UTF-8");
                return StrataStatus::InvalidUtf8;
            }
        };
        let rt = &mut *runtime;
        let ev = PlatformEvent::KeyDown {
            key: parse_key_code(key_str),
            modifiers: decode_modifiers(modifiers_bits),
        };
        let res = rt.dispatch_key_event(&ev);
        let dirty = res.needs_redraw();
        if dirty {
            rt.mark_paint_dirty();
        }
        if !out_dirty.is_null() {
            *out_dirty = dirty;
        }
        clear_last_error_internal();
        StrataStatus::Ok
    })
}

/// Dispatches a keyboard key-up event (`modifiers_bits`: bit 0=Shift, 1=Ctrl, 2=Alt, 3=Meta).
#[no_mangle]
pub unsafe extern "C" fn strata_runtime_key_up(
    runtime: *mut AppRuntime,
    key_name: *const c_char,
    modifiers_bits: u32,
    out_dirty: *mut bool,
) -> StrataStatus {
    ffi_guard(|| {
        if runtime.is_null() || key_name.is_null() {
            set_last_error("strata_runtime_key_up: runtime or key_name pointer is NULL");
            return StrataStatus::NullPointer;
        }
        let key_str = match CStr::from_ptr(key_name).to_str() {
            Ok(s) => s,
            Err(_) => {
                set_last_error("strata_runtime_key_up: key_name is invalid UTF-8");
                return StrataStatus::InvalidUtf8;
            }
        };
        let rt = &mut *runtime;
        let ev = PlatformEvent::KeyUp {
            key: parse_key_code(key_str),
            modifiers: decode_modifiers(modifiers_bits),
        };
        let res = rt.dispatch_key_event(&ev);
        let dirty = res.needs_redraw();
        if dirty {
            rt.mark_paint_dirty();
        }
        if !out_dirty.is_null() {
            *out_dirty = dirty;
        }
        clear_last_error_internal();
        StrataStatus::Ok
    })
}

/// Dispatches UTF-8 text input into the currently focused `<input>` or `<textarea>`.
#[no_mangle]
pub unsafe extern "C" fn strata_runtime_text_input(
    runtime: *mut AppRuntime,
    utf8_text: *const c_char,
    out_dirty: *mut bool,
) -> StrataStatus {
    ffi_guard(|| {
        if runtime.is_null() || utf8_text.is_null() {
            set_last_error("strata_runtime_text_input: runtime or utf8_text pointer is NULL");
            return StrataStatus::NullPointer;
        }
        let text = match CStr::from_ptr(utf8_text).to_str() {
            Ok(s) => s.to_string(),
            Err(_) => {
                set_last_error("strata_runtime_text_input: utf8_text is invalid UTF-8");
                return StrataStatus::InvalidUtf8;
            }
        };
        let rt = &mut *runtime;
        let ev = PlatformEvent::ImeCommit(text);
        let res = rt.dispatch_key_event(&ev);
        let dirty = res.needs_redraw();
        if dirty {
            rt.mark_paint_dirty();
        }
        if !out_dirty.is_null() {
            *out_dirty = dirty;
        }
        clear_last_error_internal();
        StrataStatus::Ok
    })
}

/// Queries the RGBA8 framebuffer dimensions and required byte capacity for `runtime`.
#[no_mangle]
pub unsafe extern "C" fn strata_runtime_framebuffer_info(
    runtime: *const AppRuntime,
    out_info: *mut StrataFramebufferInfo,
) -> StrataStatus {
    ffi_guard(|| {
        if runtime.is_null() || out_info.is_null() {
            set_last_error("strata_runtime_framebuffer_info: runtime or out_info pointer is NULL");
            return StrataStatus::NullPointer;
        }
        let rt = &*runtime;
        let width = (rt.viewport_width.max(1.0).round() as u32).max(1);
        let height = (rt.viewport_height.max(1.0).round() as u32).max(1);
        let stride = width.saturating_mul(4);
        let byte_len = (stride as usize).saturating_mul(height as usize);
        *out_info = StrataFramebufferInfo {
            width,
            height,
            stride,
            byte_len,
        };
        clear_last_error_internal();
        StrataStatus::Ok
    })
}

/// Renders `runtime` into caller-allocated `out_rgba` with strict bounds verification.
///
/// If `buf_len` is smaller than the required RGBA8 byte length, writes the required size into
/// `*out_written` and returns [`StrataStatus::BufferTooSmall`] without writing out of bounds.
#[no_mangle]
pub unsafe extern "C" fn strata_runtime_render_rgba(
    runtime: *mut AppRuntime,
    out_rgba: *mut u8,
    buf_len: usize,
    out_written: *mut usize,
) -> StrataStatus {
    ffi_guard(|| {
        if runtime.is_null() || out_rgba.is_null() {
            set_last_error("strata_runtime_render_rgba: runtime or out_rgba pointer is NULL");
            return StrataStatus::NullPointer;
        }
        let rt = &mut *runtime;
        let fb = rt.render_to_framebuffer();
        let required = fb.pixels.len();
        if buf_len < required {
            if !out_written.is_null() {
                *out_written = required;
            }
            set_last_error(format!(
                "strata_runtime_render_rgba: buffer too small (provided {buf_len}, required {required})"
            ));
            return StrataStatus::BufferTooSmall;
        }
        std::ptr::copy_nonoverlapping(fb.pixels.as_ptr(), out_rgba, required);
        if !out_written.is_null() {
            *out_written = required;
        }
        clear_last_error_internal();
        StrataStatus::Ok
    })
}

/// Renders `runtime` and writes a PNG screenshot to `path`.
#[no_mangle]
pub unsafe extern "C" fn strata_runtime_export_png(
    runtime: *mut AppRuntime,
    path: *const c_char,
) -> StrataStatus {
    ffi_guard(|| {
        if runtime.is_null() || path.is_null() {
            set_last_error("strata_runtime_export_png: runtime or path pointer is NULL");
            return StrataStatus::NullPointer;
        }
        let path_str = match CStr::from_ptr(path).to_str() {
            Ok(s) => s,
            Err(_) => {
                set_last_error("strata_runtime_export_png: path is invalid UTF-8");
                return StrataStatus::InvalidUtf8;
            }
        };
        let rt = &mut *runtime;
        match rt.export_png(path_str) {
            Ok(()) => {
                clear_last_error_internal();
                StrataStatus::Ok
            }
            Err(err) => {
                set_last_error(format!("strata_runtime_export_png: {err}"));
                StrataStatus::RuntimeError
            }
        }
    })
}

/// Destroys an [`AppRuntime`] instance.
#[no_mangle]
pub unsafe extern "C" fn strata_runtime_destroy(runtime: *mut AppRuntime) {
    if !runtime.is_null() {
        let _ = Box::from_raw(runtime);
    }
}

/// Launches a Strata desktop window (or headless pump when `options.headless == true`)
/// displaying `view`, **taking ownership** of `view`.
///
/// After this call returns, the caller must NOT call `strata_view_destroy(view)`.
#[no_mangle]
pub unsafe extern "C" fn strata_launch_native_window(
    view: *mut StrataViewHandle,
    options: *const StrataWindowOptions,
) -> StrataStatus {
    ffi_guard(|| {
        if view.is_null() {
            set_last_error("strata_launch_native_window: view handle is NULL");
            return StrataStatus::NullPointer;
        }
        let owned_view = Box::from_raw(view).view;
        let mut config = WindowConfig::default();
        let mut force_headless = false;

        if !options.is_null() {
            let opt = *options;
            if !opt.title.is_null() {
                if let Ok(title) = CStr::from_ptr(opt.title).to_str() {
                    config.title = title.to_string();
                }
            }
            if opt.width > 0 {
                config.width = opt.width;
            }
            if opt.height > 0 {
                config.height = opt.height;
            }
            config.resizable = opt.resizable;
            force_headless = opt.headless;
        }

        let res = if force_headless || std::env::var("STRATA_HEADLESS").is_ok() {
            strata::launch_headless_pump(move || owned_view.clone(), config, |_, _| false)
        } else {
            strata::launch_with_config(move || owned_view.clone(), config)
        };

        match res {
            Ok(()) => {
                clear_last_error_internal();
                StrataStatus::Ok
            }
            Err(err) => {
                set_last_error(format!("strata_launch_native_window: {err}"));
                StrataStatus::RuntimeError
            }
        }
    })
}

/// Renders the window content into its internal frame buffer and copies pixels to `out_rgba_ptr`
/// (legacy unchecked signature preserved for backwards compatibility; prefer `strata_runtime_render_rgba`).
#[no_mangle]
pub unsafe extern "C" fn strata_window_render_frame(
    win: *mut StrataWindowHandle,
    out_rgba_ptr: *mut u8,
) -> i32 {
    ffi_guard_i32(|| {
        if win.is_null() || out_rgba_ptr.is_null() {
            set_last_error("strata_window_render_frame: NULL window or output pointer");
            return -1;
        }
        let w = &mut *win;
        if !w.runtime.is_null() {
            let runtime = &mut *w.runtime;
            let fb = runtime.render_to_framebuffer();
            let bytes = fb.pixels.len();
            std::ptr::copy_nonoverlapping(fb.pixels.as_ptr(), out_rgba_ptr, bytes);
            w.frame_buffer = fb;
            clear_last_error_internal();
            bytes as i32
        } else {
            let mut dl = strata_render::compositor::DisplayList::new();
            dl.push(strata_render::compositor::DrawCommand::DrawRect {
                bounds: strata_layout::Rect::new(0.0, 0.0, w.width as f32, w.height as f32),
                color: strata_style::types::Color::rgba(255, 255, 255, 255),
            });
            w.frame_buffer.render_display_list(&dl);
            let bytes = w.frame_buffer.pixels.len();
            std::ptr::copy_nonoverlapping(w.frame_buffer.pixels.as_ptr(), out_rgba_ptr, bytes);
            clear_last_error_internal();
            bytes as i32
        }
    })
}

/// Updates the title of the window handle. Returns `0` on success, `-1` on invalid handle/UTF-8.
#[no_mangle]
pub unsafe extern "C" fn strata_window_set_title(
    win: *mut StrataWindowHandle,
    title: *const c_char,
) -> i32 {
    ffi_guard_i32(|| {
        if win.is_null() || title.is_null() {
            set_last_error("strata_window_set_title: window or title pointer is NULL");
            return -1;
        }
        if let Ok(c_str) = CStr::from_ptr(title).to_str() {
            (*win).title = c_str.to_string();
            clear_last_error_internal();
            0
        } else {
            set_last_error("strata_window_set_title: title is invalid UTF-8");
            -1
        }
    })
}

/// Returns the current width in logical pixels (`0` if NULL).
#[no_mangle]
pub unsafe extern "C" fn strata_window_get_width(win: *const StrataWindowHandle) -> u32 {
    if win.is_null() {
        0
    } else {
        (*win).width
    }
}

/// Returns the current height in logical pixels (`0` if NULL).
#[no_mangle]
pub unsafe extern "C" fn strata_window_get_height(win: *const StrataWindowHandle) -> u32 {
    if win.is_null() {
        0
    } else {
        (*win).height
    }
}

/// Destroys an individual window handle.
#[no_mangle]
pub unsafe extern "C" fn strata_window_destroy(win: *mut StrataWindowHandle) {
    if !win.is_null() {
        let app = (*win).app;
        if !app.is_null() {
            (*app).windows.retain(|&w| w != win);
        }
        let _ = Box::from_raw(win);
    }
}
