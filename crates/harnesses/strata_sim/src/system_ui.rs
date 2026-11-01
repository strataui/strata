//! Status Bar, System UI Compositor, Notification Shade, and Quick Settings Control Center.

/// Status bar text and glyph theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusBarStyle {
    LightContent, // White icons/text on dark backgrounds
    DarkContent,  // Black icons/text on light backgrounds
}

/// Status bar clock mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockDisplayMode {
    RealTime,
    MarketingScreenshot(u8, u8), // e.g., 9:41 AM (Apple) or 12:00 PM (Android)
    Frozen,
}

/// Active cellular radio generation tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellularTech {
    Tech5G,
    Tech5GPlus,
    TechLte,
    Tech3G,
    NoService,
}

/// Live status bar data model.
#[derive(Debug, Clone, PartialEq)]
pub struct StatusBarModel {
    pub style: StatusBarStyle,
    pub clock_mode: ClockDisplayMode,
    pub carrier_label: String,
    pub cellular_bars: u8,
    pub cellular_tech: CellularTech,
    pub wifi_bars: u8,
    pub battery_level: f32,
    pub is_charging: bool,
    pub is_battery_saver: bool,
    pub camera_privacy_dot: bool,
    pub mic_privacy_dot: bool,
}

impl Default for StatusBarModel {
    fn default() -> Self {
        Self::new()
    }
}

impl StatusBarModel {
    pub fn new() -> Self {
        Self {
            style: StatusBarStyle::LightContent,
            clock_mode: ClockDisplayMode::MarketingScreenshot(9, 41),
            carrier_label: "Strata Mobile".to_string(),
            cellular_bars: 4,
            cellular_tech: CellularTech::Tech5G,
            wifi_bars: 3,
            battery_level: 0.88,
            is_charging: false,
            is_battery_saver: false,
            camera_privacy_dot: false,
            mic_privacy_dot: false,
        }
    }

    /// Returns the formatted clock string according to active mode.
    pub fn formatted_time(&self) -> String {
        match self.clock_mode {
            ClockDisplayMode::MarketingScreenshot(h, m) => format!("{}:{:02}", h, m),
            ClockDisplayMode::Frozen => "9:41".to_string(),
            ClockDisplayMode::RealTime => "12:00".to_string(),
        }
    }
}

/// Interactive Control Center / Quick Settings tile state.
#[derive(Debug, Clone, PartialEq)]
pub struct ControlCenterModel {
    pub is_open: bool,
    pub brightness: f32,
    pub volume: f32,
    pub wifi_enabled: bool,
    pub bluetooth_enabled: bool,
    pub cellular_enabled: bool,
    pub airplane_mode: bool,
    pub flashlight_enabled: bool,
    pub dark_mode_enabled: bool,
    pub battery_saver_enabled: bool,
}

impl Default for ControlCenterModel {
    fn default() -> Self {
        Self {
            is_open: false,
            brightness: 0.80,
            volume: 0.65,
            wifi_enabled: true,
            bluetooth_enabled: true,
            cellular_enabled: true,
            airplane_mode: false,
            flashlight_enabled: false,
            dark_mode_enabled: true,
            battery_saver_enabled: false,
        }
    }
}

/// Mock notification entry in Notification Shade.
#[derive(Debug, Clone, PartialEq)]
pub struct MockNotification {
    pub id: u64,
    pub app_name: String,
    pub title: String,
    pub message: String,
    pub timestamp_str: String,
}

/// Notification shade panel.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct NotificationShadeModel {
    pub is_open: bool,
    pub notifications: Vec<MockNotification>,
}
