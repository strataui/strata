//! Strata Chromium Surface: Isolated On-Demand Chromium Subprocess Harness & OSR Bridge.
//!
//! Spawns Chromium worker processes lazily only when `<WebView />` mounts,
//! streams rendered frames through shared memory zero-copy, and terminates
//! instantly on unmount to maintain Strata's sub-3MB baseline memory footprint.

pub mod ipc;
pub mod surface;
pub mod process;
pub mod webview;

pub use ipc::{
    ChromiumCommand, ChromiumEvent, KeyAction, Modifiers, MouseAction, MouseButton, NavigationState,
};
pub use surface::{PixelFormat, ShmSurface};
pub use process::{ChromiumProcessManager, ProcessState};
pub use webview::{WebViewConfig, WebViewController};

pub mod prelude {
    pub use crate::ipc::{
        ChromiumCommand, ChromiumEvent, KeyAction, Modifiers, MouseAction, MouseButton,
        NavigationState,
    };
    pub use crate::surface::{PixelFormat, ShmSurface};
    pub use crate::process::{ChromiumProcessManager, ProcessState};
    pub use crate::webview::{WebViewConfig, WebViewController};
}
