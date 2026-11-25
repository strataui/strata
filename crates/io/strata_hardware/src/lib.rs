//! Strata Hardware: Zero-C-dependency physical hardware I/O buses (Serial/UART, USB Raw HID,
//! Bluetooth LE GATT), Gamepad/HOTAS controllers, and unified tactile Haptics engine.

pub mod ble;
pub mod gamepad;
pub mod haptics;
pub mod hid;
pub mod serial;

pub use ble::{
    clear_latest_ble_heart_rate_bpm, latest_ble_heart_rate_bpm, parse_heart_rate_measurement,
    probe_bluez_adapter_available, use_ble_devices, BleDeviceInfo, BleGattCharacteristic,
    BleGattService, BleHeartRateSample, MockBleAdapter, MockBlePeripheral, UUID_BATTERY_LEVEL,
    UUID_BATTERY_SERVICE, UUID_HEART_RATE_MEASUREMENT, UUID_HEART_RATE_SERVICE,
};
pub use gamepad::{
    poll_gamepads, set_gamepad_rumble, use_gamepads, DeadzoneConfig, GamepadButtonState,
    GamepadMapping, GamepadState, MockGamepadController, RumbleCommand,
};
pub use haptics::{HapticPattern, HapticSegment, HapticsEngine, HapticsSink, MockHapticsSink};
pub use hid::{list_hid_devices, HidDevice, HidDeviceInfo, MockHidDevice};
pub use serial::{
    list_serial_ports, use_serial_ports, MockSerialEndpoint, SerialConfig, SerialDataBits,
    SerialFlowControl, SerialParity, SerialPort, SerialPortInfo, SerialStopBits,
};

pub mod prelude {
    pub use crate::ble::{
        latest_ble_heart_rate_bpm, parse_heart_rate_measurement, use_ble_devices, BleDeviceInfo,
        BleHeartRateSample, MockBleAdapter, UUID_BATTERY_LEVEL, UUID_BATTERY_SERVICE,
        UUID_HEART_RATE_MEASUREMENT, UUID_HEART_RATE_SERVICE,
    };
    pub use crate::gamepad::{
        poll_gamepads, set_gamepad_rumble, use_gamepads, DeadzoneConfig, GamepadButtonState,
        GamepadMapping, GamepadState, MockGamepadController, RumbleCommand,
    };
    pub use crate::haptics::{
        HapticPattern, HapticSegment, HapticsEngine, HapticsSink, MockHapticsSink,
    };
    pub use crate::hid::{list_hid_devices, HidDevice, HidDeviceInfo, MockHidDevice};
    pub use crate::serial::{
        list_serial_ports, use_serial_ports, MockSerialEndpoint, SerialConfig, SerialDataBits,
        SerialFlowControl, SerialParity, SerialPort, SerialPortInfo, SerialStopBits,
    };
}
