//! Image decoding, base64 data URI handling, SVG path conversion, and screenshots.

use super::*;

/// Returns the base64 payload of a `data:` URI, if the URI declares base64 encoding.
pub(crate) fn data_uri_base64_payload(src: &str) -> Option<&str> {
    let trimmed = src.trim();
    let bytes = trimmed.as_bytes();
    if bytes.len() < 5 || !bytes[..5].eq_ignore_ascii_case(b"data:") {
        return None;
    }
    let needle = b";base64,";
    bytes
        .windows(needle.len())
        .position(|window| window.eq_ignore_ascii_case(needle))
        .map(|index| &trimmed[index + needle.len()..])
}

/// Decodes a standard-alphabet base64 payload, ignoring ASCII whitespace.
pub(crate) fn decode_base64(input: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let mut accumulator: u32 = 0;
    let mut bits: u32 = 0;
    let mut padding = 0usize;
    for &byte in input.as_bytes() {
        if byte.is_ascii_whitespace() {
            continue;
        }
        if byte == b'=' {
            padding += 1;
            continue;
        }
        if padding > 0 {
            return None;
        }
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        };
        accumulator = (accumulator << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((accumulator >> bits) as u8);
            accumulator &= (1u32 << bits) - 1;
        }
    }
    if padding > 2 {
        return None;
    }
    Some(out)
}

/// Reads the intrinsic width and height from a PNG signature and IHDR chunk.
pub(crate) fn png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    const SIGNATURE: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];
    if bytes.len() < 24 || bytes[..8] != SIGNATURE || &bytes[12..16] != b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
    if width == 0 || height == 0 {
        return None;
    }
    Some((width, height))
}

impl AppRuntime {
    /// Captures a high-resolution PNG screenshot with the active MCP cursor composited.
    pub fn mcp_capture_screenshot_png(&mut self) -> Result<Vec<u8>, String> {
        let overlay = self.mcp_cursor.as_ref().map(|c| (c, &self.cursor_theme));
        strata_platform::render_to_png_bytes(
            &self.display_list,
            self.viewport_width as i32,
            self.viewport_height as i32,
            overlay,
        )
    }

    /// Helper to convert SVG points attribute into path commands.
    pub(crate) fn parse_points_to_path(points_str: &str, closed: bool) -> Option<String> {
        let coords: Vec<f32> = points_str
            .replace(',', " ")
            .split_whitespace()
            .filter_map(|s| s.parse::<f32>().ok())
            .collect();
        if coords.len() < 2 {
            return None;
        }
        let mut path = format!("M {} {}", coords[0], coords[1]);
        for chunk in coords[2..].chunks(2) {
            if chunk.len() == 2 {
                path.push_str(&format!(" L {} {}", chunk[0], chunk[1]));
            }
        }
        if closed {
            path.push_str(" Z");
        }
        Some(path)
    }

    /// Helper to parse a color or CSS `var(--token, fallback)` string.
    pub(crate) fn parse_svg_color_attr(value: &str) -> Option<strata_style::types::Color> {
        let trimmed = value.trim();
        if let Some(inner) = trimmed
            .strip_prefix("var(")
            .and_then(|s| s.strip_suffix(')'))
        {
            if let Some((_, fallback)) = inner.split_once(',') {
                return strata_style::parser::parse_color(fallback.trim());
            }
        }
        strata_style::parser::parse_color(trimmed)
    }

    /// Converts a possibly-rotated ellipse (or circle when rx == ry) into an exact 4-segment cubic Bezier SVG path.
    pub(crate) fn ellipse_to_cubic_bezier_path(
        cx: f32,
        cy: f32,
        rx: f32,
        ry: f32,
        rot_deg: f32,
        rot_cx: f32,
        rot_cy: f32,
    ) -> String {
        const KAPPA: f32 = 0.552_284_8;
        let theta = rot_deg.to_radians();
        let cos_t = theta.cos();
        let sin_t = theta.sin();

        // Rotate ellipse center around (rot_cx, rot_cy) if needed
        let dx = cx - rot_cx;
        let dy = cy - rot_cy;
        let ecx = rot_cx + dx * cos_t - dy * sin_t;
        let ecy = rot_cy + dx * sin_t + dy * cos_t;

        // Semi-axis vectors in rotated frame
        let ux = rx * cos_t;
        let uy = rx * sin_t;
        let vx = -ry * sin_t;
        let vy = ry * cos_t;

        let p0 = (ecx + ux, ecy + uy);
        let p1 = (ecx + vx, ecy + vy);
        let p2 = (ecx - ux, ecy - uy);
        let p3 = (ecx - vx, ecy - vy);

        let c0_1 = (p0.0 + KAPPA * vx, p0.1 + KAPPA * vy);
        let c0_2 = (p1.0 + KAPPA * ux, p1.1 + KAPPA * uy);

        let c1_1 = (p1.0 - KAPPA * ux, p1.1 - KAPPA * uy);
        let c1_2 = (p2.0 + KAPPA * vx, p2.1 + KAPPA * vy);

        let c2_1 = (p2.0 - KAPPA * vx, p2.1 - KAPPA * vy);
        let c2_2 = (p3.0 - KAPPA * ux, p3.1 - KAPPA * uy);

        let c3_1 = (p3.0 + KAPPA * ux, p3.1 + KAPPA * uy);
        let c3_2 = (p0.0 - KAPPA * vx, p0.1 - KAPPA * vy);

        format!(
        "M {:.3} {:.3} C {:.3} {:.3} {:.3} {:.3} {:.3} {:.3} C {:.3} {:.3} {:.3} {:.3} {:.3} {:.3} C {:.3} {:.3} {:.3} {:.3} {:.3} {:.3} C {:.3} {:.3} {:.3} {:.3} {:.3} {:.3} Z",
        p0.0, p0.1,
        c0_1.0, c0_1.1, c0_2.0, c0_2.1, p1.0, p1.1,
        c1_1.0, c1_1.1, c1_2.0, c1_2.1, p2.0, p2.1,
        c2_1.0, c2_1.1, c2_2.0, c2_2.1, p3.0, p3.1,
        c3_1.0, c3_1.1, c3_2.0, c3_2.1, p0.0, p0.1,
    )
    }

    /// Helper function to extract all SVG vector shapes from a blueprint hierarchy.
    pub(crate) fn collect_svg_items_from_blueprint(
        blueprint: &NodeBlueprint,
        default_view_box: (f32, f32),
        inherited_stroke: Option<strata_style::types::Color>,
        inherited_fill: Option<strata_style::types::Color>,
        inherited_stroke_width: f32,
        inherited_linecap: strata_layout::StrokeLineCap,
        inherited_linejoin: strata_layout::StrokeLineJoin,
        inherited_dasharray: Vec<f32>,
        inherited_fill_rule: strata_layout::FillRule,
        parent_color: strata_style::types::Color,
    ) -> Vec<strata_layout::SvgItem> {
        Self::collect_svg_items_with_transform(
            blueprint,
            default_view_box,
            inherited_stroke,
            inherited_fill,
            inherited_stroke_width,
            inherited_linecap,
            inherited_linejoin,
            inherited_dasharray,
            inherited_fill_rule,
            parent_color,
            (0.0, default_view_box.0 * 0.5, default_view_box.1 * 0.5),
        )
    }

    pub(crate) fn collect_svg_items_with_transform(
        blueprint: &NodeBlueprint,
        default_view_box: (f32, f32),
        inherited_stroke: Option<strata_style::types::Color>,
        inherited_fill: Option<strata_style::types::Color>,
        inherited_stroke_width: f32,
        inherited_linecap: strata_layout::StrokeLineCap,
        inherited_linejoin: strata_layout::StrokeLineJoin,
        inherited_dasharray: Vec<f32>,
        inherited_fill_rule: strata_layout::FillRule,
        parent_color: strata_style::types::Color,
        inherited_rotation: (f32, f32, f32),
    ) -> Vec<strata_layout::SvgItem> {
        let mut items = Vec::new();
        match blueprint {
            NodeBlueprint::Element {
                tag,
                attributes,
                children,
                ..
            } => {
                let mut d = None;
                let mut stroke = inherited_stroke;
                let mut fill = inherited_fill;
                let mut stroke_width = inherited_stroke_width;
                let mut stroke_linecap = inherited_linecap;
                let mut stroke_linejoin = inherited_linejoin;
                let mut stroke_dasharray = inherited_dasharray.clone();
                let mut fill_rule = inherited_fill_rule;
                let mut rotation = inherited_rotation;

                let mut x1 = 0.0f32;
                let mut y1 = 0.0f32;
                let mut x2 = 0.0f32;
                let mut y2 = 0.0f32;
                let mut has_line = false;

                let mut cx = 0.0f32;
                let mut cy = 0.0f32;
                let mut r = 0.0f32;
                let mut rx = 0.0f32;
                let mut ry = 0.0f32;
                let mut has_circle = false;
                let mut has_ellipse = false;

                let mut rect_x = 0.0f32;
                let mut rect_y = 0.0f32;
                let mut rect_w = 0.0f32;
                let mut rect_h = 0.0f32;
                let mut rect_rx = 0.0f32;
                let mut has_rect = false;

                let mut points = None;
                let is_polygon = match tag {
                    Tag::Custom(s) => *s == "polygon",
                    _ => false,
                };
                let is_rect_tag = match tag {
                    Tag::Custom(s) => *s == "rect",
                    _ => false,
                };

                for attr in attributes {
                    if let strata_dom::Attribute::Property { key, value } = attr {
                        match key.as_str() {
                            "d" => d = Some(value.clone()),
                            "stroke" => {
                                if value == "currentColor" {
                                    stroke = Some(parent_color);
                                } else if value == "none" {
                                    stroke = None;
                                } else if let Some(c) = Self::parse_svg_color_attr(value) {
                                    stroke = Some(c);
                                }
                            }
                            "fill" => {
                                if value == "currentColor" {
                                    fill = Some(parent_color);
                                } else if value == "none" {
                                    fill = None;
                                } else if let Some(c) = Self::parse_svg_color_attr(value) {
                                    fill = Some(c);
                                }
                            }
                            "transform" => {
                                let trimmed = value.trim();
                                if let Some(inner) = trimmed
                                    .strip_prefix("rotate(")
                                    .and_then(|s| s.strip_suffix(')'))
                                {
                                    let nums: Vec<f32> = inner
                                        .replace(',', " ")
                                        .split_whitespace()
                                        .filter_map(|s| s.parse::<f32>().ok())
                                        .collect();
                                    if let Some(&deg) = nums.first() {
                                        let rcx = nums
                                            .get(1)
                                            .copied()
                                            .unwrap_or(default_view_box.0 * 0.5);
                                        let rcy = nums
                                            .get(2)
                                            .copied()
                                            .unwrap_or(default_view_box.1 * 0.5);
                                        rotation = (rotation.0 + deg, rcx, rcy);
                                    }
                                }
                            }
                            "stroke-width" => {
                                if let Ok(w) = value.parse::<f32>() {
                                    stroke_width = w;
                                }
                            }
                            "stroke-linecap" => match value.as_str() {
                                "round" => stroke_linecap = strata_layout::StrokeLineCap::Round,
                                "square" => stroke_linecap = strata_layout::StrokeLineCap::Square,
                                "butt" => stroke_linecap = strata_layout::StrokeLineCap::Butt,
                                _ => {}
                            },
                            "stroke-linejoin" => match value.as_str() {
                                "round" => stroke_linejoin = strata_layout::StrokeLineJoin::Round,
                                "bevel" => stroke_linejoin = strata_layout::StrokeLineJoin::Bevel,
                                "miter" => stroke_linejoin = strata_layout::StrokeLineJoin::Miter,
                                _ => {}
                            },
                            "stroke-dasharray" => {
                                stroke_dasharray = value
                                    .replace(',', " ")
                                    .split_whitespace()
                                    .filter_map(|s| s.parse::<f32>().ok())
                                    .collect();
                            }
                            "fill-rule" => match value.as_str() {
                                "evenodd" => fill_rule = strata_layout::FillRule::EvenOdd,
                                "nonzero" => fill_rule = strata_layout::FillRule::NonZero,
                                _ => {}
                            },
                            "points" => points = Some(value.clone()),
                            "x1" => {
                                x1 = value.parse().unwrap_or(0.0);
                                has_line = true;
                            }
                            "y1" => {
                                y1 = value.parse().unwrap_or(0.0);
                                has_line = true;
                            }
                            "x2" => {
                                x2 = value.parse().unwrap_or(0.0);
                                has_line = true;
                            }
                            "y2" => {
                                y2 = value.parse().unwrap_or(0.0);
                                has_line = true;
                            }
                            "cx" => {
                                cx = value.parse().unwrap_or(0.0);
                            }
                            "cy" => {
                                cy = value.parse().unwrap_or(0.0);
                            }
                            "r" => {
                                r = value.parse().unwrap_or(0.0);
                                has_circle = true;
                            }
                            "rx" => {
                                let v = value.parse().unwrap_or(0.0);
                                rx = v;
                                rect_rx = v;
                            }
                            "ry" => {
                                ry = value.parse().unwrap_or(0.0);
                                has_ellipse = true;
                            }
                            "x" => {
                                rect_x = value.parse().unwrap_or(0.0);
                            }
                            "y" => {
                                rect_y = value.parse().unwrap_or(0.0);
                            }
                            "width" => {
                                rect_w = value.parse().unwrap_or(0.0);
                                has_rect = true;
                            }
                            "height" => {
                                rect_h = value.parse().unwrap_or(0.0);
                                has_rect = true;
                            }
                            _ => {}
                        }
                    }
                }

                if let Some(path_str) = d {
                    items.push(strata_layout::SvgItem {
                        path_data: path_str,
                        stroke_color: stroke,
                        fill_color: fill,
                        stroke_width,
                        stroke_linecap,
                        stroke_linejoin,
                        stroke_dasharray: stroke_dasharray.clone(),
                        fill_rule,
                        view_box: default_view_box,
                    });
                } else if let Some(pts) = points {
                    if let Some(poly_path) = Self::parse_points_to_path(&pts, is_polygon) {
                        items.push(strata_layout::SvgItem {
                            path_data: poly_path,
                            stroke_color: stroke,
                            fill_color: fill,
                            stroke_width,
                            stroke_linecap,
                            stroke_linejoin,
                            stroke_dasharray: stroke_dasharray.clone(),
                            fill_rule,
                            view_box: default_view_box,
                        });
                    }
                } else if has_line {
                    items.push(strata_layout::SvgItem {
                        path_data: format!("M {} {} L {} {}", x1, y1, x2, y2),
                        stroke_color: stroke.or(Some(parent_color)),
                        fill_color: None,
                        stroke_width,
                        stroke_linecap,
                        stroke_linejoin,
                        stroke_dasharray: stroke_dasharray.clone(),
                        fill_rule,
                        view_box: default_view_box,
                    });
                } else if has_circle && r > 0.0 {
                    items.push(strata_layout::SvgItem {
                        path_data: Self::ellipse_to_cubic_bezier_path(
                            cx, cy, r, r, rotation.0, rotation.1, rotation.2,
                        ),
                        stroke_color: stroke,
                        fill_color: fill,
                        stroke_width,
                        stroke_linecap,
                        stroke_linejoin,
                        stroke_dasharray: stroke_dasharray.clone(),
                        fill_rule,
                        view_box: default_view_box,
                    });
                } else if has_ellipse && rx > 0.0 && ry > 0.0 {
                    items.push(strata_layout::SvgItem {
                        path_data: Self::ellipse_to_cubic_bezier_path(
                            cx, cy, rx, ry, rotation.0, rotation.1, rotation.2,
                        ),
                        stroke_color: stroke,
                        fill_color: fill,
                        stroke_width,
                        stroke_linecap,
                        stroke_linejoin,
                        stroke_dasharray: stroke_dasharray.clone(),
                        fill_rule,
                        view_box: default_view_box,
                    });
                } else if is_rect_tag && has_rect && rect_w > 0.0 && rect_h > 0.0 {
                    let rect_path = if rect_rx > 0.0 {
                        const K: f32 = 0.552_284_8;
                        let r = rect_rx.min(rect_w * 0.5).min(rect_h * 0.5);
                        let x0 = rect_x;
                        let y0 = rect_y;
                        let x1 = rect_x + rect_w;
                        let y1 = rect_y + rect_h;
                        let c = r * (1.0 - K);
                        format!(
                        "M {:.3} {:.3} L {:.3} {:.3} C {:.3} {:.3} {:.3} {:.3} {:.3} {:.3} L {:.3} {:.3} C {:.3} {:.3} {:.3} {:.3} {:.3} {:.3} L {:.3} {:.3} C {:.3} {:.3} {:.3} {:.3} {:.3} {:.3} L {:.3} {:.3} C {:.3} {:.3} {:.3} {:.3} {:.3} {:.3} Z",
                        x0 + r, y0,
                        x1 - r, y0,
                        x1 - c, y0, x1, y0 + c, x1, y0 + r,
                        x1, y1 - r,
                        x1, y1 - c, x1 - c, y1, x1 - r, y1,
                        x0 + r, y1,
                        x0 + c, y1, x0, y1 - c, x0, y1 - r,
                        x0, y0 + r,
                        x0, y0 + c, x0 + c, y0, x0 + r, y0,
                    )
                    } else {
                        format!(
                            "M {} {} h {} v {} h {} Z",
                            rect_x, rect_y, rect_w, rect_h, -rect_w
                        )
                    };
                    items.push(strata_layout::SvgItem {
                        path_data: rect_path,
                        stroke_color: stroke,
                        fill_color: fill,
                        stroke_width,
                        stroke_linecap,
                        stroke_linejoin,
                        stroke_dasharray: stroke_dasharray.clone(),
                        fill_rule,
                        view_box: default_view_box,
                    });
                }

                // Recurse for children (e.g. <g>, or nested paths)
                for child in children {
                    let child_items = Self::collect_svg_items_with_transform(
                        child,
                        default_view_box,
                        stroke,
                        fill,
                        stroke_width,
                        stroke_linecap,
                        stroke_linejoin,
                        stroke_dasharray.clone(),
                        fill_rule,
                        parent_color,
                        rotation,
                    );
                    items.extend(child_items);
                }
            }
            NodeBlueprint::Fragment(children) => {
                for child in children {
                    let child_items = Self::collect_svg_items_with_transform(
                        child,
                        default_view_box,
                        inherited_stroke,
                        inherited_fill,
                        inherited_stroke_width,
                        inherited_linecap,
                        inherited_linejoin,
                        inherited_dasharray.clone(),
                        inherited_fill_rule,
                        parent_color,
                        inherited_rotation,
                    );
                    items.extend(child_items);
                }
            }
            _ => {}
        }
        items
    }

}
