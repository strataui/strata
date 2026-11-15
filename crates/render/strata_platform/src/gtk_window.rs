//! High-Fidelity Native GTK3 and Cairo Presentation Backend for Linux.
//!
//! Connects Strata's DisplayList directly to Cairo graphics context with system typography,
//! FreeType/HarfBuzz font shaping, and subpixel anti-aliased vector geometry.

use strata_render::compositor::{DisplayList, DrawCommand};
use strata_render::{FillRule, StrokeLineCap, StrokeLineJoin};
use strata_style::types::{BorderStyle, FontStyle, FontWeight, TextDecoration};

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
    if let Ok(mut data) = snapshot.data() {
        let mut buf = vec![0u8; w_us * h_us * 4];
        for row in 0..h_us {
            let src_off = row * stride;
            let dst_off = row * w_us * 4;
            buf[dst_off..dst_off + w_us * 4].copy_from_slice(&data[src_off..src_off + w_us * 4]);
        }

        for op in &filter.operations {
            match *op {
                strata_style::effects::FilterOp::Blur(radius_px) => {
                    let blur_r = ((radius_px as f64 * dev_scale).round() as usize).min(128);
                    if blur_r >= 1 {
                        box_blur_3pass(&mut buf, w_us, h_us, blur_r);
                    }
                }
                other => apply_color_filter_op_premul(&mut buf, other),
            }
        }

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

/// One rasterized 3D viewport frame, keyed by scene identity and pixel size.
struct Viewport3DCacheEntry {
    /// Held so the allocation, and therefore the pointer identity, stays valid.
    scene: std::sync::Arc<strata_render::scene3d::Scene3D>,
    width: usize,
    height: usize,
    surface: cairo::ImageSurface,
}

thread_local! {
    static VIEWPORT_3D_CACHE: std::cell::RefCell<Vec<Viewport3DCacheEntry>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Maximum number of rasterized viewport frames kept alive.
const VIEWPORT_3D_CACHE_CAPACITY: usize = 4;

/// Returns the cairo surface for a 3D viewport, software-rasterizing it only
/// when this exact `Arc<Scene3D>` has not been drawn at this size before.
///
/// Scenes are immutable behind the `Arc`, so pointer identity covers camera,
/// meshes, overlay and texture changes: any change produces a new `Arc`.
fn viewport_3d_surface(
    scene: &std::sync::Arc<strata_render::scene3d::Scene3D>,
    w: usize,
    h: usize,
) -> Option<cairo::ImageSurface> {
    let hit = VIEWPORT_3D_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let idx = cache.iter().position(|e| {
            std::sync::Arc::ptr_eq(&e.scene, scene) && e.width == w && e.height == h
        })?;
        // Move to the back so the front is always least recently used.
        let entry = cache.remove(idx);
        let surface = entry.surface.clone();
        cache.push(entry);
        Some(surface)
    });
    if hit.is_some() {
        return hit;
    }

    let mut fb = strata_render::rasterizer::FrameBuffer::new(w, h);
    strata_render::rasterizer3d::render_scene_3d_with_textures(
        &mut fb,
        strata_layout::Rect::new(0.0, 0.0, w as f32, h as f32),
        scene,
        None,
        scene.textures_deref(),
    );

    let mut surface =
        cairo::ImageSurface::create(cairo::Format::ARgb32, w as i32, h as i32).ok()?;
    {
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
    }

    VIEWPORT_3D_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() >= VIEWPORT_3D_CACHE_CAPACITY {
            cache.remove(0);
        }
        cache.push(Viewport3DCacheEntry {
            scene: std::sync::Arc::clone(scene),
            width: w,
            height: h,
            surface: surface.clone(),
        });
    });
    Some(surface)
}

/// Executes a Strata DisplayList into a Cairo graphics context with full typography and vector support.
pub fn render_display_list_to_cairo(cr: &cairo::Context, list: &DisplayList) {
    let perf_start = std::time::Instant::now();
    let mut current_clip = cr
        .clip_extents()
        .unwrap_or((0.0, 0.0, 100_000.0, 100_000.0));
    let mut operator_stack: Vec<cairo::Operator> = Vec::new();
    let mut opacity_stack: Vec<f64> = Vec::new();

    for cmd in list.commands() {
        if !cmd.is_clip() && !cmd.is_state_command() {
            let (cx1, cy1, cx2, cy2) = current_clip;
            if let Some(b) = cmd.bounds() {
                let bx1 = b.x as f64;
                let by1 = b.y as f64;
                let bx2 = (b.x + b.width) as f64;
                let by2 = (b.y + b.height) as f64;
                if bx2 < cx1 || bx1 > cx2 || by2 < cy1 || by1 > cy2 {
                    continue;
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
                cairo_apply_backdrop_filter(cr, bounds, *radius, filter);
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
                let layout = create_parity_layout(cr);
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
                pangocairo::functions::show_layout(cr, &layout);
            }
            DrawCommand::DrawTextFragments { fragments } => {
                let clip_opt = cr.clip_extents().ok();
                for frag in fragments {
                    if frag.text.is_empty() && frag.style.background_color.is_none() {
                        continue;
                    }

                    if let Some((cx1, cy1, cx2, cy2)) = clip_opt {
                        let fx1 = (frag.bounds.x - 4.0) as f64;
                        let fy1 = (frag.bounds.y - 2.0) as f64;
                        let fx2 = (frag.bounds.x + frag.bounds.width + 4.0) as f64;
                        let fy2 = (frag.bounds.y + frag.bounds.height + 2.0) as f64;
                        if fx2 < cx1 || fx1 > cx2 || fy2 < cy1 || fy1 > cy2 {
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
                        let layout = create_parity_layout(cr);
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
                        pangocairo::functions::show_layout(cr, &layout);

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
                cr.push_group();
                opacity_stack.push(*alpha as f64);
            }
            DrawCommand::PopOpacity => {
                if let Some(alpha) = opacity_stack.pop() {
                    let _ = cr.pop_group_to_source();
                    let _ = cr.paint_with_alpha(alpha);
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
                let w = bounds.width.max(1.0).round() as usize;
                let h = bounds.height.max(1.0).round() as usize;
                if let Some(surface) = viewport_3d_surface(scene, w, h) {
                    let _ = cr.save();
                    let _ = cr.set_source_surface(&surface, bounds.x as f64, bounds.y as f64);
                    let _ = cr.paint();
                    let _ = cr.restore();
                }
            }
            _ => {}
        }
    }

    static PERF_ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    static RENDER_FRAMES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    if *PERF_ENABLED.get_or_init(|| std::env::var_os("STRATA_PERF_LOG").is_some()) {
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
                    _ => gtk::gdk::Cursor::from_name(&display, "default").or_else(|| {
                        gtk::gdk::Cursor::for_display(&display, gtk::gdk::CursorType::LeftPtr)
                    }),
                };
                gdk_win.set_cursor(cursor.as_ref());
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
    use gtk::prelude::*;

    if gtk::init().is_err() {
        return Err("Failed to initialize GTK on current DISPLAY".to_string());
    }

    IS_CLOSING.with(|c| c.set(false));

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
    window.add_events(gtk::gdk::EventMask::STRUCTURE_MASK);

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
        im_unrealize.set_client_window(None::<&gtk::gdk::Window>);
    });

    let im_focus_in = im_context.clone();
    drawing_area.connect_focus_in_event(move |_widget, _event| {
        use gtk::prelude::*;
        im_focus_in.focus_in();
        gtk::glib::Propagation::Proceed
    });

    let im_focus_out = im_context.clone();
    drawing_area.connect_focus_out_event(move |_widget, _event| {
        use gtk::prelude::*;
        im_focus_out.focus_out();
        gtk::glib::Propagation::Proceed
    });

    let on_draw_rc = std::rc::Rc::new(std::cell::RefCell::new(on_draw));
    let on_draw_cb = on_draw_rc.clone();
    drawing_area.connect_draw(move |widget, cr| {
        let w = widget.allocated_width() as f32;
        let h = widget.allocated_height() as f32;
        // Live paint must use the same font options as measurement.
        cr.set_font_options(&parity_font_options());
        (on_draw_cb.borrow_mut())(w, h, cr);
        gtk::glib::Propagation::Proceed
    });

    // Any non-tick event marks the UI active so the animation clock runs at
    // full rate; the tick source drops to a slow poll while nothing happens.
    let last_activity = std::rc::Rc::new(std::cell::Cell::new(std::time::Instant::now()));
    let activity_marker = last_activity.clone();
    let mut on_event = on_event;
    let on_event = move |event: &crate::events::PlatformEvent| -> bool {
        if !matches!(event, crate::events::PlatformEvent::AnimationTick) {
            activity_marker.set(std::time::Instant::now());
        }
        on_event(event)
    };
    let on_event_rc = std::rc::Rc::new(std::cell::RefCell::new(on_event));

    DRAWING_AREA_HANDLE.with(|da| {
        *da.borrow_mut() = Some(drawing_area.clone());
    });
    let on_event_dispatch = on_event_rc.clone();
    EVENT_DISPATCH_CALLBACK.with(|cb| {
        *cb.borrow_mut() = Some(Box::new(move |ev| (on_event_dispatch.borrow_mut())(&ev)));
    });

    // Mouse button click handler
    let on_event_mouse = on_event_rc.clone();
    drawing_area.connect_button_press_event(move |widget, event| {
        let (x, y) = event.position();
        let (root_x, root_y) = event.root();
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

        // If manual window drag fallback is active, update window position (unless docked)
        if !IS_WINDOW_DOCKED.with(|d| d.get()) {
            DRAG_STATE.with(|d| {
                if let Some(ds) = d.get() {
                    if ds.active {
                        CURRENT_GTK_WINDOW.with(|w| {
                            use gtk::prelude::*;
                            if let Some(win) = w.borrow().as_ref() {
                                let dx = (root_x - ds.start_root_x) as i32;
                                let dy = (root_y - ds.start_root_y) as i32;
                                win.move_(ds.start_win_x + dx, (ds.start_win_y + dy).max(0));
                            }
                        });
                    }
                }
            });
        }

        if IS_CLOSING.with(|c| c.get()) {
            return gtk::glib::Propagation::Proceed;
        }

        let plat_event = crate::events::PlatformEvent::MouseMove {
            x: x as f32,
            y: y as f32,
        };

        if (on_event_motion.borrow_mut())(&plat_event) {
            widget.queue_draw();
        }

        gtk::glib::Propagation::Proceed
    });

    let on_event_leave = on_event_rc.clone();
    drawing_area.connect_leave_notify_event(move |widget, _event| {
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
    im_context.connect_preedit_changed(move |ctx| {
        use gtk::prelude::*;
        let (text, _attrs, cursor_pos) = ctx.preedit_string();
        let text_str = text.to_string();
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
    im_context.connect_commit(move |_ctx, text| {
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

    // Keyboard handlers on Window to capture global shortcuts reliably with IME filtering
    let im_key_press = im_context.clone();
    let on_event_key = on_event_rc.clone();
    let da_key = drawing_area.clone();
    window.connect_key_press_event(move |_widget, event| {
        use gtk::prelude::*;
        if im_key_press.filter_keypress(event) {
            return gtk::glib::Propagation::Stop;
        }

        let state = event.state();
        let modifiers = crate::events::Modifiers {
            shift: state.contains(gtk::gdk::ModifierType::SHIFT_MASK),
            ctrl: state.contains(gtk::gdk::ModifierType::CONTROL_MASK),
            alt: state.contains(gtk::gdk::ModifierType::MOD1_MASK),
            meta: state.contains(gtk::gdk::ModifierType::META_MASK)
                || state.contains(gtk::gdk::ModifierType::SUPER_MASK),
        };

        let keyval = event.keyval();
        let key_name = keyval.name().unwrap_or_default();
        let key_code = map_key_name_to_keycode(key_name.as_str());

        let mut redraw = false;

        let plat_event = crate::events::PlatformEvent::KeyDown {
            key: key_code,
            modifiers,
        };

        if (on_event_key.borrow_mut())(&plat_event) {
            redraw = true;
        }

        // If printable character and not a Ctrl/Alt/Meta command, also emit ReceivedCharacter
        if !modifiers.ctrl && !modifiers.alt && !modifiers.meta {
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

        gtk::glib::Propagation::Proceed
    });

    let im_key_release = im_context.clone();
    let on_event_key_up = on_event_rc.clone();
    let da_key_up = drawing_area.clone();
    window.connect_key_release_event(move |_widget, event| {
        use gtk::prelude::*;
        if im_key_release.filter_keypress(event) {
            return gtk::glib::Propagation::Stop;
        }

        let state = event.state();
        let modifiers = crate::events::Modifiers {
            shift: state.contains(gtk::gdk::ModifierType::SHIFT_MASK),
            ctrl: state.contains(gtk::gdk::ModifierType::CONTROL_MASK),
            alt: state.contains(gtk::gdk::ModifierType::MOD1_MASK),
            meta: state.contains(gtk::gdk::ModifierType::META_MASK)
                || state.contains(gtk::gdk::ModifierType::SUPER_MASK),
        };

        let keyval = event.keyval();
        let key_name = keyval.name().unwrap_or_default();
        let key_code = map_key_name_to_keycode(key_name.as_str());

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

    // Single adaptive tick source. The previous vsync tick callback plus 16 ms
    // timer woke the process twice per frame and the frame clock never slept.
    // Now one timer runs at ~60 Hz while input or animation is recent and
    // backs off to a slow poll when idle, which still lets caret blink and
    // externally started transitions advance.
    const ACTIVE_INTERVAL_MS: u64 = 16;
    const IDLE_INTERVAL_MS: u64 = 100;
    const ACTIVE_WINDOW: std::time::Duration = std::time::Duration::from_millis(1000);

    fn schedule_tick(
        da: gtk::DrawingArea,
        on_event: std::rc::Rc<std::cell::RefCell<dyn FnMut(&crate::events::PlatformEvent) -> bool>>,
        activity: std::rc::Rc<std::cell::Cell<std::time::Instant>>,
        delay_ms: u64,
    ) {
        gtk::glib::timeout_add_local_once(std::time::Duration::from_millis(delay_ms), move || {
            if IS_CLOSING.with(|c| c.get()) {
                return;
            }
            let tick_ev = crate::events::PlatformEvent::AnimationTick;
            let changed = (on_event.borrow_mut())(&tick_ev);
            if changed {
                use gtk::prelude::WidgetExt;
                da.queue_draw();
                activity.set(std::time::Instant::now());
            }
            let next = if activity.get().elapsed() < ACTIVE_WINDOW {
                ACTIVE_INTERVAL_MS
            } else {
                IDLE_INTERVAL_MS
            };
            schedule_tick(da, on_event, activity, next);
        });
    }
    let tick_target: std::rc::Rc<
        std::cell::RefCell<dyn FnMut(&crate::events::PlatformEvent) -> bool>,
    > = on_event_rc.clone();
    schedule_tick(
        drawing_area.clone(),
        tick_target,
        last_activity,
        ACTIVE_INTERVAL_MS,
    );

    gtk::main();

    Ok(())
}

thread_local! {
    static MEASURE_SURFACE_AND_CR: (cairo::ImageSurface, cairo::Context) = {
        let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 1, 1)
            .expect("failed to create cairo measurement surface");
        let cr = cairo::Context::new(&surface)
            .expect("failed to create cairo measurement context");
        // Measurement must use the same unhinted, grayscale font options as paint.
        cr.set_font_options(&parity_font_options());
        (surface, cr)
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

    MEASURE_SURFACE_AND_CR.with(|(_, cr)| {
        let layout = create_parity_layout(cr);
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

    MEASURE_SURFACE_AND_CR.with(|(_, cr)| {
        let layout = create_parity_layout(cr);
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

    MEASURE_SURFACE_AND_CR.with(|(_, cr)| {
        let layout = create_parity_layout(cr);
        let mut font_desc = pango::FontDescription::new();
        let family = resolved_font_family(font_family, is_monospace);
        font_desc.set_family(&family);
        font_desc.set_absolute_size(font_size as f64 * pango::SCALE as f64);
        layout.set_font_description(Some(&font_desc));
        layout.set_text(text);

        if let Some(mw) = max_width {
            layout.set_width((mw * pango::SCALE as f32) as i32);
            layout.set_wrap(pango::WrapMode::WordChar);
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

    MEASURE_SURFACE_AND_CR.with(|(_, cr)| {
        let layout = create_parity_layout(cr);
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
    MEASURE_SURFACE_AND_CR.with(|(_, cr)| {
        let context = pangocairo::functions::create_context(cr);
        context.set_round_glyph_positions(false);
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
