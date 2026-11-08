//! Integration tests for OS window vibrancy, Mica, Acrylic, and backdrop composition.

use strata_platform::vibrancy::{
    apply_vibrancy, clear_vibrancy, VibrancyConfig, VibrancyEffect, VibrancyError,
};
use strata_platform::WindowConfig;

#[test]
fn test_vibrancy_effect_properties() {
    assert!(VibrancyEffect::None.is_supported_on_current_host());

    if cfg!(windows) {
        assert!(VibrancyEffect::Mica.is_supported_on_current_host());
        assert!(VibrancyEffect::MicaAlt.is_supported_on_current_host());
        assert!(VibrancyEffect::Acrylic.is_supported_on_current_host());
        assert!(VibrancyEffect::BlurBehind.is_supported_on_current_host());
        assert!(!VibrancyEffect::UnderWindowBackground.is_supported_on_current_host());
    } else if cfg!(target_os = "macos") {
        assert!(VibrancyEffect::UnderWindowBackground.is_supported_on_current_host());
        assert!(VibrancyEffect::HudWindow.is_supported_on_current_host());
        assert!(VibrancyEffect::FullScreenUI.is_supported_on_current_host());
        assert!(!VibrancyEffect::Mica.is_supported_on_current_host());
    }
}

#[test]
fn test_vibrancy_config_builder() {
    let config = VibrancyConfig::new(VibrancyEffect::Mica).with_dark_mode(true);
    assert_eq!(config.effect, VibrancyEffect::Mica);
    assert_eq!(config.dark_mode, Some(true));
}

#[test]
fn test_window_config_with_vibrancy() {
    let config = WindowConfig::default()
        .with_vibrancy(VibrancyEffect::Acrylic);
    assert_eq!(config.vibrancy, VibrancyEffect::Acrylic);
}

#[test]
fn test_apply_vibrancy_invalid_handle_returns_error() {
    let err = apply_vibrancy(0, VibrancyConfig::default()).unwrap_err();
    assert_eq!(err, VibrancyError::InvalidWindowHandle);
}

#[test]
fn test_apply_and_clear_vibrancy_lifecycle() {
    let dummy_handle = 0x12345;
    let res = apply_vibrancy(dummy_handle, VibrancyConfig::new(VibrancyEffect::None));
    // On Linux / mock handles, None returns Ok(())
    if cfg!(target_os = "linux") {
        assert!(res.is_ok());
        assert!(clear_vibrancy(dummy_handle).is_ok());
    }
}
