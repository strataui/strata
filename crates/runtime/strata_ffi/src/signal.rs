use std::os::raw::c_void;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::RwLock;

pub type SignalCallbackInt = extern "C" fn(new_value: i64, user_data: *mut c_void);

#[derive(Clone, Copy)]
struct Subscription {
    id: u64,
    callback: SignalCallbackInt,
    user_data: *mut c_void,
}

// Safety: Subscription holds raw user_data passed from C FFI, safe to transfer across threads
unsafe impl Send for Subscription {}
unsafe impl Sync for Subscription {}

struct SignalInner {
    next_sub_id: u64,
    subscriptions: Vec<Subscription>,
}

/// Opaque handle representing a thread-safe integer reactive signal over the C ABI.
pub struct StrataSignalHandle {
    pub value: AtomicI64,
    inner: RwLock<SignalInner>,
}

/// Creates a new reactive integer signal initialized to `initial`.
#[no_mangle]
pub extern "C" fn strata_signal_create_int(initial: i64) -> *mut StrataSignalHandle {
    let sig = Box::new(StrataSignalHandle {
        value: AtomicI64::new(initial),
        inner: RwLock::new(SignalInner {
            next_sub_id: 1,
            subscriptions: Vec::new(),
        }),
    });
    Box::into_raw(sig)
}

/// Reads the current value of the integer signal.
#[no_mangle]
pub unsafe extern "C" fn strata_signal_get_int(sig: *const StrataSignalHandle) -> i64 {
    if sig.is_null() {
        0
    } else {
        (*sig).value.load(Ordering::SeqCst)
    }
}

/// Updates the integer signal value and triggers all registered C callbacks.
#[no_mangle]
pub unsafe extern "C" fn strata_signal_set_int(sig: *mut StrataSignalHandle, value: i64) {
    if sig.is_null() {
        return;
    }

    let old = (*sig).value.swap(value, Ordering::SeqCst);
    if old != value {
        let subs = if let Ok(guard) = (*sig).inner.read() {
            guard.subscriptions.clone()
        } else {
            Vec::new()
        };
        for sub in subs {
            (sub.callback)(value, sub.user_data);
        }
    }
}

/// Subscribes a C callback to mutations of this signal. Returns a subscription ID (>0).
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

    if let Ok(mut guard) = (*sig).inner.write() {
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

/// Unsubscribes a callback by its subscription ID.
#[no_mangle]
pub unsafe extern "C" fn strata_signal_unsubscribe_int(
    sig: *mut StrataSignalHandle,
    sub_id: u64,
) -> i32 {
    if sig.is_null() {
        return -1;
    }

    if let Ok(mut guard) = (*sig).inner.write() {
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

/// Releases resources associated with the reactive signal handle.
#[no_mangle]
pub unsafe extern "C" fn strata_signal_destroy(sig: *mut StrataSignalHandle) {
    if !sig.is_null() {
        let _ = Box::from_raw(sig);
    }
}
