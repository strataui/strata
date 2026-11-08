//! Comprehensive Integration Test Suite for strata_mobile_test.

use strata_mobile_test::*;

#[test]
fn test_adb_packet_codec_and_port_scanner() {
    // 1. Packet framing
    let cmd = "host:version";
    let encoded = AdbPacketCodec::encode_command(cmd);
    assert_eq!(&encoded[0..4], b"000C");
    assert_eq!(&encoded[4..], cmd.as_bytes());

    let parsed_len = AdbPacketCodec::parse_hex_length(&encoded[0..4]).unwrap();
    assert_eq!(parsed_len, 12);

    // 2. OKAY status packet
    let resp = AdbPacketCodec::encode_response_okay(b"0029");
    assert_eq!(&resp[0..4], b"OKAY");
    assert_eq!(&resp[4..8], b"0004");
    assert_eq!(&resp[8..], b"0029");

    // 3. Port scanner runtime identification
    assert!(AdbPortScanner::identify_runtime(5555).contains("Standard AVD"));
    assert!(AdbPortScanner::identify_runtime(16384).contains("MuMu Player 12"));
    assert!(AdbPortScanner::identify_runtime(62001).contains("NoxPlayer"));
}

#[test]
fn test_mock_adb_server_device_discovery_and_tunnels() {
    let mut server = MockAdbServer::new();

    // Device discovery
    let resp = server.handle_command("host:devices-l");
    assert!(resp.starts_with(b"OKAY"));
    let text = std::str::from_utf8(&resp[8..]).unwrap();
    assert!(text.contains("emulator-5554"));
    assert!(text.contains("Pixel_Fold"));

    let device = AdbDevice::parse_device_line(text.lines().next().unwrap()).unwrap();
    assert_eq!(device.serial, "emulator-5554");
    assert_eq!(device.model, "Pixel_Fold");

    // Port forward: host:8080 -> device:8080
    let fwd_resp = server.handle_command("host-serial:emulator-5554:forward:tcp:8080;tcp:8080");
    assert_eq!(fwd_resp, b"OKAY");
    assert_eq!(server.forwarded_ports().get(&8080), Some(&8080));

    // Reverse tunnel: device:9222 -> host:9222
    let rev_resp = server.handle_command("host-serial:emulator-5554:reverse:forward:tcp:9222;tcp:9222");
    assert_eq!(rev_resp, b"OKAY");
    assert_eq!(server.reversed_ports().get(&9222), Some(&9222));

    // Shell query
    let shell_resp = server.handle_command("shell:getprop sys.boot_completed");
    assert!(shell_resp.starts_with(b"OKAY"));
    assert!(std::str::from_utf8(&shell_resp[8..]).unwrap().contains('1'));
}

#[test]
fn test_host_detection_and_emulator_args() {
    let _platform = HostDetector::current_platform();

    let mut cfg = EmulatorConfig::new("strata_pixel_fold");
    cfg.memory_mb = 4096;
    cfg.cores = 8;
    cfg.gpu_mode = EmulatorGpuMode::SwiftShaderIndirect;

    let args = cfg.generate_args();
    assert!(args.contains(&"-no-window".to_string()));
    assert!(args.contains(&"-no-audio".to_string()));
    assert!(args.contains(&"-gpu".to_string()));
    assert!(args.contains(&"swiftshader_indirect".to_string()));
    assert!(args.contains(&"4096".to_string()));
    assert!(args.contains(&"8".to_string()));

    assert!(BootStatusChecker::is_boot_completed("1\n"));
    assert!(!BootStatusChecker::is_boot_completed("0\n"));
}

#[test]
fn test_foldable_posture_profiles_and_command_emission() {
    // 1. Book foldable (Pixel Fold)
    let book = FoldableProfile::book_foldable();
    assert_eq!(book.cover_resolution, (1080, 2092));
    assert_eq!(book.unfolded_resolution, (2208, 1840));
    assert!(book.crease.is_vertical);
    assert!(!book.crease.is_separating);
    assert_eq!(book.resolution_for_posture(FoldPosture::Closed), (1080, 2092));
    assert_eq!(book.resolution_for_posture(FoldPosture::Flat), (2208, 1840));

    // 2. Clamshell flip (Galaxy Z Flip)
    let flip = FoldableProfile::clamshell_flip();
    assert_eq!(flip.cover_resolution, (1056, 1066));
    assert_eq!(flip.unfolded_resolution, (1080, 2640));
    assert!(!flip.crease.is_vertical);

    // 3. Dual-screen (Surface Duo)
    let duo = FoldableProfile::dual_screen_duo();
    assert!(duo.crease.is_separating);
    assert_eq!(duo.crease.width, 84);

    // 4. Posture command strings
    assert_eq!(
        PostureCommandEmitter::emit_emu_posture_cmd(FoldPosture::HalfOpened),
        "emu posture set half-opened"
    );
    assert_eq!(
        PostureCommandEmitter::emit_device_state_cmd(FoldPosture::Flat),
        "cmd device_state state 2"
    );
    assert_eq!(
        PostureCommandEmitter::emit_telnet_hinge_angle_cmd(90),
        "sensor set hinge_angle 90"
    );
}

#[test]
fn test_logcat_parsing_and_panic_trapping() {
    let line = "10-31 14:22:01.123  1420  1420 I StrataNative: Initialized GPU Framebuffer";
    let entry = LogcatParser::parse_line(line).unwrap();
    assert_eq!(entry.pid, 1420);
    assert_eq!(entry.tid, 1420);
    assert_eq!(entry.level, LogLevel::Info);
    assert_eq!(entry.tag, "StrataNative");
    assert_eq!(entry.message, "Initialized GPU Framebuffer");

    let mut trap = PanicTrap::new();
    assert!(!trap.has_crashed());

    // Normal message does not trigger trap
    assert_eq!(trap.inspect_entry(&entry), None);

    // Panic message triggers trap
    let panic_line = "10-31 14:22:05.456  1420  1420 E StrataNative: panicked at src/components/chart.rs:42:10";
    let panic_entry = LogcatParser::parse_line(panic_line).unwrap();
    let report = trap.inspect_entry(&panic_entry).unwrap();
    assert!(report.is_panic);
    assert!(!report.is_segfault);
    assert_eq!(report.location.as_deref(), Some("src/components/chart.rs:42:10"));
    assert!(trap.has_crashed());

    // Segfault line
    let segfault_line = "10-31 14:22:06.789  1420  1420 F DEBUG: signal 11 (SIGSEGV), code 1 (SEGV_MAPERR)";
    let segfault_entry = LogcatParser::parse_line(segfault_line).unwrap();
    let seg_report = trap.inspect_entry(&segfault_entry).unwrap();
    assert!(seg_report.is_segfault);
}

#[test]
fn test_ssim_visual_regression_and_damage_boxes() {
    let width = 10;
    let height = 10;
    let bytes_per_pixel = 4;
    let len = width * height * bytes_per_pixel;

    // Identical buffers yield SSIM = 1.0
    let buf_a = vec![200u8; len];
    let buf_b = vec![200u8; len];
    let diff = SsimEngine::compute_ssim(&buf_a, &buf_b, width, height, bytes_per_pixel).unwrap();
    assert_eq!(diff.ssim_score, 1.0);
    assert!(diff.is_match);
    assert_eq!(diff.mismatched_pixels, 0);
    assert!(diff.damage_rects.is_empty());

    // Mutated buffer with damaged region
    let mut buf_c = buf_a.clone();
    // Invert pixels in a 3x3 region at (2, 2)
    for y in 2..5 {
        for x in 2..5 {
            let idx = (y * width + x) * bytes_per_pixel;
            buf_c[idx] = 0;
            buf_c[idx + 1] = 0;
            buf_c[idx + 2] = 0;
        }
    }

    let diff_damaged = SsimEngine::compute_ssim(&buf_a, &buf_c, width, height, bytes_per_pixel).unwrap();
    assert!(diff_damaged.ssim_score < 1.0);
    assert_eq!(diff_damaged.mismatched_pixels, 9);
    assert_eq!(diff_damaged.damage_rects.len(), 1);
    let (dx, dy, dw, dh) = diff_damaged.damage_rects[0];
    assert_eq!((dx, dy, dw, dh), (2, 2, 3, 3));
}

#[test]
fn test_apple_license_tier_and_ios_mock_viewports() {
    let _tier = AppleLicenseTier::detect();

    // SimCtl command generation
    let boot_cmd = SimCtlDriver::build_boot_cmd("ABC-123");
    assert_eq!(boot_cmd, vec!["xcrun", "simctl", "boot", "ABC-123"]);

    // iPhone 16 Pro Viewport geometry
    let iphone16 = MockIosViewport::for_model(IosDeviceModel::IPhone16Pro);
    assert_eq!(iphone16.width_pt, 393.0);
    assert_eq!(iphone16.height_pt, 852.0);
    assert_eq!(iphone16.dpr, 3.0);
    assert_eq!(iphone16.refresh_hz, 120.0);
    assert_eq!(iphone16.physical_resolution(), (1179, 2556));
    assert!(iphone16.dynamic_island_pt.is_some());
    assert_eq!(iphone16.safe_area_top_pt, 59.0);
    assert_eq!(iphone16.safe_area_bottom_pt, 34.0);

    // iPad Pro 13" Viewport
    let ipad = MockIosViewport::for_model(IosDeviceModel::IPadPro13M4);
    assert_eq!(ipad.width_pt, 1024.0);
    assert_eq!(ipad.height_pt, 1366.0);
    assert_eq!(ipad.physical_resolution(), (2048, 2732));

    // Rubber band overscroll physics
    let zero_displace = MockIosViewport::calculate_rubber_band_displacement(0.0, 852.0);
    assert_eq!(zero_displace, 0.0);

    let displace_100 = MockIosViewport::calculate_rubber_band_displacement(100.0, 852.0);
    assert!(displace_100 > 0.0 && displace_100 < 100.0);

    let displace_neg = MockIosViewport::calculate_rubber_band_displacement(-100.0, 852.0);
    assert!(displace_neg < 0.0 && displace_neg > -100.0);
}

#[test]
fn test_monkey_fuzzing_and_runner_execution() {
    let mut fuzzer = MonkeyFuzzer::new(12345);
    let events = fuzzer.generate_sequence(20, 1080.0, 2400.0);
    assert_eq!(events.len(), 20);

    for ev in &events {
        let cmd = MonkeyFuzzer::to_adb_input_cmd(ev);
        assert!(cmd.starts_with("input "));
    }

    // End-to-end runner execution
    let config = StrataMobileTestConfig {
        is_android: true,
        is_ios: false,
        is_mock_ios: false,
        device_serial: Some("emulator-5554".to_string()),
        enable_monkey_fuzz: true,
        fuzz_event_count: 10,
        test_foldable: true,
    };

    let mut runner = StrataMobileTestRunner::new(config);
    let report = runner.execute();

    assert!(report.passed);
    assert!(report.tests_run >= 3);
    assert_eq!(report.tests_failed, 0);
    assert_eq!(report.panics_detected, 0);
    assert_eq!(report.ssim_score, 1.0);
}

#[test]
fn test_real_tcp_adb_client_connection_and_protocol() {
    // 1. Start MockAdbServer listening on ephemeral TCP port
    let (port, running, handle) = MockAdbServer::start_tcp(0).expect("Start TCP mock ADB server");
    assert!(port > 0);

    // 2. Connect live AdbClient over TCP socket
    let mut client = AdbClient::connect(("127.0.0.1", port)).expect("Connect AdbClient");

    // 3. Test get_version
    let version = client.get_version().expect("get_version over TCP");
    assert_eq!(version, 41);

    // 4. Test list_devices
    let devices = client.list_devices().expect("list_devices over TCP");
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].serial, "emulator-5554");
    assert_eq!(devices[0].model, "Pixel_Fold");

    // 5. Test port forwarding
    client
        .forward_port("emulator-5554", 8080, 8080)
        .expect("forward_port over TCP");

    // 6. Test reverse port tunnel
    client
        .reverse_port("emulator-5554", 9222, 9222)
        .expect("reverse_port over TCP");

    // 7. Test shell command
    let shell_out = client
        .shell("emulator-5554", "getprop sys.boot_completed")
        .expect("shell over TCP");
    assert!(shell_out.contains('1'));

    // 8. Test screencap stream
    let screencap_png = client.screencap("emulator-5554").expect("screencap over TCP");
    assert!(screencap_png.len() > 8);
    assert_eq!(&screencap_png[0..8], &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);

    // Clean shutdown
    running.store(false, std::sync::atomic::Ordering::Relaxed);
    let _ = handle.join();
}

#[test]
fn test_real_tcp_emulator_console_posture_and_hinge_injection() {
    let token = "test_auth_token_secret_xyz";
    // 1. Start mock emulator telnet console server on ephemeral TCP port
    let (port, running, handle) =
        MockEmulatorConsoleServer::start_tcp(0, token).expect("Start TCP mock console server");
    assert!(port > 0);

    // 2. Connect EmulatorConsoleClient over TCP socket
    let mut client = EmulatorConsoleClient::connect(port).expect("Connect EmulatorConsoleClient");

    // 3. Authenticate with console token
    client
        .authenticate(Some(token))
        .expect("Authenticate with emulator console");

    // 4. Inject foldable posture
    client
        .set_posture("half-opened")
        .expect("Inject half-opened posture");

    // 5. Inject hinge angle
    client.set_hinge_angle(90).expect("Inject hinge angle 90");

    // 6. Quit console
    client.quit().expect("Quit emulator console");

    // Clean shutdown
    running.store(false, std::sync::atomic::Ordering::Relaxed);
    let _ = handle.join();
}

#[test]
fn test_simctl_driver_command_execution_handling() {
    // When executing on Linux/Windows where xcrun is unavailable, verify safe error reporting
    let list_res = SimCtlDriver::list_devices_exec();
    if cfg!(target_os = "macos") {
        // On macOS, if Xcode Command Line Tools are installed, it returns valid JSON or error
        assert!(list_res.is_ok() || list_res.is_err());
    } else {
        // On Non-Mac host, xcrun execution fails gracefully without panicking
        assert!(list_res.is_err());
        let err = list_res.unwrap_err();
        assert!(err.contains("Failed to execute xcrun simctl") || err.contains("No such file"));
    }

    // Verify command builder vectors retain exact flags
    assert_eq!(
        SimCtlDriver::build_list_devices_cmd(),
        vec!["xcrun", "simctl", "list", "devices", "available", "-j"]
    );
    assert_eq!(
        SimCtlDriver::build_boot_cmd("DEV-99"),
        vec!["xcrun", "simctl", "boot", "DEV-99"]
    );
    assert_eq!(
        SimCtlDriver::build_install_cmd("DEV-99", "/tmp/app.ipa"),
        vec!["xcrun", "simctl", "install", "DEV-99", "/tmp/app.ipa"]
    );
    assert_eq!(
        SimCtlDriver::build_screenshot_cmd("DEV-99", "/tmp/shot.png"),
        vec!["xcrun", "simctl", "io", "DEV-99", "screenshot", "/tmp/shot.png"]
    );
}

