//! GPU instance buffers and path command encoding for hardware rasterization.

use crate::curve::QuadraticBezier;
use crate::path::Path;

/// Packed GPU instance data for a single quadratic Bézier curve segment (32 bytes).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GpuCurveInstance {
    pub p0_x: f32,
    pub p0_y: f32,
    pub p1_x: f32,
    pub p1_y: f32,
    pub p2_x: f32,
    pub p2_y: f32,
    pub flags: u32,
    pub z_index: f32,
}

impl GpuCurveInstance {
    pub const FLAG_FILL: u32 = 1 << 0;
    pub const FLAG_STROKE: u32 = 1 << 1;

    pub fn from_bezier(curve: &QuadraticBezier, flags: u32, z_index: f32) -> Self {
        Self {
            p0_x: curve.p0.x,
            p0_y: curve.p0.y,
            p1_x: curve.p1.x,
            p1_y: curve.p1.y,
            p2_x: curve.p2.x,
            p2_y: curve.p2.y,
            flags,
            z_index,
        }
    }
}

/// Batches vector paths into GPU draw calls and instance buffers.
#[derive(Debug, Default)]
pub struct PathCommandBuffer {
    pub instances: Vec<GpuCurveInstance>,
    pub total_draw_calls: usize,
}

impl PathCommandBuffer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a vector path to the GPU draw buffer.
    pub fn append_path(&mut self, path: &Path, tolerance: f32, z_index: f32) {
        let quads = path.to_quadratics(tolerance);
        let mut flags = 0;
        if path.fill_rule.is_some() {
            flags |= GpuCurveInstance::FLAG_FILL;
        }
        if path.stroke.is_some() {
            flags |= GpuCurveInstance::FLAG_STROKE;
        }

        for q in quads {
            self.instances.push(GpuCurveInstance::from_bezier(&q, flags, z_index));
        }
        self.total_draw_calls += 1;
    }

    pub fn clear(&mut self) {
        self.instances.clear();
        self.total_draw_calls = 0;
    }

    pub fn total_curves(&self) -> usize {
        self.instances.len()
    }

    pub fn byte_size(&self) -> usize {
        self.instances.len() * std::mem::size_of::<GpuCurveInstance>()
    }
}
