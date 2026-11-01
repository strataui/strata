//! Parametric 3D Geometry Generators: Box, Sphere, Plane, Cylinder, and Custom Buffers.

use crate::math3d::Vec3;

/// An interleaved 3D vertex with position, normal, and UV coordinates.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vertex3D {
    pub position: Vec3,
    pub normal: Vec3,
    pub uv: (f32, f32),
}

impl Vertex3D {
    #[inline(always)]
    pub const fn new(position: Vec3, normal: Vec3, uv: (f32, f32)) -> Self {
        Self { position, normal, uv }
    }
}

/// Generated GPU mesh buffers (interleaved vertices and index list).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MeshBuffers {
    pub vertices: Vec<Vertex3D>,
    pub indices: Vec<u32>,
}

impl MeshBuffers {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn vertex_count(&self) -> usize {
        self.vertices.len()
    }

    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }
}

/// Generates a 3D Box (cube) with distinct face normals (24 vertices, 36 indices).
pub fn generate_box(width: f32, height: f32, depth: f32) -> MeshBuffers {
    let w = width * 0.5;
    let h = height * 0.5;
    let d = depth * 0.5;

    let mut vertices = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);

    let faces = [
        // Front (+Z)
        (Vec3::new(0.0, 0.0, 1.0), [Vec3::new(-w, -h, d), Vec3::new(w, -h, d), Vec3::new(w, h, d), Vec3::new(-w, h, d)]),
        // Back (-Z)
        (Vec3::new(0.0, 0.0, -1.0), [Vec3::new(w, -h, -d), Vec3::new(-w, -h, -d), Vec3::new(-w, h, -d), Vec3::new(w, h, -d)]),
        // Top (+Y)
        (Vec3::new(0.0, 1.0, 0.0), [Vec3::new(-w, h, d), Vec3::new(w, h, d), Vec3::new(w, h, -d), Vec3::new(-w, h, -d)]),
        // Bottom (-Y)
        (Vec3::new(0.0, -1.0, 0.0), [Vec3::new(-w, -h, -d), Vec3::new(w, -h, -d), Vec3::new(w, -h, d), Vec3::new(-w, -h, d)]),
        // Right (+X)
        (Vec3::new(1.0, 0.0, 0.0), [Vec3::new(w, -h, d), Vec3::new(w, -h, -d), Vec3::new(w, h, -d), Vec3::new(w, h, d)]),
        // Left (-X)
        (Vec3::new(-1.0, 0.0, 0.0), [Vec3::new(-w, -h, -d), Vec3::new(-w, -h, d), Vec3::new(-w, h, d), Vec3::new(-w, h, -d)]),
    ];

    let uvs = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];

    for (normal, corners) in faces {
        let base_idx = vertices.len() as u32;
        for i in 0..4 {
            vertices.push(Vertex3D::new(corners[i], normal, uvs[i]));
        }
        indices.extend_from_slice(&[
            base_idx, base_idx + 1, base_idx + 2,
            base_idx, base_idx + 2, base_idx + 3,
        ]);
    }

    MeshBuffers { vertices, indices }
}

/// Generates a UV Sphere with configurable radial rings and segments.
pub fn generate_sphere(radius: f32, width_segments: u32, height_segments: u32) -> MeshBuffers {
    let ws = width_segments.max(3);
    let hs = height_segments.max(2);

    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    for y in 0..=hs {
        let v = y as f32 / hs as f32;
        let theta = v * std::f32::consts::PI;

        for x in 0..=ws {
            let u = x as f32 / ws as f32;
            let phi = u * std::f32::consts::TAU;

            let sin_theta = theta.sin();
            let cos_theta = theta.cos();
            let sin_phi = phi.sin();
            let cos_phi = phi.cos();

            let nx = sin_theta * cos_phi;
            let ny = cos_theta;
            let nz = sin_theta * sin_phi;

            let normal = Vec3::new(nx, ny, nz);
            let position = normal * radius;

            vertices.push(Vertex3D::new(position, normal, (u, 1.0 - v)));
        }
    }

    for y in 0..hs {
        for x in 0..ws {
            let i0 = y * (ws + 1) + x;
            let i1 = i0 + 1;
            let i2 = i0 + (ws + 1);
            let i3 = i2 + 1;

            if y != 0 {
                indices.extend_from_slice(&[i0, i1, i2]);
            }
            if y != hs - 1 {
                indices.extend_from_slice(&[i1, i3, i2]);
            }
        }
    }

    MeshBuffers { vertices, indices }
}

/// Generates a planar subdivided grid quad.
pub fn generate_plane(width: f32, height: f32, width_segments: u32, height_segments: u32) -> MeshBuffers {
    let ws = width_segments.max(1);
    let hs = height_segments.max(1);

    let w_half = width * 0.5;
    let h_half = height * 0.5;

    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    for y in 0..=hs {
        let v = y as f32 / hs as f32;
        let pos_y = h_half - v * height;

        for x in 0..=ws {
            let u = x as f32 / ws as f32;
            let pos_x = -w_half + u * width;

            vertices.push(Vertex3D::new(
                Vec3::new(pos_x, pos_y, 0.0),
                Vec3::new(0.0, 0.0, 1.0),
                (u, 1.0 - v),
            ));
        }
    }

    for y in 0..hs {
        for x in 0..ws {
            let a = y * (ws + 1) + x;
            let b = a + 1;
            let c = a + (ws + 1);
            let d = c + 1;

            indices.extend_from_slice(&[a, b, c]);
            indices.extend_from_slice(&[b, d, c]);
        }
    }

    MeshBuffers { vertices, indices }
}

/// Generates a cylinder geometry with top and bottom caps.
pub fn generate_cylinder(
    radius_top: f32,
    radius_bottom: f32,
    height: f32,
    radial_segments: u32,
) -> MeshBuffers {
    let segments = radial_segments.max(3);
    let half_h = height * 0.5;

    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    // 1. Mantle
    for y in 0..=1 {
        let v = y as f32;
        let radius = if y == 0 { radius_top } else { radius_bottom };
        let py = if y == 0 { half_h } else { -half_h };

        for x in 0..=segments {
            let u = x as f32 / segments as f32;
            let phi = u * std::f32::consts::TAU;

            let sin_phi = phi.sin();
            let cos_phi = phi.cos();

            let position = Vec3::new(radius * cos_phi, py, radius * sin_phi);
            let normal = Vec3::new(cos_phi, 0.0, sin_phi).normalize();

            vertices.push(Vertex3D::new(position, normal, (u, v)));
        }
    }

    for x in 0..segments {
        let a = x;
        let b = a + 1;
        let c = a + (segments + 1);
        let d = c + 1;

        indices.extend_from_slice(&[a, c, b]);
        indices.extend_from_slice(&[b, c, d]);
    }

    MeshBuffers { vertices, indices }
}

/// Wraps arbitrary flat vertices into MeshBuffers.
pub fn generate_custom(vertices_flat: &[f32], indices: &[u32]) -> MeshBuffers {
    let mut vertices = Vec::new();
    // Interleaved layout: pos(3), norm(3), uv(2) = 8 floats per vertex
    let stride = 8;
    for chunk in vertices_flat.chunks_exact(stride) {
        vertices.push(Vertex3D::new(
            Vec3::new(chunk[0], chunk[1], chunk[2]),
            Vec3::new(chunk[3], chunk[4], chunk[5]),
            (chunk[6], chunk[7]),
        ));
    }

    MeshBuffers {
        vertices,
        indices: indices.to_vec(),
    }
}
