//! GPU Compute Buffer Layouts, Raw Curve Representations, and Compute Dispatchers.

use crate::curve::{CubicBezier, Point2D, QuadraticBezier};

/// Flag indicating that the path segment belongs to a filled polygon / region.
pub const FLAG_FILL: u32 = 1 << 0;
/// Flag indicating that the path segment is stroked with a line width.
pub const FLAG_STROKE: u32 = 1 << 1;
/// Flag indicating Even-Odd fill rule (default is Non-Zero winding rule).
pub const FLAG_EVEN_ODD: u32 = 1 << 2;
/// Flag indicating that the path segment is closed.
pub const FLAG_CLOSED: u32 = 1 << 3;

/// 64-byte aligned raw cubic curve input data fed into WebGPU compute pipelines.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RawCurveInput {
    pub p0: [f32; 2],
    pub p1: [f32; 2],
    pub p2: [f32; 2],
    pub p3: [f32; 2],
    pub stroke_width: f32,
    pub flags: u32,
    pub z_index: f32,
    pub _pad: f32,
    pub color: [f32; 4],
}

impl RawCurveInput {
    pub const BYTE_SIZE: usize = std::mem::size_of::<Self>();

    pub fn new_cubic(
        cubic: &CubicBezier,
        stroke_width: f32,
        flags: u32,
        color: [f32; 4],
        z_index: f32,
    ) -> Self {
        Self {
            p0: [cubic.p0.x, cubic.p0.y],
            p1: [cubic.p1.x, cubic.p1.y],
            p2: [cubic.p2.x, cubic.p2.y],
            p3: [cubic.p3.x, cubic.p3.y],
            stroke_width,
            flags,
            z_index,
            _pad: 0.0,
            color,
        }
    }

    pub fn new_quadratic(
        quad: &QuadraticBezier,
        stroke_width: f32,
        flags: u32,
        color: [f32; 4],
        z_index: f32,
    ) -> Self {
        // Degree elevate or replicate control point for compute pipeline uniformity
        Self {
            p0: [quad.p0.x, quad.p0.y],
            p1: [quad.p1.x, quad.p1.y],
            p2: [quad.p1.x, quad.p1.y],
            p3: [quad.p2.x, quad.p2.y],
            stroke_width,
            flags,
            z_index,
            _pad: 0.0,
            color,
        }
    }

    pub fn new_line(
        p0: Point2D,
        p1: Point2D,
        stroke_width: f32,
        flags: u32,
        color: [f32; 4],
        z_index: f32,
    ) -> Self {
        let mid = p0.lerp(p1, 0.5);
        Self {
            p0: [p0.x, p0.y],
            p1: [mid.x, mid.y],
            p2: [mid.x, mid.y],
            p3: [p1.x, p1.y],
            stroke_width,
            flags,
            z_index,
            _pad: 0.0,
            color,
        }
    }

    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts((self as *const Self) as *const u8, Self::BYTE_SIZE) }
    }

    #[inline]
    pub fn slice_to_bytes(slice: &[Self]) -> &[u8] {
        unsafe {
            std::slice::from_raw_parts(slice.as_ptr() as *const u8, slice.len() * Self::BYTE_SIZE)
        }
    }
}

/// 64-byte aligned output quadratic segment produced by compute shaders or CPU fallback.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GpuPathSegment {
    pub p0: [f32; 2],
    pub p1: [f32; 2],
    pub p2: [f32; 2],
    pub stroke_width: f32,
    pub flags: u32,
    pub color: [f32; 4],
    pub z_index: f32,
    pub _pad0: f32,
    pub _pad1: f32,
    pub _pad2: f32,
}

impl GpuPathSegment {
    pub const BYTE_SIZE: usize = std::mem::size_of::<Self>();

    pub fn from_quadratic(
        quad: &QuadraticBezier,
        stroke_width: f32,
        flags: u32,
        color: [f32; 4],
        z_index: f32,
    ) -> Self {
        Self {
            p0: [quad.p0.x, quad.p0.y],
            p1: [quad.p1.x, quad.p1.y],
            p2: [quad.p2.x, quad.p2.y],
            stroke_width,
            flags,
            color,
            z_index,
            _pad0: 0.0,
            _pad1: 0.0,
            _pad2: 0.0,
        }
    }

    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts((self as *const Self) as *const u8, Self::BYTE_SIZE) }
    }

    #[inline]
    pub fn slice_to_bytes(slice: &[Self]) -> &[u8] {
        unsafe {
            std::slice::from_raw_parts(slice.as_ptr() as *const u8, slice.len() * Self::BYTE_SIZE)
        }
    }
}

/// 16-byte aligned Uniform Buffer object for path compute operations.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GpuPathUniforms {
    pub viewport_size: [f32; 2],
    pub tolerance: f32,
    pub curve_count: u32,
}

impl GpuPathUniforms {
    pub const BYTE_SIZE: usize = std::mem::size_of::<Self>();

    pub fn new(viewport_width: f32, viewport_height: f32, tolerance: f32, curve_count: u32) -> Self {
        Self {
            viewport_size: [viewport_width, viewport_height],
            tolerance,
            curve_count,
        }
    }

    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts((self as *const Self) as *const u8, Self::BYTE_SIZE) }
    }
}

/// High-performance CPU dispatcher and simulator matching the WGSL compute pipeline layout.
#[derive(Debug, Clone, Default)]
pub struct CurveComputeDispatcher {
    pub input_curves: Vec<RawCurveInput>,
    pub tolerance: f32,
}

impl CurveComputeDispatcher {
    pub fn new(tolerance: f32) -> Self {
        Self {
            input_curves: Vec::new(),
            tolerance: if tolerance <= 0.0 { 0.25 } else { tolerance },
        }
    }

    pub fn push_cubic(
        &mut self,
        cubic: &CubicBezier,
        stroke_width: f32,
        flags: u32,
        color: [f32; 4],
        z_index: f32,
    ) {
        self.input_curves.push(RawCurveInput::new_cubic(
            cubic,
            stroke_width,
            flags,
            color,
            z_index,
        ));
    }

    pub fn push_quadratic(
        &mut self,
        quad: &QuadraticBezier,
        stroke_width: f32,
        flags: u32,
        color: [f32; 4],
        z_index: f32,
    ) {
        self.input_curves.push(RawCurveInput::new_quadratic(
            quad,
            stroke_width,
            flags,
            color,
            z_index,
        ));
    }

    pub fn push_line(
        &mut self,
        p0: Point2D,
        p1: Point2D,
        stroke_width: f32,
        flags: u32,
        color: [f32; 4],
        z_index: f32,
    ) {
        self.input_curves.push(RawCurveInput::new_line(
            p0,
            p1,
            stroke_width,
            flags,
            color,
            z_index,
        ));
    }

    pub fn clear(&mut self) {
        self.input_curves.clear();
    }

    pub fn len(&self) -> usize {
        self.input_curves.len()
    }

    pub fn is_empty(&self) -> bool {
        self.input_curves.is_empty()
    }

    pub fn input_bytes(&self) -> &[u8] {
        RawCurveInput::slice_to_bytes(&self.input_curves)
    }

    /// Simulates the WGSL compute pass on CPU, adaptively decomposing cubics into quadratics.
    pub fn dispatch_compute(&self) -> Vec<GpuPathSegment> {
        let mut output = Vec::with_capacity(self.input_curves.len());

        for raw in &self.input_curves {
            let p0 = Point2D::new(raw.p0[0], raw.p0[1]);
            let p1 = Point2D::new(raw.p1[0], raw.p1[1]);
            let p2 = Point2D::new(raw.p2[0], raw.p2[1]);
            let p3 = Point2D::new(raw.p3[0], raw.p3[1]);

            let cubic = CubicBezier::new(p0, p1, p2, p3);
            let quads = cubic.to_quadratics(self.tolerance);

            for q in quads {
                output.push(GpuPathSegment::from_quadratic(
                    &q,
                    raw.stroke_width,
                    raw.flags,
                    raw.color,
                    raw.z_index,
                ));
            }
        }

        output
    }
}
