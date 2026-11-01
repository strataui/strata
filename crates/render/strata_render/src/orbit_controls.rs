//! OrbitControls: Interactive 3D Camera Controller with Spherical Kinematics, Panning, and Damping.
//!
//! Provides mouse/touch orbit rotation, screen-space camera plane panning,
//! and geometric zooming with pole gimbal-lock prevention.

use crate::math3d::{Mat4, Vec3};
use crate::scene3d::Camera3D;

/// Active mouse or pointer drag interaction mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OrbitInteractionMode {
    #[default]
    None,
    Rotate,
    Pan,
    Zoom,
}

/// Interactive 3D Camera Orbit Controller.
#[derive(Debug, Clone, PartialEq)]
pub struct OrbitControls {
    // Current spherical coordinates
    pub radius: f32,
    pub phi: f32,
    pub theta: f32,
    pub target: Vec3,

    // Target spherical coordinates for damping / inertia
    target_radius: f32,
    target_phi: f32,
    target_theta: f32,
    target_center: Vec3,

    // Limits and tuning
    pub min_distance: f32,
    pub max_distance: f32,
    pub min_polar_angle: f32,
    pub max_polar_angle: f32,
    pub rotate_speed: f32,
    pub zoom_speed: f32,
    pub pan_speed: f32,
    pub enable_damping: bool,
    pub damping_factor: f32,

    // Pointer interaction tracking
    pub active_mode: OrbitInteractionMode,
    last_pointer_x: f32,
    last_pointer_y: f32,
}

impl Default for OrbitControls {
    fn default() -> Self {
        Self::new_with_target_and_eye(Vec3::ZERO, Vec3::new(0.0, 2.0, 5.0))
    }
}

impl OrbitControls {
    /// Creates OrbitControls initialized from target focus point and camera eye position.
    pub fn new_with_target_and_eye(target: Vec3, eye: Vec3) -> Self {
        let offset = eye - target;
        let radius = offset.length().max(0.1);
        let phi = if radius > 1e-5 {
            (offset.y / radius).clamp(-1.0, 1.0).acos()
        } else {
            std::f32::consts::FRAC_PI_2
        };
        let theta = offset.x.atan2(offset.z);

        Self {
            radius,
            phi,
            theta,
            target,
            target_radius: radius,
            target_phi: phi,
            target_theta: theta,
            target_center: target,
            min_distance: 0.2,
            max_distance: 2000.0,
            min_polar_angle: 0.01,
            max_polar_angle: std::f32::consts::PI - 0.01,
            rotate_speed: 1.0,
            zoom_speed: 1.0,
            pan_speed: 1.0,
            enable_damping: true,
            damping_factor: 0.1,
            active_mode: OrbitInteractionMode::None,
            last_pointer_x: 0.0,
            last_pointer_y: 0.0,
        }
    }

    /// Initializes OrbitControls from an existing Camera3D.
    pub fn from_camera(camera: &Camera3D) -> Self {
        match camera {
            Camera3D::Perspective { position, target, .. }
            | Camera3D::Orthographic { position, target, .. } => {
                let eye = Vec3::new(position[0], position[1], position[2]);
                let tgt = Vec3::new(target[0], target[1], target[2]);
                Self::new_with_target_and_eye(tgt, eye)
            }
        }
    }

    /// Computes the current 3D eye position in Cartesian coordinates.
    #[inline]
    pub fn eye_position(&self) -> Vec3 {
        let sin_phi = self.phi.sin();
        let cos_phi = self.phi.cos();
        let sin_theta = self.theta.sin();
        let cos_theta = self.theta.cos();

        Vec3::new(
            self.target.x + self.radius * sin_phi * sin_theta,
            self.target.y + self.radius * cos_phi,
            self.target.z + self.radius * sin_phi * cos_theta,
        )
    }

    /// Rotates azimuthal (theta) and polar (phi) angles by given deltas.
    pub fn rotate_by(&mut self, delta_theta: f32, delta_phi: f32) {
        self.target_theta += delta_theta * self.rotate_speed;
        self.target_phi = (self.target_phi + delta_phi * self.rotate_speed)
            .clamp(self.min_polar_angle, self.max_polar_angle);

        if !self.enable_damping {
            self.theta = self.target_theta;
            self.phi = self.target_phi;
        }
    }

    /// Zooms camera radius by multiplicative factor or wheel delta.
    pub fn zoom_delta(&mut self, delta: f32) {
        let factor = 1.0 - delta * 0.1 * self.zoom_speed;
        self.target_radius = (self.target_radius * factor)
            .clamp(self.min_distance, self.max_distance);

        if !self.enable_damping {
            self.radius = self.target_radius;
        }
    }

    /// Pans camera target point in screen-space camera plane.
    pub fn pan_by(
        &mut self,
        delta_x: f32,
        delta_y: f32,
        viewport_width: f32,
        viewport_height: f32,
        fov_degrees: f32,
    ) {
        let eye = self.eye_position();
        let forward = (self.target - eye).normalize();
        let right = forward.cross(Vec3::UP).normalize();
        let up = right.cross(forward).normalize();

        let fov_rad = fov_degrees.to_radians();
        let factor = 2.0 * self.radius * (fov_rad * 0.5).tan();
        let dx = (-delta_x / viewport_width.max(1.0)) * factor * (viewport_width / viewport_height.max(1.0)) * self.pan_speed;
        let dy = (delta_y / viewport_height.max(1.0)) * factor * self.pan_speed;

        let pan_offset = right * dx + up * dy;
        self.target_center = self.target_center + pan_offset;

        if !self.enable_damping {
            self.target = self.target_center;
        }
    }

    // --- Pointer Input Event Handlers ---

    /// Handles pointer down: button 0 = rotate, 1 = zoom, 2 = pan.
    pub fn on_pointer_down(&mut self, x: f32, y: f32, button: u8) {
        self.last_pointer_x = x;
        self.last_pointer_y = y;
        self.active_mode = match button {
            0 => OrbitInteractionMode::Rotate,
            1 => OrbitInteractionMode::Zoom,
            2 => OrbitInteractionMode::Pan,
            _ => OrbitInteractionMode::Rotate,
        };
    }

    /// Handles pointer move: computes screen deltas and applies appropriate transformation.
    pub fn on_pointer_move(
        &mut self,
        x: f32,
        y: f32,
        viewport_w: f32,
        viewport_h: f32,
        fov_degrees: f32,
    ) {
        if self.active_mode == OrbitInteractionMode::None {
            return;
        }

        let dx = x - self.last_pointer_x;
        let dy = y - self.last_pointer_y;
        self.last_pointer_x = x;
        self.last_pointer_y = y;

        match self.active_mode {
            OrbitInteractionMode::Rotate => {
                let delta_theta = -dx * (2.0 * std::f32::consts::PI / viewport_w.max(1.0));
                let delta_phi = -dy * (std::f32::consts::PI / viewport_h.max(1.0));
                self.rotate_by(delta_theta, delta_phi);
            }
            OrbitInteractionMode::Pan => {
                self.pan_by(dx, dy, viewport_w, viewport_h, fov_degrees);
            }
            OrbitInteractionMode::Zoom => {
                self.zoom_delta(dy * 0.05);
            }
            OrbitInteractionMode::None => {}
        }
    }

    /// Handles pointer up: releases active drag mode.
    pub fn on_pointer_up(&mut self) {
        self.active_mode = OrbitInteractionMode::None;
    }

    /// Handles mouse wheel zoom delta.
    pub fn on_wheel(&mut self, wheel_delta_y: f32) {
        self.zoom_delta(wheel_delta_y * 0.02);
    }

    /// Advances kinematic damping simulation.
    pub fn tick(&mut self, dt: f32) {
        if !self.enable_damping {
            return;
        }

        let alpha = (self.damping_factor * (dt / 0.016)).clamp(0.01, 1.0);
        self.radius += (self.target_radius - self.radius) * alpha;
        self.phi += (self.target_phi - self.phi) * alpha;
        self.theta += (self.target_theta - self.theta) * alpha;
        self.target = self.target.lerp(self.target_center, alpha);
    }

    /// Writes updated eye position and target back to a Camera3D.
    pub fn update_camera(&self, camera: &mut Camera3D) {
        let eye = self.eye_position();
        match camera {
            Camera3D::Perspective { position, target, .. }
            | Camera3D::Orthographic { position, target, .. } => {
                *position = [eye.x, eye.y, eye.z];
                *target = [self.target.x, self.target.y, self.target.z];
            }
        }
    }

    /// Computes the look-at view matrix corresponding to the current camera state.
    pub fn view_matrix(&self) -> Mat4 {
        Mat4::look_at(self.eye_position(), self.target, Vec3::UP)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_orbit_controls_initialization() {
        let target = Vec3::ZERO;
        let eye = Vec3::new(0.0, 0.0, 10.0);
        let controls = OrbitControls::new_with_target_and_eye(target, eye);

        assert!((controls.radius - 10.0).abs() < 1e-4);
        assert!((controls.phi - std::f32::consts::FRAC_PI_2).abs() < 1e-4);
        assert!(controls.theta.abs() < 1e-4);

        let eye_computed = controls.eye_position();
        assert!((eye_computed.z - 10.0).abs() < 1e-4);
    }

    #[test]
    fn test_orbit_controls_pole_clamping() {
        let mut controls = OrbitControls::default();
        controls.enable_damping = false;

        // Try rotating past north pole (phi < 0)
        controls.rotate_by(0.0, -100.0);
        assert!(controls.phi >= controls.min_polar_angle);

        // Try rotating past south pole (phi > PI)
        controls.rotate_by(0.0, 100.0);
        assert!(controls.phi <= controls.max_polar_angle);
    }

    #[test]
    fn test_orbit_controls_zoom_limits() {
        let mut controls = OrbitControls::default();
        controls.enable_damping = false;

        // Zoom in to minimum limit
        for _ in 0..100 {
            controls.zoom_delta(5.0);
        }
        assert!(controls.radius >= controls.min_distance);

        // Zoom out to maximum limit
        for _ in 0..100 {
            controls.zoom_delta(-5.0);
        }
        assert!(controls.radius <= controls.max_distance);
    }

    #[test]
    fn test_orbit_controls_pan_shifts_target() {
        let mut controls = OrbitControls::default();
        controls.enable_damping = false;
        let orig_target = controls.target;

        controls.pan_by(50.0, 0.0, 800.0, 600.0, 60.0);
        assert_ne!(controls.target, orig_target);
    }
}
