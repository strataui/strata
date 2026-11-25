//! Native Strata GTK/Cairo Window Runner and Software FrameBuffer Renderer.

use crate::fixture::{get_fixture_view, ParityLevel};
use gtk::prelude::*;
use std::thread;
use std::time::Duration;
use strata_render::compositor::{Compositor, DisplayList, DrawCommand};
use strata_render::rasterizer::FrameBuffer;

/// Draws a rounded rectangle path into a Cairo graphics context.
pub fn cairo_rounded_rect(cr: &cairo::Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    let r = r.min(w / 2.0).min(h / 2.0);
    let degrees = std::f64::consts::PI / 180.0;

    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -90.0 * degrees, 0.0 * degrees);
    cr.arc(x + w - r, y + h - r, r, 0.0 * degrees, 90.0 * degrees);
    cr.arc(x + r, y + h - r, r, 90.0 * degrees, 180.0 * degrees);
    cr.arc(x + r, y + r, r, 180.0 * degrees, 270.0 * degrees);
    cr.close_path();
}

/// Executes a Strata DisplayList into a Cairo graphics context.
pub fn render_display_list_to_cairo(cr: &cairo::Context, list: &DisplayList) {
    for cmd in list.commands() {
        if !cmd.is_clip() && !cmd.is_state_command() {
            if let Ok((cx1, cy1, cx2, cy2)) = cr.clip_extents() {
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
            DrawCommand::DrawBorder {
                bounds,
                widths,
                color,
                border_radius,
                ..
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
                        cr.line_to(
                            (bounds.x + bounds.width) as f64,
                            (bounds.y + bounds.height) as f64 - w / 2.0,
                        );
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
                        cr.line_to(
                            (bounds.x + bounds.width) as f64 - w / 2.0,
                            (bounds.y + bounds.height) as f64,
                        );
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
                is_monospace,
                ..
            } => {
                let weight = match font_weight {
                    strata_style::types::FontWeight::Bold
                    | strata_style::types::FontWeight::ExtraBold
                    | strata_style::types::FontWeight::Black
                    | strata_style::types::FontWeight::SemiBold => cairo::FontWeight::Bold,
                    _ => cairo::FontWeight::Normal,
                };
                let font_family = if *is_monospace {
                    "monospace"
                } else {
                    "sans-serif"
                };
                cr.select_font_face(font_family, cairo::FontSlant::Normal, weight);
                cr.set_font_size(*font_size as f64);
                cr.set_source_rgba(
                    color.r() as f64,
                    color.g() as f64,
                    color.b() as f64,
                    color.a() as f64,
                );
                // In CSS line box, baseline is at top + half_leading + ascent (ascent ~ 0.85 * font_size)
                let half_leading = (font_size * line_height - font_size).max(0.0) / 2.0;
                let baseline_y = (origin.1 + half_leading + font_size * 0.85) as f64;
                cr.move_to(origin.0 as f64, baseline_y);
                let _ = cr.show_text(text);
            }
            _ => {}
        }
    }
}

/// Runs the live Strata desktop window on DISPLAY=:1 maximized, takes a screenshot, and exits.
pub fn run_strata_window(level: ParityLevel, screenshot_path: &str) {
    if gtk::init().is_err() {
        eprintln!("[!] Failed to initialize GTK on current DISPLAY.");
        return;
    }

    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title("Parity Benchmark - Strata");
    window.set_default_size(1920, 1080);
    window.set_decorated(false);
    window.maximize();

    let drawing_area = gtk::DrawingArea::new();

    drawing_area.connect_draw(move |widget, cr| {
        let width = widget.allocated_width() as f32;
        let height = widget.allocated_height() as f32;

        let view = get_fixture_view(level);
        let mut layout_root = strata::AppRuntime::build_layout_tree(&view.root);
        layout_root.compute_layout(width, height);

        let display_list = Compositor::build_display_list(&layout_root);

        // Fill background
        cr.set_source_rgb(0.0, 0.0, 0.0);
        let _ = cr.paint();

        render_display_list_to_cairo(cr, &display_list);

        gtk::glib::Propagation::Proceed
    });

    window.add(&drawing_area);
    window.show_all();

    let path_str = screenshot_path.to_string();
    thread::spawn(move || {
        println!(
            "[Strata] Window opened maximized (decorated: false). Waiting 2.0s for presentation..."
        );
        thread::sleep(Duration::from_millis(2000));
        println!(
            "[Strata] Capturing window-scoped snapshot to {}...",
            path_str
        );
        let captured =
            crate::capture::capture_window_by_title("Parity Benchmark - Strata", &path_str);
        println!("[Strata] Window capture result: {}", captured);
        thread::sleep(Duration::from_millis(500));
        gtk::glib::idle_add(|| {
            gtk::main_quit();
            gtk::glib::ControlFlow::Break
        });
    });

    gtk::main();
}

/// Also produces the pure-Rust software rasterized FrameBuffer image.
pub fn render_strata_software_framebuffer(
    level: ParityLevel,
    width: usize,
    height: usize,
    output_path: &str,
) {
    let view = get_fixture_view(level);
    let mut layout_root = strata::AppRuntime::build_layout_tree(&view.root);
    layout_root.compute_layout(width as f32, height as f32);

    let display_list = Compositor::build_display_list(&layout_root);

    let mut fb = FrameBuffer::new(width, height);
    fb.clear(strata_style::types::Color::rgba(9, 9, 11, 255));
    fb.execute_display_list(&display_list);

    // Save as PNG
    let img: image::RgbaImage =
        image::ImageBuffer::from_raw(width as u32, height as u32, fb.pixels)
            .expect("Buffer size matches dimensions");
    let _ = img.save(output_path);
    println!("[Strata] Software FrameBuffer saved to: {}", output_path);
}
