//! Zero-dependency 3D linear algebra engine: Vec3, Vec4, Quat, Mat4, and Ray3D.

use std::ops::{Add, Div, Mul, Neg, Sub};

/// 3D Vector
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0);
    pub const ONE: Self = Self::new(1.0, 1.0, 1.0);
    pub const UP: Self = Self::new(0.0, 1.0, 0.0);
    pub const RIGHT: Self = Self::new(1.0, 0.0, 0.0);
    pub const FORWARD: Self = Self::new(0.0, 0.0, -1.0);

    #[inline(always)]
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    #[inline(always)]
    pub fn dot(self, other: Self) -> f32 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    #[inline(always)]
    pub fn cross(self, other: Self) -> Self {
        Self {
            x: self.y * other.z - self.z * other.y,
            y: self.z * other.x - self.x * other.z,
            z: self.x * other.y - self.y * other.x,
        }
    }

    #[inline(always)]
    pub fn length_squared(self) -> f32 {
        self.dot(self)
    }

    #[inline(always)]
    pub fn length(self) -> f32 {
        self.length_squared().sqrt()
    }

    #[inline(always)]
    pub fn normalize(self) -> Self {
        let len = self.length();
        if len > 1e-6 {
            self / len
        } else {
            Self::ZERO
        }
    }

    #[inline(always)]
    pub fn lerp(self, other: Self, t: f32) -> Self {
        self + (other - self) * t
    }
}

impl Add for Vec3 {
    type Output = Self;
    #[inline(always)]
    fn add(self, rhs: Self) -> Self::Output {
        Self::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }
}

impl Sub for Vec3 {
    type Output = Self;
    #[inline(always)]
    fn sub(self, rhs: Self) -> Self::Output {
        Self::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }
}

impl Mul<f32> for Vec3 {
    type Output = Self;
    #[inline(always)]
    fn mul(self, rhs: f32) -> Self::Output {
        Self::new(self.x * rhs, self.y * rhs, self.z * rhs)
    }
}

impl Div<f32> for Vec3 {
    type Output = Self;
    #[inline(always)]
    fn div(self, rhs: f32) -> Self::Output {
        let inv = 1.0 / rhs;
        Self::new(self.x * inv, self.y * inv, self.z * inv)
    }
}

impl Neg for Vec3 {
    type Output = Self;
    #[inline(always)]
    fn neg(self) -> Self::Output {
        Self::new(-self.x, -self.y, -self.z)
    }
}

/// 4D Vector (Homogeneous coordinates)
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec4 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Vec4 {
    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0, 0.0);

    #[inline(always)]
    pub const fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self { x, y, z, w }
    }

    #[inline(always)]
    pub const fn from_vec3(v: Vec3, w: f32) -> Self {
        Self { x: v.x, y: v.y, z: v.z, w }
    }

    #[inline(always)]
    pub const fn xyz(self) -> Vec3 {
        Vec3::new(self.x, self.y, self.z)
    }

    #[inline(always)]
    pub fn dot(self, other: Self) -> f32 {
        self.x * other.x + self.y * other.y + self.z * other.z + self.w * other.w
    }
}

impl Add for Vec4 {
    type Output = Self;
    #[inline(always)]
    fn add(self, rhs: Self) -> Self::Output {
        Self::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z, self.w + rhs.w)
    }
}

impl Sub for Vec4 {
    type Output = Self;
    #[inline(always)]
    fn sub(self, rhs: Self) -> Self::Output {
        Self::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z, self.w - rhs.w)
    }
}

impl Mul<f32> for Vec4 {
    type Output = Self;
    #[inline(always)]
    fn mul(self, rhs: f32) -> Self::Output {
        Self::new(self.x * rhs, self.y * rhs, self.z * rhs, self.w * rhs)
    }
}

impl Div<f32> for Vec4 {
    type Output = Self;
    #[inline(always)]
    fn div(self, rhs: f32) -> Self::Output {
        let inv = 1.0 / rhs;
        Self::new(self.x * inv, self.y * inv, self.z * inv, self.w * inv)
    }
}

/// 4x4 Column-Major Matrix for 3D Graphics Pipelines
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mat4 {
    /// 16 floats stored in column-major order:
    /// [0]=m00, [1]=m10, [2]=m20, [3]=m30 (col 0)
    /// [4]=m01, [5]=m11, [6]=m21, [7]=m31 (col 1)
    /// [8]=m02, [9]=m12, [10]=m22, [11]=m32 (col 2)
    /// [12]=m03, [13]=m13, [14]=m23, [15]=m33 (col 3)
    pub m: [f32; 16],
}

impl Default for Mat4 {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Mat4 {
    pub const IDENTITY: Self = Self {
        m: [
            1.0, 0.0, 0.0, 0.0, // col 0
            0.0, 1.0, 0.0, 0.0, // col 1
            0.0, 0.0, 1.0, 0.0, // col 2
            0.0, 0.0, 0.0, 1.0, // col 3
        ],
    };

    pub const fn from_cols(c0: Vec4, c1: Vec4, c2: Vec4, c3: Vec4) -> Self {
        Self {
            m: [
                c0.x, c0.y, c0.z, c0.w,
                c1.x, c1.y, c1.z, c1.w,
                c2.x, c2.y, c2.z, c2.w,
                c3.x, c3.y, c3.z, c3.w,
            ],
        }
    }

    pub fn from_translation(v: Vec3) -> Self {
        let mut mat = Self::IDENTITY;
        mat.m[12] = v.x;
        mat.m[13] = v.y;
        mat.m[14] = v.z;
        mat
    }

    pub fn from_scale(s: Vec3) -> Self {
        let mut mat = Self::IDENTITY;
        mat.m[0] = s.x;
        mat.m[5] = s.y;
        mat.m[10] = s.z;
        mat
    }

    pub fn from_rotation_x(radians: f32) -> Self {
        let (sin, cos) = radians.sin_cos();
        let mut mat = Self::IDENTITY;
        mat.m[5] = cos;
        mat.m[6] = sin;
        mat.m[9] = -sin;
        mat.m[10] = cos;
        mat
    }

    pub fn from_rotation_y(radians: f32) -> Self {
        let (sin, cos) = radians.sin_cos();
        let mut mat = Self::IDENTITY;
        mat.m[0] = cos;
        mat.m[2] = -sin;
        mat.m[8] = sin;
        mat.m[10] = cos;
        mat
    }

    pub fn from_rotation_z(radians: f32) -> Self {
        let (sin, cos) = radians.sin_cos();
        let mut mat = Self::IDENTITY;
        mat.m[0] = cos;
        mat.m[1] = sin;
        mat.m[4] = -sin;
        mat.m[5] = cos;
        mat
    }

    pub fn from_euler(rx: f32, ry: f32, rz: f32) -> Self {
        // YZX rotation order
        let mx = Self::from_rotation_x(rx);
        let my = Self::from_rotation_y(ry);
        let mz = Self::from_rotation_z(rz);
        my.multiply(&mz).multiply(&mx)
    }

    pub fn from_trs(translation: Vec3, rotation: Vec3, scale: Vec3) -> Self {
        let t = Self::from_translation(translation);
        let r = Self::from_euler(rotation.x, rotation.y, rotation.z);
        let s = Self::from_scale(scale);
        t.multiply(&r).multiply(&s)
    }

    pub fn multiply(&self, rhs: &Self) -> Self {
        let mut out = [0.0f32; 16];
        for col in 0..4 {
            for row in 0..4 {
                let mut sum = 0.0f32;
                for k in 0..4 {
                    sum += self.m[k * 4 + row] * rhs.m[col * 4 + k];
                }
                out[col * 4 + row] = sum;
            }
        }
        Self { m: out }
    }

    pub fn transform_vec4(&self, v: Vec4) -> Vec4 {
        Vec4::new(
            self.m[0] * v.x + self.m[4] * v.y + self.m[8] * v.z + self.m[12] * v.w,
            self.m[1] * v.x + self.m[5] * v.y + self.m[9] * v.z + self.m[13] * v.w,
            self.m[2] * v.x + self.m[6] * v.y + self.m[10] * v.z + self.m[14] * v.w,
            self.m[3] * v.x + self.m[7] * v.y + self.m[11] * v.z + self.m[15] * v.w,
        )
    }

    pub fn transform_point(&self, p: Vec3) -> Vec3 {
        let v = self.transform_vec4(Vec4::from_vec3(p, 1.0));
        if v.w.abs() > 1e-6 {
            Vec3::new(v.x / v.w, v.y / v.w, v.z / v.w)
        } else {
            Vec3::new(v.x, v.y, v.z)
        }
    }

    pub fn transform_vector(&self, v: Vec3) -> Vec3 {
        let res = self.transform_vec4(Vec4::from_vec3(v, 0.0));
        Vec3::new(res.x, res.y, res.z)
    }

    pub fn transpose(&self) -> Self {
        let mut out = [0.0f32; 16];
        for r in 0..4 {
            for c in 0..4 {
                out[c * 4 + r] = self.m[r * 4 + c];
            }
        }
        Self { m: out }
    }

    pub fn determinant(&self) -> f32 {
        let m = self.m;
        let s0 = m[0] * m[5] - m[1] * m[4];
        let s1 = m[0] * m[6] - m[2] * m[4];
        let s2 = m[0] * m[7] - m[3] * m[4];
        let s3 = m[1] * m[6] - m[2] * m[5];
        let s4 = m[1] * m[7] - m[3] * m[5];
        let s5 = m[2] * m[7] - m[3] * m[6];

        let c5 = m[10] * m[15] - m[11] * m[14];
        let c4 = m[9] * m[15] - m[11] * m[13];
        let c3 = m[9] * m[14] - m[10] * m[13];
        let c2 = m[8] * m[15] - m[11] * m[12];
        let c1 = m[8] * m[14] - m[10] * m[12];
        let c0 = m[8] * m[13] - m[9] * m[12];

        s0 * c5 - s1 * c4 + s2 * c3 + s3 * c2 - s4 * c1 + s5 * c0
    }

    pub fn inverse(&self) -> Option<Self> {
        let det = self.determinant();
        if det.abs() < 1e-7 {
            return None;
        }

        let inv_det = 1.0 / det;
        let m = self.m;
        let mut inv = [0.0f32; 16];

        inv[0] = (m[5] * (m[10] * m[15] - m[11] * m[14]) - m[9] * (m[6] * m[15] - m[7] * m[14]) + m[13] * (m[6] * m[11] - m[7] * m[10])) * inv_det;
        inv[1] = -(m[1] * (m[10] * m[15] - m[11] * m[14]) - m[9] * (m[2] * m[15] - m[3] * m[14]) + m[13] * (m[2] * m[11] - m[3] * m[10])) * inv_det;
        inv[2] = (m[1] * (m[6] * m[15] - m[7] * m[14]) - m[5] * (m[2] * m[15] - m[3] * m[14]) + m[13] * (m[2] * m[7] - m[3] * m[6])) * inv_det;
        inv[3] = -(m[1] * (m[6] * m[11] - m[7] * m[10]) - m[5] * (m[2] * m[11] - m[3] * m[10]) + m[9] * (m[2] * m[7] - m[3] * m[6])) * inv_det;

        inv[4] = -(m[4] * (m[10] * m[15] - m[11] * m[14]) - m[8] * (m[6] * m[15] - m[7] * m[14]) + m[12] * (m[6] * m[11] - m[7] * m[10])) * inv_det;
        inv[5] = (m[0] * (m[10] * m[15] - m[11] * m[14]) - m[8] * (m[2] * m[15] - m[3] * m[14]) + m[12] * (m[2] * m[11] - m[3] * m[10])) * inv_det;
        inv[6] = -(m[0] * (m[6] * m[15] - m[7] * m[14]) - m[4] * (m[2] * m[15] - m[3] * m[14]) + m[12] * (m[2] * m[7] - m[3] * m[6])) * inv_det;
        inv[7] = (m[0] * (m[6] * m[11] - m[7] * m[10]) - m[4] * (m[2] * m[11] - m[3] * m[10]) + m[8] * (m[2] * m[7] - m[3] * m[6])) * inv_det;

        inv[8] = (m[4] * (m[9] * m[15] - m[11] * m[13]) - m[8] * (m[5] * m[15] - m[7] * m[13]) + m[12] * (m[5] * m[11] - m[7] * m[9])) * inv_det;
        inv[9] = -(m[0] * (m[9] * m[15] - m[11] * m[13]) - m[8] * (m[1] * m[15] - m[3] * m[13]) + m[12] * (m[1] * m[11] - m[3] * m[9])) * inv_det;
        inv[10] = (m[0] * (m[5] * m[15] - m[7] * m[13]) - m[4] * (m[1] * m[15] - m[3] * m[13]) + m[12] * (m[1] * m[7] - m[3] * m[5])) * inv_det;
        inv[11] = -(m[0] * (m[5] * m[11] - m[7] * m[9]) - m[4] * (m[1] * m[11] - m[3] * m[9]) + m[8] * (m[1] * m[7] - m[3] * m[5])) * inv_det;

        inv[12] = -(m[4] * (m[9] * m[14] - m[10] * m[13]) - m[8] * (m[5] * m[14] - m[6] * m[13]) + m[12] * (m[5] * m[10] - m[6] * m[9])) * inv_det;
        inv[13] = (m[0] * (m[9] * m[14] - m[10] * m[13]) - m[8] * (m[1] * m[14] - m[2] * m[13]) + m[12] * (m[1] * m[10] - m[2] * m[9])) * inv_det;
        inv[14] = -(m[0] * (m[5] * m[14] - m[6] * m[13]) - m[4] * (m[1] * m[14] - m[2] * m[13]) + m[12] * (m[1] * m[6] - m[2] * m[5])) * inv_det;
        inv[15] = (m[0] * (m[5] * m[10] - m[6] * m[9]) - m[4] * (m[1] * m[10] - m[2] * m[9]) + m[8] * (m[1] * m[6] - m[2] * m[5])) * inv_det;

        Some(Self { m: inv })
    }

    /// View Matrix from Eye, Target, and Up vectors (Right-Handed, matching Vulkan/OpenGL)
    pub fn look_at(eye: Vec3, target: Vec3, up: Vec3) -> Self {
        let f = (target - eye).normalize();
        let s = f.cross(up).normalize();
        let u = s.cross(f);

        Self {
            m: [
                s.x, u.x, -f.x, 0.0,
                s.y, u.y, -f.y, 0.0,
                s.z, u.z, -f.z, 0.0,
                -s.dot(eye), -u.dot(eye), f.dot(eye), 1.0,
            ],
        }
    }

    /// Perspective Projection Matrix (Field of View Y in radians, depth [0, 1])
    pub fn perspective(fov_y_rad: f32, aspect: f32, near: f32, far: f32) -> Self {
        let f = 1.0 / (fov_y_rad * 0.5).tan();
        let range_inv = 1.0 / (near - far);

        Self {
            m: [
                f / aspect, 0.0, 0.0, 0.0,
                0.0, f, 0.0, 0.0,
                0.0, 0.0, far * range_inv, -1.0,
                0.0, 0.0, near * far * range_inv, 0.0,
            ],
        }
    }

    /// Orthographic Projection Matrix (depth [0, 1])
    pub fn orthographic(left: f32, right: f32, bottom: f32, top: f32, near: f32, far: f32) -> Self {
        let r_l = 1.0 / (right - left);
        let t_b = 1.0 / (top - bottom);
        let f_n = 1.0 / (near - far);

        Self {
            m: [
                2.0 * r_l, 0.0, 0.0, 0.0,
                0.0, 2.0 * t_b, 0.0, 0.0,
                0.0, 0.0, f_n, 0.0,
                -(right + left) * r_l, -(top + bottom) * t_b, near * f_n, 1.0,
            ],
        }
    }
}

/// 3D Ray for screen-to-world raycasting
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ray3D {
    pub origin: Vec3,
    pub direction: Vec3,
}

impl Ray3D {
    pub fn new(origin: Vec3, direction: Vec3) -> Self {
        Self {
            origin,
            direction: direction.normalize(),
        }
    }

    /// Unprojects screen coordinates into a world-space Ray3D.
    pub fn unproject(
        screen_x: f32,
        screen_y: f32,
        viewport_w: f32,
        viewport_h: f32,
        inv_proj: &Mat4,
        inv_view: &Mat4,
    ) -> Self {
        // Normalized Device Coordinates (NDC) in range [-1, 1]
        let ndc_x = (2.0 * screen_x / viewport_w) - 1.0;
        let ndc_y = 1.0 - (2.0 * screen_y / viewport_h);

        // Camera position in world space (eye)
        let eye = inv_view.transform_point(Vec3::ZERO);

        // Far plane point in world space
        let far_ndc = Vec4::new(ndc_x, ndc_y, 1.0, 1.0);
        let far_view = inv_proj.transform_vec4(far_ndc);
        let far_world = inv_view.transform_vec4(far_view);
        let p_far = far_world.xyz() / far_world.w;

        let dir = (p_far - eye).normalize();
        Self::new(eye, dir)
    }
}
