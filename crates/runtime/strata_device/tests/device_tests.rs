use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use strata_device::prelude::*;

#[test]
fn test_default_device_telemetry_snapshot() {
    let controller = DeviceMockController::new();
    controller.reset_to_defaults();

    let battery = controller.battery();
    assert_eq!(battery.level, 1.0);
    assert_eq!(battery.state, BatteryState::Full);
    assert!(!battery.is_low_power_mode);

    assert_eq!(controller.thermal(), ThermalState::Nominal);

    let net = controller.network();
    assert!(net.is_online);
    assert_eq!(net.connection_type, ConnectionType::Wifi);
    assert!(!net.is_metered);

    let screen = controller.screen();
    assert_eq!(screen.width_px, 1280);
    assert_eq!(screen.height_px, 800);
    assert_eq!(screen.scale_factor, 1.0);
    assert_eq!(screen.safe_area, SafeAreaInsets::zero());
    assert_eq!(screen.color_gamut, ColorGamut::Srgb);

    let refresh = controller.display_refresh();
    assert_eq!(refresh.current_hz, 60.0);
    assert!(!refresh.is_high_refresh_rate);
}

#[test]
fn test_reactive_battery_updates_via_controller() {
    let controller = DeviceMockController::new();
    controller.reset_to_defaults();

    let battery_signal = use_battery();
    assert_eq!(battery_signal.get().level, 1.0);
    assert_eq!(battery_signal.get().state, BatteryState::Full);

    controller.set_battery_level(0.42);
    assert_eq!(battery_signal.get().level, 0.42);

    controller.set_battery(BatteryInfo {
        level: 0.85,
        state: BatteryState::Charging,
        is_low_power_mode: true,
        time_to_empty_secs: None,
        time_to_full_secs: Some(1800),
    });

    let updated = battery_signal.get();
    assert_eq!(updated.level, 0.85);
    assert_eq!(updated.state, BatteryState::Charging);
    assert!(updated.is_low_power_mode);
    assert_eq!(updated.time_to_full_secs, Some(1800));
}

#[test]
fn test_reactive_thermal_and_display_refresh() {
    let controller = DeviceMockController::new();
    controller.reset_to_defaults();

    let thermal_signal = use_thermal();
    let refresh_signal = use_display_refresh();

    assert_eq!(thermal_signal.get(), ThermalState::Nominal);
    assert_eq!(refresh_signal.get().current_hz, 60.0);

    // Thermal throttling escalation
    controller.set_thermal(ThermalState::Serious);
    assert_eq!(thermal_signal.get(), ThermalState::Serious);

    controller.set_thermal(ThermalState::Critical);
    assert_eq!(thermal_signal.get(), ThermalState::Critical);

    // ProMotion 120Hz display
    controller.set_display_refresh(DisplayRefreshInfo {
        current_hz: 120.0,
        max_hz: 120.0,
        is_variable_refresh_rate: true,
        is_high_refresh_rate: true,
    });

    let refresh = refresh_signal.get();
    assert_eq!(refresh.current_hz, 120.0);
    assert!(refresh.is_high_refresh_rate);
    assert!(refresh.is_variable_refresh_rate);
}

#[test]
fn test_reactive_network_and_screen_geometry() {
    let controller = DeviceMockController::new();
    controller.reset_to_defaults();

    let network_signal = use_network();
    let screen_signal = use_screen();

    assert!(network_signal.get().is_online);
    assert_eq!(screen_signal.get().safe_area, SafeAreaInsets::zero());

    // Cutout / Dynamic Island safe area
    controller.set_safe_area(SafeAreaInsets {
        top: 59.0,
        right: 0.0,
        bottom: 34.0,
        left: 0.0,
    });
    assert_eq!(screen_signal.get().safe_area.top, 59.0);
    assert_eq!(screen_signal.get().safe_area.bottom, 34.0);

    // Drop network
    controller.set_online(false);
    assert!(!network_signal.get().is_online);
}

#[test]
fn test_memory_pressure_eviction_registry() {
    clear_memory_pressure_listeners();

    let warning_count = Arc::new(AtomicUsize::new(0));
    let critical_count = Arc::new(AtomicUsize::new(0));

    let w_clone = Arc::clone(&warning_count);
    let c_clone = Arc::clone(&critical_count);

    let id = on_memory_pressure(move |level| match level {
        MemoryPressure::Normal => {}
        MemoryPressure::Warning => {
            w_clone.fetch_add(1, Ordering::SeqCst);
        }
        MemoryPressure::Critical => {
            c_clone.fetch_add(1, Ordering::SeqCst);
        }
    });

    notify_memory_pressure(MemoryPressure::Warning);
    assert_eq!(warning_count.load(Ordering::SeqCst), 1);
    assert_eq!(critical_count.load(Ordering::SeqCst), 0);

    notify_memory_pressure(MemoryPressure::Critical);
    assert_eq!(warning_count.load(Ordering::SeqCst), 1);
    assert_eq!(critical_count.load(Ordering::SeqCst), 1);

    // Unregister and ensure no further calls
    unregister_memory_pressure(id);

    notify_memory_pressure(MemoryPressure::Critical);
    assert_eq!(critical_count.load(Ordering::SeqCst), 1); // Remains 1
}

#[test]
fn test_gpu_introspection_mutation() {
    let controller = DeviceMockController::new();
    assert_eq!(controller.gpu().backend, GpuBackend::Software);

    controller.set_gpu(GpuInfo {
        backend: GpuBackend::Metal,
        device_name: "Apple M3 Max".to_string(),
        driver_version: "Metal 3.1".to_string(),
        vram_total_bytes: 36 * 1024 * 1024 * 1024,
        vram_used_bytes: 4 * 1024 * 1024 * 1024,
    });

    let gpu = controller.gpu();
    assert_eq!(gpu.backend, GpuBackend::Metal);
    assert_eq!(gpu.device_name, "Apple M3 Max");
    assert_eq!(gpu.vram_total_bytes, 38654705664);
}

#[test]
fn test_real_os_hardware_telemetry_probing() {
    // 1. Probe OS battery
    if let Some(battery) = strata_device::probe_os_battery() {
        assert!((0.0..=1.0).contains(&battery.level));
    }

    // 2. Probe OS thermal
    if let Some(thermal) = strata_device::probe_os_thermal() {
        assert!(matches!(
            thermal,
            ThermalState::Nominal
                | ThermalState::Fair
                | ThermalState::Serious
                | ThermalState::Critical
        ));
    }

    // 3. Probe OS network
    if let Some(net) = strata_device::probe_os_network() {
        if net.is_online {
            assert!(matches!(
                net.connection_type,
                ConnectionType::Wifi
                    | ConnectionType::Ethernet
                    | ConnectionType::Cellular
                    | ConnectionType::Unknown
            ));
        }
    }

    // 4. Test live hardware refresh integration
    let _detected = strata_device::refresh_from_hardware();
}

#[test]
fn test_win32_power_status_decoding() {
    use strata_device::os_telemetry::{decode_win32_power_status, Win32SystemPowerStatus};

    // Scenario 1: AC power plugged in, battery fully charged at 100%
    let ac_full = Win32SystemPowerStatus {
        ac_line_status: 1,
        battery_flag: 1, // High
        battery_life_percent: 100,
        system_status_flag: 0,
        battery_life_time: 0xFFFF_FFFF,
        battery_full_life_time: 0xFFFF_FFFF,
    };
    let info = decode_win32_power_status(&ac_full).expect("Must decode AC full status");
    assert_eq!(info.level, 1.0);
    assert_eq!(info.state, BatteryState::Full);
    assert!(!info.is_low_power_mode);
    assert_eq!(info.time_to_empty_secs, None);

    // Scenario 2: Charging at 45% with 1800 seconds to full
    let charging = Win32SystemPowerStatus {
        ac_line_status: 1,
        battery_flag: 8, // Charging
        battery_life_percent: 45,
        system_status_flag: 0,
        battery_life_time: 0xFFFF_FFFF,
        battery_full_life_time: 1800,
    };
    let info = decode_win32_power_status(&charging).expect("Must decode charging status");
    assert_eq!(info.level, 0.45);
    assert_eq!(info.state, BatteryState::Charging);
    assert_eq!(info.time_to_full_secs, Some(1800));

    // Scenario 3: Discharging on battery at 72% with 7200 seconds remaining
    let discharging = Win32SystemPowerStatus {
        ac_line_status: 0,
        battery_flag: 1,
        battery_life_percent: 72,
        system_status_flag: 1, // Low power mode enabled
        battery_life_time: 7200,
        battery_full_life_time: 0xFFFF_FFFF,
    };
    let info = decode_win32_power_status(&discharging).expect("Must decode discharging status");
    assert_eq!(info.level, 0.72);
    assert_eq!(info.state, BatteryState::Discharging);
    assert!(info.is_low_power_mode);
    assert_eq!(info.time_to_empty_secs, Some(7200));

    // Scenario 4: Desktop PC without battery (flag 128)
    let no_battery = Win32SystemPowerStatus {
        ac_line_status: 1,
        battery_flag: 128,
        battery_life_percent: 255,
        system_status_flag: 0,
        battery_life_time: 0xFFFF_FFFF,
        battery_full_life_time: 0xFFFF_FFFF,
    };
    assert!(decode_win32_power_status(&no_battery).is_none());
}

#[test]
fn test_macos_power_source_snapshot_decoding() {
    use strata_device::os_telemetry::{decode_macos_power_snapshot, MacOsPowerSourceSnapshot};

    // Scenario 1: Fully charged on AC
    let charged = MacOsPowerSourceSnapshot {
        current_capacity: 5200,
        max_capacity: 5200,
        is_charging: false,
        is_charged: true,
        is_low_power_mode: false,
        time_to_empty_min: None,
        time_to_full_min: None,
        power_source_state: "AC Power".to_string(),
    };
    let info = decode_macos_power_snapshot(&charged).expect("Must decode charged snapshot");
    assert_eq!(info.level, 1.0);
    assert_eq!(info.state, BatteryState::Full);
    assert!(!info.is_low_power_mode);

    // Scenario 2: Fast charging at 60% with 30 minutes to full
    let charging = MacOsPowerSourceSnapshot {
        current_capacity: 3120,
        max_capacity: 5200,
        is_charging: true,
        is_charged: false,
        is_low_power_mode: true,
        time_to_empty_min: None,
        time_to_full_min: Some(30),
        power_source_state: "AC Power".to_string(),
    };
    let info = decode_macos_power_snapshot(&charging).expect("Must decode charging snapshot");
    assert_eq!(info.level, 0.60);
    assert_eq!(info.state, BatteryState::Charging);
    assert!(info.is_low_power_mode);
    assert_eq!(info.time_to_full_secs, Some(1800));

    // Scenario 3: Discharging on battery at 80% with 240 minutes remaining
    let discharging = MacOsPowerSourceSnapshot {
        current_capacity: 4000,
        max_capacity: 5000,
        is_charging: false,
        is_charged: false,
        is_low_power_mode: false,
        time_to_empty_min: Some(240),
        time_to_full_min: None,
        power_source_state: "Battery Power".to_string(),
    };
    let info = decode_macos_power_snapshot(&discharging).expect("Must decode discharging snapshot");
    assert_eq!(info.level, 0.80);
    assert_eq!(info.state, BatteryState::Discharging);
    assert_eq!(info.time_to_empty_secs, Some(14400));

    // Scenario 4: Connected to AC adapter but paused/not charging
    let ac_not_charging = MacOsPowerSourceSnapshot {
        current_capacity: 4000,
        max_capacity: 5000,
        is_charging: false,
        is_charged: false,
        is_low_power_mode: false,
        time_to_empty_min: None,
        time_to_full_min: None,
        power_source_state: "AC Power".to_string(),
    };
    let info = decode_macos_power_snapshot(&ac_not_charging).expect("Must decode AC not charging");
    assert_eq!(info.state, BatteryState::NotCharging);

    // Scenario 5: Zero max capacity returns None
    let invalid = MacOsPowerSourceSnapshot {
        current_capacity: 0,
        max_capacity: 0,
        is_charging: false,
        is_charged: false,
        is_low_power_mode: false,
        time_to_empty_min: None,
        time_to_full_min: None,
        power_source_state: "Battery Power".to_string(),
    };
    assert!(decode_macos_power_snapshot(&invalid).is_none());
}

#[test]
fn test_reactive_geolocation_and_hardware_sensor_hooks() {
    let controller = DeviceMockController::new();
    controller.reset_to_defaults();

    let geo = use_geolocation();
    let compass = use_compass();
    let imu = use_imu();
    let baro = use_barometer();
    let als = use_ambient_light();

    assert!(!geo.get().is_available);
    assert!(!compass.get().available);
    assert!(!imu.get().available);
    assert!(!baro.get().available);
    assert!(!als.get().available);

    // 1. Geolocation coordinates + reactive signal sync + probe priority
    controller.set_coordinates(41.0082, 28.9784, Some(40.0), 6.0);
    let g = geo.get();
    assert!(g.is_available);
    assert!((g.latitude - 41.0082).abs() < 1e-6);
    assert!((g.longitude - 28.9784).abs() < 1e-6);
    assert_eq!(g.altitude_m, Some(40.0));
    assert_eq!(g.source, GeolocationSource::MockOverride);

    let probed = probe_os_geolocation(false).expect("Mock override must win immediately");
    assert_eq!(probed.source, GeolocationSource::MockOverride);
    assert!((probed.latitude - 41.0082).abs() < 1e-6);

    // Bridge check: strata_web_api::Geolocation resolves through strata_device automatically
    register_web_api_geolocation_provider();
    let web_geo = strata_web_api::Geolocation::new();
    let web_pos = web_geo
        .get_current_position()
        .expect("strata_web_api::Geolocation must bridge to strata_device");
    assert!((web_pos.coords.latitude - 41.0082).abs() < 1e-6);

    // Async probe invokes callback cleanly without blocking
    let (tx, rx) = std::sync::mpsc::channel();
    probe_os_geolocation_async(false, move |res| {
        let _ = tx.send(res);
    });
    let async_fix = rx
        .recv_timeout(std::time::Duration::from_secs(2))
        .expect("Async probe must finish quickly")
        .expect("Mock override must resolve");
    assert!((async_fix.longitude - 28.9784).abs() < 1e-6);

    // 2. Compass heading & declination wraparound
    let comp_val = CompassReading::from_magnetic_field_ut([25.0, 0.0, -40.0], 5.0);
    assert!((comp_val.magnetic_heading_deg - 90.0).abs() < 0.1);
    assert!((comp_val.true_heading_deg - 95.0).abs() < 0.1);
    controller.set_compass(comp_val);
    assert!(compass.get().available);
    assert!((compass.get().true_heading_deg - 95.0).abs() < 0.1);

    // 3. 6-DoF IMU attitude & unit quaternion normalization
    let imu_val = ImuReading::from_accel_gyro([0.0, 4.9033, 8.4928], [0.01, -0.02, 0.03]);
    let [qx, qy, qz, qw] = imu_val.quaternion;
    let q_norm = (qx * qx + qy * qy + qz * qz + qw * qw).sqrt();
    assert!((q_norm - 1.0).abs() < 1e-4, "IMU quaternion must be unit length");
    assert!((imu_val.attitude_deg[0] - 30.0).abs() < 0.2, "Expected ~30 deg roll");
    controller.set_imu(imu_val);
    assert!(imu.get().available);

    // 4. Barometer ISA altitude formula accuracy
    let sea_level = isa_pressure_altitude_m(1013.25);
    assert!(sea_level.abs() < 0.1);
    let alt_1km = isa_pressure_altitude_m(898.75);
    assert!(
        (alt_1km - 1000.0).abs() < 15.0,
        "898.75 hPa should correspond to ~1000m ISA altitude, got {alt_1km}"
    );
    controller.set_barometer(BarometerReading::from_hpa(898.75, Some(15.2)));
    assert!(baro.get().available);
    assert_eq!(baro.get().temperature_c, Some(15.2));

    // 5. Ambient light lux -> monotonic daylight factor in [0, 1]
    assert_eq!(lux_to_daylight_factor(0.0), 0.0);
    let indoor = lux_to_daylight_factor(300.0);
    let sunny = lux_to_daylight_factor(10_000.0);
    assert!(indoor > 0.3 && indoor < 0.8);
    assert!((sunny - 1.0).abs() < 1e-5);
    controller.set_ambient_light(AmbientLightReading::from_lux(300.0));
    assert!(als.get().available);
    assert!((als.get().daylight_factor - indoor).abs() < 1e-5);

    controller.reset_to_defaults();
}

#[test]
fn test_linux_iio_sysfs_parsers_and_nmea_file_probe() {
    use std::fs;
    use strata_device::{
        probe_linux_iio_ambient_light_in_dir, probe_linux_iio_barometer_in_dir,
        probe_linux_iio_compass_in_dir, probe_linux_iio_imu_in_dir, probe_nmea_file,
    };

    let tmp_root = std::env::temp_dir().join(format!(
        "strata_device_iio_test_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&tmp_root);

    // Synthetic Linux IIO sysfs tree
    let dev_mag = tmp_root.join("iio:device0");
    let dev_imu = tmp_root.join("iio:device1");
    let dev_baro = tmp_root.join("iio:device2");
    let dev_als = tmp_root.join("iio:device3");
    fs::create_dir_all(&dev_mag).unwrap();
    fs::create_dir_all(&dev_imu).unwrap();
    fs::create_dir_all(&dev_baro).unwrap();
    fs::create_dir_all(&dev_als).unwrap();

    // Magnetometer: X=0, Y=400 raw @ 0.001 Gauss/LSB -> 0 uT east, 40 uT north -> heading 0 deg
    fs::write(dev_mag.join("in_magn_x_raw"), "0\n").unwrap();
    fs::write(dev_mag.join("in_magn_y_raw"), "400\n").unwrap();
    fs::write(dev_mag.join("in_magn_z_raw"), "-300\n").unwrap();
    fs::write(dev_mag.join("in_magn_scale"), "0.001\n").unwrap();

    // IMU: level gravity on Z + steady yaw rate
    fs::write(dev_imu.join("in_accel_x_raw"), "0\n").unwrap();
    fs::write(dev_imu.join("in_accel_y_raw"), "0\n").unwrap();
    fs::write(dev_imu.join("in_accel_z_raw"), "1000\n").unwrap();
    fs::write(dev_imu.join("in_accel_scale"), "0.00980665\n").unwrap();
    fs::write(dev_imu.join("in_anglvel_x_raw"), "10\n").unwrap();
    fs::write(dev_imu.join("in_anglvel_y_raw"), "-20\n").unwrap();
    fs::write(dev_imu.join("in_anglvel_z_raw"), "30\n").unwrap();
    fs::write(dev_imu.join("in_anglvel_scale"), "0.001\n").unwrap();

    // Barometer: 101.325 kPa -> 1013.25 hPa (0m ISA), temp 22.5 C
    fs::write(dev_baro.join("in_pressure_raw"), "101325\n").unwrap();
    fs::write(dev_baro.join("in_pressure_scale"), "0.001\n").unwrap();
    fs::write(dev_baro.join("in_temp_input"), "22500\n").unwrap();

    // Ambient light: 650 lux
    fs::write(dev_als.join("in_illuminance_input"), "650\n").unwrap();

    let comp = probe_linux_iio_compass_in_dir(&tmp_root).expect("IIO compass must parse");
    assert!(comp.available);
    assert!(comp.magnetic_heading_deg < 0.5 || comp.magnetic_heading_deg > 359.5);
    assert!((comp.magnetic_field_ut[1] - 40.0).abs() < 0.1);

    let imu = probe_linux_iio_imu_in_dir(&tmp_root).expect("IIO IMU must parse");
    assert!(imu.available);
    assert!((imu.accel_mps2[2] - 9.80665).abs() < 1e-3);
    assert!((imu.gyro_rps[2] - 0.03).abs() < 1e-4);
    assert!(imu.attitude_deg[0].abs() < 0.1 && imu.attitude_deg[1].abs() < 0.1);

    let baro = probe_linux_iio_barometer_in_dir(&tmp_root).expect("IIO barometer must parse");
    assert!(baro.available);
    assert!((baro.pressure_hpa - 1013.25).abs() < 0.05);
    assert!(baro.pressure_altitude_m.abs() < 0.5);
    assert!((baro.temperature_c.unwrap() - 22.5).abs() < 0.05);

    let als = probe_linux_iio_ambient_light_in_dir(&tmp_root).expect("IIO ALS must parse");
    assert!(als.available);
    assert!((als.illuminance_lux - 650.0).abs() < 0.1);
    assert!(als.daylight_factor > 0.5);

    // NMEA-0183 serial/file capture probe
    let nmea_path = tmp_root.join("gps_capture.nmea");
    let body = "GPGGA,123519,4807.038,N,01131.000,E,1,08,0.9,545.4,M,46.9,M,,";
    let ck = body.bytes().fold(0u8, |a, b| a ^ b);
    fs::write(&nmea_path, format!("$GNGSA,A,3\n${body}*{ck:02X}\n")).unwrap();
    let nmea_fix = probe_nmea_file(&nmea_path).expect("NMEA capture must parse");
    assert!(nmea_fix.is_available);
    assert_eq!(nmea_fix.source, GeolocationSource::HardwareGpsNmea);
    assert!((nmea_fix.latitude - 48.1173).abs() < 1e-4);
    assert!((nmea_fix.longitude - 11.516667).abs() < 1e-4);
    assert!((nmea_fix.altitude_m.unwrap() - 545.4).abs() < 1e-2);

    let _ = fs::remove_dir_all(&tmp_root);
}

