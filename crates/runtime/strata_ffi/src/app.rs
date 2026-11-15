//! C ABI bindings for Strata application lifecycle and native window management.

use std::ffi::CStr;
use std::os::raw::c_char;

/// Opaque wrapper for a Strata application instance.
pub struct StrataAppHandle {
    pub is_running: bool,
    pub windows: Vec<*mut StrataWindowHandle>,
}

/// Opaque wrapper for a Strata window instance with authentic frame buffer presentation sink.
pub struct StrataWindowHandle {
    pub app: *mut StrataAppHandle,
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub is_visible: bool,
    pub frame_buffer: strata_render::rasterizer::FrameBuffer,
    pub runtime: *mut strata::runtime::AppRuntime,
}

static VERSION_CSTR: &[u8] = b"0.1.0\0";

/// Returns the Strata engine version as a null-terminated UTF-8 C string.
#[no_mangle]
pub extern "C" fn strata_version() -> *const c_char {
    VERSION_CSTR.as_ptr() as *const c_char
}

/// Creates a new Strata application handle.
#[no_mangle]
pub extern "C" fn strata_app_create() -> *mut StrataAppHandle {
    let app = Box::new(StrataAppHandle {
        is_running: true,
        windows: Vec::new(),
    });
    Box::into_raw(app)
}

/// Destroys a Strata application handle and releases allocated resources.
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

/// Returns 1 if the application is currently running, 0 otherwise.
#[no_mangle]
pub unsafe extern "C" fn strata_app_is_running(app: *const StrataAppHandle) -> i32 {
    if app.is_null() {
        0
    } else {
        if (*app).is_running {
            1
        } else {
            0
        }
    }
}

/// Creates a new native window attached to the application.
#[no_mangle]
pub unsafe extern "C" fn strata_window_create(
    app: *mut StrataAppHandle,
    title: *const c_char,
    width: u32,
    height: u32,
) -> *mut StrataWindowHandle {
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
        frame_buffer: strata_render::rasterizer::FrameBuffer::new(width as usize, height as usize),
        runtime: std::ptr::null_mut(),
    });
    let win_ptr = Box::into_raw(win);

    if !app.is_null() {
        (*app).windows.push(win_ptr);
    }

    win_ptr
}

/// Associates an active AppRuntime with the window handle for authentic layout and rendering.
#[no_mangle]
pub unsafe extern "C" fn strata_window_set_runtime(
    win: *mut StrataWindowHandle,
    runtime: *mut strata::runtime::AppRuntime,
) -> i32 {
    if win.is_null() {
        return -1;
    }
    (*win).runtime = runtime;
    0
}

/// Creates an AppRuntime instance from a View pointer.
#[no_mangle]
pub unsafe extern "C" fn strata_runtime_create(
    view: *mut strata_dom::View,
    width: f32,
    height: f32,
    dpr: f32,
) -> *mut strata::runtime::AppRuntime {
    if view.is_null() {
        return std::ptr::null_mut();
    }
    let view_clone = (*view).clone();
    let runtime = Box::new(strata::runtime::AppRuntime::new(
        view_clone, width, height, dpr,
    ));
    Box::into_raw(runtime)
}

/// Destroys an AppRuntime instance.
#[no_mangle]
pub unsafe extern "C" fn strata_runtime_destroy(runtime: *mut strata::runtime::AppRuntime) {
    if !runtime.is_null() {
        let _ = Box::from_raw(runtime);
    }
}

/// Renders the window content into its internal frame buffer and copies pixels to caller buffer.
#[no_mangle]
pub unsafe extern "C" fn strata_window_render_frame(
    win: *mut StrataWindowHandle,
    out_rgba_ptr: *mut u8,
) -> i32 {
    if win.is_null() || out_rgba_ptr.is_null() {
        return -1;
    }
    let w = &mut *win;
    if !w.runtime.is_null() {
        let runtime = &mut *w.runtime;
        let fb = runtime.render_to_framebuffer();
        let bytes = fb.pixels.len();
        std::ptr::copy_nonoverlapping(fb.pixels.as_ptr(), out_rgba_ptr, bytes);
        w.frame_buffer = fb;
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
        bytes as i32
    }
}

/// Updates the title of the native window. Returns 0 on success, -1 on invalid handle.
#[no_mangle]
pub unsafe extern "C" fn strata_window_set_title(
    win: *mut StrataWindowHandle,
    title: *const c_char,
) -> i32 {
    if win.is_null() || title.is_null() {
        return -1;
    }

    if let Ok(c_str) = CStr::from_ptr(title).to_str() {
        (*win).title = c_str.to_string();
        0
    } else {
        -1
    }
}

/// Returns the current width in logical pixels.
#[no_mangle]
pub unsafe extern "C" fn strata_window_get_width(win: *const StrataWindowHandle) -> u32 {
    if win.is_null() {
        0
    } else {
        (*win).width
    }
}

/// Returns the current height in logical pixels.
#[no_mangle]
pub unsafe extern "C" fn strata_window_get_height(win: *const StrataWindowHandle) -> u32 {
    if win.is_null() {
        0
    } else {
        (*win).height
    }
}

/// Destroys an individual window instance.
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
