//! State-preserving native Hot Module Replacement (HMR) orchestrator for Strata.
//!
//! Provides:
//! - [`HotReloader`]: Epoch-staged dynamic library loading (`libloading`), UAF-safe handle retention,
//!   binary snapshot migration, and live `strata_dom::View` rendering.
//! - [`DirectoryWatcher`]: Real recursive, debounced source watcher ignoring `target/` and `.git/`.
//! - [`ipc`]: Cross-platform JSON-Lines HMR control channel (Unix sockets / Windows loopback TCP).
//! - [`export_hot_view!`]: One-line macro exporting the ABI symbols required by `strata dev`.

pub mod ipc;
pub mod protocol;
pub mod reloader;
pub mod snapshot;
pub mod watcher;

pub use ipc::{
    default_hmr_socket_path, HmrIpcClient, HmrIpcServer, HmrRequest, HmrResponse,
    PendingHmrCommand, STRATA_HMR_SOCKET_ENV,
};
pub use protocol::HotReloadMessage;
pub use reloader::HotReloader;
pub use snapshot::{
    ComponentSnapshot, FieldAction, FieldMigration, GenerationalFieldValue, HotReloadSnapshot,
    SignalMigration, SignalSnapshot,
};
pub use watcher::{DirectoryWatcher, FileChangeTracker};

/// Exports the standard Strata HMR dynamic-library entry points for a root view function
/// (`fn() -> strata_dom::View`), enabling zero-boilerplate state-preserving hot reload with `strata dev`.
///
/// # Example
/// ```rust,ignore
/// use strata::prelude::*;
///
/// pub fn root_view() -> View {
///     let count = use_hmr_signal("counter", 0i32);
///     rsx! { <div>{format!("Count: {}", count.get())}</div> }
/// }
///
/// strata_hot_reload::export_hot_view!(root_view);
/// ```
#[macro_export]
macro_rules! export_hot_view {
    ($view_fn:path) => {
        #[no_mangle]
        pub extern "C" fn strata_hot_reload_version() -> u32 {
            $crate::HotReloader::CURRENT_ABI_VERSION
        }

        #[no_mangle]
        pub unsafe extern "C" fn strata_hot_reload_hydrate(
            ptr: *const u8,
            len: usize,
        ) -> i32 {
            if ptr.is_null() || len == 0 {
                return 0;
            }
            let slice = std::slice::from_raw_parts(ptr, len);
            if let Some(snapshot) = $crate::HotReloadSnapshot::decode(slice) {
                let restored = $crate::HotReloader::hydrate_dom_signals(&snapshot);
                // Return 0 on success so existing ABI contract (`res == 0` => ok) stays exact.
                let _ = restored;
                0
            } else {
                -1
            }
        }

        #[no_mangle]
        pub unsafe extern "C" fn strata_hot_reload_snapshot(
            out_ptr: *mut *mut u8,
            out_len: *mut usize,
        ) -> i32 {
            if out_ptr.is_null() || out_len.is_null() {
                return -1;
            }
            let mut snap = $crate::HotReloadSnapshot::new(0);
            for (idx, (key, type_name, bytes)) in
                $crate::strata_dom_export::export_hmr_signals().into_iter().enumerate()
            {
                snap.insert_signal(
                    idx as u64 + 1,
                    format!("hmr:{key}:{type_name}"),
                    bytes,
                );
            }
            let mut encoded = snap.encode().into_boxed_slice();
            let len = encoded.len();
            let ptr = encoded.as_mut_ptr();
            std::mem::forget(encoded);
            *out_ptr = ptr;
            *out_len = len;
            0
        }

        #[no_mangle]
        pub unsafe extern "C" fn strata_hot_reload_free_snapshot(ptr: *mut u8, len: usize) {
            if !ptr.is_null() && len > 0 {
                let _ = Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, len));
            }
        }

        #[no_mangle]
        pub extern "C" fn strata_hot_reload_entry() -> i32 {
            0
        }

        #[no_mangle]
        pub extern "C" fn strata_hmr_render_view() -> *mut $crate::strata_dom_export::View {
            let view: $crate::strata_dom_export::View =
                $crate::strata_dom_export::with_root_component_scope($view_fn);
            Box::into_raw(Box::new(view))
        }

        #[no_mangle]
        pub extern "C" fn strata_hmr_has_dirty_signals() -> u8 {
            if $crate::strata_dom_export::reactive::has_dirty_signals() {
                1
            } else {
                0
            }
        }

        #[no_mangle]
        pub extern "C" fn strata_hmr_take_dirty_signals() -> u8 {
            if $crate::strata_dom_export::reactive::take_dirty_signals() {
                1
            } else {
                0
            }
        }
    };
}

#[doc(hidden)]
pub use strata_dom as strata_dom_export;

/// Convenient re-exports for HMR-enabled Strata applications and dev tooling.
pub mod prelude {
    pub use super::ipc::{
        default_hmr_socket_path, HmrIpcClient, HmrIpcServer, HmrRequest, HmrResponse,
        PendingHmrCommand, STRATA_HMR_SOCKET_ENV,
    };
    pub use super::protocol::HotReloadMessage;
    pub use super::reloader::HotReloader;
    pub use super::snapshot::{
        ComponentSnapshot, FieldAction, FieldMigration, GenerationalFieldValue, HotReloadSnapshot,
        SignalMigration, SignalSnapshot,
    };
    pub use super::watcher::{DirectoryWatcher, FileChangeTracker};
    pub use crate::export_hot_view;
}
