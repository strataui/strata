//! High-Fidelity Native GTK3 and Cairo Presentation Backend for Linux.
//!
//! Connects Strata's DisplayList directly to Cairo graphics context with system typography,
//! FreeType/HarfBuzz font shaping, and subpixel anti-aliased vector geometry.

use strata_render::compositor::{DisplayList, DrawCommand};
use strata_render::{FillRule, StrokeLineCap, StrokeLineJoin};
use strata_style::types::{BorderStyle, FontStyle, FontWeight, TextDecoration};

#[inline]
fn gtk_perf_logging_enabled() -> bool {
    static PERF_ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *PERF_ENABLED.get_or_init(|| std::env::var_os("STRATA_PERF_LOG").is_some())
}

/// Draws a rounded rectangle path into a Cairo graphics context.
pub fn cairo_rounded_rect(cr: &cairo::Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    if r <= 0.001 {
        cr.rectangle(x, y, w, h);
        return;
    }
    let degrees = std::f64::consts::PI / 180.0;

    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -90.0 * degrees, 0.0 * degrees);
    cr.arc(x + w - r, y + h - r, r, 0.0 * degrees, 90.0 * degrees);
    cr.arc(x + r, y + h - r, r, 90.0 * degrees, 180.0 * degrees);
    cr.arc(x + r, y + r, r, 180.0 * degrees, 270.0 * degrees);
    cr.close_path();
}

/// Draws a per-corner CSS rounded rectangle path into a Cairo graphics context.
pub fn cairo_rounded_rect_radii(
    cr: &cairo::Context,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    radius: strata_style::types::BorderRadius,
) {
    let w = w.max(0.0);
    let h = h.max(0.0);
    let mut tl = (radius.top_left as f64).max(0.0);
    let mut tr = (radius.top_right as f64).max(0.0);
    let mut br = (radius.bottom_right as f64).max(0.0);
    let mut bl = (radius.bottom_left as f64).max(0.0);

    if tl <= 0.001 && tr <= 0.001 && br <= 0.001 && bl <= 0.001 {
        cr.rectangle(x, y, w, h);
        return;
    }

    // CSS Backgrounds and Borders Level 3 proportional corner clamping
    let mut scale = 1.0_f64;
    if tl + tr > w && tl + tr > 0.0 {
        scale = scale.min(w / (tl + tr));
    }
    if bl + br > w && bl + br > 0.0 {
        scale = scale.min(w / (bl + br));
    }
    if tl + bl > h && tl + bl > 0.0 {
        scale = scale.min(h / (tl + bl));
    }
    if tr + br > h && tr + br > 0.0 {
        scale = scale.min(h / (tr + br));
    }
    tl *= scale;
    tr *= scale;
    br *= scale;
    bl *= scale;

    let half_pi = std::f64::consts::FRAC_PI_2;
    let pi = std::f64::consts::PI;

    cr.new_sub_path();
    cr.move_to(x + tl, y);
    if tr > 0.001 {
        cr.arc(x + w - tr, y + tr, tr, -half_pi, 0.0);
    } else {
        cr.line_to(x + w, y);
    }
    if br > 0.001 {
        cr.arc(x + w - br, y + h - br, br, 0.0, half_pi);
    } else {
        cr.line_to(x + w, y + h);
    }
    if bl > 0.001 {
        cr.arc(x + bl, y + h - bl, bl, half_pi, pi);
    } else {
        cr.line_to(x, y + h);
    }
    if tl > 0.001 {
        cr.arc(x + tl, y + tl, tl, pi, 3.0 * half_pi);
    } else {
        cr.line_to(x, y);
    }
    cr.close_path();
}

#[inline]
fn inset_border_radius(
    radius: strata_style::types::BorderRadius,
    inset: f32,
) -> strata_style::types::BorderRadius {
    strata_style::types::BorderRadius::new(
        (radius.top_left - inset).max(0.0),
        (radius.top_right - inset).max(0.0),
        (radius.bottom_right - inset).max(0.0),
        (radius.bottom_left - inset).max(0.0),
    )
}

/// Strokes a uniform border edge using the requested CSS border style and per-corner radii.
fn stroke_uniform_border(
    cr: &cairo::Context,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    radius: strata_style::types::BorderRadius,
    border_width: f64,
    style: BorderStyle,
) {
    if border_width <= 0.0 {
        return;
    }
    match style {
        BorderStyle::None | BorderStyle::Hidden => {}
        BorderStyle::Solid => {
            let half = border_width / 2.0;
            let r = inset_border_radius(radius, half as f32);
            cairo_rounded_rect_radii(
                cr,
                x + half,
                y + half,
                width - border_width,
                height - border_width,
                r,
            );
            cr.set_line_width(border_width);
            cr.set_dash(&[], 0.0);
            cr.set_line_cap(cairo::LineCap::Butt);
            let _ = cr.stroke();
        }
        BorderStyle::Dashed => {
            let half = border_width / 2.0;
            let r = inset_border_radius(radius, half as f32);
            cairo_rounded_rect_radii(
                cr,
                x + half,
                y + half,
                width - border_width,
                height - border_width,
                r,
            );
            cr.set_line_width(border_width);
            cr.set_dash(&[3.0 * border_width, 3.0 * border_width], 0.0);
            cr.set_line_cap(cairo::LineCap::Butt);
            let _ = cr.stroke();
            cr.set_dash(&[], 0.0);
        }
        BorderStyle::Dotted => {
            let half = border_width / 2.0;
            let r = inset_border_radius(radius, half as f32);
            cairo_rounded_rect_radii(
                cr,
                x + half,
                y + half,
                width - border_width,
                height - border_width,
                r,
            );
            cr.set_line_width(border_width);
            cr.set_dash(&[0.0, 2.0 * border_width], 0.0);
            cr.set_line_cap(cairo::LineCap::Round);
            let _ = cr.stroke();
            cr.set_dash(&[], 0.0);
            cr.set_line_cap(cairo::LineCap::Butt);
        }
        BorderStyle::Double => {
            let stroke_width = border_width / 3.0;
            for inset in [border_width / 6.0, 5.0 * border_width / 6.0] {
                let r = inset_border_radius(radius, inset as f32);
                cairo_rounded_rect_radii(
                    cr,
                    x + inset,
                    y + inset,
                    width - 2.0 * inset,
                    height - 2.0 * inset,
                    r,
                );
                cr.set_line_width(stroke_width);
                cr.set_dash(&[], 0.0);
                cr.set_line_cap(cairo::LineCap::Butt);
                let _ = cr.stroke();
            }
        }
    }
}

/// Strokes a single non-uniform border edge with dashed, dotted, or solid pattern.
fn stroke_border_side(
    cr: &cairo::Context,
    style: BorderStyle,
    w: f64,
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
) {
    cr.set_line_width(w);
    match style {
        BorderStyle::Dashed => {
            cr.set_dash(&[3.0 * w, 3.0 * w], 0.0);
            cr.set_line_cap(cairo::LineCap::Butt);
        }
        BorderStyle::Dotted => {
            cr.set_dash(&[0.0, 2.0 * w], 0.0);
            cr.set_line_cap(cairo::LineCap::Round);
        }
        _ => {
            cr.set_dash(&[], 0.0);
            cr.set_line_cap(cairo::LineCap::Butt);
        }
    }
    cr.move_to(x1, y1);
    cr.line_to(x2, y2);
    let _ = cr.stroke();
    cr.set_dash(&[], 0.0);
    cr.set_line_cap(cairo::LineCap::Butt);
}

#[inline]
fn expand_shadow_corner(base_r: f32, spread: f64, blur_expand: f64) -> f32 {
    if base_r > 0.001 {
        (base_r as f64 + spread + blur_expand).max(0.0) as f32
    } else {
        (blur_expand * 0.5).max(0.0) as f32
    }
}

/// Renders an outer box shadow with per-corner border-radius contour and Even-Odd interior knockout.
pub fn cairo_draw_shadow(
    cr: &cairo::Context,
    bounds: &strata_layout::Rect,
    shadow: &strata_style::types::BoxShadow,
    border_radius: strata_style::types::BorderRadius,
) {
    if shadow.color.a() <= 0.001 || bounds.width <= 0.0 || bounds.height <= 0.0 {
        return;
    }
    let r = shadow.color.r() as f64;
    let g = shadow.color.g() as f64;
    let b = shadow.color.b() as f64;
    let base_a = shadow.color.a() as f64;

    let _ = cr.save();

    // CSS Backgrounds and Borders Level 3: clip out the element's own rounded border-box
    // via Even-Odd fill rule so outer drop shadows never darken translucent glass interiors.
    let max_expand =
        (shadow.spread_radius.max(0.0) + shadow.blur_radius.max(0.0) * 1.5 + 8.0) as f64;
    let bx1 = bounds.x as f64;
    let by1 = bounds.y as f64;
    let bx2 = (bounds.x + bounds.width) as f64;
    let by2 = (bounds.y + bounds.height) as f64;
    let sx1 = (bounds.x + shadow.offset_x) as f64 - max_expand;
    let sy1 = (bounds.y + shadow.offset_y) as f64 - max_expand;
    let sx2 = (bounds.x + bounds.width + shadow.offset_x) as f64 + max_expand;
    let sy2 = (bounds.y + bounds.height + shadow.offset_y) as f64 + max_expand;
    let outer_x1 = bx1.min(sx1) - 4.0;
    let outer_y1 = by1.min(sy1) - 4.0;
    let outer_x2 = bx2.max(sx2) + 4.0;
    let outer_y2 = by2.max(sy2) + 4.0;

    cr.new_path();
    cr.rectangle(
        outer_x1,
        outer_y1,
        (outer_x2 - outer_x1).max(0.0),
        (outer_y2 - outer_y1).max(0.0),
    );
    cairo_rounded_rect_radii(
        cr,
        bx1,
        by1,
        bounds.width as f64,
        bounds.height as f64,
        border_radius,
    );
    cr.set_fill_rule(cairo::FillRule::EvenOdd);
    cr.clip();
    cr.set_fill_rule(cairo::FillRule::Winding);

    let spread = shadow.spread_radius as f64;

    if shadow.blur_radius <= 0.5 {
        let x = (bounds.x + shadow.offset_x) as f64 - spread;
        let y = (bounds.y + shadow.offset_y) as f64 - spread;
        let w = (bounds.width as f64 + spread * 2.0).max(0.0);
        let h = (bounds.height as f64 + spread * 2.0).max(0.0);
        let sharp_r = strata_style::types::BorderRadius::new(
            expand_shadow_corner(border_radius.top_left, spread, 0.0),
            expand_shadow_corner(border_radius.top_right, spread, 0.0),
            expand_shadow_corner(border_radius.bottom_right, spread, 0.0),
            expand_shadow_corner(border_radius.bottom_left, spread, 0.0),
        );
        cairo_rounded_rect_radii(cr, x, y, w, h, sharp_r);
        cr.set_source_rgba(r, g, b, base_a);
        let _ = cr.fill();
        let _ = cr.restore();
        return;
    }

    // Concentric Gaussian approximation following per-corner radii
    let steps = (shadow.blur_radius.ceil() as usize).clamp(4, 16);
    let blur = shadow.blur_radius as f64;
    let total_layers = steps as f64;

    for i in 0..steps {
        let t = (i as f64 + 0.5) / total_layers;
        let alpha = base_a * (1.0 - t * t) / (total_layers * 0.65);
        let blur_expand = t * blur;
        let current_expand = spread + blur_expand;

        let x = (bounds.x + shadow.offset_x) as f64 - current_expand;
        let y = (bounds.y + shadow.offset_y) as f64 - current_expand;
        let w = (bounds.width as f64 + current_expand * 2.0).max(0.0);
        let h = (bounds.height as f64 + current_expand * 2.0).max(0.0);

        let step_r = strata_style::types::BorderRadius::new(
            expand_shadow_corner(border_radius.top_left, spread, blur_expand),
            expand_shadow_corner(border_radius.top_right, spread, blur_expand),
            expand_shadow_corner(border_radius.bottom_right, spread, blur_expand),
            expand_shadow_corner(border_radius.bottom_left, spread, blur_expand),
        );
        cairo_rounded_rect_radii(cr, x, y, w, h, step_r);
        cr.set_source_rgba(r, g, b, alpha);
        let _ = cr.fill();
    }

    let _ = cr.restore();
}

/// Renders an inset box shadow clipped to the element's border box in Cairo.
pub fn cairo_draw_inset_shadow(
    cr: &cairo::Context,
    bounds: &strata_layout::Rect,
    shadow: &strata_style::types::BoxShadow,
    border_radius: strata_style::types::BorderRadius,
) {
    if shadow.color.a() <= 0.001 {
        return;
    }
    let _ = cr.save();
    cairo_rounded_rect_radii(
        cr,
        bounds.x as f64,
        bounds.y as f64,
        bounds.width as f64,
        bounds.height as f64,
        border_radius,
    );
    cr.clip();

    let r = shadow.color.r() as f64;
    let g = shadow.color.g() as f64;
    let b = shadow.color.b() as f64;
    let base_a = shadow.color.a() as f64;

    let blur = shadow.blur_radius.max(1.0) as f64;
    let steps = (blur.ceil() as usize).clamp(3, 10);
    let total_layers = steps as f64;

    for i in 0..steps {
        let t = (i as f64 + 0.5) / total_layers;
        let alpha = base_a * (1.0 - t * t) / (total_layers * 0.7);
        let stroke_w = (t * blur * 1.5).max(1.0);

        let inset_x = bounds.x as f64 + shadow.offset_x as f64;
        let inset_y = bounds.y as f64 + shadow.offset_y as f64;
        let inset_w = bounds.width as f64;
        let inset_h = bounds.height as f64;

        cairo_rounded_rect_radii(cr, inset_x, inset_y, inset_w, inset_h, border_radius);
        cr.set_source_rgba(r, g, b, alpha);
        cr.set_line_width(stroke_w);
        let _ = cr.stroke();
    }

    let _ = cr.restore();
}

/// Resolves missing and out-of-order color stop positions per CSS Images Level 3 section 3.4.3.
fn resolve_gradient_stop_positions(stops: &[strata_style::types::ColorStop]) -> Vec<f64> {
    let n = stops.len();
    if n == 0 {
        return Vec::new();
    }
    let mut positions: Vec<Option<f64>> =
        stops.iter().map(|s| s.position.map(|p| p as f64)).collect();
    if positions[0].is_none() {
        positions[0] = Some(0.0);
    }
    if positions[n - 1].is_none() {
        positions[n - 1] = Some(1.0);
    }

    let mut max_seen = positions[0].unwrap_or(0.0);
    for pos in positions.iter_mut().flatten() {
        if *pos < max_seen {
            *pos = max_seen;
        } else {
            max_seen = *pos;
        }
    }

    let mut out = vec![0.0; n];
    let mut idx = 0;
    while idx < n {
        if let Some(p) = positions[idx] {
            out[idx] = p;
            idx += 1;
        } else {
            let start_idx = idx - 1;
            let start_pos = out[start_idx];
            let mut end_idx = idx + 1;
            while end_idx < n && positions[end_idx].is_none() {
                end_idx += 1;
            }
            let end_pos = positions[end_idx.min(n - 1)].unwrap_or(1.0);
            let span = (end_idx - start_idx) as f64;
            for (k, slot) in out.iter_mut().enumerate().take(end_idx).skip(idx) {
                *slot = start_pos + (end_pos - start_pos) * ((k - start_idx) as f64 / span);
            }
            idx = end_idx;
        }
    }
    out
}

/// Renders a CSS Linear Gradient using high-precision Cairo LinearGradient patterns.
pub fn cairo_draw_linear_gradient(
    cr: &cairo::Context,
    bounds: &strata_layout::Rect,
    angle_deg: f32,
    stops: &[strata_style::types::ColorStop],
    border_radius: strata_style::types::BorderRadius,
) {
    if stops.is_empty() || bounds.width <= 0.0 || bounds.height <= 0.0 {
        return;
    }

    let rad = (angle_deg.to_radians()) as f64;
    let w = bounds.width as f64;
    let h = bounds.height as f64;
    let cx = bounds.x as f64 + w / 2.0;
    let cy = bounds.y as f64 + h / 2.0;

    let length = (w * rad.sin().abs() + h * rad.cos().abs()) / 2.0;
    let dx = rad.sin() * length;
    let dy = -rad.cos() * length;

    let x0 = cx - dx;
    let y0 = cy - dy;
    let x1 = cx + dx;
    let y1 = cy + dy;

    let pattern = cairo::LinearGradient::new(x0, y0, x1, y1);
    let resolved_positions = resolve_gradient_stop_positions(stops);
    for (stop, pos) in stops.iter().zip(resolved_positions) {
        pattern.add_color_stop_rgba(
            pos.clamp(0.0, 1.0),
            stop.color.r() as f64,
            stop.color.g() as f64,
            stop.color.b() as f64,
            stop.color.a() as f64,
        );
    }

    cairo_rounded_rect_radii(cr, bounds.x as f64, bounds.y as f64, w, h, border_radius);

    let _ = cr.set_source(&pattern);
    let _ = cr.fill();
}

/// Renders a CSS Radial Gradient using high-precision Cairo RadialGradient patterns.
pub fn cairo_draw_radial_gradient(
    cr: &cairo::Context,
    bounds: &strata_layout::Rect,
    center: (f32, f32),
    stops: &[strata_style::types::ColorStop],
    border_radius: strata_style::types::BorderRadius,
) {
    if stops.is_empty() || bounds.width <= 0.0 || bounds.height <= 0.0 {
        return;
    }

    let w = bounds.width as f64;
    let h = bounds.height as f64;
    let cx = bounds.x as f64 + w * center.0 as f64;
    let cy = bounds.y as f64 + h * center.1 as f64;
    let radius = (w.max(h) / 2.0).max(1.0);

    let pattern = cairo::RadialGradient::new(cx, cy, 0.0, cx, cy, radius);
    let resolved_positions = resolve_gradient_stop_positions(stops);
    for (stop, pos) in stops.iter().zip(resolved_positions) {
        pattern.add_color_stop_rgba(
            pos.clamp(0.0, 1.0),
            stop.color.r() as f64,
            stop.color.g() as f64,
            stop.color.b() as f64,
            stop.color.a() as f64,
        );
    }

    cairo_rounded_rect_radii(cr, bounds.x as f64, bounds.y as f64, w, h, border_radius);

    let _ = cr.set_source(&pattern);
    let _ = cr.fill();
}

fn box_blur_horizontal(src: &[u8], dst: &mut [u8], w: usize, h: usize, radius: usize) {
    let diam = (2 * radius + 1) as u32;
    let half = diam / 2;
    for y in 0..h {
        let row = y * w * 4;
        let mut sum_b = src[row] as u32 * (radius as u32 + 1);
        let mut sum_g = src[row + 1] as u32 * (radius as u32 + 1);
        let mut sum_r = src[row + 2] as u32 * (radius as u32 + 1);
        let mut sum_a = src[row + 3] as u32 * (radius as u32 + 1);
        for ix in 1..=radius {
            let p = row + ix.min(w - 1) * 4;
            sum_b += src[p] as u32;
            sum_g += src[p + 1] as u32;
            sum_r += src[p + 2] as u32;
            sum_a += src[p + 3] as u32;
        }
        for x in 0..w {
            let out = row + x * 4;
            dst[out] = ((sum_b + half) / diam) as u8;
            dst[out + 1] = ((sum_g + half) / diam) as u8;
            dst[out + 2] = ((sum_r + half) / diam) as u8;
            dst[out + 3] = ((sum_a + half) / diam) as u8;

            let left = row + x.saturating_sub(radius) * 4;
            let right = row + (x + radius + 1).min(w - 1) * 4;
            sum_b = sum_b + src[right] as u32 - src[left] as u32;
            sum_g = sum_g + src[right + 1] as u32 - src[left + 1] as u32;
            sum_r = sum_r + src[right + 2] as u32 - src[left + 2] as u32;
            sum_a = sum_a + src[right + 3] as u32 - src[left + 3] as u32;
        }
    }
}

fn box_blur_vertical(src: &[u8], dst: &mut [u8], w: usize, h: usize, radius: usize) {
    let diam = (2 * radius + 1) as u32;
    let half = diam / 2;
    for x in 0..w {
        let col0 = x * 4;
        let mut sum_b = src[col0] as u32 * (radius as u32 + 1);
        let mut sum_g = src[col0 + 1] as u32 * (radius as u32 + 1);
        let mut sum_r = src[col0 + 2] as u32 * (radius as u32 + 1);
        let mut sum_a = src[col0 + 3] as u32 * (radius as u32 + 1);
        for iy in 1..=radius {
            let p = iy.min(h - 1) * w * 4 + col0;
            sum_b += src[p] as u32;
            sum_g += src[p + 1] as u32;
            sum_r += src[p + 2] as u32;
            sum_a += src[p + 3] as u32;
        }
        for y in 0..h {
            let out = y * w * 4 + col0;
            dst[out] = ((sum_b + half) / diam) as u8;
            dst[out + 1] = ((sum_g + half) / diam) as u8;
            dst[out + 2] = ((sum_r + half) / diam) as u8;
            dst[out + 3] = ((sum_a + half) / diam) as u8;

            let top = y.saturating_sub(radius) * w * 4 + col0;
            let bot = (y + radius + 1).min(h - 1) * w * 4 + col0;
            sum_b = sum_b + src[bot] as u32 - src[top] as u32;
            sum_g = sum_g + src[bot + 1] as u32 - src[top + 1] as u32;
            sum_r = sum_r + src[bot + 2] as u32 - src[top + 2] as u32;
            sum_a = sum_a + src[bot + 3] as u32 - src[top + 3] as u32;
        }
    }
}

fn box_blur_3pass(buf: &mut [u8], w: usize, h: usize, radius: usize) {
    if radius == 0 || w == 0 || h == 0 {
        return;
    }
    let mut tmp = vec![0u8; buf.len()];
    for _ in 0..3 {
        box_blur_horizontal(buf, &mut tmp, w, h, radius);
        box_blur_vertical(&tmp, buf, w, h, radius);
    }
}

fn apply_color_filter_op_premul(buf: &mut [u8], op: strata_style::effects::FilterOp) {
    use strata_style::effects::FilterOp;
    match op {
        FilterOp::Blur(_) | FilterOp::DropShadow { .. } => {}
        FilterOp::Brightness(amount) => {
            let f = amount.max(0.0);
            for px in buf.chunks_exact_mut(4) {
                let a = px[3] as f32;
                px[0] = (px[0] as f32 * f).round().clamp(0.0, a) as u8;
                px[1] = (px[1] as f32 * f).round().clamp(0.0, a) as u8;
                px[2] = (px[2] as f32 * f).round().clamp(0.0, a) as u8;
            }
        }
        FilterOp::Contrast(amount) => {
            let f = amount.max(0.0);
            for px in buf.chunks_exact_mut(4) {
                let a = px[3] as f32;
                let mid = 0.5 * a;
                px[0] = ((px[0] as f32 - mid) * f + mid).round().clamp(0.0, a) as u8;
                px[1] = ((px[1] as f32 - mid) * f + mid).round().clamp(0.0, a) as u8;
                px[2] = ((px[2] as f32 - mid) * f + mid).round().clamp(0.0, a) as u8;
            }
        }
        FilterOp::Saturate(amount) => {
            let s = amount.max(0.0);
            for px in buf.chunks_exact_mut(4) {
                let a = px[3] as f32;
                let b = px[0] as f32;
                let g = px[1] as f32;
                let r = px[2] as f32;
                let lum = 0.2126 * r + 0.7152 * g + 0.0722 * b;
                px[0] = (lum + (b - lum) * s).round().clamp(0.0, a) as u8;
                px[1] = (lum + (g - lum) * s).round().clamp(0.0, a) as u8;
                px[2] = (lum + (r - lum) * s).round().clamp(0.0, a) as u8;
            }
        }
        FilterOp::Grayscale(amount) => {
            let t = amount.clamp(0.0, 1.0);
            for px in buf.chunks_exact_mut(4) {
                let a = px[3] as f32;
                let b = px[0] as f32;
                let g = px[1] as f32;
                let r = px[2] as f32;
                let lum = 0.2126 * r + 0.7152 * g + 0.0722 * b;
                px[0] = (b + (lum - b) * t).round().clamp(0.0, a) as u8;
                px[1] = (g + (lum - g) * t).round().clamp(0.0, a) as u8;
                px[2] = (r + (lum - r) * t).round().clamp(0.0, a) as u8;
            }
        }
        FilterOp::Invert(amount) => {
            let t = amount.clamp(0.0, 1.0);
            for px in buf.chunks_exact_mut(4) {
                let a = px[3] as f32;
                px[0] = (px[0] as f32 + (a - 2.0 * px[0] as f32) * t)
                    .round()
                    .clamp(0.0, a) as u8;
                px[1] = (px[1] as f32 + (a - 2.0 * px[1] as f32) * t)
                    .round()
                    .clamp(0.0, a) as u8;
                px[2] = (px[2] as f32 + (a - 2.0 * px[2] as f32) * t)
                    .round()
                    .clamp(0.0, a) as u8;
            }
        }
        FilterOp::Sepia(amount) => {
            let t = amount.clamp(0.0, 1.0);
            for px in buf.chunks_exact_mut(4) {
                let a = px[3] as f32;
                let b = px[0] as f32;
                let g = px[1] as f32;
                let r = px[2] as f32;
                let sr = (0.393 * r + 0.769 * g + 0.189 * b).min(a);
                let sg = (0.349 * r + 0.686 * g + 0.168 * b).min(a);
                let sb = (0.272 * r + 0.534 * g + 0.131 * b).min(a);
                px[0] = (b + (sb - b) * t).round().clamp(0.0, a) as u8;
                px[1] = (g + (sg - g) * t).round().clamp(0.0, a) as u8;
                px[2] = (r + (sr - r) * t).round().clamp(0.0, a) as u8;
            }
        }
        FilterOp::Opacity(amount) => {
            let f = amount.clamp(0.0, 1.0);
            for px in buf.chunks_exact_mut(4) {
                px[0] = (px[0] as f32 * f).round() as u8;
                px[1] = (px[1] as f32 * f).round() as u8;
                px[2] = (px[2] as f32 * f).round() as u8;
                px[3] = (px[3] as f32 * f).round() as u8;
            }
        }
        FilterOp::HueRotate(deg) => {
            let rad = deg.to_radians();
            let cos_a = rad.cos();
            let sin_a = rad.sin();
            let m00 = 0.213 + cos_a * 0.787 - sin_a * 0.213;
            let m01 = 0.715 - cos_a * 0.715 - sin_a * 0.715;
            let m02 = 0.072 - cos_a * 0.072 + sin_a * 0.928;
            let m10 = 0.213 - cos_a * 0.213 + sin_a * 0.143;
            let m11 = 0.715 + cos_a * 0.285 + sin_a * 0.140;
            let m12 = 0.072 - cos_a * 0.072 - sin_a * 0.283;
            let m20 = 0.213 - cos_a * 0.213 - sin_a * 0.787;
            let m21 = 0.715 - cos_a * 0.715 + sin_a * 0.715;
            let m22 = 0.072 + cos_a * 0.928 + sin_a * 0.072;
            for px in buf.chunks_exact_mut(4) {
                let a = px[3] as f32;
                let b = px[0] as f32;
                let g = px[1] as f32;
                let r = px[2] as f32;
                px[2] = (r * m00 + g * m01 + b * m02).round().clamp(0.0, a) as u8;
                px[1] = (r * m10 + g * m11 + b * m12).round().clamp(0.0, a) as u8;
                px[0] = (r * m20 + g * m21 + b * m22).round().clamp(0.0, a) as u8;
            }
        }
    }
}

/// Applies CSS `backdrop-filter` operations (3-pass separable Gaussian-approximating box blur,
/// saturate, brightness, contrast, etc.) to the underlying Cairo surface pixels inside `bounds`
/// clipped to `radius`, restoring the Cairo clip immediately so outer shadows and children are not clipped.
pub fn cairo_apply_backdrop_filter(
    cr: &cairo::Context,
    bounds: &strata_layout::Rect,
    radius: strata_style::types::BorderRadius,
    filter: &strata_style::effects::BackdropFilter,
) {
    if filter.is_empty() || bounds.width <= 0.0 || bounds.height <= 0.0 {
        return;
    }

    let (d0x, d0y) = cr.user_to_device(bounds.x as f64, bounds.y as f64);
    let (d1x, d1y) = cr.user_to_device(
        (bounds.x + bounds.width) as f64,
        (bounds.y + bounds.height) as f64,
    );
    let bx1 = d0x.min(d1x);
    let by1 = d0y.min(d1y);
    let bx2 = d0x.max(d1x);
    let by2 = d0y.max(d1y);

    let dev_scale = ((bx2 - bx1) / bounds.width as f64).abs().max(1.0);
    let max_blur = filter.operations.iter().fold(0.0_f64, |acc, op| match op {
        strata_style::effects::FilterOp::Blur(r) => acc.max(*r as f64),
        _ => acc,
    });
    let pad = ((max_blur * dev_scale).ceil() as i32).clamp(0, 128);

    let (cx1, cy1, cx2, cy2) = if let Ok((ux1, uy1, ux2, uy2)) = cr.clip_extents() {
        let (dc0x, dc0y) = cr.user_to_device(ux1, uy1);
        let (dc1x, dc1y) = cr.user_to_device(ux2, uy2);
        (
            dc0x.min(dc1x).floor() as i32,
            dc0y.min(dc1y).floor() as i32,
            dc0x.max(dc1x).ceil() as i32,
            dc0y.max(dc1y).ceil() as i32,
        )
    } else {
        (i32::MIN / 2, i32::MIN / 2, i32::MAX / 2, i32::MAX / 2)
    };

    let sx1 = (bx1.floor() as i32 - pad).max(cx1);
    let sy1 = (by1.floor() as i32 - pad).max(cy1);
    let sx2 = (bx2.ceil() as i32 + pad).min(cx2);
    let sy2 = (by2.ceil() as i32 + pad).min(cy2);
    let sw = sx2 - sx1;
    let sh = sy2 - sy1;
    if sw <= 0 || sh <= 0 {
        return;
    }

    let mut snapshot = match cairo::ImageSurface::create(cairo::Format::ARgb32, sw, sh) {
        Ok(s) => s,
        Err(_) => return,
    };

    {
        let target = cr.target();
        target.flush();
        if let Ok(snap_cr) = cairo::Context::new(&snapshot) {
            snap_cr.set_operator(cairo::Operator::Source);
            let _ = snap_cr.set_source_surface(&target, -sx1 as f64, -sy1 as f64);
            let _ = snap_cr.paint();
        }
    }
    snapshot.flush();

    let w_us = sw as usize;
    let h_us = sh as usize;
    let stride = snapshot.stride() as usize;
    let mut buf = vec![0u8; w_us * h_us * 4];
    if let Ok(data) = snapshot.data() {
        for row in 0..h_us {
            let src_off = row * stride;
            let dst_off = row * w_us * 4;
            buf[dst_off..dst_off + w_us * 4].copy_from_slice(&data[src_off..src_off + w_us * 4]);
        }
    } else {
        return;
    }

    let first_px = [buf[0], buf[1], buf[2], buf[3]];
    let is_uniform = buf.chunks_exact(4).all(|px| px == first_px);
    let has_color_ops = filter
        .operations
        .iter()
        .any(|op| !matches!(*op, strata_style::effects::FilterOp::Blur(_)));

    // A uniform or transparent backdrop (e.g. HUD panels floating over a cleared <Canvas3D> hole)
    // blurs identically to itself, so skip the expensive 3-pass CPU box blur cleanly.
    if is_uniform && !has_color_ops {
        return;
    }

    for op in &filter.operations {
        match *op {
            strata_style::effects::FilterOp::Blur(radius_px) => {
                if !is_uniform {
                    let blur_r = ((radius_px as f64 * dev_scale).round() as usize).min(128);
                    if blur_r >= 1 {
                        box_blur_3pass(&mut buf, w_us, h_us, blur_r);
                    }
                }
            }
            other => apply_color_filter_op_premul(&mut buf, other),
        }
    }

    if let Ok(mut data) = snapshot.data() {
        for row in 0..h_us {
            let src_off = row * w_us * 4;
            let dst_off = row * stride;
            data[dst_off..dst_off + w_us * 4].copy_from_slice(&buf[src_off..src_off + w_us * 4]);
        }
    }
    snapshot.mark_dirty();

    let _ = cr.save();
    cairo_rounded_rect_radii(
        cr,
        bounds.x as f64,
        bounds.y as f64,
        bounds.width as f64,
        bounds.height as f64,
        radius,
    );
    cr.clip();
    cr.identity_matrix();
    cr.set_operator(cairo::Operator::Source);
    let _ = cr.set_source_surface(&snapshot, sx1 as f64, sy1 as f64);
    let _ = cr.paint();
    let _ = cr.restore();
}

fn skip_ws(chars: &mut std::iter::Peekable<std::str::Chars>) {
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() || c == ',' {
            chars.next();
        } else {
            break;
        }
    }
}

fn next_flag(chars: &mut std::iter::Peekable<std::str::Chars>) -> Option<bool> {
    skip_ws(chars);
    match chars.peek() {
        Some('0') => {
            chars.next();
            Some(false)
        }
        Some('1') => {
            chars.next();
            Some(true)
        }
        _ => None,
    }
}

fn next_number(chars: &mut std::iter::Peekable<std::str::Chars>) -> Option<f64> {
    skip_ws(chars);
    let mut buf = String::new();
    if let Some(&c) = chars.peek() {
        if c == '-' || c == '+' {
            buf.push(chars.next().unwrap());
        }
    }

    let mut has_digit = false;
    let mut has_dot = false;
    while let Some(&c) = chars.peek() {
        if c.is_ascii_digit() {
            has_digit = true;
            buf.push(chars.next().unwrap());
        } else if c == '.' && !has_dot {
            has_dot = true;
            buf.push(chars.next().unwrap());
        } else {
            break;
        }
    }

    if has_digit {
        if let Some(&c) = chars.peek() {
            if c == 'e' || c == 'E' {
                let e_char = chars.next().unwrap();
                let mut exp_buf = String::new();
                exp_buf.push(e_char);
                if let Some(&sign) = chars.peek() {
                    if sign == '+' || sign == '-' {
                        exp_buf.push(chars.next().unwrap());
                    }
                }
                let mut exp_has_digit = false;
                while let Some(&d) = chars.peek() {
                    if d.is_ascii_digit() {
                        exp_has_digit = true;
                        exp_buf.push(chars.next().unwrap());
                    } else {
                        break;
                    }
                }
                if exp_has_digit {
                    buf.push_str(&exp_buf);
                }
            }
        }
        buf.parse::<f64>().ok()
    } else {
        None
    }
}

fn arc_to_cairo(
    cr: &cairo::Context,
    x0: f64,
    y0: f64,
    mut rx: f64,
    mut ry: f64,
    x_axis_rotation_deg: f64,
    large_arc: bool,
    sweep: bool,
    x1: f64,
    y1: f64,
) {
    if (x0 - x1).abs() < 1e-6 && (y0 - y1).abs() < 1e-6 {
        return;
    }
    if rx.abs() < 1e-6 || ry.abs() < 1e-6 {
        cr.line_to(x1, y1);
        return;
    }

    rx = rx.abs();
    ry = ry.abs();
    let phi = x_axis_rotation_deg.to_radians();
    let cos_phi = phi.cos();
    let sin_phi = phi.sin();

    // Step 1: Compute (x1', y1')
    let dx = (x0 - x1) / 2.0;
    let dy = (y0 - y1) / 2.0;
    let x1_prime = cos_phi * dx + sin_phi * dy;
    let y1_prime = -sin_phi * dx + cos_phi * dy;

    // Check radii size
    let lambda = (x1_prime * x1_prime) / (rx * rx) + (y1_prime * y1_prime) / (ry * ry);
    if lambda > 1.0 {
        let sqrt_lambda = lambda.sqrt();
        rx *= sqrt_lambda;
        ry *= sqrt_lambda;
    }

    // Step 2: Compute (cx', cy')
    let rx_sq = rx * rx;
    let ry_sq = ry * ry;
    let x1_p_sq = x1_prime * x1_prime;
    let y1_p_sq = y1_prime * y1_prime;

    let numerator = (rx_sq * ry_sq - rx_sq * y1_p_sq - ry_sq * x1_p_sq).max(0.0);
    let denominator = rx_sq * y1_p_sq + ry_sq * x1_p_sq;
    let factor = if denominator > 1e-12 {
        let s = (numerator / denominator).sqrt();
        if large_arc == sweep {
            -s
        } else {
            s
        }
    } else {
        0.0
    };

    let cx_prime = factor * (rx * y1_prime / ry);
    let cy_prime = factor * (-ry * x1_prime / rx);

    // Step 3: Compute (cx, cy)
    let cx = cos_phi * cx_prime - sin_phi * cy_prime + (x0 + x1) / 2.0;
    let cy = sin_phi * cx_prime + cos_phi * cy_prime + (y0 + y1) / 2.0;

    // Step 4: Compute theta1 and d_theta
    fn angle(ux: f64, uy: f64, vx: f64, vy: f64) -> f64 {
        let dot = ux * vx + uy * vy;
        let len = (ux * ux + uy * uy).sqrt() * (vx * vx + vy * vy).sqrt();
        let cos_val = (dot / len.max(1e-12)).clamp(-1.0, 1.0);
        let ang = cos_val.acos();
        if ux * vy - uy * vx < 0.0 {
            -ang
        } else {
            ang
        }
    }

    let ux = (x1_prime - cx_prime) / rx;
    let uy = (y1_prime - cy_prime) / ry;
    let vx = (-x1_prime - cx_prime) / rx;
    let vy = (-y1_prime - cy_prime) / ry;

    let theta1 = angle(1.0, 0.0, ux, uy);
    let mut d_theta = angle(ux, uy, vx, vy);

    if !sweep && d_theta > 0.0 {
        d_theta -= 2.0 * std::f64::consts::PI;
    } else if sweep && d_theta < 0.0 {
        d_theta += 2.0 * std::f64::consts::PI;
    }

    // Step 5: Approximate arc by cubic Bezier segments (each spanning <= pi/2)
    let num_segments = (d_theta.abs() / std::f64::consts::FRAC_PI_2)
        .ceil()
        .max(1.0) as usize;
    let delta = d_theta / num_segments as f64;

    let mut current_theta = theta1;
    for _ in 0..num_segments {
        let next_theta = current_theta + delta;
        let alpha = (4.0 / 3.0) * (delta / 4.0).tan();

        let cos1 = current_theta.cos();
        let sin1 = current_theta.sin();
        let cos2 = next_theta.cos();
        let sin2 = next_theta.sin();

        let p1x = rx * cos1;
        let p1y = ry * sin1;
        let p2x = rx * cos2;
        let p2y = ry * sin2;

        let cp1x = p1x - alpha * rx * sin1;
        let cp1y = p1y + alpha * ry * cos1;
        let cp2x = p2x + alpha * rx * sin2;
        let cp2y = p2y - alpha * ry * cos2;

        let transform = |px: f64, py: f64| -> (f64, f64) {
            (
                cos_phi * px - sin_phi * py + cx,
                sin_phi * px + cos_phi * py + cy,
            )
        };

        let (tcp1x, tcp1y) = transform(cp1x, cp1y);
        let (tcp2x, tcp2y) = transform(cp2x, cp2y);
        let (tp2x, tp2y) = transform(p2x, p2y);

        cr.curve_to(tcp1x, tcp1y, tcp2x, tcp2y, tp2x, tp2y);
        current_theta = next_theta;
    }
}

/// Parses and executes SVG path data into Cairo path commands.
pub fn cairo_interpret_svg_path(cr: &cairo::Context, path_data: &str) {
    let mut chars = path_data.chars().peekable();
    let mut current_x = 0.0f64;
    let mut current_y = 0.0f64;
    let mut subpath_start_x = 0.0f64;
    let mut subpath_start_y = 0.0f64;
    let mut last_cp2_x = 0.0f64;
    let mut last_cp2_y = 0.0f64;
    let mut last_cmd_was_cubic = false;
    let mut last_qp_x = 0.0f64;
    let mut last_qp_y = 0.0f64;
    let mut last_cmd_was_quad = false;

    while let Some(&ch) = chars.peek() {
        if ch.is_whitespace() || ch == ',' {
            chars.next();
            continue;
        }

        if ch.is_ascii_alphabetic() {
            let cmd = chars.next().unwrap();
            match cmd {
                'M' | 'm' => {
                    let is_rel = cmd == 'm';
                    if let (Some(x), Some(y)) = (next_number(&mut chars), next_number(&mut chars)) {
                        current_x = if is_rel { current_x + x } else { x };
                        current_y = if is_rel { current_y + y } else { y };
                        subpath_start_x = current_x;
                        subpath_start_y = current_y;
                        cr.move_to(current_x, current_y);
                    }
                    last_cmd_was_cubic = false;
                    last_cmd_was_quad = false;

                    while let (Some(x), Some(y)) =
                        (next_number(&mut chars), next_number(&mut chars))
                    {
                        current_x = if is_rel { current_x + x } else { x };
                        current_y = if is_rel { current_y + y } else { y };
                        cr.line_to(current_x, current_y);
                    }
                }
                'L' | 'l' => {
                    let is_rel = cmd == 'l';
                    while let (Some(x), Some(y)) =
                        (next_number(&mut chars), next_number(&mut chars))
                    {
                        current_x = if is_rel { current_x + x } else { x };
                        current_y = if is_rel { current_y + y } else { y };
                        cr.line_to(current_x, current_y);
                    }
                    last_cmd_was_cubic = false;
                    last_cmd_was_quad = false;
                }
                'H' | 'h' => {
                    let is_rel = cmd == 'h';
                    while let Some(x) = next_number(&mut chars) {
                        current_x = if is_rel { current_x + x } else { x };
                        cr.line_to(current_x, current_y);
                    }
                    last_cmd_was_cubic = false;
                    last_cmd_was_quad = false;
                }
                'V' | 'v' => {
                    let is_rel = cmd == 'v';
                    while let Some(y) = next_number(&mut chars) {
                        current_y = if is_rel { current_y + y } else { y };
                        cr.line_to(current_x, current_y);
                    }
                    last_cmd_was_cubic = false;
                    last_cmd_was_quad = false;
                }
                'C' | 'c' => {
                    let is_rel = cmd == 'c';
                    while let (Some(x1), Some(y1), Some(x2), Some(y2), Some(x), Some(y)) = (
                        next_number(&mut chars),
                        next_number(&mut chars),
                        next_number(&mut chars),
                        next_number(&mut chars),
                        next_number(&mut chars),
                        next_number(&mut chars),
                    ) {
                        let (cx1, cy1) = if is_rel {
                            (current_x + x1, current_y + y1)
                        } else {
                            (x1, y1)
                        };
                        let (cx2, cy2) = if is_rel {
                            (current_x + x2, current_y + y2)
                        } else {
                            (x2, y2)
                        };
                        let (cx, cy) = if is_rel {
                            (current_x + x, current_y + y)
                        } else {
                            (x, y)
                        };
                        cr.curve_to(cx1, cy1, cx2, cy2, cx, cy);
                        last_cp2_x = cx2;
                        last_cp2_y = cy2;
                        current_x = cx;
                        current_y = cy;
                        last_cmd_was_cubic = true;
                        last_cmd_was_quad = false;
                    }
                }
                'S' | 's' => {
                    let is_rel = cmd == 's';
                    while let (Some(x2), Some(y2), Some(x), Some(y)) = (
                        next_number(&mut chars),
                        next_number(&mut chars),
                        next_number(&mut chars),
                        next_number(&mut chars),
                    ) {
                        let (cx2, cy2) = if is_rel {
                            (current_x + x2, current_y + y2)
                        } else {
                            (x2, y2)
                        };
                        let (cx, cy) = if is_rel {
                            (current_x + x, current_y + y)
                        } else {
                            (x, y)
                        };
                        let (cx1, cy1) = if last_cmd_was_cubic {
                            (2.0 * current_x - last_cp2_x, 2.0 * current_y - last_cp2_y)
                        } else {
                            (current_x, current_y)
                        };
                        cr.curve_to(cx1, cy1, cx2, cy2, cx, cy);
                        last_cp2_x = cx2;
                        last_cp2_y = cy2;
                        current_x = cx;
                        current_y = cy;
                        last_cmd_was_cubic = true;
                        last_cmd_was_quad = false;
                    }
                }
                'Q' | 'q' => {
                    let is_rel = cmd == 'q';
                    while let (Some(x1), Some(y1), Some(x), Some(y)) = (
                        next_number(&mut chars),
                        next_number(&mut chars),
                        next_number(&mut chars),
                        next_number(&mut chars),
                    ) {
                        let (qx, qy) = if is_rel {
                            (current_x + x1, current_y + y1)
                        } else {
                            (x1, y1)
                        };
                        let (cx, cy) = if is_rel {
                            (current_x + x, current_y + y)
                        } else {
                            (x, y)
                        };
                        let cx1 = current_x + (2.0 / 3.0) * (qx - current_x);
                        let cy1 = current_y + (2.0 / 3.0) * (qy - current_y);
                        let cx2 = cx + (2.0 / 3.0) * (qx - cx);
                        let cy2 = cy + (2.0 / 3.0) * (qy - cy);
                        cr.curve_to(cx1, cy1, cx2, cy2, cx, cy);
                        last_qp_x = qx;
                        last_qp_y = qy;
                        current_x = cx;
                        current_y = cy;
                        last_cmd_was_cubic = false;
                        last_cmd_was_quad = true;
                    }
                }
                'T' | 't' => {
                    let is_rel = cmd == 't';
                    while let (Some(x), Some(y)) =
                        (next_number(&mut chars), next_number(&mut chars))
                    {
                        let (cx, cy) = if is_rel {
                            (current_x + x, current_y + y)
                        } else {
                            (x, y)
                        };
                        let (qx, qy) = if last_cmd_was_quad {
                            (2.0 * current_x - last_qp_x, 2.0 * current_y - last_qp_y)
                        } else {
                            (current_x, current_y)
                        };
                        let cx1 = current_x + (2.0 / 3.0) * (qx - current_x);
                        let cy1 = current_y + (2.0 / 3.0) * (qy - current_y);
                        let cx2 = cx + (2.0 / 3.0) * (qx - cx);
                        let cy2 = cy + (2.0 / 3.0) * (qy - cy);
                        cr.curve_to(cx1, cy1, cx2, cy2, cx, cy);
                        last_qp_x = qx;
                        last_qp_y = qy;
                        current_x = cx;
                        current_y = cy;
                        last_cmd_was_cubic = false;
                        last_cmd_was_quad = true;
                    }
                }
                'A' | 'a' => {
                    let is_rel = cmd == 'a';
                    while let Some(rx) = next_number(&mut chars) {
                        let Some(ry) = next_number(&mut chars) else {
                            break;
                        };
                        let Some(x_rot) = next_number(&mut chars) else {
                            break;
                        };
                        let Some(large_arc) = next_flag(&mut chars) else {
                            break;
                        };
                        let Some(sweep) = next_flag(&mut chars) else {
                            break;
                        };
                        let Some(x) = next_number(&mut chars) else {
                            break;
                        };
                        let Some(y) = next_number(&mut chars) else {
                            break;
                        };

                        let target_x = if is_rel { current_x + x } else { x };
                        let target_y = if is_rel { current_y + y } else { y };

                        arc_to_cairo(
                            cr, current_x, current_y, rx, ry, x_rot, large_arc, sweep, target_x,
                            target_y,
                        );
                        current_x = target_x;
                        current_y = target_y;
                        last_cmd_was_cubic = false;
                        last_cmd_was_quad = false;
                    }
                }
                'Z' | 'z' => {
                    cr.close_path();
                    current_x = subpath_start_x;
                    current_y = subpath_start_y;
                    last_cmd_was_cubic = false;
                    last_cmd_was_quad = false;
                }
                _ => {}
            }
        } else {
            chars.next();
        }
    }
}

/// Maps Strata's W3C BlendMode to the corresponding Cairo compositing operator.
pub fn blend_mode_to_cairo_operator(mode: strata_style::types::BlendMode) -> cairo::Operator {
    match mode {
        strata_style::types::BlendMode::Normal => cairo::Operator::Over,
        strata_style::types::BlendMode::Multiply => cairo::Operator::Multiply,
        strata_style::types::BlendMode::Screen => cairo::Operator::Screen,
        strata_style::types::BlendMode::Overlay => cairo::Operator::Overlay,
        strata_style::types::BlendMode::Darken => cairo::Operator::Darken,
        strata_style::types::BlendMode::Lighten => cairo::Operator::Lighten,
        strata_style::types::BlendMode::ColorDodge => cairo::Operator::ColorDodge,
        strata_style::types::BlendMode::ColorBurn => cairo::Operator::ColorBurn,
        strata_style::types::BlendMode::HardLight => cairo::Operator::HardLight,
        strata_style::types::BlendMode::SoftLight => cairo::Operator::SoftLight,
        strata_style::types::BlendMode::Difference => cairo::Operator::Difference,
        strata_style::types::BlendMode::Exclusion => cairo::Operator::Exclusion,
        strata_style::types::BlendMode::Hue => cairo::Operator::HslHue,
        strata_style::types::BlendMode::Saturation => cairo::Operator::HslSaturation,
        strata_style::types::BlendMode::Color => cairo::Operator::HslColor,
        strata_style::types::BlendMode::Luminosity => cairo::Operator::HslLuminosity,
    }
}

///// One rasterized 3D viewport frame, keyed by scene identity, retained generations, and pixel size.
struct Viewport3DCacheEntry {
    /// Held so the allocation, and therefore the pointer identity, stays valid.
    scene: std::sync::Arc<strata_render::scene3d::Scene3D>,
    generations: (u64, u64, u64),
    width: usize,
    height: usize,
    is_half_res: bool,
    surface: cairo::ImageSurface,
}

#[inline]
fn scene_retained_generations(scene: &strata_render::scene3d::Scene3D) -> (u64, u64, u64) {
    scene
        .retained_handle()
        .map(|h| h.generations())
        .unwrap_or_default()
}

struct Viewport3DSurfaceResult {
    surface: cairo::ImageSurface,
    src_width: usize,
    src_height: usize,
    needs_debounced_full_res: bool,
}

thread_local! {
    static VIEWPORT_3D_CACHE: std::cell::RefCell<Vec<Viewport3DCacheEntry>> =
        const { std::cell::RefCell::new(Vec::new()) };
    static VIEWPORT_3D_SCRATCH_FB: std::cell::RefCell<Option<strata_render::rasterizer::FrameBuffer>> =
        const { std::cell::RefCell::new(None) };
    static VIEWPORT_3D_LAST_RASTER_AT: std::cell::Cell<Option<std::time::Instant>> =
        const { std::cell::Cell::new(None) };
    static VIEWPORT_3D_RESIZE_DEBOUNCE: std::cell::Cell<Option<(usize, usize, std::time::Instant)>> =
        const { std::cell::Cell::new(None) };
    static VIEWPORT_3D_DEBOUNCE_SCHEDULED: std::cell::Cell<bool> =
        const { std::cell::Cell::new(false) };
    static IN_LIVE_GTK_DRAW: std::cell::Cell<bool> =
        const { std::cell::Cell::new(false) };
    static CURRENT_DRAWING_AREA: std::cell::RefCell<Option<gtk::DrawingArea>> =
        const { std::cell::RefCell::new(None) };
}

/// Maximum number of rasterized viewport frames kept alive.
const VIEWPORT_3D_CACHE_CAPACITY: usize = 4;
const VIEWPORT_3D_RESIZE_DEBOUNCE_MS: u64 = 80;
const VIEWPORT_3D_INTERACTIVE_WINDOW_MS: u64 = 45;

fn schedule_viewport_3d_debounced_redraw() {
    let already = VIEWPORT_3D_DEBOUNCE_SCHEDULED.with(|s| {
        if s.get() {
            true
        } else {
            s.set(true);
            false
        }
    });
    if already {
        return;
    }
    gtk::glib::timeout_add_local_once(
        std::time::Duration::from_millis(VIEWPORT_3D_RESIZE_DEBOUNCE_MS + 5),
        move || {
            VIEWPORT_3D_DEBOUNCE_SCHEDULED.with(|s| s.set(false));
            CURRENT_DRAWING_AREA.with(|da| {
                if let Some(widget) = da.borrow().as_ref() {
                    use gtk::prelude::WidgetExt;
                    widget.queue_draw();
                }
            });
        },
    );
}

fn rasterize_scene_3d_to_cairo_surface(
    scene: &std::sync::Arc<strata_render::scene3d::Scene3D>,
    w: usize,
    h: usize,
) -> Option<cairo::ImageSurface> {
    let mut surface =
        cairo::ImageSurface::create(cairo::Format::ARgb32, w as i32, h as i32).ok()?;
    #[cfg(feature = "wgpu-backend")]
    if scene.retained_handle().is_some() || scene.retained_terrain().is_some() {
        if let Some(bgra) =
            crate::wgpu_surface::render_scene_3d_wgpu_bgra(scene, w as u32, h as u32)
        {
            let copied = if let Ok(mut data) = surface.data() {
                let copy_len = data.len().min(bgra.len());
                data[..copy_len].copy_from_slice(&bgra[..copy_len]);
                true
            } else {
                false
            };
            if copied {
                return Some(surface);
            }
        }
    }
    VIEWPORT_3D_SCRATCH_FB.with(|scratch| {
        let mut scratch = scratch.borrow_mut();
        let fb = match scratch.as_mut() {
            Some(fb) if fb.width == w && fb.height == h => {
                fb.pixels.fill(0);
                fb
            }
            _ => {
                *scratch = Some(strata_render::rasterizer::FrameBuffer::new(w, h));
                scratch.as_mut().expect("just initialized")
            }
        };
        strata_render::rasterizer3d::render_scene_3d_with_textures(
            fb,
            strata_layout::Rect::new(0.0, 0.0, w as f32, h as f32),
            scene,
            None,
            scene.textures_deref(),
        );

        let mut data = surface.data().ok()?;
        for (src, dst) in fb.pixels.chunks_exact(4).zip(data.chunks_exact_mut(4)) {
            let r = src[0] as u32;
            let g = src[1] as u32;
            let b = src[2] as u32;
            let a = src[3] as u32;
            if a == 255 {
                dst[0] = b as u8;
                dst[1] = g as u8;
                dst[2] = r as u8;
                dst[3] = 255;
            } else if a == 0 {
                dst[0] = 0;
                dst[1] = 0;
                dst[2] = 0;
                dst[3] = 0;
            } else {
                dst[0] = ((b * a + 127) / 255) as u8;
                dst[1] = ((g * a + 127) / 255) as u8;
                dst[2] = ((r * a + 127) / 255) as u8;
                dst[3] = a as u8;
            }
        }
        Some(())
    })?;
    Some(surface)
}

/// Returns the cairo surface for a 3D viewport, software-rasterizing it only
/// when this exact `Arc<Scene3D>` has not been drawn at this size before.
///
/// During interactive resize or rapid camera pan/zoom in live GTK windows, returns a fast
/// scaled cached surface or half-res rasterization and schedules a debounced full-res redraw.
fn viewport_3d_surface_with_fallback(
    scene: &std::sync::Arc<strata_render::scene3d::Scene3D>,
    w: usize,
    h: usize,
    allow_interactive_fallback: bool,
) -> Option<Viewport3DSurfaceResult> {
    let now = std::time::Instant::now();
    let gens = scene_retained_generations(scene);
    let is_gpu_retained = scene.retained_handle().is_some() || scene.retained_terrain().is_some();

    // 1. Exact full-res cache hit.
    let exact_hit = VIEWPORT_3D_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let idx = cache.iter().position(|e| {
            std::sync::Arc::ptr_eq(&e.scene, scene)
                && e.generations == gens
                && e.width == w
                && e.height == h
                && !e.is_half_res
        })?;
        let entry = cache.remove(idx);
        let surface = entry.surface.clone();
        cache.push(entry);
        Some(Viewport3DSurfaceResult {
            surface,
            src_width: w,
            src_height: h,
            needs_debounced_full_res: false,
        })
    });
    if exact_hit.is_some() {
        VIEWPORT_3D_RESIZE_DEBOUNCE.with(|d| d.set(None));
        return exact_hit;
    }

    // 2. Interactive resize fallback: same Arc<Scene3D> + generations at a different cached size.
    if allow_interactive_fallback {
        let debounce_elapsed = VIEWPORT_3D_RESIZE_DEBOUNCE.with(|d| {
            let cur = d.get();
            match cur {
                Some((dw, dh, deadline)) if dw == w && dh == h => {
                    if now >= deadline {
                        d.set(None);
                        true
                    } else {
                        false
                    }
                }
                _ => {
                    d.set(Some((
                        w,
                        h,
                        now + std::time::Duration::from_millis(VIEWPORT_3D_RESIZE_DEBOUNCE_MS),
                    )));
                    false
                }
            }
        });
        if !debounce_elapsed {
            let resize_hit = VIEWPORT_3D_CACHE.with(|cache| {
                let cache = cache.borrow();
                cache
                    .iter()
                    .rev()
                    .find(|e| std::sync::Arc::ptr_eq(&e.scene, scene) && e.generations == gens)
                    .map(|e| Viewport3DSurfaceResult {
                        surface: e.surface.clone(),
                        src_width: e.width,
                        src_height: e.height,
                        needs_debounced_full_res: true,
                    })
            });
            if resize_hit.is_some() {
                return resize_hit;
            }
        }
    }

    // 3. Interactive camera pan/zoom half-res fallback when software rasterizing rapidly.
    let use_half_res = allow_interactive_fallback
        && !is_gpu_retained
        && w >= 128
        && h >= 128
        && VIEWPORT_3D_LAST_RASTER_AT.with(|last| {
            last.get()
                .map(|t| {
                    now.duration_since(t).as_millis() < VIEWPORT_3D_INTERACTIVE_WINDOW_MS as u128
                })
                .unwrap_or(false)
        });

    let (raster_w, raster_h) = if use_half_res {
        ((w / 2).max(1), (h / 2).max(1))
    } else {
        (w, h)
    };

    let surface = rasterize_scene_3d_to_cairo_surface(scene, raster_w, raster_h)?;
    VIEWPORT_3D_LAST_RASTER_AT.with(|last| last.set(Some(now)));

    VIEWPORT_3D_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() >= VIEWPORT_3D_CACHE_CAPACITY {
            cache.remove(0);
        }
        cache.push(Viewport3DCacheEntry {
            scene: std::sync::Arc::clone(scene),
            generations: gens,
            width: raster_w,
            height: raster_h,
            is_half_res: use_half_res,
            surface: surface.clone(),
        });
    });

    Some(Viewport3DSurfaceResult {
        surface,
        src_width: raster_w,
        src_height: raster_h,
        needs_debounced_full_res: use_half_res,
    })
}

#[allow(dead_code)]
fn viewport_3d_surface(
    scene: &std::sync::Arc<strata_render::scene3d::Scene3D>,
    w: usize,
    h: usize,
) -> Option<cairo::ImageSurface> {
    viewport_3d_surface_with_fallback(scene, w, h, false).map(|res| res.surface)
}

/// Executes a Strata DisplayList into a Cairo graphics context with full typography and vector support.
pub fn render_display_list_to_cairo(cr: &cairo::Context, list: &DisplayList) {
    render_display_list_to_cairo_impl(cr, list, false);
}

fn compute_opacity_group_clip(rest: &[DrawCommand]) -> Option<(f64, f64, f64, f64)> {
    let mut depth = 1usize;
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    for cmd in rest {
        match cmd {
            DrawCommand::PushOpacity { .. } => depth += 1,
            DrawCommand::PopOpacity => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            DrawCommand::PushTransform { .. } | DrawCommand::DrawViewport3D { .. } => {
                return None;
            }
            _ => {
                if let Some((bx1, by1, bx2, by2)) = command_paint_rect(cmd) {
                    if bx2 > bx1 && by2 > by1 {
                        min_x = min_x.min(bx1 as f32);
                        min_y = min_y.min(by1 as f32);
                        max_x = max_x.max(bx2 as f32);
                        max_y = max_y.max(by2 as f32);
                    }
                }
            }
        }
    }
    if min_x < max_x && min_y < max_y {
        let pad = 8.0f64;
        let x = (min_x as f64 - pad).floor();
        let y = (min_y as f64 - pad).floor();
        let w = ((max_x as f64 + pad).ceil() - x).max(1.0);
        let h = ((max_y as f64 + pad).ceil() - y).max(1.0);
        Some((x, y, w, h))
    } else {
        None
    }
}

#[inline]
fn rect_overlaps_dirty_tiles(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    bits: &[u64],
    cols: usize,
    rows: usize,
) -> bool {
    if cols == 0 || rows == 0 || x2 <= 0.0 || y2 <= 0.0 || x2 <= x1 || y2 <= y1 {
        return false;
    }
    let c0 = ((x1.floor().max(0.0) as usize) >> 6).min(cols - 1);
    let r0 = ((y1.floor().max(0.0) as usize) >> 6).min(rows - 1);
    let c1 = (((x2.ceil().max(1.0) as usize) - 1) >> 6).min(cols - 1);
    let r1 = (((y2.ceil().max(1.0) as usize) - 1) >> 6).min(rows - 1);
    if c0 > c1 || r0 > r1 {
        return false;
    }
    for r in r0..=r1 {
        let row_base = r * cols;
        for c in c0..=c1 {
            let idx = row_base + c;
            if (bits[idx >> 6] & (1u64 << (idx & 63))) != 0 {
                return true;
            }
        }
    }
    false
}

#[inline]
fn command_paint_rect(cmd: &DrawCommand) -> Option<(f64, f64, f64, f64)> {
    match cmd {
        DrawCommand::DrawShadow { bounds, shadow, .. } => {
            let ext = (shadow.blur_radius.max(0.0) * 2.5 + shadow.spread_radius.max(0.0) + 12.0)
                as f64;
            let sx1 = bounds.x as f64 + shadow.offset_x.min(0.0) as f64 - ext;
            let sy1 = bounds.y as f64 + shadow.offset_y.min(0.0) as f64 - ext;
            let sx2 = (bounds.x + bounds.width) as f64 + shadow.offset_x.max(0.0) as f64 + ext;
            let sy2 = (bounds.y + bounds.height) as f64 + shadow.offset_y.max(0.0) as f64 + ext;
            Some((sx1, sy1, sx2, sy2))
        }
        DrawCommand::DrawText {
            origin,
            bounds_width,
            bounds_height,
            text,
            font_size,
            line_height,
            ..
        } => {
            let w = bounds_width
                .unwrap_or(0.0)
                .max(text.len() as f32 * (*font_size) * 0.95) as f64;
            let h = bounds_height
                .unwrap_or(0.0)
                .max(line_height.max(*font_size) * 1.5) as f64;
            let pad = 16.0f64;
            Some((
                origin.0 as f64 - pad,
                origin.1 as f64 - pad,
                origin.0 as f64 + w + pad,
                origin.1 as f64 + h + pad,
            ))
        }
        _ => cmd.bounds().map(|b| {
            let pad = 12.0f64;
            (
                b.x as f64 - pad,
                b.y as f64 - pad,
                (b.x + b.width) as f64 + pad,
                (b.y + b.height) as f64 + pad,
            )
        }),
    }
}

fn render_display_list_to_cairo_impl(
    cr: &cairo::Context,
    list: &DisplayList,
    punch_viewport_hole: bool,
) {
    render_display_list_to_cairo_with_dirty_tiles(cr, list, punch_viewport_hole, None);
}

fn render_display_list_to_cairo_with_dirty_tiles(
    cr: &cairo::Context,
    list: &DisplayList,
    punch_viewport_hole: bool,
    dirty_tiles: Option<(&[u64], usize, usize)>,
) {
    let perf_start = std::time::Instant::now();
    let mut current_clip = cr
        .clip_extents()
        .unwrap_or((0.0, 0.0, 100_000.0, 100_000.0));
    let mut operator_stack: Vec<cairo::Operator> = Vec::new();
    let mut opacity_stack: Vec<Option<f64>> = Vec::new();
    let mut shared_layout: Option<pango::Layout> = None;
    let cmds = list.commands();

    for (cmd_idx, cmd) in cmds.iter().enumerate() {
        if !cmd.is_clip() && !cmd.is_state_command() {
            let (cx1, cy1, cx2, cy2) = current_clip;
            if let Some((bx1, by1, bx2, by2)) = command_paint_rect(cmd) {
                if bx2 < cx1 || bx1 > cx2 || by2 < cy1 || by1 > cy2 {
                    continue;
                }
                if let Some((bits, cols, rows)) = dirty_tiles {
                    if !rect_overlaps_dirty_tiles(bx1, by1, bx2, by2, bits, cols, rows) {
                        continue;
                    }
                }
            }
        }
        match cmd {
            DrawCommand::DrawRect { bounds, color } => {
                cr.set_source_rgba(
                    color.r() as f64,
                    color.g() as f64,
                    color.b() as f64,
                    color.a() as f64,
                );
                cr.rectangle(
                    bounds.x as f64,
                    bounds.y as f64,
                    bounds.width as f64,
                    bounds.height as f64,
                );
                let _ = cr.fill();
            }
            DrawCommand::DrawRoundedRect {
                bounds,
                color,
                border_radius,
                border_width,
                border_color,
            } => {
                cairo_rounded_rect_radii(
                    cr,
                    bounds.x as f64,
                    bounds.y as f64,
                    bounds.width as f64,
                    bounds.height as f64,
                    *border_radius,
                );
                cr.set_source_rgba(
                    color.r() as f64,
                    color.g() as f64,
                    color.b() as f64,
                    color.a() as f64,
                );
                let _ = cr.fill();

                if let Some(bc) = border_color {
                    let w = border_width.top as f64;
                    if w > 0.0 {
                        let half_w = w / 2.0;
                        let r = inset_border_radius(*border_radius, half_w as f32);
                        cairo_rounded_rect_radii(
                            cr,
                            bounds.x as f64 + half_w,
                            bounds.y as f64 + half_w,
                            bounds.width as f64 - w,
                            bounds.height as f64 - w,
                            r,
                        );
                        cr.set_source_rgba(
                            bc.r() as f64,
                            bc.g() as f64,
                            bc.b() as f64,
                            bc.a() as f64,
                        );
                        cr.set_line_width(w);
                        let _ = cr.stroke();
                    }
                }
            }
            DrawCommand::DrawShadow {
                bounds,
                shadow,
                border_radius,
            } => {
                cairo_draw_shadow(cr, bounds, shadow, *border_radius);
            }
            DrawCommand::DrawInsetShadow {
                bounds,
                shadow,
                border_radius,
            } => {
                cairo_draw_inset_shadow(cr, bounds, shadow, *border_radius);
            }
            DrawCommand::DrawLinearGradient {
                bounds,
                angle_deg,
                stops,
                border_radius,
            } => {
                cairo_draw_linear_gradient(cr, bounds, *angle_deg, stops, *border_radius);
            }
            DrawCommand::DrawRadialGradient {
                bounds,
                center,
                stops,
                border_radius,
            } => {
                cairo_draw_radial_gradient(cr, bounds, *center, stops, *border_radius);
            }
            DrawCommand::PushBackdropFilter {
                bounds,
                radius,
                filter,
            } => {
                if !punch_viewport_hole {
                    cairo_apply_backdrop_filter(cr, bounds, *radius, filter);
                }
            }
            DrawCommand::PopBackdropFilter => {}
            DrawCommand::DrawBorder {
                bounds,
                widths,
                color,
                border_radius,
                style,
            } => {
                cr.set_source_rgba(
                    color.r() as f64,
                    color.g() as f64,
                    color.b() as f64,
                    color.a() as f64,
                );

                let is_uniform = (widths.top - widths.bottom).abs() < 1e-4
                    && (widths.top - widths.left).abs() < 1e-4
                    && (widths.top - widths.right).abs() < 1e-4;

                if is_uniform && widths.top > 0.0 {
                    stroke_uniform_border(
                        cr,
                        bounds.x as f64,
                        bounds.y as f64,
                        bounds.width as f64,
                        bounds.height as f64,
                        *border_radius,
                        widths.top as f64,
                        *style,
                    );
                } else {
                    if widths.top > 0.0 {
                        stroke_border_side(
                            cr,
                            *style,
                            widths.top as f64,
                            bounds.x as f64,
                            bounds.y as f64 + widths.top as f64 / 2.0,
                            (bounds.x + bounds.width) as f64,
                            bounds.y as f64 + widths.top as f64 / 2.0,
                        );
                    }
                    if widths.bottom > 0.0 {
                        stroke_border_side(
                            cr,
                            *style,
                            widths.bottom as f64,
                            bounds.x as f64,
                            (bounds.y + bounds.height) as f64 - widths.bottom as f64 / 2.0,
                            (bounds.x + bounds.width) as f64,
                            (bounds.y + bounds.height) as f64 - widths.bottom as f64 / 2.0,
                        );
                    }
                    if widths.left > 0.0 {
                        stroke_border_side(
                            cr,
                            *style,
                            widths.left as f64,
                            bounds.x as f64 + widths.left as f64 / 2.0,
                            bounds.y as f64,
                            bounds.x as f64 + widths.left as f64 / 2.0,
                            (bounds.y + bounds.height) as f64,
                        );
                    }
                    if widths.right > 0.0 {
                        stroke_border_side(
                            cr,
                            *style,
                            widths.right as f64,
                            (bounds.x + bounds.width) as f64 - widths.right as f64 / 2.0,
                            bounds.y as f64,
                            (bounds.x + bounds.width) as f64 - widths.right as f64 / 2.0,
                            (bounds.y + bounds.height) as f64,
                        );
                    }
                }
            }
            DrawCommand::DrawText {
                origin,
                bounds_width,
                bounds_height,
                text_align,
                text,
                font_size,
                line_height,
                font_weight,
                color,
                is_monospace,
                font_family,
            } => {
                let layout = shared_layout.get_or_insert_with(|| create_parity_layout(cr));
                layout.set_width(-1);
                layout.set_wrap(pango::WrapMode::Word);
                layout.set_alignment(pango::Alignment::Left);
                layout.set_justify(false);
                let mut font_desc = pango::FontDescription::new();
                let family = resolved_font_family(*font_family, *is_monospace);
                font_desc.set_family(&family);
                font_desc.set_absolute_size(*font_size as f64 * pango::SCALE as f64);
                font_desc.set_weight(pango_weight(*font_weight));
                layout.set_font_description(Some(&font_desc));
                layout.set_text(text);

                if let Some(w) = bounds_width {
                    let (unwrapped_w, _) = layout.size();
                    let unwrapped_px = unwrapped_w as f32 / pango::SCALE as f32;
                    if unwrapped_px > *w * 1.12 {
                        layout.set_width((*w * pango::SCALE as f32) as i32);
                        layout.set_wrap(pango::WrapMode::WordChar);
                    } else if *text_align != strata_style::TextAlign::Left {
                        layout.set_width((*w * pango::SCALE as f32) as i32);
                    }
                    match text_align {
                        strata_style::TextAlign::Left => {
                            layout.set_alignment(pango::Alignment::Left)
                        }
                        strata_style::TextAlign::Center => {
                            layout.set_alignment(pango::Alignment::Center)
                        }
                        strata_style::TextAlign::Right => {
                            layout.set_alignment(pango::Alignment::Right)
                        }
                        strata_style::TextAlign::Justify => layout.set_justify(true),
                    }
                }

                cr.set_source_rgba(
                    color.r() as f64,
                    color.g() as f64,
                    color.b() as f64,
                    color.a() as f64,
                );
                let (_, pango_h) = layout.size();
                let pango_h_px = pango_h as f64 / pango::SCALE as f64;
                let target_h = bounds_height
                    .unwrap_or(*font_size * *line_height)
                    .max(*font_size * *line_height) as f64;
                let vert_offset = ((target_h - pango_h_px) / 2.0).max(0.0);
                cr.move_to(origin.0 as f64, origin.1 as f64 + vert_offset);
                pangocairo::functions::show_layout(cr, layout);
            }
            DrawCommand::DrawTextFragments { fragments } => {
                let clip_opt = cr.clip_extents().ok();
                let layout = shared_layout.get_or_insert_with(|| create_parity_layout(cr));
                layout.set_width(-1);
                layout.set_wrap(pango::WrapMode::Word);
                layout.set_alignment(pango::Alignment::Left);
                layout.set_justify(false);
                for frag in fragments {
                    if frag.text.is_empty() && frag.style.background_color.is_none() {
                        continue;
                    }

                    let fx1 = (frag.bounds.x - 12.0) as f64;
                    let fy1 = (frag.bounds.y - 12.0) as f64;
                    let fx2 = (frag.bounds.x + frag.bounds.width + 12.0) as f64;
                    let fy2 = (frag.bounds.y + frag.bounds.height + 12.0) as f64;
                    if let Some((cx1, cy1, cx2, cy2)) = clip_opt {
                        if fx2 < cx1 || fx1 > cx2 || fy2 < cy1 || fy1 > cy2 {
                            continue;
                        }
                    }
                    if let Some((bits, cols, rows)) = dirty_tiles {
                        if !rect_overlaps_dirty_tiles(fx1, fy1, fx2, fy2, bits, cols, rows) {
                            continue;
                        }
                    }

                    // 1. Span background rectangle (e.g. <code> badge)
                    if let Some(bg_color) = frag.style.background_color {
                        cr.set_source_rgba(
                            bg_color.r() as f64,
                            bg_color.g() as f64,
                            bg_color.b() as f64,
                            bg_color.a() as f64,
                        );
                        let bg_x = frag.bounds.x as f64 - 2.0;
                        let bg_y = frag.bounds.y as f64 - 1.0;
                        let bg_w = frag.bounds.width as f64 + 4.0;
                        let bg_h = frag.bounds.height as f64 + 2.0;
                        cairo_rounded_rect(cr, bg_x, bg_y, bg_w, bg_h, 3.0);
                        let _ = cr.fill();
                    }

                    // 2. Fragment text rendering with exact font weight and style
                    if !frag.text.is_empty() {
                        let mut font_desc = pango::FontDescription::new();
                        let family =
                            resolved_font_family(frag.font_family, frag.style.is_monospace);
                        font_desc.set_family(&family);
                        font_desc
                            .set_absolute_size(frag.style.font_size as f64 * pango::SCALE as f64);

                        font_desc.set_weight(pango_weight(frag.style.font_weight));

                        if frag.style.font_style == FontStyle::Italic {
                            font_desc.set_style(pango::Style::Italic);
                        } else if frag.style.font_style == FontStyle::Oblique {
                            font_desc.set_style(pango::Style::Oblique);
                        }

                        layout.set_font_description(Some(&font_desc));
                        layout.set_text(&frag.text);

                        cr.set_source_rgba(
                            frag.style.color.r() as f64,
                            frag.style.color.g() as f64,
                            frag.style.color.b() as f64,
                            frag.style.color.a() as f64,
                        );

                        cr.move_to(frag.bounds.x as f64, frag.bounds.y as f64);
                        pangocairo::functions::show_layout(cr, layout);

                        // 3. Text decorations (Underline, LineThrough)
                        let pango_baseline = {
                            let bl = layout.baseline() as f64 / pango::SCALE as f64;
                            if bl > 0.0 {
                                bl
                            } else {
                                frag.style.font_size as f64 * 0.82
                            }
                        };
                        match frag.style.text_decoration {
                            TextDecoration::Underline => {
                                cr.set_line_width(1.0);
                                let underline_y =
                                    (frag.bounds.y as f64 + pango_baseline + 1.5).round();
                                cr.move_to(frag.bounds.x as f64, underline_y);
                                cr.line_to((frag.bounds.x + frag.bounds.width) as f64, underline_y);
                                let _ = cr.stroke();
                            }
                            TextDecoration::LineThrough => {
                                cr.set_line_width(1.0);
                                let strike_y = (frag.bounds.y as f64 + pango_baseline
                                    - (frag.style.font_size as f64 * 0.28))
                                    .round();
                                cr.move_to(frag.bounds.x as f64, strike_y);
                                cr.line_to((frag.bounds.x + frag.bounds.width) as f64, strike_y);
                                let _ = cr.stroke();
                            }
                            _ => {}
                        }
                    }
                }
            }
            DrawCommand::DrawCanvas2D { bounds, commands } => {
                if bounds.width > 0.0 && bounds.height > 0.0 && !commands.is_empty() {
                    let _ = cr.save();
                    cr.rectangle(
                        bounds.x as f64,
                        bounds.y as f64,
                        bounds.width as f64,
                        bounds.height as f64,
                    );
                    cr.clip();
                    cr.translate(bounds.x as f64, bounds.y as f64);
                    cr.set_dash(&[], 0.0);
                    let mut save_depth = 0usize;

                    let emit_canvas_verbs =
                        |cr: &cairo::Context,
                         verbs: &[strata_render::canvas2d::CanvasPathVerb]| {
                            cr.new_path();
                            let mut cur = (0.0_f64, 0.0_f64);
                            let mut start = (0.0_f64, 0.0_f64);
                            for v in verbs {
                                match *v {
                                    strata_render::canvas2d::CanvasPathVerb::MoveTo(x, y) => {
                                        cur = (x as f64, y as f64);
                                        start = cur;
                                        cr.move_to(cur.0, cur.1);
                                    }
                                    strata_render::canvas2d::CanvasPathVerb::LineTo(x, y) => {
                                        cur = (x as f64, y as f64);
                                        cr.line_to(cur.0, cur.1);
                                    }
                                    strata_render::canvas2d::CanvasPathVerb::QuadraticCurveTo {
                                        cpx,
                                        cpy,
                                        x,
                                        y,
                                    } => {
                                        // Exact degree elevation from quadratic (cur, cp, p2) to cubic
                                        let qx = cpx as f64;
                                        let qy = cpy as f64;
                                        let ex = x as f64;
                                        let ey = y as f64;
                                        let cp1x = cur.0 + (2.0 / 3.0) * (qx - cur.0);
                                        let cp1y = cur.1 + (2.0 / 3.0) * (qy - cur.1);
                                        let cp2x = ex + (2.0 / 3.0) * (qx - ex);
                                        let cp2y = ey + (2.0 / 3.0) * (qy - ey);
                                        cr.curve_to(cp1x, cp1y, cp2x, cp2y, ex, ey);
                                        cur = (ex, ey);
                                    }
                                    strata_render::canvas2d::CanvasPathVerb::BezierCurveTo {
                                        cp1x,
                                        cp1y,
                                        cp2x,
                                        cp2y,
                                        x,
                                        y,
                                    } => {
                                        let ex = x as f64;
                                        let ey = y as f64;
                                        cr.curve_to(
                                            cp1x as f64,
                                            cp1y as f64,
                                            cp2x as f64,
                                            cp2y as f64,
                                            ex,
                                            ey,
                                        );
                                        cur = (ex, ey);
                                    }
                                    strata_render::canvas2d::CanvasPathVerb::Arc {
                                        cx,
                                        cy,
                                        radius,
                                        start_angle,
                                        end_angle,
                                        counter_clockwise,
                                    } => {
                                        if counter_clockwise {
                                            cr.arc_negative(
                                                cx as f64,
                                                cy as f64,
                                                radius as f64,
                                                start_angle as f64,
                                                end_angle as f64,
                                            );
                                        } else {
                                            cr.arc(
                                                cx as f64,
                                                cy as f64,
                                                radius as f64,
                                                start_angle as f64,
                                                end_angle as f64,
                                            );
                                        }
                                        cur = (
                                            cx as f64 + radius as f64 * (end_angle as f64).cos(),
                                            cy as f64 + radius as f64 * (end_angle as f64).sin(),
                                        );
                                    }
                                    strata_render::canvas2d::CanvasPathVerb::Close => {
                                        cr.close_path();
                                        cur = start;
                                    }
                                }
                            }
                        };

                    for ccmd in commands {
                        match ccmd {
                            strata_render::canvas2d::CanvasCommand::Save => {
                                let _ = cr.save();
                                save_depth += 1;
                            }
                            strata_render::canvas2d::CanvasCommand::Restore => {
                                if save_depth > 0 {
                                    let _ = cr.restore();
                                    save_depth -= 1;
                                }
                            }
                            strata_render::canvas2d::CanvasCommand::SetLineDash {
                                segments,
                                offset,
                            } => {
                                if segments.is_empty() {
                                    cr.set_dash(&[], 0.0);
                                } else {
                                    let dashes: Vec<f64> =
                                        segments.iter().map(|&d| d as f64).collect();
                                    cr.set_dash(&dashes, *offset as f64);
                                }
                            }
                            strata_render::canvas2d::CanvasCommand::ClipPath { verbs } => {
                                emit_canvas_verbs(cr, verbs);
                                cr.clip();
                            }
                            strata_render::canvas2d::CanvasCommand::FillRect {
                                bounds: rb,
                                color,
                            } => {
                                cr.set_source_rgba(
                                    color.r() as f64,
                                    color.g() as f64,
                                    color.b() as f64,
                                    color.a() as f64,
                                );
                                cr.rectangle(
                                    rb.x as f64,
                                    rb.y as f64,
                                    rb.width as f64,
                                    rb.height as f64,
                                );
                                let _ = cr.fill();
                            }
                            strata_render::canvas2d::CanvasCommand::StrokeRect {
                                bounds: rb,
                                color,
                                line_width,
                            } => {
                                cr.set_source_rgba(
                                    color.r() as f64,
                                    color.g() as f64,
                                    color.b() as f64,
                                    color.a() as f64,
                                );
                                cr.set_line_width(*line_width as f64);
                                cr.rectangle(
                                    rb.x as f64,
                                    rb.y as f64,
                                    rb.width as f64,
                                    rb.height as f64,
                                );
                                let _ = cr.stroke();
                            }
                            strata_render::canvas2d::CanvasCommand::ClearRect { bounds: rb } => {
                                let _ = cr.save();
                                cr.set_operator(cairo::Operator::Clear);
                                cr.rectangle(
                                    rb.x as f64,
                                    rb.y as f64,
                                    rb.width as f64,
                                    rb.height as f64,
                                );
                                let _ = cr.fill();
                                let _ = cr.restore();
                            }
                            strata_render::canvas2d::CanvasCommand::DrawLine {
                                p0,
                                p1,
                                color,
                                line_width,
                            } => {
                                cr.set_source_rgba(
                                    color.r() as f64,
                                    color.g() as f64,
                                    color.b() as f64,
                                    color.a() as f64,
                                );
                                cr.set_line_width(*line_width as f64);
                                cr.new_path();
                                cr.move_to(p0.0 as f64, p0.1 as f64);
                                cr.line_to(p1.0 as f64, p1.1 as f64);
                                let _ = cr.stroke();
                            }
                            strata_render::canvas2d::CanvasCommand::DrawCircle {
                                center,
                                radius,
                                fill_color,
                                stroke_color,
                                line_width,
                            } => {
                                cr.new_path();
                                cr.arc(
                                    center.0 as f64,
                                    center.1 as f64,
                                    *radius as f64,
                                    0.0,
                                    std::f64::consts::TAU,
                                );
                                if let Some(fc) = fill_color {
                                    cr.set_source_rgba(
                                        fc.r() as f64,
                                        fc.g() as f64,
                                        fc.b() as f64,
                                        fc.a() as f64,
                                    );
                                    if stroke_color.is_some() {
                                        let _ = cr.fill_preserve();
                                    } else {
                                        let _ = cr.fill();
                                    }
                                }
                                if let Some(sc) = stroke_color {
                                    cr.set_source_rgba(
                                        sc.r() as f64,
                                        sc.g() as f64,
                                        sc.b() as f64,
                                        sc.a() as f64,
                                    );
                                    cr.set_line_width(*line_width as f64);
                                    let _ = cr.stroke();
                                }
                            }
                            strata_render::canvas2d::CanvasCommand::DrawPath {
                                verbs,
                                fill_color,
                                stroke_color,
                                line_width,
                            } => {
                                emit_canvas_verbs(cr, verbs);
                                if let Some(fc) = fill_color {
                                    cr.set_source_rgba(
                                        fc.r() as f64,
                                        fc.g() as f64,
                                        fc.b() as f64,
                                        fc.a() as f64,
                                    );
                                    if stroke_color.is_some() {
                                        let _ = cr.fill_preserve();
                                    } else {
                                        let _ = cr.fill();
                                    }
                                }
                                if let Some(sc) = stroke_color {
                                    cr.set_source_rgba(
                                        sc.r() as f64,
                                        sc.g() as f64,
                                        sc.b() as f64,
                                        sc.a() as f64,
                                    );
                                    cr.set_line_width(*line_width as f64);
                                    let _ = cr.stroke();
                                }
                            }
                            strata_render::canvas2d::CanvasCommand::FillText {
                                text,
                                x,
                                y,
                                color,
                                font_size,
                                font_family,
                                font_weight,
                                italic,
                                text_align,
                                text_baseline,
                                max_width,
                                ..
                            }
                            | strata_render::canvas2d::CanvasCommand::StrokeText {
                                text,
                                x,
                                y,
                                color,
                                font_size,
                                font_family,
                                font_weight,
                                italic,
                                text_align,
                                text_baseline,
                                max_width,
                                ..
                            } => {
                                let stroke_lw = match ccmd {
                                    strata_render::canvas2d::CanvasCommand::StrokeText {
                                        line_width,
                                        ..
                                    } => Some(*line_width as f64),
                                    _ => None,
                                };
                                let layout =
                                    shared_layout.get_or_insert_with(|| create_parity_layout(cr));
                                layout.set_width(-1);
                                layout.set_alignment(pango::Alignment::Left);
                                layout.set_justify(false);
                                let mut font_desc = pango::FontDescription::new();
                                font_desc.set_family(font_family);
                                font_desc.set_absolute_size(
                                    (*font_size as f64).max(1.0) * pango::SCALE as f64,
                                );
                                font_desc.set_weight(pango::Weight::__Unknown(*font_weight as i32));
                                font_desc.set_style(if *italic {
                                    pango::Style::Italic
                                } else {
                                    pango::Style::Normal
                                });
                                layout.set_font_description(Some(&font_desc));
                                layout.set_text(text);

                                let (pw, ph) = layout.size();
                                let w_px = pw as f64 / pango::SCALE as f64;
                                let h_px = ph as f64 / pango::SCALE as f64;
                                let baseline_px = layout.baseline() as f64 / pango::SCALE as f64;

                                let scale_x = match max_width {
                                    Some(mw) if w_px > (*mw as f64) && w_px > 0.0 => {
                                        (*mw as f64) / w_px
                                    }
                                    _ => 1.0,
                                };
                                let eff_w = w_px * scale_x;

                                let tx = match text_align {
                                    strata_render::canvas2d::CanvasTextAlign::Left
                                    | strata_render::canvas2d::CanvasTextAlign::Start => *x as f64,
                                    strata_render::canvas2d::CanvasTextAlign::Center => {
                                        *x as f64 - eff_w * 0.5
                                    }
                                    strata_render::canvas2d::CanvasTextAlign::Right
                                    | strata_render::canvas2d::CanvasTextAlign::End => {
                                        *x as f64 - eff_w
                                    }
                                };
                                let ty = match text_baseline {
                                    strata_render::canvas2d::CanvasTextBaseline::Top
                                    | strata_render::canvas2d::CanvasTextBaseline::Hanging => {
                                        *y as f64
                                    }
                                    strata_render::canvas2d::CanvasTextBaseline::Middle => {
                                        *y as f64 - h_px * 0.5
                                    }
                                    strata_render::canvas2d::CanvasTextBaseline::Bottom
                                    | strata_render::canvas2d::CanvasTextBaseline::Ideographic => {
                                        *y as f64 - h_px
                                    }
                                    strata_render::canvas2d::CanvasTextBaseline::Alphabetic => {
                                        *y as f64 - baseline_px
                                    }
                                };

                                let _ = cr.save();
                                cr.set_source_rgba(
                                    color.r() as f64,
                                    color.g() as f64,
                                    color.b() as f64,
                                    color.a() as f64,
                                );
                                cr.translate(tx, ty);
                                if (scale_x - 1.0).abs() > 1e-4 {
                                    cr.scale(scale_x, 1.0);
                                }
                                if let Some(lw) = stroke_lw {
                                    cr.new_path();
                                    pangocairo::functions::layout_path(cr, layout);
                                    cr.set_line_width(lw);
                                    let _ = cr.stroke();
                                } else {
                                    pangocairo::functions::show_layout(cr, layout);
                                }
                                let _ = cr.restore();
                            }
                        }
                    }
                    while save_depth > 0 {
                        let _ = cr.restore();
                        save_depth -= 1;
                    }
                    let _ = cr.restore();
                }
            }
            DrawCommand::DrawSvgPath {
                bounds,
                path_data,
                stroke_color,
                fill_color,
                stroke_width,
                stroke_linecap,
                stroke_linejoin,
                stroke_dasharray,
                fill_rule,
                view_box,
            } => {
                if bounds.width > 0.0 && bounds.height > 0.0 && view_box.0 > 0.0 && view_box.1 > 0.0
                {
                    let _ = cr.save();
                    let scale_x = bounds.width as f64 / view_box.0 as f64;
                    let scale_y = bounds.height as f64 / view_box.1 as f64;
                    cr.translate(bounds.x as f64, bounds.y as f64);
                    cr.scale(scale_x, scale_y);

                    cr.new_path();
                    match fill_rule {
                        FillRule::NonZero => cr.set_fill_rule(cairo::FillRule::Winding),
                        FillRule::EvenOdd => cr.set_fill_rule(cairo::FillRule::EvenOdd),
                    }
                    cairo_interpret_svg_path(cr, path_data);

                    if let Some(fill) = fill_color {
                        cr.set_source_rgba(
                            fill.r() as f64,
                            fill.g() as f64,
                            fill.b() as f64,
                            fill.a() as f64,
                        );
                        if stroke_color.is_some() {
                            let _ = cr.fill_preserve();
                        } else {
                            let _ = cr.fill();
                        }
                    }
                    if let Some(stroke) = stroke_color {
                        cr.set_source_rgba(
                            stroke.r() as f64,
                            stroke.g() as f64,
                            stroke.b() as f64,
                            stroke.a() as f64,
                        );
                        cr.set_line_width(*stroke_width as f64);

                        match stroke_linecap {
                            StrokeLineCap::Butt => cr.set_line_cap(cairo::LineCap::Butt),
                            StrokeLineCap::Round => cr.set_line_cap(cairo::LineCap::Round),
                            StrokeLineCap::Square => cr.set_line_cap(cairo::LineCap::Square),
                        }

                        match stroke_linejoin {
                            StrokeLineJoin::Miter => cr.set_line_join(cairo::LineJoin::Miter),
                            StrokeLineJoin::Round => cr.set_line_join(cairo::LineJoin::Round),
                            StrokeLineJoin::Bevel => cr.set_line_join(cairo::LineJoin::Bevel),
                        }

                        if !stroke_dasharray.is_empty() {
                            let dashes: Vec<f64> =
                                stroke_dasharray.iter().map(|&d| d as f64).collect();
                            cr.set_dash(&dashes, 0.0);
                        } else {
                            cr.set_dash(&[], 0.0);
                        }

                        let _ = cr.stroke();
                    }
                    let _ = cr.restore();
                }
            }
            DrawCommand::DrawGpuPath {
                bounds,
                path,
                stroke,
                fill_color,
                stroke_color,
                transform: _,
            } => {
                if bounds.width > 0.0 && bounds.height > 0.0 {
                    let _ = cr.save();
                    cr.new_path();
                    if let Some(fill_rule) = path.fill_rule {
                        match fill_rule {
                            strata_render::gpu_path::FillRule::NonZero => {
                                cr.set_fill_rule(cairo::FillRule::Winding)
                            }
                            strata_render::gpu_path::FillRule::EvenOdd => {
                                cr.set_fill_rule(cairo::FillRule::EvenOdd)
                            }
                        }
                    }
                    for verb in &path.verbs {
                        match verb {
                            strata_render::gpu_path::PathVerb::MoveTo(p) => {
                                cr.move_to(p.x as f64, p.y as f64)
                            }
                            strata_render::gpu_path::PathVerb::LineTo(p) => {
                                cr.line_to(p.x as f64, p.y as f64)
                            }
                            strata_render::gpu_path::PathVerb::QuadTo(p1, p2) => {
                                let (x0, y0) =
                                    cr.current_point().unwrap_or((p1.x as f64, p1.y as f64));
                                let cp1_x = x0 + 2.0 / 3.0 * (p1.x as f64 - x0);
                                let cp1_y = y0 + 2.0 / 3.0 * (p1.y as f64 - y0);
                                let cp2_x = p2.x as f64 + 2.0 / 3.0 * (p1.x as f64 - p2.x as f64);
                                let cp2_y = p2.y as f64 + 2.0 / 3.0 * (p1.y as f64 - p2.y as f64);
                                cr.curve_to(cp1_x, cp1_y, cp2_x, cp2_y, p2.x as f64, p2.y as f64);
                            }
                            strata_render::gpu_path::PathVerb::CubicTo(p1, p2, p3) => {
                                cr.curve_to(
                                    p1.x as f64,
                                    p1.y as f64,
                                    p2.x as f64,
                                    p2.y as f64,
                                    p3.x as f64,
                                    p3.y as f64,
                                );
                            }
                            strata_render::gpu_path::PathVerb::Close => cr.close_path(),
                        }
                    }
                    if let Some(fill) = fill_color {
                        cr.set_source_rgba(
                            fill.r() as f64,
                            fill.g() as f64,
                            fill.b() as f64,
                            fill.a() as f64,
                        );
                        if stroke_color.is_some() {
                            let _ = cr.fill_preserve();
                        } else {
                            let _ = cr.fill();
                        }
                    }
                    if let Some(stroke_col) = stroke_color {
                        if let Some(s) = stroke {
                            cr.set_line_width(s.width as f64);
                            cr.set_miter_limit(s.miter_limit as f64);
                            match s.cap {
                                strata_render::gpu_path::LineCap::Butt => {
                                    cr.set_line_cap(cairo::LineCap::Butt)
                                }
                                strata_render::gpu_path::LineCap::Round => {
                                    cr.set_line_cap(cairo::LineCap::Round)
                                }
                                strata_render::gpu_path::LineCap::Square => {
                                    cr.set_line_cap(cairo::LineCap::Square)
                                }
                            }
                            match s.join {
                                strata_render::gpu_path::LineJoin::Miter => {
                                    cr.set_line_join(cairo::LineJoin::Miter)
                                }
                                strata_render::gpu_path::LineJoin::Round => {
                                    cr.set_line_join(cairo::LineJoin::Round)
                                }
                                strata_render::gpu_path::LineJoin::Bevel => {
                                    cr.set_line_join(cairo::LineJoin::Bevel)
                                }
                            }
                        }
                        cr.set_source_rgba(
                            stroke_col.r() as f64,
                            stroke_col.g() as f64,
                            stroke_col.b() as f64,
                            stroke_col.a() as f64,
                        );
                        let _ = cr.stroke();
                    }
                    let _ = cr.restore();
                }
            }
            DrawCommand::PushClip { bounds, radius } => {
                let _ = cr.save();
                cairo_rounded_rect_radii(
                    cr,
                    bounds.x as f64,
                    bounds.y as f64,
                    bounds.width as f64,
                    bounds.height as f64,
                    *radius,
                );
                cr.clip();
                current_clip = cr
                    .clip_extents()
                    .unwrap_or((0.0, 0.0, 100_000.0, 100_000.0));
            }
            DrawCommand::PopClip => {
                let _ = cr.restore();
                current_clip = cr
                    .clip_extents()
                    .unwrap_or((0.0, 0.0, 100_000.0, 100_000.0));
            }
            DrawCommand::PushBlendMode { mode } => {
                operator_stack.push(cr.operator());
                cr.set_operator(blend_mode_to_cairo_operator(*mode));
            }
            DrawCommand::PopBlendMode => {
                if let Some(op) = operator_stack.pop() {
                    cr.set_operator(op);
                }
            }
            DrawCommand::PushIsolation { isolate } => {
                if *isolate {
                    cr.push_group();
                }
            }
            DrawCommand::PopIsolation => {
                let _ = cr.pop_group_to_source();
                let _ = cr.paint();
            }
            DrawCommand::PushOpacity { alpha } => {
                let _ = cr.save();
                let mut skip_group = false;
                if let Some((bx, by, bw, bh)) = compute_opacity_group_clip(&cmds[cmd_idx + 1..]) {
                    let (cx1, cy1, cx2, cy2) = current_clip;
                    if bx + bw < cx1 || bx > cx2 || by + bh < cy1 || by > cy2 {
                        skip_group = true;
                    } else if let Some((bits, cols, rows)) = dirty_tiles {
                        if !rect_overlaps_dirty_tiles(
                            bx - 12.0,
                            by - 12.0,
                            bx + bw + 12.0,
                            by + bh + 12.0,
                            bits,
                            cols,
                            rows,
                        ) {
                            skip_group = true;
                        }
                    }
                    if skip_group {
                        cr.rectangle(0.0, 0.0, 0.0, 0.0);
                        cr.clip();
                        current_clip = (0.0, 0.0, 0.0, 0.0);
                    } else {
                        cr.rectangle(bx, by, bw, bh);
                        cr.clip();
                        current_clip = cr
                            .clip_extents()
                            .unwrap_or((0.0, 0.0, 100_000.0, 100_000.0));
                    }
                }
                if skip_group {
                    opacity_stack.push(None);
                } else {
                    cr.push_group();
                    opacity_stack.push(Some(*alpha as f64));
                }
            }
            DrawCommand::PopOpacity => {
                if let Some(alpha_opt) = opacity_stack.pop() {
                    if let Some(alpha) = alpha_opt {
                        let _ = cr.pop_group_to_source();
                        let _ = cr.paint_with_alpha(alpha);
                    }
                    let _ = cr.restore();
                    current_clip = cr
                        .clip_extents()
                        .unwrap_or((0.0, 0.0, 100_000.0, 100_000.0));
                }
            }
            DrawCommand::PushFilter { .. } => {
                cr.push_group();
            }
            DrawCommand::PopFilter => {
                let _ = cr.pop_group_to_source();
                let _ = cr.paint();
            }
            DrawCommand::PushClipPath { bounds, clip_path } => {
                let _ = cr.save();
                match clip_path {
                    strata_style::effects::ClipPath::Inset {
                        top,
                        right,
                        bottom,
                        left,
                        border_radius,
                    } => {
                        let ix = bounds.x as f64 + *left as f64;
                        let iy = bounds.y as f64 + *top as f64;
                        let iw = (bounds.width as f64 - *left as f64 - *right as f64).max(0.0);
                        let ih = (bounds.height as f64 - *top as f64 - *bottom as f64).max(0.0);
                        cairo_rounded_rect(cr, ix, iy, iw, ih, *border_radius as f64);
                        cr.clip();
                    }
                    strata_style::effects::ClipPath::Circle {
                        radius,
                        center_x,
                        center_y,
                    } => {
                        let cx = bounds.x as f64 + *center_x as f64;
                        let cy = bounds.y as f64 + *center_y as f64;
                        cr.arc(cx, cy, *radius as f64, 0.0, 2.0 * std::f64::consts::PI);
                        cr.clip();
                    }
                    strata_style::effects::ClipPath::Ellipse {
                        radius_x,
                        radius_y,
                        center_x,
                        center_y,
                    } => {
                        if *radius_x > 0.0 && *radius_y > 0.0 {
                            let cx = bounds.x as f64 + *center_x as f64;
                            let cy = bounds.y as f64 + *center_y as f64;
                            let rx = *radius_x as f64;
                            let ry = *radius_y as f64;
                            let _ = cr.save();
                            cr.translate(cx, cy);
                            cr.scale(rx, ry);
                            cr.arc(0.0, 0.0, 1.0, 0.0, 2.0 * std::f64::consts::PI);
                            let _ = cr.restore();
                            cr.clip();
                        }
                    }
                    strata_style::effects::ClipPath::Polygon(vertices) => {
                        if !vertices.is_empty() {
                            cr.new_path();
                            cr.move_to(
                                bounds.x as f64 + vertices[0].x as f64,
                                bounds.y as f64 + vertices[0].y as f64,
                            );
                            for pt in &vertices[1..] {
                                cr.line_to(
                                    bounds.x as f64 + pt.x as f64,
                                    bounds.y as f64 + pt.y as f64,
                                );
                            }
                            cr.close_path();
                            cr.clip();
                        }
                    }
                    strata_style::effects::ClipPath::Path(svg_path) => {
                        let _ = cr.save();
                        cr.translate(bounds.x as f64, bounds.y as f64);
                        cairo_interpret_svg_path(cr, svg_path);
                        let _ = cr.restore();
                        cr.clip();
                    }
                    strata_style::effects::ClipPath::None => {}
                }
                current_clip = cr
                    .clip_extents()
                    .unwrap_or((0.0, 0.0, 100_000.0, 100_000.0));
            }
            DrawCommand::PopClipPath => {
                let _ = cr.restore();
                current_clip = cr
                    .clip_extents()
                    .unwrap_or((0.0, 0.0, 100_000.0, 100_000.0));
            }
            DrawCommand::PushTransform { matrix } => {
                let _ = cr.save();
                let m = matrix.m;
                let cairo_mat = cairo::Matrix::new(
                    m[0] as f64,
                    m[1] as f64,
                    m[4] as f64,
                    m[5] as f64,
                    m[12] as f64,
                    m[13] as f64,
                );
                cr.transform(cairo_mat);
            }
            DrawCommand::PopTransform => {
                let _ = cr.restore();
            }
            DrawCommand::DrawImage { bounds, image }
                if bounds.width > 0.0 && bounds.height > 0.0 =>
            {
                let decoded = cairo::ImageSurface::create_from_png(&mut std::io::Cursor::new(
                    &image.data[..],
                ));
                if let Ok(surface) = decoded {
                    let surface_w = surface.width() as f64;
                    let surface_h = surface.height() as f64;
                    if surface_w > 0.0 && surface_h > 0.0 {
                        let _ = cr.save();
                        cr.rectangle(
                            bounds.x as f64,
                            bounds.y as f64,
                            bounds.width as f64,
                            bounds.height as f64,
                        );
                        cr.clip();
                        cr.translate(bounds.x as f64, bounds.y as f64);
                        cr.scale(
                            bounds.width as f64 / surface_w,
                            bounds.height as f64 / surface_h,
                        );
                        let _ = cr.set_source_surface(&surface, 0.0, 0.0);
                        cr.source().set_filter(cairo::Filter::Good);
                        let _ = cr.paint();
                        let _ = cr.restore();
                    }
                }
            }
            DrawCommand::DrawViewport3D { bounds, scene }
                if bounds.width > 0.0 && bounds.height > 0.0 =>
            {
                if punch_viewport_hole {
                    let _ = cr.save();
                    cr.rectangle(
                        bounds.x as f64,
                        bounds.y as f64,
                        bounds.width as f64,
                        bounds.height as f64,
                    );
                    cr.set_operator(cairo::Operator::Clear);
                    let _ = cr.fill();
                    let _ = cr.restore();
                } else {
                    let w = bounds.width.max(1.0).round() as usize;
                    let h = bounds.height.max(1.0).round() as usize;
                    let interactive = IN_LIVE_GTK_DRAW.with(|live| live.get());
                    if let Some(res) = viewport_3d_surface_with_fallback(scene, w, h, interactive) {
                        let _ = cr.save();
                        cr.translate(bounds.x as f64, bounds.y as f64);
                        if res.src_width != w || res.src_height != h {
                            cr.scale(
                                bounds.width as f64 / res.src_width.max(1) as f64,
                                bounds.height as f64 / res.src_height.max(1) as f64,
                            );
                        }
                        let _ = cr.set_source_surface(&res.surface, 0.0, 0.0);
                        let _ = cr.paint();
                        let _ = cr.restore();
                        if res.needs_debounced_full_res && interactive {
                            schedule_viewport_3d_debounced_redraw();
                        }
                    }
                }
            }
            _ => {}
        }
    }

    static RENDER_FRAMES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    if gtk_perf_logging_enabled() {
        let frame = RENDER_FRAMES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if frame < 6 || frame.is_multiple_of(120) {
            let (w, h) = cr
                .clip_extents()
                .map(|e| (e.2 - e.0, e.3 - e.1))
                .unwrap_or((0.0, 0.0));
            eprintln!(
                "[perf] cairo_render: {:.3} ms (frame {} {:.0}x{:.0} {} commands)",
                perf_start.elapsed().as_secs_f64() * 1000.0,
                frame,
                w,
                h,
                list.commands().len()
            );
        }
    }
}

/// Renders a DisplayList into an in-memory Cairo ImageSurface and saves it as a PNG file.
pub fn render_to_png(
    display_list: &DisplayList,
    width: i32,
    height: i32,
    path: &str,
) -> Result<(), String> {
    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, width, height)
        .map_err(|e| format!("Failed to create Cairo image surface: {:?}", e))?;
    let cr = cairo::Context::new(&surface)
        .map_err(|e| format!("Failed to create Cairo context: {:?}", e))?;

    // Match the measurement context and the harness Chromium font flags.
    cr.set_font_options(&parity_font_options());

    // Fill background black
    cr.set_source_rgb(0.035, 0.035, 0.043);
    let _ = cr.paint();

    render_display_list_to_cairo(&cr, display_list);

    let mut file =
        std::fs::File::create(path).map_err(|e| format!("Failed to create output file: {}", e))?;
    surface
        .write_to_png(&mut file)
        .map_err(|e| format!("Failed to write PNG: {:?}", e))?;

    Ok(())
}

/// Renders a DisplayList into an in-memory Cairo ImageSurface and returns raw PNG bytes.
/// Optionally composites an emulated MCP cursor with authentic Yaru theme bitmaps.
pub fn render_to_png_bytes(
    display_list: &DisplayList,
    width: i32,
    height: i32,
    cursor_overlay: Option<(
        &crate::mcp_cursor::McpCursorState,
        &crate::cursor_theme::CursorTheme,
    )>,
) -> Result<Vec<u8>, String> {
    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, width, height)
        .map_err(|e| format!("Failed to create Cairo image surface: {:?}", e))?;
    let cr = cairo::Context::new(&surface)
        .map_err(|e| format!("Failed to create Cairo context: {:?}", e))?;

    cr.set_font_options(&parity_font_options());

    cr.set_source_rgb(0.035, 0.035, 0.043);
    let _ = cr.paint();

    render_display_list_to_cairo(&cr, display_list);

    if let Some((cursor_state, theme)) = cursor_overlay {
        if cursor_state.fade_opacity > 0.001 {
            theme.render_cairo(
                &cr,
                cursor_state.cursor_icon,
                cursor_state.current_x,
                cursor_state.current_y,
                cursor_state.fade_opacity,
                cursor_state.is_down,
            );
        }
    }

    let mut buf = Vec::new();
    {
        let mut cursor = std::io::Cursor::new(&mut buf);
        surface
            .write_to_png(&mut cursor)
            .map_err(|e| format!("Failed to write PNG: {:?}", e))?;
    }

    Ok(buf)
}

thread_local! {
    static CURRENT_GTK_WINDOW: std::cell::RefCell<Option<gtk::Window>> = const { std::cell::RefCell::new(None) };
    static IS_MAXIMIZED_TRACKER: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static IS_WINDOW_DOCKED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static WINDOW_NORMAL_GEOMETRY: std::cell::Cell<(i32, i32, i32, i32)> = const { std::cell::Cell::new((100, 100, 1280, 800)) };
}

/// Registers the GTK window used by the global window-control helpers.
///
/// `launch_gtk_window` calls this for the window it creates. Embedders that
/// build their own `gtk::Window` can call it so `gtk_toggle_maximize`,
/// `gtk_minimize`, and `gtk_focus_window` operate on that window. It must be
/// called on the GTK main thread.
#[doc(hidden)]
pub fn gtk_register_window(window: gtk::Window) {
    CURRENT_GTK_WINDOW.with(|cell| {
        *cell.borrow_mut() = Some(window);
    });
}

/// Minimizes the current native GTK desktop window.
pub fn gtk_minimize() {
    CURRENT_GTK_WINDOW.with(|w| {
        use gtk::prelude::*;
        if let Some(win) = w.borrow().as_ref() {
            win.iconify();
        }
    });
}

/// Toggles maximize / restore state on the current native GTK desktop window.
/// Returns true if the window is now maximized, false otherwise.
pub fn gtk_toggle_maximize() -> bool {
    if IS_WINDOW_DOCKED.with(|d| d.get()) {
        return false;
    }
    CURRENT_GTK_WINDOW.with(|w| {
        use gtk::prelude::*;
        if let Some(win) = w.borrow().as_ref() {
            let was_max = win.is_maximized() || IS_MAXIMIZED_TRACKER.with(|t| t.get());
            let next_max = !was_max;
            IS_MAXIMIZED_TRACKER.with(|t| t.set(next_max));
            if was_max {
                win.unmaximize();
                let (gx, gy, gw, gh) = WINDOW_NORMAL_GEOMETRY.with(|g| g.get());
                win.resize(gw, gh);
                win.move_(gx, gy);
            } else {
                let (wx, wy) = win.position();
                let (ww, wh) = win.size();
                WINDOW_NORMAL_GEOMETRY.with(|g| g.set((wx, wy, ww, wh)));
                win.maximize();
                if let Some(display) = gtk::gdk::Display::default() {
                    if let Some(monitor) = display.primary_monitor() {
                        let geom = monitor.geometry();
                        win.move_(geom.x(), geom.y());
                        win.resize(geom.width(), geom.height());
                    }
                }
            }
            next_max
        } else {
            false
        }
    })
}

/// Returns true while the native GTK window is maximized.
pub fn gtk_is_maximized() -> bool {
    CURRENT_GTK_WINDOW.with(|w| {
        use gtk::prelude::*;
        w.borrow()
            .as_ref()
            .map(|win| win.is_maximized() || IS_MAXIMIZED_TRACKER.with(|t| t.get()))
            .unwrap_or(false)
    })
}

/// Brings the native GTK desktop window to the foreground, un-minimizing / deiconifying if needed.
pub fn gtk_focus_window() {
    CURRENT_GTK_WINDOW.with(|w| {
        use gtk::prelude::*;
        if let Some(win) = w.borrow().as_ref() {
            win.deiconify();
            win.present();
        }
    });
}

thread_local! {
    static EVENT_DISPATCH_CALLBACK: std::cell::RefCell<Option<Box<dyn Fn(crate::events::PlatformEvent) -> bool>>> = const { std::cell::RefCell::new(None) };
    static DRAWING_AREA_HANDLE: std::cell::RefCell<Option<gtk::DrawingArea>> = const { std::cell::RefCell::new(None) };
    static ACTIVE_PRESENT_MODE: std::cell::Cell<crate::surface::PresentMode> = const { std::cell::Cell::new(crate::surface::PresentMode::Fifo) };
}

#[cfg(all(feature = "wgpu-backend", feature = "xlib"))]
mod gtk_gpu_x11 {
    use gtk::glib::translate::FromGlib;
    use gtk::prelude::*;
    use std::ffi::c_void;
    use std::ptr::NonNull;
    use std::sync::Arc;
    use strata_render::compositor::{DisplayList, DrawCommand};
    use strata_render::scene3d::Scene3D;
    use x11::xlib;

    #[link(name = "gdk-3")]
    extern "C" {
        fn gdk_x11_display_get_type() -> gtk::glib::ffi::GType;
        fn gdk_x11_window_get_xid(window: *mut gtk::gdk::ffi::GdkWindow) -> libc::c_ulong;
        fn gdk_x11_display_get_xdisplay(
            display: *mut gtk::gdk::ffi::GdkDisplay,
        ) -> *mut xlib::Display;
    }

    #[link(name = "Xext")]
    extern "C" {
        fn XShapeCombineRectangles(
            display: *mut xlib::Display,
            dest: xlib::Window,
            dest_kind: libc::c_int,
            x_off: libc::c_int,
            y_off: libc::c_int,
            rectangles: *const c_void,
            n_rects: libc::c_int,
            op: libc::c_int,
            ordering: libc::c_int,
        );
    }

    const OVERLAY_TILE_PX: usize = 64;

    pub(super) struct GtkGpuSurfaceState {
        xdisplay: *mut xlib::Display,
        child_xid: xlib::Window,
        presenter: crate::wgpu_surface::WgpuWindowPresenter,
        width: u32,
        height: u32,
        mapped: bool,
        overlay_uploaded: bool,
        last_overlay_hash: Option<u64>,
        overlay_surface: Option<(u32, u32, cairo::ImageSurface)>,
        tile_cols: usize,
        tile_rows: usize,
        last_tile_hashes: Vec<u64>,
        new_tile_hashes: Vec<u64>,
        dirty_tile_bits: Vec<u64>,
        dirty_upload_rects: Vec<(u32, u32, u32, u32)>,
        upload_scratch: Vec<u8>,
        last_viewports: Vec<([f32; 4], Arc<Scene3D>)>,
    }

    impl GtkGpuSurfaceState {
        fn set_mapped(&mut self, want_mapped: bool) {
            if self.mapped == want_mapped {
                return;
            }
            unsafe {
                if want_mapped {
                    xlib::XMapWindow(self.xdisplay, self.child_xid);
                } else {
                    xlib::XUnmapWindow(self.xdisplay, self.child_xid);
                }
                xlib::XFlush(self.xdisplay);
            }
            self.mapped = want_mapped;
        }

        fn resize(&mut self, w: u32, h: u32) {
            let w = w.max(1);
            let h = h.max(1);
            if self.width == w && self.height == h {
                return;
            }
            unsafe {
                xlib::XResizeWindow(self.xdisplay, self.child_xid, w, h);
                xlib::XSync(self.xdisplay, 0);
            }
            self.width = w;
            self.height = h;
            self.presenter.resize(w, h);
            self.overlay_uploaded = false;
            self.last_overlay_hash = None;
            self.overlay_surface = None;
            self.tile_cols = 0;
            self.tile_rows = 0;
            self.last_tile_hashes.clear();
            self.last_viewports.clear();
        }

        fn destroy(self) {
            let xdisplay = self.xdisplay;
            let child_xid = self.child_xid;
            // Drop wgpu::Surface before destroying the underlying X11 child window.
            drop(self.presenter);
            unsafe {
                xlib::XDestroyWindow(xdisplay, child_xid);
                xlib::XFlush(xdisplay);
            }
        }
    }

    struct FnvFmtHasher(u64);

    impl std::fmt::Write for FnvFmtHasher {
        #[inline]
        fn write_str(&mut self, s: &str) -> std::fmt::Result {
            for &b in s.as_bytes() {
                self.0 ^= b as u64;
                self.0 = self.0.wrapping_mul(0x100_0000_01b3);
            }
            Ok(())
        }
    }

    impl FnvFmtHasher {
        #[inline]
        fn push_u64(&mut self, v: u64) {
            for b in v.to_le_bytes() {
                self.0 ^= b as u64;
                self.0 = self.0.wrapping_mul(0x100_0000_01b3);
            }
        }
    }

    #[inline]
    fn rgba_to_u64(c: strata_primitives::Rgba) -> u64 {
        ((c.r as u64) << 24) | ((c.g as u64) << 16) | ((c.b as u64) << 8) | (c.a as u64)
    }

    fn hash_single_draw_command(cmd: &DrawCommand, seed: u64) -> u64 {
        use std::fmt::Write as _;
        let mut h_acc = FnvFmtHasher(seed);
        match cmd {
            DrawCommand::DrawViewport3D { bounds, .. } => {
                h_acc.push_u64(0x3D);
                h_acc.push_u64(bounds.x.to_bits() as u64);
                h_acc.push_u64(bounds.y.to_bits() as u64);
                h_acc.push_u64(bounds.width.to_bits() as u64);
                h_acc.push_u64(bounds.height.to_bits() as u64);
            }
            DrawCommand::DrawRect { bounds, color } => {
                h_acc.push_u64(0x10);
                h_acc.push_u64(bounds.x.to_bits() as u64);
                h_acc.push_u64(bounds.y.to_bits() as u64);
                h_acc.push_u64(bounds.width.to_bits() as u64);
                h_acc.push_u64(bounds.height.to_bits() as u64);
                h_acc.push_u64(rgba_to_u64(*color));
            }
            DrawCommand::DrawRoundedRect {
                bounds,
                color,
                border_radius,
                border_width,
                border_color,
            } => {
                h_acc.push_u64(0x11);
                h_acc.push_u64(bounds.x.to_bits() as u64);
                h_acc.push_u64(bounds.y.to_bits() as u64);
                h_acc.push_u64(bounds.width.to_bits() as u64);
                h_acc.push_u64(bounds.height.to_bits() as u64);
                h_acc.push_u64(rgba_to_u64(*color));
                h_acc.push_u64(border_radius.top_left.to_bits() as u64);
                h_acc.push_u64(border_radius.top_right.to_bits() as u64);
                h_acc.push_u64(border_radius.bottom_right.to_bits() as u64);
                h_acc.push_u64(border_radius.bottom_left.to_bits() as u64);
                h_acc.push_u64(border_width.top.to_bits() as u64);
                h_acc.push_u64(border_width.right.to_bits() as u64);
                h_acc.push_u64(border_width.bottom.to_bits() as u64);
                h_acc.push_u64(border_width.left.to_bits() as u64);
                if let Some(bc) = border_color {
                    h_acc.push_u64(rgba_to_u64(*bc));
                }
            }
            DrawCommand::DrawTextFragments { fragments } => {
                h_acc.push_u64(0x22);
                h_acc.push_u64(fragments.len() as u64);
                for f in fragments {
                    let _ = h_acc.write_str(&f.text);
                    h_acc.push_u64(f.bounds.x.to_bits() as u64);
                    h_acc.push_u64(f.bounds.y.to_bits() as u64);
                    h_acc.push_u64(f.bounds.width.to_bits() as u64);
                    h_acc.push_u64(f.bounds.height.to_bits() as u64);
                    h_acc.push_u64(rgba_to_u64(f.style.color));
                    h_acc.push_u64(
                        f.style
                            .background_color
                            .map(rgba_to_u64)
                            .unwrap_or(0xdead_beef),
                    );
                    h_acc.push_u64(f.style.font_size.to_bits() as u64);
                    h_acc.push_u64(f.style.font_weight as u64);
                    h_acc.push_u64(f.style.font_style as u64);
                    h_acc.push_u64(f.style.text_decoration as u64);
                }
            }
            DrawCommand::DrawText {
                origin,
                bounds_width,
                bounds_height,
                text_align,
                text,
                font_size,
                line_height,
                font_weight,
                color,
                is_monospace,
                font_family,
            } => {
                h_acc.push_u64(0x23);
                let _ = h_acc.write_str(text);
                h_acc.push_u64(origin.0.to_bits() as u64);
                h_acc.push_u64(origin.1.to_bits() as u64);
                h_acc.push_u64(bounds_width.unwrap_or(-1.0).to_bits() as u64);
                h_acc.push_u64(bounds_height.unwrap_or(-1.0).to_bits() as u64);
                h_acc.push_u64(*text_align as u64);
                h_acc.push_u64(font_size.to_bits() as u64);
                h_acc.push_u64(line_height.to_bits() as u64);
                h_acc.push_u64(*font_weight as u64);
                h_acc.push_u64(rgba_to_u64(*color));
                h_acc.push_u64(*is_monospace as u64);
                let _ = write!(h_acc, "{:?}", font_family);
            }
            DrawCommand::DrawSvgPath {
                bounds,
                path_data,
                stroke_color,
                fill_color,
                stroke_width,
                view_box,
                ..
            } => {
                h_acc.push_u64(0x28);
                h_acc.push_u64(bounds.x.to_bits() as u64);
                h_acc.push_u64(bounds.y.to_bits() as u64);
                h_acc.push_u64(bounds.width.to_bits() as u64);
                h_acc.push_u64(bounds.height.to_bits() as u64);
                let _ = h_acc.write_str(path_data);
                h_acc.push_u64(stroke_color.map(rgba_to_u64).unwrap_or(0xdead_0001));
                h_acc.push_u64(fill_color.map(rgba_to_u64).unwrap_or(0xdead_0002));
                h_acc.push_u64(stroke_width.to_bits() as u64);
                h_acc.push_u64(view_box.0.to_bits() as u64);
                h_acc.push_u64(view_box.1.to_bits() as u64);
            }
            DrawCommand::PushOpacity { alpha } => {
                h_acc.push_u64(0x33);
                h_acc.push_u64(alpha.to_bits() as u64);
            }
            DrawCommand::PopOpacity => h_acc.push_u64(0x34),
            DrawCommand::PushClip { bounds, radius } => {
                h_acc.push_u64(0x35);
                h_acc.push_u64(bounds.x.to_bits() as u64);
                h_acc.push_u64(bounds.y.to_bits() as u64);
                h_acc.push_u64(bounds.width.to_bits() as u64);
                h_acc.push_u64(bounds.height.to_bits() as u64);
                h_acc.push_u64(radius.top_left.to_bits() as u64);
                h_acc.push_u64(radius.top_right.to_bits() as u64);
                h_acc.push_u64(radius.bottom_right.to_bits() as u64);
                h_acc.push_u64(radius.bottom_left.to_bits() as u64);
            }
            DrawCommand::PopClip => h_acc.push_u64(0x36),
            DrawCommand::PushBackdropFilter { .. } | DrawCommand::PopBackdropFilter => {}
            other => {
                let _ = write!(h_acc, "{:?}", other);
            }
        }
        h_acc.0
    }

    fn compute_overlay_hash(
        list: &DisplayList,
        w: u32,
        h: u32,
        transparent_bg: bool,
        cursor_overlay: Option<(&crate::McpCursorState, &crate::CursorTheme)>,
    ) -> u64 {
        use std::fmt::Write as _;
        let mut h_acc = FnvFmtHasher(0xcbf2_9ce4_8422_2325);
        h_acc.push_u64(w as u64);
        h_acc.push_u64(h as u64);
        h_acc.push_u64(transparent_bg as u64);
        if let Some((mc, _)) = cursor_overlay {
            if mc.fade_opacity > 0.001 {
                h_acc.push_u64(1);
                h_acc.push_u64(mc.current_x.to_bits() as u64);
                h_acc.push_u64(mc.current_y.to_bits() as u64);
                h_acc.push_u64(mc.fade_opacity.to_bits() as u64);
                h_acc.push_u64(mc.is_down as u64);
                let _ = write!(h_acc, "{:?}", mc.cursor_icon);
            }
        }
        h_acc.push_u64(list.commands().len() as u64);
        let mut acc = h_acc.0;
        for cmd in list.commands() {
            acc = hash_single_draw_command(cmd, acc);
        }
        acc
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn compute_overlay_tile_hashes(
        list: &DisplayList,
        w: u32,
        h: u32,
        transparent_bg: bool,
        cursor_overlay: Option<(&crate::McpCursorState, &crate::CursorTheme)>,
        cols: usize,
        rows: usize,
        out_hashes: &mut Vec<u64>,
    ) {
        use std::fmt::Write as _;
        out_hashes.clear();
        out_hashes.resize(
            cols * rows,
            0xcbf2_9ce4_8422_2325 ^ ((transparent_bg as u64) << 1),
        );
        let wf = w as f64;
        let hf = h as f64;
        let mut clip_stack: Vec<(f64, f64, f64, f64)> = vec![(0.0, 0.0, wf, hf)];
        let mut state_stack: Vec<u64> = vec![0x9e37_79b9_7f4a_7c15];

        let mut mix_rect = |rx1: f64, ry1: f64, rx2: f64, ry2: f64, cmd_h: u64| {
            if rx2 <= rx1 || ry2 <= ry1 {
                return;
            }
            let c0 = ((rx1.floor().max(0.0) as usize) >> 6).min(cols - 1);
            let r0 = ((ry1.floor().max(0.0) as usize) >> 6).min(rows - 1);
            let c1 = (((rx2.ceil().max(1.0) as usize) - 1) >> 6).min(cols - 1);
            let r1 = (((ry2.ceil().max(1.0) as usize) - 1) >> 6).min(rows - 1);
            if c0 == 0 && c1 + 1 == cols && r0 == 0 && r1 + 1 == rows {
                for slot in out_hashes.iter_mut() {
                    *slot = (*slot ^ cmd_h).wrapping_mul(0x100_0000_01b3);
                }
            } else {
                for r in r0..=r1 {
                    let row_off = r * cols;
                    for c in c0..=c1 {
                        let slot = &mut out_hashes[row_off + c];
                        *slot = (*slot ^ cmd_h).wrapping_mul(0x100_0000_01b3);
                    }
                }
            }
        };

        for cmd in list.commands() {
            let cur_state = *state_stack.last().unwrap_or(&0x9e37_79b9_7f4a_7c15);
            let (cx1, cy1, cx2, cy2) = *clip_stack.last().unwrap_or(&(0.0, 0.0, wf, hf));
            match cmd {
                DrawCommand::PushClip { bounds, .. } => {
                    state_stack.push(hash_single_draw_command(cmd, cur_state));
                    clip_stack.push((
                        cx1.max(bounds.x as f64),
                        cy1.max(bounds.y as f64),
                        cx2.min((bounds.x + bounds.width) as f64),
                        cy2.min((bounds.y + bounds.height) as f64),
                    ));
                }
                DrawCommand::PopClip => {
                    if clip_stack.len() > 1 {
                        clip_stack.pop();
                    }
                    if state_stack.len() > 1 {
                        state_stack.pop();
                    }
                }
                DrawCommand::PushClipPath { bounds, .. } => {
                    state_stack.push(hash_single_draw_command(cmd, cur_state));
                    clip_stack.push((
                        cx1.max(bounds.x as f64),
                        cy1.max(bounds.y as f64),
                        cx2.min((bounds.x + bounds.width) as f64),
                        cy2.min((bounds.y + bounds.height) as f64),
                    ));
                }
                DrawCommand::PopClipPath => {
                    if clip_stack.len() > 1 {
                        clip_stack.pop();
                    }
                    if state_stack.len() > 1 {
                        state_stack.pop();
                    }
                }
                DrawCommand::PushOpacity { .. } | DrawCommand::PushBlendMode { .. } => {
                    state_stack.push(hash_single_draw_command(cmd, cur_state));
                }
                DrawCommand::PopOpacity | DrawCommand::PopBlendMode => {
                    if state_stack.len() > 1 {
                        state_stack.pop();
                    }
                }
                DrawCommand::PushBackdropFilter { .. } | DrawCommand::PopBackdropFilter => {}
                _ => {
                    let cmd_h = hash_single_draw_command(cmd, cur_state);
                    let (bx1, by1, bx2, by2) =
                        super::command_paint_rect(cmd).unwrap_or((cx1, cy1, cx2, cy2));
                    mix_rect(
                        bx1.max(cx1).max(0.0),
                        by1.max(cy1).max(0.0),
                        bx2.min(cx2).min(wf),
                        by2.min(cy2).min(hf),
                        cmd_h,
                    );
                }
            }
        }

        if let Some((mc, _)) = cursor_overlay {
            if mc.fade_opacity > 0.001 {
                let mut ch = FnvFmtHasher(0x517c_c1b7_2722_0a95);
                ch.push_u64(mc.current_x.to_bits() as u64);
                ch.push_u64(mc.current_y.to_bits() as u64);
                ch.push_u64(mc.fade_opacity.to_bits() as u64);
                ch.push_u64(mc.is_down as u64);
                let _ = write!(ch, "{:?}", mc.cursor_icon);
                let cx = mc.current_x as f64;
                let cy = mc.current_y as f64;
                mix_rect(
                    (cx - 32.0).max(0.0),
                    (cy - 32.0).max(0.0),
                    (cx + 32.0).min(wf),
                    (cy + 32.0).min(hf),
                    ch.0,
                );
            }
        }
    }

    thread_local! {
        static GPU_SURFACE_STATE: std::cell::RefCell<Option<GtkGpuSurfaceState>> =
            const { std::cell::RefCell::new(None) };
        static GPU_SURFACE_INIT_FAILED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }

    fn init_state_from_drawing_area(
        da: &gtk::DrawingArea,
        width: u32,
        height: u32,
        present_mode: crate::surface::PresentMode,
    ) -> Result<GtkGpuSurfaceState, String> {
        let gdk_win = da
            .window()
            .ok_or_else(|| "GTK drawing area not yet realized".to_string())?;
        let display = da.display();
        let x11_disp_type = unsafe { gtk::glib::Type::from_glib(gdk_x11_display_get_type()) };
        if display.type_() != x11_disp_type {
            return Err("active GDK display is not X11".to_string());
        }

        let xdisplay = unsafe {
            gdk_x11_display_get_xdisplay(gtk::glib::translate::ToGlibPtr::to_glib_none(&display).0)
        };
        let parent_xid = unsafe {
            gdk_x11_window_get_xid(gtk::glib::translate::ToGlibPtr::to_glib_none(&gdk_win).0)
        };
        if xdisplay.is_null() || parent_xid == 0 {
            return Err("invalid X11 display or parent window handle".to_string());
        }

        let w = width.max(1);
        let h = height.max(1);
        let (child_xid, screen, visual_id) = unsafe {
            let mut attrs: xlib::XWindowAttributes = std::mem::zeroed();
            xlib::XGetWindowAttributes(xdisplay, parent_xid, &mut attrs);
            let child = xlib::XCreateSimpleWindow(xdisplay, parent_xid, 0, 0, w, h, 0, 0, 0);
            // Zero event mask and empty input shape let all pointer/cursor events stay cleanly on GTK DrawingArea.
            xlib::XSelectInput(xdisplay, child, 0);
            XShapeCombineRectangles(xdisplay, child, 2, 0, 0, std::ptr::null(), 0, 0, 0);
            xlib::XMapWindow(xdisplay, child);
            xlib::XSync(xdisplay, 0);
            let scr = xlib::XDefaultScreen(xdisplay);
            let vid = if !attrs.visual.is_null() {
                (*attrs.visual).visualid
            } else {
                0
            };
            (child, scr, vid)
        };

        let mut win_handle = raw_window_handle::XlibWindowHandle::new(child_xid);
        win_handle.visual_id = visual_id;
        let disp_handle = raw_window_handle::XlibDisplayHandle::new(
            NonNull::new(xdisplay as *mut c_void),
            screen,
        );

        let swapchain_mode = match present_mode {
            crate::surface::PresentMode::Fifo => crate::surface::PresentMode::Mailbox,
            other => other,
        };
        let presenter = unsafe {
            crate::wgpu_surface::WgpuWindowPresenter::from_raw_handles(
                raw_window_handle::RawWindowHandle::Xlib(win_handle),
                raw_window_handle::RawDisplayHandle::Xlib(disp_handle),
                w,
                h,
                swapchain_mode,
            )
        };
        let presenter = match presenter {
            Ok(p) => p,
            Err(e) => {
                unsafe {
                    xlib::XDestroyWindow(xdisplay, child_xid);
                    xlib::XFlush(xdisplay);
                }
                return Err(e);
            }
        };

        if super::gtk_perf_logging_enabled() {
            eprintln!(
                "[perf] wgpu direct X11 swapchain ready ({}x{}, requested={:?}, active={:?})",
                w,
                h,
                present_mode,
                presenter.active_wgpu_present_mode()
            );
        }

        Ok(GtkGpuSurfaceState {
            xdisplay,
            child_xid,
            presenter,
            width: w,
            height: h,
            mapped: true,
            overlay_uploaded: false,
            last_overlay_hash: None,
            overlay_surface: None,
            tile_cols: 0,
            tile_rows: 0,
            last_tile_hashes: Vec::new(),
            new_tile_hashes: Vec::new(),
            dirty_tile_bits: Vec::new(),
            dirty_upload_rects: Vec::new(),
            upload_scratch: Vec::new(),
            last_viewports: Vec::new(),
        })
    }

    fn rasterize_and_upload_overlay(
        state: &mut GtkGpuSurfaceState,
        list: &DisplayList,
        w: u32,
        h: u32,
        transparent_bg: bool,
        cursor_overlay: Option<(&crate::McpCursorState, &crate::CursorTheme)>,
    ) -> bool {
        let needs_new_surf = state
            .overlay_surface
            .as_ref()
            .map(|(sw, sh, _)| *sw != w || *sh != h)
            .unwrap_or(true);
        if needs_new_surf {
            let Ok(surf) = cairo::ImageSurface::create(cairo::Format::ARgb32, w as i32, h as i32)
            else {
                return false;
            };
            state.overlay_surface = Some((w, h, surf));
            state.tile_cols = 0;
            state.tile_rows = 0;
            state.last_tile_hashes.clear();
        }

        let has_unbounded = list.commands().iter().any(|c| {
            matches!(
                c,
                DrawCommand::PushTransform { .. }
                    | DrawCommand::PushFilter { .. }
                    | DrawCommand::PushIsolation { isolate: true }
            )
        });
        let cols = (w as usize).div_ceil(OVERLAY_TILE_PX).max(1);
        let rows = (h as usize).div_ceil(OVERLAY_TILE_PX).max(1);
        let total_tiles = cols * rows;

        if !has_unbounded {
            compute_overlay_tile_hashes(
                list,
                w,
                h,
                transparent_bg,
                cursor_overlay,
                cols,
                rows,
                &mut state.new_tile_hashes,
            );
        } else {
            state.last_tile_hashes.clear();
        }

        let can_incremental = !needs_new_surf
            && state.overlay_uploaded
            && !has_unbounded
            && state.tile_cols == cols
            && state.tile_rows == rows
            && state.last_tile_hashes.len() == total_tiles;

        if can_incremental {
            state.dirty_tile_bits.clear();
            state.dirty_tile_bits.resize(total_tiles.div_ceil(64), 0);
            state.dirty_upload_rects.clear();
            let mut dirty_tiles_count = 0usize;
            for r in 0..rows {
                let row_off = r * cols;
                let mut c = 0usize;
                while c < cols {
                    let idx = row_off + c;
                    if state.new_tile_hashes[idx] == state.last_tile_hashes[idx] {
                        c += 1;
                        continue;
                    }
                    let c_start = c;
                    while c < cols
                        && state.new_tile_hashes[row_off + c] != state.last_tile_hashes[row_off + c]
                    {
                        let bit_idx = row_off + c;
                        state.dirty_tile_bits[bit_idx >> 6] |= 1u64 << (bit_idx & 63);
                        dirty_tiles_count += 1;
                        c += 1;
                    }
                    let rx = (c_start * OVERLAY_TILE_PX) as u32;
                    let ry = (r * OVERLAY_TILE_PX) as u32;
                    let rw = ((c * OVERLAY_TILE_PX) as u32).min(w) - rx;
                    let rh = (((r + 1) * OVERLAY_TILE_PX) as u32).min(h) - ry;
                    if let Some(prev) = state.dirty_upload_rects.last_mut() {
                        if prev.0 == rx && prev.2 == rw && prev.1 + prev.3 == ry {
                            prev.3 += rh;
                            continue;
                        }
                    }
                    state.dirty_upload_rects.push((rx, ry, rw, rh));
                }
            }

            if dirty_tiles_count == 0 {
                return true;
            }
            if dirty_tiles_count * 2 <= total_tiles {
                let Some((_, _, ref mut surf)) = state.overlay_surface else {
                    return false;
                };
                {
                    let Ok(cr) = cairo::Context::new(&*surf) else {
                        return false;
                    };
                    cr.set_font_options(&super::parity_font_options());
                    for &(rx, ry, rw, rh) in &state.dirty_upload_rects {
                        cr.rectangle(rx as f64, ry as f64, rw as f64, rh as f64);
                    }
                    cr.clip();
                    if transparent_bg {
                        cr.set_operator(cairo::Operator::Clear);
                        let _ = cr.paint();
                    } else {
                        cr.set_operator(cairo::Operator::Source);
                        cr.set_source_rgb(0.035, 0.035, 0.043);
                        let _ = cr.paint();
                    }
                    cr.set_operator(cairo::Operator::Over);
                    super::render_display_list_to_cairo_with_dirty_tiles(
                        &cr,
                        list,
                        true,
                        Some((&state.dirty_tile_bits, cols, rows)),
                    );
                    if let Some((mcp_cursor, theme)) = cursor_overlay {
                        if mcp_cursor.fade_opacity > 0.001 {
                            theme.render_cairo(
                                &cr,
                                mcp_cursor.cursor_icon,
                                mcp_cursor.current_x,
                                mcp_cursor.current_y,
                                mcp_cursor.fade_opacity,
                                mcp_cursor.is_down,
                            );
                        }
                    }
                }
                surf.flush();
                let stride = surf.stride() as usize;
                let Ok(data) = surf.data() else {
                    return false;
                };
                state.presenter.upload_ui_overlay_bgra_rects(
                    w,
                    h,
                    stride,
                    &data,
                    &state.dirty_upload_rects,
                    &mut state.upload_scratch,
                );
                std::mem::swap(&mut state.last_tile_hashes, &mut state.new_tile_hashes);
                return true;
            }
        }

        let Some((_, _, ref mut surf)) = state.overlay_surface else {
            return false;
        };
        {
            let Ok(cr) = cairo::Context::new(&*surf) else {
                return false;
            };
            cr.set_font_options(&super::parity_font_options());
            if transparent_bg {
                cr.set_operator(cairo::Operator::Clear);
                let _ = cr.paint();
            } else {
                cr.set_operator(cairo::Operator::Source);
                cr.set_source_rgb(0.035, 0.035, 0.043);
                let _ = cr.paint();
            }
            cr.set_operator(cairo::Operator::Over);
            super::render_display_list_to_cairo_impl(&cr, list, true);
            if let Some((mcp_cursor, theme)) = cursor_overlay {
                if mcp_cursor.fade_opacity > 0.001 {
                    theme.render_cairo(
                        &cr,
                        mcp_cursor.cursor_icon,
                        mcp_cursor.current_x,
                        mcp_cursor.current_y,
                        mcp_cursor.fade_opacity,
                        mcp_cursor.is_down,
                    );
                }
            }
        }
        surf.flush();
        let stride = surf.stride() as usize;
        let row_bytes = (w as usize) * 4;
        let Ok(data) = surf.data() else {
            return false;
        };
        if stride == row_bytes {
            state
                .presenter
                .upload_ui_overlay_bgra(w, h, &data[..row_bytes * (h as usize)]);
        } else {
            state.upload_scratch.resize(row_bytes * (h as usize), 0);
            for row in 0..(h as usize) {
                let src_off = row * stride;
                let dst_off = row * row_bytes;
                state.upload_scratch[dst_off..dst_off + row_bytes]
                    .copy_from_slice(&data[src_off..src_off + row_bytes]);
            }
            state
                .presenter
                .upload_ui_overlay_bgra(w, h, &state.upload_scratch);
        }
        if !has_unbounded {
            state.tile_cols = cols;
            state.tile_rows = rows;
            std::mem::swap(&mut state.last_tile_hashes, &mut state.new_tile_hashes);
        } else {
            state.tile_cols = 0;
            state.tile_rows = 0;
            state.last_tile_hashes.clear();
        }
        true
    }

    pub(super) fn try_present_display_list(
        list: &DisplayList,
        width: f32,
        height: f32,
        overlay_dirty: bool,
        transparent_bg: bool,
        cursor_overlay: Option<(&crate::McpCursorState, &crate::CursorTheme)>,
    ) -> bool {
        let mut viewports: Vec<([f32; 4], Arc<Scene3D>)> = Vec::new();
        for cmd in list.commands() {
            if let DrawCommand::DrawViewport3D { bounds, scene } = cmd {
                if bounds.width > 0.0 && bounds.height > 0.0 {
                    viewports.push((
                        [bounds.x, bounds.y, bounds.width, bounds.height],
                        Arc::clone(scene),
                    ));
                }
            }
        }

        if viewports.is_empty() {
            GPU_SURFACE_STATE.with(|slot| {
                if let Some(ref mut state) = *slot.borrow_mut() {
                    state.set_mapped(false);
                }
            });
            return false;
        }

        let w = width.max(1.0).round() as u32;
        let h = height.max(1.0).round() as u32;
        if GPU_SURFACE_INIT_FAILED.with(|f| f.get()) {
            return false;
        }

        let present_mode = super::ACTIVE_PRESENT_MODE.with(|m| m.get());
        GPU_SURFACE_STATE.with(|slot| {
            let mut cell = slot.borrow_mut();
            if cell.is_none() {
                let da_opt = super::DRAWING_AREA_HANDLE.with(|da| da.borrow().clone());
                let Some(da) = da_opt else {
                    return false;
                };
                match init_state_from_drawing_area(&da, w, h, present_mode) {
                    Ok(state) => *cell = Some(state),
                    Err(e) => {
                        if super::gtk_perf_logging_enabled() {
                            eprintln!("[perf] wgpu X11 swapchain init failed: {e}");
                        }
                        GPU_SURFACE_INIT_FAILED.with(|f| f.set(true));
                        return false;
                    }
                }
            }

            let Some(ref mut state) = *cell else {
                return false;
            };
            state.resize(w, h);
            state.set_mapped(true);

            if overlay_dirty || !state.overlay_uploaded {
                let t_ov0 = std::time::Instant::now();
                let overlay_hash =
                    compute_overlay_hash(list, w, h, transparent_bg, cursor_overlay);
                let t_hash = t_ov0.elapsed().as_secs_f64() * 1000.0;
                if !state.overlay_uploaded || state.last_overlay_hash != Some(overlay_hash) {
                    let t_r0 = std::time::Instant::now();
                    if rasterize_and_upload_overlay(
                        state,
                        list,
                        w,
                        h,
                        transparent_bg,
                        cursor_overlay,
                    ) {
                        state.overlay_uploaded = true;
                        state.last_overlay_hash = Some(overlay_hash);
                    }
                    let t_rast = t_r0.elapsed().as_secs_f64() * 1000.0;
                    if super::gtk_perf_logging_enabled() && (t_hash + t_rast) > 1.2 {
                        eprintln!(
                            "[perf] overlay_upload hash={:.2}ms rast_up={:.2}ms",
                            t_hash, t_rast
                        );
                    }
                }
            }

            state.last_viewports = viewports;
            let vp_refs: Vec<([f32; 4], &Scene3D)> = state
                .last_viewports
                .iter()
                .map(|(vp, sc)| (*vp, sc.as_ref()))
                .collect();
            state
                .presenter
                .present_viewports_to_surface(&vp_refs, state.overlay_uploaded)
                .is_ok()
        })
    }

    pub(super) fn present_active_now() -> bool {
        GPU_SURFACE_STATE.with(|slot| {
            let mut cell = slot.borrow_mut();
            let Some(ref mut state) = *cell else {
                return false;
            };
            if !state.mapped || !state.overlay_uploaded || state.last_viewports.is_empty() {
                return false;
            }
            let vp_refs: Vec<([f32; 4], &Scene3D)> = state
                .last_viewports
                .iter()
                .map(|(vp, sc)| (*vp, sc.as_ref()))
                .collect();
            state
                .presenter
                .present_viewports_to_surface(&vp_refs, state.overlay_uploaded)
                .is_ok()
        })
    }

    pub(super) fn is_active() -> bool {
        GPU_SURFACE_STATE.with(|slot| slot.borrow().as_ref().is_some_and(|s| s.mapped))
    }

    pub(super) fn resize_active(w: u32, h: u32) {
        GPU_SURFACE_STATE.with(|slot| {
            if let Some(ref mut state) = *slot.borrow_mut() {
                state.resize(w, h);
            }
        });
    }

    pub(super) fn destroy_active() {
        GPU_SURFACE_INIT_FAILED.with(|f| f.set(false));
        GPU_SURFACE_STATE.with(|slot| {
            if let Some(state) = slot.borrow_mut().take() {
                state.destroy();
            }
        });
    }
}

/// Attempts direct zero-readback GPU swapchain presentation for `list` on the live GTK window.
///
/// Returns `true` when the active native GPU swapchain rendered and presented the 3D viewport plus
/// cached 2D HUD overlay (`ui_overlay_texture`), allowing the caller to skip Cairo window painting.
pub fn try_present_display_list_gpu(
    list: &DisplayList,
    width: f32,
    height: f32,
    overlay_dirty: bool,
    transparent_bg: bool,
    cursor_overlay: Option<(&crate::McpCursorState, &crate::CursorTheme)>,
) -> bool {
    #[cfg(all(feature = "wgpu-backend", feature = "xlib"))]
    {
        gtk_gpu_x11::try_present_display_list(
            list,
            width,
            height,
            overlay_dirty,
            transparent_bg,
            cursor_overlay,
        )
    }
    #[cfg(not(all(feature = "wgpu-backend", feature = "xlib")))]
    {
        let _ = (
            list,
            width,
            height,
            overlay_dirty,
            transparent_bg,
            cursor_overlay,
        );
        false
    }
}

/// Immediately re-presents the currently mapped 3D scene and cached 2D overlay on the live GPU
/// swapchain without invoking GTK's Cairo draw pass or waiting for GDK's VSync frame clock.
pub fn gtk_present_active_gpu_surface_now() -> bool {
    #[cfg(all(feature = "wgpu-backend", feature = "xlib"))]
    {
        gtk_gpu_x11::present_active_now()
    }
    #[cfg(not(all(feature = "wgpu-backend", feature = "xlib")))]
    {
        false
    }
}

/// Returns `true` when the live GTK window currently has a mapped zero-readback GPU swapchain.
pub fn gtk_gpu_surface_is_active() -> bool {
    #[cfg(all(feature = "wgpu-backend", feature = "xlib"))]
    {
        gtk_gpu_x11::is_active()
    }
    #[cfg(not(all(feature = "wgpu-backend", feature = "xlib")))]
    {
        false
    }
}

/// Posts a platform event to the running GTK application event loop from any background thread.
pub fn post_platform_event(event: crate::events::PlatformEvent) {
    let mut opt = Some(event);
    gtk::glib::idle_add(move || {
        use gtk::prelude::*;
        if let Some(ev) = opt.take() {
            let redraw = EVENT_DISPATCH_CALLBACK.with(|cb| {
                if let Some(ref f) = *cb.borrow() {
                    f(ev)
                } else {
                    false
                }
            });
            if redraw {
                DRAWING_AREA_HANDLE.with(|da| {
                    if let Some(ref d) = *da.borrow() {
                        d.queue_draw();
                    }
                });
            }
        }
        gtk::glib::ControlFlow::Break
    });
}

#[derive(Clone, Copy, Debug)]
struct WindowDragState {
    start_root_x: f64,
    start_root_y: f64,
    start_win_x: i32,
    start_win_y: i32,
    active: bool,
}

thread_local! {
    /// Root-relative position, button and timestamp of the latest button press, as required by
    /// `gtk_window_begin_move_drag`.
    static LAST_PRESS: std::cell::Cell<(f64, f64, u32, u32)> = const { std::cell::Cell::new((0.0, 0.0, 1, 0)) };

    /// Fallback window drag state for environments where the native window manager does not capture pointer grabs.
    static DRAG_STATE: std::cell::Cell<Option<WindowDragState>> = const { std::cell::Cell::new(None) };

    /// Active keyboard modifiers tracked from input events.
    static CURRENT_MODIFIERS: std::cell::Cell<crate::events::Modifiers> = const {
        std::cell::Cell::new(crate::events::Modifiers {
            shift: false,
            ctrl: false,
            alt: false,
            meta: false,
        })
    };
}

/// Returns the currently active keyboard modifier flags.
pub fn gtk_current_modifiers() -> crate::events::Modifiers {
    CURRENT_MODIFIERS.with(|m| m.get())
}

/// Starts a window-manager move for the current press. When the window is maximized it is first
/// restored and repositioned so the pointer keeps its proportional place on the title strip.
pub fn gtk_begin_window_drag(fraction_x: f32, local_y: f32) {
    if IS_WINDOW_DOCKED.with(|d| d.get()) {
        return;
    }
    let (rx, ry, button, time) = LAST_PRESS.with(|p| p.get());
    CURRENT_GTK_WINDOW.with(|w| {
        use gtk::prelude::*;
        if let Some(win) = w.borrow().as_ref() {
            let is_max = win.is_maximized() || IS_MAXIMIZED_TRACKER.with(|t| t.get());
            if is_max {
                let (normal_w, normal_h) = {
                    let (_, _, gw, gh) = WINDOW_NORMAL_GEOMETRY.with(|g| g.get());
                    let (dw, dh) = win.default_size();
                    (if dw > 0 { dw } else { gw }, if dh > 0 { dh } else { gh })
                };
                win.unmaximize();
                IS_MAXIMIZED_TRACKER.with(|t| t.set(false));
                win.resize(normal_w, normal_h);
                let new_x = (rx - fraction_x as f64 * normal_w as f64) as i32;
                let new_y = (ry - local_y as f64) as i32;
                win.move_(new_x, new_y.max(0));
            }
            // Request native window manager interactive movement drag
            win.begin_move_drag(button as i32, rx as i32, ry as i32, time);

            // Record drag state for manual motion fallback if WM does not grab
            let (wx, wy) = win.position();
            DRAG_STATE.with(|d| {
                d.set(Some(WindowDragState {
                    start_root_x: rx,
                    start_root_y: ry,
                    start_win_x: wx,
                    start_win_y: wy,
                    active: true,
                }));
            });
        }
    });
}

/// Computes the next manual window drag position when active and left-button is held,
/// clearing stale `DRAG_STATE` when a WM move grab consumed the release event.
fn step_window_drag_motion(
    button1_held: bool,
    is_docked: bool,
    root_x: f64,
    root_y: f64,
) -> Option<(i32, i32)> {
    DRAG_STATE.with(|d| {
        let ds = d.get()?;
        if !button1_held || is_docked {
            d.set(None);
            return None;
        }
        if ds.active {
            let dx = (root_x - ds.start_root_x) as i32;
            let dy = (root_y - ds.start_root_y) as i32;
            Some((ds.start_win_x + dx, (ds.start_win_y + dy).max(0)))
        } else {
            None
        }
    })
}

thread_local! {
    static IS_CLOSING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Returns true if the native GTK window is in the process of closing.
pub fn gtk_is_closing() -> bool {
    IS_CLOSING.with(|c| c.get())
}

/// Closes the current native GTK desktop window immediately.
pub fn gtk_close() {
    IS_CLOSING.with(|c| c.set(true));
    CURRENT_GTK_WINDOW.with(|w| {
        use gtk::prelude::*;
        if let Some(win) = w.borrow().as_ref() {
            win.close();
        }
        gtk::main_quit();
    });
}

/// Reads current system clipboard text content via GTK.
///
/// Returns `None` when GTK has not been initialized (headless tests, worker
/// threads, or non-GUI processes); GTK's clipboard API panics in that state.
pub fn gtk_get_clipboard_text() -> Option<String> {
    if !gtk::is_initialized() || !gtk::is_initialized_main_thread() {
        return None;
    }
    let clip = gtk::Clipboard::get(&gtk::gdk::SELECTION_CLIPBOARD);
    clip.wait_for_text().map(|s| s.to_string())
}

/// Sets current system clipboard text content via GTK.
///
/// No-op when GTK has not been initialized (headless tests, worker threads, or
/// non-GUI processes); GTK's clipboard API panics in that state.
pub fn gtk_set_clipboard_text(text: &str) {
    if !gtk::is_initialized() || !gtk::is_initialized_main_thread() {
        return;
    }
    let clip = gtk::Clipboard::get(&gtk::gdk::SELECTION_CLIPBOARD);
    clip.set_text(text);
}

thread_local! {
    static CURRENT_CURSOR: std::cell::Cell<Option<crate::window::CursorIcon>> = const { std::cell::Cell::new(None) };
    static PENDING_CURSOR_WARP: std::cell::Cell<Option<(f64, f64)>> = const { std::cell::Cell::new(None) };
}

/// Warps the OS pointer to window-local coordinates `(x, y)` on the active GTK window
/// and marks the target position so the resulting synthetic motion event is filtered.
pub fn gtk_warp_cursor(x: f64, y: f64) {
    PENDING_CURSOR_WARP.with(|w| w.set(Some((x, y))));
    if !gtk::is_initialized() || !gtk::is_initialized_main_thread() {
        return;
    }
    CURRENT_GTK_WINDOW.with(|w| {
        use gtk::prelude::*;
        if let Some(win) = w.borrow().as_ref() {
            if let Some(gdk_win) = win.window() {
                let display = gdk_win.display();
                if let Some(seat) = display.default_seat() {
                    if let Some(pointer) = seat.pointer() {
                        let screen = gdk_win.screen();
                        let (_, origin_x, origin_y) = gdk_win.origin();
                        let target_root_x = origin_x + x.round() as i32;
                        let target_root_y = origin_y + y.round() as i32;
                        pointer.warp(&screen, target_root_x, target_root_y);
                    }
                }
            }
        }
    });
}

/// Alias for [`gtk_warp_cursor`].
pub fn set_cursor_position(x: f64, y: f64) {
    gtk_warp_cursor(x, y);
}

/// Alias for [`gtk_warp_cursor`].
pub fn warp_cursor(x: f64, y: f64) {
    gtk_warp_cursor(x, y);
}

/// Sets the current mouse cursor icon on the active native GTK window.
pub fn gtk_set_cursor(cursor_icon: crate::window::CursorIcon) {
    if CURRENT_CURSOR.with(|c| c.get()) == Some(cursor_icon) {
        return;
    }

    CURRENT_GTK_WINDOW.with(|w| {
        use gtk::prelude::*;
        if let Some(win) = w.borrow().as_ref() {
            if let Some(gdk_win) = win.window() {
                let display = win.display();
                let cursor = match cursor_icon {
                    crate::window::CursorIcon::Pointer => {
                        gtk::gdk::Cursor::from_name(&display, "pointer").or_else(|| {
                            gtk::gdk::Cursor::for_display(&display, gtk::gdk::CursorType::Hand2)
                        })
                    }
                    crate::window::CursorIcon::Text => {
                        gtk::gdk::Cursor::from_name(&display, "text").or_else(|| {
                            gtk::gdk::Cursor::for_display(&display, gtk::gdk::CursorType::Xterm)
                        })
                    }
                    crate::window::CursorIcon::Crosshair => {
                        gtk::gdk::Cursor::from_name(&display, "crosshair").or_else(|| {
                            gtk::gdk::Cursor::for_display(&display, gtk::gdk::CursorType::Crosshair)
                        })
                    }
                    crate::window::CursorIcon::Move => {
                        gtk::gdk::Cursor::from_name(&display, "move").or_else(|| {
                            gtk::gdk::Cursor::for_display(&display, gtk::gdk::CursorType::Fleur)
                        })
                    }
                    crate::window::CursorIcon::NotAllowed => {
                        gtk::gdk::Cursor::from_name(&display, "not-allowed").or_else(|| {
                            gtk::gdk::Cursor::for_display(&display, gtk::gdk::CursorType::Circle)
                        })
                    }
                    crate::window::CursorIcon::Wait => {
                        gtk::gdk::Cursor::from_name(&display, "wait").or_else(|| {
                            gtk::gdk::Cursor::for_display(&display, gtk::gdk::CursorType::Watch)
                        })
                    }
                    crate::window::CursorIcon::Grab => {
                        gtk::gdk::Cursor::from_name(&display, "grab").or_else(|| {
                            gtk::gdk::Cursor::for_display(&display, gtk::gdk::CursorType::Hand1)
                        })
                    }
                    crate::window::CursorIcon::Grabbing => {
                        gtk::gdk::Cursor::from_name(&display, "grabbing").or_else(|| {
                            gtk::gdk::Cursor::for_display(&display, gtk::gdk::CursorType::Hand1)
                        })
                    }
                    crate::window::CursorIcon::ColResize => {
                        gtk::gdk::Cursor::from_name(&display, "col-resize").or_else(|| {
                            gtk::gdk::Cursor::for_display(
                                &display,
                                gtk::gdk::CursorType::SbHDoubleArrow,
                            )
                        })
                    }
                    crate::window::CursorIcon::RowResize => {
                        gtk::gdk::Cursor::from_name(&display, "row-resize").or_else(|| {
                            gtk::gdk::Cursor::for_display(
                                &display,
                                gtk::gdk::CursorType::SbVDoubleArrow,
                            )
                        })
                    }
                    crate::window::CursorIcon::ResizeN | crate::window::CursorIcon::ResizeS => {
                        gtk::gdk::Cursor::from_name(&display, "ns-resize").or_else(|| {
                            gtk::gdk::Cursor::for_display(
                                &display,
                                gtk::gdk::CursorType::SbVDoubleArrow,
                            )
                        })
                    }
                    crate::window::CursorIcon::ResizeE | crate::window::CursorIcon::ResizeW => {
                        gtk::gdk::Cursor::from_name(&display, "ew-resize").or_else(|| {
                            gtk::gdk::Cursor::for_display(
                                &display,
                                gtk::gdk::CursorType::SbHDoubleArrow,
                            )
                        })
                    }
                    crate::window::CursorIcon::ResizeNw | crate::window::CursorIcon::ResizeSe => {
                        gtk::gdk::Cursor::from_name(&display, "nwse-resize").or_else(|| {
                            gtk::gdk::Cursor::for_display(
                                &display,
                                gtk::gdk::CursorType::TopLeftCorner,
                            )
                        })
                    }
                    crate::window::CursorIcon::ResizeNe | crate::window::CursorIcon::ResizeSw => {
                        gtk::gdk::Cursor::from_name(&display, "nesw-resize").or_else(|| {
                            gtk::gdk::Cursor::for_display(
                                &display,
                                gtk::gdk::CursorType::TopRightCorner,
                            )
                        })
                    }
                    crate::window::CursorIcon::Help => {
                        gtk::gdk::Cursor::from_name(&display, "help").or_else(|| {
                            gtk::gdk::Cursor::for_display(
                                &display,
                                gtk::gdk::CursorType::QuestionArrow,
                            )
                        })
                    }
                    crate::window::CursorIcon::None => {
                        gtk::gdk::Cursor::from_name(&display, "none").or_else(|| {
                            gtk::gdk::Cursor::for_display(
                                &display,
                                gtk::gdk::CursorType::BlankCursor,
                            )
                        })
                    }
                    crate::window::CursorIcon::Default => {
                        gtk::gdk::Cursor::from_name(&display, "default").or_else(|| {
                            gtk::gdk::Cursor::for_display(&display, gtk::gdk::CursorType::LeftPtr)
                        })
                    }
                };
                gdk_win.set_cursor(cursor.as_ref());
                DRAWING_AREA_HANDLE.with(|da| {
                    if let Some(area) = da.borrow().as_ref() {
                        if let Some(da_win) = area.window() {
                            da_win.set_cursor(cursor.as_ref());
                        }
                    }
                });
                display.flush();
                CURRENT_CURSOR.with(|c| c.set(Some(cursor_icon)));
            }
        }
    });
}

thread_local! {
    static CURRENT_IM_CONTEXT: std::cell::RefCell<Option<gtk::IMMulticontext>> = const { std::cell::RefCell::new(None) };
    static IME_SURROUNDING_TEXT: std::cell::RefCell<(String, i32)> = const { std::cell::RefCell::new((String::new(), 0)) };
}

/// Sets the candidate window positioning following the active text caret coordinate.
pub fn gtk_set_ime_cursor_location(x: i32, y: i32, width: i32, height: i32) {
    CURRENT_IM_CONTEXT.with(|im| {
        use gtk::prelude::*;
        if let Some(ctx) = im.borrow().as_ref() {
            let rect = gtk::gdk::Rectangle::new(x, y, width, height);
            ctx.set_cursor_location(&rect);
        }
    });
}

/// Updates the surrounding text buffer and cursor index for context-aware IME prediction.
pub fn gtk_set_ime_surrounding_text(text: &str, cursor_index: i32) {
    IME_SURROUNDING_TEXT.with(|st| {
        *st.borrow_mut() = (text.to_string(), cursor_index);
    });
    CURRENT_IM_CONTEXT.with(|im| {
        use gtk::prelude::*;
        if let Some(ctx) = im.borrow().as_ref() {
            ctx.set_surrounding(text, cursor_index);
        }
    });
}

/// Resets or cancels any active IME composition session.
pub fn gtk_reset_ime() {
    CURRENT_IM_CONTEXT.with(|im| {
        use gtk::prelude::*;
        if let Some(ctx) = im.borrow().as_ref() {
            ctx.reset();
        }
    });
}

/// Maps a GDK keyval name to an internal KeyCode.
fn map_key_name_to_keycode(key_name: &str) -> crate::events::KeyCode {
    match key_name {
        "a" | "A" => crate::events::KeyCode::KeyA,
        "b" | "B" => crate::events::KeyCode::KeyB,
        "c" | "C" => crate::events::KeyCode::KeyC,
        "d" | "D" => crate::events::KeyCode::KeyD,
        "e" | "E" => crate::events::KeyCode::KeyE,
        "f" | "F" => crate::events::KeyCode::KeyF,
        "g" | "G" => crate::events::KeyCode::KeyG,
        "h" | "H" => crate::events::KeyCode::KeyH,
        "i" | "I" => crate::events::KeyCode::KeyI,
        "j" | "J" => crate::events::KeyCode::KeyJ,
        "k" | "K" => crate::events::KeyCode::KeyK,
        "l" | "L" => crate::events::KeyCode::KeyL,
        "m" | "M" => crate::events::KeyCode::KeyM,
        "n" | "N" => crate::events::KeyCode::KeyN,
        "o" | "O" => crate::events::KeyCode::KeyO,
        "p" | "P" => crate::events::KeyCode::KeyP,
        "q" | "Q" => crate::events::KeyCode::KeyQ,
        "r" | "R" => crate::events::KeyCode::KeyR,
        "s" | "S" => crate::events::KeyCode::KeyS,
        "t" | "T" => crate::events::KeyCode::KeyT,
        "u" | "U" => crate::events::KeyCode::KeyU,
        "v" | "V" => crate::events::KeyCode::KeyV,
        "w" | "W" => crate::events::KeyCode::KeyW,
        "x" | "X" => crate::events::KeyCode::KeyX,
        "y" | "Y" => crate::events::KeyCode::KeyY,
        "z" | "Z" => crate::events::KeyCode::KeyZ,
        "0" => crate::events::KeyCode::Digit0,
        "1" => crate::events::KeyCode::Digit1,
        "2" => crate::events::KeyCode::Digit2,
        "3" => crate::events::KeyCode::Digit3,
        "4" => crate::events::KeyCode::Digit4,
        "5" => crate::events::KeyCode::Digit5,
        "6" => crate::events::KeyCode::Digit6,
        "7" => crate::events::KeyCode::Digit7,
        "8" => crate::events::KeyCode::Digit8,
        "9" => crate::events::KeyCode::Digit9,
        "BackSpace" => crate::events::KeyCode::Backspace,
        "Return" => crate::events::KeyCode::Enter,
        "Tab" => crate::events::KeyCode::Tab,
        "Escape" => crate::events::KeyCode::Escape,
        "Delete" => crate::events::KeyCode::Delete,
        "Left" => crate::events::KeyCode::ArrowLeft,
        "Right" => crate::events::KeyCode::ArrowRight,
        "Up" => crate::events::KeyCode::ArrowUp,
        "Down" => crate::events::KeyCode::ArrowDown,
        "Home" => crate::events::KeyCode::Home,
        "End" => crate::events::KeyCode::End,
        "Page_Up" => crate::events::KeyCode::PageUp,
        "Page_Down" => crate::events::KeyCode::PageDown,
        "F1" => crate::events::KeyCode::F1,
        "F2" => crate::events::KeyCode::F2,
        "F3" => crate::events::KeyCode::F3,
        "F4" => crate::events::KeyCode::F4,
        "F5" => crate::events::KeyCode::F5,
        "F6" => crate::events::KeyCode::F6,
        "F7" => crate::events::KeyCode::F7,
        "F8" => crate::events::KeyCode::F8,
        "F9" => crate::events::KeyCode::F9,
        "F10" => crate::events::KeyCode::F10,
        "F11" => crate::events::KeyCode::F11,
        "F12" => crate::events::KeyCode::F12,
        "space" => crate::events::KeyCode::Space,
        _ => crate::events::KeyCode::Unknown,
    }
}

/// Fallback mapping from Linux X11/Wayland evdev+8 hardware keycodes so shifted symbols
/// (e.g. `Shift+1` -> `"exclam"`) and non-Latin layouts still resolve physical keys.
fn map_evdev_hardware_keycode(hw_code: u16) -> crate::events::KeyCode {
    match hw_code {
        9 => crate::events::KeyCode::Escape,
        10 => crate::events::KeyCode::Digit1,
        11 => crate::events::KeyCode::Digit2,
        12 => crate::events::KeyCode::Digit3,
        13 => crate::events::KeyCode::Digit4,
        14 => crate::events::KeyCode::Digit5,
        15 => crate::events::KeyCode::Digit6,
        16 => crate::events::KeyCode::Digit7,
        17 => crate::events::KeyCode::Digit8,
        18 => crate::events::KeyCode::Digit9,
        19 => crate::events::KeyCode::Digit0,
        22 => crate::events::KeyCode::Backspace,
        23 => crate::events::KeyCode::Tab,
        24 => crate::events::KeyCode::KeyQ,
        25 => crate::events::KeyCode::KeyW,
        26 => crate::events::KeyCode::KeyE,
        27 => crate::events::KeyCode::KeyR,
        28 => crate::events::KeyCode::KeyT,
        29 => crate::events::KeyCode::KeyY,
        30 => crate::events::KeyCode::KeyU,
        31 => crate::events::KeyCode::KeyI,
        32 => crate::events::KeyCode::KeyO,
        33 => crate::events::KeyCode::KeyP,
        36 => crate::events::KeyCode::Enter,
        38 => crate::events::KeyCode::KeyA,
        39 => crate::events::KeyCode::KeyS,
        40 => crate::events::KeyCode::KeyD,
        41 => crate::events::KeyCode::KeyF,
        42 => crate::events::KeyCode::KeyG,
        43 => crate::events::KeyCode::KeyH,
        44 => crate::events::KeyCode::KeyJ,
        45 => crate::events::KeyCode::KeyK,
        46 => crate::events::KeyCode::KeyL,
        52 => crate::events::KeyCode::KeyZ,
        53 => crate::events::KeyCode::KeyX,
        54 => crate::events::KeyCode::KeyC,
        55 => crate::events::KeyCode::KeyV,
        56 => crate::events::KeyCode::KeyB,
        57 => crate::events::KeyCode::KeyN,
        58 => crate::events::KeyCode::KeyM,
        65 => crate::events::KeyCode::Space,
        67 => crate::events::KeyCode::F1,
        68 => crate::events::KeyCode::F2,
        69 => crate::events::KeyCode::F3,
        70 => crate::events::KeyCode::F4,
        71 => crate::events::KeyCode::F5,
        72 => crate::events::KeyCode::F6,
        73 => crate::events::KeyCode::F7,
        74 => crate::events::KeyCode::F8,
        75 => crate::events::KeyCode::F9,
        76 => crate::events::KeyCode::F10,
        95 => crate::events::KeyCode::F11,
        96 => crate::events::KeyCode::F12,
        110 => crate::events::KeyCode::Home,
        111 => crate::events::KeyCode::ArrowUp,
        112 => crate::events::KeyCode::PageUp,
        113 => crate::events::KeyCode::ArrowLeft,
        114 => crate::events::KeyCode::ArrowRight,
        115 => crate::events::KeyCode::End,
        116 => crate::events::KeyCode::ArrowDown,
        117 => crate::events::KeyCode::PageDown,
        119 => crate::events::KeyCode::Delete,
        _ => crate::events::KeyCode::Unknown,
    }
}

#[inline]
fn resolve_gtk_keycode(key_name: &str, hardware_keycode: u16) -> crate::events::KeyCode {
    let by_name = map_key_name_to_keycode(key_name);
    if by_name != crate::events::KeyCode::Unknown {
        by_name
    } else {
        map_evdev_hardware_keycode(hardware_keycode)
    }
}

/// Returns the native refresh interval of the active window's monitor (or primary monitor) from GDK.
///
/// Queries `gdk::Monitor::refresh_rate()` (reported in millihertz, e.g. `144_000` for 144 Hz,
/// returning `~6.944 ms`). Falls back to `16 ms` (~60 Hz) when GDK is not initialized or the
/// display server does not report a positive refresh rate.
pub fn gtk_primary_refresh_interval() -> std::time::Duration {
    use gtk::gdk::prelude::MonitorExt;
    if let Some(display) = gtk::gdk::Display::default() {
        let monitor = CURRENT_GTK_WINDOW
            .with(|w| {
                use gtk::prelude::WidgetExt;
                w.borrow()
                    .as_ref()
                    .and_then(|win| win.window())
                    .and_then(|gdk_win| display.monitor_at_window(&gdk_win))
            })
            .or_else(|| display.primary_monitor())
            .or_else(|| display.monitor(0));
        if let Some(m) = monitor {
            let refresh_mhz = m.refresh_rate();
            if refresh_mhz > 0 {
                let nanos = (1_000_000_000_000u64 / (refresh_mhz as u64)).max(100_000);
                return std::time::Duration::from_nanos(nanos);
            }
        }
    }
    std::time::Duration::from_millis(16)
}

fn query_primary_workarea() -> Option<(i32, i32, i32, i32)> {
    use gtk::gdk::prelude::MonitorExt;
    let display = gtk::gdk::Display::default()?;
    let monitor = display.primary_monitor().or_else(|| display.monitor(0))?;
    let workarea = monitor.workarea();
    Some((
        workarea.x(),
        workarea.y(),
        workarea.width(),
        workarea.height(),
    ))
}

fn snap_window_to_anchor(
    win: &gtk::Window,
    anchor: crate::window::WindowAnchor,
    fallback_w: i32,
    fallback_h: i32,
) {
    use gtk::prelude::*;
    if let Some((wx, wy, ww, wh)) = query_primary_workarea() {
        let (sw, sh) = win.size();
        let w = if sw > 0 { sw } else { fallback_w };
        let h = if sh > 0 { sh } else { fallback_h };
        let (tx, ty) = anchor.compute_position(wx, wy, ww, wh, w, h);
        if win.position() != (tx, ty) {
            win.move_(tx, ty);
        }
    }
}

fn attach_rgba_visual(win: &gtk::Window) {
    use gtk::prelude::*;
    if let Some(screen) = gtk::prelude::WidgetExt::screen(win) {
        if let Some(visual) = screen.rgba_visual() {
            win.set_visual(Some(&visual));
        }
    }
}

/// Launches a live native GTK3 desktop window with Cairo graphics presentation.
pub fn launch_gtk_window<F, E>(
    title: &str,
    width: u32,
    height: u32,
    decorated: bool,
    on_draw: F,
    on_event: E,
) -> Result<(), String>
where
    F: FnMut(f32, f32, &cairo::Context) + 'static,
    E: FnMut(&crate::events::PlatformEvent) -> bool + 'static,
{
    let config = crate::window::WindowConfig {
        title: title.to_string(),
        width,
        height,
        decoration: if decorated {
            crate::window::WindowDecoration::NativeOs
        } else {
            crate::window::WindowDecoration::ClientSideDecorations
        },
        ..Default::default()
    };
    launch_gtk_window_with_config(&config, on_draw, on_event)
}

/// Launches a live native GTK3 desktop window honoring the full [`crate::window::WindowConfig`]
/// (resizability, always-on-top, RGBA visual transparency, window mode, and monitor workarea docking).
pub fn launch_gtk_window_with_config<F, E>(
    config: &crate::window::WindowConfig,
    on_draw: F,
    on_event: E,
) -> Result<(), String>
where
    F: FnMut(f32, f32, &cairo::Context) + 'static,
    E: FnMut(&crate::events::PlatformEvent) -> bool + 'static,
{
    launch_gtk_window_with_scheduler(config, on_draw, on_event, || None)
}

/// Launches a live native GTK3 desktop window with a unified wakeup scheduler.
///
/// `next_wakeup` is queried after every input event and every tick to determine
/// when the next [`crate::events::PlatformEvent::AnimationTick`] should fire.
/// Returning `None` suspends the tick timer completely so an idle window wakes
/// zero times per second.
pub fn launch_gtk_window_with_scheduler<F, E, S>(
    config: &crate::window::WindowConfig,
    on_draw: F,
    on_event: E,
    next_wakeup: S,
) -> Result<(), String>
where
    F: FnMut(f32, f32, &cairo::Context) + 'static,
    E: FnMut(&crate::events::PlatformEvent) -> bool + 'static,
    S: FnMut() -> Option<std::time::Duration> + 'static,
{
    use gtk::prelude::*;

    if gtk::init().is_err() {
        return Err("Failed to initialize GTK on current DISPLAY".to_string());
    }

    IS_CLOSING.with(|c| c.set(false));
    ACTIVE_PRESENT_MODE.with(|m| m.set(config.present_mode));

    let width = config.width.max(1);
    let height = config.height.max(1);
    let win_w = width as i32;
    let win_h = height as i32;
    let effective_anchor = config.effective_anchor();
    let docked = config.is_docked();
    let decorated = config.decoration == crate::window::WindowDecoration::NativeOs;
    let transparent = config.transparent
        || config.decoration == crate::window::WindowDecoration::FramelessTransparent;
    let resizable = if docked { false } else { config.resizable };
    let always_on_top = docked || config.always_on_top;

    IS_WINDOW_DOCKED.with(|d| d.set(docked));

    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title(&config.title);
    window.set_default_size(win_w, win_h);
    if resizable {
        let min_w = config.min_width.unwrap_or(1).max(1) as i32;
        let min_h = config.min_height.unwrap_or(1).max(1) as i32;
        window.set_size_request(min_w, min_h);
    } else {
        window.set_size_request(win_w, win_h);
    }
    window.resize(win_w, win_h);
    window.set_decorated(decorated);
    window.set_resizable(resizable);
    window.set_keep_above(always_on_top);
    window.add_events(
        gtk::gdk::EventMask::STRUCTURE_MASK
            | gtk::gdk::EventMask::KEY_PRESS_MASK
            | gtk::gdk::EventMask::KEY_RELEASE_MASK
            | gtk::gdk::EventMask::FOCUS_CHANGE_MASK,
    );

    if transparent {
        window.set_app_paintable(true);
        attach_rgba_visual(&window);
        window.connect_screen_changed(|win, _prev_screen| {
            attach_rgba_visual(win);
        });
    }

    match effective_anchor {
        Some(crate::window::WindowAnchor::Center) | None => {
            window.set_position(gtk::WindowPosition::Center);
        }
        Some(anchor) => {
            let gravity = match anchor {
                crate::window::WindowAnchor::TopLeft => gtk::gdk::Gravity::NorthWest,
                crate::window::WindowAnchor::TopRight => gtk::gdk::Gravity::NorthEast,
                crate::window::WindowAnchor::BottomLeft => gtk::gdk::Gravity::SouthWest,
                crate::window::WindowAnchor::BottomRight => gtk::gdk::Gravity::SouthEast,
                crate::window::WindowAnchor::Center => gtk::gdk::Gravity::Center,
            };
            window.set_gravity(gravity);
            snap_window_to_anchor(&window, anchor, win_w, win_h);
        }
    }

    if !docked {
        match config.mode {
            crate::window::WindowMode::Maximized => {
                IS_MAXIMIZED_TRACKER.with(|t| t.set(true));
                window.maximize();
            }
            crate::window::WindowMode::Minimized => {
                window.iconify();
            }
            crate::window::WindowMode::BorderlessFullscreen
            | crate::window::WindowMode::ExclusiveFullscreen => {
                window.fullscreen();
            }
            crate::window::WindowMode::Windowed => {}
        }
    }

    CURRENT_GTK_WINDOW.with(|w| {
        *w.borrow_mut() = Some(window.clone());
    });

    let drawing_area = gtk::DrawingArea::new();
    if transparent {
        drawing_area.set_app_paintable(true);
    }
    drawing_area.add_events(
        gtk::gdk::EventMask::BUTTON_PRESS_MASK
            | gtk::gdk::EventMask::BUTTON_RELEASE_MASK
            | gtk::gdk::EventMask::POINTER_MOTION_MASK
            | gtk::gdk::EventMask::KEY_PRESS_MASK
            | gtk::gdk::EventMask::KEY_RELEASE_MASK
            | gtk::gdk::EventMask::FOCUS_CHANGE_MASK
            | gtk::gdk::EventMask::STRUCTURE_MASK
            | gtk::gdk::EventMask::ENTER_NOTIFY_MASK
            | gtk::gdk::EventMask::LEAVE_NOTIFY_MASK
            | gtk::gdk::EventMask::SCROLL_MASK
            | gtk::gdk::EventMask::SMOOTH_SCROLL_MASK,
    );
    drawing_area.set_can_focus(true);
    drawing_area.set_hexpand(true);
    drawing_area.set_vexpand(true);
    if resizable {
        let min_w = config.min_width.unwrap_or(1).max(1) as i32;
        let min_h = config.min_height.unwrap_or(1).max(1) as i32;
        drawing_area.set_size_request(min_w, min_h);
    } else {
        drawing_area.set_size_request(win_w, win_h);
    }

    // Initialize OS Input Method Editor (IME) context for CJK and international text entry
    let im_context = gtk::IMMulticontext::new();
    let ime_composing = std::rc::Rc::new(std::cell::Cell::new(false));
    CURRENT_IM_CONTEXT.with(|im| {
        *im.borrow_mut() = Some(im_context.clone());
    });

    let im_realize = im_context.clone();
    drawing_area.connect_realize(move |widget| {
        use gtk::prelude::*;
        if let Some(gdk_win) = widget.window() {
            im_realize.set_client_window(Some(&gdk_win));
        }
    });

    let im_unrealize = im_context.clone();
    drawing_area.connect_unrealize(move |_widget| {
        use gtk::prelude::*;
        #[cfg(all(feature = "wgpu-backend", feature = "xlib"))]
        gtk_gpu_x11::destroy_active();
        im_unrealize.set_client_window(None::<&gtk::gdk::Window>);
    });

    let im_focus_in = im_context.clone();
    drawing_area.connect_focus_in_event(move |_widget, _event| {
        use gtk::prelude::*;
        im_focus_in.focus_in();
        gtk::glib::Propagation::Proceed
    });

    let im_focus_out = im_context.clone();
    let ime_composing_focus_out = ime_composing.clone();
    drawing_area.connect_focus_out_event(move |_widget, _event| {
        use gtk::prelude::*;
        DRAG_STATE.with(|d| d.set(None));
        ime_composing_focus_out.set(false);
        im_focus_out.focus_out();
        gtk::glib::Propagation::Proceed
    });

    drawing_area.connect_grab_broken_event(|_widget, _event| {
        DRAG_STATE.with(|d| d.set(None));
        gtk::glib::Propagation::Proceed
    });

    let on_draw_rc = std::rc::Rc::new(std::cell::RefCell::new(on_draw));
    let on_draw_cb = on_draw_rc.clone();
    drawing_area.connect_draw(move |widget, cr| {
        let w = widget.allocated_width() as f32;
        let h = widget.allocated_height() as f32;
        // Live paint must use the same font options as measurement.
        cr.set_font_options(&parity_font_options());
        CURRENT_DRAWING_AREA.with(|da| *da.borrow_mut() = Some(widget.clone()));
        IN_LIVE_GTK_DRAW.with(|live| live.set(true));
        (on_draw_cb.borrow_mut())(w, h, cr);
        IN_LIVE_GTK_DRAW.with(|live| live.set(false));
        gtk::glib::Propagation::Proceed
    });

    let tick_gen = std::rc::Rc::new(std::cell::Cell::new(0u64));
    let armed_deadline: std::rc::Rc<std::cell::Cell<Option<std::time::Instant>>> =
        std::rc::Rc::new(std::cell::Cell::new(None));
    let next_wakeup_rc: std::rc::Rc<
        std::cell::RefCell<dyn FnMut() -> Option<std::time::Duration>>,
    > = std::rc::Rc::new(std::cell::RefCell::new(next_wakeup));

    fn arm_next_tick(
        da: gtk::DrawingArea,
        on_event: std::rc::Weak<
            std::cell::RefCell<dyn FnMut(&crate::events::PlatformEvent) -> bool>,
        >,
        next_wakeup: std::rc::Rc<std::cell::RefCell<dyn FnMut() -> Option<std::time::Duration>>>,
        tick_gen: std::rc::Rc<std::cell::Cell<u64>>,
        armed_deadline: std::rc::Rc<std::cell::Cell<Option<std::time::Instant>>>,
    ) {
        let delay = (next_wakeup.borrow_mut())();
        let now = std::time::Instant::now();
        let (target_deadline, delay) = match compute_tick_rearm_decision(
            armed_deadline.get(),
            now,
            delay,
        ) {
            TickRearmDecision::KeepPending => return,
            TickRearmDecision::CancelPending => {
                armed_deadline.set(None);
                tick_gen.set(tick_gen.get().wrapping_add(1));
                return;
            }
            TickRearmDecision::ArmAt(target, d) => (target, d),
        };
        let next_gen = tick_gen.get().wrapping_add(1);
        tick_gen.set(next_gen);
        armed_deadline.set(Some(target_deadline));
        let armed_tick = armed_deadline.clone();
        let tick_gen_run = tick_gen.clone();
        let run_tick = move || {
            if IS_CLOSING.with(|c| c.get()) || tick_gen_run.get() != next_gen {
                return;
            }
            armed_tick.set(None);
            let Some(on_event_strong) = on_event.upgrade() else {
                return;
            };
            let tick_ev = crate::events::PlatformEvent::AnimationTick;
            let changed = (on_event_strong.borrow_mut())(&tick_ev);
            if changed {
                use gtk::prelude::WidgetExt;
                da.queue_draw();
            }
            arm_next_tick(da, on_event, next_wakeup, tick_gen_run, armed_tick);
        };
        if delay.is_zero() {
            let mut run_opt = Some(run_tick);
            // Must be strictly greater (lower priority) than GTK_PRIORITY_RESIZE (110)
            // and GDK_PRIORITY_REDRAW (120) so continuous 0ms wakeups cannot starve
            // initial/pending layout or connect_draw, while still beating DEFAULT_IDLE (200).
            const ZERO_DELAY_TICK_PRIORITY: i32 = 130;
            gtk::glib::idle_add_local_full(
                gtk::glib::Priority::from(ZERO_DELAY_TICK_PRIORITY),
                move || {
                    if let Some(f) = run_opt.take() {
                        f();
                    }
                    gtk::glib::ControlFlow::Break
                },
            );
        } else {
            let rounded_ms = ((delay.as_micros() + 500) / 1000).max(1) as u64;
            gtk::glib::timeout_add_local_once(
                std::time::Duration::from_millis(rounded_ms),
                run_tick,
            );
        }
    }

    let on_event_rc: std::rc::Rc<
        std::cell::RefCell<dyn FnMut(&crate::events::PlatformEvent) -> bool>,
    > = std::rc::Rc::new(std::cell::RefCell::new(on_event));

    // Only re-arm when no AnimationTick is currently pending or the newly requested wakeup is strictly earlier.
    let rearm_da = drawing_area.clone();
    let rearm_weak = std::rc::Rc::downgrade(&on_event_rc);
    let rearm_wakeup = next_wakeup_rc.clone();
    let rearm_gen = tick_gen.clone();
    let rearm_deadline = armed_deadline.clone();
    let inner_rc = on_event_rc.clone();
    let inner_weak = std::rc::Rc::downgrade(&inner_rc);
    let on_event_rc: std::rc::Rc<
        std::cell::RefCell<dyn FnMut(&crate::events::PlatformEvent) -> bool>,
    > = std::rc::Rc::new(std::cell::RefCell::new(
        move |event: &crate::events::PlatformEvent| -> bool {
            let changed = (inner_rc.borrow_mut())(event);
            if !matches!(event, crate::events::PlatformEvent::AnimationTick) {
                arm_next_tick(
                    rearm_da.clone(),
                    rearm_weak.clone(),
                    rearm_wakeup.clone(),
                    rearm_gen.clone(),
                    rearm_deadline.clone(),
                );
            }
            changed
        },
    ));

    let last_allocated_size = std::cell::Cell::new((0u32, 0u32));
    let on_event_resize = on_event_rc.clone();
    drawing_area.connect_size_allocate(move |widget, alloc| {
        use gtk::prelude::WidgetExt;
        let w = alloc.width().max(1) as u32;
        let h = alloc.height().max(1) as u32;
        #[cfg(all(feature = "wgpu-backend", feature = "xlib"))]
        gtk_gpu_x11::resize_active(w, h);
        if last_allocated_size.get() != (w, h) {
            last_allocated_size.set((w, h));
            let ev = crate::events::PlatformEvent::WindowResized {
                width: w,
                height: h,
            };
            if let Ok(mut cb) = on_event_resize.try_borrow_mut() {
                let _ = (*cb)(&ev);
            }
            widget.queue_draw();
        }
    });

    DRAWING_AREA_HANDLE.with(|da| {
        *da.borrow_mut() = Some(drawing_area.clone());
    });
    let on_event_dispatch = on_event_rc.clone();
    EVENT_DISPATCH_CALLBACK.with(|cb| {
        *cb.borrow_mut() = Some(Box::new(move |ev| (on_event_dispatch.borrow_mut())(&ev)));
    });

    // OS file drag-and-drop destination handler (`text/uri-list` / `text/plain`)
    {
        use gtk::prelude::{DragContextExtManual, WidgetExt, WidgetExtManual};
        let targets = [
            gtk::TargetEntry::new("text/uri-list", gtk::TargetFlags::OTHER_APP, 0),
            gtk::TargetEntry::new("text/plain", gtk::TargetFlags::OTHER_APP, 1),
        ];
        drawing_area.drag_dest_set(
            gtk::DestDefaults::ALL,
            &targets,
            gtk::gdk::DragAction::COPY | gtk::gdk::DragAction::MOVE,
        );
        let on_event_drop = on_event_rc.clone();
        drawing_area.connect_drag_data_received(
            move |widget, context, x, y, selection_data, _info, time| {
                let mut paths = Vec::new();
                for uri in selection_data.uris() {
                    if let Some(p) = crate::events::decode_file_uri_to_path(uri.as_str()) {
                        paths.push(p);
                    }
                }
                if paths.is_empty() {
                    if let Some(text) = selection_data.text() {
                        paths = crate::events::parse_dropped_uri_list_to_paths(text.as_str());
                    }
                }
                if !paths.is_empty() {
                    let move_ev = crate::events::PlatformEvent::MouseMove {
                        x: x as f32,
                        y: y as f32,
                    };
                    let drop_ev = crate::events::PlatformEvent::FileDrop(paths);
                    let mut redraw = false;
                    if let Ok(mut cb) = on_event_drop.try_borrow_mut() {
                        if (*cb)(&move_ev) {
                            redraw = true;
                        }
                        if (*cb)(&drop_ev) {
                            redraw = true;
                        }
                    }
                    if redraw {
                        widget.queue_draw();
                    }
                    context.drag_finish(true, false, time);
                } else {
                    context.drag_finish(false, false, time);
                }
            },
        );
    }

    // Mouse button click handler
    let frameless_resizable =
        resizable && config.decoration == crate::window::WindowDecoration::FramelessTransparent;
    let window_for_resize = window.clone();
    let on_event_mouse = on_event_rc.clone();
    drawing_area.connect_button_press_event(move |widget, event| {
        use gtk::prelude::GtkWindowExt;
        use gtk::prelude::WidgetExt;
        let (x, y) = event.position();
        let (root_x, root_y) = event.root();

        if frameless_resizable
            && event.button() == 1
            && event.event_type() == gtk::gdk::EventType::ButtonPress
        {
            let alloc = widget.allocation();
            if let Some((edge, _)) = frameless_resize_edge(x, y, alloc.width(), alloc.height()) {
                window_for_resize.begin_resize_drag(
                    edge,
                    1,
                    root_x as i32,
                    root_y as i32,
                    event.time(),
                );
                return gtk::glib::Propagation::Stop;
            }
        }

        LAST_PRESS.with(|p| p.set((root_x, root_y, event.button(), event.time())));

        let state = event.state();
        let modifiers = crate::events::Modifiers {
            shift: state.contains(gtk::gdk::ModifierType::SHIFT_MASK),
            ctrl: state.contains(gtk::gdk::ModifierType::CONTROL_MASK),
            alt: state.contains(gtk::gdk::ModifierType::MOD1_MASK),
            meta: state.contains(gtk::gdk::ModifierType::META_MASK)
                || state.contains(gtk::gdk::ModifierType::SUPER_MASK),
        };
        CURRENT_MODIFIERS.with(|m| m.set(modifiers));

        let btn = match event.button() {
            1 => crate::events::MouseButton::Left,
            2 => crate::events::MouseButton::Middle,
            3 => crate::events::MouseButton::Right,
            b => crate::events::MouseButton::Other(b as u16),
        };

        let plat_event = match event.event_type() {
            gtk::gdk::EventType::DoubleButtonPress => crate::events::PlatformEvent::DoubleClick {
                button: btn,
                x: x as f32,
                y: y as f32,
            },
            gtk::gdk::EventType::TripleButtonPress => return gtk::glib::Propagation::Proceed,
            _ => crate::events::PlatformEvent::MouseDown {
                button: btn,
                x: x as f32,
                y: y as f32,
            },
        };

        if (on_event_mouse.borrow_mut())(&plat_event) {
            widget.queue_draw();
        }

        gtk::glib::Propagation::Proceed
    });

    // Mouse button release handler
    let on_event_release = on_event_rc.clone();
    drawing_area.connect_button_release_event(move |widget, event| {
        // Clear manual window drag state upon mouse release
        DRAG_STATE.with(|d| d.set(None));

        let (x, y) = event.position();
        let state = event.state();
        let modifiers = crate::events::Modifiers {
            shift: state.contains(gtk::gdk::ModifierType::SHIFT_MASK),
            ctrl: state.contains(gtk::gdk::ModifierType::CONTROL_MASK),
            alt: state.contains(gtk::gdk::ModifierType::MOD1_MASK),
            meta: state.contains(gtk::gdk::ModifierType::META_MASK)
                || state.contains(gtk::gdk::ModifierType::SUPER_MASK),
        };
        CURRENT_MODIFIERS.with(|m| m.set(modifiers));

        let btn = match event.button() {
            1 => crate::events::MouseButton::Left,
            2 => crate::events::MouseButton::Middle,
            3 => crate::events::MouseButton::Right,
            b => crate::events::MouseButton::Other(b as u16),
        };

        let plat_event = crate::events::PlatformEvent::MouseUp {
            button: btn,
            x: x as f32,
            y: y as f32,
        };

        if (on_event_release.borrow_mut())(&plat_event) {
            widget.queue_draw();
        }

        gtk::glib::Propagation::Proceed
    });

    // Pointer motion handler
    let on_event_motion = on_event_rc.clone();
    drawing_area.connect_motion_notify_event(move |widget, event| {
        use gtk::prelude::WidgetExt;
        let (x, y) = event.position();
        let (root_x, root_y) = event.root();

        let state = event.state();
        let modifiers = crate::events::Modifiers {
            shift: state.contains(gtk::gdk::ModifierType::SHIFT_MASK),
            ctrl: state.contains(gtk::gdk::ModifierType::CONTROL_MASK),
            alt: state.contains(gtk::gdk::ModifierType::MOD1_MASK),
            meta: state.contains(gtk::gdk::ModifierType::META_MASK)
                || state.contains(gtk::gdk::ModifierType::SUPER_MASK),
        };
        CURRENT_MODIFIERS.with(|m| m.set(modifiers));

        // If manual window drag fallback is active, update window position only while left button is held
        if let Some((next_x, next_y)) = step_window_drag_motion(
            state.contains(gtk::gdk::ModifierType::BUTTON1_MASK),
            IS_WINDOW_DOCKED.with(|d| d.get()),
            root_x,
            root_y,
        ) {
            CURRENT_GTK_WINDOW.with(|w| {
                use gtk::prelude::*;
                if let Some(win) = w.borrow().as_ref() {
                    win.move_(next_x, next_y);
                }
            });
        }

        if IS_CLOSING.with(|c| c.get()) {
            return gtk::glib::Propagation::Proceed;
        }

        let is_synthetic_warp = PENDING_CURSOR_WARP.with(|w| {
            if let Some((wx, wy)) = w.get() {
                if (x - wx).abs() <= 2.5 && (y - wy).abs() <= 2.5 {
                    w.set(None);
                    return true;
                }
            }
            false
        });
        if is_synthetic_warp {
            return gtk::glib::Propagation::Proceed;
        }

        let plat_event = crate::events::PlatformEvent::MouseMove {
            x: x as f32,
            y: y as f32,
        };

        if (on_event_motion.borrow_mut())(&plat_event) {
            widget.queue_draw();
        }

        if frameless_resizable {
            let alloc = widget.allocation();
            if let Some((_, cursor)) = frameless_resize_edge(x, y, alloc.width(), alloc.height()) {
                gtk_set_cursor(cursor);
            }
        }

        gtk::glib::Propagation::Proceed
    });

    let on_event_leave = on_event_rc.clone();
    drawing_area.connect_leave_notify_event(move |widget, event| {
        if event.mode() != gtk::gdk::CrossingMode::Normal {
            return gtk::glib::Propagation::Proceed;
        }
        let plat_event = crate::events::PlatformEvent::CursorLeft;
        if (on_event_leave.borrow_mut())(&plat_event) {
            widget.queue_draw();
        }
        gtk::glib::Propagation::Proceed
    });

    let on_event_enter = on_event_rc.clone();
    drawing_area.connect_enter_notify_event(move |widget, _event| {
        let plat_event = crate::events::PlatformEvent::CursorEntered;
        if (on_event_enter.borrow_mut())(&plat_event) {
            widget.queue_draw();
        }
        gtk::glib::Propagation::Proceed
    });

    let on_event_scroll = on_event_rc.clone();
    drawing_area.connect_scroll_event(move |widget, event| {
        let (x, y) = event.position();
        let (mut dx, mut dy) = (0.0f32, 0.0f32);
        if let Some((smooth_dx, smooth_dy)) = event.scroll_deltas() {
            dx = smooth_dx as f32 * 110.0;
            dy = smooth_dy as f32 * 110.0;
        } else {
            match event.direction() {
                gtk::gdk::ScrollDirection::Up => dy = -110.0,
                gtk::gdk::ScrollDirection::Down => dy = 110.0,
                gtk::gdk::ScrollDirection::Left => dx = -110.0,
                gtk::gdk::ScrollDirection::Right => dx = 110.0,
                _ => {}
            }
        }

        let move_ev = crate::events::PlatformEvent::MouseMove {
            x: x as f32,
            y: y as f32,
        };
        let _ = (on_event_scroll.borrow_mut())(&move_ev);

        let scroll_ev = crate::events::PlatformEvent::Scroll {
            delta_x: dx,
            delta_y: dy,
            is_momentum: false,
        };

        if (on_event_scroll.borrow_mut())(&scroll_ev) {
            widget.queue_draw();
        }

        gtk::glib::Propagation::Proceed
    });

    // Connect IME signals (preedit-changed, commit, retrieve-surrounding, delete-surrounding)
    let on_event_preedit = on_event_rc.clone();
    let da_preedit = drawing_area.clone();
    let ime_composing_preedit = ime_composing.clone();
    im_context.connect_preedit_changed(move |ctx| {
        use gtk::prelude::*;
        let (text, _attrs, cursor_pos) = ctx.preedit_string();
        let text_str = text.to_string();
        ime_composing_preedit.set(!text_str.is_empty());
        let cursor = if cursor_pos >= 0 {
            Some((cursor_pos as usize, cursor_pos as usize))
        } else {
            None
        };
        let plat_event = crate::events::PlatformEvent::ImePreedit {
            text: text_str,
            cursor,
        };
        if (on_event_preedit.borrow_mut())(&plat_event) {
            da_preedit.queue_draw();
        }
    });

    let on_event_commit = on_event_rc.clone();
    let da_commit = drawing_area.clone();
    let ime_composing_commit = ime_composing.clone();
    im_context.connect_commit(move |_ctx, text| {
        ime_composing_commit.set(false);
        let plat_event = crate::events::PlatformEvent::ImeCommit(text.to_string());
        if (on_event_commit.borrow_mut())(&plat_event) {
            da_commit.queue_draw();
        }
    });

    im_context.connect_retrieve_surrounding(move |ctx| {
        use gtk::prelude::*;
        let (text, cursor) = IME_SURROUNDING_TEXT.with(|st| st.borrow().clone());
        ctx.set_surrounding(&text, cursor);
        true
    });

    let on_event_del = on_event_rc.clone();
    let da_del = drawing_area.clone();
    im_context.connect_delete_surrounding(move |_ctx, offset, n_chars| {
        let plat_event = crate::events::PlatformEvent::ImeDeleteSurrounding {
            offset,
            n_chars: n_chars as u32,
        };
        if (on_event_del.borrow_mut())(&plat_event) {
            da_del.queue_draw();
        }
        true
    });

    // Keyboard handlers on Window: always emit physical KeyDown/KeyUp for printable keys
    // (unless an active multi-keystroke IME preedit composition consumes the navigation key),
    // and only suppress fallback ReceivedCharacter when IMMulticontext already committed text.
    let im_key_press = im_context.clone();
    let ime_composing_press = ime_composing.clone();
    let on_event_key = on_event_rc.clone();
    let da_key = drawing_area.clone();
    window.connect_key_press_event(move |_widget, event| {
        use gtk::prelude::*;

        let state = event.state();
        let modifiers = crate::events::Modifiers {
            shift: state.contains(gtk::gdk::ModifierType::SHIFT_MASK),
            ctrl: state.contains(gtk::gdk::ModifierType::CONTROL_MASK),
            alt: state.contains(gtk::gdk::ModifierType::MOD1_MASK),
            meta: state.contains(gtk::gdk::ModifierType::META_MASK)
                || state.contains(gtk::gdk::ModifierType::SUPER_MASK),
        };
        CURRENT_MODIFIERS.with(|m| m.set(modifiers));

        if ime_composing_press.get() && im_key_press.filter_keypress(event) {
            return gtk::glib::Propagation::Stop;
        }

        let keyval = event.keyval();
        let key_name = keyval.name().unwrap_or_default();
        let key_code = resolve_gtk_keycode(key_name.as_str(), event.hardware_keycode());

        let mut redraw = false;

        let plat_event = crate::events::PlatformEvent::KeyDown {
            key: key_code,
            modifiers,
        };

        if (on_event_key.borrow_mut())(&plat_event) {
            redraw = true;
        }

        let ime_handled = im_key_press.filter_keypress(event);

        // If printable character and not handled by IME or Ctrl/Alt/Meta command, emit ReceivedCharacter
        if !ime_handled && !modifiers.ctrl && !modifiers.alt && !modifiers.meta {
            if let Some(ch) = keyval.to_unicode() {
                if !ch.is_control() {
                    let char_event = crate::events::PlatformEvent::ReceivedCharacter(ch);
                    if (on_event_key.borrow_mut())(&char_event) {
                        redraw = true;
                    }
                }
            }
        }

        if redraw {
            da_key.queue_draw();
        }

        if ime_handled || redraw {
            gtk::glib::Propagation::Stop
        } else {
            gtk::glib::Propagation::Proceed
        }
    });

    let im_key_release = im_context.clone();
    let ime_composing_release = ime_composing;
    let on_event_key_up = on_event_rc.clone();
    let da_key_up = drawing_area.clone();
    window.connect_key_release_event(move |_widget, event| {
        use gtk::prelude::*;

        let state = event.state();
        let modifiers = crate::events::Modifiers {
            shift: state.contains(gtk::gdk::ModifierType::SHIFT_MASK),
            ctrl: state.contains(gtk::gdk::ModifierType::CONTROL_MASK),
            alt: state.contains(gtk::gdk::ModifierType::MOD1_MASK),
            meta: state.contains(gtk::gdk::ModifierType::META_MASK)
                || state.contains(gtk::gdk::ModifierType::SUPER_MASK),
        };
        CURRENT_MODIFIERS.with(|m| m.set(modifiers));

        let was_composing = ime_composing_release.get();
        let ime_handled = im_key_release.filter_keypress(event);
        if was_composing && ime_handled {
            return gtk::glib::Propagation::Stop;
        }

        let keyval = event.keyval();
        let key_name = keyval.name().unwrap_or_default();
        let key_code = resolve_gtk_keycode(key_name.as_str(), event.hardware_keycode());

        let plat_event = crate::events::PlatformEvent::KeyUp {
            key: key_code,
            modifiers,
        };

        if (on_event_key_up.borrow_mut())(&plat_event) {
            da_key_up.queue_draw();
        }

        gtk::glib::Propagation::Proceed
    });

    // Maximize-state tracking so the UI can swap its maximize / restore affordance.
    let on_event_state = on_event_rc.clone();
    let da_state = drawing_area.clone();
    window.connect_window_state_event(move |_widget, event| {
        let maximized = event
            .new_window_state()
            .contains(gtk::gdk::WindowState::MAXIMIZED);
        let changed = event
            .changed_mask()
            .contains(gtk::gdk::WindowState::MAXIMIZED);
        if changed {
            IS_MAXIMIZED_TRACKER.with(|t| t.set(maximized));
            if let Ok(mut cb) = on_event_state.try_borrow_mut() {
                let _ = cb(&crate::events::PlatformEvent::WindowMaximized(maximized));
            }
            da_state.queue_draw();
        }
        gtk::glib::Propagation::Proceed
    });

    let on_event_focus_in = on_event_rc.clone();
    window.connect_focus_in_event(move |_widget, _event| {
        if let Ok(mut cb) = on_event_focus_in.try_borrow_mut() {
            let _ = cb(&crate::events::PlatformEvent::WindowFocused(true));
        }
        gtk::glib::Propagation::Proceed
    });

    let on_event_focus_out = on_event_rc.clone();
    window.connect_focus_out_event(move |_widget, _event| {
        if let Ok(mut cb) = on_event_focus_out.try_borrow_mut() {
            let _ = cb(&crate::events::PlatformEvent::WindowFocused(false));
        }
        gtk::glib::Propagation::Proceed
    });

    if let Some(anchor) = effective_anchor {
        if anchor.is_sticky_dock() {
            window.connect_realize(move |win| {
                snap_window_to_anchor(win, anchor, win_w, win_h);
            });
            window.connect_map_event(move |win, _event| {
                snap_window_to_anchor(win, anchor, win_w, win_h);
                gtk::glib::Propagation::Proceed
            });
            window.connect_size_allocate(move |win, alloc| {
                if let Some((wx, wy, ww, wh)) = query_primary_workarea() {
                    let aw = if alloc.width() > 0 {
                        alloc.width()
                    } else {
                        win_w
                    };
                    let ah = if alloc.height() > 0 {
                        alloc.height()
                    } else {
                        win_h
                    };
                    let (tx, ty) = anchor.compute_position(wx, wy, ww, wh, aw, ah);
                    if win.position() != (tx, ty) {
                        win.move_(tx, ty);
                    }
                }
            });
            window.connect_configure_event(move |win, event| {
                if let Some((wx, wy, ww, wh)) = query_primary_workarea() {
                    let (ew, eh) = event.size();
                    let cw = if ew > 0 { ew as i32 } else { win_w };
                    let ch = if eh > 0 { eh as i32 } else { win_h };
                    let (tx, ty) = anchor.compute_position(wx, wy, ww, wh, cw, ch);
                    if event.position() != (tx, ty) {
                        win.move_(tx, ty);
                    }
                }
                false
            });
        }
    }

    window.connect_delete_event(|_, _| {
        IS_CLOSING.with(|c| c.set(true));
        gtk::main_quit();
        gtk::glib::Propagation::Proceed
    });

    window.connect_destroy(|_| {
        #[cfg(all(feature = "wgpu-backend", feature = "xlib"))]
        gtk_gpu_x11::destroy_active();
        IS_CLOSING.with(|c| c.set(true));
        IS_MAXIMIZED_TRACKER.with(|t| t.set(false));
        IS_WINDOW_DOCKED.with(|d| d.set(false));
        DRAG_STATE.with(|d| d.set(None));
        CURRENT_GTK_WINDOW.with(|w| {
            *w.borrow_mut() = None;
        });
        EVENT_DISPATCH_CALLBACK.with(|cb| {
            *cb.borrow_mut() = None;
        });
        DRAWING_AREA_HANDLE.with(|da| {
            *da.borrow_mut() = None;
        });
        gtk::main_quit();
    });

    window.add(&drawing_area);
    window.show_all();
    if let Some(anchor) = effective_anchor {
        if anchor.is_sticky_dock() {
            snap_window_to_anchor(&window, anchor, win_w, win_h);
        }
    }
    drawing_area.grab_focus();
    gtk_set_cursor(crate::window::CursorIcon::Default);

    // Single unified wakeup scheduler: arms one timer for the minimum of any
    // active animation, caret blink, JS/native timer, or periodic on_frame hook.
    // When nothing is pending (next_wakeup returns None), no timer is armed.
    arm_next_tick(
        drawing_area.clone(),
        inner_weak,
        next_wakeup_rc,
        tick_gen,
        armed_deadline,
    );

    gtk::main();

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TickRearmDecision {
    KeepPending,
    CancelPending,
    ArmAt(std::time::Instant, std::time::Duration),
}

fn compute_tick_rearm_decision(
    armed_deadline: Option<std::time::Instant>,
    now: std::time::Instant,
    next_delay: Option<std::time::Duration>,
) -> TickRearmDecision {
    let Some(delay) = next_delay else {
        return if armed_deadline.is_some() {
            TickRearmDecision::CancelPending
        } else {
            TickRearmDecision::KeepPending
        };
    };
    let target = now.checked_add(delay).unwrap_or(now);
    if let Some(existing) = armed_deadline {
        if target >= existing {
            return TickRearmDecision::KeepPending;
        }
    }
    TickRearmDecision::ArmAt(target, delay)
}

thread_local! {
    static MEASURE_SURFACE_AND_CR: (cairo::ImageSurface, cairo::Context, pango::Context, pango::Layout) = {
        let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 1, 1)
            .expect("failed to create cairo measurement surface");
        let cr = cairo::Context::new(&surface)
            .expect("failed to create cairo measurement context");
        // Measurement must use the same unhinted, grayscale font options as paint.
        cr.set_font_options(&parity_font_options());
        let pango_ctx = pangocairo::functions::create_context(&cr);
        pango_ctx.set_round_glyph_positions(false);
        let layout = pango::Layout::new(&pango_ctx);
        (surface, cr, pango_ctx, layout)
    };
}

/// Shared proportional font family stack used by both measurement and rasterization.
const SANS_FONT_FAMILY: &str = "Open Sans, DejaVu Sans, Noto Sans, system-ui, sans-serif";
/// Shared monospace font family stack used by both measurement and rasterization.
const MONO_FONT_FAMILY: &str = "JetBrains Mono, Fira Code, DejaVu Sans Mono, monospace";

/// Builds the Cairo font options used by both measurement and paint.
///
/// Chromium in the parity harness runs with `--font-render-hinting=none` and
/// `--disable-lcd-text`. Mirroring those settings keeps Pango advances
/// unhinted and grayscale-antialiased, so line breaking, measured widths, and
/// painted glyph positions all agree with the Chromium oracle.
fn parity_font_options() -> cairo::FontOptions {
    let mut options = cairo::FontOptions::new().expect("failed to create cairo font options");
    options.set_antialias(cairo::Antialias::Gray);
    options.set_hint_style(cairo::HintStyle::None);
    options.set_hint_metrics(cairo::HintMetrics::Off);
    options
}

/// Creates a Pango layout whose context matches the parity text pipeline.
///
/// Chromium and WebKitGTK keep glyph positions at subpixel precision. Pango
/// rounds them to whole pixels by default, which accumulates a per-run advance
/// error (about 0.7 percent here) and drifts line widths. Disabling glyph
/// position rounding on both the measurement and paint contexts keeps Strata's
/// advances fractional and aligned with the Chromium oracle.
fn create_parity_layout(cr: &cairo::Context) -> pango::Layout {
    let context = pangocairo::functions::create_context(cr);
    context.set_round_glyph_positions(false);
    pango::Layout::new(&context)
}

/// Single shared CSS-to-Pango weight mapping used by measurement and paint.
fn pango_weight(weight: FontWeight) -> pango::Weight {
    match weight {
        FontWeight::Thin => pango::Weight::Thin,
        FontWeight::ExtraLight => pango::Weight::Ultralight,
        FontWeight::Light => pango::Weight::Light,
        FontWeight::Normal => pango::Weight::Normal,
        FontWeight::Medium => pango::Weight::Medium,
        FontWeight::SemiBold => pango::Weight::Semibold,
        FontWeight::Bold => pango::Weight::Bold,
        FontWeight::ExtraBold => pango::Weight::Ultrabold,
        FontWeight::Black => pango::Weight::Heavy,
    }
}

/// Resolves an interned `font-family` atom to a single Pango family name.
///
/// The first comma-separated entry is used, surrounding quotes and whitespace are
/// stripped, and generic CSS families are mapped to the host's pinned faces so
/// measurement and paint select the same physical font.
pub fn resolved_font_family(atom: strata_primitives::AtomId, is_monospace: bool) -> String {
    let raw = strata_primitives::atom::resolve(atom).unwrap_or_default();
    let first = raw
        .split(',')
        .next()
        .unwrap_or("")
        .trim()
        .trim_matches(|c| c == '\'' || c == '"')
        .trim();

    if first.is_empty()
        || first.eq_ignore_ascii_case("system-ui")
        || first.eq_ignore_ascii_case("sans-serif")
        || first.eq_ignore_ascii_case("ui-sans-serif")
    {
        if is_monospace {
            "DejaVu Sans Mono".to_string()
        } else {
            "DejaVu Sans".to_string()
        }
    } else if first.eq_ignore_ascii_case("monospace") || first.eq_ignore_ascii_case("ui-monospace")
    {
        "DejaVu Sans Mono".to_string()
    } else if first.eq_ignore_ascii_case("ui-serif") {
        "serif".to_string()
    } else {
        first.to_string()
    }
}

/// Measures exact text dimensions using Pango font metrics and system fallback fonts.
pub fn pango_measure_text(
    text: &str,
    font_size: f32,
    line_height_multiplier: f32,
    max_width: Option<f32>,
    is_monospace: bool,
) -> (f32, f32) {
    if text.is_empty() {
        return (0.0, font_size * line_height_multiplier);
    }

    MEASURE_SURFACE_AND_CR.with(|(_, _, _, layout)| {
        let mut font_desc = pango::FontDescription::new();
        // Must match the family used by the DrawTextFragments rasterizer exactly,
        // otherwise layout widths diverge from painted glyph widths.
        if is_monospace {
            font_desc.set_family(MONO_FONT_FAMILY);
        } else {
            font_desc.set_family(SANS_FONT_FAMILY);
        }
        font_desc.set_absolute_size(font_size as f64 * pango::SCALE as f64);
        layout.set_font_description(Some(&font_desc));
        layout.set_text(text);

        if let Some(mw) = max_width {
            layout.set_width((mw * pango::SCALE as f32) as i32);
            layout.set_wrap(pango::WrapMode::WordChar);
        } else {
            layout.set_width(-1);
        }

        let (p_w, p_h) = layout.size();
        let measured_w = p_w as f32 / pango::SCALE as f32;
        let measured_h = (p_h as f32 / pango::SCALE as f32).max(font_size * line_height_multiplier);
        (measured_w, measured_h)
    })
}

/// Measures exact text dimensions using Pango with specific font weight and font style (Italic/Oblique).
pub fn pango_measure_styled_text(
    text: &str,
    font_size: f32,
    line_height_multiplier: f32,
    max_width: Option<f32>,
    is_monospace: bool,
    font_weight: strata_style::FontWeight,
    font_style: strata_style::FontStyle,
) -> (f32, f32) {
    if text.is_empty() {
        return (0.0, font_size * line_height_multiplier);
    }

    MEASURE_SURFACE_AND_CR.with(|(_, _, _, layout)| {
        let mut font_desc = pango::FontDescription::new();
        if is_monospace {
            font_desc.set_family(MONO_FONT_FAMILY);
        } else {
            font_desc.set_family(SANS_FONT_FAMILY);
        }
        font_desc.set_absolute_size(font_size as f64 * pango::SCALE as f64);

        font_desc.set_weight(pango_weight(font_weight));

        if font_style == FontStyle::Italic {
            font_desc.set_style(pango::Style::Italic);
        } else if font_style == FontStyle::Oblique {
            font_desc.set_style(pango::Style::Oblique);
        }

        layout.set_font_description(Some(&font_desc));
        layout.set_text(text);

        if let Some(mw) = max_width {
            layout.set_width((mw * pango::SCALE as f32) as i32);
            layout.set_wrap(pango::WrapMode::WordChar);
        } else {
            layout.set_width(-1);
        }

        let (p_w, p_h) = layout.size();
        let measured_w = p_w as f32 / pango::SCALE as f32;
        let measured_h = (p_h as f32 / pango::SCALE as f32).max(font_size * line_height_multiplier);
        (measured_w, measured_h)
    })
}

/// Measures exact text dimensions using Pango with an explicit font family.
pub fn pango_measure_text_family(
    text: &str,
    font_size: f32,
    line_height_multiplier: f32,
    max_width: Option<f32>,
    is_monospace: bool,
    font_family: strata_primitives::AtomId,
) -> (f32, f32) {
    if text.is_empty() {
        return (0.0, font_size * line_height_multiplier);
    }

    MEASURE_SURFACE_AND_CR.with(|(_, _, _, layout)| {
        let mut font_desc = pango::FontDescription::new();
        let family = resolved_font_family(font_family, is_monospace);
        font_desc.set_family(&family);
        font_desc.set_absolute_size(font_size as f64 * pango::SCALE as f64);
        layout.set_font_description(Some(&font_desc));
        layout.set_text(text);

        if let Some(mw) = max_width {
            layout.set_width((mw * pango::SCALE as f32) as i32);
            layout.set_wrap(pango::WrapMode::WordChar);
        } else {
            layout.set_width(-1);
        }

        let (p_w, p_h) = layout.size();
        let measured_w = p_w as f32 / pango::SCALE as f32;
        let measured_h = (p_h as f32 / pango::SCALE as f32).max(font_size * line_height_multiplier);
        (measured_w, measured_h)
    })
}

/// Measures exact text dimensions using Pango with an explicit font family, weight, and style.
pub fn pango_measure_styled_text_family(
    text: &str,
    font_size: f32,
    line_height_multiplier: f32,
    max_width: Option<f32>,
    is_monospace: bool,
    font_family: strata_primitives::AtomId,
    font_weight: strata_style::FontWeight,
    font_style: strata_style::FontStyle,
) -> (f32, f32) {
    if text.is_empty() {
        return (0.0, font_size * line_height_multiplier);
    }

    MEASURE_SURFACE_AND_CR.with(|(_, _, _, layout)| {
        let mut font_desc = pango::FontDescription::new();
        let family = resolved_font_family(font_family, is_monospace);
        font_desc.set_family(&family);
        font_desc.set_absolute_size(font_size as f64 * pango::SCALE as f64);

        font_desc.set_weight(pango_weight(font_weight));

        if font_style == FontStyle::Italic {
            font_desc.set_style(pango::Style::Italic);
        } else if font_style == FontStyle::Oblique {
            font_desc.set_style(pango::Style::Oblique);
        }

        layout.set_font_description(Some(&font_desc));
        layout.set_text(text);

        if let Some(mw) = max_width {
            layout.set_width((mw * pango::SCALE as f32) as i32);
            layout.set_wrap(pango::WrapMode::WordChar);
        } else {
            layout.set_width(-1);
        }

        let (p_w, p_h) = layout.size();
        let measured_w = p_w as f32 / pango::SCALE as f32;
        let measured_h = (p_h as f32 / pango::SCALE as f32).max(font_size * line_height_multiplier);
        (measured_w, measured_h)
    })
}

/// Reports Pango font metrics as `(ascent, descent, line_gap)` in CSS pixels.
pub fn pango_font_metrics(
    font_family: strata_primitives::AtomId,
    font_size: f32,
    is_monospace: bool,
) -> (f32, f32, f32) {
    MEASURE_SURFACE_AND_CR.with(|(_, _, context, _)| {
        let mut font_desc = pango::FontDescription::new();
        let family = resolved_font_family(font_family, is_monospace);
        font_desc.set_family(&family);
        font_desc.set_absolute_size(font_size as f64 * pango::SCALE as f64);
        let metrics = context.metrics(Some(&font_desc), None);
        let raw_ascent = metrics.ascent() as f32 / pango::SCALE as f32;
        let raw_descent = metrics.descent() as f32 / pango::SCALE as f32;
        let raw_height = metrics.height() as f32 / pango::SCALE as f32;
        let ascent = (raw_ascent * 64.0).round() / 64.0;
        let descent = (raw_descent * 64.0).round() / 64.0;
        let height = (raw_height * 64.0).round() / 64.0;
        let line_gap = ((height - ascent - descent).max(0.0) * 64.0).round() / 64.0;
        (ascent, descent, line_gap)
    })
}

/// Initializes Pango text measurement hook into `strata_layout`.
pub fn init_pango_text_measurement() {
    strata_layout::text::register_text_measurer(pango_measure_text);
    strata_layout::text::register_styled_text_measurer(pango_measure_styled_text);
    strata_layout::text::register_family_text_measurer(pango_measure_text_family);
    strata_layout::text::register_family_styled_text_measurer(pango_measure_styled_text_family);
    strata_layout::text::register_font_metrics(pango_font_metrics);
}

/// Hit-tests a 6px resize border/corner zone around a frameless window, returning the
/// corresponding `gdk::WindowEdge` and directional resize `CursorIcon`.
pub fn frameless_resize_edge(
    x: f64,
    y: f64,
    width: i32,
    height: i32,
) -> Option<(gtk::gdk::WindowEdge, crate::window::CursorIcon)> {
    const BORDER: f64 = 6.0;
    if width <= 0 || height <= 0 {
        return None;
    }
    let w = width as f64;
    let h = height as f64;
    let left = x < BORDER;
    let right = x >= w - BORDER;
    let top = y < BORDER;
    let bottom = y >= h - BORDER;

    match (left, right, top, bottom) {
        (true, _, true, _) => Some((
            gtk::gdk::WindowEdge::NorthWest,
            crate::window::CursorIcon::ResizeNw,
        )),
        (_, true, true, _) => Some((
            gtk::gdk::WindowEdge::NorthEast,
            crate::window::CursorIcon::ResizeNe,
        )),
        (true, _, _, true) => Some((
            gtk::gdk::WindowEdge::SouthWest,
            crate::window::CursorIcon::ResizeSw,
        )),
        (_, true, _, true) => Some((
            gtk::gdk::WindowEdge::SouthEast,
            crate::window::CursorIcon::ResizeSe,
        )),
        (true, _, _, _) => Some((
            gtk::gdk::WindowEdge::West,
            crate::window::CursorIcon::ResizeW,
        )),
        (_, true, _, _) => Some((
            gtk::gdk::WindowEdge::East,
            crate::window::CursorIcon::ResizeE,
        )),
        (_, _, true, _) => Some((
            gtk::gdk::WindowEdge::North,
            crate::window::CursorIcon::ResizeN,
        )),
        (_, _, _, true) => Some((
            gtk::gdk::WindowEdge::South,
            crate::window::CursorIcon::ResizeS,
        )),
        _ => None,
    }
}

#[cfg(test)]
mod frameless_resize_tests {
    use super::frameless_resize_edge;
    use crate::window::CursorIcon;

    #[test]
    fn test_frameless_resize_edge_hit_testing() {
        assert_eq!(
            frameless_resize_edge(2.0, 2.0, 800, 600),
            Some((gtk::gdk::WindowEdge::NorthWest, CursorIcon::ResizeNw))
        );
        assert_eq!(
            frameless_resize_edge(798.0, 2.0, 800, 600),
            Some((gtk::gdk::WindowEdge::NorthEast, CursorIcon::ResizeNe))
        );
        assert_eq!(
            frameless_resize_edge(2.0, 598.0, 800, 600),
            Some((gtk::gdk::WindowEdge::SouthWest, CursorIcon::ResizeSw))
        );
        assert_eq!(
            frameless_resize_edge(798.0, 598.0, 800, 600),
            Some((gtk::gdk::WindowEdge::SouthEast, CursorIcon::ResizeSe))
        );
        assert_eq!(
            frameless_resize_edge(2.0, 300.0, 800, 600),
            Some((gtk::gdk::WindowEdge::West, CursorIcon::ResizeW))
        );
        assert_eq!(
            frameless_resize_edge(798.0, 300.0, 800, 600),
            Some((gtk::gdk::WindowEdge::East, CursorIcon::ResizeE))
        );
        assert_eq!(
            frameless_resize_edge(400.0, 2.0, 800, 600),
            Some((gtk::gdk::WindowEdge::North, CursorIcon::ResizeN))
        );
        assert_eq!(
            frameless_resize_edge(400.0, 598.0, 800, 600),
            Some((gtk::gdk::WindowEdge::South, CursorIcon::ResizeS))
        );
        assert_eq!(frameless_resize_edge(400.0, 300.0, 800, 600), None);
    }

    #[test]
    fn test_viewport_3d_resize_and_interactive_half_res_fallback() {
        let scene = std::sync::Arc::new(strata_render::scene3d::Scene3D::new());
        // 1. Cold render at 200x200 produces full-res surface.
        let first = super::viewport_3d_surface_with_fallback(&scene, 200, 200, true).unwrap();
        assert_eq!((first.src_width, first.src_height), (200, 200));
        assert!(!first.needs_debounced_full_res);

        // 2. Immediate resize of the same Arc<Scene3D> to 210x200 reuses cached 200x200 surface.
        let resized = super::viewport_3d_surface_with_fallback(&scene, 210, 200, true).unwrap();
        assert_eq!((resized.src_width, resized.src_height), (200, 200));
        assert!(resized.needs_debounced_full_res);

        // 3. Rapid camera pan (new Arc<Scene3D>) within interactive window uses half-res.
        let scene_moved = std::sync::Arc::new(strata_render::scene3d::Scene3D::new());
        let panned =
            super::viewport_3d_surface_with_fallback(&scene_moved, 210, 200, true).unwrap();
        assert_eq!((panned.src_width, panned.src_height), (105, 100));
        assert!(panned.needs_debounced_full_res);
    }

    #[test]
    fn test_window_drag_clears_when_button1_released_after_wm_grab() {
        super::DRAG_STATE.with(|d| {
            d.set(Some(super::WindowDragState {
                start_root_x: 100.0,
                start_root_y: 50.0,
                start_win_x: 200,
                start_win_y: 120,
                active: true,
            }));
        });

        // While left mouse button is still held, motion updates window coordinates.
        assert_eq!(
            super::step_window_drag_motion(true, false, 145.0, 80.0),
            Some((245, 150))
        );
        assert!(super::DRAG_STATE.with(|d| d.get().is_some()));

        // When WM consumed button release, next motion arrives without BUTTON1_MASK:
        // must clear DRAG_STATE and stop moving the window.
        assert_eq!(
            super::step_window_drag_motion(false, false, 300.0, 200.0),
            None
        );
        assert!(super::DRAG_STATE.with(|d| d.get().is_none()));
    }

    #[test]
    fn test_tick_rearm_preserves_pending_unless_strictly_earlier() {
        use std::time::{Duration, Instant};
        let t0 = Instant::now();
        let d16 = Duration::from_millis(16);
        let d4 = Duration::from_millis(4);

        // No tick pending -> arms at t0 + 16ms.
        assert_eq!(
            super::compute_tick_rearm_decision(None, t0, Some(d16)),
            super::TickRearmDecision::ArmAt(t0 + d16, d16)
        );

        // MouseMove 5ms later requesting another 16ms wakeup (t0 + 21ms >= t0 + 16ms)
        // must keep existing pending tick instead of cancelling/postponing it.
        let armed = Some(t0 + d16);
        let t_move = t0 + Duration::from_millis(5);
        assert_eq!(
            super::compute_tick_rearm_decision(armed, t_move, Some(d16)),
            super::TickRearmDecision::KeepPending
        );

        // Immediate wakeup (0ms) or strictly earlier wakeup (t_move + 4ms < t0 + 16ms) re-arms earlier.
        assert_eq!(
            super::compute_tick_rearm_decision(armed, t_move, Some(d4)),
            super::TickRearmDecision::ArmAt(t_move + d4, d4)
        );
        assert_eq!(
            super::compute_tick_rearm_decision(armed, t_move, Some(Duration::ZERO)),
            super::TickRearmDecision::ArmAt(t_move, Duration::ZERO)
        );

        // No future wakeup needed -> cancels armed tick cleanly.
        assert_eq!(
            super::compute_tick_rearm_decision(armed, t_move, None),
            super::TickRearmDecision::CancelPending
        );
        assert_eq!(
            super::compute_tick_rearm_decision(None, t_move, None),
            super::TickRearmDecision::KeepPending
        );
    }

    #[cfg(all(feature = "wgpu-backend", feature = "xlib"))]
    #[test]
    fn test_overlay_64x64_tile_damage_isolates_moving_labels_from_static_hud() {
        use strata_layout::Rect;
        use strata_primitives::Rgba;
        use strata_render::compositor::{DisplayList, DrawCommand};

        let make_frame = |label_x: f32| -> DisplayList {
            let mut dl = DisplayList::new();
            // Full-screen root + 3D viewport hole
            dl.push(DrawCommand::DrawRect {
                bounds: Rect::new(0.0, 0.0, 1920.0, 1080.0),
                color: Rgba::rgba(9, 9, 11, 255),
            });
            // Static top-right HUD panel (around x=1500..1900, y=12..140)
            dl.push(DrawCommand::DrawRect {
                bounds: Rect::new(1500.0, 12.0, 400.0, 128.0),
                color: Rgba::rgba(20, 20, 24, 220),
            });
            // Moving map label pill around (label_x, 540)
            dl.push(DrawCommand::DrawRect {
                bounds: Rect::new(label_x, 540.0, 72.0, 20.0),
                color: Rgba::rgba(0, 0, 0, 180),
            });
            dl
        };

        let cols = 1920usize.div_ceil(64);
        let rows = 1080usize.div_ceil(64);
        let mut h1 = Vec::new();
        let mut h2 = Vec::new();
        super::gtk_gpu_x11::compute_overlay_tile_hashes(
            &make_frame(420.0),
            1920,
            1080,
            true,
            None,
            cols,
            rows,
            &mut h1,
        );
        super::gtk_gpu_x11::compute_overlay_tile_hashes(
            &make_frame(423.5),
            1920,
            1080,
            true,
            None,
            cols,
            rows,
            &mut h2,
        );

        let mut dirty_bits = vec![0u64; (cols * rows).div_ceil(64)];
        let mut dirty_count = 0usize;
        for (i, (&a, &b)) in h1.iter().zip(h2.iter()).enumerate() {
            if a != b {
                dirty_bits[i >> 6] |= 1u64 << (i & 63);
                dirty_count += 1;
            }
        }
        assert!(
            dirty_count > 0 && dirty_count <= 8,
            "moving 1 label should dirty only 2..8 64x64 tiles out of 510, got {dirty_count}"
        );
        // Static top-right HUD (1500..1900, 12..140) must NOT overlap any dirty tile!
        assert!(
            !super::rect_overlaps_dirty_tiles(1488.0, 0.0, 1912.0, 152.0, &dirty_bits, cols, rows),
            "static top-right HUD panel must be completely culled on label-only frames"
        );
        // Moved label region (420..495, 540..560) MUST overlap dirty tiles!
        assert!(super::rect_overlaps_dirty_tiles(
            420.0, 540.0, 495.0, 560.0, &dirty_bits, cols, rows
        ));
    }

    #[test]
    fn test_resolve_gtk_keycode_printable_and_evdev_fallback() {
        use crate::events::KeyCode;
        assert_eq!(super::resolve_gtk_keycode("k", 45), KeyCode::KeyK);
        assert_eq!(super::resolve_gtk_keycode("K", 45), KeyCode::KeyK);
        assert_eq!(super::resolve_gtk_keycode("l", 46), KeyCode::KeyL);
        assert_eq!(super::resolve_gtk_keycode("L", 46), KeyCode::KeyL);
        // Non-Latin layout or shifted symbol falls back to evdev+8 hardware keycode:
        assert_eq!(super::resolve_gtk_keycode("Cyrillic_el", 45), KeyCode::KeyK);
        assert_eq!(super::resolve_gtk_keycode("Cyrillic_de", 46), KeyCode::KeyL);
        assert_eq!(super::resolve_gtk_keycode("exclam", 10), KeyCode::Digit1);
        assert_eq!(super::resolve_gtk_keycode("idotless", 31), KeyCode::KeyI);
    }
}

