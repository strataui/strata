//! Incremental mesh construction for tiled terrain, relief walls, building
//! prisms, and their roofs.
//!
//! The reference rebuilds per-tile height fields and building geometry every
//! frame. `Geometry3D` can already carry arbitrary vertices, but a scene built
//! from hundreds of tiles and thousands of buildings must not reallocate in the
//! frame hot path. [`MeshBuilder`] owns reusable vertex and index capacity; a
//! rebuild clears and refills it, so steady-state frames allocate nothing.
//!
//! Coordinates are f64 because tiled geometry is authored at Earth scale.
//! [`MeshBuilder::finish`] subtracts an RTC origin before narrowing, which is
//! the same contract as [`crate::camera64::RenderFrame64`].

use crate::camera64::DVec3;
use crate::geometry3d::TEXTURED_VERTEX_STRIDE;
use crate::scene3d::Geometry3D;

/// Reusable builder for a textured mesh at Earth scale.
///
/// Vertex layout is the eight-float stride `x, y, z, nx, ny, nz, u, v` that
/// [`Geometry3D::CustomTextured`] consumes.
#[derive(Debug, Clone, Default)]
pub struct MeshBuilder {
    vertices: Vec<f32>,
    indices: Vec<u32>,
    /// RTC origin subtracted when the mesh is finished.
    origin: DVec3,
}

impl MeshBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Reserves capacity for `vertices` vertices and `triangles` triangles.
    pub fn with_capacity(vertices: usize, triangles: usize) -> Self {
        Self {
            vertices: Vec::with_capacity(vertices * TEXTURED_VERTEX_STRIDE),
            indices: Vec::with_capacity(triangles * 3),
            origin: DVec3::ZERO,
        }
    }

    /// Sets the point subtracted before narrowing to f32.
    pub fn set_origin(&mut self, origin: DVec3) {
        self.origin = origin;
    }

    /// Empties the builder while keeping its allocation.
    pub fn clear(&mut self) {
        self.vertices.clear();
        self.indices.clear();
    }

    /// Capacity currently held by the vertex buffer, in floats.
    pub fn vertex_capacity(&self) -> usize {
        self.vertices.capacity()
    }

    /// Capacity currently held by the index buffer, in indices.
    pub fn index_capacity(&self) -> usize {
        self.indices.capacity()
    }

    pub fn vertex_count(&self) -> usize {
        self.vertices.len() / TEXTURED_VERTEX_STRIDE
    }

    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// Number of indices appended so far, for batching index ranges.
    pub fn index_count(&self) -> usize {
        self.indices.len()
    }

    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    /// Appends a vertex at world scale, relative to the builder origin, and
    /// returns its local index.
    pub fn push_vertex(&mut self, position: DVec3, normal: [f32; 3], uv: (f32, f32)) -> u32 {
        let local = position.sub(self.origin);
        self.vertices.push(local.x as f32);
        self.vertices.push(local.y as f32);
        self.vertices.push(local.z as f32);
        self.vertices.extend_from_slice(&normal);
        self.vertices.push(uv.0);
        self.vertices.push(uv.1);
        (self.vertex_count() - 1) as u32
    }

    /// Appends a triangle. Degenerate index triplets are dropped.
    pub fn push_triangle(&mut self, a: u32, b: u32, c: u32) {
        if a == b || b == c || a == c {
            return;
        }
        self.indices.extend_from_slice(&[a, b, c]);
    }

    /// Appends a quad as two triangles.
    pub fn push_quad(&mut self, a: u32, b: u32, c: u32, d: u32) {
        self.push_triangle(a, b, c);
        self.push_triangle(a, c, d);
    }

    /// Appends a terrain patch from a height field.
    ///
    /// `heights` is a `(cols x rows)` grid of world-space positions in
    /// row-major order; `uv_rect` is the tile's sub-rectangle of the source
    /// image, expressed in the quad's own 0..1 space and mapped across the
    /// patch. The patch is two triangles per grid cell.
    pub fn push_grid(
        &mut self,
        heights: &[DVec3],
        cols: usize,
        rows: usize,
        uv_rect: (f32, f32, f32, f32),
    ) {
        if cols < 2 || rows < 2 || heights.len() != cols * rows {
            return;
        }
        let (u0, v0, u1, v1) = uv_rect;
        let base = self.vertex_count() as u32;
        for row in 0..rows {
            let v = v0 + (v1 - v0) * (row as f32 / (rows - 1) as f32);
            for col in 0..cols {
                let u = u0 + (u1 - u0) * (col as f32 / (cols - 1) as f32);
                let position = heights[row * cols + col];
                self.push_vertex(position, [0.0, 0.0, 1.0], (u, v));
            }
        }
        for row in 0..rows - 1 {
            for col in 0..cols - 1 {
                let i = base + (row * cols + col) as u32;
                let right = i + 1;
                let down = i + cols as u32;
                let down_right = down + 1;
                self.push_quad(i, right, down_right, down);
            }
        }
    }

    /// Appends a grid patch with explicit per-vertex normals.
    ///
    /// Use this for relief, where the surface normal is derived from the
    /// height field rather than being the sphere radial.
    pub fn push_grid_with_normals(
        &mut self,
        positions: &[DVec3],
        normals: &[[f32; 3]],
        cols: usize,
        rows: usize,
        uv_rect: (f32, f32, f32, f32),
    ) {
        if cols < 2 || rows < 2 || positions.len() != cols * rows || normals.len() != cols * rows {
            return;
        }
        let (u0, v0, u1, v1) = uv_rect;
        let base = self.vertex_count() as u32;
        for row in 0..rows {
            let v = v0 + (v1 - v0) * (row as f32 / (rows - 1) as f32);
            for col in 0..cols {
                let u = u0 + (u1 - u0) * (col as f32 / (cols - 1) as f32);
                let index = row * cols + col;
                self.push_vertex(positions[index], normals[index], (u, v));
            }
        }
        for row in 0..rows - 1 {
            for col in 0..cols - 1 {
                let i = base + (row * cols + col) as u32;
                let right = i + 1;
                let down = i + cols as u32;
                let down_right = down + 1;
                self.push_quad(i, right, down_right, down);
            }
        }
    }

    /// Appends a vertical relief wall from a top edge to a bottom edge.
    ///
    /// This is the side skirt that hides the gap between a raised terrain tile
    /// and the tile behind it. `top` and `bottom` are equal-length edge points.
    pub fn push_wall(&mut self, top: &[DVec3], bottom: &[DVec3], uv_rect: (f32, f32, f32, f32)) {
        if top.len() < 2 || top.len() != bottom.len() {
            return;
        }
        let (u0, v0, u1, v1) = uv_rect;
        let segments = top.len() - 1;
        for segment in 0..segments {
            let t0 = segment as f32 / segments as f32;
            let t1 = (segment + 1) as f32 / segments as f32;
            let ua = u0 + (u1 - u0) * t0;
            let ub = u0 + (u1 - u0) * t1;
            let a = self.push_vertex(top[segment], [0.0, 0.0, 1.0], (ua, v1));
            let b = self.push_vertex(top[segment + 1], [0.0, 0.0, 1.0], (ub, v1));
            let c = self.push_vertex(bottom[segment + 1], [0.0, 0.0, 1.0], (ub, v0));
            let d = self.push_vertex(bottom[segment], [0.0, 0.0, 1.0], (ua, v0));
            self.push_quad(a, b, c, d);
        }
    }

    /// Appends a building prism plus its roof cap.
    ///
    /// `base` is the footprint centre on the ground, `half` the footprint half
    /// extents in the local X and Z axes, and `height` the extrusion. Walls use
    /// the side of `facade_uv` and the roof uses `roof_uv`, both sub-rectangles
    /// of one atlas.
    pub fn push_building(
        &mut self,
        base: DVec3,
        half: (f64, f64),
        height: f64,
        facade_uv: (f32, f32, f32, f32),
        roof_uv: (f32, f32, f32, f32),
    ) {
        if half.0 <= 0.0 || half.1 <= 0.0 || height <= 0.0 {
            return;
        }
        let (hx, hz) = half;
        let corners = [
            base.add(DVec3::new(-hx, -hz, 0.0)),
            base.add(DVec3::new(hx, -hz, 0.0)),
            base.add(DVec3::new(hx, hz, 0.0)),
            base.add(DVec3::new(-hx, hz, 0.0)),
        ];
        let top: Vec<DVec3> = corners
            .iter()
            .map(|c| c.add(DVec3::new(0.0, 0.0, height)))
            .collect();

        let (u0, v0, u1, v1) = facade_uv;
        let normals = [
            [0.0, -1.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [-1.0, 0.0, 0.0],
        ];
        for side in 0..4 {
            let next = (side + 1) % 4;
            let normal = normals[side];
            let a = self.push_vertex(corners[side], normal, (u0, v0));
            let b = self.push_vertex(corners[next], normal, (u1, v0));
            let c = self.push_vertex(top[next], normal, (u1, v1));
            let d = self.push_vertex(top[side], normal, (u0, v1));
            self.push_quad(a, b, c, d);
        }

        let (ru0, rv0, ru1, rv1) = roof_uv;
        let r0 = self.push_vertex(top[0], [0.0, 0.0, 1.0], (ru0, rv1));
        let r1 = self.push_vertex(top[1], [0.0, 0.0, 1.0], (ru1, rv1));
        let r2 = self.push_vertex(top[2], [0.0, 0.0, 1.0], (ru1, rv0));
        let r3 = self.push_vertex(top[3], [0.0, 0.0, 1.0], (ru0, rv0));
        self.push_quad(r0, r1, r2, r3);
    }

    /// Freezes the builder into a scene geometry, narrowing to f32 after the
    /// RTC subtraction. Returns `None` when nothing was appended.
    pub fn finish(&self) -> Option<Geometry3D> {
        if self.indices.is_empty() {
            return None;
        }
        Some(Geometry3D::CustomTextured {
            vertices: self.vertices.clone(),
            indices: self.indices.clone(),
        })
    }
}
