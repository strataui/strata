//! Phase 6 integration tests: End-to-end Stylus/Pen tablet pressure, tilt, twist & hover
//! dispatch through `AppRuntime` into W3C `strata_dom::Event`, plus OS Biometrics mock verification.

use std::cell::RefCell;
use std::rc::Rc;
use strata::platform::{
    authenticate_biometric, probe_biometric_capability, set_mock_biometric_result,
    BiometricAuthResult, BiometricKind, PlatformEvent, PointerKind, StylusPhase,
};
use strata::prelude::*;

#[test]
fn test_stylus_pressure_tilt_twist_and_hover_dispatch_to_dom() {
    let captured = Rc::new(RefCell::new(Vec::<(&'static str, f32, f32, f32, f32)>::new()));
    let cap_down = captured.clone();
    let cap_move = captured.clone();

    let view = rsx! {
        <div
            class="w-[300px] h-[200px]"
            onpointerdown={move |ev: Event| {
                cap_down.borrow_mut().push((
                    ev.pointer_type,
                    ev.pressure,
                    ev.tilt_x,
                    ev.tilt_y,
                    ev.twist,
                ));
            }}
            onpointermove={move |ev: Event| {
                cap_move.borrow_mut().push((
                    ev.pointer_type,
                    ev.pressure,
                    ev.tilt_x,
                    ev.tilt_y,
                    ev.twist,
                ));
            }}
        />
    };

    let mut rt = AppRuntime::new(view, 400.0, 300.0, 1.0);

    // 1. Stylus pen down with pressure 0.72, tilt (15°, -22°), barrel twist 90°
    rt.step_event(&PlatformEvent::Stylus {
        phase: StylusPhase::Down,
        pointer_id: 7,
        tool: PointerKind::Pen,
        x: 100.0,
        y: 80.0,
        pressure: 0.72,
        tilt_x: 15.0,
        tilt_y: -22.0,
        twist: 90.0,
        tangential_pressure: 0.1,
        hovering: false,
        buttons: 1,
    });

    // 2. Stylus hover move -> pressure forced to 0.0 while retaining tilt
    rt.step_event(&PlatformEvent::Stylus {
        phase: StylusPhase::Hover,
        pointer_id: 7,
        tool: PointerKind::Eraser,
        x: 120.0,
        y: 90.0,
        pressure: 0.5,
        tilt_x: -8.0,
        tilt_y: 12.0,
        twist: 180.0,
        tangential_pressure: 0.0,
        hovering: true,
        buttons: 0,
    });

    let log = captured.borrow();
    assert_eq!(log.len(), 2);
    assert_eq!(log[0].0, "pen");
    assert!((log[0].1 - 0.72).abs() < 1e-4);
    assert!((log[0].2 - 15.0).abs() < 1e-4);
    assert!((log[0].3 - (-22.0)).abs() < 1e-4);
    assert!((log[0].4 - 90.0).abs() < 1e-4);

    assert_eq!(log[1].0, "pen");
    assert_eq!(log[1].1, 0.0); // Hovering -> zero tip pressure
    assert!((log[1].2 - (-8.0)).abs() < 1e-4);
}

#[test]
fn test_biometrics_capability_and_mock_auth_lifecycle() {
    set_mock_biometric_result(Some(BiometricAuthResult::Authenticated));
    let cap = probe_biometric_capability();
    assert!(cap.available);
    assert_eq!(cap.kind, BiometricKind::Fingerprint);
    assert_eq!(
        authenticate_biometric("Authorize hardware unlock"),
        BiometricAuthResult::Authenticated
    );

    set_mock_biometric_result(Some(BiometricAuthResult::UserCancelled));
    assert_eq!(
        authenticate_biometric("Authorize hardware unlock"),
        BiometricAuthResult::UserCancelled
    );

    set_mock_biometric_result(None);
}
