//! Pure SVG Geometry & Path Generator.

use crate::scale::{ScaleBand, ScaleLinear};

/// Generates an SVG path string for a line chart series (`M x y L x y ...`).
pub fn line_path(points: &[(f32, f32)]) -> String {
    if points.is_empty() {
        return String::new();
    }

    let mut path = format!("M {:.2} {:.2}", points[0].0, points[0].1);
    for p in &points[1..] {
        path.push_str(&format!(" L {:.2} {:.2}", p.0, p.1));
    }
    path
}

/// Generates a closed SVG polygon path string for filled area charts.
pub fn area_path(points: &[(f32, f32)], baseline_y: f32) -> String {
    if points.is_empty() {
        return String::new();
    }

    let mut path = format!("M {:.2} {:.2}", points[0].0, points[0].1);
    for p in &points[1..] {
        path.push_str(&format!(" L {:.2} {:.2}", p.0, p.1));
    }
    let last_x = points.last().unwrap().0;
    let first_x = points[0].0;
    path.push_str(&format!(" L {:.2} {:.2} L {:.2} {:.2} Z", last_x, baseline_y, first_x, baseline_y));
    path
}

/// Computes bar rectangle geometries `(x, y, width, height)` for categorical bar charts.
pub fn bar_rects(
    categories: &[&str],
    values: &[f32],
    x_scale: &ScaleBand,
    y_scale: &ScaleLinear,
) -> Vec<(f32, f32, f32, f32)> {
    let mut rects = Vec::with_capacity(categories.len());
    let bw = x_scale.bandwidth();
    let base_y = y_scale.map(0.0);

    for (&cat, &val) in categories.iter().zip(values.iter()) {
        if let Some(x) = x_scale.map(cat) {
            let y = y_scale.map(val);
            let top = y.min(base_y);
            let height = (base_y - y).abs();
            rects.push((x, top, bw, height));
        }
    }

    rects
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_svg_path_generators() {
        let pts = vec![(0.0, 100.0), (50.0, 50.0), (100.0, 0.0)];
        let line = line_path(&pts);
        assert_eq!(line, "M 0.00 100.00 L 50.00 50.00 L 100.00 0.00");

        let area = area_path(&pts, 120.0);
        assert_eq!(area, "M 0.00 100.00 L 50.00 50.00 L 100.00 0.00 L 100.00 120.00 L 0.00 120.00 Z");
    }
}
