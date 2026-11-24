//! Strata Device: Hardware introspection, OS Geolocation/GPS, hardware sensors, and reactive signals.
//!
//! Provides reactive hooks for geolocation (`use_geolocation`), compass (`use_compass`),
//! 6-DoF IMU (`use_imu`), barometer (`use_barometer`), ambient light (`use_ambient_light`),
//! battery state, thermals, network, display geometry, safe area insets, and system-wide
//! memory pressure eviction callbacks.

pub mod mock;
pub mod os_sensors;
pub mod os_telemetry;
pub mod signals;
pub mod types;

pub use mock::{DeviceMockController, DeviceTelemetrySnapshot};
pub use os_sensors::{
    clear_ip_geolocation_cache, probe_linux_iio_ambient_light_in_dir,
    probe_linux_iio_barometer_in_dir, probe_linux_iio_compass_in_dir, probe_linux_iio_imu_in_dir,
    probe_nmea_file, probe_os_ambient_light, probe_os_barometer, probe_os_compass,
    probe_os_geolocation, probe_os_geolocation_async, probe_os_imu, refresh_sensors_from_hardware,
    register_web_api_geolocation_provider,
};
pub use os_telemetry::{
    probe_hardware_telemetry, probe_os_battery, probe_os_network, probe_os_thermal,
    refresh_from_hardware,
};
pub use signals::{
    clear_memory_pressure_listeners, notify_memory_pressure, on_memory_pressure,
    poll_geolocation_updates, request_geolocation_refresh_async, unregister_memory_pressure,
    use_ambient_light, use_barometer, use_battery, use_compass, use_display_refresh,
    use_geolocation, use_geolocation_with_options, use_imu, use_network, use_screen, use_thermal,
};
pub use types::{
    isa_pressure_altitude_m, lux_to_daylight_factor, AmbientLightReading, BarometerReading,
    BatteryInfo, BatteryState, CellularGen, ColorGamut, CompassReading, ConnectionType,
    DisplayRefreshInfo, GeolocationInfo, GeolocationSource, GpuBackend, GpuInfo, ImuReading,
    MemoryPressure, NetworkStatus, SafeAreaInsets, ScreenGeometry, ThermalState,
};

/// Device telemetry and hardware sensor prelude module.
pub mod prelude {
    pub use super::mock::DeviceMockController;
    pub use super::os_sensors::{
        probe_os_ambient_light, probe_os_barometer, probe_os_compass, probe_os_geolocation,
        probe_os_geolocation_async, probe_os_imu, refresh_sensors_from_hardware,
        register_web_api_geolocation_provider,
    };
    pub use super::signals::{
        clear_memory_pressure_listeners, notify_memory_pressure, on_memory_pressure,
        poll_geolocation_updates, request_geolocation_refresh_async, unregister_memory_pressure,
        use_ambient_light, use_barometer, use_battery, use_compass, use_display_refresh,
        use_geolocation, use_geolocation_with_options, use_imu, use_network, use_screen,
        use_thermal,
    };
    pub use super::types::{
        isa_pressure_altitude_m, lux_to_daylight_factor, AmbientLightReading, BarometerReading,
        BatteryInfo, BatteryState, CellularGen, ColorGamut, CompassReading, ConnectionType,
        DisplayRefreshInfo, GeolocationInfo, GeolocationSource, GpuBackend, GpuInfo, ImuReading,
        MemoryPressure, NetworkStatus, SafeAreaInsets, ScreenGeometry, ThermalState,
    };
}
