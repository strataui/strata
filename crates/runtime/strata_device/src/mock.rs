//! DeviceMockController: Thread-safe hardware simulation controller for tests, CI, and emulators.

use std::sync::{Arc, Mutex, OnceLock};
use crate::signals::{notify_memory_pressure, sync_signals_from_mock};
use crate::types::*;

/// Snapshot of simulated hardware telemetry.
#[derive(Debug, Clone, PartialEq)]
#[derive(Default)]
pub struct DeviceTelemetrySnapshot {
    pub battery: BatteryInfo,
    pub thermal: ThermalState,
    pub network: NetworkStatus,
    pub screen: ScreenGeometry,
    pub display_refresh: DisplayRefreshInfo,
    pub gpu: GpuInfo,
}


static GLOBAL_MOCK: OnceLock<Arc<Mutex<DeviceTelemetrySnapshot>>> = OnceLock::new();

fn get_global_mock() -> &'static Arc<Mutex<DeviceTelemetrySnapshot>> {
    GLOBAL_MOCK.get_or_init(|| Arc::new(Mutex::new(DeviceTelemetrySnapshot::default())))
}

/// Controller allowing programmatic mutation of simulated device telemetry.
#[derive(Debug, Clone)]
pub struct DeviceMockController {
    state: Arc<Mutex<DeviceTelemetrySnapshot>>,
}

impl Default for DeviceMockController {
    fn default() -> Self {
        Self::new()
    }
}

impl DeviceMockController {
    /// Obtains a controller handle connected to the global hardware telemetry simulation.
    pub fn new() -> Self {
        Self {
            state: Arc::clone(get_global_mock()),
        }
    }

    /// Resets all simulated device telemetry to standard factory defaults.
    pub fn reset_to_defaults(&self) {
        let mut guard = self.state.lock().unwrap();
        *guard = DeviceTelemetrySnapshot::default();
        drop(guard);
        sync_signals_from_mock(self);
    }

    /// Gets a copy of current battery info.
    pub fn battery(&self) -> BatteryInfo {
        self.state.lock().unwrap().battery
    }

    /// Updates battery info and notifies reactive signals.
    pub fn set_battery(&self, info: BatteryInfo) {
        self.state.lock().unwrap().battery = info;
        sync_signals_from_mock(self);
    }

    /// Updates battery level in range [0.0, 1.0].
    pub fn set_battery_level(&self, level: f32) {
        let mut guard = self.state.lock().unwrap();
        guard.battery.level = level.clamp(0.0, 1.0);
        drop(guard);
        sync_signals_from_mock(self);
    }

    /// Gets current thermal state.
    pub fn thermal(&self) -> ThermalState {
        self.state.lock().unwrap().thermal
    }

    /// Updates thermal state and notifies reactive signals.
    pub fn set_thermal(&self, state: ThermalState) {
        self.state.lock().unwrap().thermal = state;
        sync_signals_from_mock(self);
    }

    /// Gets current network status.
    pub fn network(&self) -> NetworkStatus {
        self.state.lock().unwrap().network
    }

    /// Updates network status and notifies reactive signals.
    pub fn set_network(&self, status: NetworkStatus) {
        self.state.lock().unwrap().network = status;
        sync_signals_from_mock(self);
    }

    /// Sets online status.
    pub fn set_online(&self, is_online: bool) {
        let mut guard = self.state.lock().unwrap();
        guard.network.is_online = is_online;
        drop(guard);
        sync_signals_from_mock(self);
    }

    /// Gets current screen geometry.
    pub fn screen(&self) -> ScreenGeometry {
        self.state.lock().unwrap().screen
    }

    /// Updates screen geometry and notifies reactive signals.
    pub fn set_screen(&self, screen: ScreenGeometry) {
        self.state.lock().unwrap().screen = screen;
        sync_signals_from_mock(self);
    }

    /// Sets screen safe area insets (notches, dynamic island).
    pub fn set_safe_area(&self, insets: SafeAreaInsets) {
        let mut guard = self.state.lock().unwrap();
        guard.screen.safe_area = insets;
        drop(guard);
        sync_signals_from_mock(self);
    }

    /// Gets current display refresh info.
    pub fn display_refresh(&self) -> DisplayRefreshInfo {
        self.state.lock().unwrap().display_refresh
    }

    /// Updates display refresh characteristics and capabilities.
    pub fn set_display_refresh(&self, info: DisplayRefreshInfo) {
        self.state.lock().unwrap().display_refresh = info;
        sync_signals_from_mock(self);
    }

    /// Gets current GPU info.
    pub fn gpu(&self) -> GpuInfo {
        self.state.lock().unwrap().gpu.clone()
    }

    /// Updates GPU introspection info.
    pub fn set_gpu(&self, gpu: GpuInfo) {
        self.state.lock().unwrap().gpu = gpu;
    }

    /// Triggers a system memory pressure notification across all registered callbacks.
    pub fn trigger_memory_pressure(&self, level: MemoryPressure) {
        notify_memory_pressure(level);
    }
}
