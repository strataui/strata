//! C ABI bridge for standards-compliant Tailwind CSS evaluation and offscreen buffer rendering.

use std::ffi::CStr;
use std::os::raw::c_char;

use strata_style::TailwindEngine;

use crate::error::{
    clear_last_error_internal, ffi_guard, ffi_guard_i32, set_last_error, StrataStatus,
};
use crate::style::serialize_computed_style_css;

/// Evaluates a space-separated list of Tailwind CSS classes into standards-compliant inline CSS
/// (`width: 256px; height: 128px; background-color: rgba(59, 130, 246, 1); ...`).
///
/// Supports two-call length probing: pass `out_buf == NULL` to write the required capacity
/// (including trailing NUL) into `*out_written` and return [`StrataStatus::Ok`].
#[no_mangle]
pub unsafe extern "C" fn strata_eval_tailwind_css_checked(
    classes: *const c_char,
    out_buf: *mut c_char,
    buf_len: usize,
    out_written: *mut usize,
) -> StrataStatus {
    ffi_guard(|| {
        if classes.is_null() {
            set_last_error("strata_eval_tailwind_css_checked: classes pointer is NULL");
            return StrataStatus::NullPointer;
        }
        let class_str = match CStr::from_ptr(classes).to_str() {
            Ok(s) => s,
            Err(_) => {
                set_last_error("strata_eval_tailwind_css_checked: classes is invalid UTF-8");
                return StrataStatus::InvalidUtf8;
            }
        };

        let style = TailwindEngine::resolve_classes(class_str);
        let css_out = serialize_computed_style_css(&style);
        let bytes = css_out.as_bytes();
        let required = bytes.len() + 1;

        if out_buf.is_null() {
            if !out_written.is_null() {
                *out_written = required;
            }
            clear_last_error_internal();
            return StrataStatus::Ok;
        }
        if buf_len < required {
            if !out_written.is_null() {
                *out_written = required;
            }
            set_last_error(format!(
                "strata_eval_tailwind_css_checked: buffer too small (provided {buf_len}, required {required})"
            ));
            return StrataStatus::BufferTooSmall;
        }

        std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_buf as *mut u8, bytes.len());
        *out_buf.add(bytes.len()) = 0;
        if !out_written.is_null() {
            *out_written = bytes.len();
        }
        clear_last_error_internal();
        StrataStatus::Ok
    })
}

/// Evaluates a space-separated list of Tailwind CSS classes and writes standards-compliant CSS
/// into `out_buf`. Returns bytes written (excluding NUL) on success, or `-1` on error.
#[no_mangle]
pub unsafe extern "C" fn strata_eval_tailwind_css(
    classes: *const c_char,
    out_buf: *mut c_char,
    max_len: usize,
) -> i32 {
    ffi_guard_i32(|| {
        if classes.is_null() || out_buf.is_null() || max_len == 0 {
            set_last_error("strata_eval_tailwind_css: invalid buffer or NULL pointer");
            return -1;
        }
        let mut written: usize = 0;
        match strata_eval_tailwind_css_checked(classes, out_buf, max_len, &mut written) {
            StrataStatus::Ok => written as i32,
            _ => -1,
        }
    })
}

/// Renders a solid rectangle with alpha into a caller-allocated RGBA8 buffer using Strata's
/// `DisplayList` and analytical SDF rasterizer. Returns total bytes written (`width * height * 4`),
/// or `-1` on invalid arguments.
#[no_mangle]
pub unsafe extern "C" fn strata_render_to_buffer(
    width: u32,
    height: u32,
    r: u8,
    g: u8,
    b: u8,
    a: u8,
    out_rgba_ptr: *mut u8,
) -> i32 {
    ffi_guard_i32(|| {
        if out_rgba_ptr.is_null() || width == 0 || height == 0 {
            set_last_error("strata_render_to_buffer: NULL output pointer or zero dimensions");
            return -1;
        }

        let mut fb = strata_render::rasterizer::FrameBuffer::new(width as usize, height as usize);
        let mut dl = strata_render::compositor::DisplayList::new();
        dl.push(strata_render::compositor::DrawCommand::DrawRect {
            bounds: strata_layout::Rect::new(0.0, 0.0, width as f32, height as f32),
            color: strata_style::types::Color::rgba(r, g, b, a),
        });
        fb.render_display_list(&dl);

        let total_bytes = fb.pixels.len();
        std::ptr::copy_nonoverlapping(fb.pixels.as_ptr(), out_rgba_ptr, total_bytes);
        clear_last_error_internal();
        total_bytes as i32
    })
}
