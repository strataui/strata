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
use crate::texture3d::{sample_texture, TextureStore};
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
            fov,
            near,
            far,
            position,
            target,
            ..
        } => {
            // The scene camera's field of view is horizontal; the projection
            // matrix takes a vertical one.
            let fov_x = fov.to_radians();
            let fov_y = 2.0 * ((fov_x * 0.5).tan() / aspect).atan();
            let p = Mat4::perspective(fov_y, aspect, *near, *far);
            let eye = Vec3::new(position[0], position[1], position[2]);
            let tgt = Vec3::new(target[0], target[1], target[2]);
            let v = Mat4::look_at(eye, tgt, scene_up(eye, tgt));
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
            let v = Mat4::look_at(eye, tgt, scene_up(eye, tgt));
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

    // 4. Render Meshes
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
        let s = Vec3::new(
            mesh.transform.scale[0],
            mesh.transform.scale[1],
            mesh.transform.scale[2],
        );

        let model_mat = Mat4::from_trs(t, r, s);
        let normal_mat = model_mat.inverse().unwrap_or(Mat4::IDENTITY).transpose();
        let view_model = view_mat.multiply(&model_mat);

        let textured: Option<&TexturedMaterial3D> = match &mesh.material {
            Material3D::Textured(material) => Some(material),
            _ => None,
        };

        let base_color = match &mesh.material {
            Material3D::Standard { color, .. } => *color,
            Material3D::Physical { color, .. } => *color,
            Material3D::Basic { color } => *color,
            Material3D::Shader { .. } => Color::rgba(180, 180, 220, 255),
            Material3D::Textured(material) => material.tint,
        };

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

        // Rasterize triangles. Triangles crossing the near plane are clipped
        // and re-fanned, so close-up geometry still covers the frame instead of
        // being dropped vertex by vertex.
        for tri in buffers.indices.chunks_exact(3) {
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
                rasterize_view_triangle(
                    frame_buffer,
                    viewport,
                    clip,
                    &projection,
                    &clipped[0],
                    &clipped[fan],
                    &clipped[fan + 1],
                    &mut depth,
                    background_filled,
                    textured,
                    textures,
                    base_color,
                    ambient_color,
                    ambient_intensity,
                    dir_light_color,
                    dir_light_dir,
                    dir_light_intensity,
                    vp_w,
                    vp_h,
                );
            }
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
            uv: (
                mix(self.uv.0, other.uv.0),
                mix(self.uv.1, other.uv.1),
            ),
            world_normal: self.world_normal * (1.0 - t) + other.world_normal * t,
        }
    }
}

/// Projects a view-space point to screen space with the scene's projection.
///
/// Returns `None` for points on or behind the near plane.
fn project_view_point(
    point: Vec3,
    projection: &Mat4,
    viewport: Rect,
) -> Option<(f32, f32, f32, f32)> {
    let clip = projection.transform_vec4(Vec4::from_vec3(point, 1.0));
    if clip.w <= 1e-6 {
        return None;
    }
    let inv_w = 1.0 / clip.w;
    let sx = (clip.x * inv_w + 1.0) * 0.5 * viewport.width;
    let sy = (1.0 - clip.y * inv_w) * 0.5 * viewport.height;
    Some((sx, sy, clip.z * inv_w, inv_w))
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

impl RealDepthBuffer {
    #[inline]
    fn test_and_write(&mut self, index: usize, depth: f32) -> bool {
        if depth >= 0.0 && depth < self.rows[index] {
            self.rows[index] = depth;
            true
        } else {
            false
        }
    }

    #[inline]
    fn slacken(&mut self, index: usize, depth: f32) {
        if self.rows[index] < depth {
            self.rows[index] = depth;
        }
    }
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

#[allow(clippy::too_many_arguments)]
fn rasterize_view_triangle(
    frame_buffer: &mut FrameBuffer,
    viewport: Rect,
    clip: Option<Rect>,
    projection: &Mat4,
    v0: &ClipVertex,
    v1: &ClipVertex,
    v2: &ClipVertex,
    depth: &mut RealDepthBuffer,
    translucent_over_background: bool,
    textured: Option<&TexturedMaterial3D>,
    textures: Option<&TextureStore>,
    base_color: Color,
    ambient_color: Color,
    ambient_intensity: f32,
    dir_light_color: Color,
    dir_light_dir: Vec3,
    dir_light_intensity: f32,
    vp_w: usize,
    vp_h: usize,
) {
    // Project the clipped view-space corners to screen space.
    let (Some((x0, y0, z0, inv0)), Some((x1, y1, z1, inv1)), Some((x2, y2, z2, inv2))) = (
        project_view_point(v0.position, projection, viewport),
        project_view_point(v1.position, projection, viewport),
        project_view_point(v2.position, projection, viewport),
    ) else {
        return;
    };

    // Triangle bounding box
    // The box is inclusive: a triangle smaller than one pixel still has to be
    // able to reach the pixel its area covers.
    let min_x = (x0.min(x1).min(x2).floor() as isize).max(0).min(vp_w as isize) as usize;
    let max_x = ((x0.max(x1).max(x2).ceil() as isize) + 1).max(0).min(vp_w as isize) as usize;
    let min_y = (y0.min(y1).min(y2).floor() as isize).max(0).min(vp_h as isize) as usize;
    let max_y = ((y0.max(y1).max(y2).ceil() as isize) + 1).max(0).min(vp_h as isize) as usize;

    let denom = (y1 - y2) * (x0 - x2) + (x2 - x1) * (y0 - y2);
    // Sub-pixel triangles at planet scale produce real area in the low bits;
    // only a genuinely degenerate triangle is skipped.
    if denom.abs() < 1e-12 {
        return;
    }
    let inv_denom = 1.0 / denom;

    let uv0 = v0.uv;
    let uv1 = v1.uv;
    let uv2 = v2.uv;

    for py in min_y..max_y {
        let fy = py as f32 + 0.5;
        for px in min_x..max_x {
            let fx = px as f32 + 0.5;

            let w0 = ((y1 - y2) * (fx - x2) + (x2 - x1) * (fy - y2)) * inv_denom;
            let w1 = ((y2 - y0) * (fx - x2) + (x0 - x2) * (fy - y2)) * inv_denom;
            let w2 = 1.0 - w0 - w1;

            // A tiny positive epsilon keeps sub-pixel triangles covering the
            // pixel centres that fall just outside their exact area.
            if w0 < -1e-6 || w1 < -1e-6 || w2 < -1e-6 {
                continue;
            }

            let interp_depth = w0 * z0 + w1 * z1 + w2 * z2;
            let depth_idx = py * vp_w + px;

            // Shade before the depth test so a translucent texel can still be
            // blended over the background, but keep the shading lazy for opaque
            // fragments that lose the test.
            let norm = (v0.world_normal * w0 + v1.world_normal * w1 + v2.world_normal * w2)
                .normalize();
            let n_dot_l = norm.dot(dir_light_dir).max(0.0);
            let diffuse = dir_light_intensity * n_dot_l;
            let r_factor =
                (ambient_color.r() * ambient_intensity + dir_light_color.r() * diffuse).min(1.0);
            let g_factor =
                (ambient_color.g() * ambient_intensity + dir_light_color.g() * diffuse).min(1.0);
            let b_factor =
                (ambient_color.b() * ambient_intensity + dir_light_color.b() * diffuse).min(1.0);

            let (r, g, b, a) = match textured {
                Some(material) => {
                    let sampled = textures
                        .and_then(|store| store.get(material.texture))
                        .map(|texture| {
                            let inv = w0 * inv0 + w1 * inv1 + w2 * inv2;
                            let uv = if inv.abs() > 1e-12 {
                                (
                                    (w0 * uv0.0 * inv0 + w1 * uv1.0 * inv1 + w2 * uv2.0 * inv2)
                                        / inv,
                                    (w0 * uv0.1 * inv0 + w1 * uv1.1 * inv1 + w2 * uv2.1 * inv2)
                                        / inv,
                                )
                            } else {
                                uv0
                            };
                            sample_texture(texture, material.uv_transform.apply(uv))
                        })
                        .unwrap_or([255, 255, 255, 255]);

                    let sr = sampled[0] as f32 / 255.0 * base_color.r();
                    let sg = sampled[1] as f32 / 255.0 * base_color.g();
                    let sb = sampled[2] as f32 / 255.0 * base_color.b();
                    let sa = sampled[3] as f32 / 255.0 * material.opacity;

                    let (lr, lg, lb) = if material.lit {
                        (r_factor, g_factor, b_factor)
                    } else {
                        (1.0, 1.0, 1.0)
                    };

                    (
                        ((sr * lr) * 255.0).min(255.0) as u8,
                        ((sg * lg) * 255.0).min(255.0) as u8,
                        ((sb * lb) * 255.0).min(255.0) as u8,
                        ((sa * base_color.a() * 255.0) + 0.5).min(255.0) as u8,
                    )
                }
                None => {
                    let r = ((base_color.r() * r_factor) * 255.0).min(255.0) as u8;
                    let g = ((base_color.g() * g_factor) * 255.0).min(255.0) as u8;
                    let b = ((base_color.b() * b_factor) * 255.0).min(255.0) as u8;
                    (r, g, b, (base_color.a() * 255.0).round() as u8)
                }
            };

            let global_x = viewport.x as usize + px;
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
                continue;
            }

            if a >= 255 {
                if depth.test_and_write(depth_idx, interp_depth) {
                    frame_buffer.set_pixel(global_x, global_y, Color::rgba(r, g, b, 255));
                }
            } else if a > 0 {
                // A translucent texel only steals geometry that is already
                // nearer than the background plane, then blends over whatever
                // is there. It never writes its own depth, so a later opaque
                // tile can still win the pixel.
                let background_plane = depth.rows[depth_idx];
                if translucent_over_background && interp_depth < background_plane {
                    depth.slacken(depth_idx, interp_depth);
                }
                frame_buffer.set_pixel(global_x, global_y, Color::rgba(r, g, b, a));
            }
        }
    }
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
            fov,
            near,
            far,
            position,
            target,
            ..
        } => Camera64 {
            eye: DVec3::new(position[0] as f64, position[1] as f64, position[2] as f64),
            target: DVec3::new(target[0] as f64, target[1] as f64, target[2] as f64),
            up: DVec3::UP,
            fov_y_deg: *fov as f64,
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
            up: DVec3::UP,
            fov_y_deg: 60.0,
            near: *near as f64,
            far: *far as f64,
        },
    };
    RenderFrame64::from_camera(&camera, viewport.width as f64, viewport.height as f64)
}
