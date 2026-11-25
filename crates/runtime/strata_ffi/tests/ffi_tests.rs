use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_void};
use strata_ffi::*;

#[test]
fn test_version_string() {
    let ver_ptr = strata_version();
    assert!(!ver_ptr.is_null());
    let ver_str = unsafe { CStr::from_ptr(ver_ptr).to_str().unwrap() };
    assert_eq!(ver_str, env!("CARGO_PKG_VERSION"));
}

#[test]
fn test_app_lifecycle_and_window_creation() {
    unsafe {
        let app = strata_app_create();
        assert!(!app.is_null());
        assert_eq!(strata_app_is_running(app), 1);

        let title = CString::new("FFI Test Window").unwrap();
        let win = strata_window_create(app, title.as_ptr(), 1024, 768);
        assert!(!win.is_null());

        assert_eq!(strata_window_get_width(win), 1024);
        assert_eq!(strata_window_get_height(win), 768);

        let new_title = CString::new("Updated Title").unwrap();
        assert_eq!(strata_window_set_title(win, new_title.as_ptr()), 0);

        let mut frame_buf = vec![0u8; 1024 * 768 * 4];
        let bytes_rendered = strata_window_render_frame(win, frame_buf.as_mut_ptr());
        assert_eq!(bytes_rendered, 1024 * 768 * 4);
        assert_eq!(frame_buf[0], 255);
        assert_eq!(frame_buf[3], 255);

        strata_app_destroy(app);
    }
}

extern "C" fn test_callback(new_val: i64, user_data: *mut c_void) {
    let counter = unsafe { &mut *(user_data as *mut i64) };
    *counter = new_val;
}

#[test]
fn test_reactive_signal_read_write_subscribe() {
    unsafe {
        let sig = strata_signal_create_int(42);
        assert_eq!(strata_signal_get_int(sig), 42);

        let mut observed_val: i64 = 0;
        let user_data = &mut observed_val as *mut i64 as *mut c_void;

        let sub_id = strata_signal_subscribe_int(sig, Some(test_callback), user_data);
        assert!(sub_id > 0);

        strata_signal_set_int(sig, 100);
        assert_eq!(strata_signal_get_int(sig), 100);
        assert_eq!(observed_val, 100);

        strata_signal_set_int(sig, 250);
        assert_eq!(strata_signal_get_int(sig), 250);
        assert_eq!(observed_val, 250);

        assert_eq!(strata_signal_unsubscribe_int(sig, sub_id), 0);

        strata_signal_set_int(sig, 500);
        assert_eq!(strata_signal_get_int(sig), 500);
        // Observed value should NOT update after unsubscribe
        assert_eq!(observed_val, 250);

        strata_signal_destroy(sig);
    }
}

#[test]
fn test_tailwind_evaluation_over_c_ffi() {
    unsafe {
        let classes = CString::new("w-64 h-32 bg-blue-500").unwrap();
        let mut buffer = [0 as c_char; 256];

        let bytes_written = strata_eval_tailwind_css(classes.as_ptr(), buffer.as_mut_ptr(), 256);
        assert!(bytes_written > 0);

        let result_str = CStr::from_ptr(buffer.as_ptr()).to_str().unwrap();
        assert!(result_str.contains("width:"));
        assert!(result_str.contains("height:"));
        assert!(result_str.contains("background-color:"));
    }
}

#[test]
fn test_render_to_buffer_rgba() {
    unsafe {
        let width = 4;
        let height = 4;
        let mut buffer = vec![0u8; (width * height * 4) as usize];

        let written = strata_render_to_buffer(
            width,
            height,
            255, // R
            128, // G
            64,  // B
            255, // A
            buffer.as_mut_ptr(),
        );

        assert_eq!(written, 64);
        assert_eq!(buffer[0], 255);
        assert_eq!(buffer[1], 128);
        assert_eq!(buffer[2], 64);
        assert_eq!(buffer[3], 255);
        assert_eq!(buffer[60], 255);
        assert_eq!(buffer[61], 128);
        assert_eq!(buffer[62], 64);
        assert_eq!(buffer[63], 255);
    }
}

#[test]
fn test_window_destruction_before_app_destroy_safety() {
    unsafe {
        let app = strata_app_create();
        assert!(!app.is_null());
        let title = CString::new("Safety Test Window").unwrap();
        let win = strata_window_create(app, title.as_ptr(), 640, 480);
        assert!(!win.is_null());

        // Explicitly destroy the window handle first.
        strata_window_destroy(win);

        // Subsequent destruction of the app must not double-free the window handle.
        strata_app_destroy(app);
    }
}

extern "C" fn mt_callback(_new_val: i64, user_data: *mut c_void) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let counter = unsafe { &*(user_data as *const AtomicUsize) };
    counter.fetch_add(1, Ordering::SeqCst);
}

#[test]
fn test_multithreaded_signal_subscriptions() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::thread;

    unsafe {
        let sig = strata_signal_create_int(0);
        let counter = Arc::new(AtomicUsize::new(0));
        let user_data = Arc::as_ptr(&counter) as *mut c_void;

        let sub_id = strata_signal_subscribe_int(sig, Some(mt_callback), user_data);
        assert!(sub_id > 0);

        let sig_addr = sig as usize;
        let mut handles = Vec::new();

        // Spawn 4 writer threads mutating signal concurrently
        for t in 0..4 {
            handles.push(thread::spawn(move || {
                let sig_ptr = sig_addr as *mut StrataSignalHandle;
                for i in 0..100 {
                    strata_signal_set_int(sig_ptr, (t * 1000 + i) as i64);
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        assert!(counter.load(Ordering::SeqCst) > 0);
        assert_eq!(strata_signal_unsubscribe_int(sig, sub_id), 0);
        strata_signal_destroy(sig);
    }
}

#[test]
fn test_window_renders_complex_ui_pixels() {
    use strata_dom::{NodeBlueprint, Tag, View};

    unsafe {
        let app = strata_app_create();
        assert!(!app.is_null());

        let title = CString::new("Complex UI Window").unwrap();
        let width = 100u32;
        let height = 100u32;
        let win = strata_window_create(app, title.as_ptr(), width, height);
        assert!(!win.is_null());

        // Create a multi-element layout:
        // Outer div with bg-red (239, 68, 68), inner child with bg-blue (59, 130, 246)
        let child = NodeBlueprint::Element {
            tag: Tag::Div,
            classes: vec![
                "w-[50px]".to_string(),
                "h-[50px]".to_string(),
                "bg-blue-500".to_string(),
            ]
            .into(),
            attributes: Vec::new(),
            children: Vec::new(),
        };
        let root = NodeBlueprint::Element {
            tag: Tag::Div,
            classes: vec![
                "w-[100px]".to_string(),
                "h-[100px]".to_string(),
                "bg-red-500".to_string(),
            ]
            .into(),
            attributes: Vec::new(),
            children: vec![child],
        };
        let mut view = View::new(root);
        let runtime = strata_runtime_create(&mut view as *mut _, width as f32, height as f32, 1.0);
        assert!(!runtime.is_null());

        assert_eq!(strata_window_set_runtime(win, runtime), 0);

        let mut buffer = vec![0u8; (width * height * 4) as usize];
        let bytes_rendered = strata_window_render_frame(win, buffer.as_mut_ptr());
        assert_eq!(bytes_rendered, (width * height * 4) as i32);

        // Verify pixel rendering is authentic and non-uniform:
        // Frame must contain distinct red and blue pixels from the multi-element fixture
        let has_red = buffer.chunks(4).any(|p| p[0] > 180 && p[2] < 120);
        let has_blue = buffer.chunks(4).any(|p| p[2] > 180 && p[0] < 120);
        assert!(has_red, "Rendered frame must contain red background pixels");
        assert!(has_blue, "Rendered frame must contain blue child pixels");

        strata_runtime_destroy(runtime);
        strata_app_destroy(app);
    }
}

extern "C" fn dom_click_cb(ev: *const StrataDomEvent, user_data: *mut c_void) {
    unsafe {
        assert!(!ev.is_null());
        assert_eq!((*ev).event_kind, 1);
        let sig = user_data as *mut StrataSignalHandle;
        let next = strata_signal_get_int(sig) + 1;
        strata_signal_set_int(sig, next);
    }
}

#[test]
fn test_c_dom_builders_signals_click_checked_rgba_and_png() {
    unsafe {
        let count_sig = strata_signal_create_int(5);
        assert!(!count_sig.is_null());

        let div_tag = CString::new("div").unwrap();
        let btn_tag = CString::new("button").unwrap();
        let root_cls = CString::new("w-[120px] h-[100px] bg-red-500 flex flex-col").unwrap();
        let btn_cls = CString::new("w-[80px] h-[50px] bg-blue-500").unwrap();
        let click_ev = CString::new("click").unwrap();
        let id_k = CString::new("id").unwrap();
        let id_v = CString::new("ffi-btn").unwrap();

        let root = strata_node_element(div_tag.as_ptr());
        assert_eq!(strata_node_set_class(root, root_cls.as_ptr()), StrataStatus::Ok);

        let btn = strata_node_element(btn_tag.as_ptr());
        assert_eq!(strata_node_set_attr(btn, id_k.as_ptr(), id_v.as_ptr()), StrataStatus::Ok);
        assert_eq!(strata_node_set_class(btn, btn_cls.as_ptr()), StrataStatus::Ok);
        assert_eq!(
            strata_node_on_event(btn, click_ev.as_ptr(), Some(dom_click_cb), count_sig as *mut c_void),
            StrataStatus::Ok
        );

        let dyn_txt = strata_node_dynamic_text_signal(count_sig);
        assert!(!dyn_txt.is_null());
        assert_eq!(strata_node_append_child(btn, dyn_txt), StrataStatus::Ok);
        assert_eq!(strata_node_append_child(root, btn), StrataStatus::Ok);

        let view = strata_view_from_root(root);
        assert!(!view.is_null());

        let rt = strata_runtime_create_from_view(view, 120.0, 100.0, 1.0);
        assert!(!rt.is_null());

        let mut dirty = false;
        assert_eq!(strata_runtime_pointer_move(rt, 20.0, 20.0, &mut dirty), StrataStatus::Ok);
        assert_eq!(strata_runtime_pointer_down(rt, 20.0, 20.0, 0, &mut dirty), StrataStatus::Ok);
        assert_eq!(strata_runtime_pointer_up(rt, 20.0, 20.0, 0, &mut dirty), StrataStatus::Ok);
        assert_eq!(strata_signal_get_int(count_sig), 6);

        // Checked framebuffer info & BufferTooSmall safety check
        let mut fb_info = StrataFramebufferInfo {
            width: 0,
            height: 0,
            stride: 0,
            byte_len: 0,
        };
        assert_eq!(strata_runtime_framebuffer_info(rt, &mut fb_info), StrataStatus::Ok);
        assert_eq!(fb_info.width, 120);
        assert_eq!(fb_info.height, 100);
        assert_eq!(fb_info.byte_len, 120 * 100 * 4);

        let mut too_small = [0u8; 8];
        let mut required = 0usize;
        assert_eq!(
            strata_runtime_render_rgba(rt, too_small.as_mut_ptr(), too_small.len(), &mut required),
            StrataStatus::BufferTooSmall
        );
        assert_eq!(required, fb_info.byte_len);
        assert!(!CStr::from_ptr(strata_last_error()).to_str().unwrap().is_empty());

        let mut pixels = vec![0u8; fb_info.byte_len];
        let mut written = 0usize;
        assert_eq!(
            strata_runtime_render_rgba(rt, pixels.as_mut_ptr(), pixels.len(), &mut written),
            StrataStatus::Ok
        );
        assert_eq!(written, fb_info.byte_len);
        assert!(pixels.chunks(4).any(|p| p[2] > 180 && p[0] < 120));
        assert!(pixels.chunks(4).any(|p| p[0] > 180 && p[2] < 120));

        // PNG export verification
        let png_path = std::env::temp_dir().join(format!("strata_ffi_test_{}.png", std::process::id()));
        let png_c = CString::new(png_path.to_str().unwrap()).unwrap();
        assert_eq!(strata_runtime_export_png(rt, png_c.as_ptr()), StrataStatus::Ok);
        let png_bytes = std::fs::read(&png_path).unwrap();
        assert_eq!(&png_bytes[0..8], b"\x89PNG\r\n\x1a\n");
        let _ = std::fs::remove_file(&png_path);

        // Resize + headless native window launch
        assert_eq!(strata_runtime_resize(rt, 200.0, 140.0, 1.0), StrataStatus::Ok);
        assert_eq!(strata_runtime_framebuffer_info(rt, &mut fb_info), StrataStatus::Ok);
        assert_eq!(fb_info.width, 200);
        assert_eq!(fb_info.height, 140);

        strata_runtime_destroy(rt);

        let win_title = CString::new("FFI Headless").unwrap();
        let opts = StrataWindowOptions {
            title: win_title.as_ptr(),
            width: 160,
            height: 120,
            resizable: false,
            headless: true,
        };
        assert_eq!(strata_launch_native_window(view, &opts), StrataStatus::Ok);
        strata_signal_destroy(count_sig);
    }
}

#[test]
fn test_standards_compliant_css_serializer_no_debug_tokens() {
    unsafe {
        let classes = CString::new("flex flex-col w-64 h-32 p-4 bg-blue-500 rounded-md").unwrap();
        let mut needed = 0usize;
        assert_eq!(
            strata_eval_tailwind_css_checked(classes.as_ptr(), std::ptr::null_mut(), 0, &mut needed),
            StrataStatus::Ok
        );
        assert!(needed > 20);

        let mut buf = vec![0 as c_char; needed];
        let mut written = 0usize;
        assert_eq!(
            strata_eval_tailwind_css_checked(classes.as_ptr(), buf.as_mut_ptr(), buf.len(), &mut written),
            StrataStatus::Ok
        );
        let css = CStr::from_ptr(buf.as_ptr()).to_str().unwrap();
        assert!(css.contains("display: flex;"), "css was: {css}");
        assert!(css.contains("flex-direction: column;"), "css was: {css}");
        assert!(css.contains("width: 256px;"), "css was: {css}");
        assert!(css.contains("height: 128px;"), "css was: {css}");
        assert!(css.contains("background-color: rgba(59, 130, 246, 1);"), "css was: {css}");
        assert!(!css.contains("Px("), "Must never leak Rust Debug syntax: {css}");
    }
}

#[test]
fn test_typed_f64_bool_str_signals_and_null_error_diagnostics() {
    unsafe {
        let f_sig = strata_signal_create_f64(3.25);
        assert!((strata_signal_get_f64(f_sig) - 3.25).abs() < 1e-9);
        assert_eq!(strata_signal_set_f64(f_sig, 8.5), StrataStatus::Ok);
        assert!((strata_signal_get_f64(f_sig) - 8.5).abs() < 1e-9);
        strata_signal_destroy(f_sig);

        let b_sig = strata_signal_create_bool(true);
        assert!(strata_signal_get_bool(b_sig));
        assert_eq!(strata_signal_set_bool(b_sig, false), StrataStatus::Ok);
        assert!(!strata_signal_get_bool(b_sig));
        strata_signal_destroy(b_sig);

        let init = CString::new("hello ffi").unwrap();
        let s_sig = strata_signal_create_str(init.as_ptr());
        let mut needed = 0usize;
        assert_eq!(
            strata_signal_get_str(s_sig, std::ptr::null_mut(), 0, &mut needed),
            StrataStatus::Ok
        );
        assert_eq!(needed, "hello ffi".len() + 1);
        let mut buf = vec![0 as c_char; needed];
        let mut written = 0usize;
        assert_eq!(
            strata_signal_get_str(s_sig, buf.as_mut_ptr(), buf.len(), &mut written),
            StrataStatus::Ok
        );
        assert_eq!(CStr::from_ptr(buf.as_ptr()).to_str().unwrap(), "hello ffi");

        let updated = CString::new("updated text").unwrap();
        assert_eq!(strata_signal_set_str(s_sig, updated.as_ptr()), StrataStatus::Ok);
        let mut tiny = [0 as c_char; 4];
        assert_eq!(
            strata_signal_get_str(s_sig, tiny.as_mut_ptr(), tiny.len(), &mut needed),
            StrataStatus::BufferTooSmall
        );
        assert_eq!(needed, "updated text".len() + 1);
        strata_signal_destroy(s_sig);

        // NULL pointer check populates strata_last_error() cleanly
        assert_eq!(
            strata_signal_set_f64(std::ptr::null_mut(), 1.0),
            StrataStatus::NullPointer
        );
        assert!(CStr::from_ptr(strata_last_error()).to_str().unwrap().contains("NULL"));
        strata_clear_last_error();
        assert_eq!(CStr::from_ptr(strata_last_error()).to_str().unwrap(), "");
    }
}

