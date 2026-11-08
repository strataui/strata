//! Strata Chromium Surface: Isolated On-Demand Chromium Subprocess Harness & OSR Bridge.
//!
//! Architectural Boundary & Scope:
//! Strata core is 100% pure Rust without embedded Chromium, WebViews, or V8 engines.
//! This crate (`strata_chromium_surface`) is an optional external plugin harness and
//! Out-of-Process Offscreen Rendering (OSR) bridge. When an application explicitly mounts
//! an optional `<WebView />`, this harness coordinates an external browser process (e.g. CEF,
//! Chromium, WebKitGTK) over zero-copy shared memory texture buffers, and terminates the
//! subprocess instantly on unmount to restore Strata's native sub-3MB baseline memory footprint.

pub mod ipc;
pub mod surface;
pub mod process;
pub mod webview;

pub use ipc::{
    ChromiumCommand, ChromiumEvent, KeyAction, Modifiers, MouseAction, MouseButton, NavigationState,
};
pub use surface::{PixelFormat, ShmBufferHeader, ShmError, ShmSurface, SHM_HEADER_SIZE, SHM_MAGIC};
pub use process::{ChromiumProcessManager, ProcessState};
pub use webview::{WebViewConfig, WebViewController};

pub mod prelude {
    pub use crate::ipc::{
        ChromiumCommand, ChromiumEvent, KeyAction, Modifiers, MouseAction, MouseButton,
        NavigationState,
    };
    pub use crate::surface::{
        PixelFormat, ShmBufferHeader, ShmError, ShmSurface, SHM_HEADER_SIZE, SHM_MAGIC,
    };
    pub use crate::process::{ChromiumProcessManager, ProcessState};
    pub use crate::webview::{WebViewConfig, WebViewController};
}
