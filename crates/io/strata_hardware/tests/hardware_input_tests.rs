//! Phase 6 integration tests: Gamepad/HOTAS deadzone curves, mock controller & rumble,
//! reactive `use_gamepads`, and unified `HapticsEngine`.

use std::sync::Arc;
use strata_hardware::gamepad::{
    poll_gamepads, set_gamepad_rumble, use_gamepads, DeadzoneConfig, GamepadButtonState,
    GamepadMapping, GamepadState, MockGamepadController,
};
use strata_hardware::haptics::{HapticPattern, HapticSegment, HapticsEngine, MockHapticsSink};

#[test]
fn test_gamepad_axial_and_radial_deadzone_shaping() {
    let dz = DeadzoneConfig {
        inner: 0.15,
        outer: 0.95,
        exponent: 1.0,
    };
    assert_eq!(dz.apply_axial(0.10), 0.0);
    assert_eq!(dz.apply_axial(-0.14), 0.0);
    assert!((dz.apply_axial(0.55) - 0.5).abs() < 1e-4);
    assert!((dz.apply_axial(-0.98) - (-1.0)).abs() < 1e-4);

    let (rx0, ry0) = dz.apply_radial(0.08, 0.08);
    assert_eq!((rx0, ry0), (0.0, 0.0));
    let (rx1, ry1) = dz.apply_radial(0.55, 0.0);
    assert!((rx1 - 0.5).abs() < 1e-4 && ry1.abs() < 1e-6);
}

#[test]
fn test_mock_gamepad_hotas_rumble_and_reactive_hook() {
    MockGamepadController::clear_all();
    let id = "mock:hotas-0";
    MockGamepadController::connect(GamepadState {
        id: id.into(),
        name: "Strata Flight HOTAS".into(),
        vendor_id: 0x044F,
        product_id: 0xB10A,
        connected: true,
        mapping: GamepadMapping::FlightHotas,
        axes: vec![0.0, 0.0, -1.0, 0.0],
        buttons: vec![GamepadButtonState::default(); 8],
        supports_rumble: true,
    });

    MockGamepadController::update_axis(id, 0, 0.42);
    MockGamepadController::update_button(id, 0, true, 1.0);

    let gps = poll_gamepads();
    let hotas = gps.iter().find(|g| g.id == id).expect("mock HOTAS missing");
    assert_eq!(hotas.mapping, GamepadMapping::FlightHotas);
    assert!((hotas.axes[0] - 0.42).abs() < 1e-5);
    assert!(hotas.buttons[0].pressed);

    let sig = use_gamepads();
    assert!(sig.get().iter().any(|g| g.id == id));

    set_gamepad_rumble(id, 0.8, 0.3, 250).unwrap();
    let rumbles = MockGamepadController::rumble_history();
    assert_eq!(rumbles.len(), 1);
    assert_eq!(rumbles[0].duration_ms, 250);
    assert!((rumbles[0].low_freq_magnitude - 0.8).abs() < 1e-5);

    MockGamepadController::disconnect(id);
    assert!(!poll_gamepads().iter().any(|g| g.id == id));
}

#[test]
fn test_haptics_engine_patterns_and_mock_sink() {
    let sink = Arc::new(MockHapticsSink::new());
    let mut engine = HapticsEngine::with_sink(sink.clone());

    assert!(engine.trigger(&HapticPattern::ImpactHeavy).unwrap());
    assert!(engine.trigger(&HapticPattern::NotificationSuccess).unwrap());
    assert!(engine
        .trigger(&HapticPattern::CustomWaveform(vec![HapticSegment {
            delay_ms: 0,
            duration_ms: 30,
            intensity: 0.9,
            sharpness: 0.6,
        }]))
        .unwrap());

    assert_eq!(sink.triggered_patterns().len(), 3);
    assert_eq!(HapticPattern::NotificationError.to_segments().len(), 3);

    // Muted engine skips dispatch cleanly
    engine.set_enabled(false);
    assert!(!engine.trigger(&HapticPattern::SelectionTick).unwrap());
    assert_eq!(sink.triggered_patterns().len(), 3);
}
