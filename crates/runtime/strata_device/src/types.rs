//! Data models and enums representing native device telemetry and hardware states.

/// Battery charging state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BatteryState {
    Charging,
    Discharging,
    #[default]
    Full,
    NotCharging,
    Unknown,
}

/// Comprehensive battery telemetry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BatteryInfo {
    /// Battery level in range [0.0, 1.0].
    pub level: f32,
    /// Current battery charging state.
    pub state: BatteryState,
    /// Whether OS low power / battery saver mode is active.
    pub is_low_power_mode: bool,
    /// Estimated seconds remaining until battery reaches 0%.
    pub time_to_empty_secs: Option<u32>,
    /// Estimated seconds remaining until battery reaches 100%.
    pub time_to_full_secs: Option<u32>,
}

impl Default for BatteryInfo {
    fn default() -> Self {
        Self {
            level: 1.0,
            state: BatteryState::Full,
            is_low_power_mode: false,
            time_to_empty_secs: None,
            time_to_full_secs: None,
        }
    }
}

/// Thermal throttling state of the device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum ThermalState {
    /// Normal operating thermal conditions.
    #[default]
    Nominal,
    /// Elevated thermal conditions; active cooling (fans) engaged.
    Fair,
    /// Heavy thermal throttling; frame rates capped to 60Hz.
    Serious,
    /// Extreme thermal state; immediate power throttling, cap to 30Hz, disable 3D.
    Critical,
}

/// System-wide memory pressure level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum MemoryPressure {
    /// Memory consumption is within normal operational boundaries.
    #[default]
    Normal,
    /// High memory pressure; caches, glyph atlases, and inactive subtrees should be purged.
    Warning,
    /// Imminent OS process termination; emergency eviction of all non-essential allocations.
    Critical,
}

/// Underlying GPU rendering backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GpuBackend {
    Metal,
    Vulkan,
    Direct3D12,
    WebGpu,
    OpenGl,
    #[default]
    Software,
}

/// GPU hardware introspection information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuInfo {
    pub backend: GpuBackend,
    pub device_name: String,
    pub driver_version: String,
    pub vram_total_bytes: u64,
    pub vram_used_bytes: u64,
}

impl Default for GpuInfo {
    fn default() -> Self {
        Self {
            backend: GpuBackend::Software,
            device_name: "Strata Software Rasterizer".to_string(),
            driver_version: "1.0.0".to_string(),
            vram_total_bytes: 0,
            vram_used_bytes: 0,
        }
    }
}

/// Display refresh rate characteristics and capabilities.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DisplayRefreshInfo {
    pub current_hz: f32,
    pub max_hz: f32,
    pub is_variable_refresh_rate: bool,
    pub is_high_refresh_rate: bool,
}

impl Default for DisplayRefreshInfo {
    fn default() -> Self {
        Self {
            current_hz: 60.0,
            max_hz: 60.0,
            is_variable_refresh_rate: false,
            is_high_refresh_rate: false,
        }
    }
}

/// Active network interface connection type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConnectionType {
    #[default]
    Wifi,
    Cellular,
    Ethernet,
    Bluetooth,
    Unknown,
    None,
}

/// Cellular radio generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellularGen {
    TwoG,
    ThreeG,
    FourG,
    FiveG,
}

/// Network connectivity and bandwidth telemetry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NetworkStatus {
    pub is_online: bool,
    pub connection_type: ConnectionType,
    pub cellular_generation: Option<CellularGen>,
    pub downlink_mbps: Option<f32>,
    pub rtt_ms: Option<u32>,
    pub is_metered: bool,
}

impl Default for NetworkStatus {
    fn default() -> Self {
        Self {
            is_online: true,
            connection_type: ConnectionType::Wifi,
            cellular_generation: None,
            downlink_mbps: Some(100.0),
            rtt_ms: Some(20),
            is_metered: false,
        }
    }
}

/// Screen safe area insets for notches, Dynamic Island, and curved display bezels.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SafeAreaInsets {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl SafeAreaInsets {
    pub const fn zero() -> Self {
        Self {
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
            left: 0.0,
        }
    }

    pub const fn all(val: f32) -> Self {
        Self {
            top: val,
            right: val,
            bottom: val,
            left: val,
        }
    }

    pub const fn symmetric(vertical: f32, horizontal: f32) -> Self {
        Self {
            top: vertical,
            right: horizontal,
            bottom: vertical,
            left: horizontal,
        }
    }
}

/// Supported display color gamut standard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorGamut {
    #[default]
    Srgb,
    DisplayP3,
    Rec2020,
}

/// Geometry, scale factor, and color capabilities of the active display.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenGeometry {
    pub width_px: u32,
    pub height_px: u32,
    pub scale_factor: f32,
    pub safe_area: SafeAreaInsets,
    pub color_gamut: ColorGamut,
}

impl Default for ScreenGeometry {
    fn default() -> Self {
        Self {
            width_px: 1280,
            height_px: 800,
            scale_factor: 1.0,
            safe_area: SafeAreaInsets::zero(),
            color_gamut: ColorGamut::Srgb,
        }
    }
}

/// Origin source of a geolocation fix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GeolocationSource {
    #[default]
    None,
    MockOverride,
    EnvOverride,
    OsNativeDaemon,
    HardwareGpsNmea,
    IpGeolocationFallback,
}

/// Complete geolocation telemetry fix for reactive UI and map centering.
#[derive(Debug, Clone, PartialEq)]
pub struct GeolocationInfo {
    pub is_available: bool,
    pub latitude: f64,
    pub longitude: f64,
    pub altitude_m: Option<f64>,
    pub accuracy_m: f64,
    pub heading_deg: Option<f64>,
    pub speed_mps: Option<f64>,
    pub timestamp_ms: u64,
    pub source: GeolocationSource,
}

impl Default for GeolocationInfo {
    fn default() -> Self {
        Self {
            is_available: false,
            latitude: 0.0,
            longitude: 0.0,
            altitude_m: None,
            accuracy_m: 0.0,
            heading_deg: None,
            speed_mps: None,
            timestamp_ms: 0,
            source: GeolocationSource::None,
        }
    }
}

impl GeolocationInfo {
    /// Constructs an available geolocation fix stamped with current wall-clock milliseconds.
    pub fn available(
        latitude: f64,
        longitude: f64,
        accuracy_m: f64,
        source: GeolocationSource,
    ) -> Self {
        let timestamp_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        Self {
            is_available: true,
            latitude,
            longitude,
            altitude_m: None,
            accuracy_m: accuracy_m.max(0.0),
            heading_deg: None,
            speed_mps: None,
            timestamp_ms,
            source,
        }
    }

    pub fn with_altitude(mut self, altitude_m: Option<f64>) -> Self {
        self.altitude_m = altitude_m;
        self
    }

    pub fn with_motion(mut self, heading_deg: Option<f64>, speed_mps: Option<f64>) -> Self {
        self.heading_deg = heading_deg.map(|h| h.rem_euclid(360.0));
        self.speed_mps = speed_mps.filter(|s| s.is_finite() && *s >= 0.0);
        self
    }
}

/// Magnetometer / compass telemetry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CompassReading {
    pub available: bool,
    /// Magnetic heading in degrees `[0.0, 360.0)` clockwise from magnetic north.
    pub magnetic_heading_deg: f32,
    /// True geographic heading in degrees `[0.0, 360.0)` clockwise from true north.
    pub true_heading_deg: f32,
    /// Estimated heading uncertainty in degrees.
    pub heading_accuracy_deg: f32,
    /// Raw magnetic flux density vector `[x, y, z]` in microteslas (uT).
    pub magnetic_field_ut: [f32; 3],
}

impl Default for CompassReading {
    fn default() -> Self {
        Self {
            available: false,
            magnetic_heading_deg: 0.0,
            true_heading_deg: 0.0,
            heading_accuracy_deg: 0.0,
            magnetic_field_ut: [0.0; 3],
        }
    }
}

impl CompassReading {
    /// Computes normalized compass heading from a horizontal/3-axis magnetometer reading `[mx, my, mz]`
    /// in microteslas (uT) and optional magnetic declination in degrees.
    pub fn from_magnetic_field_ut(magnetic_field_ut: [f32; 3], declination_deg: f32) -> Self {
        let [mx, my, _mz] = magnetic_field_ut;
        let heading = mx.atan2(my).to_degrees().rem_euclid(360.0);
        let true_heading = (heading + declination_deg).rem_euclid(360.0);
        let mag_norm = (mx * mx + my * my + magnetic_field_ut[2] * magnetic_field_ut[2]).sqrt();
        // Typical Earth magnetic field is ~25..65 uT; anomalies widen estimated accuracy.
        let accuracy_deg = if (20.0..=75.0).contains(&mag_norm) {
            5.0
        } else {
            18.0
        };
        Self {
            available: true,
            magnetic_heading_deg: heading,
            true_heading_deg: true_heading,
            heading_accuracy_deg: accuracy_deg,
            magnetic_field_ut,
        }
    }
}

/// 6-DoF Inertial Measurement Unit (accelerometer + gyroscope + fused attitude) reading.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImuReading {
    pub available: bool,
    /// Linear acceleration including gravity `[ax, ay, az]` in m/s^2.
    pub accel_mps2: [f32; 3],
    /// Angular velocity `[gx, gy, gz]` in rad/s.
    pub gyro_rps: [f32; 3],
    /// Fused orientation Euler angles `(roll_deg, pitch_deg, yaw_deg)`.
    pub attitude_deg: [f32; 3],
    /// Unit quaternion `[x, y, z, w]`.
    pub quaternion: [f32; 4],
}

impl Default for ImuReading {
    fn default() -> Self {
        Self {
            available: false,
            accel_mps2: [0.0, 0.0, 9.80665],
            gyro_rps: [0.0; 3],
            attitude_deg: [0.0; 3],
            quaternion: [0.0, 0.0, 0.0, 1.0],
        }
    }
}

impl ImuReading {
    /// Computes static gravity-aligned roll/pitch and unit quaternion from raw accelerometer (`m/s^2`)
    /// and gyroscope (`rad/s`) vectors.
    pub fn from_accel_gyro(accel_mps2: [f32; 3], gyro_rps: [f32; 3]) -> Self {
        let [ax, ay, az] = accel_mps2;
        let roll_rad = ay.atan2(az);
        let pitch_rad = (-ax).atan2((ay * ay + az * az).sqrt());
        let yaw_rad = 0.0_f32;

        let (sr, cr) = (roll_rad * 0.5).sin_cos();
        let (sp, cp) = (pitch_rad * 0.5).sin_cos();
        let (sy, cy) = (yaw_rad * 0.5).sin_cos();

        let qx = sr * cp * cy - cr * sp * sy;
        let qy = cr * sp * cy + sr * cp * sy;
        let qz = cr * cp * sy - sr * sp * cy;
        let qw = cr * cp * cy + sr * sp * sy;
        let q_len = (qx * qx + qy * qy + qz * qz + qw * qw).sqrt().max(1e-6);

        Self {
            available: true,
            accel_mps2,
            gyro_rps,
            attitude_deg: [roll_rad.to_degrees(), pitch_rad.to_degrees(), 0.0],
            quaternion: [qx / q_len, qy / q_len, qz / q_len, qw / q_len],
        }
    }
}

/// Computes International Standard Atmosphere (ISA) pressure altitude in meters from barometric
/// pressure in hectopascals (hPa / mbar):
/// `h = 44330.0 * (1.0 - (P / 1013.25)^0.190295)`.
#[inline]
pub fn isa_pressure_altitude_m(pressure_hpa: f32) -> f32 {
    if !pressure_hpa.is_finite() || pressure_hpa <= 0.0 {
        return 0.0;
    }
    44330.0 * (1.0 - (pressure_hpa / 1013.25).powf(0.190295))
}

/// Barometric pressure and ISA pressure-altitude sensor reading.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BarometerReading {
    pub available: bool,
    /// Atmospheric pressure in hectopascals (hPa / mbar).
    pub pressure_hpa: f32,
    /// Estimated altitude above mean sea level in meters using the ISA barometric formula.
    pub pressure_altitude_m: f32,
    /// Sensor die temperature in Celsius, if available.
    pub temperature_c: Option<f32>,
}

impl Default for BarometerReading {
    fn default() -> Self {
        Self {
            available: false,
            pressure_hpa: 1013.25,
            pressure_altitude_m: 0.0,
            temperature_c: None,
        }
    }
}

impl BarometerReading {
    pub fn from_hpa(pressure_hpa: f32, temperature_c: Option<f32>) -> Self {
        Self {
            available: true,
            pressure_hpa,
            pressure_altitude_m: isa_pressure_altitude_m(pressure_hpa),
            temperature_c,
        }
    }
}

/// Maps illuminance in lux to a smooth perceptual daylight factor in `[0.0, 1.0]`
/// (`0.0` = dark room, `1.0` = bright outdoor daylight >= 10,000 lux).
#[inline]
pub fn lux_to_daylight_factor(illuminance_lux: f32) -> f32 {
    if !illuminance_lux.is_finite() || illuminance_lux <= 0.0 {
        return 0.0;
    }
    ((1.0 + illuminance_lux).ln() / (1.0 + 10_000.0_f32).ln()).clamp(0.0, 1.0)
}

/// Ambient light sensor (ALS) reading.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AmbientLightReading {
    pub available: bool,
    /// Ambient illuminance in lux.
    pub illuminance_lux: f32,
    /// Suggested UI daylight adaptation factor in `[0.0, 1.0]`.
    pub daylight_factor: f32,
}

impl Default for AmbientLightReading {
    fn default() -> Self {
        Self {
            available: false,
            illuminance_lux: 0.0,
            daylight_factor: 0.0,
        }
    }
}

impl AmbientLightReading {
    pub fn from_lux(illuminance_lux: f32) -> Self {
        let lux = illuminance_lux.max(0.0);
        Self {
            available: true,
            illuminance_lux: lux,
            daylight_factor: lux_to_daylight_factor(lux),
        }
    }
}

/// Optical / infrared proximity sensor reading (phone ear-detection / obstacle proximity).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ProximityReading {
    pub available: bool,
    pub is_near: bool,
    pub distance_cm: f32,
}

/// Physical posture inferred from a foldable or laptop lid hinge angle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FoldablePosture {
    Closed,
    HalfOpen,
    #[default]
    Flat,
    Tent,
}

/// Foldable display / laptop lid hinge angle reading in degrees (`0..=360`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HingeAngleReading {
    pub available: bool,
    pub angle_deg: f32,
    pub posture: FoldablePosture,
}

impl Default for HingeAngleReading {
    fn default() -> Self {
        Self {
            available: false,
            angle_deg: 180.0,
            posture: FoldablePosture::Flat,
        }
    }
}

impl HingeAngleReading {
    pub fn from_degrees(angle_deg: f32) -> Self {
        let clamped = angle_deg.clamp(0.0, 360.0);
        let posture = if clamped < 25.0 {
            FoldablePosture::Closed
        } else if clamped < 155.0 {
            FoldablePosture::HalfOpen
        } else if clamped <= 215.0 {
            FoldablePosture::Flat
        } else {
            FoldablePosture::Tent
        };
        Self {
            available: true,
            angle_deg: clamped,
            posture,
        }
    }
}

/// Wireless radio RSSI and link-quality telemetry (Wi-Fi + Cellular).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RadioSignalReading {
    pub available: bool,
    pub wifi_interface: Option<String>,
    /// Wi-Fi received signal strength indicator in dBm (e.g., `-45` strong, `-85` weak).
    pub wifi_rssi_dbm: Option<i32>,
    /// Normalized Wi-Fi link quality percentage in `[0.0, 100.0]`.
    pub wifi_link_quality_pct: Option<f32>,
    /// Cellular signal strength in dBm, if available.
    pub cellular_rssi_dbm: Option<i32>,
}

/// Ambient Sound Pressure Level (SPL) reading bridged from live microphone audio metering.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AmbientSplReading {
    pub available: bool,
    /// IEC 61672 A-weighted ambient sound pressure level (`dBA`).
    pub spl_dba: f32,
    /// `true` when ambient noise exceeds `75 dBA` (loud industrial / cockpit environment).
    pub is_loud_environment: bool,
}

impl AmbientSplReading {
    pub fn from_dba(spl_dba: f32) -> Self {
        let val = spl_dba.max(0.0);
        Self {
            available: true,
            spl_dba: val,
            is_loud_environment: val >= 75.0,
        }
    }
}

/// Hardware step counter / pedometer reading.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PedometerReading {
    pub available: bool,
    pub steps: u64,
    pub cadence_spm: f32,
}

/// Biometric heart-rate monitor reading (bridged from BLE HRS `0x180D` or wearable sensors).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct HeartRateReading {
    pub available: bool,
    pub bpm: u16,
    pub sensor_contact: bool,
}
