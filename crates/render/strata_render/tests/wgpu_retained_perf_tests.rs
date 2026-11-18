#![cfg(feature = "wgpu-backend")]

//! Offscreen wgpu 3D renderer correctness & 1,000-frame 1440x900 throughput test.

use std::sync::Arc;
use std::time::Instant;

use strata_render::gpu_wgpu::Wgpu3DRenderer;
use strata_render::scene3d::{
    Background3D, RetainedCameraUniforms, RetainedScene3D, RetainedTerrainFrame,
    RetainedTileInstance, RetainedTileMesh, RetainedTileTexture, RteTileVertex, Scene3D, TileKeyId,
};
use strata_style::Color;

fn sub3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot3(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn norm3(a: [f64; 3]) -> [f64; 3] {
    let len = dot3(a, a).sqrt().max(1e-12);
    [a[0] / len, a[1] / len, a[2] / len]
}

fn mul_mat4_col(a: &[[f32; 4]; 4], b: &[[f32; 4]; 4]) -> [[f32; 4]; 4] {
    let mut out = [[0.0f32; 4]; 4];
    for c in 0..4 {
        for r in 0..4 {
            out[c][r] =
                a[0][r] * b[c][0] + a[1][r] * b[c][1] + a[2][r] * b[c][2] + a[3][r] * b[c][3];
        }
    }
    out
}

fn camera_uniforms(
    eye: [f64; 3],
    aim: [f64; 3],
    width: u32,
    height: u32,
) -> RetainedCameraUniforms {
    let fwd = norm3(sub3(aim, eye));
    let right = norm3(cross3(fwd, [0.0, 1.0, 0.0]));
    let up = norm3(cross3(right, fwd));

    let view_rot = [
        [right[0] as f32, up[0] as f32, -fwd[0] as f32, 0.0],
        [right[1] as f32, up[1] as f32, -fwd[1] as f32, 0.0],
        [right[2] as f32, up[2] as f32, -fwd[2] as f32, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];

    let aspect = (width as f64) / (height.max(1) as f64);
    let f = 1.0 / (45.0_f64.to_radians() * 0.5).tan();
    let near = 1.0_f64;
    let far = 5_000.0_f64;
    let range_inv = 1.0 / (near - far);
    let proj = [
        [(f / aspect) as f32, 0.0, 0.0, 0.0],
        [0.0, f as f32, 0.0, 0.0],
        [0.0, 0.0, (far * range_inv) as f32, -1.0],
        [0.0, 0.0, (near * far * range_inv) as f32, 0.0],
    ];

    RetainedCameraUniforms {
        proj,
        view_rot,
        view_proj_rte: mul_mat4_col(&proj, &view_rot),
        eye_ecef: eye,
        sun_dir: [0.4, 0.5, 0.77],
        sun_color: [1.0, 0.98, 0.94],
        ambient_color: [0.25, 0.28, 0.32],
        background: Background3D::VerticalGradient {
            top: Color::rgba(16, 28, 48, 255),
            bottom: Color::rgba(92, 136, 184, 255),
        },
        near: near as f32,
        far: far as f32,
        ..Default::default()
    }
}

fn build_retained_grid_frame(width: u32, height: u32) -> RetainedTerrainFrame {
    let grid_res = 9u32;
    let tile_span = 40.0f32;
    let mut meshes = Vec::new();
    let mut textures = Vec::new();
    let mut instances = Vec::new();

    for ty in 0..4u32 {
        for tx in 0..4u32 {
            let id = (ty * 4 + tx + 1) as u64;
            let key = TileKeyId::new(2, tx, ty, 0);
            let cx = ((tx as f64) - 1.5) * (tile_span as f64);
            let cy = ((ty as f64) - 1.5) * (tile_span as f64);
            let center_ecef = [cx, cy, 0.0];

            let mut verts = Vec::with_capacity((grid_res * grid_res) as usize);
            for r in 0..grid_res {
                let v = r as f32 / (grid_res - 1) as f32;
                let y = (v - 0.5) * tile_span;
                for c in 0..grid_res {
                    let u = c as f32 / (grid_res - 1) as f32;
                    let x = (u - 0.5) * tile_span;
                    verts.push(RteTileVertex::new([x, y, 0.0], [0.0, 0.0, 1.0], [u, v]));
                }
            }

            meshes.push(Arc::new(RetainedTileMesh {
                key,
                revision: 1,
                center_ecef,
                grid_res,
                has_skirts: false,
                vertices: Arc::new(verts),
                bounding_radius: tile_span,
            }));

            let green = 110 + ((tx * 25 + ty * 15) as u8);
            textures.push(Arc::new(RetainedTileTexture {
                id,
                revision: 1,
                width: 4,
                height: 4,
                rgba8: Arc::new([45, green, 55, 255].repeat(16)),
            }));

            instances.push(RetainedTileInstance {
                key,
                mesh_revision: 1,
                center_ecef,
                texture_id: Some(id),
                texture_revision: 1,
                uv_rect: [0.0, 0.0, 1.0, 1.0],
                tint: [1.0, 1.0, 1.0, 1.0],
                unlit: false,
                ..Default::default()
            });
        }
    }

    RetainedTerrainFrame {
        camera: camera_uniforms([0.0, -35.0, 160.0], [0.0, 0.0, 0.0], width, height),
        geometry_revision: 1,
        meshes,
        instances,
        textures,
        dynamic_meshes: Vec::new(),
        environment: None,
    }
}

#[test]
fn wgpu_3d_renderer_retained_render_and_1000_frame_1440x900_throughput() {
    let mut renderer = match Wgpu3DRenderer::new_headless(wgpu::TextureFormat::Bgra8Unorm) {
        Ok(r) => r,
        Err(reason) => {
            eprintln!("[skip wgpu_3d_renderer test] {}", reason);
            return;
        }
    };

    let w = 1440u32;
    let h = 900u32;
    let frame = build_retained_grid_frame(w, h);
    let retained = RetainedScene3D::new(frame.camera);
    retained.update_terrain(Arc::new(frame));
    let scene = Scene3D::new().with_retained_handle(retained.clone());

    // 1. Verify first frame with readback contains both sky gradient and green terrain pixels.
    let bgra = renderer
        .render_offscreen(&scene, w, h, true, true)
        .expect("offscreen wgpu render with readback");
    assert_eq!(bgra.len(), (w * h * 4) as usize);

    // Center pixel should hit terrain (green channel noticeably brighter than blue/red),
    // while top-left corner should hit sky gradient (blue > green).
    let top_idx = ((10 * w + 10) * 4) as usize;
    let top_b = bgra[top_idx];
    let top_g = bgra[top_idx + 1];
    assert!(
        top_b > top_g,
        "expected sky background at top corner (BGRA={:?})",
        &bgra[top_idx..top_idx + 4]
    );

    let center_idx = (((h / 2) * w + (w / 2)) * 4) as usize;
    let mid_b = bgra[center_idx];
    let mid_g = bgra[center_idx + 1];
    let mid_r = bgra[center_idx + 2];
    assert!(
        mid_g > mid_b && mid_g > mid_r,
        "expected green terrain pixel at viewport center (BGRA={:?})",
        &bgra[center_idx..center_idx + 4]
    );

    // 2. Warm-up + 1,000 continuous camera orbit frames at 1440x900 without CPU readback.
    for _ in 0..20 {
        renderer
            .render_offscreen(&scene, w, h, false, true)
            .expect("warmup frame");
    }

    let n_frames = 1_000usize;
    let mut frame_ms = Vec::with_capacity(n_frames);
    let total_t0 = Instant::now();
    for i in 0..n_frames {
        let angle = (i as f64) * 0.00628;
        let eye = [30.0 * angle.sin(), -35.0 + 20.0 * angle.cos(), 160.0];
        retained.update_camera(camera_uniforms(eye, [0.0, 0.0, 0.0], w, h));
        let ft0 = Instant::now();
        renderer
            .render_offscreen(&scene, w, h, false, true)
            .expect("bench frame");
        frame_ms.push(ft0.elapsed().as_secs_f64() * 1000.0);
    }
    let total_elapsed = total_t0.elapsed();
    let fps = (n_frames as f64) / total_elapsed.as_secs_f64();
    frame_ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p50 = frame_ms[n_frames / 2];
    let p99 = frame_ms[(n_frames * 99) / 100];
    let avg = (total_elapsed.as_secs_f64() * 1000.0) / (n_frames as f64);

    eprintln!(
        "[bench wgpu 1440x900] {} frames in {:.2} ms | {:.1} FPS | avg={:.3} ms | P50={:.3} ms | P99={:.3} ms",
        n_frames,
        total_elapsed.as_secs_f64() * 1000.0,
        fps,
        avg,
        p50,
        p99
    );
}

#[test]
fn wgpu_25d_shadowless_daylight_with_below_horizon_sun() {
    let mut renderer = match Wgpu3DRenderer::new_headless(wgpu::TextureFormat::Bgra8UnormSrgb) {
        Ok(r) => r,
        Err(reason) => {
            eprintln!("[skip] {}", reason);
            return;
        }
    };
    let w = 320u32;
    let h = 200u32;
    let mut frame = build_retained_grid_frame(w, h);
    frame.camera.sun_dir = [0.0, -0.4, -0.9];
    frame.camera.day_night_strength = 0.0;
    frame.camera.city_lights_intensity = 0.0;
    frame.camera.local_25d_blend = 1.0;

    let retained = RetainedScene3D::new(frame.camera);
    retained.update_terrain(Arc::new(frame));
    let scene = Scene3D::new().with_retained_handle(retained);
    let bgra = renderer
        .render_offscreen(&scene, w, h, true, true)
        .expect("offscreen");
    let center_idx = (((h / 2) * w + (w / 2)) * 4) as usize;
    let b = bgra[center_idx];
    let g = bgra[center_idx + 1];
    let r = bgra[center_idx + 2];
    assert!(
        g > 60 && g > b && g > r,
        "expected shadowless bright green 2.5D terrain even with sun below horizon (got BGRA={:?})",
        &bgra[center_idx..center_idx + 4]
    );

    // Check high texture count (>64 distinct textures) with limited budget renders bright cleanly
    let mut hi_frame = build_retained_grid_frame(w, h);
    let mut extra_textures = Vec::new();
    for tid in 10u64..110u64 {
        extra_textures.push(Arc::new(RetainedTileTexture {
            id: tid,
            revision: 1,
            width: 4,
            height: 4,
            rgba8: Arc::new([40, 210, 60, 255].repeat(16)),
        }));
    }
    hi_frame.textures = extra_textures;
    hi_frame.instances[0].texture_id = Some(109);
    hi_frame.instances[0].texture_revision = 1;
    let hi_ret = RetainedScene3D::new(hi_frame.camera);
    hi_ret.update_terrain(Arc::new(hi_frame));
    let hi_scene = Scene3D::new().with_retained_handle(hi_ret.clone());
    for _ in 0..3 {
        let _ = renderer.render_offscreen(&scene, w, h, false, false);
    }
    let hi_bgra = renderer
        .render_offscreen(&hi_scene, w, h, true, false)
        .expect("hi offscreen");
    assert!(
        hi_bgra[center_idx + 1] > 60,
        "expected high texture count tile to upload cleanly (got BGRA={:?})",
        &hi_bgra[center_idx..center_idx + 4]
    );
}

#[test]
fn wgpu_skybox_cubemap_renders_faces_and_fades_in_25d() {
    use strata_render::scene3d::{RetainedCubemapTexture, RetainedEnvironmentAssets};

    let mut renderer = match Wgpu3DRenderer::new_headless(wgpu::TextureFormat::Bgra8Unorm) {
        Ok(r) => r,
        Err(reason) => {
            eprintln!("[skip wgpu_skybox_cubemap test] {}", reason);
            return;
        }
    };
    let (w, h) = (64u32, 64u32);
    let face_colors: [[u8; 4]; 6] = [
        [240, 20, 20, 255],   // +X = red
        [20, 240, 20, 255],   // -X = green
        [20, 20, 240, 255],   // +Y = blue
        [240, 240, 20, 255],  // -Y = yellow
        [240, 20, 240, 255],  // +Z = magenta
        [20, 240, 240, 255],  // -Z = cyan
    ];
    let faces_rgba8: [Arc<Vec<u8>>; 6] = std::array::from_fn(|i| {
        Arc::new(face_colors[i].repeat(16)) // 4x4 face
    });
    let cube = Arc::new(RetainedCubemapTexture {
        id: 77,
        revision: 1,
        face_size: 4,
        faces_rgba8,
    });
    let env = Arc::new(RetainedEnvironmentAssets {
        revision: 1,
        skybox_cubemap: Some(cube),
        ..Default::default()
    });

    let aims: [[f64; 3]; 6] = [
        [1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, -1.0, 0.0],
        [0.0, 0.0, 1.0],
        [0.0, 0.0, -1.0],
    ];
    for (idx, aim) in aims.iter().enumerate() {
        let up_ref = if aim[1].abs() > 0.9 {
            [0.0, 0.0, 1.0]
        } else {
            [0.0, 1.0, 0.0]
        };
        let fwd = norm3(*aim);
        let right = norm3(cross3(fwd, up_ref));
        let up = norm3(cross3(right, fwd));
        let view_rot = [
            [right[0] as f32, up[0] as f32, -fwd[0] as f32, 0.0],
            [right[1] as f32, up[1] as f32, -fwd[1] as f32, 0.0],
            [right[2] as f32, up[2] as f32, -fwd[2] as f32, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        let mut cam = camera_uniforms([0.0, 0.0, 0.0], [0.0, 0.0, -1.0], w, h);
        cam.view_rot = view_rot;
        cam.view_proj_rte = mul_mat4_col(&cam.proj, &view_rot);
        cam.stars_visible = true;
        cam.stars_intensity = 1.0;
        cam.skybox_intensity = 1.0;
        cam.local_25d_blend = 0.0;

        let frame = RetainedTerrainFrame {
            camera: cam,
            geometry_revision: 1,
            meshes: Vec::new(),
            instances: Vec::new(),
            textures: Vec::new(),
            dynamic_meshes: Vec::new(),
            environment: Some(Arc::clone(&env)),
        };
        let retained = RetainedScene3D::new(cam);
        retained.update_terrain(Arc::new(frame));
        let scene = Scene3D::new().with_retained_handle(retained);
        let bgra = renderer.render_offscreen(&scene, w, h, true, true).expect("offscreen");
        let center_idx = (((h / 2) * w + (w / 2)) * 4) as usize;
        let b = bgra[center_idx];
        let g = bgra[center_idx + 1];
        let r = bgra[center_idx + 2];
        let exp = face_colors[idx];
        // Verify dominant channels match the uploaded cubemap face color.
        assert_eq!(r > 150, exp[0] > 150, "face {idx} R mismatch: got RGB=({r},{g},{b}) exp={exp:?}");
        assert_eq!(g > 150, exp[1] > 150, "face {idx} G mismatch: got RGB=({r},{g},{b}) exp={exp:?}");
        assert_eq!(b > 150, exp[2] > 150, "face {idx} B mismatch: got RGB=({r},{g},{b}) exp={exp:?}");
    }
}

