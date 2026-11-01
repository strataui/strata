use crate::profile::{CutoutProfile, DeviceProfile, NavigationType};

/// Device physical orientation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceOrientation {
    Portrait,
    LandscapeLeft,
    LandscapeRight,
    PortraitInverted,
}

/// Calculated safe area insets for application viewport padding.
#[derive(Debug, Clone, PartialEq)]
pub struct SafeAreaInsets {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl SafeAreaInsets {
    pub fn zero() -> Self {
        Self {
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
            left: 0.0,
        }
    }
}

/// Geometry solver for device chassis, displays, and insets.
pub struct GeometrySolver;

impl GeometrySolver {
    /// Mathematically derives the inner screen corner radius to guarantee exact concentricity with the outer chassis edge.
    ///
    /// Formula: R_inner = max(0.0, R_outer - B_bezel)
    pub fn derive_concentric_inner_radius(outer_radius: f32, bezel_thickness: f32) -> f32 {
        (outer_radius - bezel_thickness).max(0.0)
    }

    /// Computes the total outer physical chassis dimensions (width, height) including bezels.
    pub fn calculate_chassis_dimensions(profile: &DeviceProfile) -> (f32, f32) {
        let screen = profile.form_factor.active_screen();
        let bezels = &profile.chassis.bezel_insets;

        let total_w = screen.logical_width + bezels.left + bezels.right;
        let total_h = screen.logical_height + bezels.top + bezels.bottom;

        (total_w, total_h)
    }

    /// Computes safe area insets for the active screen under the specified orientation.
    pub fn calculate_safe_area(profile: &DeviceProfile, orientation: DeviceOrientation) -> SafeAreaInsets {
        let _screen = profile.form_factor.active_screen();
        let bezels = &profile.chassis.bezel_insets;

        let nav_bottom_inset = match profile.navigation_type {
            NavigationType::Gesture => 34.0, // Gesture pill bar clearance
            NavigationType::ThreeButton => 48.0, // 3-button navigation bar height
        };

        let cutout_top_inset = match &profile.cutout {
            CutoutProfile::None => bezels.top + 10.0,
            CutoutProfile::TopNotch { height, .. } => bezels.top + height + 6.0,
            CutoutProfile::Teardrop { height, .. } => bezels.top + height + 6.0,
            CutoutProfile::HolePunch { top_offset, diameter, .. } => bezels.top + top_offset + diameter + 6.0,
            CutoutProfile::DualHole { pill_height, .. } => bezels.top + pill_height + 14.0,
            CutoutProfile::DynamicIsland { resting_height, top_offset, .. } => bezels.top + top_offset + resting_height + 10.0,
        };

        match orientation {
            DeviceOrientation::Portrait => SafeAreaInsets {
                top: cutout_top_inset.max(bezels.top + 20.0),
                right: bezels.right,
                bottom: (bezels.bottom + nav_bottom_inset).max(24.0),
                left: bezels.left,
            },
            DeviceOrientation::LandscapeLeft => SafeAreaInsets {
                top: bezels.top + 10.0,
                right: bezels.right + 20.0,
                bottom: bezels.bottom + 21.0,
                left: (bezels.left + cutout_top_inset).max(44.0), // Camera on left side
            },
            DeviceOrientation::LandscapeRight => SafeAreaInsets {
                top: bezels.top + 10.0,
                right: (bezels.right + cutout_top_inset).max(44.0), // Camera on right side
                bottom: bezels.bottom + 21.0,
                left: bezels.left + 20.0,
            },
            DeviceOrientation::PortraitInverted => SafeAreaInsets {
                top: bezels.top + nav_bottom_inset,
                right: bezels.right,
                bottom: cutout_top_inset,
                left: bezels.left,
            },
        }
    }
}
