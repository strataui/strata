//! Strata Device: Hardware introspection, OS Geolocation/GPS, 14+ physical hardware sensors,
//! unified permissions manager, and fine-grained reactive signals.

pub mod mock;
pub mod os_sensors;
pub mod os_telemetry;
pub mod permissions;
pub mod signals;
pub mod types;

pub use mock::{DeviceMockController, DeviceTelemetrySnapshot};
pub use os_sensors::{
    clear_ip_geolocation_cache, parse_proc_net_wireless, probe_linux_iio_ambient_light_in_dir,
    probe_linux_iio_barometer_in_dir, probe_linux_iio_compass_in_dir,
    probe_linux_iio_hinge_angle_in_dir, probe_linux_iio_imu_in_dir,
    probe_linux_iio_pedometer_in_dir, probe_linux_iio_proximity_in_dir, probe_nmea_file,
    probe_os_ambient_light, probe_os_ambient_spl, probe_os_barometer, probe_os_compass,
    probe_os_geolocation, probe_os_geolocation_async, probe_os_heart_rate, probe_os_hinge_angle,
    probe_os_imu, probe_os_pedometer, probe_os_proximity, probe_os_radio_signal,
    refresh_sensors_from_hardware, register_ambient_spl_provider, register_heart_rate_provider,
    register_web_api_geolocation_provider,
};
pub use os_telemetry::{
    probe_hardware_telemetry, probe_os_battery, probe_os_network, probe_os_thermal,
    refresh_from_hardware,
};
pub use permissions::{
    check_permission, clear_mock_permissions, request_permission, set_mock_permission,
    use_permission, PermissionKind, PermissionStatus,
};
pub use signals::{
    clear_memory_pressure_listeners, notify_memory_pressure, on_memory_pressure,
    poll_geolocation_updates, request_geolocation_refresh_async, unregister_memory_pressure,
    use_ambient_light, use_ambient_spl, use_barometer, use_battery, use_compass,
    use_display_refresh, use_geolocation, use_geolocation_with_options, use_heart_rate,
    use_hinge_angle, use_imu, use_network, use_pedometer, use_proximity, use_radio_signal,
    use_screen, use_thermal,
};
pub use types::{
    isa_pressure_altitude_m, lux_to_daylight_factor, AmbientLightReading, AmbientSplReading,
    BarometerReading, BatteryInfo, BatteryState, CellularGen, ColorGamut, CompassReading,
    ConnectionType, DisplayRefreshInfo, FoldablePosture, GeolocationInfo, GeolocationSource,
    GpuBackend, GpuInfo, HeartRateReading, HingeAngleReading, ImuReading, MemoryPressure,
    NetworkStatus, PedometerReading, ProximityReading, RadioSignalReading, SafeAreaInsets,
    ScreenGeometry, ThermalState,
};

/// Device telemetry, expanded hardware sensors, and permissions prelude module.
pub mod prelude {
    pub use super::mock::DeviceMockController;
    pub use super::os_sensors::{
        probe_os_ambient_light, probe_os_ambient_spl, probe_os_barometer, probe_os_compass,
        probe_os_geolocation, probe_os_geolocation_async, probe_os_heart_rate,
        probe_os_hinge_angle, probe_os_imu, probe_os_pedometer, probe_os_proximity,
        probe_os_radio_signal, refresh_sensors_from_hardware, register_ambient_spl_provider,
        register_heart_rate_provider, register_web_api_geolocation_provider,
    };
    pub use super::permissions::{
        check_permission, clear_mock_permissions, request_permission, set_mock_permission,
        use_permission, PermissionKind, PermissionStatus,
    };
    pub use super::signals::{
        clear_memory_pressure_listeners, notify_memory_pressure, on_memory_pressure,
        poll_geolocation_updates, request_geolocation_refresh_async, unregister_memory_pressure,
        use_ambient_light, use_ambient_spl, use_barometer, use_battery, use_compass,
        use_display_refresh, use_geolocation, use_geolocation_with_options, use_heart_rate,
        use_hinge_angle, use_imu, use_network, use_pedometer, use_proximity, use_radio_signal,
        use_screen, use_thermal,
    };
    pub use super::types::{
        isa_pressure_altitude_m, lux_to_daylight_factor, AmbientLightReading, AmbientSplReading,
        BarometerReading, BatteryInfo, BatteryState, CellularGen, ColorGamut, CompassReading,
        ConnectionType, DisplayRefreshInfo, FoldablePosture, GeolocationInfo, GeolocationSource,
        GpuBackend, GpuInfo, HeartRateReading, HingeAngleReading, ImuReading, MemoryPressure,
        NetworkStatus, PedometerReading, ProximityReading, RadioSignalReading, SafeAreaInsets,
        ScreenGeometry, ThermalState,
    };
}
