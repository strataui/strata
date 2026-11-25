//! Bluetooth Low Energy (BLE) Central & GATT Client (Linux `org.bluez` over pure-Rust
//! `strata_platform::dbus`, Bluetooth SIG Heart Rate `0x180D`/`0x2A37` parser, and
//! deterministic `MockBleAdapter` for CI).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, Mutex};
use strata_dom::Signal;

/// Standard Bluetooth SIG Heart Rate Service UUID (`0x180D`).
pub const UUID_HEART_RATE_SERVICE: &str = "0000180d-0000-1000-8000-00805f9b34fb";
/// Standard Bluetooth SIG Heart Rate Measurement Characteristic UUID (`0x2A37`).
pub const UUID_HEART_RATE_MEASUREMENT: &str = "00002a37-0000-1000-8000-00805f9b34fb";
/// Standard Bluetooth SIG Battery Service UUID (`0x180F`).
pub const UUID_BATTERY_SERVICE: &str = "0000180f-0000-1000-8000-00805f9b34fb";
/// Standard Bluetooth SIG Battery Level Characteristic UUID (`0x2A19`).
pub const UUID_BATTERY_LEVEL: &str = "00002a19-0000-1000-8000-00805f9b34fb";

/// Latest BLE Heart Rate BPM published for decoupled `strata-device::use_heart_rate()` bridge (`0` = none).
static LATEST_BLE_HR_BPM: AtomicU16 = AtomicU16::new(0);

/// Returns the latest BLE Heart Rate Measurement BPM received, if any.
pub fn latest_ble_heart_rate_bpm() -> Option<u16> {
    let v = LATEST_BLE_HR_BPM.load(Ordering::Relaxed);
    if v == 0 {
        None
    } else {
        Some(v)
    }
}

/// Clears the global BLE heart-rate cache (for deterministic tests).
pub fn clear_latest_ble_heart_rate_bpm() {
    LATEST_BLE_HR_BPM.store(0, Ordering::Relaxed);
}

/// Parsed Bluetooth SIG Heart Rate Measurement (`0x2A37`) notification packet.
#[derive(Debug, Clone, PartialEq)]
pub struct BleHeartRateSample {
    pub bpm: u16,
    pub sensor_contact_detected: Option<bool>,
    pub energy_expended_kj: Option<u16>,
    /// R-R beat intervals in milliseconds (resolution `1000 / 1024 ms`).
    pub rr_intervals_ms: Vec<f32>,
}

/// Decodes a raw Bluetooth SIG Heart Rate Measurement (`0x2A37`) characteristic notification.
pub fn parse_heart_rate_measurement(bytes: &[u8]) -> Option<BleHeartRateSample> {
    let flags = *bytes.first()?;
    let is_u16 = (flags & 0x01) != 0;
    let contact_supported = (flags & 0x04) != 0;
    let contact_detected = (flags & 0x02) != 0;
    let has_energy = (flags & 0x08) != 0;
    let has_rr = (flags & 0x10) != 0;

    let mut pos = 1usize;
    let bpm = if is_u16 {
        let v = u16::from_le_bytes([*bytes.get(pos)?, *bytes.get(pos + 1)?]);
        pos += 2;
        v
    } else {
        let v = *bytes.get(pos)? as u16;
        pos += 1;
        v
    };

    let energy_expended_kj = if has_energy && pos + 2 <= bytes.len() {
        let e = u16::from_le_bytes([bytes[pos], bytes[pos + 1]]);
        pos += 2;
        Some(e)
    } else {
        None
    };

    let mut rr_intervals_ms = Vec::new();
    if has_rr {
        while pos + 2 <= bytes.len() {
            let raw = u16::from_le_bytes([bytes[pos], bytes[pos + 1]]);
            rr_intervals_ms.push((raw as f32 / 1024.0) * 1000.0);
            pos += 2;
        }
    }

    LATEST_BLE_HR_BPM.store(bpm, Ordering::Relaxed);

    Some(BleHeartRateSample {
        bpm,
        sensor_contact_detected: if contact_supported {
            Some(contact_detected)
        } else {
            None
        },
        energy_expended_kj,
        rr_intervals_ms,
    })
}

/// Metadata describing a discovered BLE peripheral.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BleDeviceInfo {
    pub id: String,
    pub address: String,
    pub name: Option<String>,
    pub rssi_dbm: i16,
    pub service_uuids: Vec<String>,
    pub connected: bool,
}

/// Descriptor for a GATT characteristic exposed by a BLE peripheral.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BleGattCharacteristic {
    pub uuid: String,
    pub readable: bool,
    pub writable: bool,
    pub notifiable: bool,
    pub value: Vec<u8>,
}

/// Descriptor for a GATT service grouping characteristics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BleGattService {
    pub uuid: String,
    pub primary: bool,
    pub characteristics: Vec<BleGattCharacteristic>,
}

/// Checks whether Linux BlueZ (`org.bluez`) is reachable over the pure-Rust system D-Bus client.
#[cfg(target_os = "linux")]
pub fn probe_bluez_adapter_available() -> bool {
    if let Ok(mut client) = strata_platform::dbus::DbusClient::connect_system() {
        client
            .call_method(
                "org.bluez",
                "/",
                "org.freedesktop.DBus.ObjectManager",
                "GetManagedObjects",
                None,
                &[],
            )
            .is_ok()
    } else {
        false
    }
}

#[cfg(not(target_os = "linux"))]
pub fn probe_bluez_adapter_available() -> bool {
    false
}

/// Simulated GATT peripheral backing [`MockBleAdapter`].
#[derive(Debug, Clone)]
pub struct MockBlePeripheral {
    pub info: BleDeviceInfo,
    pub services: Vec<BleGattService>,
}

/// Deterministic in-memory Bluetooth LE Central Adapter for headless CI & GATT protocol testing.
#[derive(Clone, Default)]
pub struct MockBleAdapter {
    peripherals: Arc<Mutex<HashMap<String, MockBlePeripheral>>>,
    notifications: Arc<Mutex<HashMap<String, Vec<Vec<u8>>>>>,
}

impl MockBleAdapter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a pre-populated adapter containing a simulated Polar-class BLE Heart Rate
    /// Monitor (`0x180D` + `0x180F` Battery Service).
    pub fn with_demo_heart_rate_monitor() -> Self {
        let adapter = Self::new();
        adapter.add_peripheral(MockBlePeripheral {
            info: BleDeviceInfo {
                id: "ble:mock-hrm-01".into(),
                address: "AA:BB:CC:11:22:33".into(),
                name: Some("Strata Polar H10 Mock".into()),
                rssi_dbm: -54,
                service_uuids: vec![
                    UUID_HEART_RATE_SERVICE.into(),
                    UUID_BATTERY_SERVICE.into(),
                ],
                connected: false,
            },
            services: vec![
                BleGattService {
                    uuid: UUID_HEART_RATE_SERVICE.into(),
                    primary: true,
                    characteristics: vec![BleGattCharacteristic {
                        uuid: UUID_HEART_RATE_MEASUREMENT.into(),
                        readable: true,
                        writable: false,
                        notifiable: true,
                        // Flags=0x06 (8-bit BPM + contact detected), BPM=72
                        value: vec![0x06, 72],
                    }],
                },
                BleGattService {
                    uuid: UUID_BATTERY_SERVICE.into(),
                    primary: true,
                    characteristics: vec![BleGattCharacteristic {
                        uuid: UUID_BATTERY_LEVEL.into(),
                        readable: true,
                        writable: false,
                        notifiable: true,
                        value: vec![94],
                    }],
                },
            ],
        });
        adapter
    }

    pub fn add_peripheral(&self, peripheral: MockBlePeripheral) {
        if let Ok(mut map) = self.peripherals.lock() {
            map.insert(peripheral.info.id.clone(), peripheral);
        }
    }

    /// Scans peripherals, optionally filtering by advertised GATT service UUID.
    pub fn scan(&self, service_uuid_filter: Option<&str>) -> Vec<BleDeviceInfo> {
        let Ok(map) = self.peripherals.lock() else {
            return Vec::new();
        };
        map.values()
            .filter(|p| match service_uuid_filter {
                Some(uuid) => p
                    .info
                    .service_uuids
                    .iter()
                    .any(|u| u.eq_ignore_ascii_case(uuid)),
                None => true,
            })
            .map(|p| p.info.clone())
            .collect()
    }

    pub fn connect(&self, device_id: &str) -> Result<(), String> {
        let mut map = self.peripherals.lock().map_err(|e| e.to_string())?;
        let dev = map
            .get_mut(device_id)
            .ok_or_else(|| format!("BLE peripheral '{device_id}' not found"))?;
        dev.info.connected = true;
        Ok(())
    }

    pub fn disconnect(&self, device_id: &str) -> Result<(), String> {
        let mut map = self.peripherals.lock().map_err(|e| e.to_string())?;
        let dev = map
            .get_mut(device_id)
            .ok_or_else(|| format!("BLE peripheral '{device_id}' not found"))?;
        dev.info.connected = false;
        Ok(())
    }

    pub fn read_characteristic(&self, device_id: &str, char_uuid: &str) -> Result<Vec<u8>, String> {
        let map = self.peripherals.lock().map_err(|e| e.to_string())?;
        let dev = map
            .get(device_id)
            .ok_or_else(|| format!("BLE peripheral '{device_id}' not found"))?;
        if !dev.info.connected {
            return Err("BLE peripheral is not connected".into());
        }
        for svc in &dev.services {
            if let Some(ch) = svc
                .characteristics
                .iter()
                .find(|c| c.uuid.eq_ignore_ascii_case(char_uuid))
            {
                return Ok(ch.value.clone());
            }
        }
        Err(format!("Characteristic '{char_uuid}' not found"))
    }

    pub fn write_characteristic(
        &self,
        device_id: &str,
        char_uuid: &str,
        value: Vec<u8>,
    ) -> Result<(), String> {
        let mut map = self.peripherals.lock().map_err(|e| e.to_string())?;
        let dev = map
            .get_mut(device_id)
            .ok_or_else(|| format!("BLE peripheral '{device_id}' not found"))?;
        if !dev.info.connected {
            return Err("BLE peripheral is not connected".into());
        }
        for svc in &mut dev.services {
            if let Some(ch) = svc
                .characteristics
                .iter_mut()
                .find(|c| c.uuid.eq_ignore_ascii_case(char_uuid))
            {
                ch.value = value;
                return Ok(());
            }
        }
        Err(format!("Characteristic '{char_uuid}' not found"))
    }

    /// Emits a characteristic notification packet (auto-updating BLE HRS global BPM if `0x2A37`).
    pub fn emit_notification(&self, device_id: &str, char_uuid: &str, payload: Vec<u8>) {
        if char_uuid.eq_ignore_ascii_case(UUID_HEART_RATE_MEASUREMENT) {
            let _ = parse_heart_rate_measurement(&payload);
        }
        if let Ok(mut notifs) = self.notifications.lock() {
            let key = format!("{device_id}:{char_uuid}");
            notifs.entry(key).or_default().push(payload);
        }
    }

    pub fn drain_notifications(&self, device_id: &str, char_uuid: &str) -> Vec<Vec<u8>> {
        let key = format!("{device_id}:{char_uuid}");
        self.notifications
            .lock()
            .ok()
            .and_then(|mut m| m.remove(&key))
            .unwrap_or_default()
    }
}

/// Reactive Strata hook returning scanned BLE devices from the provided adapter snapshot.
pub fn use_ble_devices(adapter: &MockBleAdapter) -> Signal<Vec<BleDeviceInfo>> {
    Signal::new(adapter.scan(None))
}
