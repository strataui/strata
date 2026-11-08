//! Strata Device: Hardware introspection, device telemetry, and reactive hardware signals.
//!
//! Provides reactive hooks for battery state, thermals, network, display geometry, safe area insets,
//! and system-wide memory pressure eviction callbacks.

pub mod mock;
pub mod os_telemetry;
pub mod signals;
pub mod types;

pub use mock::{DeviceMockController, DeviceTelemetrySnapshot};
pub use os_telemetry::{
    probe_hardware_telemetry, probe_os_battery, probe_os_network, probe_os_thermal,
    refresh_from_hardware,
};
pub use signals::{
    clear_memory_pressure_listeners, notify_memory_pressure, on_memory_pressure,
    unregister_memory_pressure, use_battery, use_display_refresh, use_network, use_screen,
    use_thermal,
};
pub use types::{
    BatteryInfo, BatteryState, CellularGen, ColorGamut, ConnectionType, DisplayRefreshInfo,
    GpuBackend, GpuInfo, MemoryPressure, NetworkStatus, SafeAreaInsets, ScreenGeometry,
    ThermalState,
};

/// Device telemetry prelude module.
pub mod prelude {
    pub use super::mock::DeviceMockController;
    pub use super::signals::{
        clear_memory_pressure_listeners, notify_memory_pressure, on_memory_pressure,
        unregister_memory_pressure, use_battery, use_display_refresh, use_network, use_screen,
        use_thermal,
    };
    pub use super::types::{
        BatteryInfo, BatteryState, CellularGen, ColorGamut, ConnectionType, DisplayRefreshInfo,
        GpuBackend, GpuInfo, MemoryPressure, NetworkStatus, SafeAreaInsets, ScreenGeometry,
        ThermalState,
    };
}
