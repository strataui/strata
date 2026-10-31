//! IEEE standard schematic symbols and orthogonal netlist routing via analytical GPU Bézier paths.

use strata_gpu_path::{Path, PathBuilder, Point2D};

/// Generates IEEE standard resistor zig-zag symbol (6 peaks).
pub fn build_resistor_symbol(start: Point2D, length: f32) -> Path {
    let dx = length / 7.0;
    let amp = 8.0;

    PathBuilder::new()
        .move_to(start.x, start.y)
        .line_to(start.x + dx, start.y)
        .line_to(start.x + dx * 1.5, start.y - amp)
        .line_to(start.x + dx * 2.5, start.y + amp)
        .line_to(start.x + dx * 3.5, start.y - amp)
        .line_to(start.x + dx * 4.5, start.y + amp)
        .line_to(start.x + dx * 5.5, start.y - amp)
        .line_to(start.x + dx * 6.0, start.y)
        .line_to(start.x + length, start.y)
        .build()
}

/// Generates IEEE standard capacitor parallel plates symbol.
pub fn build_capacitor_symbol(center: Point2D, plate_height: f32, plate_gap: f32) -> Path {
    let half_h = plate_height * 0.5;
    let half_gap = plate_gap * 0.5;

    PathBuilder::new()
        // Left terminal
        .move_to(center.x - plate_height, center.y)
        .line_to(center.x - half_gap, center.y)
        // Left plate
        .move_to(center.x - half_gap, center.y - half_h)
        .line_to(center.x - half_gap, center.y + half_h)
        // Right plate
        .move_to(center.x + half_gap, center.y - half_h)
        .line_to(center.x + half_gap, center.y + half_h)
        // Right terminal
        .move_to(center.x + half_gap, center.y)
        .line_to(center.x + plate_height, center.y)
        .build()
}

/// Generates IEEE standard ground symbol (3 descending horizontal bars).
pub fn build_ground_symbol(top: Point2D) -> Path {
    PathBuilder::new()
        // Vertical stem
        .move_to(top.x, top.y)
        .line_to(top.x, top.y + 10.0)
        // Bar 1 (width 20)
        .move_to(top.x - 10.0, top.y + 10.0)
        .line_to(top.x + 10.0, top.y + 10.0)
        // Bar 2 (width 12)
        .move_to(top.x - 6.0, top.y + 14.0)
        .line_to(top.x + 6.0, top.y + 14.0)
        // Bar 3 (width 4)
        .move_to(top.x - 2.0, top.y + 18.0)
        .line_to(top.x + 2.0, top.y + 18.0)
        .build()
}

/// Generates IEEE standard logic inverter symbol (triangle with inversion bubble).
pub fn build_inverter_symbol(origin: Point2D, width: f32, height: f32) -> Path {
    let half_h = height * 0.5;
    let bubble_radius = 4.0;
    let tri_width = width - bubble_radius * 2.0;

    PathBuilder::new()
        // Input lead
        .move_to(origin.x, origin.y)
        .line_to(origin.x + 10.0, origin.y)
        // Triangle body
        .move_to(origin.x + 10.0, origin.y - half_h)
        .line_to(origin.x + 10.0 + tri_width, origin.y)
        .line_to(origin.x + 10.0, origin.y + half_h)
        .close()
        // Inversion bubble
        .move_to(origin.x + 10.0 + tri_width + bubble_radius * 2.0, origin.y)
        .quad_to(
            origin.x + 10.0 + tri_width + bubble_radius,
            origin.y - bubble_radius,
            origin.x + 10.0 + tri_width,
            origin.y,
        )
        .quad_to(
            origin.x + 10.0 + tri_width + bubble_radius,
            origin.y + bubble_radius,
            origin.x + 10.0 + tri_width + bubble_radius * 2.0,
            origin.y,
        )
        // Output lead
        .move_to(origin.x + 10.0 + width, origin.y)
        .line_to(origin.x + width + 20.0, origin.y)
        .build()
}

/// Generates orthogonal wire path between two points with 90-degree corner.
pub fn build_orthogonal_wire(from: Point2D, to: Point2D) -> Path {
    PathBuilder::new()
        .move_to(from.x, from.y)
        .line_to(to.x, from.y)
        .line_to(to.x, to.y)
        .build()
}
