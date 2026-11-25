//! C99 FFI bindings for Universal I/O capabilities:
//! - Real-Time Audio Voice DSP (`VoicePipeline`: Noise Cancellation, AEC, AGC, VAD)
//! - Lossless Vector PDF 1.7 Export (`strata_runtime_export_pdf`)
//! - Native OS Notifications (`strata_notify_send`)
//! - Unified Hardware Permissions (`strata_permission_check`)

use std::ffi::CStr;
use std::os::raw::c_char;

use strata::audio::{NoiseSuppressionPreset, VoicePipeline, VoicePipelineConfig};
use strata::device::{check_permission, PermissionKind, PermissionStatus};
use strata::runtime::AppRuntime;
use strata_platform::{NotificationBuilder, NotificationUrgency};

use crate::error::{
    clear_last_error_internal, ffi_guard, ffi_guard_i32, ffi_guard_ptr, set_last_error,
    StrataStatus,
};

/// Opaque C handle wrapping a real-time zero-allocation [`VoicePipeline`].
pub struct StrataVoicePipelineHandle {
    pipeline: VoicePipeline,
}

/// Real-time telemetry returned per audio frame by `strata_audio_voice_pipeline_process`.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct StrataVoiceStats {
    pub is_speech: bool,
    pub speech_probability: f32,
    pub estimated_snr_db: f32,
    pub erle_db: f32,
    pub double_talk: bool,
    pub agc_gain_db: f32,
    pub latency_ms: f32,
}

/// Creates a real-time Voice DSP pipeline (`preset_id`: `0=Raw`, `1=Telephony`, `2=Podcast`, `3=NoisyCockpit`).
#[no_mangle]
pub extern "C" fn strata_audio_voice_pipeline_new(
    sample_rate: u32,
    preset_id: u32,
) -> *mut StrataVoicePipelineHandle {
    ffi_guard_ptr(|| {
        clear_last_error_internal();
        let cfg = match preset_id {
            0 => VoicePipelineConfig::raw_passthrough(sample_rate),
            2 => VoicePipelineConfig::podcast(sample_rate),
            3 => VoicePipelineConfig::noisy_cockpit(sample_rate),
            _ => VoicePipelineConfig::telephony(sample_rate),
        };
        Box::into_raw(Box::new(StrataVoicePipelineHandle {
            pipeline: VoicePipeline::new(cfg),
        }))
    })
}

/// Dynamically updates Noise Cancellation strength (`0=Off, 1=Low, 2=Medium, 3=High, 4=Broadcast`).
#[no_mangle]
pub unsafe extern "C" fn strata_audio_voice_pipeline_set_ns_preset(
    handle: *mut StrataVoicePipelineHandle,
    ns_preset: u32,
) -> StrataStatus {
    ffi_guard(|| {
        if handle.is_null() {
            set_last_error("strata_audio_voice_pipeline_set_ns_preset: NULL handle");
            return StrataStatus::NullPointer;
        }
        let preset = match ns_preset {
            0 => NoiseSuppressionPreset::Off,
            1 => NoiseSuppressionPreset::Low,
            2 => NoiseSuppressionPreset::Medium,
            3 => NoiseSuppressionPreset::High,
            _ => NoiseSuppressionPreset::Broadcast,
        };
        (*handle).pipeline.set_noise_suppression(preset);
        clear_last_error_internal();
        StrataStatus::Ok
    })
}

/// Processes a mono PCM float frame through the Voice DSP pipeline (zero heap allocations).
/// `far_in` may be NULL if Acoustic Echo Cancellation is not supplied a speaker reference.
#[no_mangle]
pub unsafe extern "C" fn strata_audio_voice_pipeline_process(
    handle: *mut StrataVoicePipelineHandle,
    mic_in: *const f32,
    far_in: *const f32,
    out_pcm: *mut f32,
    frame_len: usize,
    out_stats: *mut StrataVoiceStats,
) -> StrataStatus {
    ffi_guard(|| {
        if handle.is_null() || mic_in.is_null() || out_pcm.is_null() {
            set_last_error("strata_audio_voice_pipeline_process: NULL pointer argument");
            return StrataStatus::NullPointer;
        }
        let mic_slice = std::slice::from_raw_parts(mic_in, frame_len);
        let far_opt = if far_in.is_null() {
            None
        } else {
            Some(std::slice::from_raw_parts(far_in, frame_len))
        };
        let out_slice = std::slice::from_raw_parts_mut(out_pcm, frame_len);
        let stats = (*handle).pipeline.process_frame(mic_slice, far_opt, out_slice);
        if !out_stats.is_null() {
            *out_stats = StrataVoiceStats {
                is_speech: stats.vad.is_speech,
                speech_probability: stats.vad.probability,
                estimated_snr_db: stats.estimated_snr_db,
                erle_db: stats.erle_db,
                double_talk: stats.double_talk,
                agc_gain_db: stats.agc_gain_db,
                latency_ms: stats.algorithmic_latency_ms,
            };
        }
        clear_last_error_internal();
        StrataStatus::Ok
    })
}

/// Frees a [`StrataVoicePipelineHandle`]. Safe to pass NULL.
#[no_mangle]
pub unsafe extern "C" fn strata_audio_voice_pipeline_destroy(
    handle: *mut StrataVoicePipelineHandle,
) {
    if !handle.is_null() {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            drop(Box::from_raw(handle));
        }));
    }
}

/// Compiles the current `AppRuntime` UI display list directly into a Vector PDF 1.7 file on disk.
#[no_mangle]
pub unsafe extern "C" fn strata_runtime_export_pdf(
    runtime: *mut AppRuntime,
    path: *const c_char,
) -> StrataStatus {
    ffi_guard(|| {
        if runtime.is_null() || path.is_null() {
            set_last_error("strata_runtime_export_pdf: runtime or path pointer is NULL");
            return StrataStatus::NullPointer;
        }
        let path_str = match CStr::from_ptr(path).to_str() {
            Ok(s) => s,
            Err(_) => {
                set_last_error("strata_runtime_export_pdf: path is invalid UTF-8");
                return StrataStatus::InvalidUtf8;
            }
        };
        let rt = &mut *runtime;
        match rt.export_pdf(path_str, &strata_render::PdfExportOptions::default()) {
            Ok(()) => {
                clear_last_error_internal();
                StrataStatus::Ok
            }
            Err(err) => {
                set_last_error(format!("strata_runtime_export_pdf: {err}"));
                StrataStatus::RuntimeError
            }
        }
    })
}

/// Dispatches a native OS notification (`urgency`: `0=Low`, `1=Normal`, `2=Critical`).
#[no_mangle]
pub unsafe extern "C" fn strata_notify_send(
    title: *const c_char,
    body: *const c_char,
    urgency: u8,
    out_id: *mut u32,
) -> StrataStatus {
    ffi_guard(|| {
        if title.is_null() || body.is_null() {
            set_last_error("strata_notify_send: title or body pointer is NULL");
            return StrataStatus::NullPointer;
        }
        let Ok(title_str) = CStr::from_ptr(title).to_str() else {
            set_last_error("strata_notify_send: invalid UTF-8 title");
            return StrataStatus::InvalidUtf8;
        };
        let Ok(body_str) = CStr::from_ptr(body).to_str() else {
            set_last_error("strata_notify_send: invalid UTF-8 body");
            return StrataStatus::InvalidUtf8;
        };
        let u = match urgency {
            0 => NotificationUrgency::Low,
            2 => NotificationUrgency::Critical,
            _ => NotificationUrgency::Normal,
        };
        match NotificationBuilder::new(title_str, body_str)
            .urgency(u)
            .send()
        {
            Ok(id) => {
                if !out_id.is_null() {
                    *out_id = id;
                }
                clear_last_error_internal();
                StrataStatus::Ok
            }
            Err(err) => {
                set_last_error(format!("strata_notify_send: {err}"));
                StrataStatus::RuntimeError
            }
        }
    })
}

/// Checks OS permission status for `kind_id` (`0=Mic, 1=Cam, 2=Screen, 3=Geo, 4=BLE, 5=Serial, 6=HID, 7=MIDI, 8=Notify, 9=Bio, 10=Motion, 11=Body`).
/// Returns `0=Granted, 1=Denied, 2=Prompt, 3=Restricted, 4=Unsupported`, or `-1` on invalid kind.
#[no_mangle]
pub extern "C" fn strata_permission_check(kind_id: u32) -> i32 {
    ffi_guard_i32(|| {
        let kind = match kind_id {
            0 => PermissionKind::Microphone,
            1 => PermissionKind::Camera,
            2 => PermissionKind::ScreenCapture,
            3 => PermissionKind::Geolocation,
            4 => PermissionKind::Bluetooth,
            5 => PermissionKind::Serial,
            6 => PermissionKind::UsbHid,
            7 => PermissionKind::Midi,
            8 => PermissionKind::Notifications,
            9 => PermissionKind::Biometrics,
            10 => PermissionKind::MotionSensors,
            11 => PermissionKind::BodySensors,
            _ => return -1,
        };
        match check_permission(kind) {
            PermissionStatus::Granted => 0,
            PermissionStatus::Denied => 1,
            PermissionStatus::Prompt => 2,
            PermissionStatus::Restricted => 3,
            PermissionStatus::Unsupported => 4,
        }
    })
}
