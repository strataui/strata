//! 3D Chassis geometry, articulated mechanical hinge mesh, and specular crease rendering.

use crate::profile::{ChassisMaterial, DeviceProfile, HingeAxis};

/// 3D Transform and orientation matrix for the phone chassis.
#[derive(Debug, Clone, PartialEq)]
pub struct Chassis3DTransform {
    pub pitch_deg: f32,
    pub yaw_deg: f32,
    pub roll_deg: f32,
    pub hinge_angle_deg: f32,
}

impl Default for Chassis3DTransform {
    fn default() -> Self {
        Self {
            pitch_deg: 0.0,
            yaw_deg: 0.0,
            roll_deg: 0.0,
            hinge_angle_deg: 180.0,
        }
    }
}

/// 3D Chassis mesh specification.
#[derive(Debug, Clone, PartialEq)]
pub struct ChassisMeshSpec {
    pub width: f32,
    pub height: f32,
    pub depth: f32,
    pub outer_radius: f32,
    pub material: ChassisMaterial,
    pub finish_color: [f32; 4],
    pub is_articulated_hinge: bool,
    pub hinge_axis: Option<HingeAxis>,
}

/// Chassis 3D renderer and mesh generator.
pub struct ChassisRenderer;

impl ChassisRenderer {
    /// Generates the chassis mesh specification for the given device profile.
    pub fn build_spec(profile: &DeviceProfile, hinge_angle: f32) -> ChassisMeshSpec {
        let screen = profile.form_factor.active_screen();
        let bezels = &profile.chassis.bezel_insets;

        let total_w = screen.logical_width + bezels.left + bezels.right;
        let total_h = screen.logical_height + bezels.top + bezels.bottom;

        let (is_articulated, axis) = match &profile.form_factor {
            crate::profile::DeviceFormFactor::ClamshellFlip { hinge_axis, .. } => (true, Some(*hinge_axis)),
            crate::profile::DeviceFormFactor::BookFoldable { hinge_axis, .. } => (true, Some(*hinge_axis)),
            crate::profile::DeviceFormFactor::DualScreen { .. } => (true, Some(HingeAxis::Vertical)),
            crate::profile::DeviceFormFactor::Monolithic(_) => (false, None),
        };

        ChassisMeshSpec {
            width: total_w,
            height: total_h,
            depth: profile.chassis.depth_mm * 3.78, // mm to points approx
            outer_radius: profile.chassis.outer_corner_radius,
            material: profile.chassis.material,
            finish_color: profile.chassis.finish_color,
            is_articulated_hinge: is_articulated && hinge_angle < 179.0,
            hinge_axis: axis,
        }
    }

    /// Calculates normal perturbation for flexible OLED crease reflection.
    ///
    /// Distorts the specular normal map along the fold axis according to:
    /// N_pert = exp(-d^2 / (2 * sigma^2))
    pub fn calculate_crease_perturbation(dist_from_hinge_px: f32, sigma_px: f32) -> f32 {
        let d = dist_from_hinge_px;
        let exponent = -(d * d) / (2.0 * sigma_px * sigma_px);
        exponent.exp()
    }
}
