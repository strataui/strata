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

#[test]
fn test_gpu_compute_memory_layout_and_alignment() {
    use strata_gpu_path::{
        GpuPathSegment, GpuPathUniforms, RawCurveInput,
        FLAG_CLOSED, FLAG_EVEN_ODD, FLAG_FILL, FLAG_STROKE,
    };

    assert_eq!(std::mem::size_of::<RawCurveInput>(), 64);
    assert_eq!(std::mem::size_of::<GpuPathSegment>(), 64);
    assert_eq!(std::mem::size_of::<GpuPathUniforms>(), 16);

    // Flags are distinct bitmasks
    assert_eq!(FLAG_FILL, 1);
    assert_eq!(FLAG_STROKE, 2);
    assert_eq!(FLAG_EVEN_ODD, 4);
    assert_eq!(FLAG_CLOSED, 8);

    let raw = RawCurveInput {
        p0: [10.0, 20.0],
        p1: [30.0, 40.0],
        p2: [50.0, 60.0],
        p3: [70.0, 80.0],
        stroke_width: 2.5,
        flags: FLAG_STROKE | FLAG_CLOSED,
        z_index: 3.0,
        _pad: 0.0,
        color: [0.1, 0.2, 0.3, 1.0],
    };

    let bytes = raw.as_bytes();
    assert_eq!(bytes.len(), 64);

    let seg = GpuPathSegment {
        p0: [10.0, 20.0],
        p1: [40.0, 50.0],
        p2: [70.0, 80.0],
        stroke_width: 2.5,
        flags: FLAG_STROKE,
        color: [0.1, 0.2, 0.3, 1.0],
        z_index: 3.0,
        _pad0: 0.0,
        _pad1: 0.0,
        _pad2: 0.0,
    };
    assert_eq!(seg.as_bytes().len(), 64);
}

#[test]
fn test_curve_compute_dispatcher() {
    use strata_gpu_path::{
        CubicBezier, CurveComputeDispatcher, Point2D, QuadraticBezier,
        FLAG_FILL, FLAG_STROKE,
    };

    let mut dispatcher = CurveComputeDispatcher::new(0.5);

    // Add a cubic S-curve
    let cubic = CubicBezier::new(
        Point2D::new(0.0, 0.0),
        Point2D::new(50.0, 100.0),
        Point2D::new(50.0, -100.0),
        Point2D::new(100.0, 0.0),
    );
    dispatcher.push_cubic(&cubic, 0.0, FLAG_FILL, [0.0, 1.0, 0.0, 1.0], 1.0);

    // Add a quadratic curve
    let quad = QuadraticBezier::new(
        Point2D::new(100.0, 0.0),
        Point2D::new(150.0, 50.0),
        Point2D::new(200.0, 0.0),
    );
    dispatcher.push_quadratic(&quad, 2.0, FLAG_STROKE, [1.0, 0.0, 0.0, 1.0], 1.0);

    // Add a straight line
    dispatcher.push_line(
        Point2D::new(200.0, 0.0),
        Point2D::new(300.0, 0.0),
        1.5,
        FLAG_STROKE,
        [0.0, 0.0, 1.0, 1.0],
        1.0,
    );

    assert_eq!(dispatcher.len(), 3);
    assert_eq!(dispatcher.input_bytes().len(), 3 * 64);

    let segments = dispatcher.dispatch_compute();
    assert!(segments.len() >= 3, "Compute dispatch should produce flattened segments");

    // Start point of first segment must match start of cubic
    assert_eq!(segments[0].p0, [0.0, 0.0]);
    // Color of first segment must match
    assert_eq!(segments[0].color, [0.0, 1.0, 0.0, 1.0]);
}

#[test]
fn test_analytical_stroke_expansion() {
    use strata_gpu_path::{
        expand_stroke_quadratics, LineCap, LineJoin, Point2D, QuadraticBezier, StrokeOptions,
    };

    let curve = QuadraticBezier::new(
        Point2D::new(0.0, 0.0),
        Point2D::new(50.0, 0.0),
        Point2D::new(100.0, 0.0),
    );

    let options = StrokeOptions::new(10.0)
        .with_cap(LineCap::Round)
        .with_join(LineJoin::Round);

    let expanded = expand_stroke_quadratics(&[curve], &options);
    assert!(!expanded.is_empty(), "Expanded stroke must yield outline curves");

    // Should contain outer curve, end cap, inner curve, start cap
    assert!(expanded.len() >= 4);
}

#[test]
fn test_wgsl_shader_constants_validity() {
    use strata_gpu_path::{WGSL_ANALYTICAL_PATH_RASTER_SHADER, WGSL_PATH_COMPUTE_FLATTEN_SHADER};

    assert!(WGSL_PATH_COMPUTE_FLATTEN_SHADER.contains("@compute @workgroup_size(64)"));
    assert!(WGSL_PATH_COMPUTE_FLATTEN_SHADER.contains("cs_flatten_curves"));
    assert!(WGSL_ANALYTICAL_PATH_RASTER_SHADER.contains("@vertex"));
    assert!(WGSL_ANALYTICAL_PATH_RASTER_SHADER.contains("@fragment"));
    assert!(WGSL_ANALYTICAL_PATH_RASTER_SHADER.contains("sd_quadratic_bezier"));
}

#[test]
fn test_slug_font_shader_and_glyph_buffer() {
    use strata_gpu_path::{
        AnalyticGlyph, Point2D, QuadraticBezier, Rect2D, SlugGlyphBuffer, WGSL_SLUG_FONT_SHADER,
    };

    assert!(WGSL_SLUG_FONT_SHADER.contains("solve_quadratic_roots"));
    assert!(WGSL_SLUG_FONT_SHADER.contains("GpuGlyphCurve"));
    assert!(WGSL_SLUG_FONT_SHADER.contains("@vertex"));
    assert!(WGSL_SLUG_FONT_SHADER.contains("@fragment"));

    let glyph = AnalyticGlyph {
        char_code: 'A',
        advance_width: 24.0,
        bounds: Rect2D::new(0.0, 0.0, 24.0, 32.0),
        curves: vec![
            QuadraticBezier::new(Point2D::new(0.0, 0.0), Point2D::new(12.0, 32.0), Point2D::new(24.0, 0.0)),
            QuadraticBezier::new(Point2D::new(4.0, 10.0), Point2D::new(12.0, 10.0), Point2D::new(20.0, 10.0)),
        ],
    };

    let mut buffer = SlugGlyphBuffer::new();
    buffer.push_glyph(
        &glyph,
        Rect2D::new(100.0, 100.0, 124.0, 132.0),
        [1.0, 1.0, 1.0, 1.0],
        Rect2D::new(0.0, 0.0, 1920.0, 1080.0),
    );

    assert_eq!(buffer.instances.len(), 1);
    assert_eq!(buffer.curves.len(), 2);
    assert_eq!(buffer.instances[0].curve_range, [0, 2]);
    assert!(!buffer.instances_as_bytes().is_empty());
    assert!(!buffer.curves_as_bytes().is_empty());
}

#[test]
fn test_geometry_types_bidirectional_conversions() {
    use strata_gpu_path::{Point2D, Rect2D};
    use strata_primitives::geom::{Point, Rect};

    let p_prim = Point::new(15.0f32, 25.0f32);
    let p_gpu: Point2D = p_prim.into();
    assert_eq!(p_gpu.x, 15.0);
    assert_eq!(p_gpu.y, 25.0);

    let p_back: Point<f32> = p_gpu.into();
    assert_eq!(p_back, p_prim);

    let r_prim = Rect::from_xywh(10.0, 20.0, 100.0, 200.0);
    let r_gpu: Rect2D = r_prim.into();
    assert_eq!(r_gpu.min_x, 10.0);
    assert_eq!(r_gpu.min_y, 20.0);
    assert_eq!(r_gpu.max_x, 110.0);
    assert_eq!(r_gpu.max_y, 220.0);
    assert_eq!(r_gpu.width(), 100.0);
    assert_eq!(r_gpu.height(), 200.0);

    let r_back: Rect<f32> = r_gpu.into();
    assert_eq!(r_back.x(), 10.0);
    assert_eq!(r_back.y(), 20.0);
    assert_eq!(r_back.width(), 100.0);
    assert_eq!(r_back.height(), 200.0);
}

