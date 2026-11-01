//! C ABI bindings for reactive signals and state observation.

use std::os::raw::c_void;

pub type SignalCallbackInt = extern "C" fn(new_value: i64, user_data: *mut c_void);

struct Subscription {
    id: u64,
    callback: SignalCallbackInt,
    user_data: *mut c_void,
}

/// Opaque handle representing an integer reactive signal over the C ABI.
pub struct StrataSignalHandle {
    pub value: i64,
    next_sub_id: u64,
    subscriptions: Vec<Subscription>,
}

/// Creates a new reactive integer signal initialized to `initial`.
#[no_mangle]
pub extern "C" fn strata_signal_create_int(initial: i64) -> *mut StrataSignalHandle {
    let sig = Box::new(StrataSignalHandle {
        value: initial,
        next_sub_id: 1,
        subscriptions: Vec::new(),
    });
    Box::into_raw(sig)
}

/// Reads the current value of the integer signal.
#[no_mangle]
pub unsafe extern "C" fn strata_signal_get_int(sig: *const StrataSignalHandle) -> i64 {
    if sig.is_null() {
        0
    } else {
        (*sig).value
    }
}

/// Updates the integer signal value and triggers all registered C callbacks.
#[no_mangle]
pub unsafe extern "C" fn strata_signal_set_int(sig: *mut StrataSignalHandle, value: i64) {
    if sig.is_null() {
        return;
    }

    if (*sig).value != value {
        (*sig).value = value;
        for sub in &(*sig).subscriptions {
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

    let id = (*sig).next_sub_id;
    (*sig).next_sub_id += 1;

    (*sig).subscriptions.push(Subscription {
        id,
        callback: cb,
        user_data,
    });

    id
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

    let initial_len = (*sig).subscriptions.len();
    (*sig).subscriptions.retain(|s| s.id != sub_id);

    if (*sig).subscriptions.len() < initial_len {
        0
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
