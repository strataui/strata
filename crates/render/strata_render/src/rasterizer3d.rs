//! 3D Software Rasterizer & 2D/3D Viewport Compositing Bridge.

use crate::geometry3d::{
    generate_box, generate_custom, generate_custom_textured, generate_cylinder, generate_plane,
    generate_sphere,
};
use crate::math3d::{Mat4, Vec3, Vec4};
use crate::rasterizer::FrameBuffer;
use crate::scene3d::{
    Background3D, Camera3D, Geometry3D, Light3D, Material3D, Scene3D, TexturedMaterial3D,
};
use crate::texture3d::{
    sample_bilinear_clamped, sample_texture, AddressMode, FilterMode, TextureStore,
};
use strata_layout::Rect;
use strata_style::types::Color;

/// Renders a Scene3D directly into a FrameBuffer viewport region with Z-buffering.
pub fn render_scene_3d(
    frame_buffer: &mut FrameBuffer,
    viewport: Rect,
    scene: &Scene3D,
    clip: Option<Rect>,
) {
    render_scene_3d_with_textures(frame_buffer, viewport, scene, clip, None);
}

/// Renders a Scene3D, resolving textured materials against `textures`, then
/// draws the scene overlay pass on top.
///
/// Passing `None` keeps the untextured behaviour: textured meshes fall back to
/// their tint colour so a scene that references a missing texture is still
/// visible instead of silently disappearing.
///
/// Overlay text uses the rasterizer installed with
/// [`crate::scene3d::register_overlay_text_rasterizer`]; without one, labels
/// are skipped and the geometric overlay still draws.
pub fn render_scene_3d_with_textures(
    frame_buffer: &mut FrameBuffer,
    viewport: Rect,
    scene: &Scene3D,
    clip: Option<Rect>,
    textures: Option<&TextureStore>,
) {
    if viewport.width <= 1.0 || viewport.height <= 1.0 {
        return;
    }

    // Background fill happens before any depth-tested geometry. Pixels are
    // written directly, so a translucent clear colour still replaces the
    // previous frame instead of being alpha-blended over it.
    match scene.background() {
        Background3D::Transparent => {}
        Background3D::Solid(color) => fill_viewport(frame_buffer, viewport, |_| color),
        Background3D::VerticalGradient { .. } => {
            let height = viewport.height.max(1.0);
            let background = scene.background();
            fill_viewport(frame_buffer, viewport, |row| {
                background
                    .color_at(row as f32 / height)
                    .unwrap_or(Color::TRANSPARENT)
            });
        }
    }

    let aspect = viewport.width / viewport.height;

    // 1. Compute Projection and View Matrices. The near plane is needed by the
    // clipping stage, and the combined matrix is only used by the overlay.
    let (proj_mat, view_mat, near_plane) = match &scene.camera {
        Camera3D::Perspective {
            fov_horizontal_deg,
            near,
            far,
            position,
            target,
        } => {
            // The scene camera declares a horizontal field of view; the
            // projection matrix takes a vertical one.
            let fov_x = fov_horizontal_deg.to_radians();
            let fov_y = 2.0 * ((fov_x * 0.5).tan() / aspect).atan();
            let p = Mat4::perspective(fov_y, aspect, *near, *far);
            let eye = Vec3::new(position[0], position[1], position[2]);
            let tgt = Vec3::new(target[0], target[1], target[2]);
            let v = Mat4::look_at(eye, tgt, camera_up_hint(scene, eye, tgt));
            (p, v, *near)
        }
        Camera3D::Orthographic {
            left,
            right,
            top,
            bottom,
            near,
            far,
            position,
            target,
        } => {
            let p = Mat4::orthographic(*left, *right, *bottom, *top, *near, *far);
            let eye = Vec3::new(position[0], position[1], position[2]);
            let tgt = Vec3::new(target[0], target[1], target[2]);
            let v = Mat4::look_at(eye, tgt, camera_up_hint(scene, eye, tgt));
            (p, v, *near)
        }
    };

    let projection = proj_mat;

    // 2. Collect Scene Lights
    let mut ambient_intensity = 0.2f32;
    let mut ambient_color = Color::WHITE;
    let mut dir_light_dir = Vec3::new(0.5, 1.0, 0.7).normalize();
    let mut dir_light_color = Color::WHITE;
    let mut dir_light_intensity = 0.8f32;

    for light in &scene.lights {
        match light {
            Light3D::Ambient { color, intensity } => {
                ambient_color = *color;
                ambient_intensity = *intensity;
            }
            Light3D::Directional {
                position,
                color,
                intensity,
                ..
            } => {
                dir_light_dir = Vec3::new(position[0], position[1], position[2]).normalize();
                dir_light_color = *color;
                dir_light_intensity = *intensity;
            }
            Light3D::Point { .. } => {}
        }
    }

    // 3. Viewport Z-Buffer
    let vp_w = viewport.width.ceil() as usize;
    let vp_h = viewport.height.ceil() as usize;
    let background_filled = !matches!(scene.background(), Background3D::Transparent);
    let mut depth = RealDepthBuffer {
        rows: vec![1.0f32; vp_w * vp_h],
    };

    // 4. Prepare draws and project every visible triangle. Geometry is
    // collected once, then rasterized in screen-space rows: projection is
    // cheap, and the pixel loop is what dominates a full-screen terrain frame.
    let mut prepared: Vec<PreparedDraw> = Vec::new();
    let mut triangles: Vec<ScreenTriangle> = Vec::new();

    for mesh in &scene.meshes {
        let buffers = match &mesh.geometry {
            Geometry3D::Box {
                width,
                height,
                depth,
            } => generate_box(*width, *height, *depth),
            Geometry3D::Sphere {
                radius,
                width_segments,
                height_segments,
            } => generate_sphere(*radius, *width_segments, *height_segments),
            Geometry3D::Plane { width, height } => generate_plane(*width, *height, 1, 1),
            Geometry3D::Cylinder {
                radius_top,
                radius_bottom,
                height,
                radial_segments,
            } => generate_cylinder(*radius_top, *radius_bottom, *height, *radial_segments),
            Geometry3D::Custom { vertices, indices } => generate_custom(vertices, indices),
            Geometry3D::CustomTextured { vertices, indices } => {
                generate_custom_textured(vertices, indices)
            }
        };

        let t = Vec3::new(
            mesh.transform.position[0],
            mesh.transform.position[1],
            mesh.transform.position[2],
        );
        let r = Vec3::new(
            mesh.transform.rotation[0],
            mesh.transform.rotation[1],
            mesh.transform.rotation[2],
        );
        let scale = Vec3::new(
            mesh.transform.scale[0],
            mesh.transform.scale[1],
            mesh.transform.scale[2],
        );

        let model_mat = Mat4::from_trs(t, r, scale);
        let normal_mat = model_mat.inverse().unwrap_or(Mat4::IDENTITY).transpose();
        let view_model = view_mat.multiply(&model_mat);

        let base_color = match &mesh.material {
            Material3D::Standard { color, .. } => *color,
            Material3D::Physical { color, .. } => *color,
            Material3D::Basic { color } => *color,
            Material3D::Shader { .. } => Color::rgba(180, 180, 220, 255),
            Material3D::Textured(material) => material.tint,
            Material3D::TexturedBatch(_) => Color::rgba(255, 255, 255, 255),
        };

        // One draw for an untextured or single-texture mesh, one per index
        // range for a batch.
        struct DrawRange<'a> {
            material: Option<&'a TexturedMaterial3D>,
            start: usize,
            end: usize,
            color: Color,
        }
        let mut draw_ranges: Vec<DrawRange> = Vec::new();
        match &mesh.material {
            Material3D::TexturedBatch(batch) => {
                for draw in &batch.draws {
                    let start = draw.index_start as usize;
                    let end = (start + draw.index_count as usize).min(buffers.indices.len());
                    draw_ranges.push(DrawRange {
                        material: Some(&draw.material),
                        start,
                        end,
                        color: draw.material.tint,
                    });
                }
            }
            Material3D::Textured(material) => draw_ranges.push(DrawRange {
                material: Some(material),
                start: 0,
                end: buffers.indices.len(),
                color: material.tint,
            }),
            _ => draw_ranges.push(DrawRange {
                material: None,
                start: 0,
                end: buffers.indices.len(),
                color: base_color,
            }),
        }

        // Transform every vertex into view space first: that is where the
        // near-plane clipping runs, so `w` never has to be trusted here.
        let mut view_vertices = Vec::with_capacity(buffers.vertices.len());
        for v in &buffers.vertices {
            let position = view_model.transform_point(v.position);
            let norm = normal_mat.transform_vector(v.normal).normalize();
            view_vertices.push(ClipVertex {
                position,
                uv: v.uv,
                world_normal: norm,
            });
        }

        for draw in &draw_ranges {
            if draw.start >= draw.end {
                continue;
            }
            let draw_index = prepared.len();
            prepared.push(PreparedDraw::new(draw.material, textures, draw.color));
            for tri in buffers.indices[draw.start..draw.end].chunks_exact(3) {
                let polygon = [
                    view_vertices[tri[0] as usize],
                    view_vertices[tri[1] as usize],
                    view_vertices[tri[2] as usize],
                ];
                let clipped = clip_polygon_near(&polygon, near_plane.max(1e-3));
                if clipped.len() < 3 {
                    continue;
                }
                for fan in 1..clipped.len() - 1 {
                    project_screen_triangle(
                        &clipped[0],
                        &clipped[fan],
                        &clipped[fan + 1],
                        &projection,
                        viewport,
                        draw_index,
                        &mut triangles,
                    );
                }
            }
        }
    }

    rasterize_geometry_parallel(
        frame_buffer,
        viewport,
        clip,
        &mut depth,
        background_filled,
        &triangles,
        &prepared,
        ambient_color,
        ambient_intensity,
        dir_light_color,
        dir_light_dir,
        dir_light_intensity,
        vp_w,
        vp_h,
    );

    // 4.5. Cloud shells pass: translucent atmospheric shells drawn over geometry
    // when coverage and opacity are nonzero.
    for shell in scene.cloud_shells() {
        if shell.effective_opacity() > 0.0 {
            render_cloud_shell(
                frame_buffer,
                viewport,
                clip,
                &depth.rows,
                scene,
                shell,
            );
        }
    }

    // 5. Overlay pass: 2D primitives projected with the same camera basis,
    // drawn after depth-tested geometry.
    if let Some(overlay) = scene.overlay() {
        if !overlay.is_empty() {
            let frame = overlay_frame(scene, viewport);
            if let Some(rasterizer) = crate::scene3d::overlay_text_rasterizer() {
                overlay.render_with(frame_buffer, viewport, &frame, rasterizer);
            } else {
                overlay.render_geometry_only(frame_buffer, viewport, &frame);
            }
        }
    }
}

/// A vertex with everything needed to rasterize and clip a triangle.
///
/// Positions are view space, relative to the camera eye: that is the small
/// coordinate space where f32 still has sub-metre detail at Earth scale, and it
/// is what keeps near-plane clipping numerically stable.
#[derive(Clone, Copy)]
struct ClipVertex {
    position: Vec3,
    uv: (f32, f32),
    world_normal: Vec3,
}

impl ClipVertex {
    fn lerp(&self, other: &Self, t: f32) -> Self {
        let mix = |a: f32, b: f32| a + (b - a) * t;
        Self {
            position: Vec3::new(
                mix(self.position.x, other.position.x),
                mix(self.position.y, other.position.y),
                mix(self.position.z, other.position.z),
            ),
            uv: (mix(self.uv.0, other.uv.0), mix(self.uv.1, other.uv.1)),
            world_normal: self.world_normal * (1.0 - t) + other.world_normal * t,
        }
    }
}

/// Clips a view-space polygon against the near plane `-z >= near`.
///
/// The common case is taken first: a polygon fully in front is returned as is,
/// and a polygon fully behind is dropped. Only a polygon that straddles the
/// plane is interpolated, so any numerical slip in the crossing test cannot
/// shrink geometry that is fully in front of the camera.
fn clip_polygon_near(polygon: &[ClipVertex], near: f32) -> Vec<ClipVertex> {
    if polygon.is_empty() {
        return Vec::new();
    }
    // An entirely front-facing polygon needs no interpolation at all.
    if polygon.iter().all(|v| -v.position.z >= near) {
        return polygon.to_vec();
    }

    let mut out: Vec<ClipVertex> = Vec::with_capacity(polygon.len() + 2);
    for index in 0..polygon.len() {
        let current = &polygon[index];
        let next = &polygon[(index + 1) % polygon.len()];
        let current_in = -current.position.z >= near;
        let next_in = -next.position.z >= near;
        if current_in {
            out.push(*current);
        }
        if current_in != next_in {
            let denominator = current.position.z - next.position.z;
            if denominator.abs() > 1e-12 {
                let t = (-near - current.position.z) / denominator;
                out.push(current.lerp(next, t.clamp(0.0, 1.0)));
            }
        }
    }
    out
}

/// Uses the scene's declared up vector when it is usable, otherwise falls back
/// to an axis that is not parallel to the view direction.
fn camera_up_hint(scene: &Scene3D, eye: Vec3, target: Vec3) -> Vec3 {
    let forward = (target - eye).normalize();
    if let Some(up) = scene.camera_up() {
        let hint = Vec3::new(up[0] as f32, up[1] as f32, up[2] as f32);
        if hint.length() > 1e-6 && forward.cross(hint).length() > 1e-4 {
            return hint;
        }
    }
    scene_up(eye, target)
}

/// Picks an up hint that is not parallel to the view direction.
///
/// A straight-down view has no unique right vector from the default world up,
/// so the hint falls back to a second axis; without this, the view basis
/// degenerates and the near plane sits at the world origin instead of the eye.
fn scene_up(eye: Vec3, target: Vec3) -> Vec3 {
    let forward = (target - eye).normalize();
    if forward.cross(Vec3::UP).length() > 1e-4 {
        Vec3::UP
    } else if forward.cross(Vec3::new(0.0, 0.0, 1.0)).length() > 1e-4 {
        Vec3::new(0.0, 0.0, 1.0)
    } else {
        Vec3::new(1.0, 0.0, 0.0)
    }
}

/// Depth buffer for one frame.
struct RealDepthBuffer {
    rows: Vec<f32>,
}

/// Writes a fill colour into the viewport without alpha blending.
fn fill_viewport(frame_buffer: &mut FrameBuffer, viewport: Rect, colour: impl Fn(usize) -> Color) {
    let width = frame_buffer.width;
    let x0 = viewport.x.max(0.0) as usize;
    let y0 = viewport.y.max(0.0) as usize;
    let x1 = (viewport.max_x().max(0.0) as usize).min(width);
    let y1 = (viewport.max_y().max(0.0) as usize).min(frame_buffer.height);
    for y in y0..y1 {
        let color = colour(y - y0);
        let (r, g, b, a) = color.to_rgba8();
        let row_start = (y * width + x0) * 4;
        let row_end = (y * width + x1) * 4;
        for chunk in frame_buffer.pixels[row_start..row_end].chunks_exact_mut(4) {
            chunk[0] = r;
            chunk[1] = g;
            chunk[2] = b;
            chunk[3] = a;
        }
    }
}

/// A projected vertex with screen-space attributes that interpolate linearly.
///
/// `u_over_w` and `v_over_w` are the perspective-correct quantities: they are
/// affine across a triangle in screen space, unlike raw UVs.
#[derive(Clone, Copy)]
struct ScreenVertex {
    x: f32,
    y: f32,
    z: f32,
    inv_w: f32,
    u_over_w: f32,
    v_over_w: f32,
    world_normal: Vec3,
}

impl ScreenVertex {
    fn lerp(&self, other: &Self, t: f32) -> Self {
        let mix = |a: f32, b: f32| a + (b - a) * t;
        Self {
            x: mix(self.x, other.x),
            y: mix(self.y, other.y),
            z: mix(self.z, other.z),
            inv_w: mix(self.inv_w, other.inv_w),
            u_over_w: mix(self.u_over_w, other.u_over_w),
            v_over_w: mix(self.v_over_w, other.v_over_w),
            world_normal: self.world_normal * (1.0 - t) + other.world_normal * t,
        }
    }
}

/// One triangle after near-plane and screen-space clipping.
struct ScreenTriangle {
    vertices: [ScreenVertex; 3],
    draw: usize,
}

/// Per-draw state hoisted out of the pixel loop.
struct PreparedDraw<'a> {
    material: Option<&'a TexturedMaterial3D>,
    texture: Option<&'a crate::texture3d::Texture>,
    fast_bilinear: bool,
    base: [f32; 4],
    uv_scale: [f32; 2],
    uv_offset: [f32; 2],
    opacity: f32,
}

impl<'a> PreparedDraw<'a> {
    fn new(
        material: Option<&'a TexturedMaterial3D>,
        textures: Option<&'a TextureStore>,
        color: Color,
    ) -> Self {
        let texture =
            material.and_then(|material| textures.and_then(|store| store.get(material.texture)));
        let fast_bilinear = texture
            .map(|texture| {
                texture.sampler.filter == FilterMode::Bilinear
                    && texture.sampler.address_u == AddressMode::ClampToEdge
                    && texture.sampler.address_v == AddressMode::ClampToEdge
            })
            .unwrap_or(false);
        let (uv_scale, uv_offset, opacity) = match material {
            Some(material) => (
                material.uv_transform.scale,
                material.uv_transform.offset,
                material.opacity,
            ),
            None => ([1.0, 1.0], [0.0, 0.0], 1.0),
        };
        Self {
            material,
            texture,
            fast_bilinear,
            base: [color.r(), color.g(), color.b(), color.a()],
            uv_scale,
            uv_offset,
            opacity,
        }
    }
}

fn project_view_vertex(
    vertex: &ClipVertex,
    projection: &Mat4,
    viewport: Rect,
) -> Option<ScreenVertex> {
    let clip = projection.transform_vec4(Vec4::from_vec3(vertex.position, 1.0));
    if clip.w <= 1e-6 {
        return None;
    }
    let inv_w = 1.0 / clip.w;
    Some(ScreenVertex {
        x: (clip.x * inv_w + 1.0) * 0.5 * viewport.width,
        y: (1.0 - clip.y * inv_w) * 0.5 * viewport.height,
        z: clip.z * inv_w,
        inv_w,
        u_over_w: vertex.uv.0 * inv_w,
        v_over_w: vertex.uv.1 * inv_w,
        world_normal: vertex.world_normal,
    })
}

fn clip_screen_axis(
    polygon: &[ScreenVertex],
    horizontal: bool,
    bound: f32,
    keep_greater: bool,
) -> Vec<ScreenVertex> {
    if polygon.is_empty() {
        return Vec::new();
    }
    let distance = |v: &ScreenVertex| {
        let value = if horizontal { v.x } else { v.y };
        if keep_greater {
            value - bound
        } else {
            bound - value
        }
    };
    if polygon.iter().all(|v| distance(v) >= 0.0) {
        return polygon.to_vec();
    }
    let mut out = Vec::with_capacity(polygon.len() + 2);
    for index in 0..polygon.len() {
        let current = &polygon[index];
        let next = &polygon[(index + 1) % polygon.len()];
        let d_current = distance(current);
        let d_next = distance(next);
        if d_current >= 0.0 {
            out.push(*current);
        }
        if (d_current >= 0.0) != (d_next >= 0.0) {
            let denominator = d_current - d_next;
            if denominator.abs() > 1e-12 {
                let t = (d_current / denominator).clamp(0.0, 1.0);
                out.push(current.lerp(next, t));
            }
        }
    }
    out
}

/// Projects a view-space triangle, clips it to the viewport, and appends the
/// resulting fan. Clipping in screen space keeps the raster bounding boxes
/// bounded; a triangle whose corners are far outside the frame otherwise has a
/// box covering the whole frame while covering only a sliver.
fn project_screen_triangle(
    v0: &ClipVertex,
    v1: &ClipVertex,
    v2: &ClipVertex,
    projection: &Mat4,
    viewport: Rect,
    draw: usize,
    out: &mut Vec<ScreenTriangle>,
) {
    let (Some(s0), Some(s1), Some(s2)) = (
        project_view_vertex(v0, projection, viewport),
        project_view_vertex(v1, projection, viewport),
        project_view_vertex(v2, projection, viewport),
    ) else {
        return;
    };

    let w = viewport.width;
    let h = viewport.height;
    let mut polygon = vec![s0, s1, s2];
    for (horizontal, bound, greater) in [
        (true, 0.0, true),
        (true, w, false),
        (false, 0.0, true),
        (false, h, false),
    ] {
        polygon = clip_screen_axis(&polygon, horizontal, bound, greater);
        if polygon.len() < 3 {
            return;
        }
    }

    for fan in 1..polygon.len() - 1 {
        out.push(ScreenTriangle {
            vertices: [polygon[0], polygon[fan], polygon[fan + 1]],
            draw,
        });
    }
}

/// Rasterizes every prepared triangle, splitting the viewport into horizontal
/// bands so all available cores work on disjoint rows.
#[allow(clippy::too_many_arguments)]
fn rasterize_geometry_parallel(
    frame_buffer: &mut FrameBuffer,
    viewport: Rect,
    clip: Option<Rect>,
    depth: &mut RealDepthBuffer,
    translucent_over_background: bool,
    triangles: &[ScreenTriangle],
    draws: &[PreparedDraw],
    ambient_color: Color,
    ambient_intensity: f32,
    dir_light_color: Color,
    dir_light_dir: Vec3,
    dir_light_intensity: f32,
    vp_w: usize,
    vp_h: usize,
) {
    if triangles.is_empty() {
        return;
    }
    let full_width = frame_buffer.width;
    let y_origin = viewport.y.max(0.0) as usize;
    // Bands cover framebuffer rows `y_origin .. y_origin + rows`. The pixel
    // slice must start at the viewport origin, not at row zero, or a panel
    // inset in the window paints shifted upward.
    let available_rows = frame_buffer.height.saturating_sub(y_origin).min(vp_h);
    if available_rows == 0 || vp_w == 0 {
        return;
    }
    let row_bytes = full_width * 4;
    let pixel_start = (y_origin * row_bytes).min(frame_buffer.pixels.len());

    // Spawning threads costs more than it saves for a small preview widget, so
    // tiny viewports and tiny triangle lists stay serial.
    const PARALLEL_AREA: usize = 8_192;
    let area = vp_w.saturating_mul(available_rows);
    let max_threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .clamp(1, 16);
    let threads = if area < PARALLEL_AREA { 1 } else { max_threads };
    let band_rows = available_rows.div_ceil(threads).max(1);

    let pixels = &mut frame_buffer.pixels[pixel_start..];
    let depth_rows = &mut depth.rows[..available_rows * vp_w];
    std::thread::scope(|scope| {
        let mut pixel_rest = pixels;
        let mut depth_rest = depth_rows;
        let mut py_start = 0usize;
        while py_start < available_rows {
            // The last band is clamped so the pixel and depth slices always
            // describe the same rows.
            let rows = band_rows.min(available_rows - py_start);
            let pixel_split = (rows * row_bytes).min(pixel_rest.len());
            let depth_split = (rows * vp_w).min(depth_rest.len());
            let (pixel_band, pixel_tail) = pixel_rest.split_at_mut(pixel_split);
            let (depth_band, depth_tail) = depth_rest.split_at_mut(depth_split);
            pixel_rest = pixel_tail;
            depth_rest = depth_tail;

            if threads == 1 {
                rasterize_band(
                    pixel_band,
                    full_width,
                    rows,
                    depth_band,
                    viewport,
                    clip,
                    translucent_over_background,
                    triangles,
                    draws,
                    ambient_color,
                    ambient_intensity,
                    dir_light_color,
                    dir_light_dir,
                    dir_light_intensity,
                    vp_w,
                    py_start,
                );
            } else {
                scope.spawn(move || {
                    rasterize_band(
                        pixel_band,
                        full_width,
                        rows,
                        depth_band,
                        viewport,
                        clip,
                        translucent_over_background,
                        triangles,
                        draws,
                        ambient_color,
                        ambient_intensity,
                        dir_light_color,
                        dir_light_dir,
                        dir_light_intensity,
                        vp_w,
                        py_start,
                    );
                });
            }
            py_start += rows;
        }
    });
}

/// Rasterizes the triangles that intersect one row band.
#[allow(clippy::too_many_arguments)]
fn rasterize_band(
    pixels: &mut [u8],
    full_width: usize,
    rows: usize,
    depth: &mut [f32],
    viewport: Rect,
    clip: Option<Rect>,
    translucent_over_background: bool,
    triangles: &[ScreenTriangle],
    draws: &[PreparedDraw],
    ambient_color: Color,
    ambient_intensity: f32,
    dir_light_color: Color,
    dir_light_dir: Vec3,
    dir_light_intensity: f32,
    vp_w: usize,
    py_start: usize,
) {
    let band_end = py_start + rows;
    for triangle in triangles {
        let bbox_min_y = triangle
            .vertices
            .iter()
            .map(|v| v.y)
            .fold(f32::INFINITY, f32::min)
            .floor() as isize;
        let bbox_max_y = triangle
            .vertices
            .iter()
            .map(|v| v.y)
            .fold(f32::NEG_INFINITY, f32::max)
            .ceil() as isize;
        if bbox_max_y < py_start as isize || bbox_min_y >= band_end as isize {
            continue;
        }
        rasterize_screen_triangle(
            pixels,
            full_width,
            py_start,
            rows,
            depth,
            viewport,
            clip,
            translucent_over_background,
            &triangle.vertices[0],
            &triangle.vertices[1],
            &triangle.vertices[2],
            &draws[triangle.draw],
            ambient_color,
            ambient_intensity,
            dir_light_color,
            dir_light_dir,
            dir_light_intensity,
            vp_w,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn rasterize_screen_triangle(
    pixels: &mut [u8],
    full_width: usize,
    py_start: usize,
    rows: usize,
    depth: &mut [f32],
    viewport: Rect,
    clip: Option<Rect>,
    translucent_over_background: bool,
    v0: &ScreenVertex,
    v1: &ScreenVertex,
    v2: &ScreenVertex,
    draw: &PreparedDraw,
    ambient_color: Color,
    ambient_intensity: f32,
    dir_light_color: Color,
    dir_light_dir: Vec3,
    dir_light_intensity: f32,
    vp_w: usize,
) {
    let (x0, y0, z0, inv0) = (v0.x, v0.y, v0.z, v0.inv_w);
    let (x1, y1, z1, inv1) = (v1.x, v1.y, v1.z, v1.inv_w);
    let (x2, y2, z2, inv2) = (v2.x, v2.y, v2.z, v2.inv_w);

    let min_x = (x0.min(x1).min(x2).floor() as isize)
        .max(0)
        .min(vp_w as isize) as usize;
    let max_x = ((x0.max(x1).max(x2).ceil() as isize) + 1)
        .max(0)
        .min(vp_w as isize) as usize;
    let min_y = (y0.min(y1).min(y2).floor() as isize)
        .max(py_start as isize)
        .min((py_start + rows) as isize) as usize;
    let max_y = ((y0.max(y1).max(y2).ceil() as isize) + 1)
        .max(py_start as isize)
        .min((py_start + rows) as isize) as usize;

    let denom = (y1 - y2) * (x0 - x2) + (x2 - x1) * (y0 - y2);
    if denom.abs() < 1e-12 {
        return;
    }
    let inv_denom = 1.0 / denom;

    let (u0, v0_uv) = (v0.u_over_w, v0.v_over_w);
    let (u1, v1_uv) = (v1.u_over_w, v1.v_over_w);
    let (u2, v2_uv) = (v2.u_over_w, v2.v_over_w);

    let needs_light = match draw.material {
        Some(material) => material.lit,
        None => true,
    };
    let (base_r, base_g, base_b, base_a) = (draw.base[0], draw.base[1], draw.base[2], draw.base[3]);

    let dw0_dx = (y1 - y2) * inv_denom;
    let dw1_dx = (y2 - y0) * inv_denom;
    let dz_dx = dw0_dx * (z0 - z2) + dw1_dx * (z1 - z2);

    for py in min_y..max_y {
        let fy = py as f32 + 0.5;
        let fx = min_x as f32 + 0.5;
        let mut w0 = ((y1 - y2) * (fx - x2) + (x2 - x1) * (fy - y2)) * inv_denom;
        let mut w1 = ((y2 - y0) * (fx - x2) + (x0 - x2) * (fy - y2)) * inv_denom;
        let mut interp_depth = w0 * z0 + w1 * z1 + (1.0 - w0 - w1) * z2;
        let depth_row = (py - py_start) * vp_w;
        let pixel_row = (py - py_start) * full_width;

        for px in min_x..max_x {
            let w2 = 1.0 - w0 - w1;
            if w0 < -1e-6 || w1 < -1e-6 || w2 < -1e-6 {
                w0 += dw0_dx;
                w1 += dw1_dx;
                interp_depth += dz_dx;
                continue;
            }

            let depth_idx = depth_row + px;
            let (r_factor, g_factor, b_factor) = if needs_light {
                let norm = (v0.world_normal * w0 + v1.world_normal * w1 + v2.world_normal * w2)
                    .normalize();
                let n_dot_l = norm.dot(dir_light_dir).max(0.0);
                let diffuse = dir_light_intensity * n_dot_l;
                (
                    (ambient_color.r() * ambient_intensity + dir_light_color.r() * diffuse)
                        .min(1.0),
                    (ambient_color.g() * ambient_intensity + dir_light_color.g() * diffuse)
                        .min(1.0),
                    (ambient_color.b() * ambient_intensity + dir_light_color.b() * diffuse)
                        .min(1.0),
                )
            } else {
                (1.0, 1.0, 1.0)
            };

            let (r, g, b, a) = match draw.material {
                Some(_) => {
                    let sampled = match draw.texture {
                        Some(texture) => {
                            let inv = w0 * inv0 + w1 * inv1 + w2 * inv2;
                            let uv = if inv.abs() > 1e-12 {
                                (
                                    (w0 * u0 + w1 * u1 + w2 * u2) / inv,
                                    (w0 * v0_uv + w1 * v1_uv + w2 * v2_uv) / inv,
                                )
                            } else {
                                (u0, v0_uv)
                            };
                            let u = uv.0 * draw.uv_scale[0] + draw.uv_offset[0];
                            let v = uv.1 * draw.uv_scale[1] + draw.uv_offset[1];
                            if draw.fast_bilinear {
                                sample_bilinear_clamped(texture, u, v)
                            } else {
                                sample_texture(texture, (u, v))
                            }
                        }
                        None => [255, 255, 255, 255],
                    };

                    let sr = sampled[0] as f32 / 255.0 * base_r;
                    let sg = sampled[1] as f32 / 255.0 * base_g;
                    let sb = sampled[2] as f32 / 255.0 * base_b;
                    let sa = sampled[3] as f32 / 255.0 * draw.opacity;

                    let lit = draw.material.map(|m| m.lit).unwrap_or(false);
                    let (lr, lg, lb) = if lit {
                        (r_factor, g_factor, b_factor)
                    } else {
                        (1.0, 1.0, 1.0)
                    };

                    (
                        ((sr * lr) * 255.0).min(255.0) as u8,
                        ((sg * lg) * 255.0).min(255.0) as u8,
                        ((sb * lb) * 255.0).min(255.0) as u8,
                        ((sa * base_a * 255.0) + 0.5).min(255.0) as u8,
                    )
                }
                None => {
                    let r = ((base_r * r_factor) * 255.0).min(255.0) as u8;
                    let g = ((base_g * g_factor) * 255.0).min(255.0) as u8;
                    let b = ((base_b * b_factor) * 255.0).min(255.0) as u8;
                    (r, g, b, (base_a * 255.0).round() as u8)
                }
            };

            let global_x = viewport.x as usize + px;
            if global_x >= full_width {
                break;
            }
            let global_y = viewport.y as usize + py;

            let inside_clip = match clip {
                Some(c) => {
                    let gx = global_x as f32;
                    let gy = global_y as f32;
                    gx >= c.x && gx < c.max_x() && gy >= c.y && gy < c.max_y()
                }
                None => true,
            };
            if !inside_clip {
                w0 += dw0_dx;
                w1 += dw1_dx;
                interp_depth += dz_dx;
                continue;
            }

            let pixel_index = pixel_row + global_x;
            if a == 255 {
                if depth[depth_idx] > interp_depth && interp_depth >= 0.0 {
                    depth[depth_idx] = interp_depth;
                    write_opaque_band(pixels, pixel_index, r, g, b);
                }
            } else if a > 0 {
                if translucent_over_background && interp_depth < depth[depth_idx] {
                    depth[depth_idx] = depth[depth_idx].min(interp_depth);
                }
                blend_band(pixels, pixel_index, r, g, b, a);
            }

            w0 += dw0_dx;
            w1 += dw1_dx;
            interp_depth += dz_dx;
        }
    }
}

#[inline(always)]
fn write_opaque_band(pixels: &mut [u8], index: usize, r: u8, g: u8, b: u8) {
    if index * 4 + 3 >= pixels.len() {
        return;
    }
    pixels[index * 4] = r;
    pixels[index * 4 + 1] = g;
    pixels[index * 4 + 2] = b;
    pixels[index * 4 + 3] = 255;
}

/// Porter-Duff over into a band slice.
#[inline]
fn blend_band(pixels: &mut [u8], index: usize, r: u8, g: u8, b: u8, a: u8) {
    if index * 4 + 3 >= pixels.len() {
        return;
    }
    let src_a = a as f32 / 255.0;
    let dst = index * 4;
    let dst_a = pixels[dst + 3] as f32 / 255.0;
    let out_a = src_a + dst_a * (1.0 - src_a);
    if out_a <= 1e-5 {
        return;
    }
    for (offset, src) in [(0, r), (1, g), (2, b)] {
        let src_f = src as f32 / 255.0;
        let dst_f = pixels[dst + offset] as f32 / 255.0;
        let out = (src_f * src_a + dst_f * dst_a * (1.0 - src_a)) / out_a;
        pixels[dst + offset] = (out * 255.0).round() as u8;
    }
    pixels[dst + 3] = (out_a * 255.0).round() as u8;
}

/// Builds the f64 overlay frame that shares the scene camera's eye, target, and
/// field of view, so overlay anchors land on the geometry they annotate.
///
/// This is the same frame the overlay pass rasterizes with; callers that need
/// to project their own anchors should use it rather than rebuilding a camera.
pub fn overlay_frame(scene: &Scene3D, viewport: Rect) -> crate::camera64::RenderFrame64 {
    use crate::camera64::{Camera64, DVec3, RenderFrame64};
    let camera = match &scene.camera {
        Camera3D::Perspective {
            fov_horizontal_deg,
            near,
            far,
            position,
            target,
        } => Camera64 {
            eye: DVec3::new(position[0] as f64, position[1] as f64, position[2] as f64),
            target: DVec3::new(target[0] as f64, target[1] as f64, target[2] as f64),
            up: scene
                .camera_up()
                .map(|u| DVec3::new(u[0], u[1], u[2]))
                .unwrap_or(DVec3::UP),
            fov_horizontal_deg: *fov_horizontal_deg as f64,
            near: *near as f64,
            far: *far as f64,
        },
        Camera3D::Orthographic {
            near,
            far,
            position,
            target,
            ..
        } => Camera64 {
            eye: DVec3::new(position[0] as f64, position[1] as f64, position[2] as f64),
            target: DVec3::new(target[0] as f64, target[1] as f64, target[2] as f64),
            up: scene
                .camera_up()
                .map(|u| DVec3::new(u[0], u[1], u[2]))
                .unwrap_or(DVec3::UP),
            fov_horizontal_deg: 60.0,
            near: *near as f64,
            far: *far as f64,
        },
    };
    RenderFrame64::from_camera(&camera, viewport.width as f64, viewport.height as f64)
}

fn render_cloud_shell(
    frame_buffer: &mut FrameBuffer,
    viewport: Rect,
    clip: Option<Rect>,
    depth_buffer: &[f32],
    scene: &Scene3D,
    shell: &crate::scene3d::CloudShell3D,
) {
    let opacity = shell.effective_opacity();
    if opacity <= 0.0 {
        return;
    }

    let vp_x = viewport.x.max(0.0) as usize;
    let vp_y = viewport.y.max(0.0) as usize;
    let vp_w = viewport.width.ceil().max(1.0) as usize;
    let vp_h = viewport.height.ceil().max(1.0) as usize;

    let (clip_min_x, clip_max_x, clip_min_y, clip_max_y) = match clip {
        Some(c) => (
            c.x.max(viewport.x).floor() as usize,
            (c.x + c.width).min(viewport.x + viewport.width).ceil() as usize,
            c.y.max(viewport.y).floor() as usize,
            (c.y + c.height).min(viewport.y + viewport.height).ceil() as usize,
        ),
        None => (
            vp_x,
            (viewport.x + viewport.width).ceil() as usize,
            vp_y,
            (viewport.y + viewport.height).ceil() as usize,
        ),
    };

    let (camera_eye, camera_target, near_plane, far_plane, fov_y_rad) = match &scene.camera {
        crate::scene3d::Camera3D::Perspective {
            position,
            target,
            near,
            far,
            fov_horizontal_deg,
        } => {
            let eye = crate::camera64::DVec3::new(position[0] as f64, position[1] as f64, position[2] as f64);
            let tgt = crate::camera64::DVec3::new(target[0] as f64, target[1] as f64, target[2] as f64);
            let aspect = vp_w as f64 / vp_h.max(1) as f64;
            let fov_x = fov_horizontal_deg.to_radians() as f64;
            let fov_y = 2.0 * ((fov_x * 0.5).tan() / aspect).atan();
            (eye, tgt, *near as f64, *far as f64, fov_y)
        }
        _ => return,
    };

    let forward = camera_target.sub(camera_eye).normalize();
    let cam_up = scene
        .camera_up()
        .map(|u| crate::camera64::DVec3::new(u[0], u[1], u[2]))
        .unwrap_or(crate::camera64::DVec3::UP);
    let up_hint = if forward.cross(cam_up).length() < 1e-6 {
        if forward.cross(crate::camera64::DVec3::new(0.0, 0.0, 1.0)).length() > 1e-6 {
            crate::camera64::DVec3::new(0.0, 0.0, 1.0)
        } else {
            crate::camera64::DVec3::new(0.0, 1.0, 0.0)
        }
    } else {
        cam_up
    };
    let right = forward.cross(up_hint).normalize();
    let up = right.cross(forward).normalize();

    let aspect = vp_w as f64 / vp_h.max(1) as f64;
    let tan_half_y = (fov_y_rad * 0.5).tan();
    let tan_half_x = tan_half_y * aspect;

    let sphere_center = crate::camera64::DVec3::new(shell.centre[0], shell.centre[1], shell.centre[2]);
    let sphere_radius = shell.radius;
    let sphere_radius_sq = sphere_radius * sphere_radius;

    let shell_color = shell.color;
    let base_alpha = opacity * shell_color.a();

    for y in clip_min_y..clip_max_y {
        if y >= frame_buffer.height {
            break;
        }
        let row_ndc_y = 1.0 - ((y as f64 - vp_y as f64 + 0.5) / (vp_h as f64)) * 2.0;
        let depth_row_offset = (y - vp_y) * vp_w;

        for x in clip_min_x..clip_max_x {
            if x >= frame_buffer.width {
                break;
            }
            let col_ndc_x = ((x as f64 - vp_x as f64 + 0.5) / (vp_w as f64)) * 2.0 - 1.0;

            let ray_dir = right
                .scale(col_ndc_x * tan_half_x)
                .add(up.scale(row_ndc_y * tan_half_y))
                .add(forward)
                .normalize();

            let p0 = camera_eye.sub(sphere_center);
            let b = 2.0 * p0.dot(ray_dir);
            let c = p0.dot(p0) - sphere_radius_sq;
            let discriminant = b * b - 4.0 * c;

            if discriminant < 0.0 {
                continue;
            }

            let sqrt_disc = discriminant.sqrt();
            let t1 = (-b - sqrt_disc) * 0.5;
            let t2 = (-b + sqrt_disc) * 0.5;

            let t = if t1 >= near_plane && t1 <= far_plane {
                t1
            } else if t2 >= near_plane && t2 <= far_plane {
                t2
            } else {
                continue;
            };

            let pixel_sub_x = x - vp_x;
            if pixel_sub_x < vp_w {
                let depth_idx = depth_row_offset + pixel_sub_x;
                if depth_idx < depth_buffer.len() {
                    let d = depth_buffer[depth_idx];
                    let view_z = ray_dir.scale(t).dot(forward);
                    let range = far_plane - near_plane;
                    let cloud_ndc_depth = if range.abs() > 1e-6 {
                        ((far_plane / range) * (1.0 - near_plane / view_z.max(near_plane))) as f32
                    } else {
                        0.5
                    };
                    if d < cloud_ndc_depth {
                        continue;
                    }
                }
            }

            let hit_point = camera_eye.add(ray_dir.scale(t));
            let normal = hit_point.sub(sphere_center).normalize();
            let cos_theta = (-ray_dir.dot(normal)).abs().clamp(0.0, 1.0);
            let limb_factor = (1.0 - cos_theta * 0.4).clamp(0.6, 1.4);
            let pixel_alpha = (base_alpha * limb_factor as f32).clamp(0.0, 1.0);

            let (cr, cg, cb, _) = shell_color.to_rgba8();
            let blend_color = strata_style::types::Color::rgba(
                cr,
                cg,
                cb,
                (pixel_alpha * 255.0).round() as u8,
            );
            frame_buffer.set_pixel(x, y, blend_color);
        }
    }
}
