//! Strata Mobile Device Simulator & Virtual Phone Operating System in Pure Rust.

pub mod profile;
pub mod geometry;
pub mod hal;
pub mod posture;
pub mod island;
pub mod chassis;
pub mod system_ui;
pub mod navigation;
pub mod launcher;
pub mod sandbox;
pub mod workspace;

pub use profile::{
    AspectRatio, ChassisMaterial, ChassisProfile, CutoutAlignment, CutoutProfile, DeviceFormFactor,
    DeviceManufacturer, DeviceProfile, HingeAxis, NavigationType, PhysicalButton, ScreenProfile,
};
pub use geometry::{
    ConcentricFilletSdf, DeviceOrientation, GeometrySolver, SafeAreaInsets,
    WGSL_CONCENTRIC_FILLET_SHADER,
};
pub use hal::{
    GpsCoordinates, NetworkRadioMode, PowerSource, SensorTelemetry, ThermalState, VirtualHalBus,
};
pub use posture::{DevicePosture, FoldableState};
pub use island::{DynamicAlertKind, DynamicIslandOscillator, DynamicIslandState};
pub use chassis::{Chassis3DTransform, ChassisMeshSpec, ChassisRenderer};
pub use system_ui::{
    CellularTech, ClockDisplayMode, ControlCenterModel, MockNotification, NotificationShadeModel,
    StatusBarModel, StatusBarStyle,
};
pub use navigation::{
    ButtonLayout, ButtonShape, GestureNavEngine, NavAction, ThreeButtonNavBar,
};
pub use launcher::{AppMetadata, HomeScreenState, RecentsCarouselState};
pub use sandbox::{
    AppLifecycleState, BytecodeSandboxEngine, GuestInstruction, MultiAppSandbox,
    SandboxExecutionError, SandboxedAppInstance,
};
pub use workspace::MobileSimulatorWorkspace;

/// Prelude re-exporting all simulator constructs.
pub mod prelude {
    pub use crate::profile::*;
    pub use crate::geometry::*;
    pub use crate::hal::*;
    pub use crate::posture::*;
    pub use crate::island::*;
    pub use crate::chassis::*;
    pub use crate::system_ui::*;
    pub use crate::navigation::*;
    pub use crate::launcher::*;
    pub use crate::sandbox::*;
    pub use crate::workspace::*;
}
