//! Thread-safe typed reactive signals bridged between C/foreign hosts and `strata_dom`.

use std::ffi::CStr;
use std::os::raw::{c_char, c_void};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, RwLock};

use crate::error::{clear_last_error_internal, ffi_guard, ffi_guard_ptr, set_last_error, StrataStatus};

/// Global dirty flag set whenever any FFI signal changes from any thread so `strata_runtime_tick`
/// detects cross-thread mutations even when the UI runtime lives on a different thread.
static FFI_SIGNALS_DIRTY: AtomicBool = AtomicBool::new(false);

pub(crate) fn mark_ffi_signal_dirty() {
    FFI_SIGNALS_DIRTY.store(true, Ordering::Release);
    strata_dom::mark_dirty();
}

pub(crate) fn take_ffi_signals_dirty() -> bool {
    FFI_SIGNALS_DIRTY.swap(false, Ordering::AcqRel)
}

pub type SignalCallbackInt = extern "C" fn(new_value: i64, user_data: *mut c_void);

#[derive(Clone, Copy)]
struct Subscription {
    id: u64,
    callback: SignalCallbackInt,
    user_data: *mut c_void,
}

// Safety: Foreign host guarantees `user_data` synchronization or thread affinity.
unsafe impl Send for Subscription {}
unsafe impl Sync for Subscription {}

#[derive(Debug, Clone)]
pub(crate) enum SignalValue {
    Int64(i64),
    Float64(f64),
    Bool(bool),
    Utf8String(String),
}

impl SignalValue {
    pub(crate) fn to_display_string(&self) -> String {
        match self {
            SignalValue::Int64(v) => v.to_string(),
            SignalValue::Float64(v) => v.to_string(),
            SignalValue::Bool(v) => v.to_string(),
            SignalValue::Utf8String(s) => s.clone(),
        }
    }
}

struct SignalInner {
    next_sub_id: u64,
    subscriptions: Vec<Subscription>,
    typed_value: SignalValue,
}

/// Shared backing cell allowing `DynamicText` nodes to outlive or observe an FFI signal safely.
pub(crate) struct SharedSignalCell {
    atomic_i64: AtomicI64,
    inner: RwLock<SignalInner>,
}

impl SharedSignalCell {
    fn new(initial: SignalValue) -> Self {
        let initial_i64 = match initial {
            SignalValue::Int64(v) => v,
            SignalValue::Bool(b) => i64::from(b),
            _ => 0,
        };
        Self {
            atomic_i64: AtomicI64::new(initial_i64),
            inner: RwLock::new(SignalInner {
                next_sub_id: 1,
                subscriptions: Vec::new(),
                typed_value: initial,
            }),
        }
    }

    pub(crate) fn format_text(&self) -> String {
        if let Ok(guard) = self.inner.read() {
            match &guard.typed_value {
                SignalValue::Int64(_) => self.atomic_i64.load(Ordering::SeqCst).to_string(),
                other => other.to_display_string(),
            }
        } else {
            self.atomic_i64.load(Ordering::SeqCst).to_string()
        }
    }
}

/// Opaque handle representing a thread-safe typed reactive signal over the C ABI.
pub struct StrataSignalHandle {
    cell: Arc<SharedSignalCell>,
}

impl StrataSignalHandle {
    pub(crate) fn clone_cell(&self) -> Arc<SharedSignalCell> {
        Arc::clone(&self.cell)
    }
}

/// Creates a new reactive 64-bit integer signal initialized to `initial`.
#[no_mangle]
pub extern "C" fn strata_signal_create_int(initial: i64) -> *mut StrataSignalHandle {
    ffi_guard_ptr(|| {
        clear_last_error_internal();
        Box::into_raw(Box::new(StrataSignalHandle {
            cell: Arc::new(SharedSignalCell::new(SignalValue::Int64(initial))),
        }))
    })
}

/// Reads the current value of an integer signal (`0` if `sig` is NULL).
#[no_mangle]
pub unsafe extern "C" fn strata_signal_get_int(sig: *const StrataSignalHandle) -> i64 {
    if sig.is_null() {
        return 0;
    }
    (&(*sig).cell).atomic_i64.load(Ordering::SeqCst)
}

/// Updates an integer signal value, notifies C subscribers, and marks Strata reactive graphs dirty.
#[no_mangle]
pub unsafe extern "C" fn strata_signal_set_int(sig: *mut StrataSignalHandle, value: i64) {
    if sig.is_null() {
        return;
    }
    let _ = ffi_guard(|| {
        let cell = &(*sig).cell;
        let old = cell.atomic_i64.swap(value, Ordering::SeqCst);
        if old != value {
            mark_ffi_signal_dirty();
            let subs = if let Ok(mut guard) = cell.inner.write() {
                guard.typed_value = SignalValue::Int64(value);
                guard.subscriptions.clone()
            } else {
                Vec::new()
            };
            for sub in subs {
                (sub.callback)(value, sub.user_data);
            }
        }
        StrataStatus::Ok
    });
}

/// Subscribes a C callback to integer mutations of this signal. Returns a subscription ID (`> 0`), or `0` on error.
#[no_mangle]
pub unsafe extern "C" fn strata_signal_subscribe_int(
    sig: *mut StrataSignalHandle,
    callback: Option<SignalCallbackInt>,
    user_data: *mut c_void,
) -> u64 {
    if sig.is_null() {
        return 0;
    }
    let cb = match callback {
        Some(c) => c,
        None => return 0,
    };
    if let Ok(mut guard) = (&(*sig).cell).inner.write() {
        let id = guard.next_sub_id;
        guard.next_sub_id += 1;
        guard.subscriptions.push(Subscription {
            id,
            callback: cb,
            user_data,
        });
        id
    } else {
        0
    }
}

/// Unsubscribes a callback by its subscription ID. Returns `0` on success, `-1` if not found.
#[no_mangle]
pub unsafe extern "C" fn strata_signal_unsubscribe_int(
    sig: *mut StrataSignalHandle,
    sub_id: u64,
) -> i32 {
    if sig.is_null() {
        return -1;
    }
    if let Ok(mut guard) = (&(*sig).cell).inner.write() {
        let initial_len = guard.subscriptions.len();
        guard.subscriptions.retain(|s| s.id != sub_id);
        if guard.subscriptions.len() < initial_len {
            0
        } else {
            -1
        }
    } else {
        -1
    }
}

/// Creates a new reactive 64-bit floating-point signal initialized to `initial`.
#[no_mangle]
pub extern "C" fn strata_signal_create_f64(initial: f64) -> *mut StrataSignalHandle {
    ffi_guard_ptr(|| {
        clear_last_error_internal();
        Box::into_raw(Box::new(StrataSignalHandle {
            cell: Arc::new(SharedSignalCell::new(SignalValue::Float64(initial))),
        }))
    })
}

/// Reads the current `f64` value of a signal (`0.0` if NULL or non-numeric).
#[no_mangle]
pub unsafe extern "C" fn strata_signal_get_f64(sig: *const StrataSignalHandle) -> f64 {
    if sig.is_null() {
        return 0.0;
    }
    if let Ok(guard) = (&(*sig).cell).inner.read() {
        match guard.typed_value {
            SignalValue::Float64(v) => v,
            SignalValue::Int64(v) => v as f64,
            _ => 0.0,
        }
    } else {
        0.0
    }
}

/// Updates a floating-point signal and marks Strata reactive graphs dirty.
#[no_mangle]
pub unsafe extern "C" fn strata_signal_set_f64(
    sig: *mut StrataSignalHandle,
    value: f64,
) -> StrataStatus {
    ffi_guard(|| {
        if sig.is_null() {
            set_last_error("strata_signal_set_f64: signal pointer is NULL");
            return StrataStatus::NullPointer;
        }
        if let Ok(mut guard) = (&(*sig).cell).inner.write() {
            let changed = !matches!(guard.typed_value, SignalValue::Float64(v) if v.to_bits() == value.to_bits());
            guard.typed_value = SignalValue::Float64(value);
            if changed {
                mark_ffi_signal_dirty();
            }
            clear_last_error_internal();
            StrataStatus::Ok
        } else {
            set_last_error("strata_signal_set_f64: signal lock poisoned");
            StrataStatus::RuntimeError
        }
    })
}

/// Creates a new reactive boolean signal initialized to `initial`.
#[no_mangle]
pub extern "C" fn strata_signal_create_bool(initial: bool) -> *mut StrataSignalHandle {
    ffi_guard_ptr(|| {
        clear_last_error_internal();
        Box::into_raw(Box::new(StrataSignalHandle {
            cell: Arc::new(SharedSignalCell::new(SignalValue::Bool(initial))),
        }))
    })
}

/// Reads the current boolean value of a signal (`false` if NULL).
#[no_mangle]
pub unsafe extern "C" fn strata_signal_get_bool(sig: *const StrataSignalHandle) -> bool {
    if sig.is_null() {
        return false;
    }
    if let Ok(guard) = (&(*sig).cell).inner.read() {
        match guard.typed_value {
            SignalValue::Bool(b) => b,
            SignalValue::Int64(v) => v != 0,
            _ => false,
        }
    } else {
        false
    }
}

/// Updates a boolean signal and marks Strata reactive graphs dirty.
#[no_mangle]
pub unsafe extern "C" fn strata_signal_set_bool(
    sig: *mut StrataSignalHandle,
    value: bool,
) -> StrataStatus {
    ffi_guard(|| {
        if sig.is_null() {
            set_last_error("strata_signal_set_bool: signal pointer is NULL");
            return StrataStatus::NullPointer;
        }
        (&(*sig).cell).atomic_i64.store(i64::from(value), Ordering::SeqCst);
        if let Ok(mut guard) = (&(*sig).cell).inner.write() {
            let changed = !matches!(guard.typed_value, SignalValue::Bool(b) if b == value);
            guard.typed_value = SignalValue::Bool(value);
            if changed {
                mark_ffi_signal_dirty();
            }
            clear_last_error_internal();
            StrataStatus::Ok
        } else {
            set_last_error("strata_signal_set_bool: signal lock poisoned");
            StrataStatus::RuntimeError
        }
    })
}

/// Creates a new reactive UTF-8 string signal initialized to `initial` (empty string if NULL).
#[no_mangle]
pub unsafe extern "C" fn strata_signal_create_str(
    initial: *const c_char,
) -> *mut StrataSignalHandle {
    ffi_guard_ptr(|| {
        let s = if initial.is_null() {
            String::new()
        } else {
            match CStr::from_ptr(initial).to_str() {
                Ok(v) => v.to_string(),
                Err(_) => {
                    set_last_error("strata_signal_create_str: initial value is invalid UTF-8");
                    return std::ptr::null_mut();
                }
            }
        };
        clear_last_error_internal();
        Box::into_raw(Box::new(StrataSignalHandle {
            cell: Arc::new(SharedSignalCell::new(SignalValue::Utf8String(s))),
        }))
    })
}

/// Reads the string representation of a signal into `out_buf` (null-terminated UTF-8).
///
/// Supports two-call length probing: pass `out_buf == NULL` to query the required buffer size
/// (including the NUL terminator) via `*out_written` and receive `StrataStatus::Ok`.
#[no_mangle]
pub unsafe extern "C" fn strata_signal_get_str(
    sig: *const StrataSignalHandle,
    out_buf: *mut c_char,
    buf_len: usize,
    out_written: *mut usize,
) -> StrataStatus {
    ffi_guard(|| {
        if sig.is_null() {
            set_last_error("strata_signal_get_str: signal pointer is NULL");
            return StrataStatus::NullPointer;
        }
        let text = (&(*sig).cell).format_text();
        let bytes = text.as_bytes();
        let required = bytes.len() + 1;

        if out_buf.is_null() {
            if !out_written.is_null() {
                *out_written = required;
            }
            clear_last_error_internal();
            return StrataStatus::Ok;
        }
        if buf_len < required {
            if !out_written.is_null() {
                *out_written = required;
            }
            set_last_error(format!(
                "strata_signal_get_str: buffer too small (provided {buf_len}, required {required})"
            ));
            return StrataStatus::BufferTooSmall;
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_buf as *mut u8, bytes.len());
        *out_buf.add(bytes.len()) = 0;
        if !out_written.is_null() {
            *out_written = bytes.len();
        }
        clear_last_error_internal();
        StrataStatus::Ok
    })
}

/// Updates a UTF-8 string signal and marks Strata reactive graphs dirty.
#[no_mangle]
pub unsafe extern "C" fn strata_signal_set_str(
    sig: *mut StrataSignalHandle,
    value: *const c_char,
) -> StrataStatus {
    ffi_guard(|| {
        if sig.is_null() || value.is_null() {
            set_last_error("strata_signal_set_str: signal or value pointer is NULL");
            return StrataStatus::NullPointer;
        }
        let new_str = match CStr::from_ptr(value).to_str() {
            Ok(s) => s.to_string(),
            Err(_) => {
                set_last_error("strata_signal_set_str: value is invalid UTF-8");
                return StrataStatus::InvalidUtf8;
            }
        };
        if let Ok(mut guard) = (&(*sig).cell).inner.write() {
            let changed = !matches!(&guard.typed_value, SignalValue::Utf8String(old) if old == &new_str);
            guard.typed_value = SignalValue::Utf8String(new_str);
            if changed {
                mark_ffi_signal_dirty();
            }
            clear_last_error_internal();
            StrataStatus::Ok
        } else {
            set_last_error("strata_signal_set_str: signal lock poisoned");
            StrataStatus::RuntimeError
        }
    })
}

/// Releases resources associated with a reactive signal handle.
///
/// Any `DynamicText` nodes bound via `strata_node_dynamic_text_signal` hold their own reference
/// to the backing cell and remain completely memory-safe after this handle is destroyed.
#[no_mangle]
pub unsafe extern "C" fn strata_signal_destroy(sig: *mut StrataSignalHandle) {
    if !sig.is_null() {
        let _ = Box::from_raw(sig);
    }
}
