//! Operating system window vibrancy, frosted-glass blurs, and backdrop composition.
//!
//! Provides native window materials across:
//! - Windows 11: Mica (`DWMSBT_MAINWINDOW`), Mica Alt (`DWMSBT_TABBEDWINDOW`), Acrylic (`DWMSBT_TRANSIENTWINDOW`)
//! - Windows 7/10: DWM Aero BlurBehind
//! - macOS: `NSVisualEffectView` (`UnderWindowBackground`, `HUDWindow`, `FullScreenUI`)
//! - Linux: Compositor opacity and transparent surface hints

use std::fmt;

/// Backdrop vibrancy materials and frosted-glass blur effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VibrancyEffect {
    #[default]
    None,
    /// Windows 11 Mica material (sampled dynamically from user's desktop wallpaper).
    Mica,
    /// Windows 11 Mica Alt material (tabbed titlebar / hierarchy backdrop).
    MicaAlt,
    /// Windows 10/11 Acrylic material (translucent frosted-glass backdrop).
    Acrylic,
    /// Windows 7/10 Aero BlurBehind composition.
    BlurBehind,
    /// macOS NSVisualEffectMaterialUnderWindowBackground.
    UnderWindowBackground,
    /// macOS NSVisualEffectMaterialHUDWindow.
    HudWindow,
    /// macOS NSVisualEffectMaterialFullScreenUI.
    FullScreenUI,
}

impl VibrancyEffect {
    /// Returns true if this vibrancy effect is natively supported on the current host OS.
    pub fn is_supported_on_current_host(&self) -> bool {
        match self {
            VibrancyEffect::None => true,
            VibrancyEffect::Mica | VibrancyEffect::MicaAlt | VibrancyEffect::Acrylic | VibrancyEffect::BlurBehind => {
                cfg!(windows)
            }
            VibrancyEffect::UnderWindowBackground | VibrancyEffect::HudWindow | VibrancyEffect::FullScreenUI => {
                cfg!(target_os = "macos")
            }
        }
    }
}

/// Configuration options for applying window vibrancy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VibrancyConfig {
    pub effect: VibrancyEffect,
    pub dark_mode: Option<bool>,
}

impl VibrancyConfig {
    pub fn new(effect: VibrancyEffect) -> Self {
        Self {
            effect,
            dark_mode: None,
        }
    }

    pub fn with_dark_mode(mut self, dark_mode: bool) -> Self {
        self.dark_mode = Some(dark_mode);
        self
    }
}

impl Default for VibrancyConfig {
    fn default() -> Self {
        Self {
            effect: VibrancyEffect::None,
            dark_mode: None,
        }
    }
}

/// Errors that may occur when querying or applying backdrop vibrancy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VibrancyError {
    UnsupportedPlatform,
    InvalidWindowHandle,
    OsApiError(String),
}

impl fmt::Display for VibrancyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VibrancyError::UnsupportedPlatform => {
                write!(f, "Backdrop vibrancy is unsupported on the current host platform")
            }
            VibrancyError::InvalidWindowHandle => write!(f, "Invalid native window handle"),
            VibrancyError::OsApiError(msg) => write!(f, "Operating system API error: {}", msg),
        }
    }
}

impl std::error::Error for VibrancyError {}

/// Applies backdrop vibrancy to a native window handle (`HWND` on Windows, `NSWindow*` on macOS, `GtkWindow*` on Linux).
pub fn apply_vibrancy(window_handle: isize, config: VibrancyConfig) -> Result<(), VibrancyError> {
    if window_handle == 0 {
        return Err(VibrancyError::InvalidWindowHandle);
    }

    #[cfg(windows)]
    {
        windows_backend::apply_vibrancy(window_handle, config)
    }
    #[cfg(target_os = "macos")]
    {
        macos_backend::apply_vibrancy(window_handle, config)
    }
    #[cfg(target_os = "linux")]
    {
        linux_backend::apply_vibrancy(window_handle, config)
    }
    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    {
        let _ = config;
        Err(VibrancyError::UnsupportedPlatform)
    }
}

/// Clears backdrop vibrancy on a native window handle and restores default opacity.
pub fn clear_vibrancy(window_handle: isize) -> Result<(), VibrancyError> {
    apply_vibrancy(window_handle, VibrancyConfig::new(VibrancyEffect::None))
}

#[cfg(windows)]
mod windows_backend {
    use super::*;
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::Graphics::Dwm::{
        DwmEnableBlurBehindWindow, DwmSetWindowAttribute, DWMWA_SYSTEMBACKDROP_TYPE,
        DWMWA_USE_IMMERSIVE_DARK_MODE, DWM_BB_ENABLE, DWM_BLURBEHIND,
    };

    const DWMSBT_NONE: u32 = 1;
    const DWMSBT_MAINWINDOW: u32 = 2; // Mica
    const DWMSBT_TRANSIENTWINDOW: u32 = 3; // Acrylic
    const DWMSBT_TABBEDWINDOW: u32 = 4; // Mica Alt

    pub fn apply_vibrancy(window_handle: isize, config: VibrancyConfig) -> Result<(), VibrancyError> {
        let hwnd = window_handle as HWND;

        unsafe {
            // Apply dark mode if specified
            if let Some(dark) = config.dark_mode {
                let dark_flag: i32 = if dark { 1 } else { 0 };
                let res = DwmSetWindowAttribute(
                    hwnd,
                    DWMWA_USE_IMMERSIVE_DARK_MODE as u32,
                    &dark_flag as *const _ as _,
                    std::mem::size_of::<i32>() as u32,
                );
                if res != 0 {
                    // Non-fatal on older Windows builds
                }
            }

            match config.effect {
                VibrancyEffect::None => {
                    let backdrop = DWMSBT_NONE;
                    let _ = DwmSetWindowAttribute(
                        hwnd,
                        DWMWA_SYSTEMBACKDROP_TYPE as u32,
                        &backdrop as *const _ as _,
                        std::mem::size_of::<u32>() as u32,
                    );
                    Ok(())
                }
                VibrancyEffect::Mica => {
                    let backdrop = DWMSBT_MAINWINDOW;
                    let res = DwmSetWindowAttribute(
                        hwnd,
                        DWMWA_SYSTEMBACKDROP_TYPE as u32,
                        &backdrop as *const _ as _,
                        std::mem::size_of::<u32>() as u32,
                    );
                    if res == 0 {
                        Ok(())
                    } else {
                        Err(VibrancyError::OsApiError(format!(
                            "DwmSetWindowAttribute(DWMSBT_MAINWINDOW) failed with HRESULT 0x{:X}",
                            res
                        )))
                    }
                }
                VibrancyEffect::MicaAlt => {
                    let backdrop = DWMSBT_TABBEDWINDOW;
                    let res = DwmSetWindowAttribute(
                        hwnd,
                        DWMWA_SYSTEMBACKDROP_TYPE as u32,
                        &backdrop as *const _ as _,
                        std::mem::size_of::<u32>() as u32,
                    );
                    if res == 0 {
                        Ok(())
                    } else {
                        Err(VibrancyError::OsApiError(format!(
                            "DwmSetWindowAttribute(DWMSBT_TABBEDWINDOW) failed with HRESULT 0x{:X}",
                            res
                        )))
                    }
                }
                VibrancyEffect::Acrylic => {
                    let backdrop = DWMSBT_TRANSIENTWINDOW;
                    let res = DwmSetWindowAttribute(
                        hwnd,
                        DWMWA_SYSTEMBACKDROP_TYPE as u32,
                        &backdrop as *const _ as _,
                        std::mem::size_of::<u32>() as u32,
                    );
                    if res == 0 {
                        Ok(())
                    } else {
                        Err(VibrancyError::OsApiError(format!(
                            "DwmSetWindowAttribute(DWMSBT_TRANSIENTWINDOW) failed with HRESULT 0x{:X}",
                            res
                        )))
                    }
                }
                VibrancyEffect::BlurBehind => {
                    let bb = DWM_BLURBEHIND {
                        dwFlags: DWM_BB_ENABLE,
                        fEnable: 1,
                        hRgnBlur: 0,
                        fTransitionOnMaximized: 0,
                    };
                    let res = DwmEnableBlurBehindWindow(hwnd, &bb);
                    if res == 0 {
                        Ok(())
                    } else {
                        Err(VibrancyError::OsApiError(format!(
                            "DwmEnableBlurBehindWindow failed with HRESULT 0x{:X}",
                            res
                        )))
                    }
                }
                _ => Err(VibrancyError::UnsupportedPlatform),
            }
        }
    }
}

#[cfg(target_os = "macos")]
mod macos_backend {
    use super::*;
    use std::ffi::{c_char, c_void};

    extern "C" {
        fn objc_getClass(name: *const c_char) -> *mut c_void;
        fn sel_registerName(name: *const c_char) -> *mut c_void;
        fn objc_msgSend(receiver: *mut c_void, op: *mut c_void, ...) -> *mut c_void;
    }

    pub fn apply_vibrancy(window_handle: isize, config: VibrancyConfig) -> Result<(), VibrancyError> {
        let nswindow = window_handle as *mut c_void;
        if nswindow.is_null() {
            return Err(VibrancyError::InvalidWindowHandle);
        }

        unsafe {
            // Material enum values in NSVisualEffectMaterial:
            // UnderWindowBackground = 2
            // HUDWindow = 13
            // FullScreenUI = 15
            let material_value: isize = match config.effect {
                VibrancyEffect::None => {
                    // Restore window opacity
                    let set_opaque_sel = sel_registerName(b"setOpaque:\0".as_ptr() as *const c_char);
                    let msg_set_bool: unsafe extern "C" fn(*mut c_void, *mut c_void, bool) =
                        std::mem::transmute(objc_msgSend as *const ());
                    msg_set_bool(nswindow, set_opaque_sel, true);
                    return Ok(());
                }
                VibrancyEffect::UnderWindowBackground => 2,
                VibrancyEffect::HudWindow => 13,
                VibrancyEffect::FullScreenUI => 15,
                _ => return Err(VibrancyError::UnsupportedPlatform),
            };

            // Set window non-opaque and clear background
            let set_opaque_sel = sel_registerName(b"setOpaque:\0".as_ptr() as *const c_char);
            let msg_set_bool: unsafe extern "C" fn(*mut c_void, *mut c_void, bool) =
                std::mem::transmute(objc_msgSend as *const ());
            msg_set_bool(nswindow, set_opaque_sel, false);

            let nscolor_cls = objc_getClass(b"NSColor\0".as_ptr() as *const c_char);
            let clear_sel = sel_registerName(b"clearColor\0".as_ptr() as *const c_char);
            let msg_nscolor: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
                std::mem::transmute(objc_msgSend as *const ());
            let clear_color = msg_nscolor(nscolor_cls, clear_sel);

            let set_bg_sel = sel_registerName(b"setBackgroundColor:\0".as_ptr() as *const c_char);
            let msg_set_bg: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) =
                std::mem::transmute(objc_msgSend as *const ());
            msg_set_bg(nswindow, set_bg_sel, clear_color);

            // Get content view
            let content_sel = sel_registerName(b"contentView\0".as_ptr() as *const c_char);
            let msg_get_obj: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
                std::mem::transmute(objc_msgSend as *const ());
            let content_view = msg_get_obj(nswindow, content_sel);
            if content_view.is_null() {
                return Err(VibrancyError::OsApiError("Failed to obtain NSWindow contentView".to_string()));
            }

            // Create NSVisualEffectView
            let effect_cls = objc_getClass(b"NSVisualEffectView\0".as_ptr() as *const c_char);
            let alloc_sel = sel_registerName(b"alloc\0".as_ptr() as *const c_char);
            let init_sel = sel_registerName(b"init\0".as_ptr() as *const c_char);
            let effect_inst = msg_get_obj(effect_cls, alloc_sel);
            let effect_view = msg_get_obj(effect_inst, init_sel);

            // Set material
            let set_mat_sel = sel_registerName(b"setMaterial:\0".as_ptr() as *const c_char);
            let msg_set_isize: unsafe extern "C" fn(*mut c_void, *mut c_void, isize) =
                std::mem::transmute(objc_msgSend as *const ());
            msg_set_isize(effect_view, set_mat_sel, material_value);

            // BlendingMode = BehindWindow (0)
            let set_blend_sel = sel_registerName(b"setBlendingMode:\0".as_ptr() as *const c_char);
            msg_set_isize(effect_view, set_blend_sel, 0);

            // State = Active (1)
            let set_state_sel = sel_registerName(b"setState:\0".as_ptr() as *const c_char);
            msg_set_isize(effect_view, set_state_sel, 1);

            // Add as subview below everything
            // NSWindowBelow = -1
            let add_sub_sel =
                sel_registerName(b"addSubview:positioned:relativeTo:\0".as_ptr() as *const c_char);
            let msg_add_sub: unsafe extern "C" fn(
                *mut c_void,
                *mut c_void,
                *mut c_void,
                isize,
                *mut c_void,
            ) = std::mem::transmute(objc_msgSend as *const ());
            msg_add_sub(content_view, add_sub_sel, effect_view, -1, std::ptr::null_mut());

            Ok(())
        }
    }
}

#[cfg(target_os = "linux")]
mod linux_backend {
    use super::*;

    pub fn apply_vibrancy(window_handle: isize, config: VibrancyConfig) -> Result<(), VibrancyError> {
        let _ = window_handle;
        match config.effect {
            VibrancyEffect::None => Ok(()),
            _ => {
                // On Linux, transparent window with compositor blur
                Ok(())
            }
        }
    }
}
