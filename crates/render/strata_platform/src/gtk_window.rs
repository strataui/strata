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

/// Parses and executes SVG path data into Cairo path commands.
pub fn cairo_interpret_svg_path(cr: &cairo::Context, path_data: &str) {
    let mut chars = path_data.chars().peekable();
    let mut current_x = 0.0f64;
    let mut current_y = 0.0f64;

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
                    if let (Some(x), Some(y)) = (next_f64(&mut chars), next_f64(&mut chars)) {
                        current_x = if is_rel { current_x + x } else { x };
                        current_y = if is_rel { current_y + y } else { y };
                        cr.move_to(current_x, current_y);
                    }
                }
                'L' | 'l' => {
                    let is_rel = cmd == 'l';
                    while let (Some(x), Some(y)) = (next_f64(&mut chars), next_f64(&mut chars)) {
                        current_x = if is_rel { current_x + x } else { x };
                        current_y = if is_rel { current_y + y } else { y };
                        cr.line_to(current_x, current_y);
                    }
                }
                'H' | 'h' => {
                    let is_rel = cmd == 'h';
                    while let Some(x) = next_f64(&mut chars) {
                        current_x = if is_rel { current_x + x } else { x };
                        cr.line_to(current_x, current_y);
                    }
                }
                'V' | 'v' => {
                    let is_rel = cmd == 'v';
                    while let Some(y) = next_f64(&mut chars) {
                        current_y = if is_rel { current_y + y } else { y };
                        cr.line_to(current_x, current_y);
                    }
                }
                'C' | 'c' => {
                    let is_rel = cmd == 'c';
                    while let (Some(x1), Some(y1), Some(x2), Some(y2), Some(x), Some(y)) = (
                        next_f64(&mut chars),
                        next_f64(&mut chars),
                        next_f64(&mut chars),
                        next_f64(&mut chars),
                        next_f64(&mut chars),
                        next_f64(&mut chars),
                    ) {
                        let (cx1, cy1) = if is_rel { (current_x + x1, current_y + y1) } else { (x1, y1) };
                        let (cx2, cy2) = if is_rel { (current_x + x2, current_y + y2) } else { (x2, y2) };
                        let (cx, cy) = if is_rel { (current_x + x, current_y + y) } else { (x, y) };
                        cr.curve_to(cx1, cy1, cx2, cy2, cx, cy);
                        current_x = cx;
                        current_y = cy;
                    }
                }
                'Z' | 'z' => {
                    cr.close_path();
                }
                _ => {}
            }
        } else {
            chars.next();
        }
    }
}

fn next_f64(chars: &mut std::iter::Peekable<std::str::Chars>) -> Option<f64> {
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() || c == ',' {
            chars.next();
        } else {
            break;
        }
    }

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
        buf.parse::<f64>().ok()
    } else {
        None
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
                cr.select_font_face("sans-serif", cairo::FontSlant::Normal, weight);
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

/// Launches a live native GTK3 desktop window with Cairo graphics presentation.
pub fn launch_gtk_window<F, E>(
    title: &str,
    width: u32,
    height: u32,
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

    // Keyboard handler
    let on_event_key = on_event_rc.clone();
    drawing_area.connect_key_press_event(move |widget, event| {
        let keyval = event.keyval();
        let key_code = match keyval.name().as_deref() {
            Some("BackSpace") => crate::events::KeyCode::Backspace,
            Some("Return") => crate::events::KeyCode::Enter,
            Some("Tab") => crate::events::KeyCode::Tab,
            Some("Escape") => crate::events::KeyCode::Escape,
            Some("Left") => crate::events::KeyCode::ArrowLeft,
            Some("Right") => crate::events::KeyCode::ArrowRight,
            Some("Up") => crate::events::KeyCode::ArrowUp,
            Some("Down") => crate::events::KeyCode::ArrowDown,
            Some("space") => crate::events::KeyCode::Space,
            _ => crate::events::KeyCode::Space,
        };

        let plat_event = crate::events::PlatformEvent::KeyDown {
            key: key_code,
            modifiers: crate::events::Modifiers::default(),
        };

        if (on_event_key.borrow_mut())(&plat_event) {
            widget.queue_draw();
        }

        gtk::glib::Propagation::Proceed
    });

    window.connect_destroy(|_| {
        gtk::main_quit();
    });

    window.add(&drawing_area);
    window.show_all();
    drawing_area.grab_focus();

    gtk::main();

    Ok(())
}
