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

/// Strokes a uniform border edge using the requested CSS border style.
fn stroke_uniform_border(
    cr: &cairo::Context,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    radius: f64,
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
            let r = (radius - half).max(0.0);
            cairo_rounded_rect(
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
            let r = (radius - half).max(0.0);
            cairo_rounded_rect(
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
            let r = (radius - half).max(0.0);
            cairo_rounded_rect(
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
                let r = (radius - inset).max(0.0);
                cairo_rounded_rect(
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

/// Renders an outer box shadow with analytical concentric Gaussian falloff in Cairo.
pub fn cairo_draw_shadow(
    cr: &cairo::Context,
    bounds: &strata_layout::Rect,
    shadow: &strata_style::types::BoxShadow,
) {
    if shadow.color.a() <= 0.001 {
        return;
    }
    let r = shadow.color.r() as f64;
    let g = shadow.color.g() as f64;
    let b = shadow.color.b() as f64;
    let base_a = shadow.color.a() as f64;

    if shadow.blur_radius <= 0.5 {
        let x = (bounds.x + shadow.offset_x - shadow.spread_radius) as f64;
        let y = (bounds.y + shadow.offset_y - shadow.spread_radius) as f64;
        let w = (bounds.width + shadow.spread_radius * 2.0).max(0.0) as f64;
        let h = (bounds.height + shadow.spread_radius * 2.0).max(0.0) as f64;
        cr.set_source_rgba(r, g, b, base_a);
        cr.rectangle(x, y, w, h);
        let _ = cr.fill();
        return;
    }

    // Concentric Gaussian approximation
    let steps = (shadow.blur_radius.ceil() as usize).clamp(3, 12);
    let blur = shadow.blur_radius as f64;
    let spread = shadow.spread_radius as f64;
    let total_layers = steps as f64;

    for i in 0..steps {
        let t = (i as f64 + 0.5) / total_layers;
        // Gaussian falloff approximation: (1 - t^2)
        let alpha = base_a * (1.0 - t * t) / (total_layers * 0.65);
        let current_expand = spread + t * blur;

        let x = (bounds.x + shadow.offset_x) as f64 - current_expand;
        let y = (bounds.y + shadow.offset_y) as f64 - current_expand;
        let w = (bounds.width as f64 + current_expand * 2.0).max(0.0);
        let h = (bounds.height as f64 + current_expand * 2.0).max(0.0);

        let corner_r = (current_expand * 0.5).max(0.0);
        cairo_rounded_rect(cr, x, y, w, h, corner_r);
        cr.set_source_rgba(r, g, b, alpha);
        let _ = cr.fill();
    }
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
    cairo_rounded_rect(
        cr,
        bounds.x as f64,
        bounds.y as f64,
        bounds.width as f64,
        bounds.height as f64,
        border_radius.top_left as f64,
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

        cairo_rounded_rect(
            cr,
            inset_x,
            inset_y,
            inset_w,
            inset_h,
            border_radius.top_left as f64,
        );
        cr.set_source_rgba(r, g, b, alpha);
        cr.set_line_width(stroke_w);
        let _ = cr.stroke();
    }

    let _ = cr.restore();
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
    let n = stops.len();
    for (idx, stop) in stops.iter().enumerate() {
        let pos = match stop.position {
            Some(p) => p as f64,
            None => {
                if n > 1 {
                    idx as f64 / (n - 1) as f64
                } else {
                    0.0
                }
            }
        };
        pattern.add_color_stop_rgba(
            pos.clamp(0.0, 1.0),
            stop.color.r() as f64,
            stop.color.g() as f64,
            stop.color.b() as f64,
            stop.color.a() as f64,
        );
    }

    if border_radius.top_left > 0.0 || border_radius.top_right > 0.0 {
        cairo_rounded_rect(
            cr,
            bounds.x as f64,
            bounds.y as f64,
            w,
            h,
            border_radius.top_left as f64,
        );
    } else {
        cr.rectangle(bounds.x as f64, bounds.y as f64, w, h);
    }

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
    let n = stops.len();
    for (idx, stop) in stops.iter().enumerate() {
        let pos = match stop.position {
            Some(p) => p as f64,
            None => {
                if n > 1 {
                    idx as f64 / (n - 1) as f64
                } else {
                    0.0
                }
            }
        };
        pattern.add_color_stop_rgba(
            pos.clamp(0.0, 1.0),
            stop.color.r() as f64,
            stop.color.g() as f64,
            stop.color.b() as f64,
            stop.color.a() as f64,
        );
    }

    if border_radius.top_left > 0.0 || border_radius.top_right > 0.0 {
        cairo_rounded_rect(
            cr,
            bounds.x as f64,
            bounds.y as f64,
            w,
            h,
            border_radius.top_left as f64,
        );
    } else {
        cr.rectangle(bounds.x as f64, bounds.y as f64, w, h);
    }

    let _ = cr.set_source(&pattern);
    let _ = cr.fill();
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
                cairo_rounded_rect(
                    cr,
                    bounds.x as f64,
                    bounds.y as f64,
                    bounds.width as f64,
                    bounds.height as f64,
                    border_radius.top_left as f64,
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
                        let r = (border_radius.top_left as f64 - half_w).max(0.0);
                        cairo_rounded_rect(
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
            DrawCommand::DrawShadow { bounds, shadow } => {
                cairo_draw_shadow(cr, bounds, shadow);
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
                let _ = cr.save();
                cairo_rounded_rect(
                    cr,
                    bounds.x as f64,
                    bounds.y as f64,
                    bounds.width as f64,
                    bounds.height as f64,
                    radius.top_left as f64,
                );
                cr.clip();
                if filter
                    .operations
                    .iter()
                    .any(|op| matches!(op, strata_style::effects::FilterOp::Blur(_)))
                {
                    cr.set_source_rgba(1.0, 1.0, 1.0, 0.04);
                    let _ = cr.paint();
                }
            }
            DrawCommand::PopBackdropFilter => {
                let _ = cr.restore();
            }
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
                        border_radius.top_left as f64,
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
                if radius.top_left > 0.0
                    || radius.top_right > 0.0
                    || radius.bottom_left > 0.0
                    || radius.bottom_right > 0.0
                {
                    cairo_rounded_rect(
                        cr,
                        bounds.x as f64,
                        bounds.y as f64,
                        bounds.width as f64,
                        bounds.height as f64,
                        radius.top_left as f64,
                    );
                } else {
                    cr.rectangle(
                        bounds.x as f64,
                        bounds.y as f64,
                        bounds.width as f64,
                        bounds.height as f64,
                    );
                }
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
                if bounds.width > 0.0 && bounds.height > 0.0 => {
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
    cursor_overlay: Option<(&crate::mcp_cursor::McpCursorState, &crate::cursor_theme::CursorTheme)>,
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
    use gtk::prelude::*;

    if gtk::init().is_err() {
        return Err("Failed to initialize GTK on current DISPLAY".to_string());
    }

    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title(title);
    window.set_default_size(width as i32, height as i32);
    window.set_size_request(width as i32, height as i32);
    window.resize(width as i32, height as i32);
    window.set_decorated(decorated);

    CURRENT_GTK_WINDOW.with(|w| {
        *w.borrow_mut() = Some(window.clone());
    });

    let drawing_area = gtk::DrawingArea::new();
    drawing_area.add_events(
        gtk::gdk::EventMask::BUTTON_PRESS_MASK
            | gtk::gdk::EventMask::BUTTON_RELEASE_MASK
            | gtk::gdk::EventMask::POINTER_MOTION_MASK
            | gtk::gdk::EventMask::KEY_PRESS_MASK
            | gtk::gdk::EventMask::STRUCTURE_MASK
            | gtk::gdk::EventMask::ENTER_NOTIFY_MASK
            | gtk::gdk::EventMask::LEAVE_NOTIFY_MASK
            | gtk::gdk::EventMask::SCROLL_MASK
            | gtk::gdk::EventMask::SMOOTH_SCROLL_MASK,
    );
    drawing_area.set_can_focus(true);
    drawing_area.set_hexpand(true);
    drawing_area.set_vexpand(true);
    drawing_area.set_size_request(width as i32, height as i32);

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

        // If manual window drag fallback is active, update window position
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

    // Keyboard handler on Window to capture global shortcuts reliably
    let on_event_key = on_event_rc.clone();
    let da_key = drawing_area.clone();
    window.connect_key_press_event(move |_widget, event| {
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
        let key_code = match key_name.as_str() {
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
        };

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

    window.connect_delete_event(|_, _| {
        IS_CLOSING.with(|c| c.set(true));
        gtk::main_quit();
        gtk::glib::Propagation::Proceed
    });

    window.connect_destroy(|_| {
        IS_CLOSING.with(|c| c.set(true));
        IS_MAXIMIZED_TRACKER.with(|t| t.set(false));
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
    drawing_area.grab_focus();
    gtk_set_cursor(crate::window::CursorIcon::Default);

    // Hardware VSync frame tick callback for smooth 120 FPS kinetic physics and animations
    let on_event_tick = on_event_rc.clone();
    drawing_area.add_tick_callback(move |widget, _frame_clock| {
        let tick_ev = crate::events::PlatformEvent::AnimationTick;
        if (on_event_tick.borrow_mut())(&tick_ev) {
            widget.queue_draw();
        }
        gtk::glib::ControlFlow::Continue
    });

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
