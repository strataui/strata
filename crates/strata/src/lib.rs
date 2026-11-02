//! Strata: Universal High-Performance Frontend Engine & Framework in Pure Rust.

pub use strata_dom;
pub use strata_dom as dom;
pub use strata_js;
pub use strata_js as js;
pub use strata_layout as layout;
pub use strata_macro::*;
pub use strata_platform as platform;
pub use strata_primitives as primitives;
pub use strata_render as render;
pub use strata_style as style;
pub use strata_vfs as fs;
pub use strata_storage as storage;
pub use strata_web_api as web_api;

#[cfg(feature = "icons")]
pub use strata_icons as icons;
#[cfg(feature = "ui")]
pub use strata_ui as ui;
#[cfg(feature = "device")]
pub use strata_device as device;
#[cfg(feature = "parallel")]
pub use strata_parallel as parallel;
#[cfg(feature = "tui")]
pub use strata_tui as tui;
#[cfg(feature = "codegen-js")]
pub use strata_codegen_js as codegen_js;
#[cfg(feature = "devtools")]
pub use strata_devtools as devtools;
#[cfg(feature = "mobile-test")]
pub use strata_mobile_test as mobile_test;
#[cfg(feature = "sim")]
pub use strata_sim as sim;
#[cfg(feature = "a11y")]
pub use strata_a11y as a11y;
#[cfg(feature = "audio")]
pub use strata_audio as audio;
#[cfg(feature = "crdt")]
pub use strata_crdt as crdt;
#[cfg(feature = "agent-ui")]
pub use strata_agent_ui as agent_ui;
#[cfg(feature = "i18n")]
pub use strata_i18n as i18n;
#[cfg(feature = "router")]
pub use strata_router as router;
#[cfg(feature = "form")]
pub use strata_form as form;
#[cfg(feature = "query")]
pub use strata_query as query;
#[cfg(feature = "virtual")]
pub use strata_virtual as virtualizer;
#[cfg(feature = "aria")]
pub use strata_aria as aria;
#[cfg(feature = "store")]
pub use strata_store as store;
#[cfg(feature = "toast")]
pub use strata_toast as toast;
#[cfg(feature = "motion")]
pub use strata_motion as motion;
#[cfg(feature = "table")]
pub use strata_table as table;
#[cfg(feature = "charts")]
pub use strata_charts as charts;
#[cfg(feature = "editor")]
pub use strata_editor as editor;

pub mod runtime;
pub mod benchmarks;
pub mod low_level;
pub use runtime::{
    app, AppFrameInfo, AppLifecycleHooks, AppRuntime, launch, launch_headless, launch_window,
    StrataApp, StrataAppBuilder,
};
#[cfg(target_os = "linux")]
pub use runtime::{launch_x11, launch_x11_with_events, launch_gtk, launch_gtk_with_events};
#[cfg(target_os = "linux")]
pub use strata_platform::{gtk_close, gtk_get_clipboard_text, gtk_minimize, gtk_set_clipboard_text, gtk_toggle_maximize};
pub use benchmarks::{BenchmarkHarness, BenchmarkReport};
pub use low_level::{LowLevelRenderer, OffscreenBuffer, RawSurfaceHandle};

pub mod prelude {
    pub use strata_dom;
    pub use strata_js;
    pub use strata_dom::{
        use_effect, use_memo, use_signal, use_state, Attribute, Event, EventType, IntoNodeBlueprint,
        NodeBlueprint, Signal, Tag, View,
    };
    pub use strata_js::{
        json::{parse_json, stringify_json},
        promise::{drain_microtasks, pending_microtasks_count, queue_microtask, JsPromise, PromiseState},
        web::{
            advance_timers_by, clear_interval, clear_timeout, local_storage, session_storage,
            set_interval, set_timeout, tick_timers, Storage, TimerId, URLSearchParams,
        },
        JsObject, JsValue,
    };
    pub use strata_macro::{component, dynamic, rsx};
    pub use strata_platform::prelude::*;
    pub use strata_render::scene3d::{
        AmbientLight, BoxGeometry, Camera3D, Canvas3D, DirectionalLight, Geometry3D, Light3D,
        Material3D, Mesh, Mesh3D, PerspectiveCamera, Scene3D, ShaderMaterial, ShaderSource,
        SphereGeometry, StandardMaterial, Transform3D,
    };
    pub use strata_style::ComputedStyle;
    pub use strata_primitives::prelude::*;
    pub use strata_vfs::prelude::*;
    pub use strata_storage::prelude::*;
    pub use strata_web_api::prelude::*;

    #[cfg(feature = "device")]
    pub use strata_device::prelude::*;
    #[cfg(feature = "icons")]
    pub use strata_icons::Icons;
    #[cfg(feature = "ui")]
    pub use strata_ui::prelude::*;
    #[cfg(feature = "i18n")]
    pub use strata_i18n::prelude::*;
    #[cfg(feature = "router")]
    pub use strata_router::prelude::*;
    #[cfg(feature = "form")]
    pub use strata_form::{use_form, use_field_array, Form, FieldArray, ValidationRule};
    #[cfg(feature = "form")]
    pub use strata_form::field::FormField as FormFieldState;
    #[cfg(feature = "query")]
    pub use strata_query::prelude::*;
    #[cfg(feature = "virtual")]
    pub use strata_virtual::prelude::*;
    #[cfg(feature = "aria")]
    pub use strata_aria::{
        use_accordion, use_dialog, use_tooltip, compute_position,
        AccordionState, AccordionType, DialogConfig, DialogState, TooltipConfig, TooltipState,
        Alignment, ComputePositionOptions, ComputePositionResult, FloatRect, Side,
    };
    #[cfg(feature = "aria")]
    pub use strata_aria::focus_trap::FocusTrap as AriaFocusTrap;
    #[cfg(feature = "aria")]
    pub use strata_aria::roving_tabindex::{Orientation as AriaOrientation, RovingTabIndex};
    #[cfg(feature = "aria")]
    pub use strata_aria::floating::Placement as AriaPlacement;
    #[cfg(feature = "store")]
    pub use strata_store::prelude::*;
    #[cfg(feature = "toast")]
    pub use strata_toast::{
        compute_stack_layout, toast_dismiss, toast_error, toast_info, toast_success, toast_warning,
        StackedToastVisual, ToastAction, ToastItem, ToastManager, ToastPosition,
    };
    #[cfg(feature = "toast")]
    pub use strata_toast::toast::ToastKind as SonnerToastKind;
    #[cfg(feature = "motion")]
    pub use strata_motion::prelude::*;
    #[cfg(feature = "table")]
    pub use strata_table::{use_table, ColumnDef, ColumnPin, SortDirection};
    #[cfg(feature = "table")]
    pub use strata_table::table::Table as DataTableState;
    #[cfg(feature = "charts")]
    pub use strata_charts::prelude::*;
    #[cfg(feature = "editor")]
    pub use strata_editor::prelude::*;
    #[cfg(feature = "parallel")]
    pub use strata_parallel::prelude::*;
    #[cfg(feature = "tui")]
    pub use strata_tui::prelude::*;
    #[cfg(feature = "codegen-js")]
    pub use strata_codegen_js::prelude::*;
    #[cfg(feature = "devtools")]
    pub use strata_devtools::*;
    #[cfg(feature = "mobile-test")]
    pub use strata_mobile_test::*;
    #[cfg(feature = "sim")]
    pub use strata_sim as sim;
    #[cfg(feature = "sim")]
    pub use strata_sim::{
        AppMetadata, DeviceFormFactor, DeviceManufacturer, DeviceOrientation, DevicePosture,
        DeviceProfile, DynamicAlertKind, DynamicIslandOscillator, DynamicIslandState,
        FoldableState, GeometrySolver, MobileSimulatorWorkspace, MultiAppSandbox, NavAction,
        ScreenProfile, ThreeButtonNavBar, VirtualHalBus,
    };

    pub use crate::runtime::{
        app, AppFrameInfo, AppLifecycleHooks, AppRuntime, launch, launch_headless, launch_window,
        StrataApp, StrataAppBuilder,
    };
    #[cfg(target_os = "linux")]
    pub use crate::runtime::{launch_x11, launch_x11_with_events, launch_gtk, launch_gtk_with_events};
    #[cfg(target_os = "linux")]
    pub use strata_platform::{gtk_close, gtk_get_clipboard_text, gtk_minimize, gtk_set_clipboard_text, gtk_toggle_maximize};
    pub use crate::benchmarks::{BenchmarkHarness, BenchmarkReport};
}
