//! Phase 9 C99 FFI verification tests for VoicePipeline DSP, Vector PDF export,
//! Notifications, and Unified Permissions.

use std::ffi::CString;
use strata_ffi::*;
use strata_platform::notifications::{enable_notification_recorder, take_recorded_notifications};

#[test]
fn test_ffi_voice_pipeline_pdf_export_notify_and_permissions() {
    unsafe {
        // 1. Real-time Voice DSP pipeline over C99 ABI
        let pipe = strata_audio_voice_pipeline_new(16_000, 1);
        assert!(!pipe.is_null());
        assert_eq!(
            strata_audio_voice_pipeline_set_ns_preset(pipe, 3),
            StrataStatus::Ok
        );

        let mic = vec![0.12f32; 256];
        let mut out = vec![0.0f32; 256];
        let mut stats = StrataVoiceStats::default();
        assert_eq!(
            strata_audio_voice_pipeline_process(
                pipe,
                mic.as_ptr(),
                std::ptr::null(),
                out.as_mut_ptr(),
                256,
                &mut stats,
            ),
            StrataStatus::Ok
        );
        assert!(stats.latency_ms < 10.0);
        assert!(out.iter().all(|s| s.is_finite()));
        strata_audio_voice_pipeline_destroy(pipe);

        // 2. Runtime Vector PDF 1.7 export over C99 ABI
        let div_tag = CString::new("div").unwrap();
        let node = strata_node_element(div_tag.as_ptr());
        let cls = CString::new("w-[400px] h-[200px] bg-slate-900 rounded-xl p-4").unwrap();
        strata_node_set_class(node, cls.as_ptr());
        let view = strata_view_from_root(node);
        let rt = strata_runtime_create_from_view(view, 400.0, 200.0, 1.0);
        assert!(!rt.is_null());

        let pdf_path = std::env::temp_dir().join("strata_ffi_phase9_export.pdf");
        let c_pdf_path = CString::new(pdf_path.to_str().unwrap()).unwrap();
        assert_eq!(
            strata_runtime_export_pdf(rt, c_pdf_path.as_ptr()),
            StrataStatus::Ok
        );
        let bytes = std::fs::read(&pdf_path).unwrap();
        assert!(bytes.starts_with(b"%PDF-1.7\n"));
        let _ = std::fs::remove_file(&pdf_path);
        strata_runtime_destroy(rt);
        strata_view_destroy(view);

        // 3. Native OS notification over C99 ABI (recorder mode)
        enable_notification_recorder(true);
        let _ = take_recorded_notifications();
        let title = CString::new("Strata C99 FFI Alert").unwrap();
        let body = CString::new("Vector PDF exported successfully").unwrap();
        let mut nid = 0u32;
        assert_eq!(
            strata_notify_send(title.as_ptr(), body.as_ptr(), 1, &mut nid),
            StrataStatus::Ok
        );
        assert!(nid > 0);
        assert_eq!(take_recorded_notifications().len(), 1);
        enable_notification_recorder(false);

        // 4. Unified permissions query over C99 ABI
        let perm = strata_permission_check(0); // Microphone
        assert!((0..=4).contains(&perm));
        assert_eq!(strata_permission_check(999), -1);
    }
}
