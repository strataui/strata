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
pub use strata_platform::WindowConfig;
pub use strata_primitives as primitives;
pub use strata_render as render;
pub use strata_storage as storage;
pub use strata_style as style;
pub use strata_vfs as fs;
pub use strata_web_api as web_api;

#[cfg(feature = "audio")]
pub use strata_audio as audio;
#[cfg(feature = "codegen-js")]
pub use strata_codegen_js as codegen_js;
#[cfg(feature = "device")]
pub use strata_device as device;
#[cfg(feature = "devtools")]
pub use strata_devtools as devtools;
#[cfg(feature = "icons")]
pub use strata_icons as icons;
#[cfg(feature = "mobile-test")]
pub use strata_mobile_test as mobile_test;
#[cfg(feature = "parallel")]
pub use strata_parallel as parallel;
#[cfg(feature = "sim")]
pub use strata_sim as sim;
#[cfg(feature = "tui")]
pub use strata_tui as tui;
#[cfg(feature = "ui")]
pub use strata_ui as ui;

#[cfg(feature = "agent-ui")]
pub use strata_agent_ui as agent_ui;
#[cfg(feature = "aria")]
pub use strata_aria as aria;
#[cfg(feature = "charts")]
pub use strata_charts as charts;
#[cfg(feature = "crdt")]
pub use strata_crdt as crdt;
#[cfg(feature = "editor")]
pub use strata_editor as editor;
#[cfg(feature = "form")]
pub use strata_form as form;
#[cfg(feature = "i18n")]
pub use strata_i18n as i18n;
#[cfg(feature = "motion")]
pub use strata_motion as motion;
#[cfg(feature = "query")]
pub use strata_query as query;
#[cfg(feature = "router")]
pub use strata_router as router;
#[cfg(feature = "store")]
pub use strata_store as store;
#[cfg(feature = "table")]
pub use strata_table as table;
#[cfg(feature = "toast")]
pub use strata_toast as toast;
#[cfg(feature = "virtual")]
pub use strata_virtual as virtualizer;

pub mod benchmarks;
pub mod low_level;
#[cfg(unix)]
pub mod mcp_bridge;
pub mod runtime;
pub mod view_transition;
#[cfg(unix)]
pub use mcp_bridge::{default_dev_socket_path, start_mcp_dev_listener};

#[cfg(not(unix))]
pub mod mcp_bridge {
    use std::path::PathBuf;
    pub fn default_dev_socket_path() -> PathBuf {
        PathBuf::from("/tmp/strata-dev.sock")
    }
    pub fn start_mcp_dev_listener(_path: Option<PathBuf>) -> Result<(), std::io::Error> {
        Ok(())
    }
}
pub use benchmarks::{BenchmarkHarness, BenchmarkReport};
pub use low_level::{LowLevelRenderer, OffscreenBuffer, RawSurfaceHandle};
#[cfg(not(unix))]
pub use mcp_bridge::{default_dev_socket_path, start_mcp_dev_listener};
pub use runtime::{
    app, launch, launch_auto, launch_headless, launch_headless_pump, launch_window,
    launch_with_config, launch_with_events, launch_with_frame, queue_edit_command, AppFrameInfo,
    AppLifecycleHooks, AppRuntime, DynamicTextBinding, EditCommand, FrameOutcome,
    KeyDispatchResult, PointerDownAction, PointerMoveAction, PointerUpAction, StrataApp,
    StrataAppBuilder,
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
pub use view_transition::{ViewTransitionManager, ViewTransitionSnapshot};

pub mod prelude {
    pub use strata_dom;
    pub use strata_dom::{
        clear_cursor_override, create_anchored_portal, create_dropdown_portal, create_portal,
        current_cursor, dispatch_click_outside, dispatch_global_keydown, set_current_cursor,
        set_system_color_scheme, set_system_contrast, set_system_reduced_motion,
        set_viewport_dimensions, use_active, use_click_outside, use_clipboard, use_color_scheme,
        use_contrast, use_cursor, use_effect, use_focus, use_focus_within, use_hover,
        use_keyboard_shortcut, use_media_query, use_memo, use_press, use_reduced_motion,
        use_signal, use_state, AnchorPlacement, Attribute, ClickOutsideHandle, ClipboardHandle,
        ColorScheme, ContrastPreference, CursorIcon, Event, EventType, FocusHandlers,
        HoverHandlers, IntoNodeBlueprint, KeyChord, NodeBlueprint, Portal, PortalBuilder,
        PressHandlers, ShortcutHandle, Signal, Tag, View,
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

    #[cfg(feature = "aria")]
    pub use strata_aria::floating::Placement as AriaPlacement;
    #[cfg(feature = "aria")]
    pub use strata_aria::focus_trap::FocusTrap as AriaFocusTrap;
    #[cfg(feature = "aria")]
    pub use strata_aria::roving_tabindex::{Orientation as AriaOrientation, RovingTabIndex};
    #[cfg(feature = "aria")]
    pub use strata_aria::{
        compute_position, use_accordion, use_dialog, use_tooltip, AccordionState, AccordionType,
        Alignment, ComputePositionOptions, ComputePositionResult, DialogConfig, DialogState,
        FloatRect, Side, TooltipConfig, TooltipState,
    };
    #[cfg(feature = "charts")]
    pub use strata_charts::prelude::*;
    #[cfg(feature = "codegen-js")]
    pub use strata_codegen_js::prelude::*;
    #[cfg(feature = "crdt")]
    pub use strata_crdt::{
        CrdtCounterStore, CrdtSignalStore, LamportClock, LwwRegister, OrSet, PnCounter,
        ReplicatedSignal, RgaSequence, SyncMessage, VectorClock,
    };
    #[cfg(feature = "device")]
    pub use strata_device::prelude::*;
    #[cfg(feature = "devtools")]
    pub use strata_devtools::*;
    #[cfg(feature = "editor")]
    pub use strata_editor::prelude::*;
    #[cfg(feature = "form")]
    pub use strata_form::field::FormField as FormFieldState;
    #[cfg(feature = "form")]
    pub use strata_form::{use_field_array, use_form, FieldArray, Form, ValidationRule};
    #[cfg(feature = "i18n")]
    pub use strata_i18n::prelude::*;
    #[cfg(feature = "icons")]
    pub use strata_icons::Icons;
    #[cfg(feature = "mobile-test")]
    pub use strata_mobile_test::*;
    #[cfg(feature = "motion")]
    pub use strata_motion::prelude::*;
    #[cfg(feature = "parallel")]
    pub use strata_parallel::prelude::*;
    #[cfg(feature = "query")]
    pub use strata_query::prelude::*;
    #[cfg(feature = "router")]
    pub use strata_router::prelude::*;
    #[cfg(feature = "sim")]
    pub use strata_sim as sim;
    #[cfg(feature = "sim")]
    pub use strata_sim::{
        AppMetadata, DeviceFormFactor, DeviceManufacturer, DeviceOrientation, DevicePosture,
        DeviceProfile, DynamicAlertKind, DynamicIslandOscillator, DynamicIslandState,
        FoldableState, GeometrySolver, MobileSimulatorWorkspace, MultiAppSandbox, NavAction,
        ScreenProfile, ThreeButtonNavBar, VirtualHalBus,
    };
    #[cfg(feature = "store")]
    pub use strata_store::prelude::*;
    #[cfg(feature = "table")]
    pub use strata_table::table::Table as DataTableState;
    #[cfg(feature = "table")]
    pub use strata_table::{use_table, ColumnDef, ColumnPin, SortDirection};
    #[cfg(feature = "toast")]
    pub use strata_toast::toast::ToastKind as SonnerToastKind;
    #[cfg(feature = "toast")]
    pub use strata_toast::{
        compute_stack_layout, toast_dismiss, toast_error, toast_info, toast_success, toast_warning,
        StackedToastVisual, ToastAction, ToastItem, ToastManager, ToastPosition,
    };
    #[cfg(feature = "tui")]
    pub use strata_tui::prelude::*;
    #[cfg(feature = "ui")]
    pub use strata_ui::prelude::*;
    #[cfg(feature = "virtual")]
    pub use strata_virtual::prelude::*;

    pub use crate::benchmarks::{BenchmarkHarness, BenchmarkReport};
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
    pub use strata_platform::{WindowAnchor, WindowConfig};
}
