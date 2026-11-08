//! f64 camera and relative-to-centre (RTC) geometry path.
//!
//! The reference builds its render frame in f64 relative to the camera centre.
//! At Earth radius (6 378 137 m) an f32 world coordinate steps by a metre, so
//! sub-metre detail disappears before the vertex is even transformed. This
//! module keeps the camera, the view transform, and the vertex subtraction in
//! f64, and narrows only the final screen coordinates.
//!
//! Matrices are stored column-major, the same convention as
//! [`crate::math3d::Mat4`], so an f64 frame and an f32 scene camera project a
//! shared anchor to the same pixel.

/// Double-precision 3D vector.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct DVec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl DVec3 {
    pub const ZERO: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    pub const UP: Self = Self {
        x: 0.0,
        y: 1.0,
        z: 0.0,
    };

    #[inline(always)]
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    #[inline(always)]
    pub fn sub(self, other: Self) -> Self {
        self - other
    }

    #[inline(always)]
    pub fn add(self, other: Self) -> Self {
        self + other
    }
}

impl std::ops::Sub for DVec3 {
    type Output = Self;
    #[inline(always)]
    fn sub(self, other: Self) -> Self {
        Self::new(self.x - other.x, self.y - other.y, self.z - other.z)
    }
}

impl std::ops::Add for DVec3 {
    type Output = Self;
    #[inline(always)]
    fn add(self, other: Self) -> Self {
        Self::new(self.x + other.x, self.y + other.y, self.z + other.z)
    }
}

impl DVec3 {

    #[inline(always)]
    pub fn scale(self, k: f64) -> Self {
        Self::new(self.x * k, self.y * k, self.z * k)
    }

    #[inline(always)]
    pub fn dot(self, other: Self) -> f64 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    #[inline(always)]
    pub fn cross(self, other: Self) -> Self {
        Self::new(
            self.y * other.z - self.z * other.y,
            self.z * other.x - self.x * other.z,
            self.x * other.y - self.y * other.x,
        )
    }

    #[inline(always)]
    pub fn length(self) -> f64 {
        self.dot(self).sqrt()
    }

    #[inline(always)]
    pub fn normalize(self) -> Self {
        let len = self.length();
        if len <= f64::EPSILON {
            Self::ZERO
        } else {
            self.scale(1.0 / len)
        }
    }

    #[inline(always)]
    pub fn lerp(self, other: Self, t: f64) -> Self {
        self.add(other.sub(self).scale(t))
    }

    #[inline(always)]
    pub fn as_f32(self) -> [f32; 3] {
        [self.x as f32, self.y as f32, self.z as f32]
    }
}

/// Double-precision 4x4 matrix stored column-major.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DMat4 {
    /// Column-major elements: `m[col * 4 + row]`.
    pub m: [f64; 16],
}

impl DMat4 {
    pub const IDENTITY: Self = Self {
        m: [
            1.0, 0.0, 0.0, 0.0, //
            0.0, 1.0, 0.0, 0.0, //
            0.0, 0.0, 1.0, 0.0, //
            0.0, 0.0, 0.0, 1.0,
        ],
    };

    pub fn multiply(&self, rhs: &Self) -> Self {
        let mut out = [0.0f64; 16];
        for col in 0..4 {
            for row in 0..4 {
                let mut sum = 0.0;
                for k in 0..4 {
                    sum += self.m[k * 4 + row] * rhs.m[col * 4 + k];
                }
                out[col * 4 + row] = sum;
            }
        }
        Self { m: out }
    }

    /// Transforms a point and returns clip space `[x, y, z, w]`.
    pub fn transform_point4(&self, p: DVec3) -> [f64; 4] {
        let m = &self.m;
        [
            m[0] * p.x + m[4] * p.y + m[8] * p.z + m[12],
            m[1] * p.x + m[5] * p.y + m[9] * p.z + m[13],
            m[2] * p.x + m[6] * p.y + m[10] * p.z + m[14],
            m[3] * p.x + m[7] * p.y + m[11] * p.z + m[15],
        ]
    }

    /// Right-handed look-at, matching `math3d::Mat4::look_at` exactly.
    pub fn look_at(eye: DVec3, target: DVec3, up: DVec3) -> Self {
        let f = target.sub(eye).normalize();
        // A view direction parallel to the up hint has no unique right vector.
        // Fall back to a non-parallel axis, as the f32 path does in practice for
        // every camera it is handed.
        let up = if f.cross(up).length() < 1e-6 {
            if f.cross(DVec3::new(0.0, 0.0, 1.0)).length() > 1e-6 {
                DVec3::new(0.0, 0.0, 1.0)
            } else {
                DVec3::new(0.0, 1.0, 0.0)
            }
        } else {
            up
        };
        let s = f.cross(up).normalize();
        let u = s.cross(f);

        Self {
            m: [
                s.x,
                u.x,
                -f.x,
                0.0,
                s.y,
                u.y,
                -f.y,
                0.0,
                s.z,
                u.z,
                -f.z,
                0.0,
                -s.dot(eye),
                -u.dot(eye),
                f.dot(eye),
                1.0,
            ],
        }
    }

    /// Perspective projection with depth in `0..=1`, column-major.
    ///
    /// `math3d::Mat4` multiplies with `M * v` where the third and fourth rows
    /// are swapped relative to the usual convention. The f32 3D path therefore
    /// uses this layout, and the f64 path mirrors it so a point projects to the
    /// same pixel through either matrix.
    pub fn perspective(fov_y_rad: f64, aspect: f64, near: f64, far: f64) -> Self {
        let f = 1.0 / (fov_y_rad * 0.5).tan();
        let range_inv = 1.0 / (near - far);
        Self {
            m: [
                f / aspect,
                0.0,
                0.0,
                0.0,
                0.0,
                f,
                0.0,
                0.0,
                0.0,
                0.0,
                -far * range_inv,
                -1.0,
                0.0,
                0.0,
                near * far * range_inv,
                0.0,
            ],
        }
    }
}

/// Earth's nominal radius in metres.
pub const EARTH_RADIUS_M: f64 = 6_378_137.0;

/// A camera whose position and basis stay in f64.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera64 {
    pub eye: DVec3,
    pub target: DVec3,
    pub up: DVec3,
    /// Horizontal field of view in degrees, matching
    /// `Camera3D::Perspective::fov_horizontal_deg`.
    pub fov_horizontal_deg: f64,
    pub near: f64,
    pub far: f64,
}

impl Default for Camera64 {
    fn default() -> Self {
        Self {
            eye: DVec3::new(0.0, 0.0, 10.0),
            target: DVec3::ZERO,
            up: DVec3::UP,
            fov_horizontal_deg: 60.0,
            near: 0.1,
            far: 1.0e9,
        }
    }
}

/// A prepared frame: f64 projection plus the RTC origin subtracted from world
/// positions before they are narrowed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderFrame64 {
    pub view_from_world: DMat4,
    pub projection: DMat4,
    /// World-space point treated as the origin of the local vertex stream.
    pub rtc_origin: DVec3,
    /// Viewport size in logical pixels.
    pub width: f64,
    pub height: f64,
}

/// A projected screen position in f64 logical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProjectedPoint64 {
    pub x: f64,
    pub y: f64,
    /// Normalized device depth in `0..=1` for front-facing points.
    pub depth: f64,
    /// Clip-space w, positive in front of the camera.
    pub w: f64,
    /// True when the point is behind or on the near plane.
    pub clipped: bool,
}

impl RenderFrame64 {
    /// Prepares a frame. `rtc_origin` should be the camera target, a tile
    /// centre, or another point inside the region being drawn; everything
    /// nearby then stays small enough to survive f32 narrowing at the end.
    pub fn new(camera: &Camera64, rtc_origin: DVec3, width: f64, height: f64) -> Self {
        let aspect = if height.abs() > f64::EPSILON {
            width / height
        } else {
            1.0
        };
        // `Camera3D::Perspective` applies its field of view on the horizontal
        // axis, while the projection matrix takes a vertical one.
        let fov_x = camera.fov_horizontal_deg.to_radians();
        let fov_y = 2.0 * ((fov_x * 0.5).tan() / aspect).atan();
        // Build the view around the RTC origin too. `project` subtracts that
        // origin from each world point before applying this matrix, so the eye
        // and target must be shifted by the same amount or the origin would
        // not cancel.
        Self {
            view_from_world: DMat4::look_at(
                camera.eye.sub(rtc_origin),
                camera.target.sub(rtc_origin),
                camera.up,
            ),
            projection: DMat4::perspective(fov_y, aspect, camera.near, camera.far),
            rtc_origin,
            width,
            height,
        }
    }

    /// Builds a frame whose RTC origin is the camera target.
    pub fn from_camera(camera: &Camera64, width: f64, height: f64) -> Self {
        Self::new(camera, camera.target, width, height)
    }

    /// Projects a world-space point. Subtraction from the RTC origin happens in
    /// f64 before any narrowing.
    ///
    /// `math3d::Mat4` multiplies with `M * v` and stores the view translation
    /// in the last row, so clip-space `w` comes out as `-z` for a point in
    /// front of the camera. The sign is flipped here so `w` reads as positive
    /// depth, which is what the clipping test and callers expect.
    pub fn project(&self, world: DVec3) -> ProjectedPoint64 {
        let local = world.sub(self.rtc_origin);
        let view = self.view_from_world.transform_point4(local);
        let clip = self
            .projection
            .transform_point4(DVec3::new(view[0], view[1], view[2]));
        // With `math3d::Mat4`'s row layout a point in front of the camera
        // yields a positive clip w, and negative eye-space z.
        if clip[3] <= 1e-9 {
            return ProjectedPoint64 {
                x: 0.0,
                y: 0.0,
                depth: 1.0,
                w: clip[3],
                clipped: true,
            };
        }
        let ndc_x = clip[0] / clip[3];
        let ndc_y = clip[1] / clip[3];
        let ndc_z = -clip[2] / clip[3];
        ProjectedPoint64 {
            x: (ndc_x + 1.0) * 0.5 * self.width,
            y: (1.0 - ndc_y) * 0.5 * self.height,
            depth: ndc_z,
            w: clip[3],
            clipped: false,
        }
    }

    /// Projects and narrows to f32 screen coordinates, the hand-off point to
    /// the f32 rasterizer.
    pub fn project_f32(&self, world: DVec3) -> [f32; 2] {
        let p = self.project(world);
        [p.x as f32, p.y as f32]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rtc_subtraction_keeps_sub_metre_detail_visible_at_earth_radius() {
        let a = DVec3::new(EARTH_RADIUS_M, 0.0, 0.0);
        let b = DVec3::new(EARTH_RADIUS_M + 1.0, 0.0, 0.0);

        // The f32 path cannot hold sub-metre detail at this magnitude.
        let a32 = a.as_f32();
        let quarter = DVec3::new(EARTH_RADIUS_M + 0.25, 0.0, 0.0).as_f32();
        assert_eq!(
            a32, quarter,
            "an f32 world coordinate at Earth radius must quantise sub-metre offsets away"
        );

        // The f64 RTC path keeps the pair distinct: the step at constant
        // latitude runs along local east, so look down from above the polar
        // axis to see it as a screen offset.
        let camera = Camera64 {
            eye: DVec3::new(0.0, 0.0, 20_000.0),
            target: DVec3::ZERO,
            up: DVec3::new(0.0, 1.0, 0.0),
            fov_horizontal_deg: 120.0,
            near: 1.0,
            far: 1e9,
        };
        let frame = RenderFrame64::from_camera(&camera, 1440.0, 900.0);
        let pa = frame.project(a);
        let pb = frame.project(b);
        assert!(
            !pa.clipped && !pb.clipped,
            "both points must be in front of the eye"
        );
        let separation = (pa.x - pb.x).hypot(pa.y - pb.y);
        assert!(
            separation > 0.0,
            "f64 RTC must resolve the metre, got {pa:?} and {pb:?}"
        );
    }
}
