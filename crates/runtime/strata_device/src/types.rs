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
