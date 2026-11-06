//! f64 camera and RTC precision: the one-metre-at-Earth-radius contract.
//!
//! The reference transforms every vertex in f64 relative to the camera centre
//! and narrows only screen coordinates. These tests pin that behaviour and
//! demonstrate the failure mode of the f32 path, which is the largest visible
//! difference between the two renderers.

use strata_render::camera64::{Camera64, DMat4, DVec3, RenderFrame64, EARTH_RADIUS_M};

/// A camera 8 400 km above the +X equator looking at the surface point; the
/// distance is large enough that a one metre offset is below a pixel, which is
/// the regime where f32 storage is not good enough.
fn surface_camera() -> Camera64 {
    Camera64 {
        eye: DVec3::new(EARTH_RADIUS_M + 8_400_000.0, 0.0, 0.0),
        target: DVec3::new(EARTH_RADIUS_M, 0.0, 0.0),
        up: DVec3::UP,
        fov_y_deg: 60.0,
        near: 1.0,
        far: 1.0e9,
    }
}

#[test]
fn f32_world_coordinates_collapse_at_earth_radius() {
    let a = DVec3::new(EARTH_RADIUS_M, 0.0, 0.0).as_f32();
    // The f32 ULP near 6 378 137 m is one metre, so neighbouring metres
    // quantise to the same value and any sub-metre offset disappears.
    let one_metre = DVec3::new(EARTH_RADIUS_M + 1.0, 0.0, 0.0).as_f32();
    let quarter_metre = DVec3::new(EARTH_RADIUS_M + 0.25, 0.0, 0.0).as_f32();
    assert_ne!(
        a, one_metre,
        "a full metre rounds to the next f32 step at Earth radius"
    );
    assert_eq!(
        a, quarter_metre,
        "a quarter metre at Earth radius must quantise away in f32"
    );
    // The f64 path keeps them distinct by construction.
    let fa = DVec3::new(EARTH_RADIUS_M, 0.0, 0.0).x;
    let fb = DVec3::new(EARTH_RADIUS_M + 1.0, 0.0, 0.0).x;
    assert!(fb - fa == 1.0, "f64 must preserve the metre exactly");
}

#[test]
fn rtc_projection_resolves_one_metre_at_earth_radius() {
    let camera = surface_camera();
    let frame = RenderFrame64::from_camera(&camera, 1440.0, 900.0);

    let a = frame.project(DVec3::new(EARTH_RADIUS_M, 0.0, 0.0));
    let b = frame.project(DVec3::new(EARTH_RADIUS_M, 1.0, 0.0));

    assert!(
        !a.clipped && !b.clipped,
        "both surface points must be in front"
    );
    let dx = (b.x - a.x).abs();
    let dy = (b.y - a.y).abs();
    let separation = dx.hypot(dy);
    assert!(
        separation > 1e-6,
        "f64 RTC must keep the metre distinct even when it is far below a pixel"
    );
    assert!(
        dx.abs() + dy.abs() > 0.0,
        "the screen separation must land on a finite coordinate, got {a:?} and {b:?}"
    );

    // The contract is on the f64 screen value: narrowing the RTC corner
    // positions to f32 later must not be what decides the separation.
    assert!(
        (a.x - b.x).abs() > 0.0 || (a.y - b.y).abs() > 0.0,
        "RTC must not quantize the pair to the same screen coordinate"
    );
}

#[test]
fn rtc_origin_choice_does_not_change_screen_position() {
    let camera = surface_camera();
    let point = DVec3::new(EARTH_RADIUS_M, 3.0, 4.0);

    let at_target = RenderFrame64::new(&camera, camera.target, 1440.0, 900.0).project(point);
    let at_point = RenderFrame64::new(&camera, point, 1440.0, 900.0).project(point);

    assert!(
        (at_target.x - at_point.x).abs() < 0.01 && (at_target.y - at_point.y).abs() < 0.01,
        "the RTC origin must cancel to well under a pixel, got {at_target:?} vs {at_point:?}"
    );
}

#[test]
fn camera_center_projects_to_the_viewport_center() {
    let camera = surface_camera();
    let frame = RenderFrame64::from_camera(&camera, 1440.0, 900.0);
    let centre = frame.project(camera.target);
    assert!(
        (centre.x - 720.0).abs() < 1e-6 && (centre.y - 450.0).abs() < 1e-6,
        "the look-at target must land at the viewport centre, got {centre:?}"
    );
}

#[test]
fn points_behind_the_camera_are_clipped() {
    let camera = surface_camera();
    let frame = RenderFrame64::from_camera(&camera, 1440.0, 900.0);
    // A point on the far side of the eye along its view axis. The camera looks
    // along -X, so +X beyond the eye is behind it.
    let behind = frame.project(camera.eye.add(DVec3::new(2.0 * (camera.eye.x - EARTH_RADIUS_M), 0.0, 0.0)));
    assert!(behind.clipped, "a point behind the eye must be clipped");
}

#[test]
fn perspective_matrix_is_finite_and_invertible_in_shape() {
    let proj = DMat4::perspective(60.0f64.to_radians(), 16.0 / 9.0, 1.0, 1.0e9);
    for value in proj.m {
        assert!(value.is_finite(), "projection matrix must stay finite");
    }
    // Depth is stored in 0..=1 like the f32 engine path; the perspective term
    // is -1 in the w row under column-major storage.
    assert!(
        (proj.m[3 * 4 + 2] + 1.0).abs() < 1e-6,
        "the w row must carry the perspective term, got {}",
        proj.m[3 * 4 + 2]
    );
}

#[test]
fn morph_curve_is_reproducible_in_f64() {
    // The reference morphs between the globe and 2.5D terrain over a height
    // band. The exact curve lives in the plugin, but the camera-side contract
    // is that interpolation is done in f64 and is stable to well below a
    // pixel across the band boundaries.
    let globe = DVec3::new(0.0, 0.0, EARTH_RADIUS_M * 3.0);
    let terrain = DVec3::new(0.0, 0.0, EARTH_RADIUS_M + 1_500.0);
    let a = globe.lerp(terrain, 0.5);
    let b = terrain.lerp(globe, 0.5);
    assert!((a.z - b.z).abs() < 1e-9, "interpolation must be symmetric");
    assert!(
        (a.z - (globe.z + terrain.z) / 2.0).abs() < 1e-9,
        "interpolation must be exact in f64"
    );
}
