use strata_gpu_path::{
    distance_to_quadratic, is_point_inside_closed_path, AnalyticGlyphCache, CubicBezier,
    FillRule, GpuCurveInstance, PathBuilder, PathCommandBuffer, Point2D, QuadraticBezier,
};

#[test]
fn test_quadratic_bezier_evaluation_and_split() {
    let p0 = Point2D::new(0.0, 0.0);
    let p1 = Point2D::new(50.0, 100.0);
    let p2 = Point2D::new(100.0, 0.0);
    let quad = QuadraticBezier::new(p0, p1, p2);

    // Endpoints
    assert_eq!(quad.evaluate(0.0), p0);
    assert_eq!(quad.evaluate(1.0), p2);

    // Midpoint t = 0.5: B(0.5) = 0.25*0 + 0.5*50 + 0.25*100 = 50 for x; y = 0.25*0 + 0.5*100 + 0.25*0 = 50
    let mid = quad.evaluate(0.5);
    assert_eq!(mid, Point2D::new(50.0, 50.0));

    // Split at 0.5
    let (left, right) = quad.split(0.5);
    assert_eq!(left.p0, p0);
    assert_eq!(left.p2, mid);
    assert_eq!(right.p0, mid);
    assert_eq!(right.p2, p2);

    // Bounding box includes vertical peak at (50, 50)
    let bbox = quad.bounding_box();
    assert_eq!(bbox.min_x, 0.0);
    assert_eq!(bbox.max_x, 100.0);
    assert_eq!(bbox.min_y, 0.0);
    assert_eq!(bbox.max_y, 50.0);
}

#[test]
fn test_cubic_to_quadratic_subdivision() {
    let p0 = Point2D::new(0.0, 0.0);
    let p1 = Point2D::new(30.0, 70.0);
    let p2 = Point2D::new(70.0, 70.0);
    let p3 = Point2D::new(100.0, 0.0);
    let cubic = CubicBezier::new(p0, p1, p2, p3);

    let quads = cubic.to_quadratics(0.5);
    assert!(!quads.is_empty(), "Subdivision must produce quadratic segments");

    // The start of first quad must match p0 and end of last must match p3
    assert_eq!(quads.first().unwrap().p0, p0);
    assert_eq!(quads.last().unwrap().p2, p3);

    // Midpoint check
    let cubic_mid = cubic.evaluate(0.5);
    let mid_quad = &quads[quads.len() / 2];
    assert!(mid_quad.bounding_box().contains(cubic_mid) || quads.iter().any(|q| q.evaluate(0.5).distance(cubic_mid) < 2.0));
}

#[test]
fn test_analytical_distance_to_quadratic() {
    // Horizontal line segment as quadratic
    let curve = QuadraticBezier::new(Point2D::new(0.0, 0.0), Point2D::new(50.0, 0.0), Point2D::new(100.0, 0.0));

    // Point (50, 10) is exactly distance 10 away
    let d1 = distance_to_quadratic(Point2D::new(50.0, 10.0), &curve);
    assert!((d1 - 10.0).abs() < 1e-4);

    // Point on curve has distance ~0
    let d2 = distance_to_quadratic(Point2D::new(25.0, 0.0), &curve);
    assert!(d2 < 1e-4);

    // Point beyond end (110, 0) is distance 10 from (100, 0)
    let d3 = distance_to_quadratic(Point2D::new(110.0, 0.0), &curve);
    assert!((d3 - 10.0).abs() < 1e-4);
}

#[test]
fn test_svg_path_parsing_and_winding_test() {
    let svg = "M 0 0 L 100 0 L 100 100 L 0 100 Z";
    let path = PathBuilder::from_svg(svg);
    assert_eq!(path.verbs.len(), 5);

    let quads = path.to_quadratics(0.1);
    assert_eq!(quads.len(), 4);

    // Point (50, 50) is inside box
    assert!(is_point_inside_closed_path(Point2D::new(50.0, 50.0), &quads));
    // Point (150, 50) is outside box
    assert!(!is_point_inside_closed_path(Point2D::new(150.0, 50.0), &quads));
}

#[test]
fn test_gpu_command_buffer_and_font_cache() {
    let mut cmd_buf = PathCommandBuffer::new();
    let path = PathBuilder::from_svg("M 10 10 Q 50 100 90 10 Z").with_fill(FillRule::NonZero);

    cmd_buf.append_path(&path, 0.1, 1.0);
    assert_eq!(cmd_buf.total_draw_calls, 1);
    assert!(cmd_buf.total_curves() >= 1);
    assert_eq!(cmd_buf.byte_size(), cmd_buf.total_curves() * std::mem::size_of::<GpuCurveInstance>());

    // Font cache test
    let mut cache = AnalyticGlyphCache::new();
    cache.register_path_glyph('A', 100.0, &path, 0.1);

    assert!(cache.get('A').is_some());
    let sdf_val = cache.evaluate_glyph_sdf('A', Point2D::new(50.0, 40.0));
    assert!(sdf_val.is_some());
}
