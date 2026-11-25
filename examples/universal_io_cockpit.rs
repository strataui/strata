//! Showcase Example: Universal I/O Cockpit — Camera, Gamepad/HOTAS, Serial/UART, BLE GATT,
//! Sensors, Native Notifications, Haptics, and 1-Click Vector PDF Export.

use strata::hardware::{
    DeadzoneConfig, HapticPattern, HapticsEngine, MockBleAdapter, MockSerialEndpoint,
    SerialConfig, UUID_HEART_RATE_MEASUREMENT,
};
use strata::media::{use_camera, CameraConfig};
use strata::platform::{
    enable_notification_recorder, take_recorded_notifications, NotificationBuilder,
    NotificationUrgency,
};
use strata::prelude::*;

fn main() {
    strata::register_universal_io_bridges();

    println!("=== Strata Universal I/O Cockpit ===");

    // 1. Live Camera capture preview (SMPTE mock fallback when headless)
    let cam = use_camera(CameraConfig {
        width: 320,
        height: 240,
        fps: 30,
        ..Default::default()
    });
    let _mock_cam = cam.start_mock();
    let frame = cam.poll_frame().expect("camera frame");
    println!(
        "Camera frame #{}: {}x{} ({} RGBA bytes)",
        frame.sequence,
        frame.width,
        frame.height,
        frame.rgba.len()
    );

    // 2. Gamepad / HOTAS deadzone shaping & Haptics
    let dz = DeadzoneConfig::default();
    let (pitch, roll) = dz.apply_radial(0.45, -0.32);
    println!("HOTAS shaped stick -> pitch={pitch:.3}, roll={roll:.3}");
    let haptics = HapticsEngine::new();
    let _ = haptics.trigger(&HapticPattern::SelectionTick);

    // 3. Serial/UART MAVLink telemetry loopback
    let (gcs, uav) = MockSerialEndpoint::pair(SerialConfig {
        baud_rate: 921_600,
        ..Default::default()
    });
    gcs.write(b"HEARTBEAT_TX");
    let mut rx = [0u8; 32];
    let n = uav.read(&mut rx);
    println!("UART loopback received: {}", String::from_utf8_lossy(&rx[..n]));

    // 4. BLE GATT Heart Rate Monitor bridged cleanly into `strata::device::use_heart_rate()`
    let ble = MockBleAdapter::with_demo_heart_rate_monitor();
    let hrm = &ble.scan(None)[0].id;
    ble.connect(hrm).unwrap();
    ble.emit_notification(hrm, UUID_HEART_RATE_MEASUREMENT, vec![0x06, 76]);
    let hr = use_heart_rate().get();
    println!("BLE -> strata-device Heart Rate Bridge: {} BPM", hr.bpm);

    // 5. Permissions & Native Notification
    println!(
        "Camera permission status: {:?}",
        check_permission(PermissionKind::Camera)
    );
    enable_notification_recorder(true);
    let _ = NotificationBuilder::new("Cockpit Ready", "All universal I/O buses online")
        .urgency(NotificationUrgency::Normal)
        .send();
    println!(
        "Notifications dispatched: {}",
        take_recorded_notifications().len()
    );

    // 6. Declarative `<video>` UI -> Pure-Rust Vector PDF 1.7 Export
    let view = rsx! {
        <div class="w-[800px] h-[480px] bg-slate-950 p-6 flex flex-col gap-4">
            <h1 class="text-2xl font-bold text-white">"Strata Universal I/O Cockpit"</h1>
            <video id="primary-cam" camera="default" autoplay="true" muted="true" />
        </div>
    };
    let mut rt = AppRuntime::new(view, 800.0, 480.0, 1.0);
    let pdf = rt.export_pdf_bytes(&strata::render::PdfExportOptions::default());
    println!("Exported Vector PDF 1.7 report: {} bytes", pdf.len());
}
