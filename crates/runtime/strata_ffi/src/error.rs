//! Panic-safe FFI error status codes and thread-local last-error diagnostics.

use std::cell::RefCell;
use std::ffi::CString;
use std::os::raw::c_char;
use std::panic::{catch_unwind, AssertUnwindSafe};

/// Canonical C ABI status codes returned by checked Strata FFI calls.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrataStatus {
    /// Operation completed successfully.
    Ok = 0,
    /// A required pointer argument was NULL.
    NullPointer = 1,
    /// A string argument was not valid UTF-8.
    InvalidUtf8 = 2,
    /// Caller-provided output buffer was too small (check `out_written` for required size).
    BufferTooSmall = 3,
    /// An opaque handle pointer was invalid or not in the required state.
    InvalidHandle = 4,
    /// A Rust panic was caught at the FFI boundary (never unwound into C).
    Panic = 5,
    /// Runtime execution or I/O failure (see `strata_last_error()`).
    RuntimeError = 6,
    /// An argument value was out of range or unrecognized.
    InvalidArgument = 7,
}

thread_local! {
    static LAST_ERROR: RefCell<CString> = RefCell::new(CString::new("").unwrap());
}

/// Stores a diagnostic message on the current thread for `strata_last_error()`.
pub(crate) fn set_last_error(msg: impl Into<String>) {
    let sanitized = msg.into().replace('\0', " ");
    let cstr = CString::new(sanitized).unwrap_or_else(|_| CString::new("FFI error").unwrap());
    LAST_ERROR.with(|slot| {
        *slot.borrow_mut() = cstr;
    });
}

/// Clears the current thread's diagnostic error message.
pub(crate) fn clear_last_error_internal() {
    LAST_ERROR.with(|slot| {
        *slot.borrow_mut() = CString::new("").unwrap();
    });
}

/// Executes an FFI closure returning [`StrataStatus`], catching any panic before it crosses `extern "C"`.
pub(crate) fn ffi_guard<F: FnOnce() -> StrataStatus>(f: F) -> StrataStatus {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(status) => status,
        Err(payload) => {
            let msg = if let Some(s) = payload.downcast_ref::<&str>() {
                format!("Rust panic caught at FFI boundary: {s}")
            } else if let Some(s) = payload.downcast_ref::<String>() {
                format!("Rust panic caught at FFI boundary: {s}")
            } else {
                "Rust panic caught at FFI boundary".to_string()
            };
            set_last_error(msg);
            StrataStatus::Panic
        }
    }
}

/// Executes an FFI closure returning a raw mutable pointer, returning NULL on panic.
pub(crate) fn ffi_guard_ptr<T, F: FnOnce() -> *mut T>(f: F) -> *mut T {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(ptr) => ptr,
        Err(payload) => {
            let msg = if let Some(s) = payload.downcast_ref::<&str>() {
                format!("Rust panic caught at FFI boundary: {s}")
            } else if let Some(s) = payload.downcast_ref::<String>() {
                format!("Rust panic caught at FFI boundary: {s}")
            } else {
                "Rust panic caught at FFI boundary".to_string()
            };
            set_last_error(msg);
            std::ptr::null_mut()
        }
    }
}

/// Executes an FFI closure returning an `i32`, returning `-1` on panic.
pub(crate) fn ffi_guard_i32<F: FnOnce() -> i32>(f: F) -> i32 {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(code) => code,
        Err(payload) => {
            let msg = if let Some(s) = payload.downcast_ref::<&str>() {
                format!("Rust panic caught at FFI boundary: {s}")
            } else if let Some(s) = payload.downcast_ref::<String>() {
                format!("Rust panic caught at FFI boundary: {s}")
            } else {
                "Rust panic caught at FFI boundary".to_string()
            };
            set_last_error(msg);
            -1
        }
    }
}

/// Returns a thread-local null-terminated UTF-8 string describing the last FFI error on this thread.
///
/// Always returns a valid non-null pointer (`""` when no error is recorded). The pointer remains
/// valid until the next Strata FFI call on the same thread.
#[no_mangle]
pub extern "C" fn strata_last_error() -> *const c_char {
    LAST_ERROR.with(|slot| slot.borrow().as_ptr())
}

/// Clears the current thread's last error string.
#[no_mangle]
pub extern "C" fn strata_clear_last_error() {
    clear_last_error_internal();
}
