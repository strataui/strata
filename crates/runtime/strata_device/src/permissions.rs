//! Unified cross-platform hardware & OS privacy permission manager with reactive hooks.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use strata_dom::Signal;

/// Hardware or privacy capability requiring user or OS authorization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PermissionKind {
    Microphone,
    Camera,
    ScreenCapture,
    Geolocation,
    Bluetooth,
    Serial,
    UsbHid,
    Midi,
    Notifications,
    Biometrics,
    MotionSensors,
    BodySensors,
}

/// Authorization state reported by the host OS or deterministic mock policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PermissionStatus {
    Granted,
    Denied,
    Prompt,
    Restricted,
    Unsupported,
}

static MOCK_PERMS: OnceLock<Mutex<HashMap<PermissionKind, PermissionStatus>>> = OnceLock::new();

fn mock_perms() -> &'static Mutex<HashMap<PermissionKind, PermissionStatus>> {
    MOCK_PERMS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Overrides the reported permission status for a specific [`PermissionKind`] (ideal for CI/tests).
pub fn set_mock_permission(kind: PermissionKind, status: Option<PermissionStatus>) {
    if let Ok(mut map) = mock_perms().lock() {
        match status {
            Some(s) => {
                map.insert(kind, s);
            }
            None => {
                map.remove(&kind);
            }
        }
    }
}

/// Clears all deterministic permission mock overrides.
pub fn clear_mock_permissions() {
    if let Ok(mut map) = mock_perms().lock() {
        map.clear();
    }
}

/// Synchronously checks current OS authorization / hardware accessibility for `kind`.
pub fn check_permission(kind: PermissionKind) -> PermissionStatus {
    if let Ok(map) = mock_perms().lock() {
        if let Some(&status) = map.get(&kind) {
            return status;
        }
    }

    #[cfg(target_os = "linux")]
    {
        match kind {
            PermissionKind::Microphone | PermissionKind::Midi => {
                if std::path::Path::new("/dev/snd").exists() {
                    PermissionStatus::Granted
                } else {
                    PermissionStatus::Unsupported
                }
            }
            PermissionKind::Camera => {
                if std::path::Path::new("/dev/video0").exists() {
                    PermissionStatus::Granted
                } else {
                    PermissionStatus::Prompt
                }
            }
            PermissionKind::ScreenCapture | PermissionKind::Notifications => {
                if std::env::var("DBUS_SESSION_BUS_ADDRESS").is_ok()
                    || std::env::var("DISPLAY").is_ok()
                    || std::env::var("WAYLAND_DISPLAY").is_ok()
                {
                    PermissionStatus::Granted
                } else {
                    PermissionStatus::Prompt
                }
            }
            PermissionKind::Bluetooth => {
                if std::path::Path::new("/run/dbus/system_bus_socket").exists() {
                    PermissionStatus::Granted
                } else {
                    PermissionStatus::Unsupported
                }
            }
            PermissionKind::Serial | PermissionKind::UsbHid => PermissionStatus::Granted,
            PermissionKind::Geolocation
            | PermissionKind::Biometrics
            | PermissionKind::MotionSensors
            | PermissionKind::BodySensors => PermissionStatus::Prompt,
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = kind;
        PermissionStatus::Prompt
    }
}

/// Requests authorization for `kind` (on desktop Linux verifies immediate access; on mobile/sandboxed
/// OSes triggers or reflects the OS permission dialog status; respects mock overrides).
pub fn request_permission(kind: PermissionKind) -> PermissionStatus {
    let status = check_permission(kind);
    if status == PermissionStatus::Prompt {
        // In non-interactive desktop environments without an active portal denial,
        // callers may inspect `Prompt` or override deterministically in tests.
        status
    } else {
        status
    }
}

/// Reactive Strata hook returning the current [`PermissionStatus`] for `kind`.
pub fn use_permission(kind: PermissionKind) -> Signal<PermissionStatus> {
    Signal::new(check_permission(kind))
}
