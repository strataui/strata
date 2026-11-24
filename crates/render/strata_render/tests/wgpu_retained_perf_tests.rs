#![cfg(feature = "wgpu-backend")]

//! Offscreen wgpu 3D renderer correctness & 1,000-frame 1440x900 throughput test.

use std::sync::Arc;
use std::time::Instant;

use strata_render::gpu_wgpu::Wgpu3DRenderer;
use strata_render::scene3d::{
    Background3D, RetainedCameraUniforms, RetainedGeoInstance, RetainedMeshAsset, RetainedScene3D,
    RetainedTerrainFrame, RetainedTileInstance, RetainedTileMesh, RetainedTileTexture,
    RteTileVertex, Scene3D, TileKeyId,
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
        ..Default::default()
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
            environment: Some(Arc::clone(&env)),
            ..Default::default()
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

#[test]
fn wgpu_lod_zoom_burst_renders_all_100_new_chunks_without_holes() {
    let Ok(mut renderer) = Wgpu3DRenderer::new_headless(wgpu::TextureFormat::Bgra8Unorm) else {
        return;
    };
    let (w, h) = (256u32, 256u32);

    // Advance past initial unlimited warm-up frames (frame_index <= 2).
    let warmup = build_retained_grid_frame(w, h);
    let retained = RetainedScene3D::new(warmup.camera);
    retained.update_terrain(Arc::new(warmup));
    let scene = Scene3D::new().with_retained_handle(retained.clone());
    for _ in 0..4 {
        let _ = renderer
            .render_offscreen(&scene, w, h, true, false)
            .expect("warmup");
    }

    // Build a sudden 10x10 = 100-tile LOD zoom burst over a pure magenta clear background.
    // Under the old 16-mesh/frame cap, 84 of the 100 chunks were skipped (`continue`), leaving magenta holes.
    let mut cam = camera_uniforms([0.0, 0.0, 120.0], [0.0, 0.0, 0.0], w, h);
    cam.background = Background3D::Solid(Color::rgba(255, 0, 255, 255));
    cam.local_25d_blend = 1.0;

    let grid_res = 5u32;
    let tile_span = 16.0f32;
    let mut meshes = Vec::with_capacity(100);
    let mut textures = Vec::with_capacity(100);
    let mut instances = Vec::with_capacity(100);

    for ty in 0..10u32 {
        for tx in 0..10u32 {
            let id = 10_000 + (ty * 10 + tx) as u64;
            let key = TileKeyId::new(6, tx, ty, 0);
            let cx = ((tx as f64) - 4.5) * (tile_span as f64);
            let cy = ((ty as f64) - 4.5) * (tile_span as f64);
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
                center_ecef: [cx, cy, 0.0],
                grid_res,
                has_skirts: false,
                vertices: Arc::new(verts),
                bounding_radius: tile_span,
            }));
            textures.push(Arc::new(RetainedTileTexture {
                id,
                revision: 1,
                width: 4,
                height: 4,
                rgba8: Arc::new([40, 190, 90, 255].repeat(16)),
            }));
            instances.push(RetainedTileInstance {
                key,
                mesh_revision: 1,
                center_ecef: [cx, cy, 0.0],
                texture_id: Some(id),
                texture_revision: 1,
                uv_rect: [0.0, 0.0, 1.0, 1.0],
                tint: [1.0, 1.0, 1.0, 1.0],
                unlit: true,
                night_texture_id: None,
                night_texture_revision: 0,
                night_uv_rect: [0.0, 0.0, 1.0, 1.0],
            });
        }
    }

    retained.update_terrain(Arc::new(RetainedTerrainFrame {
        camera: cam,
        geometry_revision: 99,
        meshes,
        instances,
        textures,
        ..Default::default()
    }));

    // Render single flight frame (`unlimited_upload_budget = false`).
    let bgra = renderer
        .render_offscreen(&scene, w, h, true, false)
        .expect("zoom burst frame 1");

    // Every pixel in the central covered region must have both its mesh AND its green texture ([40, 190, 90])
    // uploaded and rendered on frame 1 (zero magenta holes and zero dark-slate [0.14, 0.19, 0.27] untextured tiles).
    let mut bad_pixels = 0usize;
    for py in 32..(h - 32) {
        for px in 32..(w - 32) {
            let idx = ((py * w + px) * 4) as usize;
            let b = bgra[idx];
            let g = bgra[idx + 1];
            let r = bgra[idx + 2];
            if g < 120 || r > 120 || b > 150 {
                bad_pixels += 1;
            }
        }
    }
    assert_eq!(
        bad_pixels, 0,
        "LOD zoom burst left missing or untextured chunks on frame 1 ({bad_pixels} non-green pixels)"
    );
}

#[test]
fn wgpu_instanced_geo_objects_render_at_planetary_rte_coords_with_nonuniform_scale() {
    let Ok(mut renderer) = Wgpu3DRenderer::new_headless(wgpu::TextureFormat::Bgra8Unorm) else {
        return;
    };
    let (w, h) = (128u32, 128u32);

    // Anchor at full WGS84 planetary ECEF magnitude (6,378,137 m) to verify 48-bit split RTE jitter-free rendering.
    let anchor = [6_378_137.0_f64, 125_000.0_f64, 420_000.0_f64];
    let eye = [anchor[0] + 60.0, anchor[1], anchor[2]];
    let mut cam = camera_uniforms(eye, anchor, w, h);
    cam.background = Background3D::Solid(Color::rgba(8, 12, 20, 255));
    cam.sun_dir = [1.0, 0.0, 0.0];
    cam.day_night_strength = 1.0;

    // Unit quad in local YZ plane facing +X (toward camera), uploaded ONCE as a shared RetainedMeshAsset.
    let quad_asset = Arc::new(RetainedMeshAsset {
        id: 7,
        revision: 1,
        vertices: Arc::new(vec![
            RteTileVertex::new([0.0, -1.0, -1.0], [1.0, 0.0, 0.0], [0.0, 0.0]),
            RteTileVertex::new([0.0, 1.0, -1.0], [1.0, 0.0, 0.0], [1.0, 0.0]),
            RteTileVertex::new([0.0, 1.0, 1.0], [1.0, 0.0, 0.0], [1.0, 1.0]),
            RteTileVertex::new([0.0, -1.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0]),
        ]),
        indices: Arc::new(vec![0, 1, 2, 0, 2, 3]),
        vertex_colors: None,
        albedo_texture: None,
    });

    // Two hardware instances sharing mesh_id = 7 (separated along Z, which is camera horizontal axis) with non-uniform 3x3 scale.
    let instances = Arc::new(vec![
        RetainedGeoInstance {
            mesh_id: 7,
            anchor_ecef: [anchor[0], anchor[1], anchor[2] - 14.0],
            model_cols: [[1.0, 0.0, 0.0], [0.0, 12.0, 0.0], [0.0, 0.0, 8.0]],
            base_color: [0.1, 0.85, 0.95, 1.0],
            roughness: 0.35,
            metalness: 0.1,
            unlit: false,
            ..Default::default()
        },
        RetainedGeoInstance {
            mesh_id: 7,
            anchor_ecef: [anchor[0], anchor[1], anchor[2] + 14.0],
            model_cols: [[1.0, 0.0, 0.0], [0.0, 12.0, 0.0], [0.0, 0.0, 8.0]],
            base_color: [0.95, 0.45, 0.1, 1.0],
            roughness: 0.35,
            metalness: 0.1,
            unlit: false,
            ..Default::default()
        },
    ]);

    let retained = RetainedScene3D::new(cam);
    retained.update_geo_objects(Arc::new(vec![quad_asset]), instances);
    let scene = Scene3D::new().with_retained_handle(retained);

    let bgra = renderer
        .render_offscreen(&scene, w, h, true, true)
        .expect("instanced geo render");

    let mut cyan_hits = 0usize;
    let mut orange_hits = 0usize;
    for px in 0..w {
        let idx = (((h / 2) * w + px) * 4) as usize;
        let b = bgra[idx];
        let g = bgra[idx + 1];
        let r = bgra[idx + 2];
        if b > 140 && g > 120 && r < 80 {
            cyan_hits += 1;
        }
        if r > 140 && g > 60 && b < 80 {
            orange_hits += 1;
        }
    }
    assert!(
        cyan_hits >= 5 && orange_hits >= 5,
        "both hardware-instanced RTE geo objects must render cleanly (cyan_hits={cyan_hits}, orange_hits={orange_hits})"
    );
}

#[test]
fn wgpu_particle_volumes_render_procedural_rain_snow_custom_texture_and_clip_above_ceiling() {
    use strata_render::scene3d::{
        RetainedParticleBillboard, RetainedParticleLayer, RetainedParticleSprite,
    };

    let Ok(mut renderer) = Wgpu3DRenderer::new_headless(wgpu::TextureFormat::Bgra8Unorm) else {
        return;
    };
    let (w, h) = (160u32, 160u32);
    let mut cam = camera_uniforms([0.0, 0.0, 0.0], [0.0, 0.0, -10.0], w, h);
    cam.background = Background3D::Solid(Color::rgba(0, 0, 0, 255));
    cam.day_night_strength = 0.0;

    let count_lit_pixels = |bgra: &[u8]| -> usize {
        bgra.chunks_exact(4)
            .filter(|px| px[0] > 18 || px[1] > 18 || px[2] > 18)
            .count()
    };

    // 1. Procedural rain streak volume below cloud ceiling
    let rain_layer = RetainedParticleLayer {
        id: 1,
        particle_count: 1_200,
        sprite: RetainedParticleSprite::ProceduralStreak,
        billboard: RetainedParticleBillboard::VelocityAligned,
        basis_east: [1.0, 0.0, 0.0],
        basis_north: [0.0, 0.0, -1.0],
        basis_up: [0.0, 1.0, 0.0],
        forward_bias_world: [0.0, 0.0, -6.0],
        wrapped_phase: [0.15, 0.25, 0.35],
        altitude_bounds: [200.0, 0.0, 5_500.0, 900.0],
        volume_params: [32.0, 24.0, 0.15, 0.8],
        motion_params: [26.0, 2.0, 0.5, 1.2],
        size_params: [0.06, 1.2, 0.035, 0.0],
        flutter_clamp: [0.0, 0.0, 1.6, 48.0],
        color: [0.75, 0.88, 1.0, 0.85],
        seed: 7,
    };
    let retained = RetainedScene3D::new(cam);
    retained.update_camera_and_particles(cam, Arc::new(vec![rain_layer.clone()]), Arc::default());
    let scene = Scene3D::new().with_retained_handle(retained.clone());
    let bgra_rain = renderer
        .render_offscreen(&scene, w, h, true, true)
        .expect("rain offscreen");
    let rain_lit = count_lit_pixels(&bgra_rain);
    assert!(
        rain_lit >= 60,
        "procedural 2D rain streaks must render visible pixels, got {rain_lit}"
    );

    // 2. Mathematical 6-fold 2D snowflake crystal volume
    let snow_layer = RetainedParticleLayer {
        id: 2,
        particle_count: 900,
        sprite: RetainedParticleSprite::ProceduralSnowflake,
        billboard: RetainedParticleBillboard::CameraFacing,
        size_params: [0.22, 0.22, 0.0, 1.2],
        flutter_clamp: [0.6, 1.2, 3.5, 16.0],
        color: [0.98, 0.99, 1.0, 0.95],
        seed: 19,
        ..rain_layer.clone()
    };
    retained.update_particles(Arc::new(vec![snow_layer.clone()]), Arc::default());
    let bgra_snow = renderer
        .render_offscreen(&scene, w, h, true, true)
        .expect("snow offscreen");
    let snow_lit = count_lit_pixels(&bgra_snow);
    assert!(
        snow_lit >= 100,
        "mathematical 2D snowflakes must render visible pixels, got {snow_lit}"
    );

    // 2b. ProceduralCurtain regional virga veil
    let curtain_layer = RetainedParticleLayer {
        id: 22,
        particle_count: 400,
        sprite: RetainedParticleSprite::ProceduralCurtain,
        billboard: RetainedParticleBillboard::VelocityAligned,
        size_params: [4.5, 12.0, 0.05, 0.0],
        flutter_clamp: [0.0, 0.0, 6.0, 120.0],
        color: [0.76, 0.84, 0.94, 0.40],
        seed: 43,
        ..rain_layer.clone()
    };
    retained.update_particles(Arc::new(vec![curtain_layer]), Arc::default());
    let bgra_curtain = renderer
        .render_offscreen(&scene, w, h, true, true)
        .expect("curtain offscreen");
    let curtain_lit = count_lit_pixels(&bgra_curtain);
    assert!(
        curtain_lit >= 50,
        "procedural regional rain curtain must render visible veil pixels, got {curtain_lit}"
    );

    // 3. Custom 2D RGBA8 sprite texture (bright magenta square)
    let custom_tex = Arc::new(RetainedTileTexture {
        id: 777,
        revision: 1,
        width: 4,
        height: 4,
        rgba8: Arc::new([255u8, 40, 220, 255].repeat(16)),
    });
    let tex_layer = RetainedParticleLayer {
        id: 3,
        sprite: RetainedParticleSprite::Textured(777),
        color: [1.0, 1.0, 1.0, 0.95],
        seed: 31,
        ..snow_layer.clone()
    };
    retained.update_particles(Arc::new(vec![tex_layer]), Arc::new(vec![custom_tex]));
    let bgra_tex = renderer
        .render_offscreen(&scene, w, h, true, true)
        .expect("custom texture particle offscreen");
    let magenta_hits = bgra_tex
        .chunks_exact(4)
        .filter(|px| px[2] > 120 && px[0] > 100 && px[1] < 80)
        .count();
    assert!(
        magenta_hits >= 50,
        "custom 2D RGBA8 textured particle layer must render magenta sprites, got {magenta_hits}"
    );

    // 4. Camera well above cloud ceiling clips all particles to zero
    let mut above_clouds = snow_layer;
    above_clouds.altitude_bounds = [8_000.0, 0.0, 5_500.0, 900.0];
    retained.update_particles(Arc::new(vec![above_clouds]), Arc::default());
    let bgra_above = renderer
        .render_offscreen(&scene, w, h, true, true)
        .expect("above cloud ceiling offscreen");
    let above_lit = count_lit_pixels(&bgra_above);
    assert_eq!(
        above_lit, 0,
        "particles above cloud ceiling must clip completely to zero, got {above_lit}"
    );
}

#[test]
fn wgpu_physical_25d_sky_night_stars_and_weather_darkening() {
    use strata_render::scene3d::{RetainedEnvironmentAssets, RetainedStar};

    let Ok(mut renderer) = Wgpu3DRenderer::new_headless(wgpu::TextureFormat::Bgra8Unorm) else {
        return;
    };
    let (w, h) = (160u32, 120u32);
    // Look horizontally/upward across the horizon in Y-up test coordinates (local_25d_blend = 1.0).
    let mut cam = camera_uniforms([0.0, 20.0, 0.0], [0.0, 26.0, -30.0], w, h);
    cam.local_25d_blend = 1.0;
    cam.realistic_25d_sky = 1.0;
    cam.night_stars_25d = 1.0;
    cam.sun_visible = true;
    cam.stars_visible = true;
    cam.stars_intensity = 1.5;
    cam.background = Background3D::VerticalGradient {
        top: Color::rgba(40, 100, 200, 255),
        bottom: Color::rgba(160, 210, 245, 255),
    };

    // Place bright catalog stars directly ahead above the horizon (-Z, +Y).
    let mut stars = Vec::new();
    for i in 0..40 {
        let fx = -0.35 + (i % 8) as f32 * 0.10;
        let fy = 0.20 + (i / 8) as f32 * 0.10;
        let fz = -1.0f32;
        let len = (fx * fx + fy * fy + fz * fz).sqrt();
        stars.push(RetainedStar {
            icrf_dir: [fx / len, fy / len, fz / len],
            vmag: -0.5,
            color: [1.0, 0.98, 0.92, 1.0],
        });
    }
    let env = Arc::new(RetainedEnvironmentAssets {
        revision: 1,
        stars: Some(Arc::new(stars)),
        ..Default::default()
    });

    let mean_rgb = |bgra: &[u8]| -> [f32; 3] {
        let mut r = 0.0f64;
        let mut g = 0.0f64;
        let mut b = 0.0f64;
        let n = (bgra.len() / 4).max(1) as f64;
        for px in bgra.chunks_exact(4) {
            b += px[0] as f64;
            g += px[1] as f64;
            r += px[2] as f64;
        }
        [(r / n) as f32, (g / n) as f32, (b / n) as f32]
    };

    // 1. Noon clear sky (sun high at +Y): bright azure blue (B > R)
    cam.sun_dir = [0.0, 0.92, -0.39];
    let frame_noon = Arc::new(RetainedTerrainFrame {
        camera: cam,
        geometry_revision: 1,
        environment: Some(Arc::clone(&env)),
        ..Default::default()
    });
    let retained = RetainedScene3D::new(cam);
    retained.update_terrain(frame_noon);
    let scene = Scene3D::new().with_retained_handle(retained.clone());
    let noon_px = renderer.render_offscreen(&scene, w, h, true, true).unwrap();
    let noon_rgb = mean_rgb(&noon_px);
    assert!(
        noon_rgb[2] > noon_rgb[0] + 20.0 && noon_rgb[2] > 90.0,
        "noon 2.5D sky must be crisp azure blue, got rgb={noon_rgb:?}"
    );

    // 2. Golden sunset (sun right above horizon ahead -Z): warm orange/red shift (R > B)
    cam.sun_dir = [0.0, 0.06, -0.998];
    retained.update_camera(cam);
    let sunset_px = renderer.render_offscreen(&scene, w, h, true, true).unwrap();
    let sunset_rgb = mean_rgb(&sunset_px);
    assert!(
        sunset_rgb[0] > sunset_rgb[2],
        "golden sunset 2.5D sky looking toward sun must glow warm amber (R > B), got rgb={sunset_rgb:?}"
    );

    // 3. Storm darkening dims daytime sky dramatically toward moody overcast slate
    cam.sun_dir = [0.0, 0.85, -0.52];
    cam.weather_darkness = 0.9;
    cam.weather_cloud_cover = 0.9;
    retained.update_camera(cam);
    let storm_px = renderer.render_offscreen(&scene, w, h, true, true).unwrap();
    let storm_rgb = mean_rgb(&storm_px);
    assert!(
        storm_rgb[2] < noon_rgb[2] * 0.72,
        "overcast storm darkening must darken 2.5D sky significantly (noon={noon_rgb:?}, storm={storm_rgb:?})"
    );

    // 4. Clear starry night (sun far below horizon): dark sky + bright visible stars!
    cam.sun_dir = [0.0, -0.65, -0.76];
    cam.weather_darkness = 0.0;
    cam.weather_cloud_cover = 0.0;
    retained.update_camera(cam);
    let night_px = renderer.render_offscreen(&scene, w, h, true, true).unwrap();
    let night_rgb = mean_rgb(&night_px);
    let bright_star_pixels = night_px
        .chunks_exact(4)
        .filter(|px| px[2] > 110 && px[1] > 100 && px[0] > 90)
        .count();
    assert!(
        night_rgb[2] < 48.0 && night_rgb[2] < noon_rgb[2] * 0.25 && bright_star_pixels >= 12,
        "clear 2.5D night sky must be dark and render BSC5 stars above horizon (noon={noon_rgb:?}, rgb={night_rgb:?}, star_px={bright_star_pixels})"
    );
}

#[test]
fn wgpu_directional_mountain_shadows_and_gpu_ocean_coastal_surf() {
    use strata_render::scene3d::{RetainedOceanSurface, RetainedOceanWaveTrain};

    let Ok(mut renderer) = Wgpu3DRenderer::new_headless(wgpu::TextureFormat::Bgra8Unorm) else {
        return;
    };
    let (w, h) = (160u32, 160u32);
    // Camera elevated looking down at a 16x16 terrain patch with a tall mountain wall on +X casting a shadow onto the -X valley.
    let mut cam = camera_uniforms([0.0, 85.0, 95.0], [0.0, 0.0, 0.0], w, h);
    cam.local_25d_blend = 1.0;
    cam.realistic_25d_sky = 1.0;
    cam.terrain_25d_lighting = 1.0;
    cam.terrain_25d_shadows = 0.9;
    cam.shadow_map_size = 512;
    // Low sun from +X casts a crisp shadow from the +X ridge onto the valley floor at X <= 0.
    cam.sun_dir = [0.92, 0.39, 0.0];
    cam.background = Background3D::Solid(Color::rgba(10, 15, 24, 255));

    let grid = 17u32;
    let mut verts = Vec::with_capacity((grid * grid) as usize);
    for r in 0..grid {
        let vz = 40.0 - (r as f32 / (grid - 1) as f32) * 80.0;
        let v = r as f32 / (grid - 1) as f32;
        for c in 0..grid {
            let vx = -40.0 + (c as f32 / (grid - 1) as f32) * 80.0;
            let u = c as f32 / (grid - 1) as f32;
            // Tall ridge on X in [12..28], flat valley floor on X < 10.
            let ridge_t = (1.0 - ((vx - 20.0).abs() / 10.0)).clamp(0.0, 1.0);
            let vy = ridge_t * 34.0;
            verts.push(RteTileVertex::new([vx, vy, vz], [0.0, 1.0, 0.0], [u, v]));
        }
    }
    let mesh = Arc::new(RetainedTileMesh {
        key: TileKeyId::quadtree(12, 0, 0),
        revision: 1,
        center_ecef: [0.0, 0.0, 0.0],
        grid_res: grid,
        has_skirts: false,
        vertices: Arc::new(verts),
        bounding_radius: 75.0,
    });
    let tex = Arc::new(RetainedTileTexture {
        id: 100,
        revision: 1,
        width: 4,
        height: 4,
        rgba8: Arc::new([190u8, 190, 190, 255].repeat(16)),
    });
    let inst = RetainedTileInstance {
        key: TileKeyId::quadtree(12, 0, 0),
        mesh_revision: 1,
        center_ecef: [0.0, 0.0, 0.0],
        texture_id: Some(100),
        texture_revision: 1,
        uv_rect: [0.0, 0.0, 1.0, 1.0],
        tint: [1.0, 1.0, 1.0, 1.0],
        unlit: false,
        ..Default::default()
    };

    let sample_valley_luma = |bgra: &[u8]| -> f32 {
        // Center-left valley pixels directly behind the +X ridge.
        let mut sum = 0.0f64;
        let mut cnt = 0.0f64;
        for py in 72..96u32 {
            for px in 52..76u32 {
                let idx = ((py * w + px) * 4) as usize;
                sum += (bgra[idx] as f64 + bgra[idx + 1] as f64 + bgra[idx + 2] as f64) / 3.0;
                cnt += 1.0;
            }
        }
        (sum / cnt.max(1.0)) as f32
    };

    // 1. Render WITH directional mountain shadows enabled
    let frame_sh = Arc::new(RetainedTerrainFrame {
        camera: cam,
        geometry_revision: 1,
        meshes: vec![Arc::clone(&mesh)],
        instances: vec![inst.clone()],
        textures: vec![Arc::clone(&tex)],
        ..Default::default()
    });
    let retained = RetainedScene3D::new(cam);
    retained.update_terrain(frame_sh);
    let scene = Scene3D::new().with_retained_handle(retained.clone());
    let px_shadowed = renderer.render_offscreen(&scene, w, h, true, true).unwrap();
    let valley_shadowed = sample_valley_luma(&px_shadowed);

    // 2. Render WITHOUT shadows (terrain_25d_shadows = 0.0) -> valley must be noticeably brighter!
    cam.terrain_25d_shadows = 0.0;
    retained.update_camera(cam);
    let px_unshadowed = renderer.render_offscreen(&scene, w, h, true, true).unwrap();
    let valley_unshadowed = sample_valley_luma(&px_unshadowed);
    assert!(
        valley_shadowed + 10.0 < valley_unshadowed,
        "mountain ridge must cast a real shadow darkening the valley (shadowed={valley_shadowed:.1}, unshadowed={valley_unshadowed:.1})"
    );

    // 3. Enable GPU 8-octave ocean waves + 16x16 coastal SDF surf texture and verify clean rendering
    let sdf_tex = Arc::new(RetainedTileTexture {
        id: 505,
        revision: 1,
        width: 16,
        height: 16,
        rgba8: Arc::new([255u8, 60, 255, 128].repeat(256)), // pure water close to shore, shore_dir = +East
    });
    let ocean = Arc::new(RetainedOceanSurface {
        enabled: true,
        origin_ecef: [0.0, 0.0, 0.0],
        basis_east: [1.0, 0.0, 0.0],
        basis_north: [0.0, 0.0, -1.0],
        basis_up: [0.0, 1.0, 0.0],
        train_a: RetainedOceanWaveTrain {
            dir_en: [0.85, 0.52],
            wind_speed_mps: 12.0,
            phase_time_s: 3.4,
            weight: 0.7,
        },
        train_b: RetainedOceanWaveTrain {
            dir_en: [-0.3, 0.95],
            wind_speed_mps: 16.0,
            phase_time_s: 1.2,
            weight: 0.3,
        },
        wave_strength: 1.2,
        wave_scale: 1.0,
        glint_intensity: 1.2,
        opacity: 0.9,
        alt_fade: 1.0,
        coastal_shoaling: true,
        shoaling_range_m: 120.0,
        sdf_half_span_m: 60.0,
        sdf_center_en_m: [0.0, 0.0],
        coastal_sdf: Some(sdf_tex),
        ..Default::default()
    });
    retained.update_camera_and_ocean(cam, Some(ocean));
    let px_ocean = renderer.render_offscreen(&scene, w, h, true, true).unwrap();
    assert_ne!(
        px_ocean, px_unshadowed,
        "GPU 8-octave ocean & coastal surf shading must visibly shade water pixels"
    );
}

#[test]
fn wgpu_geo_textures_vertex_colors_back_to_front_alpha_vram_eviction_and_multi_viewport() {
    let Ok(mut renderer) = Wgpu3DRenderer::new_headless(wgpu::TextureFormat::Bgra8Unorm) else {
        return;
    };
    let (w, h) = (128u32, 128u32);
    let anchor = [6_378_137.0_f64, 0.0_f64, 0.0_f64];
    let eye = [anchor[0] + 50.0, anchor[1], anchor[2]];
    let mut cam = camera_uniforms(eye, anchor, w, h);
    cam.background = Background3D::Solid(Color::rgba(0, 0, 0, 255));

    // 1. Stale background worker geo_revision guard on RetainedScene3D
    let quad_verts = Arc::new(vec![
        RteTileVertex::new([0.0, -10.0, -10.0], [1.0, 0.0, 0.0], [0.0, 0.0]),
        RteTileVertex::new([0.0, 10.0, -10.0], [1.0, 0.0, 0.0], [1.0, 0.0]),
        RteTileVertex::new([0.0, 10.0, 10.0], [1.0, 0.0, 0.0], [1.0, 1.0]),
        RteTileVertex::new([0.0, -10.0, 10.0], [1.0, 0.0, 0.0], [0.0, 1.0]),
    ]);
    let quad_inds = Arc::new(vec![0, 1, 2, 0, 2, 3]);
    let base_mesh = Arc::new(RetainedMeshAsset {
        id: 1001,
        revision: 1,
        vertices: Arc::clone(&quad_verts),
        indices: Arc::clone(&quad_inds),
        vertex_colors: None,
        albedo_texture: None,
    });
    let live_inst = RetainedGeoInstance {
        mesh_id: 1001,
        anchor_ecef: anchor,
        base_color: [0.0, 1.0, 0.0, 1.0],
        unlit: true,
        ..Default::default()
    };
    let retained = RetainedScene3D::new(cam);
    retained.update_geo_objects(
        Arc::new(vec![Arc::clone(&base_mesh)]),
        Arc::new(vec![live_inst.clone()]),
    );
    let rev_after_main = retained.geo_revision();
    assert!(rev_after_main >= 1);

    // Worker publishes an older snapshot (geo_revision = 0 < rev_after_main): must NOT overwrite live_inst!
    let stale_worker_frame = Arc::new(RetainedTerrainFrame {
        camera: cam,
        geometry_revision: 2,
        geo_revision: 0,
        geo_mesh_assets: Arc::default(),
        geo_instances: Arc::default(),
        ..Default::default()
    });
    retained.update_terrain_preserving_camera(stale_worker_frame);
    let snap = retained.snapshot_terrain().unwrap();
    assert_eq!(
        snap.geo_instances.len(),
        1,
        "stale worker frame must not overwrite newer main-thread geo_instances"
    );

    // 2. Albedo texture + per-vertex color rendering
    let tex_green = Arc::new(RetainedTileTexture {
        id: 9001,
        revision: 1,
        width: 2,
        height: 2,
        rgba8: Arc::new([0u8, 255, 0, 255].repeat(4)),
    });
    let colored_mesh = Arc::new(RetainedMeshAsset {
        id: 1002,
        revision: 1,
        vertices: Arc::clone(&quad_verts),
        indices: Arc::clone(&quad_inds),
        vertex_colors: Some(Arc::new(vec![[1.0, 1.0, 1.0, 1.0]; 4])),
        albedo_texture: None,
    });
    let textured_inst = RetainedGeoInstance {
        mesh_id: 1002,
        anchor_ecef: anchor,
        base_color: [1.0, 1.0, 1.0, 1.0],
        unlit: true,
        albedo_texture: Some(Arc::clone(&tex_green)),
        ..Default::default()
    };
    retained.update_geo_objects(
        Arc::new(vec![Arc::clone(&colored_mesh)]),
        Arc::new(vec![textured_inst]),
    );
    let scene = Scene3D::new().with_retained_handle(retained.clone());
    let bgra_tex = renderer.render_offscreen(&scene, w, h, true, true).unwrap();
    let center_idx = (((h / 2) * w + (w / 2)) * 4) as usize;
    assert!(
        bgra_tex[center_idx + 1] > 200 && bgra_tex[center_idx + 2] < 30,
        "albedo texture must tint geo object pure green at center pixel, got bgra={:?}",
        &bgra_tex[center_idx..center_idx + 4]
    );
    assert_eq!(renderer.cached_geo_mesh_count(), 1);
    assert_eq!(renderer.cached_geo_texture_count(), 1);

    // 3. Back-to-front depth sorting of overlapping transparent objects:
    // Near red quad (x = +10 from anchor, alpha = 0.5) and Far blue quad (x = -10 from anchor, alpha = 0.5).
    // Regardless of whether near or far is listed first in geo_instances, back-to-front sorting
    // must draw far blue FIRST and near red SECOND (producing R > B at the center pixel).
    let near_red = RetainedGeoInstance {
        mesh_id: 1001,
        anchor_ecef: [anchor[0] + 10.0, anchor[1], anchor[2]],
        base_color: [1.0, 0.0, 0.0, 0.5],
        unlit: true,
        ..Default::default()
    };
    let far_blue = RetainedGeoInstance {
        mesh_id: 1001,
        anchor_ecef: [anchor[0] - 10.0, anchor[1], anchor[2]],
        base_color: [0.0, 0.0, 1.0, 0.5],
        unlit: true,
        ..Default::default()
    };
    retained.update_geo_objects(
        Arc::new(vec![Arc::clone(&base_mesh)]),
        Arc::new(vec![near_red.clone(), far_blue.clone()]),
    );
    let px_near_first = renderer.render_offscreen(&scene, w, h, true, true).unwrap();
    retained.update_geo_objects(
        Arc::new(vec![Arc::clone(&base_mesh)]),
        Arc::new(vec![far_blue, near_red]),
    );
    let px_far_first = renderer.render_offscreen(&scene, w, h, true, true).unwrap();
    let c_nf = &px_near_first[center_idx..center_idx + 4];
    let c_ff = &px_far_first[center_idx..center_idx + 4];
    assert_eq!(
        c_nf, c_ff,
        "transparent geo objects must render identically regardless of spawn order"
    );
    assert!(
        c_nf[2] > c_nf[0] + 35,
        "near red quad (0.5 alpha) over far blue quad (0.5 alpha) must composite red-dominant (R={}, B={})",
        c_nf[2],
        c_nf[0]
    );

    // 4. VRAM eviction: clearing geo objects and rendering >32 frames evicts unused geo_meshes & geo_textures.
    retained.update_geo_objects(Arc::default(), Arc::default());
    for _ in 0..36 {
        let _ = renderer.render_offscreen(&scene, w, h, false, true).unwrap();
    }
    assert_eq!(
        renderer.cached_geo_mesh_count(),
        0,
        "unreferenced geo_meshes must be evicted from GPU VRAM after grace window"
    );
    assert_eq!(
        renderer.cached_geo_texture_count(),
        0,
        "unreferenced geo_textures must be evicted from GPU VRAM after grace window"
    );
}

#[test]
fn wgpu_2d_unlit_imagery_preserves_exact_brightness_across_srgb_and_non_srgb_swapchains() {
    let w = 64u32;
    let h = 64u32;
    let tile_rgb = [180u8, 140, 100];
    let tile_px = [tile_rgb[0], tile_rgb[1], tile_rgb[2], 255].repeat(256 * 256);
    let tex = Arc::new(RetainedTileTexture {
        id: 777,
        revision: 1,
        width: 256,
        height: 256,
        rgba8: Arc::new(tile_px),
    });

    // Horizontal XZ tile at Y = 0.0 (grid_res=2 winding matching Wgpu3DRenderer terrain index buffer)
    let tile_id = TileKeyId::new(10, 0, 0, 0);
    let mesh = Arc::new(RetainedTileMesh {
        key: tile_id,
        revision: 1,
        center_ecef: [0.0, 0.0, 0.0],
        grid_res: 2,
        has_skirts: false,
        vertices: Arc::new(vec![
            RteTileVertex::new([-5000.0, 0.0, 5000.0], [0.0, 1.0, 0.0], [0.0, 1.0]),
            RteTileVertex::new([5000.0, 0.0, 5000.0], [0.0, 1.0, 0.0], [1.0, 1.0]),
            RteTileVertex::new([-5000.0, 0.0, -5000.0], [0.0, 1.0, 0.0], [0.0, 0.0]),
            RteTileVertex::new([5000.0, 0.0, -5000.0], [0.0, 1.0, 0.0], [1.0, 0.0]),
        ]),
        bounding_radius: 7500.0,
    });

    // Camera ~4,240m away (> 3,500m aerial haze onset and within camera_uniforms far=5,000m) with zeroed sky_bottom in 2D mode
    let mut cam = camera_uniforms([0.0, 4_200.0, 600.0], [0.0, 0.0, 0.0], w, h);
    cam.background = Background3D::Solid(Color::rgba(0, 0, 0, 255));
    cam.day_night_strength = 0.0;
    cam.local_25d_blend = 0.0;
    cam.map_2d_morph = 1.0;

    for (fmt, unlit) in [
        (wgpu::TextureFormat::Bgra8Unorm, true),
        (wgpu::TextureFormat::Bgra8Unorm, false),
        (wgpu::TextureFormat::Bgra8UnormSrgb, true),
        (wgpu::TextureFormat::Bgra8UnormSrgb, false),
    ] {
        let Ok(mut renderer) = Wgpu3DRenderer::new_headless(fmt) else {
            return;
        };
        let frame = Arc::new(RetainedTerrainFrame {
            camera: cam,
            geometry_revision: 1,
            meshes: vec![Arc::clone(&mesh)],
            textures: vec![Arc::clone(&tex)],
            instances: vec![RetainedTileInstance {
                key: tile_id,
                mesh_revision: 1,
                center_ecef: [0.0, 0.0, 0.0],
                texture_id: Some(777),
                texture_revision: 1,
                uv_rect: [0.0, 0.0, 1.0, 1.0],
                tint: [1.0, 1.0, 1.0, 1.0],
                unlit,
                night_texture_id: None,
                night_texture_revision: 0,
                night_uv_rect: [0.0, 0.0, 1.0, 1.0],
            }],
            ..Default::default()
        });
        let retained = RetainedScene3D::new(cam);
        retained.update_terrain(frame);
        let scene = Scene3D::new().with_retained_handle(retained);
        let bgra = renderer.render_offscreen(&scene, w, h, true, true).unwrap();
        let c = ((h / 2) * w + (w / 2)) as usize * 4;
        let (r, g, b) = (bgra[c + 2], bgra[c + 1], bgra[c]);
        assert!(
            (r as i32 - tile_rgb[0] as i32).abs() <= 2
                && (g as i32 - tile_rgb[1] as i32).abs() <= 2
                && (b as i32 - tile_rgb[2] as i32).abs() <= 2,
            "2D tile on {fmt:?} (unlit={unlit}) must preserve exact imagery RGB {tile_rgb:?} without gamma/haze darkening, got ({r}, {g}, {b})"
        );
    }
}

#[test]
fn wgpu_3d_ocean_vertex_wave_displacement_and_close_ground_micro_detail() {
    use strata_render::scene3d::{RetainedOceanSurface, RetainedOceanWaveTrain};

    let Ok(mut renderer) = Wgpu3DRenderer::new_headless(wgpu::TextureFormat::Bgra8Unorm) else {
        return;
    };
    let w = 96u32;
    let h = 96u32;

    // Flat uniform grey-brown swatch for close-ground (<50m) procedural micro-detail test.
    let swatch_rgba = vec![140u8, 132, 118, 255].repeat(64 * 64);
    let land_tex = Arc::new(RetainedTileTexture {
        id: 888,
        revision: 1,
        width: 64,
        height: 64,
        rgba8: Arc::new(swatch_rgba),
    });
    // Deep ocean blue swatch for ocean wave displacement & shading test.
    let ocean_rgba = vec![24u8, 62, 115, 255].repeat(64 * 64);
    let ocean_tex = Arc::new(RetainedTileTexture {
        id: 889,
        revision: 1,
        width: 64,
        height: 64,
        rgba8: Arc::new(ocean_rgba),
    });

    // Subdivided sea-level / flat land tile grid on XZ plane (Y = 0.0, normal +Y)
    let grid = 17u32;
    let span = 200.0f32;
    let mut verts = Vec::with_capacity((grid * grid) as usize);
    for r in 0..grid {
        let v = r as f32 / (grid - 1) as f32;
        let vz = (0.5 - v) * span;
        for c in 0..grid {
            let u = c as f32 / (grid - 1) as f32;
            let vx = (u - 0.5) * span;
            verts.push(RteTileVertex::new([vx, 0.0, vz], [0.0, 1.0, 0.0], [u, v]));
        }
    }
    let key = TileKeyId::new(15, 0, 0, 0);
    let mesh = Arc::new(RetainedTileMesh {
        key,
        revision: 1,
        center_ecef: [0.0, 0.0, 0.0],
        grid_res: grid,
        has_skirts: false,
        vertices: Arc::new(verts),
        bounding_radius: 150.0,
    });

    // 1. Ocean wave test: oblique view over ocean tile at t=0.0 vs t=3.0 (plus 2D bypass check)
    let mut base_ocean_cam = camera_uniforms([0.0, 26.0, 70.0], [0.0, 0.0, -10.0], w, h);
    base_ocean_cam.sun_dir = [0.35, 0.58, 0.72];
    base_ocean_cam.sun_color = [1.0, 0.96, 0.90];
    base_ocean_cam.ambient_color = [0.25, 0.30, 0.38];
    base_ocean_cam.day_night_strength = 1.0;
    base_ocean_cam.local_25d_blend = 1.0;
    base_ocean_cam.realistic_25d_sky = 1.0;
    base_ocean_cam.terrain_25d_lighting = 1.0;

    let render_ocean_at = |renderer: &mut Wgpu3DRenderer, t: f32, map_2d: f32| {
        let mut cam = base_ocean_cam;
        cam.time_s = t;
        cam.map_2d_morph = map_2d;
        let frame = Arc::new(RetainedTerrainFrame {
            camera: cam,
            geometry_revision: 1,
            meshes: vec![Arc::clone(&mesh)],
            textures: vec![Arc::clone(&ocean_tex)],
            instances: vec![RetainedTileInstance {
                key,
                mesh_revision: 1,
                center_ecef: [0.0, 0.0, 0.0],
                texture_id: Some(889),
                texture_revision: 1,
                uv_rect: [0.0, 0.0, 1.0, 1.0],
                tint: [1.0, 1.0, 1.0, 1.0],
                unlit: false,
                night_texture_id: None,
                night_texture_revision: 0,
                night_uv_rect: [0.0, 0.0, 1.0, 1.0],
            }],
            ocean: Some(Arc::new(RetainedOceanSurface {
                enabled: true,
                alt_fade: 1.0,
                wave_strength: 2.4,
                wave_scale: 1.0,
                glint_intensity: 1.6,
                origin_ecef: [0.0, 0.0, 0.0],
                basis_east: [1.0, 0.0, 0.0],
                basis_north: [0.0, 0.0, -1.0],
                basis_up: [0.0, 1.0, 0.0],
                train_a: RetainedOceanWaveTrain {
                    dir_en: [0.8, 0.6],
                    wind_speed_mps: 20.0,
                    phase_time_s: t,
                    weight: 1.0,
                },
                opacity: 1.0,
                ..Default::default()
            })),
            ..Default::default()
        });
        let retained = RetainedScene3D::new(cam);
        retained.update_terrain(frame);
        let scene = Scene3D::new().with_retained_handle(retained);
        renderer.render_offscreen(&scene, w, h, true, true).unwrap()
    };

    let px_t0 = render_ocean_at(&mut renderer, 0.0, 0.0);
    let px_t3 = render_ocean_at(&mut renderer, 3.0, 0.0);
    assert_ne!(
        px_t0, px_t3,
        "continuous 3D ocean waves must animate vertex & normal pixels across time_s"
    );

    // 2D mode must completely suppress ocean animation & lighting on the exact same scene
    let px_2d_0 = render_ocean_at(&mut renderer, 0.0, 1.0);
    let px_2d_3 = render_ocean_at(&mut renderer, 3.0, 1.0);
    assert_eq!(
        px_2d_0, px_2d_3,
        "Mode2D (map_2d_morph = 1.0) must bypass ocean wave shading completely"
    );

    // 2. Close-ground (<50m) procedural micro-detail test without ocean overlay
    let mut close_cam = camera_uniforms([0.0, 18.0, 8.0], [0.0, 0.0, 0.0], w, h);
    close_cam.sun_dir = [0.6, 0.68, 0.4];
    close_cam.day_night_strength = 1.0;
    close_cam.local_25d_blend = 1.0;
    close_cam.terrain_25d_lighting = 1.0;

    let close_frame = Arc::new(RetainedTerrainFrame {
        camera: close_cam,
        geometry_revision: 1,
        meshes: vec![mesh],
        textures: vec![land_tex],
        instances: vec![RetainedTileInstance {
            key,
            mesh_revision: 1,
            center_ecef: [0.0, 0.0, 0.0],
            texture_id: Some(888),
            texture_revision: 1,
            uv_rect: [0.0, 0.0, 1.0, 1.0],
            tint: [1.0, 1.0, 1.0, 1.0],
            unlit: false,
            night_texture_id: None,
            night_texture_revision: 0,
            night_uv_rect: [0.0, 0.0, 1.0, 1.0],
        }],
        ..Default::default()
    });
    let close_retained = RetainedScene3D::new(close_cam);
    close_retained.update_terrain(close_frame);
    let close_scene = Scene3D::new().with_retained_handle(close_retained);
    let close_px = renderer.render_offscreen(&close_scene, w, h, true, true).unwrap();
    let mut min_l = 255i32;
    let mut max_l = 0i32;
    for y in 24..72 {
        for x in 24..72 {
            let idx = (y * w + x) as usize * 4;
            let l = (close_px[idx + 2] as i32 * 3 + close_px[idx + 1] as i32 * 6 + close_px[idx] as i32) / 10;
            min_l = min_l.min(l);
            max_l = max_l.max(l);
        }
    }
    assert!(
        max_l - min_l >= 8,
        "close-ground (<50m) shader must synthesize crisp surface micro-detail over flat swatch (spread {}..{})",
        min_l,
        max_l
    );
}

#[test]
fn wgpu_mixed_close_ground_grid_topologies_render_all_tiles_cleanly() {
    let Ok(mut renderer) = Wgpu3DRenderer::new_headless(wgpu::TextureFormat::Bgra8Unorm) else {
        return;
    };
    let (w, h) = (128u32, 128u32);

    let make_grid_mesh = |key: TileKeyId, grid: u32, min_x: f32, max_x: f32, min_z: f32, max_z: f32| {
        let n = grid as usize;
        let mut verts = Vec::with_capacity(n * n + 4 * n);
        for row in 0..n {
            let v = row as f32 / (n - 1) as f32;
            // Notice z decreases northward so triangles face +Y up under CCW winding.
            let pz = max_z + (min_z - max_z) * v;
            for col in 0..n {
                let u = col as f32 / (n - 1) as f32;
                let px = min_x + (max_x - min_x) * u;
                verts.push(RteTileVertex::new([px, 0.0, pz], [0.0, 1.0, 0.0], [u, 1.0 - v]));
            }
        }
        // Append 4*n skirt vertices matching build_terrain_grid_indices(grid, true).
        for col in 0..n {
            let mut v = verts[col];
            v.pos_u[1] -= 20.0;
            verts.push(v);
        }
        for row in 0..n {
            let mut v = verts[row * n + (n - 1)];
            v.pos_u[1] -= 20.0;
            verts.push(v);
        }
        for col in 0..n {
            let mut v = verts[(n - 1) * n + col];
            v.pos_u[1] -= 20.0;
            verts.push(v);
        }
        for row in 0..n {
            let mut v = verts[row * n];
            v.pos_u[1] -= 20.0;
            verts.push(v);
        }
        Arc::new(RetainedTileMesh {
            key,
            revision: 1,
            center_ecef: [0.0, 0.0, 0.0],
            grid_res: grid,
            has_skirts: true,
            vertices: Arc::new(verts),
            bounding_radius: 150.0,
        })
    };

    // Mix 17x17, 25x25, and 33x33 close-ground tiles in the exact same frame!
    let k_nw = TileKeyId::new(17, 10, 10, 0);
    let k_ne = TileKeyId::new(17, 11, 10, 0);
    let k_sw = TileKeyId::new(17, 10, 11, 0);
    let k_se = TileKeyId::new(17, 11, 11, 0);
    let meshes = vec![
        make_grid_mesh(k_nw, 17, -100.0, 0.0, -100.0, 0.0),
        make_grid_mesh(k_ne, 25, 0.0, 100.0, -100.0, 0.0),
        make_grid_mesh(k_sw, 33, -100.0, 0.0, 0.0, 100.0),
        make_grid_mesh(k_se, 17, 0.0, 100.0, 0.0, 100.0),
    ];
    let warm_tint = [0.90, 0.42, 0.20, 1.0];
    let instances = vec![k_nw, k_ne, k_sw, k_se]
        .into_iter()
        .map(|key| RetainedTileInstance {
            key,
            mesh_revision: 1,
            center_ecef: [0.0, 0.0, 0.0],
            texture_id: None,
            texture_revision: 0,
            uv_rect: [0.0, 0.0, 1.0, 1.0],
            tint: warm_tint,
            unlit: true,
            night_texture_id: None,
            night_texture_revision: 0,
            night_uv_rect: [0.0, 0.0, 1.0, 1.0],
        })
        .collect();

    let mut cam = camera_uniforms([0.0, 90.0, 1.0], [0.0, 0.0, 0.0], w, h);
    cam.background = Background3D::Solid(strata_style::Color::rgb(20, 110, 255));
    let frame = Arc::new(RetainedTerrainFrame {
        camera: cam,
        geometry_revision: 1,
        meshes,
        instances,
        ..Default::default()
    });
    let retained = RetainedScene3D::new(cam);
    retained.update_terrain(frame);
    let scene = Scene3D::new().with_retained_handle(retained);
    let px = renderer.render_offscreen(&scene, w, h, true, true).unwrap();

    // Every pixel in the central 80x80 covering all 4 quadrants must be warm terracotta (R > B),
    // never ripped into sawtooth voids exposing the blue background sky.
    let mut hole_count = 0usize;
    for y in 24..104 {
        for x in 24..104 {
            let idx = (y * w + x) as usize * 4;
            let b = px[idx];
            let r = px[idx + 2];
            if b >= r {
                hole_count += 1;
            }
        }
    }
    assert_eq!(
        hole_count, 0,
        "mixed close-ground tile grids (17x17, 25x25, 33x33) must all render solidly with zero index-stride holes"
    );
}

#[test]
fn wgpu_ocean_waves_are_invariant_across_origin_shift_and_non_repetitive() {
    use strata_render::scene3d::{RetainedOceanSurface, RetainedOceanWaveTrain};

    let Ok(mut renderer) = Wgpu3DRenderer::new_headless(wgpu::TextureFormat::Bgra8Unorm) else {
        return;
    };
    let (w, h) = (128u32, 128u32);
    let r_earth = 6_378_137.0_f64;

    let ocean_rgba = vec![20u8, 60, 118, 255].repeat(64 * 64);
    let ocean_tex = Arc::new(RetainedTileTexture {
        id: 890,
        revision: 1,
        width: 64,
        height: 64,
        rgba8: Arc::new(ocean_rgba),
    });

    // Place a 400x400m flat sea-level patch in a tangent frame matching WGS84 2.5D at (37.75°, -122.50°).
    let lat0_deg = 37.75_f64;
    let lon0_deg = -122.50_f64;
    let lat0_rad = lat0_deg.to_radians();
    let cos0 = lat0_rad.cos();
    let anchor = [6_378_137.0_f64, 0.0, 0.0];
    let grid = 17u32;
    let span = 400.0_f32;
    let mut verts = Vec::with_capacity((grid * grid) as usize);
    for row in 0..grid {
        let v = row as f32 / (grid - 1) as f32;
        let pz = (0.5 - v) * span; // -north
        for col in 0..grid {
            let u = col as f32 / (grid - 1) as f32;
            let px = (u - 0.5) * span; // +east
            verts.push(RteTileVertex::new([px, 0.0, pz], [0.0, 1.0, 0.0], [u, v]));
        }
    }
    let key = TileKeyId::new(15, 0, 0, 0);
    let mesh = Arc::new(RetainedTileMesh {
        key,
        revision: 1,
        center_ecef: anchor,
        grid_res: grid,
        has_skirts: false,
        vertices: Arc::new(verts),
        bounding_radius: 300.0,
    });

    let merc_y = |lat_rad: f64| -> f64 {
        r_earth * (std::f64::consts::FRAC_PI_4 + lat_rad * 0.5).tan().ln()
    };

    let render_with_origin = |renderer: &mut Wgpu3DRenderer, dlat_deg: f64, dlon_deg: f64| {
        let q_lat = lat0_deg + dlat_deg;
        let q_lon = lon0_deg + dlon_deg;
        let de_m = dlon_deg.to_radians() * r_earth * cos0;
        let dn_m = dlat_deg.to_radians() * r_earth;
        // Origin placed at anchor + de_m * east(+X) + dn_m * north(-Z).
        let orig_ecef = [anchor[0] + de_m, anchor[1], anchor[2] - dn_m];
        let wx = r_earth * q_lon.to_radians();
        let wy = merc_y(q_lat.to_radians());

        let mut cam = camera_uniforms(
            [anchor[0] + 0.0, 95.0, anchor[2] + 150.0],
            [anchor[0], 0.0, anchor[2] - 20.0],
            w,
            h,
        );
        cam.sun_dir = [0.42, 0.48, 0.76];
        cam.sun_color = [1.0, 0.96, 0.90];
        cam.day_night_strength = 1.0;
        cam.local_25d_blend = 1.0;
        cam.realistic_25d_sky = 1.0;
        cam.terrain_25d_lighting = 1.0;

        let frame = Arc::new(RetainedTerrainFrame {
            camera: cam,
            geometry_revision: 1,
            meshes: vec![Arc::clone(&mesh)],
            textures: vec![Arc::clone(&ocean_tex)],
            instances: vec![RetainedTileInstance {
                key,
                mesh_revision: 1,
                center_ecef: anchor,
                texture_id: Some(890),
                texture_revision: 1,
                uv_rect: [0.0, 0.0, 1.0, 1.0],
                tint: [1.0, 1.0, 1.0, 1.0],
                unlit: false,
                night_texture_id: None,
                night_texture_revision: 0,
                night_uv_rect: [0.0, 0.0, 1.0, 1.0],
            }],
            ocean: Some(Arc::new(RetainedOceanSurface {
                enabled: true,
                origin_ecef: orig_ecef,
                basis_east: [1.0, 0.0, 0.0],
                basis_north: [0.0, 0.0, -1.0],
                basis_up: [0.0, 1.0, 0.0],
                train_a: RetainedOceanWaveTrain {
                    dir_en: [0.92, 0.39],
                    wind_speed_mps: 9.0,
                    phase_time_s: 12.5,
                    weight: 1.0,
                },
                wave_strength: 1.3,
                wave_scale: 1.0,
                glint_intensity: 1.2,
                opacity: 1.0,
                alt_fade: 1.0,
                world_origin_merc_m: [wx, wy],
                origin_lat_rad: q_lat.to_radians() as f32,
                east_scale: cos0 as f32,
                ..Default::default()
            })),
            ..Default::default()
        });
        let retained = RetainedScene3D::new(cam);
        retained.update_terrain(frame);
        let scene = Scene3D::new().with_retained_handle(retained);
        renderer.render_offscreen(&scene, w, h, true, true).unwrap()
    };

    let px_a = render_with_origin(&mut renderer, 0.0, 0.0);
    // Simulate panning across a quantized +0.05° (+5.5 km) ocean origin snap cell!
    let px_b = render_with_origin(&mut renderer, 0.05, 0.05);

    let mut max_diff = 0i32;
    let mut sum_diff = 0i64;
    for (&a, &b) in px_a.iter().zip(px_b.iter()) {
        let d = (a as i32 - b as i32).abs();
        max_diff = max_diff.max(d);
        sum_diff += d as i64;
    }
    let mean_diff = sum_diff as f64 / px_a.len() as f64;
    assert!(
        mean_diff < 1.5 && max_diff <= 24,
        "ocean waves must stay invariant across +0.05° origin snap jumps (mean_diff={mean_diff:.3}, max_diff={max_diff})"
    );

    // Check left vs right patches aren't uniform or trivially repeated
    let mut diff_lr = 0i64;
    for y in 40..100 {
        for x in 0..48 {
            let i0 = (y * w + (x + 12)) as usize * 4;
            let i1 = (y * w + (x + 68)) as usize * 4;
            diff_lr += (px_a[i0] as i32 - px_a[i1] as i32).abs() as i64;
        }
    }
    assert!(
        diff_lr > 5_000,
        "stochastic ocean waves must show rich non-repetitive spatial variation across the water surface (diff_lr={diff_lr})"
    );
}

#[test]
fn wgpu_coastal_swash_animates_and_offshore_fetch_grows_smoothly() {
    use strata_render::scene3d::{RetainedOceanSurface, RetainedOceanWaveTrain};

    let Ok(mut renderer) = Wgpu3DRenderer::new_headless(wgpu::TextureFormat::Bgra8Unorm) else {
        return;
    };
    let (w, h) = (128u32, 128u32);

    // Coastal tile: X in [-180..+180] m where X < -90 is sloping sandy shore (+Y up to 2.2m),
    // and X >= -90 is shallow-to-open sea (Y = 0).
    let grid = 25u32;
    let span = 360.0_f32;
    let mut verts = Vec::with_capacity((grid * grid) as usize);
    for row in 0..grid {
        let v = row as f32 / (grid - 1) as f32;
        let pz = (0.5 - v) * span;
        for col in 0..grid {
            let u = col as f32 / (grid - 1) as f32;
            let px = (u - 0.5) * span;
            let py = if px < -90.0 {
                ((-90.0 - px) / 90.0) * 2.2
            } else {
                0.0
            };
            verts.push(RteTileVertex::new([px, py, pz], [0.0, 1.0, 0.0], [u, v]));
        }
    }
    let mut tex_px = Vec::with_capacity(64 * 64 * 4);
    for _y in 0..64 {
        for x in 0..64 {
            if x < 16 {
                tex_px.extend_from_slice(&[178, 152, 118, 255]);
            } else {
                tex_px.extend_from_slice(&[24, 72, 126, 255]);
            }
        }
    }
    let coast_tex = Arc::new(RetainedTileTexture {
        id: 891,
        revision: 1,
        width: 64,
        height: 64,
        rgba8: Arc::new(tex_px),
    });

    // SDF covering [-180..+180] m: shore at u = 0.25 (X = -90m), distance increasing East (+X).
    let sdf_w = 32usize;
    let mut sdf_rgba = Vec::with_capacity(sdf_w * sdf_w * 4);
    let range_m = 270.0_f32;
    for _y in 0..sdf_w {
        for x in 0..sdf_w {
            let px = ((x as f32 + 0.5) / sdf_w as f32 - 0.5) * span;
            let d_m = (px + 90.0).max(0.0);
            let w_mask = if px < -90.0 { 0u8 } else { 255u8 };
            let g = ((d_m / range_m).clamp(0.0, 1.0) * 255.0).round() as u8;
            sdf_rgba.extend_from_slice(&[w_mask, g, 0, 128]);
        }
    }
    let sdf_tex = Arc::new(RetainedTileTexture {
        id: 892,
        revision: 1,
        width: sdf_w as u32,
        height: sdf_w as u32,
        rgba8: Arc::new(sdf_rgba),
    });

    let key = TileKeyId::new(16, 0, 0, 0);
    let mesh = Arc::new(RetainedTileMesh {
        key,
        revision: 1,
        center_ecef: [0.0, 0.0, 0.0],
        grid_res: grid,
        has_skirts: false,
        vertices: Arc::new(verts),
        bounding_radius: 260.0,
    });

    let render_at_t = |renderer: &mut Wgpu3DRenderer, t: f32| {
        let mut cam = camera_uniforms([-40.0, 48.0, 75.0], [-45.0, 0.0, 0.0], w, h);
        cam.sun_dir = [0.45, 0.55, 0.70];
        cam.sun_color = [1.0, 0.96, 0.90];
        cam.day_night_strength = 1.0;
        cam.local_25d_blend = 1.0;
        cam.realistic_25d_sky = 1.0;
        cam.terrain_25d_lighting = 1.0;
        cam.time_s = t;

        let ocean = Arc::new(RetainedOceanSurface {
            enabled: true,
            origin_ecef: [0.0, 0.0, 0.0],
            basis_east: [1.0, 0.0, 0.0],
            basis_north: [0.0, 0.0, -1.0],
            basis_up: [0.0, 1.0, 0.0],
            train_a: RetainedOceanWaveTrain {
                dir_en: [-0.9, 0.43],
                wind_speed_mps: 9.0,
                phase_time_s: t,
                weight: 1.0,
            },
            wave_strength: 1.0,
            wave_scale: 1.0,
            glint_intensity: 1.2,
            opacity: 0.9,
            alt_fade: 1.0,
            coastal_shoaling: true,
            shoaling_range_m: range_m,
            sdf_half_span_m: span * 0.5,
            sdf_center_en_m: [0.0, 0.0],
            coastal_sdf: Some(Arc::clone(&sdf_tex)),
            ..Default::default()
        });
        let frame = Arc::new(RetainedTerrainFrame {
            camera: cam,
            geometry_revision: 1,
            meshes: vec![Arc::clone(&mesh)],
            textures: vec![Arc::clone(&coast_tex)],
            instances: vec![RetainedTileInstance {
                key,
                mesh_revision: 1,
                center_ecef: [0.0, 0.0, 0.0],
                texture_id: Some(891),
                texture_revision: 1,
                uv_rect: [0.0, 0.0, 1.0, 1.0],
                tint: [1.0, 1.0, 1.0, 1.0],
                unlit: false,
                ..Default::default()
            }],
            ocean: Some(ocean),
            ..Default::default()
        });
        let retained = RetainedScene3D::new(cam);
        retained.update_terrain(frame);
        let scene = Scene3D::new().with_retained_handle(retained);
        renderer.render_offscreen(&scene, w, h, true, true).unwrap()
    };

    let px_t0 = render_at_t(&mut renderer, 0.0);
    let px_t3 = render_at_t(&mut renderer, 3.2);
    let px_t6 = render_at_t(&mut renderer, 6.4);
    assert_ne!(
        px_t0, px_t3,
        "coastal swash & surf must visibly come and go between t=0s and t=3.2s"
    );
    assert_ne!(
        px_t3, px_t6,
        "coastal swash & surf must keep cycling naturally between t=3.2s and t=6.4s"
    );
}

#[test]
fn wgpu_2d_transition_has_no_pale_blue_flash_and_pole_seam_is_clean() {
    let mut renderer = match Wgpu3DRenderer::new_headless(wgpu::TextureFormat::Bgra8UnormSrgb) {
        Ok(r) => r,
        Err(reason) => {
            eprintln!("[skip wgpu_2d_transition test] {}", reason);
            return;
        }
    };

    let (w, h) = (160u32, 120u32);
    let mut tex_px = vec![0u8; 4 * 4 * 4];
    for p in tex_px.chunks_exact_mut(4) {
        p.copy_from_slice(&[72, 92, 64, 255]);
    }
    let tile_tex = Arc::new(RetainedTileTexture {
        id: 991,
        revision: 1,
        width: 4,
        height: 4,
        rgba8: Arc::new(tex_px),
    });

    // Build a North Pole cap patch centered at (0, 0, R) so center vertex has dv = (0, 0, 1).
    let r_pole = 6_356_752.314_2f64;
    let grid = 5u32;
    let mut verts = Vec::with_capacity((grid * grid) as usize);
    for row in 0..grid {
        let fy = (row as f32 / (grid - 1) as f32) - 0.5;
        for col in 0..grid {
            let fx = (col as f32 / (grid - 1) as f32) - 0.5;
            verts.push(RteTileVertex::new(
                [fx * 900.0, fy * 900.0, 0.0],
                [0.0, 0.0, 1.0],
                [col as f32 / (grid - 1) as f32, row as f32 / (grid - 1) as f32],
            ));
        }
    }
    let key = TileKeyId::quadtree(12, 0, 0);
    let mesh = Arc::new(RetainedTileMesh {
        key,
        revision: 1,
        center_ecef: [0.0, 0.0, r_pole],
        grid_res: grid,
        has_skirts: false,
        vertices: Arc::new(verts),
        bounding_radius: 750.0,
    });

    // Regime A: Orbital 3D -> 2D must NEVER flash classic pale-blue sky at any intermediate morph!
    let orbital_cam = |morph: f32| {
        let mut cam = camera_uniforms([0.0, -5_000_000.0, r_pole + 9_000_000.0], [0.0, 0.0, r_pole], w, h);
        let k = (1.0 - morph).clamp(0.0, 1.0);
        cam.background = Background3D::VerticalGradient {
            top: Color::rgba((18.0 * k) as u8, (64.0 * k) as u8, (140.0 * k) as u8, 255),
            bottom: Color::rgba((135.0 * k) as u8, (206.0 * k) as u8, (250.0 * k) as u8, 255),
        };
        cam.map_2d_morph = morph;
        cam.local_25d_blend = 0.0;
        cam.stars_visible = morph < 0.85;
        cam.realistic_25d_sky = 1.0 - morph;
        cam.terrain_25d_lighting = 1.0 - morph;
        cam.day_night_strength = 1.0 - morph;
        cam.sun_dir = [0.3, 0.4, 0.86];
        cam.sun_color = [1.0, 0.96, 0.90];
        cam
    };
    let init_cam = orbital_cam(0.0);
    let frame = Arc::new(RetainedTerrainFrame {
        camera: init_cam,
        geometry_revision: 1,
        meshes: vec![mesh],
        textures: vec![tile_tex],
        instances: vec![RetainedTileInstance {
            key,
            mesh_revision: 1,
            center_ecef: [0.0, 0.0, r_pole],
            texture_id: Some(991),
            texture_revision: 1,
            uv_rect: [0.0, 0.0, 1.0, 1.0],
            tint: [1.0, 1.0, 1.0, 1.0],
            unlit: false,
            ..Default::default()
        }],
        ..Default::default()
    });
    let retained = RetainedScene3D::new(init_cam);
    retained.update_terrain(frame);
    let scene = Scene3D::new().with_retained_handle(retained.clone());

    for morph in [0.06f32, 0.15, 0.40, 0.70, 0.95] {
        retained.update_camera(orbital_cam(morph));
        let bgra = renderer
            .render_offscreen(&scene, w, h, true, true)
            .expect("wgpu orbital intermediate 2D morph frame");
        let corner = ((4 * w + 4) * 4) as usize;
        let corner_b = bgra[corner];
        let corner_r = bgra[corner + 2];
        assert!(
            corner_b <= 48 && corner_b.saturating_sub(corner_r) <= 24,
            "unexpected pale-blue flash in orbital sky at map_2d_morph={morph}: BGRA={:?}",
            &bgra[corner..corner + 4]
        );
    }

    // Regime B: Local 2.5D -> 2D must monotonically fade daytime sky/haze down to dark without spiking bluer mid-morph.
    let local_cam = |morph: f32| {
        let mut cam = camera_uniforms([0.0, -800.0, r_pole + 1_200.0], [0.0, 0.0, r_pole], w, h);
        let k = (1.0 - morph).clamp(0.0, 1.0);
        cam.background = Background3D::VerticalGradient {
            top: Color::rgba((18.0 * k) as u8, (64.0 * k) as u8, (140.0 * k) as u8, 255),
            bottom: Color::rgba((135.0 * k) as u8, (206.0 * k) as u8, (250.0 * k) as u8, 255),
        };
        cam.map_2d_morph = morph;
        cam.local_25d_blend = 1.0 - morph;
        cam.stars_visible = morph < 0.85;
        cam.realistic_25d_sky = 1.0 - morph;
        cam.terrain_25d_lighting = 1.0 - morph;
        cam.day_night_strength = 0.0;
        cam.sun_dir = [0.3, 0.4, 0.86];
        cam.sun_color = [1.0, 0.96, 0.90];
        cam
    };
    let mut prev_sky_b = 255u8;
    for morph in [0.0f32, 0.25, 0.50, 0.75, 0.95, 1.0] {
        retained.update_camera(local_cam(morph));
        let bgra = renderer
            .render_offscreen(&scene, w, h, true, true)
            .expect("wgpu local 2.5D intermediate 2D morph frame");
        let corner = ((4 * w + 4) * 4) as usize;
        let corner_b = bgra[corner];
        assert!(
            corner_b <= prev_sky_b.saturating_add(4),
            "2.5D sky blue channel must decrease monotonically during 2D transition: prev={prev_sky_b}, cur={corner_b} at morph={morph}"
        );
        if (morph - 0.95).abs() < 1e-3 {
            assert!(
                corner_b <= 75,
                "at morph=0.95 sRGB 5% background should already be dim, got corner_b={corner_b}"
            );
        }
        prev_sky_b = corner_b;
        let center = (((h / 2) * w + (w / 2)) * 4) as usize;
        let c_b = bgra[center];
        let c_g = bgra[center + 1];
        assert!(
            c_g >= c_b,
            "North Pole center terrain must keep natural green tint without blue wash at morph={morph}: BGRA={:?}",
            &bgra[center..center + 4]
        );
    }
    assert_eq!(
        prev_sky_b, 0,
        "2D sky must reach exact black by morph=1.0, got corner_b={prev_sky_b}"
    );
}







