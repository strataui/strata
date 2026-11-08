//! Apple iOS SimCtl Automation Driver and Tier 2 High-Fidelity Mock Runner.

/// Apple virtualization licensing tier enforcing Apple SLA Section 2B.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppleLicenseTier {
    /// Tier 1: Executing on legitimate Apple-branded hardware (macOS host / AWS EC2 Mac).
    Tier1AppleHost,
    /// Tier 2: Executing on Non-Mac host (Linux / Windows / Docker) using Strata's High-Fidelity Mock Runner.
    Tier2NonMacHost,
}

impl AppleLicenseTier {
    /// Detects the applicable licensing tier based on the host OS.
    pub fn detect() -> Self {
        if cfg!(target_os = "macos") {
            Self::Tier1AppleHost
        } else {
            Self::Tier2NonMacHost
        }
    }
}

/// Headless command generator for official `xcrun simctl` CLI tools on Apple hardware.
pub struct SimCtlDriver;

impl SimCtlDriver {
    pub fn build_list_devices_cmd() -> Vec<&'static str> {
        vec!["xcrun", "simctl", "list", "devices", "available", "-j"]
    }

    pub fn build_boot_cmd(device_id: &str) -> Vec<String> {
        vec![
            "xcrun".to_string(),
            "simctl".to_string(),
            "boot".to_string(),
            device_id.to_string(),
        ]
    }

    pub fn build_install_cmd(device_id: &str, app_path: &str) -> Vec<String> {
        vec![
            "xcrun".to_string(),
            "simctl".to_string(),
            "install".to_string(),
            device_id.to_string(),
            app_path.to_string(),
        ]
    }

    pub fn build_launch_cmd(device_id: &str, bundle_id: &str) -> Vec<String> {
        vec![
            "xcrun".to_string(),
            "simctl".to_string(),
            "launch".to_string(),
            "--console-pty".to_string(),
            device_id.to_string(),
            bundle_id.to_string(),
        ]
    }

    pub fn build_screenshot_cmd(device_id: &str, out_path: &str) -> Vec<String> {
        vec![
            "xcrun".to_string(),
            "simctl".to_string(),
            "io".to_string(),
            device_id.to_string(),
            "screenshot".to_string(),
            out_path.to_string(),
        ]
    }

    pub fn build_shutdown_cmd(device_id: &str) -> Vec<String> {
        vec![
            "xcrun".to_string(),
            "simctl".to_string(),
            "shutdown".to_string(),
            device_id.to_string(),
        ]
    }

    /// Executes `xcrun simctl list devices available -j` and returns available simulators.
    pub fn list_devices_exec() -> Result<String, String> {
        let output = std::process::Command::new("xcrun")
            .args(["simctl", "list", "devices", "available", "-j"])
            .output()
            .map_err(|e| format!("Failed to execute xcrun simctl list: {e}"))?;
        if output.status.success() {
            String::from_utf8(output.stdout).map_err(|e| e.to_string())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).to_string())
        }
    }

    /// Boots the specified iOS simulator device via `xcrun simctl boot <device_id>`.
    pub fn boot_device_exec(device_id: &str) -> Result<(), String> {
        let output = std::process::Command::new("xcrun")
            .args(["simctl", "boot", device_id])
            .output()
            .map_err(|e| format!("Failed to execute xcrun simctl boot: {e}"))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).to_string())
        }
    }

    /// Installs an iOS application bundle on the simulator via `xcrun simctl install`.
    pub fn install_app_exec(device_id: &str, app_path: &str) -> Result<(), String> {
        let output = std::process::Command::new("xcrun")
            .args(["simctl", "install", device_id, app_path])
            .output()
            .map_err(|e| format!("Failed to execute xcrun simctl install: {e}"))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).to_string())
        }
    }

    /// Launches an installed application via `xcrun simctl launch --console-pty`. Returns the process ID.
    pub fn launch_app_exec(device_id: &str, bundle_id: &str) -> Result<u32, String> {
        let output = std::process::Command::new("xcrun")
            .args(["simctl", "launch", "--console-pty", device_id, bundle_id])
            .output()
            .map_err(|e| format!("Failed to execute xcrun simctl launch: {e}"))?;
        if output.status.success() {
            let out_str = String::from_utf8_lossy(&output.stdout);
            let pid = out_str
                .split_whitespace()
                .last()
                .and_then(|p| p.parse::<u32>().ok())
                .unwrap_or(0);
            Ok(pid)
        } else {
            Err(String::from_utf8_lossy(&output.stderr).to_string())
        }
    }

    /// Captures a screenshot from the simulator via `xcrun simctl io screenshot`.
    pub fn capture_screenshot_exec(device_id: &str, out_path: &str) -> Result<(), String> {
        let output = std::process::Command::new("xcrun")
            .args(["simctl", "io", device_id, "screenshot", out_path])
            .output()
            .map_err(|e| format!("Failed to execute xcrun simctl io screenshot: {e}"))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).to_string())
        }
    }

    /// Shuts down the specified simulator via `xcrun simctl shutdown`.
    pub fn shutdown_device_exec(device_id: &str) -> Result<(), String> {
        let output = std::process::Command::new("xcrun")
            .args(["simctl", "shutdown", device_id])
            .output()
            .map_err(|e| format!("Failed to execute xcrun simctl shutdown: {e}"))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).to_string())
        }
    }
}

/// Supported Apple iOS & iPadOS reference device models.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IosDeviceModel {
    IPhone16Pro,
    IPhone16ProMax,
    IPhone15,
    IPhone14,
    IPadPro13M4,
}

/// High-fidelity viewport and chassis simulation geometry for Non-Mac hosts.
#[derive(Debug, Clone, PartialEq)]
pub struct MockIosViewport {
    pub model: IosDeviceModel,
    pub width_pt: f32,
    pub height_pt: f32,
    pub dpr: f32,
    pub refresh_hz: f32,
    pub safe_area_top_pt: f32,
    pub safe_area_bottom_pt: f32,
    pub dynamic_island_pt: Option<(f32, f32, f32, f32)>, // (x, y, width, height)
    pub sensor_notch_height_pt: f32,
}

impl MockIosViewport {
    pub fn for_model(model: IosDeviceModel) -> Self {
        match model {
            IosDeviceModel::IPhone16Pro => Self {
                model,
                width_pt: 393.0,
                height_pt: 852.0,
                dpr: 3.0,
                refresh_hz: 120.0,
                safe_area_top_pt: 59.0,
                safe_area_bottom_pt: 34.0,
                dynamic_island_pt: Some(((393.0 - 126.0) * 0.5, 11.0, 126.0, 37.0)),
                sensor_notch_height_pt: 0.0,
            },
            IosDeviceModel::IPhone16ProMax => Self {
                model,
                width_pt: 440.0,
                height_pt: 956.0,
                dpr: 3.0,
                refresh_hz: 120.0,
                safe_area_top_pt: 59.0,
                safe_area_bottom_pt: 34.0,
                dynamic_island_pt: Some(((440.0 - 126.0) * 0.5, 11.0, 126.0, 37.0)),
                sensor_notch_height_pt: 0.0,
            },
            IosDeviceModel::IPhone15 => Self {
                model,
                width_pt: 393.0,
                height_pt: 852.0,
                dpr: 3.0,
                refresh_hz: 60.0,
                safe_area_top_pt: 59.0,
                safe_area_bottom_pt: 34.0,
                dynamic_island_pt: Some(((393.0 - 126.0) * 0.5, 11.0, 126.0, 37.0)),
                sensor_notch_height_pt: 0.0,
            },
            IosDeviceModel::IPhone14 => Self {
                model,
                width_pt: 390.0,
                height_pt: 844.0,
                dpr: 3.0,
                refresh_hz: 60.0,
                safe_area_top_pt: 47.0,
                safe_area_bottom_pt: 34.0,
                dynamic_island_pt: None,
                sensor_notch_height_pt: 30.0,
            },
            IosDeviceModel::IPadPro13M4 => Self {
                model,
                width_pt: 1024.0,
                height_pt: 1366.0,
                dpr: 2.0,
                refresh_hz: 120.0,
                safe_area_top_pt: 24.0,
                safe_area_bottom_pt: 20.0,
                dynamic_island_pt: None,
                sensor_notch_height_pt: 0.0,
            },
        }
    }

    /// Physical screen resolution in pixels.
    pub fn physical_resolution(&self) -> (u32, u32) {
        (
            (self.width_pt * self.dpr).round() as u32,
            (self.height_pt * self.dpr).round() as u32,
        )
    }

    /// Evaluates rubber-band scroll resistance overscroll displacement.
    /// Standard iOS decay formula: `displacement = (1.0 - (1.0 / ((overscroll * 0.55 / dimension) + 1.0))) * dimension`
    pub fn calculate_rubber_band_displacement(overscroll: f32, dimension: f32) -> f32 {
        if overscroll.abs() < 1e-4 || dimension <= 0.0 {
            return 0.0;
        }
        let sign = if overscroll > 0.0 { 1.0 } else { -1.0 };
        let abs_overscroll = overscroll.abs();
        let c = 0.55;
        let d = (1.0 - (1.0 / ((abs_overscroll * c / dimension) + 1.0))) * dimension;
        sign * d
    }
}
