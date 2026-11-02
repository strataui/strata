//! High-Fidelity Native GTK3 and Cairo Presentation Backend for Linux.
//!
//! Connects Strata's DisplayList directly to Cairo graphics context with system typography,
//! FreeType/HarfBuzz font shaping, and subpixel anti-aliased vector geometry.

use strata_render::compositor::{DisplayList, DrawCommand};
use strata_style::types::FontWeight;

/// Draws a rounded rectangle path into a Cairo graphics context.
pub fn cairo_rounded_rect(
    cr: &cairo::Context,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    r: f64,
) {
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
        if large_arc == sweep { -s } else { s }
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
        if ux * vy - uy * vx < 0.0 { -ang } else { ang }
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
    let num_segments = (d_theta.abs() / std::f64::consts::FRAC_PI_2).ceil().max(1.0) as usize;
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

                    while let (Some(x), Some(y)) = (next_number(&mut chars), next_number(&mut chars)) {
                        current_x = if is_rel { current_x + x } else { x };
                        current_y = if is_rel { current_y + y } else { y };
                        cr.line_to(current_x, current_y);
                    }
                }
                'L' | 'l' => {
                    let is_rel = cmd == 'l';
                    while let (Some(x), Some(y)) = (next_number(&mut chars), next_number(&mut chars)) {
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
                        let (cx1, cy1) = if is_rel { (current_x + x1, current_y + y1) } else { (x1, y1) };
                        let (cx2, cy2) = if is_rel { (current_x + x2, current_y + y2) } else { (x2, y2) };
                        let (cx, cy) = if is_rel { (current_x + x, current_y + y) } else { (x, y) };
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
                        let (cx2, cy2) = if is_rel { (current_x + x2, current_y + y2) } else { (x2, y2) };
                        let (cx, cy) = if is_rel { (current_x + x, current_y + y) } else { (x, y) };
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
                        let (qx, qy) = if is_rel { (current_x + x1, current_y + y1) } else { (x1, y1) };
                        let (cx, cy) = if is_rel { (current_x + x, current_y + y) } else { (x, y) };
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
                    while let (Some(x), Some(y)) = (next_number(&mut chars), next_number(&mut chars)) {
                        let (cx, cy) = if is_rel { (current_x + x, current_y + y) } else { (x, y) };
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
                        let Some(ry) = next_number(&mut chars) else { break; };
                        let Some(x_rot) = next_number(&mut chars) else { break; };
                        let Some(large_arc) = next_flag(&mut chars) else { break; };
                        let Some(sweep) = next_flag(&mut chars) else { break; };
                        let Some(x) = next_number(&mut chars) else { break; };
                        let Some(y) = next_number(&mut chars) else { break; };

                        let target_x = if is_rel { current_x + x } else { x };
                        let target_y = if is_rel { current_y + y } else { y };

                        arc_to_cairo(cr, current_x, current_y, rx, ry, x_rot, large_arc, sweep, target_x, target_y);
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

/// Executes a Strata DisplayList into a Cairo graphics context with full typography and vector support.
pub fn render_display_list_to_cairo(cr: &cairo::Context, list: &DisplayList) {
    for cmd in list.commands() {
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
            DrawCommand::DrawBorder {
                bounds,
                widths,
                color,
                border_radius,
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
                    let w = widths.top as f64;
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
                    cr.set_line_width(w);
                    let _ = cr.stroke();
                } else {
                    if widths.top > 0.0 {
                        let w = widths.top as f64;
                        cr.set_line_width(w);
                        cr.move_to(bounds.x as f64, bounds.y as f64 + w / 2.0);
                        cr.line_to((bounds.x + bounds.width) as f64, bounds.y as f64 + w / 2.0);
                        let _ = cr.stroke();
                    }
                    if widths.bottom > 0.0 {
                        let w = widths.bottom as f64;
                        cr.set_line_width(w);
                        cr.move_to(bounds.x as f64, (bounds.y + bounds.height) as f64 - w / 2.0);
                        cr.line_to((bounds.x + bounds.width) as f64, (bounds.y + bounds.height) as f64 - w / 2.0);
                        let _ = cr.stroke();
                    }
                    if widths.left > 0.0 {
                        let w = widths.left as f64;
                        cr.set_line_width(w);
                        cr.move_to(bounds.x as f64 + w / 2.0, bounds.y as f64);
                        cr.line_to(bounds.x as f64 + w / 2.0, (bounds.y + bounds.height) as f64);
                        let _ = cr.stroke();
                    }
                    if widths.right > 0.0 {
                        let w = widths.right as f64;
                        cr.set_line_width(w);
                        cr.move_to((bounds.x + bounds.width) as f64 - w / 2.0, bounds.y as f64);
                        cr.line_to((bounds.x + bounds.width) as f64 - w / 2.0, (bounds.y + bounds.height) as f64);
                        let _ = cr.stroke();
                    }
                }
            }
            DrawCommand::DrawText {
                origin,
                text,
                font_size,
                line_height,
                font_weight,
                color,
            } => {
                let weight = match font_weight {
                    FontWeight::Bold
                    | FontWeight::ExtraBold
                    | FontWeight::Black
                    | FontWeight::SemiBold => cairo::FontWeight::Bold,
                    _ => cairo::FontWeight::Normal,
                };
                cr.select_font_face("Open Sans", cairo::FontSlant::Normal, weight);
                cr.set_font_size(*font_size as f64);
                cr.set_source_rgba(
                    color.r() as f64,
                    color.g() as f64,
                    color.b() as f64,
                    color.a() as f64,
                );
                let half_leading = (font_size * line_height - font_size).max(0.0) / 2.0;
                let baseline_y = (origin.1 + half_leading + font_size * 0.85) as f64;
                cr.move_to(origin.0 as f64, baseline_y);
                let _ = cr.show_text(text);
            }
            DrawCommand::DrawSvgPath {
                bounds,
                path_data,
                stroke_color,
                fill_color,
                stroke_width,
                view_box,
            } => {
                if bounds.width > 0.0 && bounds.height > 0.0 && view_box.0 > 0.0 && view_box.1 > 0.0 {
                    let _ = cr.save();
                    let scale_x = bounds.width as f64 / view_box.0 as f64;
                    let scale_y = bounds.height as f64 / view_box.1 as f64;
                    cr.translate(bounds.x as f64, bounds.y as f64);
                    cr.scale(scale_x, scale_y);

                    cr.new_path();
                    cairo_interpret_svg_path(cr, path_data);

                    if let Some(fill) = fill_color {
                        cr.set_source_rgba(fill.r() as f64, fill.g() as f64, fill.b() as f64, fill.a() as f64);
                        if stroke_color.is_some() {
                            let _ = cr.fill_preserve();
                        } else {
                            let _ = cr.fill();
                        }
                    }
                    if let Some(stroke) = stroke_color {
                        cr.set_source_rgba(stroke.r() as f64, stroke.g() as f64, stroke.b() as f64, stroke.a() as f64);
                        cr.set_line_width(*stroke_width as f64);
                        cr.set_line_cap(cairo::LineCap::Round);
                        cr.set_line_join(cairo::LineJoin::Round);
                        let _ = cr.stroke();
                    }
                    let _ = cr.restore();
                }
            }
            _ => {}
        }
    }
}

/// Renders a DisplayList into an in-memory Cairo ImageSurface and saves it as a PNG file.
pub fn render_to_png(display_list: &DisplayList, width: i32, height: i32, path: &str) -> Result<(), String> {
    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, width, height)
        .map_err(|e| format!("Failed to create Cairo image surface: {:?}", e))?;
    let cr = cairo::Context::new(&surface)
        .map_err(|e| format!("Failed to create Cairo context: {:?}", e))?;

    // Fill background black
    cr.set_source_rgb(0.035, 0.035, 0.043);
    let _ = cr.paint();

    render_display_list_to_cairo(&cr, display_list);

    let mut file = std::fs::File::create(path)
        .map_err(|e| format!("Failed to create output file: {}", e))?;
    surface.write_to_png(&mut file)
        .map_err(|e| format!("Failed to write PNG: {:?}", e))?;

    Ok(())
}

thread_local! {
    static CURRENT_GTK_WINDOW: std::cell::RefCell<Option<gtk::Window>> = const { std::cell::RefCell::new(None) };
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
pub fn gtk_toggle_maximize() {
    CURRENT_GTK_WINDOW.with(|w| {
        use gtk::prelude::*;
        if let Some(win) = w.borrow().as_ref() {
            if win.is_maximized() {
                win.unmaximize();
            } else {
                win.maximize();
            }
        }
    });
}

/// Closes the current native GTK desktop window.
pub fn gtk_close() {
    CURRENT_GTK_WINDOW.with(|w| {
        use gtk::prelude::*;
        if let Some(win) = w.borrow().as_ref() {
            win.close();
        } else {
            gtk::main_quit();
        }
    });
}

/// Reads current system clipboard text content via GTK.
pub fn gtk_get_clipboard_text() -> Option<String> {
    let clip = gtk::Clipboard::get(&gtk::gdk::SELECTION_CLIPBOARD);
    clip.wait_for_text().map(|s| s.to_string())
}

/// Sets current system clipboard text content via GTK.
pub fn gtk_set_clipboard_text(text: &str) {
    let clip = gtk::Clipboard::get(&gtk::gdk::SELECTION_CLIPBOARD);
    clip.set_text(text);
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
            | gtk::gdk::EventMask::STRUCTURE_MASK,
    );
    drawing_area.set_can_focus(true);

    let on_draw_rc = std::rc::Rc::new(std::cell::RefCell::new(on_draw));
    let on_draw_cb = on_draw_rc.clone();
    drawing_area.connect_draw(move |widget, cr| {
        let w = widget.allocated_width() as f32;
        let h = widget.allocated_height() as f32;
        (on_draw_cb.borrow_mut())(w, h, cr);
        gtk::glib::Propagation::Proceed
    });

    let on_event_rc = std::rc::Rc::new(std::cell::RefCell::new(on_event));

    // Mouse button click handler
    let on_event_mouse = on_event_rc.clone();
    drawing_area.connect_button_press_event(move |widget, event| {
        let (x, y) = event.position();
        let btn = match event.button() {
            1 => crate::events::MouseButton::Left,
            2 => crate::events::MouseButton::Middle,
            3 => crate::events::MouseButton::Right,
            b => crate::events::MouseButton::Other(b as u16),
        };

        let plat_event = crate::events::PlatformEvent::MouseDown {
            button: btn,
            x: x as f32,
            y: y as f32,
        };

        if (on_event_mouse.borrow_mut())(&plat_event) {
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

    window.connect_destroy(|_| {
        CURRENT_GTK_WINDOW.with(|w| {
            *w.borrow_mut() = None;
        });
        gtk::main_quit();
    });

    window.add(&drawing_area);
    window.show_all();
    drawing_area.grab_focus();

    gtk::main();

    Ok(())
}
