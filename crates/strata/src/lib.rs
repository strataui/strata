//! Strata: Universal High-Performance Frontend Engine & Framework in Pure Rust.
//!
//! # Examples
//!
//! ```rust
//! use strata::prelude::*;
//!
//! let counter = Signal::new(10);
//! counter.set(25);
//! assert_eq!(counter.get(), 25);
//! ```

#![allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::field_reassign_with_default,
    clippy::collapsible_match
)]

pub use strata_a11y;
pub use strata_a11y as a11y;
pub use strata_dom;
pub use strata_dom as dom;
pub use strata_gpu_path as gpu_path;
pub use strata_js;
pub use strata_js as js;
pub use strata_layout as layout;
pub use strata_macro::*;
pub use strata_platform as platform;
pub use strata_platform::dialog;
pub use strata_platform::vibrancy;
pub use strata_platform::{HoverTooltipStyle, WindowConfig};
pub use strata_primitives as primitives;
pub use strata_render as render;
pub use strata_storage as storage;
pub use strata_style as style;
pub use strata_vfs as fs;
pub use strata_web_api as web_api;

#[cfg(feature = "device")]
pub use strata_device as device;
#[cfg(feature = "devtools")]
pub use strata_devtools as devtools;
#[cfg(feature = "hot-reload")]
pub use strata_hot_reload as hot_reload;
#[cfg(feature = "parallel")]
pub use strata_parallel as parallel;

pub mod low_level;
pub mod mcp_bridge;
pub mod runtime;
pub mod view_transition;
pub use low_level::{LowLevelRenderer, OffscreenBuffer, RawSurfaceHandle};
pub use mcp_bridge::{default_dev_socket_path, start_mcp_dev_listener};
#[cfg(feature = "hot-reload")]
pub use runtime::{HmrPatchOutcome, HmrRuntimeController};
pub use runtime::{
    app, launch, launch_auto, launch_headless, launch_headless_pump, launch_window,
    launch_with_config, launch_with_events, launch_with_frame, queue_edit_command, AppFrameInfo,
    AppLifecycleHooks, AppRuntime, DynamicTextBinding, EditCommand, FrameOutcome, FrameProfile,
    FrameProfileRecord, KeyDispatchResult, MatchMode, NodeQuery, PerfPercentileStats,
    PerfProfileSummary, PointerDownAction, PointerMoveAction, PointerUpAction, ProfilingAllocator,
    RebuildReason, ReconcileStats, StrataApp, StrataAppBuilder,
};
#[cfg(target_os = "linux")]
pub use runtime::{
    launch_gtk, launch_gtk_with_events, launch_gtk_with_frame, launch_wayland,
    launch_wayland_with_events, launch_x11, launch_x11_with_events,
};
#[cfg(target_os = "macos")]
pub use runtime::{launch_macos, launch_macos_with_events};
#[cfg(windows)]
pub use runtime::{launch_win32, launch_win32_with_events};
#[cfg(target_os = "linux")]
pub use strata_platform::{
    gtk_begin_window_drag, gtk_close, gtk_current_modifiers, gtk_get_clipboard_text,
    gtk_is_closing, gtk_is_maximized, gtk_minimize, gtk_set_clipboard_text, gtk_toggle_maximize,
};
pub use strata_dom::{
    get_scroll_offset, scroll_into_view, scroll_to, scroll_to_bottom, scroll_to_top,
    ProgrammaticScrollCommand,
};
pub use view_transition::{ViewTransitionManager, ViewTransitionSnapshot};

pub mod prelude {
    pub use strata_dom;
    pub use strata_dom::{
        clear_cursor_override, clear_hmr_signals, clear_persistent_signals, create_anchored_portal,
        create_context, create_dropdown_portal, create_portal, current_cursor,
        dispatch_click_outside, dispatch_global_keydown, export_hmr_signals, get_scroll_offset,
        import_hmr_signals, provide_context, reset_component_hooks, scroll_into_view, scroll_to,
        scroll_to_bottom, scroll_to_top, set_current_cursor, set_system_color_scheme,
        set_system_contrast, set_system_reduced_motion, set_viewport_dimensions, use_active,
        use_callback, use_click_outside, use_clipboard, use_color_scheme, use_context,
        use_contrast, use_cursor, use_effect, use_effect_with_deps, use_focus, use_focus_within,
        use_hmr_signal, use_hover, use_id, use_keyboard_shortcut, use_media_query, use_memo,
        use_memo_with_deps, use_persistent_signal, use_press, use_reduced_motion, use_reducer,
        use_reducer_init, use_ref, use_shortcut, use_signal, use_state, with_component_key,
        with_root_component_scope, AnchorPlacement, Attribute, ClickOutsideHandle, ClipboardHandle,
        ColorScheme, Context, ContrastPreference, CursorIcon, Effect, EffectGuard, ErrorBoundary,
        ErrorBoundaryBuilder, ErrorBoundaryInfo, Event, EventType, FocusHandlers, HmrCodec,
        HoverHandlers, IntoChildren, IntoEffectCleanup, IntoNodeBlueprint, KeyChord, Memo,
        NodeBlueprint, Portal, PortalBuilder, PressHandlers, ProgrammaticScrollCommand, ReadSignal,
        Ref, SetSignal, ShortcutHandle, Signal, Tag, View, WriteSignal,
    };
    pub use strata_js;
    pub use strata_js::{
        json::{parse_json, stringify_json},
        promise::{
            drain_microtasks, pending_microtasks_count, queue_microtask, JsPromise, PromiseState,
        },
        web::{
            advance_timers_by, clear_interval, clear_timeout, local_storage, session_storage,
            set_interval, set_timeout, tick_timers, Storage, TimerId, URLSearchParams,
        },
        JsObject, JsValue,
    };
    pub use strata_macro::{component, dynamic, rsx};
    pub use strata_platform::prelude::*;
    pub use strata_primitives::prelude::*;
    pub use strata_render::scene3d::{
        AmbientLight, BoxGeometry, Camera3D, Canvas3D, DirectionalLight, Geometry3D, Light3D,
        Material3D, Mesh, Mesh3D, PerspectiveCamera, Scene3D, ShaderMaterial, ShaderSource,
        SphereGeometry, StandardMaterial, Transform3D,
    };
    pub use strata_storage::prelude::*;
    pub use strata_style::ComputedStyle;
    pub use strata_vfs::prelude::*;
    pub use strata_web_api::prelude::*;

    #[cfg(feature = "device")]
    pub use strata_device::prelude::*;
    #[cfg(feature = "devtools")]
    pub use strata_devtools::*;
    #[cfg(feature = "hot-reload")]
    pub use strata_hot_reload::prelude::*;
    #[cfg(feature = "parallel")]
    pub use strata_parallel::{
        reset_thread_local_arena, translate_draw_command, with_thread_local_arena,
        AdaptivePartitioner, ContainmentClassifier, DirtySignalQueue, DisplayListChunk,
        LayoutConstraints, LayoutPartition, ParallelEngine, PartitionBoundary, PartitionId,
    };

    pub use crate::runtime::{
        app, launch, launch_auto, launch_headless, launch_headless_pump, launch_window,
        launch_with_config, launch_with_events, launch_with_frame, queue_edit_command,
        AppFrameInfo, AppLifecycleHooks, AppRuntime, DynamicTextBinding, EditCommand, FrameOutcome,
        KeyDispatchResult, PointerDownAction, PointerMoveAction, PointerUpAction, StrataApp,
        StrataAppBuilder,
    };
    #[cfg(target_os = "linux")]
    pub use crate::runtime::{
        launch_gtk, launch_gtk_with_events, launch_gtk_with_frame, launch_wayland,
        launch_wayland_with_events, launch_x11, launch_x11_with_events,
    };
    #[cfg(target_os = "macos")]
    pub use crate::runtime::{launch_macos, launch_macos_with_events};
    #[cfg(windows)]
    pub use crate::runtime::{launch_win32, launch_win32_with_events};
    #[cfg(target_os = "linux")]
    pub use strata_platform::{
        gtk_begin_window_drag, gtk_close, gtk_get_clipboard_text, gtk_is_maximized, gtk_minimize,
        gtk_set_clipboard_text, gtk_toggle_maximize,
    };
    pub use strata_platform::{HoverTooltipStyle, WindowAnchor, WindowConfig};
}
