//! Phase 8 Hardware Buses integration tests: Serial/UART loopback, USB Raw HID reports,
//! and Bluetooth LE GATT Central + SIG Heart Rate Service (0x180D / 0x2A37) parsing.

use strata_hardware::ble::{
    clear_latest_ble_heart_rate_bpm, latest_ble_heart_rate_bpm, parse_heart_rate_measurement,
    use_ble_devices, MockBleAdapter, UUID_BATTERY_LEVEL, UUID_HEART_RATE_MEASUREMENT,
    UUID_HEART_RATE_SERVICE,
};
use strata_hardware::hid::{list_hid_devices, HidDeviceInfo, MockHidDevice};
use strata_hardware::serial::{list_serial_ports, MockSerialEndpoint, SerialConfig};

#[test]
fn test_serial_uart_discovery_and_mock_mavlink_loopback() {
    let _ = list_serial_ports();
    let cfg = SerialConfig {
        baud_rate: 921_600,
        ..Default::default()
    };
    let (gcs, autopilot) = MockSerialEndpoint::pair(cfg);

    let mavlink_heartbeat = b"\xFD\x09\x00\x00\x01\x01\x01\x00\x00\x00STRATA";
    assert_eq!(gcs.write(mavlink_heartbeat), mavlink_heartbeat.len());

    let mut rx = [0u8; 64];
    let n = autopilot.read(&mut rx);
    assert_eq!(&rx[..n], mavlink_heartbeat);

    // Bidirectional reply
    let ack = b"ACK_OK";
    autopilot.write(ack);
    let m = gcs.read(&mut rx);
    assert_eq!(&rx[..m], ack);
}

#[test]
fn test_usb_raw_hid_discovery_and_mock_feature_input_roundtrip() {
    let _ = list_hid_devices();
    let dev = MockHidDevice::new(HidDeviceInfo {
        path: "/dev/hidraw99".into(),
        vendor_id: 0x28DE,
        product_id: 0x1142,
        name: "Strata Telemetry Deck HID".into(),
        serial_number: Some("SN-001".into()),
    });

    dev.push_input_report(vec![0x01, 0xAA, 0xBB, 0xCC]);
    assert_eq!(dev.read_report(), Some(vec![0x01, 0xAA, 0xBB, 0xCC]));

    dev.write_report(vec![0x02, 0xFF]);
    assert_eq!(dev.written_reports(), vec![vec![0x02, 0xFF]]);

    dev.send_feature_report(0x05, vec![0x10, 0x20]);
    assert_eq!(dev.get_feature_report(0x05), Some(vec![0x10, 0x20]));
}

#[test]
fn test_ble_gatt_scan_read_write_and_heart_rate_0x2a37_parsing() {
    clear_latest_ble_heart_rate_bpm();
    let adapter = MockBleAdapter::with_demo_heart_rate_monitor();

    let found = adapter.scan(Some(UUID_HEART_RATE_SERVICE));
    assert_eq!(found.len(), 1);
    let dev_id = &found[0].id;
    assert_eq!(use_ble_devices(&adapter).get().len(), 1);

    // Must fail before connect
    assert!(adapter
        .read_characteristic(dev_id, UUID_BATTERY_LEVEL)
        .is_err());

    adapter.connect(dev_id).unwrap();
    let batt = adapter
        .read_characteristic(dev_id, UUID_BATTERY_LEVEL)
        .unwrap();
    assert_eq!(batt, vec![94]);

    // Emit 16-bit BPM (flag 0x17: u16 BPM + contact detected + RR interval)
    // BPM = 142 (0x008E LE), RR = 512 (500.0 ms)
    let hr_packet = vec![0x17, 0x8E, 0x00, 0x00, 0x02];
    adapter.emit_notification(dev_id, UUID_HEART_RATE_MEASUREMENT, hr_packet.clone());
    let sample = parse_heart_rate_measurement(&hr_packet).unwrap();
    assert_eq!(sample.bpm, 142);
    assert_eq!(sample.sensor_contact_detected, Some(true));
    assert_eq!(sample.rr_intervals_ms.len(), 1);
    assert!((sample.rr_intervals_ms[0] - 500.0).abs() < 0.5);
    assert_eq!(latest_ble_heart_rate_bpm(), Some(142));

    let notifs = adapter.drain_notifications(dev_id, UUID_HEART_RATE_MEASUREMENT);
    assert_eq!(notifs.len(), 1);

    adapter.disconnect(dev_id).unwrap();
}
