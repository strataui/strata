//! 3D spatial audio positioning, distance attenuation, and Doppler shifts.

/// 3D vector for audio geometry and physics.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub fn dot(&self, other: Vec3) -> f32 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    pub fn length_squared(&self) -> f32 {
        self.dot(*self)
    }

    pub fn length(&self) -> f32 {
        self.length_squared().sqrt()
    }

    pub fn normalize(&self) -> Vec3 {
        let len = self.length();
        if len > 1e-6 {
            Vec3::new(self.x / len, self.y / len, self.z / len)
        } else {
            Vec3::default()
        }
    }

    pub fn sub(&self, other: Vec3) -> Vec3 {
        *self - other
    }
}

impl std::ops::Sub for Vec3 {
    type Output = Self;
    fn sub(self, other: Self) -> Self {
        Self::new(self.x - other.x, self.y - other.y, self.z - other.z)
    }
}

/// 3D audio listener representing the user's ears in virtual 3D space.
#[derive(Debug, Clone)]
pub struct AudioListener {
    pub position: Vec3,
    pub forward: Vec3,
    pub up: Vec3,
    pub velocity: Vec3,
}

impl Default for AudioListener {
    fn default() -> Self {
        Self {
            position: Vec3::new(0.0, 0.0, 0.0),
            forward: Vec3::new(0.0, 0.0, -1.0),
            up: Vec3::new(0.0, 1.0, 0.0),
            velocity: Vec3::default(),
        }
    }
}

/// 3D audio panner representing an emitter in virtual 3D space.
#[derive(Debug, Clone)]
pub struct Panner3D {
    pub position: Vec3,
    pub velocity: Vec3,
    pub ref_distance: f32,
    pub max_distance: f32,
    pub rolloff_factor: f32,
}

impl Default for Panner3D {
    fn default() -> Self {
        Self {
            position: Vec3::default(),
            velocity: Vec3::default(),
            ref_distance: 1.0,
            max_distance: 10000.0,
            rolloff_factor: 1.0,
        }
    }
}

impl Panner3D {
    pub fn new(position: Vec3) -> Self {
        Self {
            position,
            ..Default::default()
        }
    }

    /// Computes distance gain attenuation using W3C Inverse Distance model.
    pub fn compute_gain(&self, listener: &AudioListener) -> f32 {
        let dist = self.position.sub(listener.position).length();
        let clamped_dist = dist.clamp(self.ref_distance, self.max_distance);
        self.ref_distance / (self.ref_distance + self.rolloff_factor * (clamped_dist - self.ref_distance))
    }

    /// Computes physical Doppler frequency shift factor with c = 343.3 m/s.
    pub fn compute_doppler_factor(&self, listener: &AudioListener) -> f32 {
        const SPEED_OF_SOUND: f32 = 343.3; // m/s in air at 20 deg C

        let delta = listener.position.sub(self.position);
        let dist = delta.length();
        if dist < 1e-6 {
            return 1.0;
        }

        let unit = delta.normalize();
        let v_listener = listener.velocity.dot(unit);
        let v_source = self.velocity.dot(unit);

        let denom = SPEED_OF_SOUND - v_source;
        if denom.abs() < 1e-3 {
            1.0
        } else {
            ((SPEED_OF_SOUND - v_listener) / denom).clamp(0.1, 10.0)
        }
    }

    /// Computes stereo panning gains: (gain_left, gain_right).
    pub fn compute_stereo_panning(&self, listener: &AudioListener) -> (f32, f32) {
        let delta = self.position.sub(listener.position);
        let dist = delta.length();
        if dist < 1e-6 {
            return (std::f32::consts::FRAC_1_SQRT_2, std::f32::consts::FRAC_1_SQRT_2);
        }

        let dir = delta.normalize();
        // Compute right vector: forward x up
        let right = Vec3::new(
            listener.forward.y * listener.up.z - listener.forward.z * listener.up.y,
            listener.forward.z * listener.up.x - listener.forward.x * listener.up.z,
            listener.forward.x * listener.up.y - listener.forward.y * listener.up.x,
        ).normalize();

        // Pan in [-1.0 (left), 1.0 (right)]
        let pan = dir.dot(right).clamp(-1.0, 1.0);

        // Equal power panning curve: L = cos((pan+1)*pi/4), R = sin((pan+1)*pi/4)
        let angle = (pan + 1.0) * (std::f32::consts::PI / 4.0);
        let left_gain = angle.cos();
        let right_gain = angle.sin();

        (left_gain, right_gain)
    }
}
