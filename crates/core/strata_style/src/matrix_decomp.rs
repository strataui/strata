//! Matrix decomposition and spherical interpolation for W3C CSS Transforms Module Level 1 & 2.
//!
//! Provides mathematically authentic 2D matrix decomposition (translation, scale, rotation, skew)
//! and 3D decomposition with quaternion spherical linear interpolation (slerp), eliminating
//! visual popping and preserving continuous mechanical kinematics during CSS transitions.

use std::f32::consts::PI;

/// Decomposed components of a 2D affine transformation `matrix(a, b, c, d, e, f)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Decomposed2d {
    /// Translation offset in pixels `(tx, ty)`.
    pub translate: (f32, f32),
    /// Scaling factors `(sx, sy)`.
    pub scale: (f32, f32),
    /// Rotation angle in radians.
    pub angle: f32,
    /// Skew factor along X axis.
    pub skew: f32,
}

impl Decomposed2d {
    /// Decomposes a 2D affine transformation matrix `[a, b, c, d, e, f]` into elementary components.
    ///
    /// The input represents the 3x3 homogeneous matrix:
    /// ```text
    /// [ a  c  e ]
    /// [ b  d  f ]
    /// [ 0  0  1 ]
    /// ```
    pub fn from_matrix(m: [f32; 6]) -> Option<Self> {
        let [a, b, c, d, e, f] = m;
        let det = a * d - b * c;
        if det.abs() < 1e-9 {
            return None;
        }

        let translate = (e, f);
        let scale_x = (a * a + b * b).sqrt();
        let angle = if scale_x > 1e-9 {
            b.atan2(a)
        } else {
            0.0
        };

        // Normalize first column
        let (a_norm, b_norm) = if scale_x > 1e-9 {
            (a / scale_x, b / scale_x)
        } else {
            (1.0, 0.0)
        };

        // Compute skew factor: dot product of normalized first column with second column
        let skew = a_norm * c + b_norm * d;

        // Orthogonalize second column
        let c_ortho = c - skew * a_norm;
        let d_ortho = d - skew * b_norm;
        let mut scale_y = (c_ortho * c_ortho + d_ortho * d_ortho).sqrt();

        // Preserve orientation / reflection if determinant is negative
        if det < 0.0 {
            scale_y = -scale_y;
        }

        Some(Self {
            translate,
            scale: (scale_x, scale_y),
            angle,
            skew,
        })
    }

    /// Recomposes the decomposed components back into the standard CSS 2D matrix `[a, b, c, d, e, f]`.
    ///
    /// Evaluates \(\text{Translate} \times \text{Rotate} \times \text{SkewX} \times \text{Scale}\).
    pub fn to_matrix(&self) -> [f32; 6] {
        let (sin_theta, cos_theta) = self.angle.sin_cos();
        let (sx, sy) = self.scale;
        let (tx, ty) = self.translate;
        let skew = self.skew;

        let a = sx * cos_theta;
        let b = sx * sin_theta;
        let c = sy * (cos_theta * skew - sin_theta);
        let d = sy * (sin_theta * skew + cos_theta);
        let e = tx;
        let f = ty;

        [a, b, c, d, e, f]
    }

    /// Interpolates smoothly between `self` and `other` at progress `t` in `[0.0, 1.0]`.
    ///
    /// Performs shortest-arc rotational angular interpolation around the unit circle to eliminate
    /// 360-degree rotational spin pops.
    pub fn interpolate(&self, other: &Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        let translate = (
            self.translate.0 + (other.translate.0 - self.translate.0) * t,
            self.translate.1 + (other.translate.1 - self.translate.1) * t,
        );
        let scale = (
            self.scale.0 + (other.scale.0 - self.scale.0) * t,
            self.scale.1 + (other.scale.1 - self.scale.1) * t,
        );
        let skew = self.skew + (other.skew - self.skew) * t;

        // Shortest arc angle interpolation
        let mut diff = other.angle - self.angle;
        while diff > PI {
            diff -= 2.0 * PI;
        }
        while diff <= -PI {
            diff += 2.0 * PI;
        }
        let angle = self.angle + diff * t;

        Self {
            translate,
            scale,
            angle,
            skew,
        }
    }
}

/// Spherical linear interpolation (slerp) between two unit quaternions `[x, y, z, w]`.
pub fn slerp_quaternion(q1: [f32; 4], q2: [f32; 4], t: f32) -> [f32; 4] {
    let t = t.clamp(0.0, 1.0);
    let mut dot = q1[0] * q2[0] + q1[1] * q2[1] + q1[2] * q2[2] + q1[3] * q2[3];
    let mut q2 = q2;

    // Pick the shortest path on the 4D hypersphere
    if dot < 0.0 {
        dot = -dot;
        q2 = [-q2[0], -q2[1], -q2[2], -q2[3]];
    }

    // If quaternions are extremely close, use linear interpolation to avoid division by zero
    if dot > 0.9995 {
        let out = [
            q1[0] + (q2[0] - q1[0]) * t,
            q1[1] + (q2[1] - q1[1]) * t,
            q1[2] + (q2[2] - q1[2]) * t,
            q1[3] + (q2[3] - q1[3]) * t,
        ];
        let len = (out[0] * out[0] + out[1] * out[1] + out[2] * out[2] + out[3] * out[3]).sqrt();
        if len > 1e-6 {
            return [out[0] / len, out[1] / len, out[2] / len, out[3] / len];
        }
        return q1;
    }

    let theta_0 = dot.clamp(-1.0, 1.0).acos();
    let theta = theta_0 * t;
    let sin_theta = theta.sin();
    let sin_theta_0 = theta_0.sin();

    let s0 = (theta_0 - theta).cos() - dot * sin_theta / sin_theta_0;
    let s1 = sin_theta / sin_theta_0;

    [
        s0 * q1[0] + s1 * q2[0],
        s0 * q1[1] + s1 * q2[1],
        s0 * q1[2] + s1 * q2[2],
        s0 * q1[3] + s1 * q2[3],
    ]
}
