//! Phase 5 integration tests: YUV/NV12/I420/BGRA/RGB24 -> RGBA8 conversion accuracy,
//! MockCameraSource + use_camera reactive streaming, MockScreenCapture, AvClock lip-sync,
//! and VideoPlayer -> strata_render::FrameBuffer presentation.

use std::time::Duration;
use strata_media::camera::{list_cameras, use_camera, CameraConfig};
use strata_media::pixel_format::{
    bgra_to_rgba, i420_to_rgba, nv12_to_rgba, rgb24_to_rgba, yuyv_to_rgba,
};
use strata_media::screen_capture::{list_screen_sources, MockScreenCapture};
use strata_media::video_player::{AvClock, FrameSequenceDecoder, FrameSyncDecision, VideoPlayer};

#[test]
fn test_yuv_bt601_and_rgb_conversions_exact_color_accuracy() {
    // Pure white in BT.601 studio swing: Y=235, U=128, V=128 -> ~(255, 255, 255)
    // Pure black in BT.601 studio swing: Y=16, U=128, V=128 -> (0, 0, 0)
    let yuyv = [235u8, 128, 16, 128]; // 2x1 pixels: white, black
    let mut rgba = Vec::new();
    yuyv_to_rgba(&yuyv, 2, 1, &mut rgba);
    assert_eq!(rgba.len(), 8);
    assert!(rgba[0] >= 253 && rgba[1] >= 253 && rgba[2] >= 253 && rgba[3] == 255);
    assert!(rgba[4] <= 2 && rgba[5] <= 2 && rgba[6] <= 2 && rgba[7] == 255);

    // NV12 red-ish pixel: Y=81, U=90, V=240 -> high R, low G, low B
    let y_plane = [81u8; 4];
    let uv_plane = [90u8, 240];
    nv12_to_rgba(&y_plane, &uv_plane, 2, 2, &mut rgba);
    assert_eq!(rgba.len(), 16);
    assert!(rgba[0] > 230 && rgba[1] < 20 && rgba[2] < 20 && rgba[3] == 255);

    // I420 identical check
    i420_to_rgba(&y_plane, &[90u8], &[240u8], 2, 2, &mut rgba);
    assert!(rgba[0] > 230 && rgba[1] < 20 && rgba[2] < 20 && rgba[3] == 255);

    // BGRA -> RGBA swap
    bgra_to_rgba(&[10, 20, 30, 200], &mut rgba);
    assert_eq!(rgba, vec![30, 20, 10, 200]);

    // RGB24 -> RGBA opaque
    rgb24_to_rgba(&[11, 22, 33], &mut rgba);
    assert_eq!(rgba, vec![11, 22, 33, 255]);
}

#[test]
fn test_mock_camera_and_reactive_use_camera_hook() {
    // Hardware enumeration must never panic even in headless containers
    let _ = list_cameras();
    let _ = list_screen_sources();

    let cam = use_camera(CameraConfig {
        width: 160,
        height: 120,
        fps: 30,
        ..Default::default()
    });
    assert!(!cam.is_active().get());
    assert_eq!(cam.frame_count().get(), 0);

    let _mock = cam.start_mock();
    assert!(cam.is_active().get());

    let f1 = cam.poll_frame().expect("expected mock frame 1");
    assert!(f1.is_valid());
    assert_eq!(f1.width, 160);
    assert_eq!(f1.height, 120);
    assert_eq!(f1.sequence, 1);
    assert_eq!(cam.frame_count().get(), 1);

    let fb = f1.to_framebuffer();
    assert_eq!(fb.width, 160);
    assert_eq!(fb.height, 120);
    assert_eq!(fb.pixels.len(), 160 * 120 * 4);

    let f2 = cam.poll_frame().expect("expected mock frame 2");
    assert_eq!(f2.sequence, 2);
    assert_ne!(f1.rgba, f2.rgba, "Animated SMPTE scanline should advance");

    cam.stop();
    assert!(!cam.is_active().get());
}

#[test]
fn test_mock_screen_capture_and_av_clock_video_player_sync() {
    let screen = MockScreenCapture::new(64, 48, 30);
    let mut frames = Vec::new();
    for i in 0..5u64 {
        let mut f = screen.capture_frame();
        f.pts_us = i * 33_333; // 30 fps
        frames.push(f);
    }

    // AvClock evaluation unit assertions
    let mut clock = AvClock::new();
    clock.seek(Duration::from_millis(100));
    assert_eq!(clock.evaluate_frame(100_000), FrameSyncDecision::PresentNow);
    assert_eq!(clock.evaluate_frame(150_000), FrameSyncDecision::WaitUs(50_000));
    assert_eq!(clock.evaluate_frame(40_000), FrameSyncDecision::DropLateFrame);

    // VideoPlayer playback & late-frame drop verification
    let mut player = VideoPlayer::new();
    player.load_decoder(Box::new(FrameSequenceDecoder::new(frames)));
    player.play();

    // At t=0ms -> presents frame 0
    let p0 = player.tick(Duration::from_millis(0)).expect("frame 0");
    assert_eq!(p0.sequence, 1);

    // Jump forward by 100ms -> drops late intermediate frame(s) and catches up cleanly to frame 3 (~100ms)
    let p3 = player.tick(Duration::from_millis(100)).expect("caught-up frame");
    assert!(p3.pts_us >= 66_666);
    assert!(player.dropped_frames() >= 1);
    assert!(player.current_framebuffer().is_some());
}
