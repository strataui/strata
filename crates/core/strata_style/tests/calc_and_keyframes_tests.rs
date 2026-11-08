//! Integration Tests: Dynamic calc() Math Engine, Cubic-Bezier Easing, and @keyframes Animations.

use strata_primitives::geom::Point;
use strata_style::prelude::*;

#[test]
fn test_calc_expression_ast_and_unit_resolution() {
    let ctx = CalcContext {
        parent_size: 1000.0,
        root_font_size: 16.0,
        current_font_size: 16.0,
        viewport_width: 1920.0,
        viewport_height: 1080.0,
    };

    // 1. Simple addition: 100% + 20px
    let expr1 = parse_calc("100% + 20px").expect("Must parse simple calc");
    // 1000.0 + 20.0 = 1020.0
    assert_eq!(expr1.eval(&ctx), 1020.0);

    // 2. Subtraction with rem: calc(100% - 2rem)
    let expr2 = parse_calc("calc(100% - 2rem)").expect("Must parse nested calc()");
    // 1000.0 - (2 * 16.0) = 1000.0 - 32.0 = 968.0
    assert_eq!(expr2.eval(&ctx), 968.0);

    // 3. Operator precedence: 50% + 10px * 3
    let expr3 = parse_calc("50% + 10px * 3").expect("Must parse precedence");
    // 500.0 + (10.0 * 3.0) = 530.0
    assert_eq!(expr3.eval(&ctx), 530.0);

    // 4. Parentheses: (50% + 10px) * 2
    let expr4 = parse_calc("(50% + 10px) * 2").expect("Must parse parentheses");
    // (500.0 + 10.0) * 2.0 = 1020.0
    assert_eq!(expr4.eval(&ctx), 1020.0);

    // 5. Complex mixed unit calculation: 100% - 24px + 1.5rem
    let expr5 = parse_calc("100% - 24px + 1.5rem").expect("Must parse complex expression");
    // 1000.0 - 24.0 + 24.0 = 1000.0
    assert_eq!(expr5.eval(&ctx), 1000.0);
}

#[test]
fn test_calc_recursion_limit_and_division_by_zero_safety() {
    let ctx = CalcContext::default();

    // 1. Division by non-zero scalar: 100px / 4
    let div_expr = parse_calc("100px / 4").expect("Valid division should parse");
    assert_eq!(div_expr.eval(&ctx), 25.0);

    // 2. Division by zero in syntax returns Err
    let div_zero_err = parse_calc("100px / 0");
    assert!(div_zero_err.is_err());
    let err_msg = div_zero_err.unwrap_err();
    assert!(err_msg.contains("Division by zero"));

    // 3. Direct Div AST node with 0.0 or NaN evaluates safely to 0.0 without panic or NaN
    let zero_ast = CalcExpr::Div(Box::new(CalcExpr::px(100.0)), 0.0);
    assert_eq!(zero_ast.eval(&ctx), 0.0);

    let nan_ast = CalcExpr::Div(Box::new(CalcExpr::px(100.0)), f32::NAN);
    assert_eq!(nan_ast.eval(&ctx), 0.0);

    let inf_ast = CalcExpr::Mul(Box::new(CalcExpr::px(100.0)), f32::INFINITY);
    assert_eq!(inf_ast.eval(&ctx), 0.0);

    // 4. Nested parentheses within safe depth limit (10 levels)
    let nested_ok = format!("{}10px{}", "(".repeat(10), ")".repeat(10));
    let parsed_ok = parse_calc(&nested_ok);
    assert!(parsed_ok.is_ok());
    assert_eq!(parsed_ok.unwrap().eval(&ctx), 10.0);

    // 5. Unbounded recursion attack (40 levels > MAX_CALC_DEPTH 32)
    let nested_attack = format!("{}10px{}", "(".repeat(40), ")".repeat(40));
    let attack_res = parse_calc(&nested_attack);
    assert!(attack_res.is_err());
    assert!(attack_res.unwrap_err().contains("Maximum calc() expression nesting depth"));
}


#[test]
fn test_cubic_bezier_easing_curves() {
    let linear = CubicBezier::linear();
    assert_eq!(linear.solve(0.0), 0.0);
    assert_eq!(linear.solve(0.5), 0.5);
    assert_eq!(linear.solve(1.0), 1.0);

    let ease = CubicBezier::ease();
    let mid_ease = ease.solve(0.5);
    // Standard ease accelerates faster than linear in the first half
    assert!(mid_ease > 0.5);

    let ease_in = CubicBezier::ease_in();
    let mid_in = ease_in.solve(0.5);
    // Ease-in starts slowly (y < x)
    assert!(mid_in < 0.5);

    let ease_out = CubicBezier::ease_out();
    let mid_out = ease_out.solve(0.5);
    // Ease-out finishes slowly (y > x)
    assert!(mid_out > 0.5);
}

#[test]
fn test_keyframes_animation_interpolation() {
    let anim = KeyframeAnimation::new(
        "fade-and-slide",
        2.0, // 2 seconds
        CubicBezier::linear(),
        vec![
            Keyframe::new(0.0, vec![
                KeyframeProperty::Opacity(0.0),
                KeyframeProperty::TranslateX(0.0),
                KeyframeProperty::Scale(0.8),
            ]),
            Keyframe::new(0.5, vec![
                KeyframeProperty::Opacity(1.0),
                KeyframeProperty::TranslateX(100.0),
                KeyframeProperty::Scale(1.0),
            ]),
            Keyframe::new(1.0, vec![
                KeyframeProperty::Opacity(1.0),
                KeyframeProperty::TranslateX(200.0),
                KeyframeProperty::Scale(1.2),
            ]),
        ],
    );

    // Sample at t = 0.0s (start)
    let s0 = anim.sample(0.0);
    assert_eq!(s0[0].value(), 0.0); // opacity
    assert_eq!(s0[1].value(), 0.0); // translateX
    assert_eq!(s0[2].value(), 0.8); // scale

    // Sample at t = 0.5s (25% progress along 2.0s timeline, midpoint between 0.0 and 0.5)
    let s_mid = anim.sample(0.5);
    // Alpha = 0.25 / 0.5 = 0.5
    // Opacity: 0.0 + 0.5 * 1.0 = 0.5
    assert!((s_mid[0].value() - 0.5).abs() < 1e-3);
    // TranslateX: 0.0 + 0.5 * 100.0 = 50.0
    assert!((s_mid[1].value() - 50.0).abs() < 1e-3);
    // Scale: 0.8 + 0.5 * 0.2 = 0.9
    assert!((s_mid[2].value() - 0.9).abs() < 1e-3);

    // Sample at t = 1.0s (50% progress)
    let s1 = anim.sample(1.0);
    assert_eq!(s1[0].value(), 1.0);
    assert_eq!(s1[1].value(), 100.0);
    assert_eq!(s1[2].value(), 1.0);

    // Sample at t = 2.0s (100% completion)
    let s2 = anim.sample(2.0);
    assert_eq!(s2[0].value(), 1.0);
    assert_eq!(s2[1].value(), 200.0);
    assert_eq!(s2[2].value(), 1.2);
}

#[test]
fn test_backdrop_filter_and_clip_path_containment() {
    let blur_filter = BackdropFilter::blur(12.0);
    assert!(!blur_filter.is_empty());
    assert_eq!(blur_filter.operations[0], FilterOp::Blur(12.0));

    // 1. Circular clip path
    let circle_clip = ClipPath::Circle {
        radius: 50.0,
        center_x: 100.0,
        center_y: 100.0,
    };
    assert!(circle_clip.contains_point(100.0, 100.0, 200.0, 200.0)); // Center
    assert!(circle_clip.contains_point(140.0, 100.0, 200.0, 200.0)); // Inside
    assert!(!circle_clip.contains_point(160.0, 100.0, 200.0, 200.0)); // Outside

    // 2. Inset clip path
    let inset_clip = ClipPath::Inset {
        top: 10.0,
        right: 10.0,
        bottom: 10.0,
        left: 10.0,
        border_radius: 0.0,
    };
    assert!(inset_clip.contains_point(50.0, 50.0, 200.0, 200.0)); // Inside
    assert!(!inset_clip.contains_point(5.0, 50.0, 200.0, 200.0)); // In left margin
    assert!(!inset_clip.contains_point(195.0, 50.0, 200.0, 200.0)); // In right margin

    // 3. Triangle polygon clip path
    let triangle_clip = ClipPath::Polygon(vec![
        Point::new(0.0, 0.0),
        Point::new(100.0, 0.0),
        Point::new(50.0, 100.0),
    ]);
    assert!(triangle_clip.contains_point(50.0, 20.0, 100.0, 100.0)); // Inside
    assert!(!triangle_clip.contains_point(10.0, 80.0, 100.0, 100.0)); // Outside bottom left
}

#[test]
fn test_calc_interner_deduplication_and_bounded_memory() {
    let expr1 = parse_calc("100% - 24px").expect("parse calc");
    let expr2 = parse_calc("100% - 24px").expect("parse calc");

    let id1 = intern_calc(expr1);
    let id2 = intern_calc(expr2);

    // Expressions must deduplicate to the same ID
    assert_eq!(id1, id2);

    let retrieved = get_calc(id1).expect("must find interned expression");
    let ctx = CalcContext {
        parent_size: 1000.0,
        ..Default::default()
    };
    assert_eq!(retrieved.eval(&ctx), 976.0);

    // Interning multiple distinct expressions up to the boundary
    let mut last_id = id1;
    for i in 0..10_000 {
        let synth = CalcExpr::Add(
            Box::new(CalcExpr::px(i as f32)),
            Box::new(CalcExpr::percent(50.0)),
        );
        last_id = intern_calc(synth);
    }
    // Hard bound MAX_CALC_ENTRIES prevents unbounded heap growth
    assert!(last_id.0 < 8192);
}

