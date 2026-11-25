//! Strata Media: Pure-Rust camera capture (V4L2 `mmap`/PipeWire/AVFoundation/MediaFoundation),
//! screen capture, BT.601 YUV->RGBA conversion, A/V-synchronized video playback, and reactive hooks.

pub mod camera;
pub mod pixel_format;
pub mod screen_capture;
pub mod video_player;

pub use camera::{
    list_cameras, use_camera, use_cameras, CameraConfig, CameraDeviceInfo, CameraError,
    CameraHandle, CameraStream, MockCameraSource,
};
pub use pixel_format::{
    bgra_to_rgba, i420_to_rgba, nv12_to_rgba, rgb24_to_rgba, yuv_to_rgb_bt601, yuyv_to_rgba,
    PixelFormat, VideoFrame,
};
pub use screen_capture::{
    list_screen_sources, MockScreenCapture, ScreenCaptureConfig, ScreenSourceInfo,
    ScreenSourceKind,
};
pub use video_player::{
    AvClock, FrameSequenceDecoder, FrameSyncDecision, VideoDecoder, VideoPlayer,
};

pub mod prelude {
    pub use crate::camera::{
        list_cameras, use_camera, use_cameras, CameraConfig, CameraDeviceInfo, CameraError,
        CameraHandle, CameraStream, MockCameraSource,
    };
    pub use crate::pixel_format::{PixelFormat, VideoFrame};
    pub use crate::screen_capture::{
        list_screen_sources, MockScreenCapture, ScreenCaptureConfig, ScreenSourceInfo,
        ScreenSourceKind,
    };
    pub use crate::video_player::{
        AvClock, FrameSequenceDecoder, FrameSyncDecision, VideoDecoder, VideoPlayer,
    };
}
