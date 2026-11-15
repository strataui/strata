//! 3D spatial audio positioning, HRTF binaural synthesis, distance attenuation, and Doppler shifts.

use crate::filter::{BiquadFilter, BiquadFilterType};

/// Speed of sound in dry air at 20°C in meters per second.
pub const SPEED_OF_SOUND: f32 = 343.3;

/// Average adult human head radius in meters (used for Woodworth spherical ITD calculations).
pub const HUMAN_HEAD_RADIUS: f32 = 0.0875;

/// Spatialization algorithm mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SpatializationMode {
    /// Equal-power trigonometric stereophonic panning.
    EqualPower,
    /// Authentic binaural HRTF modeling with Woodworth ITD, head-shadow ILD, and pinna elevation notches.
    #[default]
    Hrtf,
}

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

    pub fn cross(&self, other: Vec3) -> Vec3 {
        Vec3::new(
            self.y * other.z - self.z * other.y,
            self.z * other.x - self.x * other.z,
            self.x * other.y - self.y * other.x,
        )
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

    pub fn add(&self, other: Vec3) -> Vec3 {
        *self + other
    }

    pub fn mul_scalar(&self, s: f32) -> Vec3 {
        *self * s
    }
}

impl std::ops::Sub for Vec3 {
    type Output = Self;
    fn sub(self, other: Self) -> Self {
        Self::new(self.x - other.x, self.y - other.y, self.z - other.z)
    }
}

impl std::ops::Add for Vec3 {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        Self::new(self.x + other.x, self.y + other.y, self.z + other.z)
    }
}

impl std::ops::Mul<f32> for Vec3 {
    type Output = Self;
    fn mul(self, rhs: f32) -> Self {
        Self::new(self.x * rhs, self.y * rhs, self.z * rhs)
    }
}

/// 3D audio listener representing the user's ears and head orientation in virtual 3D space.
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

impl AudioListener {
    /// Computes the normalized right unit vector ($\vec{R} = \vec{F} \times \vec{U}$).
    pub fn right(&self) -> Vec3 {
        self.forward.cross(self.up).normalize()
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
    pub spatialization_mode: SpatializationMode,
}

impl Default for Panner3D {
    fn default() -> Self {
        Self {
            position: Vec3::default(),
            velocity: Vec3::default(),
            ref_distance: 1.0,
            max_distance: 10000.0,
            rolloff_factor: 1.0,
            spatialization_mode: SpatializationMode::Hrtf,
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

    pub fn with_mode(mut self, mode: SpatializationMode) -> Self {
        self.spatialization_mode = mode;
        self
    }

    /// Computes distance gain attenuation using W3C Inverse Distance model.
    pub fn compute_gain(&self, listener: &AudioListener) -> f32 {
        let dist = self.position.sub(listener.position).length();
        let clamped_dist = dist.clamp(self.ref_distance, self.max_distance);
        self.ref_distance
            / (self.ref_distance + self.rolloff_factor * (clamped_dist - self.ref_distance))
    }

    /// Computes physical Doppler frequency shift factor with c = 343.3 m/s.
    pub fn compute_doppler_factor(&self, listener: &AudioListener) -> f32 {
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

    /// Computes horizontal azimuth and vertical elevation angles (in radians) relative to the listener.
    ///
    /// - Azimuth: 0 = straight ahead, $+\pi/2$ = 90° right, $-\pi/2$ = 90° left, $\pm\pi$ = behind.
    /// - Elevation: 0 = horizontal plane, $+\pi/2$ = above (+Y), $-\pi/2$ = below (-Y).
    pub fn compute_azimuth_elevation(&self, listener: &AudioListener) -> (f32, f32) {
        let delta = self.position.sub(listener.position);
        let dist = delta.length();
        if dist < 1e-6 {
            return (0.0, 0.0);
        }

        let dir = delta.normalize();
        let forward = listener.forward.normalize();
        let up = listener.up.normalize();
        let right = listener.right();

        let x_rel = dir.dot(right);
        let y_rel = dir.dot(up);
        let z_rel = dir.dot(forward);

        let elevation = y_rel.clamp(-1.0, 1.0).asin();
        let azimuth = x_rel.atan2(z_rel);

        (azimuth, elevation)
    }

    /// Computes Interaural Time Difference (ITD) in seconds using Woodworth's spherical head formula.
    ///
    /// $\Delta t = \frac{a}{c} (\sin|\theta| + |\theta|)$
    pub fn compute_woodworth_itd(&self, azimuth_rad: f32) -> f32 {
        let theta = azimuth_rad.abs().min(std::f32::consts::PI);
        let theta_clamped = theta.min(std::f32::consts::FRAC_PI_2);
        (HUMAN_HEAD_RADIUS / SPEED_OF_SOUND) * (theta_clamped.sin() + theta_clamped)
    }

    /// Computes legacy stereo panning gains: (gain_left, gain_right).
    pub fn compute_stereo_panning(&self, listener: &AudioListener) -> (f32, f32) {
        let (azimuth, _) = self.compute_azimuth_elevation(listener);
        let pan = (azimuth / std::f32::consts::FRAC_PI_2).clamp(-1.0, 1.0);

        // Equal power panning curve: L = cos((pan+1)*pi/4), R = sin((pan+1)*pi/4)
        let angle = (pan + 1.0) * (std::f32::consts::PI / 4.0);
        (angle.cos(), angle.sin())
    }

    /// Synthesizes binaural HRTF stereo channels from mono audio samples.
    ///
    /// Implements:
    /// 1. Distance attenuation ($1/d$).
    /// 2. Woodworth interaural time difference (ITD) with fractional sub-sample delay.
    /// 3. Interaural level difference (ILD) acoustic head shadow low-pass filtering.
    /// 4. Pinna spectral notch filtering based on source elevation.
    pub fn render_hrtf(
        &self,
        mono_samples: &[f32],
        listener: &AudioListener,
        sample_rate: u32,
    ) -> (Vec<f32>, Vec<f32>) {
        if mono_samples.is_empty() {
            return (Vec::new(), Vec::new());
        }

        let distance_gain = self.compute_gain(listener);
        let (azimuth, elevation) = self.compute_azimuth_elevation(listener);
        let itd_seconds = self.compute_woodworth_itd(azimuth);
        let itd_samples = itd_seconds * sample_rate as f32;

        // Pinna spectral elevation notch filter: f_notch(phi) = 7000 + 3000 * sin(phi)
        let notch_freq = (7000.0 + 3000.0 * elevation.sin()).clamp(4000.0, 12000.0);
        let mut pinna_notch =
            BiquadFilter::new(BiquadFilterType::Notch, notch_freq, 3.0, sample_rate);

        // Filter the input samples with the pinna elevation notch
        let mut pinna_filtered = Vec::with_capacity(mono_samples.len());
        for &s in mono_samples {
            pinna_filtered.push(pinna_notch.process_sample(s));
        }

        // Head shadow acoustic diffraction filter for the contralateral (shadowed) ear.
        // As azimuth moves away from the ear, high frequencies are increasingly shadowed.
        let sin_abs = azimuth.abs().sin().clamp(0.0, 1.0);
        let shadow_cutoff = 1500.0 + (20000.0 - 1500.0) * (1.0 - sin_abs);
        let mut head_shadow_filter = BiquadFilter::new(
            BiquadFilterType::Lowpass,
            shadow_cutoff,
            std::f32::consts::FRAC_1_SQRT_2,
            sample_rate,
        );

        let contra_gain = 1.0 - 0.5 * sin_abs; // ILD level attenuation up to -6 dB

        let mut left_ch;
        let mut right_ch;

        if azimuth >= 0.0 {
            // Source is to the right (+X)
            // Right ear is ipsilateral (direct path, 0 ITD delay)
            right_ch = pinna_filtered.clone();
            for s in &mut right_ch {
                *s *= distance_gain;
            }

            // Left ear is contralateral (shadowed by head + delayed by ITD)
            let mut shadow_filtered = Vec::with_capacity(pinna_filtered.len());
            for &s in &pinna_filtered {
                shadow_filtered
                    .push(head_shadow_filter.process_sample(s) * distance_gain * contra_gain);
            }
            left_ch = apply_fractional_delay(&shadow_filtered, itd_samples);
        } else {
            // Source is to the left (-X)
            // Left ear is ipsilateral (direct path, 0 ITD delay)
            left_ch = pinna_filtered.clone();
            for s in &mut left_ch {
                *s *= distance_gain;
            }

            // Right ear is contralateral (shadowed by head + delayed by ITD)
            let mut shadow_filtered = Vec::with_capacity(pinna_filtered.len());
            for &s in &pinna_filtered {
                shadow_filtered
                    .push(head_shadow_filter.process_sample(s) * distance_gain * contra_gain);
            }
            right_ch = apply_fractional_delay(&shadow_filtered, itd_samples);
        }

        (left_ch, right_ch)
    }
}

/// Applies continuous sub-sample fractional time delay to an audio slice.
pub fn apply_fractional_delay(samples: &[f32], delay_samples: f32) -> Vec<f32> {
    if delay_samples <= 0.0 || samples.is_empty() {
        return samples.to_vec();
    }
    let n = samples.len();
    let mut out = Vec::with_capacity(n);

    for i in 0..n {
        let delayed_t = i as f32 - delay_samples;
        if delayed_t < 0.0 {
            // Pre-arrival acoustic delay
            out.push(0.0);
        } else {
            let idx = delayed_t.floor() as usize;
            let sub_frac = delayed_t - idx as f32;
            let s0 = samples[idx];
            let s1 = if idx + 1 < n { samples[idx + 1] } else { 0.0 };
            out.push(s0 + (s1 - s0) * sub_frac);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vec3_operations() {
        let a = Vec3::new(1.0, 0.0, 0.0);
        let b = Vec3::new(0.0, 1.0, 0.0);
        let c = a.cross(b);
        assert_eq!(c, Vec3::new(0.0, 0.0, 1.0));
        assert_eq!(a.dot(b), 0.0);
        assert_eq!(a.length(), 1.0);
    }

    #[test]
    fn test_woodworth_itd_values() {
        let panner = Panner3D::default();
        // At 0 azimuth (straight ahead), ITD must be 0
        assert_eq!(panner.compute_woodworth_itd(0.0), 0.0);

        // At 90 degrees (PI / 2), ITD is approx 655 microseconds
        let itd_90 = panner.compute_woodworth_itd(std::f32::consts::FRAC_PI_2);
        assert!(
            (itd_90 - 0.000655).abs() < 5e-5,
            "ITD at 90 deg: got {}",
            itd_90
        );
    }

    #[test]
    fn test_fractional_delay_interpolation() {
        let input = vec![0.0, 1.0, 2.0, 3.0, 4.0];
        let delayed = apply_fractional_delay(&input, 1.5);
        assert_eq!(delayed.len(), 5);
        assert_eq!(delayed[0], 0.0);
        assert_eq!(delayed[1], 0.0); // 1.0 - 1.5 = -0.5 -> 0.0
                                     // At index 2: 2.0 - 1.5 = 0.5 -> (input[0] + input[1]) / 2 = 0.5
        assert!((delayed[2] - 0.5).abs() < 1e-6);
        // At index 3: 3.0 - 1.5 = 1.5 -> (input[1] + input[2]) / 2 = 1.5
        assert!((delayed[3] - 1.5).abs() < 1e-6);
    }

    #[test]
    fn test_hrtf_binaural_rendering_asymmetry() {
        let listener = AudioListener::default();
        // Source positioned 5 units to the right
        let panner = Panner3D::new(Vec3::new(5.0, 0.0, 0.0));
        let mono_input = vec![1.0; 100];
        let (left, right) = panner.render_hrtf(&mono_input, &listener, 44100);

        assert_eq!(left.len(), 100);
        assert_eq!(right.len(), 100);

        // The right ear (ipsilateral) receives direct sound instantly
        assert!(right[0] > 0.0);
        // The left ear (contralateral) is delayed by Woodworth ITD (~28 samples at 44.1k)
        assert_eq!(left[0], 0.0, "Contralateral ear must be delayed by ITD");
        assert_eq!(left[20], 0.0, "Contralateral ear must be delayed by ITD");
        assert!(
            left[35] > 0.0,
            "Contralateral ear receives signal after ITD arrival"
        );
    }
}
