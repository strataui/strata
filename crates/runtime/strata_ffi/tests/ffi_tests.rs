use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_void};
use strata_ffi::*;

#[test]
fn test_version_string() {
    let ver_ptr = strata_version();
    assert!(!ver_ptr.is_null());
    let ver_str = unsafe { CStr::from_ptr(ver_ptr).to_str().unwrap() };
    assert_eq!(ver_str, "0.1.0");
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
