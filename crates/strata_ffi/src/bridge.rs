//! C ABI bridge for Tailwind CSS evaluation and offscreen frame buffer rendering.

use std::ffi::CStr;
use std::os::raw::c_char;
use strata_style::TailwindEngine;

/// Evaluates a space-separated list of Tailwind CSS classes and writes the generated CSS into `out_buf`.
/// Returns the number of bytes written (excluding trailing null) on success, or -1 on error.
#[no_mangle]
pub unsafe extern "C" fn strata_eval_tailwind_css(
    classes: *const c_char,
    out_buf: *mut c_char,
    max_len: usize,
) -> i32 {
    if classes.is_null() || out_buf.is_null() || max_len == 0 {
        return -1;
    }

    let class_str = match CStr::from_ptr(classes).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };

    let style = TailwindEngine::resolve_classes(class_str);
    let mut css_out = String::new();

    match style.width {
        strata_style::Dimension::Auto => {}
        _ => css_out.push_str(&format!("width: {:?}; ", style.width)),
    }
    match style.height {
        strata_style::Dimension::Auto => {}
        _ => css_out.push_str(&format!("height: {:?}; ", style.height)),
    }
    if let Some(c) = style.background_color {
        css_out.push_str(&format!("background-color: rgba({}, {}, {}, {}); ", c.r, c.g, c.b, c.a));
    }
    css_out.push_str(&format!("color: rgba({}, {}, {}, {}); ", style.color.r, style.color.g, style.color.b, style.color.a));

    let bytes = css_out.as_bytes();
    if bytes.len() >= max_len {
        return -1;
    }

    std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_buf as *mut u8, bytes.len());
    *out_buf.add(bytes.len()) = 0; // Null terminator

    bytes.len() as i32
}

/// Renders a solid background with alpha into a caller-allocated RGBA8 offscreen memory buffer.
/// Returns total bytes written (`width * height * 4`), or -1 on invalid buffer.
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
    if out_rgba_ptr.is_null() || width == 0 || height == 0 {
        return -1;
    }

    let total_pixels = (width as usize) * (height as usize);
    let total_bytes = total_pixels * 4;

    for i in 0..total_pixels {
        let offset = i * 4;
        *out_rgba_ptr.add(offset) = r;
        *out_rgba_ptr.add(offset + 1) = g;
        *out_rgba_ptr.add(offset + 2) = b;
        *out_rgba_ptr.add(offset + 3) = a;
    }

    total_bytes as i32
}
