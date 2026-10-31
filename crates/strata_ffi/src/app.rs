//! C ABI bindings for Strata application lifecycle and native window management.

use std::ffi::CStr;
use std::os::raw::c_char;

/// Opaque wrapper for a Strata application instance.
pub struct StrataAppHandle {
    pub is_running: bool,
    pub windows: Vec<*mut StrataWindowHandle>,
}

/// Opaque wrapper for a Strata window instance.
pub struct StrataWindowHandle {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub is_visible: bool,
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
        title: title_str,
        width,
        height,
        is_visible: true,
    });
    let win_ptr = Box::into_raw(win);

    if !app.is_null() {
        (*app).windows.push(win_ptr);
    }

    win_ptr
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
        let _ = Box::from_raw(win);
    }
}
