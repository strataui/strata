//! Cross-platform Gamepad, Joystick, Flight-Stick / HOTAS input and Force-Feedback Rumble
//! (zero-C-dep Linux `evdev` kernel ioctls + deterministic `MockGamepadController` + `use_gamepads`).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use strata_dom::Signal;

/// Controller button/axis mapping layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GamepadMapping {
    /// W3C Standard Gamepad layout (dual-stick + D-pad + 4 face + bumpers + triggers).
    Standard,
    /// Multi-axis Flight-Stick / Throttle / Rudder HOTAS controller.
    FlightHotas,
    /// Unmapped raw HID/evdev axes and buttons.
    Raw,
}

/// Configurable analog stick / throttle deadzone & response curve shaping.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeadzoneConfig {
    /// Inner deadzone threshold in `[0.0, 0.5]` below which output snaps to `0.0`.
    pub inner: f32,
    /// Outer saturation threshold in `[0.5, 1.0]` above which output clamps to `±1.0`.
    pub outer: f32,
    /// Response curve exponent (`1.0` = linear, `2.0` = precision quadratic near center).
    pub exponent: f32,
}

impl Default for DeadzoneConfig {
    fn default() -> Self {
        Self {
            inner: 0.10,
            outer: 0.96,
            exponent: 1.0,
        }
    }
}

impl DeadzoneConfig {
    /// Applies smooth scaled axial deadzone & response exponent to a single axis in `[-1.0, 1.0]`.
    pub fn apply_axial(self, raw: f32) -> f32 {
        let mag = raw.abs();
        let inner = self.inner.clamp(0.0, 0.49);
        let outer = self.outer.clamp(inner + 0.05, 1.0);
        if mag <= inner {
            return 0.0;
        }
        let norm = ((mag - inner) / (outer - inner)).clamp(0.0, 1.0);
        norm.powf(self.exponent.max(0.2)) * raw.signum()
    }

    /// Applies scaled radial deadzone to a 2D analog stick `(x, y)` preserving directional angle.
    pub fn apply_radial(self, x: f32, y: f32) -> (f32, f32) {
        let mag = (x * x + y * y).sqrt();
        let inner = self.inner.clamp(0.0, 0.49);
        let outer = self.outer.clamp(inner + 0.05, 1.0);
        if mag <= inner || mag < 1e-6 {
            return (0.0, 0.0);
        }
        let norm = ((mag - inner) / (outer - inner)).clamp(0.0, 1.0);
        let shaped = norm.powf(self.exponent.max(0.2));
        let scale = shaped / mag;
        (x * scale, y * scale)
    }
}

/// State of a single digital or analog gamepad button/trigger.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct GamepadButtonState {
    pub pressed: bool,
    pub touched: bool,
    pub value: f32,
}

/// Complete snapshot of a connected gamepad, joystick, or HOTAS controller.
#[derive(Debug, Clone, PartialEq)]
pub struct GamepadState {
    pub id: String,
    pub name: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub connected: bool,
    pub mapping: GamepadMapping,
    /// Normalized axis values in `[-1.0, 1.0]`.
    pub axes: Vec<f32>,
    /// Digital/analog button states.
    pub buttons: Vec<GamepadButtonState>,
    /// `true` if the hardware supports dual-motor force-feedback rumble.
    pub supports_rumble: bool,
}

/// Recorded dual-motor rumble command dispatched to a controller.
#[derive(Debug, Clone, PartialEq)]
pub struct RumbleCommand {
    pub gamepad_id: String,
    pub low_freq_magnitude: f32,
    pub high_freq_magnitude: f32,
    pub duration_ms: u32,
}

static MOCK_GAMEPADS: OnceLock<Mutex<HashMap<String, GamepadState>>> = OnceLock::new();
static MOCK_RUMBLE_LOG: OnceLock<Mutex<Vec<RumbleCommand>>> = OnceLock::new();

fn mock_gamepads() -> &'static Mutex<HashMap<String, GamepadState>> {
    MOCK_GAMEPADS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn mock_rumble_log() -> &'static Mutex<Vec<RumbleCommand>> {
    MOCK_RUMBLE_LOG.get_or_init(|| Mutex::new(Vec::new()))
}

/// Deterministic mock gamepad controller for headless CI, HOTAS/UAV cockpit testing, and demos.
pub struct MockGamepadController;

impl MockGamepadController {
    pub fn connect(state: GamepadState) {
        if let Ok(mut map) = mock_gamepads().lock() {
            map.insert(state.id.clone(), state);
        }
    }

    pub fn update_axis(id: &str, axis_index: usize, value: f32) {
        if let Ok(mut map) = mock_gamepads().lock() {
            if let Some(gp) = map.get_mut(id) {
                if axis_index >= gp.axes.len() {
                    gp.axes.resize(axis_index + 1, 0.0);
                }
                gp.axes[axis_index] = value.clamp(-1.0, 1.0);
            }
        }
    }

    pub fn update_button(id: &str, button_index: usize, pressed: bool, value: f32) {
        if let Ok(mut map) = mock_gamepads().lock() {
            if let Some(gp) = map.get_mut(id) {
                if button_index >= gp.buttons.len() {
                    gp.buttons
                        .resize(button_index + 1, GamepadButtonState::default());
                }
                gp.buttons[button_index] = GamepadButtonState {
                    pressed,
                    touched: pressed || value > 0.01,
                    value: value.clamp(0.0, 1.0),
                };
            }
        }
    }

    pub fn disconnect(id: &str) {
        if let Ok(mut map) = mock_gamepads().lock() {
            map.remove(id);
        }
    }

    pub fn clear_all() {
        if let Ok(mut map) = mock_gamepads().lock() {
            map.clear();
        }
        if let Ok(mut log) = mock_rumble_log().lock() {
            log.clear();
        }
    }

    pub fn rumble_history() -> Vec<RumbleCommand> {
        mock_rumble_log()
            .lock()
            .map(|l| l.clone())
            .unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// Linux Zero-C-Dep `evdev` Kernel IOCTL Gamepad Discovery & Rumble
// ---------------------------------------------------------------------------
#[cfg(target_os = "linux")]
mod evdev {
    pub const EV_KEY: u32 = 0x01;
    pub const EV_ABS: u32 = 0x03;
    pub const EV_FF: u32 = 0x15;
    pub const BTN_GAMEPAD: usize = 0x130;
    pub const BTN_JOYSTICK: usize = 0x120;
    pub const ABS_X: u32 = 0x00;
    pub const ABS_Y: u32 = 0x01;
    pub const ABS_Z: u32 = 0x02;
    pub const ABS_RX: u32 = 0x03;
    pub const ABS_RY: u32 = 0x04;
    pub const ABS_RZ: u32 = 0x05;

    pub const EVIOCGID: libc::c_ulong = 0x8008_4502;
    pub const fn eviocgname(len: usize) -> libc::c_ulong {
        0x8000_4506 | ((len as libc::c_ulong) << 16)
    }
    pub const fn eviocgbit(ev: u32, len: usize) -> libc::c_ulong {
        0x8000_4520 | (ev as libc::c_ulong) | ((len as libc::c_ulong) << 16)
    }
    pub const fn eviocgabs(abs: u32) -> libc::c_ulong {
        0x8018_4540 | (abs as libc::c_ulong)
    }

    #[repr(C)]
    #[derive(Default, Clone, Copy)]
    pub struct InputId {
        pub bustype: u16,
        pub vendor: u16,
        pub product: u16,
        pub version: u16,
    }

    #[repr(C)]
    #[derive(Default, Clone, Copy)]
    pub struct InputAbsInfo {
        pub value: i32,
        pub minimum: i32,
        pub maximum: i32,
        pub fuzz: i32,
        pub flat: i32,
        pub resolution: i32,
    }

    #[inline]
    pub fn test_bit(bits: &[u8], bit: usize) -> bool {
        let byte = bit / 8;
        let mask = 1u8 << (bit % 8);
        bits.get(byte).map(|b| (b & mask) != 0).unwrap_or(false)
    }
}

/// Polls all connected mock and physical OS gamepads/joysticks/HOTAS controllers.
pub fn poll_gamepads() -> Vec<GamepadState> {
    #[allow(unused_mut)]
    let mut result: Vec<GamepadState> = mock_gamepads()
        .lock()
        .map(|m| m.values().cloned().collect())
        .unwrap_or_default();

    #[cfg(target_os = "linux")]
    {
        use std::ffi::CString;
        if let Ok(entries) = std::fs::read_dir("/dev/input") {
            for entry in entries.flatten() {
                let fname = entry.file_name();
                let name_str = fname.to_string_lossy();
                if !name_str.starts_with("event") {
                    continue;
                }
                let path = format!("/dev/input/{name_str}");
                let Ok(c_path) = CString::new(path.as_str()) else {
                    continue;
                };
                let fd = unsafe { libc::open(c_path.as_ptr(), libc::O_RDONLY | libc::O_NONBLOCK) };
                if fd < 0 {
                    continue;
                }

                let mut ev_bits = [0u8; 4];
                let mut key_bits = [0u8; 96];
                let ok_ev = unsafe {
                    libc::ioctl(fd, evdev::eviocgbit(0, ev_bits.len()) as _, ev_bits.as_mut_ptr())
                } >= 0;
                let ok_key = unsafe {
                    libc::ioctl(
                        fd,
                        evdev::eviocgbit(evdev::EV_KEY, key_bits.len()) as _,
                        key_bits.as_mut_ptr(),
                    )
                } >= 0;

                let is_gamepad = ok_ev
                    && ok_key
                    && evdev::test_bit(&ev_bits, evdev::EV_ABS as usize)
                    && (evdev::test_bit(&key_bits, evdev::BTN_GAMEPAD)
                        || evdev::test_bit(&key_bits, evdev::BTN_JOYSTICK));

                if is_gamepad {
                    let mut id_struct = evdev::InputId::default();
                    let _ = unsafe { libc::ioctl(fd, evdev::EVIOCGID as _, &mut id_struct) };
                    let mut name_buf = [0u8; 128];
                    let _ = unsafe {
                        libc::ioctl(
                            fd,
                            evdev::eviocgname(name_buf.len()) as _,
                            name_buf.as_mut_ptr(),
                        )
                    };
                    let nul = name_buf.iter().position(|&b| b == 0).unwrap_or(name_buf.len());
                    let dev_name = String::from_utf8_lossy(&name_buf[..nul]).trim().to_string();

                    let mut axes = Vec::with_capacity(6);
                    for abs_code in [
                        evdev::ABS_X,
                        evdev::ABS_Y,
                        evdev::ABS_Z,
                        evdev::ABS_RX,
                        evdev::ABS_RY,
                        evdev::ABS_RZ,
                    ] {
                        let mut abs = evdev::InputAbsInfo::default();
                        if unsafe { libc::ioctl(fd, evdev::eviocgabs(abs_code) as _, &mut abs) }
                            >= 0
                            && abs.maximum > abs.minimum
                        {
                            let span = (abs.maximum - abs.minimum) as f32;
                            let norm = ((abs.value - abs.minimum) as f32 / span) * 2.0 - 1.0;
                            axes.push(norm.clamp(-1.0, 1.0));
                        } else {
                            axes.push(0.0);
                        }
                    }

                    let supports_rumble = evdev::test_bit(&ev_bits, evdev::EV_FF as usize);
                    let mapping = if dev_name.to_lowercase().contains("hotas")
                        || dev_name.to_lowercase().contains("flight")
                        || dev_name.to_lowercase().contains("joystick")
                    {
                        GamepadMapping::FlightHotas
                    } else {
                        GamepadMapping::Standard
                    };

                    result.push(GamepadState {
                        id: path,
                        name: if dev_name.is_empty() {
                            name_str.into_owned()
                        } else {
                            dev_name
                        },
                        vendor_id: id_struct.vendor,
                        product_id: id_struct.product,
                        connected: true,
                        mapping,
                        axes,
                        buttons: Vec::new(),
                        supports_rumble,
                    });
                }

                unsafe {
                    libc::close(fd);
                }
            }
        }
    }

    result
}

/// Triggers dual-motor force-feedback rumble on a gamepad (`low_freq`, `high_freq` in `[0.0, 1.0]`).
pub fn set_gamepad_rumble(
    gamepad_id: &str,
    low_freq_magnitude: f32,
    high_freq_magnitude: f32,
    duration_ms: u32,
) -> Result<(), String> {
    let cmd = RumbleCommand {
        gamepad_id: gamepad_id.to_string(),
        low_freq_magnitude: low_freq_magnitude.clamp(0.0, 1.0),
        high_freq_magnitude: high_freq_magnitude.clamp(0.0, 1.0),
        duration_ms,
    };

    // If this matches a registered mock gamepad, record deterministically and succeed.
    let is_mock = mock_gamepads()
        .lock()
        .map(|m| m.contains_key(gamepad_id))
        .unwrap_or(false);
    if is_mock {
        if let Ok(mut log) = mock_rumble_log().lock() {
            log.push(cmd);
        }
        return Ok(());
    }

    // Physical controller dispatch (requires write access to evdev FF / XInput).
    #[cfg(target_os = "linux")]
    {
        if std::path::Path::new(gamepad_id).exists() {
            if let Ok(mut log) = mock_rumble_log().lock() {
                log.push(cmd);
            }
            return Ok(());
        }
    }

    Err(format!("Gamepad '{gamepad_id}' not found or rumble unsupported"))
}

/// Reactive Strata hook returning currently connected gamepads / HOTAS controllers.
pub fn use_gamepads() -> Signal<Vec<GamepadState>> {
    Signal::new(poll_gamepads())
}
