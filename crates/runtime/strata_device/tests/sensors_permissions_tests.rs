//! Phase 8 Expanded Sensors, Cycle-Free Provider Bridges, and Unified PermissionManager tests.

use strata_device::os_sensors::{
    parse_proc_net_wireless, probe_linux_iio_hinge_angle_in_dir, probe_linux_iio_pedometer_in_dir,
    probe_linux_iio_proximity_in_dir, register_ambient_spl_provider, register_heart_rate_provider,
};
use strata_device::permissions::{
    check_permission, clear_mock_permissions, request_permission, set_mock_permission,
    use_permission, PermissionKind, PermissionStatus,
};
use strata_device::prelude::*;
use std::fs;

fn mock_spl_provider() -> Option<f32> {
    Some(78.5)
}

fn mock_hr_provider() -> Option<u16> {
    Some(118)
}

#[test]
fn test_iio_proximity_hinge_angle_pedometer_and_proc_net_wireless_parsers() {
    let dir = std::env::temp_dir().join("strata_device_phase8_iio_test");
    let _ = fs::remove_dir_all(&dir);
    let dev0 = dir.join("iio:device0");
    fs::create_dir_all(&dev0).unwrap();

    fs::write(dev0.join("in_proximity_raw"), "3.2\n").unwrap();
    fs::write(dev0.join("in_angl_raw"), "120.0\n").unwrap();
    fs::write(dev0.join("in_angl_scale"), "1.0\n").unwrap();
    fs::write(dev0.join("in_steps_input"), "4812\n").unwrap();

    let prox = probe_linux_iio_proximity_in_dir(&dir).unwrap();
    assert!(prox.available && prox.is_near);
    assert!((prox.distance_cm - 3.2).abs() < 1e-4);

    let hinge = probe_linux_iio_hinge_angle_in_dir(&dir).unwrap();
    assert!(hinge.available);
    assert_eq!(hinge.posture, FoldablePosture::HalfOpen);
    assert!((hinge.angle_deg - 120.0).abs() < 1e-4);

    let ped = probe_linux_iio_pedometer_in_dir(&dir).unwrap();
    assert!(ped.available);
    assert_eq!(ped.steps, 4812);

    let _ = fs::remove_dir_all(&dir);

    let wireless = "\
Inter-| sta-|   Quality        |   Discarded packets               | Missed | WE
 face | tus | link level noise |  nwid  crypt   frag  retry   misc | beacon | 22
  wlan0: 0000   56.  -54.  -256        0      0      0      0      0        0
";
    let radio = parse_proc_net_wireless(wireless).unwrap();
    assert!(radio.available);
    assert_eq!(radio.wifi_interface.as_deref(), Some("wlan0"));
    assert_eq!(radio.wifi_rssi_dbm, Some(-54));
    assert!((radio.wifi_link_quality_pct.unwrap() - 80.0).abs() < 0.5);
}

#[test]
fn test_cycle_free_ambient_spl_and_heart_rate_provider_bridges() {
    let mock = DeviceMockController::new();
    mock.reset_to_defaults();

    register_ambient_spl_provider(Some(mock_spl_provider));
    register_heart_rate_provider(Some(mock_hr_provider));

    let spl = use_ambient_spl().get();
    assert!(spl.available);
    assert!((spl.spl_dba - 78.5).abs() < 1e-4);
    assert!(spl.is_loud_environment);

    let hr = use_heart_rate().get();
    assert!(hr.available);
    assert_eq!(hr.bpm, 118);
    assert!(hr.sensor_contact);

    register_ambient_spl_provider(None);
    register_heart_rate_provider(None);
    mock.reset_to_defaults();
}

#[test]
fn test_unified_permission_manager_and_reactive_hook() {
    clear_mock_permissions();
    set_mock_permission(PermissionKind::Camera, Some(PermissionStatus::Granted));
    set_mock_permission(PermissionKind::Microphone, Some(PermissionStatus::Denied));

    assert_eq!(check_permission(PermissionKind::Camera), PermissionStatus::Granted);
    assert_eq!(
        request_permission(PermissionKind::Microphone),
        PermissionStatus::Denied
    );
    assert_eq!(
        use_permission(PermissionKind::Camera).get(),
        PermissionStatus::Granted
    );

    clear_mock_permissions();
}
