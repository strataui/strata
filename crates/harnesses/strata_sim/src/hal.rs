//! Virtual Hardware Abstraction Layer (HAL) Bus simulating network, battery, thermals, sensors, and GPS.

/// Wireless network radio technology.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkRadioMode {
    Wifi6,
    Cellular5G,
    Cellular4G,
    Cellular3G,
    Offline,
}

/// Hardware power source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerSource {
    Battery,
    AcAdapter,
    WirelessQi,
}

/// Device thermal condition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThermalState {
    Nominal,
    Fair,
    Serious,
    Critical,
}

/// 3D motion and orientation telemetry.
#[derive(Debug, Clone, PartialEq)]
pub struct SensorTelemetry {
    pub pitch_deg: f32,
    pub yaw_deg: f32,
    pub roll_deg: f32,
    pub acceleration_g: [f32; 3],
}

impl Default for SensorTelemetry {
    fn default() -> Self {
        Self {
            pitch_deg: 0.0,
            yaw_deg: 0.0,
            roll_deg: 0.0,
            acceleration_g: [0.0, 0.0, 1.0], // 1G downwards
        }
    }
}

/// High-precision simulated GPS position.
#[derive(Debug, Clone, PartialEq)]
pub struct GpsCoordinates {
    pub latitude: f64,
    pub longitude: f64,
    pub altitude_m: f64,
    pub accuracy_m: f32,
}

impl Default for GpsCoordinates {
    fn default() -> Self {
        // Default to Cupertino, CA
        Self {
            latitude: 37.3346,
            longitude: -122.0090,
            altitude_m: 65.0,
            accuracy_m: 5.0,
        }
    }
}

/// Central Virtual Hardware Abstraction Layer (HAL) Bus.
#[derive(Debug, Clone, PartialEq)]
pub struct VirtualHalBus {
    pub radio_mode: NetworkRadioMode,
    pub airplane_mode: bool,
    pub network_latency_ms: u32,
    pub network_packet_loss: f32,
    pub bandwidth_kbps: u32,
    pub battery_level: f32, // 0.0..=1.0
    pub is_charging: bool,
    pub power_source: PowerSource,
    pub battery_saver_mode: bool,
    pub thermal_state: ThermalState,
    pub speaker_volume: f32, // 0.0..=1.0
    pub microphone_muted: bool,
    pub microphone_active: bool,
    pub simulated_mic_amplitude: f32,
    pub sensors: SensorTelemetry,
    pub gps: GpsCoordinates,
}

impl Default for VirtualHalBus {
    fn default() -> Self {
        Self::new()
    }
}

impl VirtualHalBus {
    /// Initializes a new HAL bus with standard nominal mobile states.
    pub fn new() -> Self {
        Self {
            radio_mode: NetworkRadioMode::Wifi6,
            airplane_mode: false,
            network_latency_ms: 12,
            network_packet_loss: 0.0,
            bandwidth_kbps: 150_000,
            battery_level: 0.88,
            is_charging: false,
            power_source: PowerSource::Battery,
            battery_saver_mode: false,
            thermal_state: ThermalState::Nominal,
            speaker_volume: 0.75,
            microphone_muted: false,
            microphone_active: false,
            simulated_mic_amplitude: 0.0,
            sensors: SensorTelemetry::default(),
            gps: GpsCoordinates::default(),
        }
    }

    /// Returns true if the virtual device possesses an active network connection.
    pub fn is_online(&self) -> bool {
        !self.airplane_mode && self.radio_mode != NetworkRadioMode::Offline
    }

    /// Toggles airplane mode, severing cellular and wireless connections and syncing to strata_device.
    pub fn set_airplane_mode(&mut self, enabled: bool) {
        self.airplane_mode = enabled;
        self.sync_to_device_runtime();
    }

    /// Sets the current battery charge fraction and charging state, syncing to strata_device.
    pub fn set_battery(&mut self, level: f32, is_charging: bool) {
        self.battery_level = level.clamp(0.0, 1.0);
        self.is_charging = is_charging;
        self.power_source = if is_charging {
            PowerSource::AcAdapter
        } else {
            PowerSource::Battery
        };
        self.sync_to_device_runtime();
    }

    /// Sets battery saver mode and syncs to strata_device.
    pub fn set_battery_saver(&mut self, enabled: bool) {
        self.battery_saver_mode = enabled;
        self.sync_to_device_runtime();
    }

    /// Sets device thermal condition and syncs to strata_device.
    pub fn set_thermal_state(&mut self, state: ThermalState) {
        self.thermal_state = state;
        self.sync_to_device_runtime();
    }

    /// Sets active network radio mode and syncs to strata_device.
    pub fn set_radio_mode(&mut self, mode: NetworkRadioMode) {
        self.radio_mode = mode;
        self.sync_to_device_runtime();
    }

    /// Injects network latency and packet loss into the virtual bus.
    pub fn inject_network_impairment(&mut self, rtt_ms: u32, packet_loss: f32) {
        self.network_latency_ms = rtt_ms;
        self.network_packet_loss = packet_loss.clamp(0.0, 1.0);
        self.sync_to_device_runtime();
    }

    /// Updates the 3D tilt coordinates from user mouse interaction.
    pub fn set_tilt(&mut self, pitch_deg: f32, yaw_deg: f32) {
        self.sensors.pitch_deg = pitch_deg.clamp(-45.0, 45.0);
        self.sensors.yaw_deg = yaw_deg.clamp(-45.0, 45.0);
    }

    /// Updates GPS geographic position.
    pub fn set_gps_location(&mut self, latitude: f64, longitude: f64, altitude_m: f64) {
        self.gps.latitude = latitude;
        self.gps.longitude = longitude;
        self.gps.altitude_m = altitude_m;
    }

    /// Resolves active screen refresh rate cap in Hz. In battery saver mode, capped at 60Hz.
    pub fn effective_refresh_rate(&self, baseline_hz: f32) -> f32 {
        if self.battery_saver_mode {
            baseline_hz.min(60.0)
        } else {
            baseline_hz
        }
    }

    /// Synchronizes Virtual HAL state into the global strata_device reactive runtime.
    /// This causes applications using `use_battery()`, `use_thermal()`, and `use_network()`
    /// to immediately update reactively.
    pub fn sync_to_device_runtime(&self) {
        let mock = strata_device::DeviceMockController::new();

        // 1. Battery Telemetry
        let battery_state = if self.is_charging {
            strata_device::BatteryState::Charging
        } else if self.power_source == PowerSource::Battery {
            if self.battery_level >= 0.99 {
                strata_device::BatteryState::Full
            } else {
                strata_device::BatteryState::Discharging
            }
        } else {
            strata_device::BatteryState::NotCharging
        };

        let time_to_empty = if !self.is_charging && self.battery_level > 0.0 {
            Some((self.battery_level * 18000.0) as u32)
        } else {
            None
        };

        let time_to_full = if self.is_charging && self.battery_level < 1.0 {
            Some(((1.0 - self.battery_level) * 5400.0) as u32)
        } else {
            None
        };

        mock.set_battery(strata_device::BatteryInfo {
            level: self.battery_level,
            state: battery_state,
            is_low_power_mode: self.battery_saver_mode,
            time_to_empty_secs: time_to_empty,
            time_to_full_secs: time_to_full,
        });

        // 2. Thermal State
        let thermal = match self.thermal_state {
            ThermalState::Nominal => strata_device::ThermalState::Nominal,
            ThermalState::Fair => strata_device::ThermalState::Fair,
            ThermalState::Serious => strata_device::ThermalState::Serious,
            ThermalState::Critical => strata_device::ThermalState::Critical,
        };
        mock.set_thermal(thermal);

        // 3. Network Status
        let (conn_type, cell_gen, metered) = match self.radio_mode {
            NetworkRadioMode::Wifi6 => (strata_device::ConnectionType::Wifi, None, false),
            NetworkRadioMode::Cellular5G => (
                strata_device::ConnectionType::Cellular,
                Some(strata_device::CellularGen::FiveG),
                true,
            ),
            NetworkRadioMode::Cellular4G => (
                strata_device::ConnectionType::Cellular,
                Some(strata_device::CellularGen::FourG),
                true,
            ),
            NetworkRadioMode::Cellular3G => (
                strata_device::ConnectionType::Cellular,
                Some(strata_device::CellularGen::ThreeG),
                true,
            ),
            NetworkRadioMode::Offline => (strata_device::ConnectionType::None, None, false),
        };

        let is_online = !self.airplane_mode && self.radio_mode != NetworkRadioMode::Offline;
        let downlink_mbps = if is_online {
            Some(self.bandwidth_kbps as f32 / 1000.0)
        } else {
            Some(0.0)
        };

        mock.set_network(strata_device::NetworkStatus {
            is_online,
            connection_type: if self.airplane_mode {
                strata_device::ConnectionType::None
            } else {
                conn_type
            },
            cellular_generation: cell_gen,
            downlink_mbps,
            rtt_ms: Some(self.network_latency_ms),
            is_metered: metered,
        });
    }
}
