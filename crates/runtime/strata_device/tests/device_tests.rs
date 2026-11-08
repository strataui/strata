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
            ThermalState::Nominal | ThermalState::Fair | ThermalState::Serious | ThermalState::Critical
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

