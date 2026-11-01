//! Host OS Detection and Multi-Backend Android Emulator Orchestration.

use std::path::Path;

/// Operating system running the host test runner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostPlatform {
    Linux,
    Windows,
    MacOS,
    Unknown,
}

/// Detects host virtualization capabilities and hardware hypervisors.
pub struct HostDetector;

impl HostDetector {
    /// Detects the compile-time and runtime host operating system.
    pub fn current_platform() -> HostPlatform {
        if cfg!(target_os = "linux") {
            HostPlatform::Linux
        } else if cfg!(target_os = "windows") {
            HostPlatform::Windows
        } else if cfg!(target_os = "macos") {
            HostPlatform::MacOS
        } else {
            HostPlatform::Unknown
        }
    }

    /// Checks if Linux KVM (`/dev/kvm`) hardware virtualization is accessible.
    pub fn has_kvm() -> bool {
        Path::new("/dev/kvm").exists()
    }

    /// Checks if Linux container Binder IPC (`/dev/binder` or `/dev/binderfs`) exists for Waydroid/Redroid.
    pub fn has_binder_ipc() -> bool {
        Path::new("/dev/binder").exists() || Path::new("/dev/binderfs").exists()
    }
}

/// Headless GPU emulation mode for the Android virtual device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EmulatorGpuMode {
    #[default]
    SwiftShaderIndirect,
    AngleIndirect,
    HostDirect,
    GuestSoftware,
}

impl EmulatorGpuMode {
    pub fn as_arg(&self) -> &'static str {
        match self {
            Self::SwiftShaderIndirect => "swiftshader_indirect",
            Self::AngleIndirect => "angle_indirect",
            Self::HostDirect => "host",
            Self::GuestSoftware => "guest",
        }
    }
}

/// Configuration settings for launching a headless Android AVD instance.
#[derive(Debug, Clone, PartialEq)]
pub struct EmulatorConfig {
    pub avd_name: String,
    pub no_window: bool,
    pub no_audio: bool,
    pub no_boot_anim: bool,
    pub gpu_mode: EmulatorGpuMode,
    pub memory_mb: u32,
    pub cores: u32,
    pub read_only: bool,
}

impl Default for EmulatorConfig {
    fn default() -> Self {
        Self {
            avd_name: "strata_ci_avd".to_string(),
            no_window: true,
            no_audio: true,
            no_boot_anim: true,
            gpu_mode: EmulatorGpuMode::SwiftShaderIndirect,
            memory_mb: 3072,
            cores: 4,
            read_only: true,
        }
    }
}

impl EmulatorConfig {
    pub fn new(avd_name: impl Into<String>) -> Self {
        Self {
            avd_name: avd_name.into(),
            ..Default::default()
        }
    }

    /// Generates standard CLI flags for `emulator`.
    pub fn generate_args(&self) -> Vec<String> {
        let mut args = vec!["-avd".to_string(), self.avd_name.clone()];

        if self.no_window {
            args.push("-no-window".to_string());
        }
        if self.no_audio {
            args.push("-no-audio".to_string());
        }
        if self.no_boot_anim {
            args.push("-no-boot-anim".to_string());
        }

        args.push("-gpu".to_string());
        args.push(self.gpu_mode.as_arg().to_string());

        args.push("-memory".to_string());
        args.push(self.memory_mb.to_string());

        args.push("-cores".to_string());
        args.push(self.cores.to_string());

        if self.read_only {
            args.push("-read-only".to_string());
        }

        args
    }
}

/// Evaluator checking Android boot completion status.
pub struct BootStatusChecker;

impl BootStatusChecker {
    /// Returns true if the output from `getprop sys.boot_completed` signifies a fully booted system.
    pub fn is_boot_completed(output: &str) -> bool {
        output.trim() == "1"
    }
}
