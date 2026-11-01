//! 3D Software Rasterizer & 2D/3D Viewport Compositing Bridge.

use crate::geometry3d::{
    generate_box, generate_cylinder, generate_plane, generate_sphere, generate_custom,
};
use crate::math3d::{Mat4, Vec3, Vec4};
use crate::rasterizer::FrameBuffer;
use crate::scene3d::{Camera3D, Geometry3D, Light3D, Material3D, Scene3D};
use strata_layout::Rect;
use strata_style::types::Color;

/// Renders a Scene3D directly into a FrameBuffer viewport region with Z-buffering.
pub fn render_scene_3d(
    frame_buffer: &mut FrameBuffer,
    viewport: Rect,
    scene: &Scene3D,
    clip: Option<Rect>,
) {
    if viewport.width <= 1.0 || viewport.height <= 1.0 {
        return;
    }

    let aspect = viewport.width / viewport.height;

    // 1. Compute Projection and View Matrices
    let (proj_mat, view_mat) = match &scene.camera {
        Camera3D::Perspective {
            fov,
            near,
            far,
            position,
            target,
            ..
        } => {
            let fov_rad = fov.to_radians();
            let p = Mat4::perspective(fov_rad, aspect, *near, *far);
            let eye = Vec3::new(position[0], position[1], position[2]);
            let tgt = Vec3::new(target[0], target[1], target[2]);
            let v = Mat4::look_at(eye, tgt, Vec3::UP);
            (p, v)
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
            let v = Mat4::look_at(eye, tgt, Vec3::UP);
            (p, v)
        }
    };

    let vp_mat = proj_mat.multiply(&view_mat);

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

    // 3. Initialize Viewport Z-Buffer
    let vp_w = viewport.width.ceil() as usize;
    let vp_h = viewport.height.ceil() as usize;
    let mut depth_buffer = vec![1.0f32; vp_w * vp_h];

    // 4. Render Meshes
    for mesh in &scene.meshes {
        let buffers = match &mesh.geometry {
            Geometry3D::Box { width, height, depth } => generate_box(*width, *height, *depth),
            Geometry3D::Sphere { radius, width_segments, height_segments } => {
                generate_sphere(*radius, *width_segments, *height_segments)
            }
            Geometry3D::Plane { width, height } => generate_plane(*width, *height, 1, 1),
            Geometry3D::Cylinder {
                radius_top,
                radius_bottom,
                height,
                radial_segments,
            } => generate_cylinder(*radius_top, *radius_bottom, *height, *radial_segments),
            Geometry3D::Custom { vertices, indices } => generate_custom(vertices, indices),
        };

        let t = Vec3::new(mesh.transform.position[0], mesh.transform.position[1], mesh.transform.position[2]);
        let r = Vec3::new(mesh.transform.rotation[0], mesh.transform.rotation[1], mesh.transform.rotation[2]);
        let s = Vec3::new(mesh.transform.scale[0], mesh.transform.scale[1], mesh.transform.scale[2]);

        let model_mat = Mat4::from_trs(t, r, s);
        let normal_mat = model_mat.inverse().unwrap_or(Mat4::IDENTITY).transpose();
        let mvp = vp_mat.multiply(&model_mat);

        let base_color = match &mesh.material {
            Material3D::Standard { color, .. } => *color,
            Material3D::Physical { color, .. } => *color,
            Material3D::Basic { color } => *color,
            Material3D::Shader { .. } => Color::rgba(180, 180, 220, 255),
        };

        // Transform all vertices to screen coordinates
        struct ScreenVertex {
            x: f32,
            y: f32,
            z: f32,
            w: f32,
            world_normal: Vec3,
        }

        let mut screen_vertices = Vec::with_capacity(buffers.vertices.len());
        for v in &buffers.vertices {
            let clip_pos = mvp.transform_vec4(Vec4::from_vec3(v.position, 1.0));
            let norm = normal_mat.transform_vector(v.normal).normalize();

            if clip_pos.w > 1e-4 {
                let inv_w = 1.0 / clip_pos.w;
                let ndc_x = clip_pos.x * inv_w;
                let ndc_y = clip_pos.y * inv_w;
                let ndc_z = clip_pos.z * inv_w;

                let sx = (ndc_x + 1.0) * 0.5 * viewport.width;
                let sy = (1.0 - ndc_y) * 0.5 * viewport.height;

                screen_vertices.push(ScreenVertex {
                    x: sx,
                    y: sy,
                    z: ndc_z,
                    w: clip_pos.w,
                    world_normal: norm,
                });
            } else {
                screen_vertices.push(ScreenVertex {
                    x: -9999.0,
                    y: -9999.0,
                    z: 2.0,
                    w: 0.0,
                    world_normal: norm,
                });
            }
        }

        // Rasterize triangles
        for tri in buffers.indices.chunks_exact(3) {
            let v0 = &screen_vertices[tri[0] as usize];
            let v1 = &screen_vertices[tri[1] as usize];
            let v2 = &screen_vertices[tri[2] as usize];

            if v0.w <= 0.0 || v1.w <= 0.0 || v2.w <= 0.0 {
                continue;
            }

            // Triangle bounding box
            let min_x = (v0.x.min(v1.x).min(v2.x).floor() as isize).max(0).min(vp_w as isize) as usize;
            let max_x = (v0.x.max(v1.x).max(v2.x).ceil() as isize).max(0).min(vp_w as isize) as usize;
            let min_y = (v0.y.min(v1.y).min(v2.y).floor() as isize).max(0).min(vp_h as isize) as usize;
            let max_y = (v0.y.max(v1.y).max(v2.y).ceil() as isize).max(0).min(vp_h as isize) as usize;

            let denom = (v1.y - v2.y) * (v0.x - v2.x) + (v2.x - v1.x) * (v0.y - v2.y);
            if denom.abs() < 1e-5 {
                continue;
            }
            let inv_denom = 1.0 / denom;

            for py in min_y..max_y {
                let fy = py as f32 + 0.5;
                for px in min_x..max_x {
                    let fx = px as f32 + 0.5;

                    // Barycentric coordinates
                    let w0 = ((v1.y - v2.y) * (fx - v2.x) + (v2.x - v1.x) * (fy - v2.y)) * inv_denom;
                    let w1 = ((v2.y - v0.y) * (fx - v2.x) + (v0.x - v2.x) * (fy - v2.y)) * inv_denom;
                    let w2 = 1.0 - w0 - w1;

                    if w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0 {
                        let depth = w0 * v0.z + w1 * v1.z + w2 * v2.z;
                        let depth_idx = py * vp_w + px;

                        if depth >= 0.0 && depth < depth_buffer[depth_idx] {
                            depth_buffer[depth_idx] = depth;

                            // Interpolated Normal
                            let norm = (v0.world_normal * w0 + v1.world_normal * w1 + v2.world_normal * w2).normalize();

                            // Blinn-Phong Diffuse Lighting
                            let n_dot_l = norm.dot(dir_light_dir).max(0.0);
                            let diffuse = dir_light_intensity * n_dot_l;

                            let r_factor = (ambient_color.r() * ambient_intensity + dir_light_color.r() * diffuse).min(1.0);
                            let g_factor = (ambient_color.g() * ambient_intensity + dir_light_color.g() * diffuse).min(1.0);
                            let b_factor = (ambient_color.b() * ambient_intensity + dir_light_color.b() * diffuse).min(1.0);

                            let r = ((base_color.r() * r_factor) * 255.0).min(255.0) as u8;
                            let g = ((base_color.g() * g_factor) * 255.0).min(255.0) as u8;
                            let b = ((base_color.b() * b_factor) * 255.0).min(255.0) as u8;

                            let global_x = viewport.x as usize + px;
                            let global_y = viewport.y as usize + py;

                            // Apply optional 2D scissor clipping
                            let inside_clip = match clip {
                                Some(c) => {
                                    let gx = global_x as f32;
                                    let gy = global_y as f32;
                                    gx >= c.x && gx < c.max_x() && gy >= c.y && gy < c.max_y()
                                }
                                None => true,
                            };

                            if inside_clip {
                                frame_buffer.set_pixel(global_x, global_y, Color::rgba(r, g, b, 255));
                            }
                        }
                    }
                }
            }
        }
    }
}
