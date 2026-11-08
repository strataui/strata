//! Integration tests for Strata Native Mobile Device Simulator.

use strata_sim::prelude::*;

#[test]
fn test_all_device_presets_construction_and_fillets() {
    let presets = DeviceProfile::all_presets();
    assert_eq!(presets.len(), 6);

    for profile in &presets {
        let (chassis_w, chassis_h) = GeometrySolver::calculate_chassis_dimensions(profile);
        let screen = profile.form_factor.active_screen();

        assert!(chassis_w > screen.logical_width);
        assert!(chassis_h > screen.logical_height);

        // Verify mathematical concentric fillet
        let derived_inner_radius = GeometrySolver::derive_concentric_inner_radius(
            profile.chassis.outer_corner_radius,
            profile.chassis.bezel_insets.left,
        );
        assert!(derived_inner_radius >= 0.0);

        // Verify safe area insets
        let safe_area = GeometrySolver::calculate_safe_area(profile, DeviceOrientation::Portrait);
        assert!(safe_area.top > 0.0);
        assert!(safe_area.bottom > 0.0);
    }
}

#[test]
fn test_foldable_hinge_mechanics_and_posture_transitions() {
    let flip_profile = DeviceProfile::galaxy_z_flip_6();
    let screen = flip_profile.form_factor.active_screen();

    let mut state = FoldableState::new(
        HingeAxis::Horizontal,
        180.0,
        screen.logical_width,
        screen.logical_height,
    );

    // Initial state: 180° Flat
    assert_eq!(state.posture, DevicePosture::Flat);
    assert_eq!(state.viewport_segments.len(), 1);

    // Half-folded: 90° Tabletop / Flex mode
    state.set_hinge_angle(90.0, false, screen.logical_width, screen.logical_height);
    assert_eq!(state.posture, DevicePosture::TabletopFlex);
    assert_eq!(state.viewport_segments.len(), 2);

    let seg0 = state.viewport_segments[0];
    let seg1 = state.viewport_segments[1];
    assert_eq!(seg0.x, 0.0);
    assert_eq!(seg0.y, 0.0);
    assert_eq!(seg1.x, 0.0);
    assert!(seg1.y > seg0.height); // Bottom pane starts after top pane + crease

    // W3C Viewport Segments API verification
    assert_eq!(state.env_segment_top(0, 0), 0.0);
    assert_eq!(state.env_segment_top(0, 1), seg1.y);

    // Closed: 0°
    state.set_hinge_angle(0.0, false, screen.logical_width, screen.logical_height);
    assert_eq!(state.posture, DevicePosture::Closed);
}

#[test]
fn test_dynamic_island_physical_spring_damper_oscillator() {
    let mut island = DynamicIslandOscillator::new(125.0, 37.0);
    assert_eq!(island.current_width, 125.0);
    assert_eq!(island.current_height, 37.0);

    // Transition to expanded transient alert
    island.transition_to(DynamicIslandState::ExpandedAlert {
        alert_kind: DynamicAlertKind::FaceIdUnlock,
    });
    assert_eq!(island.target_width, 220.0);
    assert_eq!(island.target_height, 46.0);

    // Simulate 60Hz physics frames (16.6ms per frame)
    for _ in 0..120 {
        island.step(0.016);
    }

    // Must converge to target dimensions without ringing
    assert!((island.current_width - 220.0).abs() < 0.1);
    assert!((island.current_height - 46.0).abs() < 0.1);
}

#[test]
fn test_virtual_hal_bus_network_battery_and_sensors() {
    let mut hal = VirtualHalBus::new();
    assert!(hal.is_online());
    assert_eq!(hal.effective_refresh_rate(120.0), 120.0);

    // Toggle airplane mode
    hal.set_airplane_mode(true);
    assert!(!hal.is_online());
    hal.set_airplane_mode(false);
    assert!(hal.is_online());

    // Battery saver mode caps 120Hz display refresh to 60Hz
    hal.set_battery(0.12, false);
    hal.set_battery_saver(true);
    assert_eq!(hal.effective_refresh_rate(120.0), 60.0);

    // Network impairment
    hal.inject_network_impairment(150, 0.05);
    assert_eq!(hal.network_latency_ms, 150);
    assert_eq!(hal.network_packet_loss, 0.05);

    // 3D Tilt
    hal.set_tilt(12.0, -8.0);
    assert_eq!(hal.sensors.pitch_deg, 12.0);
    assert_eq!(hal.sensors.yaw_deg, -8.0);
}

#[test]
fn test_system_ui_and_status_bar() {
    let status_bar = StatusBarModel::new();
    assert_eq!(status_bar.formatted_time(), "9:41");
    assert_eq!(status_bar.carrier_label, "Strata Mobile");

    let mut control_center = ControlCenterModel::default();
    assert!(!control_center.is_open);
    control_center.is_open = true;
    control_center.wifi_enabled = false;
    assert!(!control_center.wifi_enabled);
}

#[test]
fn test_navigation_subsystems_three_button_and_gesture() {
    // 1. Three-Button Navigation
    let mut classic_nav = ThreeButtonNavBar::new(ButtonLayout::AndroidClassic);
    assert_eq!(
        classic_nav.button_order(),
        [NavAction::Back, NavAction::Home, NavAction::Recents]
    );

    let samsung_nav = ThreeButtonNavBar::new(ButtonLayout::SamsungFlipped);
    assert_eq!(
        samsung_nav.button_order(),
        [NavAction::Recents, NavAction::Home, NavAction::Back]
    );

    // Dynamic luminance contrast
    classic_nav.update_luminance_contrast(0.85); // Light background -> Dark buttons
    assert!(classic_nav.is_dark_content);
    classic_nav.update_luminance_contrast(0.15); // Dark background -> Light buttons
    assert!(!classic_nav.is_dark_content);

    // 2. Gesture Navigation
    let mut gesture_nav = GestureNavEngine::new();
    assert_eq!(gesture_nav.on_edge_swipe_back(), NavAction::Back);
    assert_eq!(gesture_nav.on_home_flick(), NavAction::Home);
    assert_eq!(gesture_nav.on_recents_hold(), NavAction::Recents);
}

#[test]
fn test_multi_app_sandbox_lifecycle_and_recents() {
    let mut sandbox = MultiAppSandbox::new();
    let mut recents = RecentsCarouselState::new();

    // Initial state: No active foreground app
    assert!(sandbox.active_app().is_none());

    // Launch SaaS app
    let ok = sandbox.launch_app("com.strata.saas");
    assert!(ok);
    assert_eq!(sandbox.active_app().unwrap().metadata.bundle_id, "com.strata.saas");
    recents.push_task(sandbox.active_app().unwrap().metadata.clone());

    // Switch to Storefront app
    let ok2 = sandbox.launch_app("com.strata.storefront");
    assert!(ok2);
    assert_eq!(sandbox.active_app().unwrap().metadata.bundle_id, "com.strata.storefront");
    recents.push_task(sandbox.active_app().unwrap().metadata.clone());

    // Previous app is suspended in background
    assert_eq!(
        sandbox.instances.get("com.strata.saas").unwrap().state,
        AppLifecycleState::BackgroundSuspended
    );

    // Zero-state-loss display handover during fold/unfold
    sandbox.execute_display_handover();

    // Recents multitasking management
    assert_eq!(recents.tasks.len(), 2);
    let dismissed = recents.dismiss_task(0);
    assert_eq!(dismissed.unwrap().bundle_id, "com.strata.storefront");
    assert_eq!(recents.tasks.len(), 1);
}

#[test]
fn test_workspace_simulator_view_rendering() {
    let workspace = MobileSimulatorWorkspace::new(DeviceProfile::iphone_16_pro_max());
    let view = workspace.render_view();

    // Verify RSX blueprint lowers and renders cleanly through Strata Runtime
    let runtime = strata::launch_headless(move || view, 1440.0, 900.0);
    assert!(runtime.display_list.commands.len() > 10);
}

#[test]
fn test_foldable_to_media_context_integration() {
    use strata_style::DevicePostureQuery;

    // 1. Galaxy Z Fold 6 starts Flat (180 deg)
    let mut ws_fold = MobileSimulatorWorkspace::new(DeviceProfile::galaxy_z_fold_6());
    let ctx_flat = ws_fold.to_media_context();
    assert_eq!(ctx_flat.horizontal_viewport_segments, 1);
    assert_eq!(ctx_flat.vertical_viewport_segments, 1);
    assert_eq!(ctx_flat.device_posture, DevicePostureQuery::Continuous);

    // 2. Fold to 90 deg -> BookMode: 2 horizontal segments, 1 vertical, Folded posture
    ws_fold.set_hinge_angle(90.0);
    let ctx_book = ws_fold.to_media_context();
    assert_eq!(ctx_book.horizontal_viewport_segments, 2);
    assert_eq!(ctx_book.vertical_viewport_segments, 1);
    assert_eq!(ctx_book.device_posture, DevicePostureQuery::Folded);

    // 3. Galaxy Z Flip 6 at 90 deg -> TabletopFlex: 1 horizontal segment, 2 vertical, Folded posture
    let mut ws_flip = MobileSimulatorWorkspace::new(DeviceProfile::galaxy_z_flip_6());
    ws_flip.set_hinge_angle(90.0);
    let ctx_tabletop = ws_flip.to_media_context();
    assert_eq!(ctx_tabletop.horizontal_viewport_segments, 1);
    assert_eq!(ctx_tabletop.vertical_viewport_segments, 2);
    assert_eq!(ctx_tabletop.device_posture, DevicePostureQuery::Folded);

    // 4. End-to-end CSS media query evaluation against the simulator workspace context
    let css = r#"
        @media (horizontal-viewport-segments: 2) {
            .dual-pane { display: flex; }
        }
        @media (horizontal-viewport-segments: 1) {
            .dual-pane { display: block; }
        }
    "#;
    let sheet_book = strata_style::StyleSheet::parse_with_context(css, &ctx_book).expect("Parse book");
    assert!(sheet_book.rules.iter().any(|r| r.declarations.iter().any(|d| d.value.contains("flex"))));
    assert!(!sheet_book.rules.iter().any(|r| r.declarations.iter().any(|d| d.value.contains("block"))));
}

#[test]
fn test_concentric_fillet_sdf_g2_continuity_and_clipping() {
    let profile = DeviceProfile::iphone_16_pro_max();
    let sdf = GeometrySolver::build_concentric_fillet_sdf(&profile);

    // 1. Verify concentric radius formula: R_in = R_out - B
    assert_eq!(
        sdf.inner_radius,
        GeometrySolver::derive_concentric_inner_radius(sdf.outer_radius, sdf.bezel_thickness)
    );
    assert!(sdf.inner_radius > 0.0);

    // 2. Center point is deeply inside both screen and chassis
    assert!(sdf.eval_inner(0.0, 0.0) < -100.0);
    assert!(sdf.eval_outer(0.0, 0.0) < -100.0);
    assert_eq!(sdf.screen_coverage(0.0, 0.0, 1.0), 1.0);

    // 3. Point strictly on the bezel (outside screen, inside chassis)
    // Screen boundary is at x = inner_width / 2. Point at inner_width / 2 + 0.5 * bezel:
    let bezel_x = sdf.inner_width * 0.5 + sdf.bezel_thickness * 0.5;
    let bezel_y = 0.0;
    assert!(sdf.eval_inner(bezel_x, bezel_y) > 0.0); // Outside screen
    assert!(sdf.eval_outer(bezel_x, bezel_y) < 0.0); // Inside outer chassis
    assert!(sdf.eval_bezel(bezel_x, bezel_y) <= 0.0); // Inside bezel region
    assert_eq!(sdf.screen_coverage(bezel_x, bezel_y, 1.0), 0.0);

    // 4. Point completely outside chassis
    let outside_x = sdf.outer_width * 0.5 + 20.0;
    let outside_y = sdf.outer_height * 0.5 + 20.0;
    assert!(sdf.eval_outer(outside_x, outside_y) > 0.0);
    assert!(sdf.eval_inner(outside_x, outside_y) > 0.0);
    assert_eq!(sdf.screen_coverage(outside_x, outside_y, 1.0), 0.0);

    // 5. Mathematical G^2 curvature continuity verification:
    // Curvature at boundary tangency parameter t = 0 matches straight edge (kappa = 0)
    assert!(sdf.verify_g2_continuity());

    // 6. WGSL shader code validation
    let shader = ConcentricFilletSdf::wgsl_shader();
    assert!(shader.contains("concentric_fillet_clip"));
    assert!(shader.contains("sdf_squircle"));
}

#[test]
fn test_virtual_hal_to_strata_device_reactive_routing() {
    let mut hal = VirtualHalBus::new();
    hal.sync_to_device_runtime();

    let battery = strata_device::use_battery();
    let thermal = strata_device::use_thermal();
    let network = strata_device::use_network();

    // 1. Initial HAL state routed into signals
    assert!((battery.get().level - 0.88).abs() < 1e-4);
    assert_eq!(thermal.get(), strata_device::ThermalState::Nominal);
    assert!(network.get().is_online);

    // 2. Battery mutation via Virtual HAL
    hal.set_battery(0.25, true);
    assert!((battery.get().level - 0.25).abs() < 1e-4);
    assert_eq!(battery.get().state, strata_device::BatteryState::Charging);

    // 3. Battery saver mode
    hal.set_battery_saver(true);
    assert!(battery.get().is_low_power_mode);

    // 4. Thermal throttling injection
    hal.set_thermal_state(ThermalState::Serious);
    assert_eq!(thermal.get(), strata_device::ThermalState::Serious);

    hal.set_thermal_state(ThermalState::Critical);
    assert_eq!(thermal.get(), strata_device::ThermalState::Critical);

    // 5. Radio mode mutation (Cellular 5G metered)
    hal.set_radio_mode(NetworkRadioMode::Cellular5G);
    assert_eq!(network.get().connection_type, strata_device::ConnectionType::Cellular);
    assert_eq!(network.get().cellular_generation, Some(strata_device::CellularGen::FiveG));
    assert!(network.get().is_metered);

    // 6. Airplane mode disconnect
    hal.set_airplane_mode(true);
    assert!(!network.get().is_online);
    assert_eq!(network.get().connection_type, strata_device::ConnectionType::None);
}

#[test]
fn test_bytecode_sandbox_engine_isolation_and_quotas() {
    let mut engine = BytecodeSandboxEngine::new(1000, 64 * 1024);

    // 1. Basic arithmetic: 10 + 32 = 42
    let prog1 = [
        GuestInstruction::Push(10),
        GuestInstruction::Push(32),
        GuestInstruction::Add,
        GuestInstruction::Halt,
    ];
    let res1 = engine.execute(&prog1).expect("Execute prog1");
    assert_eq!(res1, Some(42));

    // 2. Memory store and load
    let prog2 = [
        GuestInstruction::Push(1337),
        GuestInstruction::Store(4),
        GuestInstruction::Load(4),
        GuestInstruction::Halt,
    ];
    let res2 = engine.execute(&prog2).expect("Execute prog2");
    assert_eq!(res2, Some(1337));

    // 3. Division by zero protection
    let prog_div0 = [
        GuestInstruction::Push(100),
        GuestInstruction::Push(0),
        GuestInstruction::Div,
        GuestInstruction::Halt,
    ];
    let err_div0 = engine.execute(&prog_div0).unwrap_err();
    assert_eq!(err_div0, SandboxExecutionError::DivisionByZero);

    // 4. Execution cycle quota exhaustion
    let mut tight_engine = BytecodeSandboxEngine::new(50, 1024);
    let infinite_loop = [GuestInstruction::Jump(0)];
    let err_loop = tight_engine.execute(&infinite_loop).unwrap_err();
    assert_eq!(err_loop, SandboxExecutionError::CycleLimitExceeded);

    // 5. Stack underflow protection
    let mut underflow_engine = BytecodeSandboxEngine::default();
    let err_underflow = underflow_engine.execute(&[GuestInstruction::Pop]).unwrap_err();
    assert_eq!(err_underflow, SandboxExecutionError::StackUnderflow);
}




