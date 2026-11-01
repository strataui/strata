//! Device profile models, form factors, screen geometry, chassis definitions, and presets.

use strata_primitives::geom::EdgeInsets;

/// Device manufacturer brands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceManufacturer {
    Apple,
    Samsung,
    Google,
    Microsoft,
    Generic,
}

/// Standard mobile screen aspect ratios.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AspectRatio {
    Ratio19_5_9,
    Ratio20_9,
    Ratio16_9,
    Ratio6_5,
    Custom(f32, f32),
}

/// Physical chassis construction material.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChassisMaterial {
    BrushedTitanium,
    MatteAluminum,
    PolishedSteel,
    Ceramic,
}

/// Hardware side buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhysicalButton {
    Power,
    VolumeUp,
    VolumeDown,
    ActionButton,
    AlertSlider,
}

/// Screen display technical profile.
#[derive(Debug, Clone, PartialEq)]
pub struct ScreenProfile {
    pub logical_width: f32,
    pub logical_height: f32,
    pub scale_factor: f32,
    pub aspect_ratio: AspectRatio,
    pub inner_corner_radius: f32,
    pub subpixel_grid_snapping: bool,
}

impl ScreenProfile {
    pub fn new(width: f32, height: f32, scale: f32, ratio: AspectRatio, radius: f32) -> Self {
        Self {
            logical_width: width,
            logical_height: height,
            scale_factor: scale,
            aspect_ratio: ratio,
            inner_corner_radius: radius,
            subpixel_grid_snapping: true,
        }
    }
}

/// Physical phone chassis and bezel dimensions.
#[derive(Debug, Clone, PartialEq)]
pub struct ChassisProfile {
    pub outer_corner_radius: f32,
    pub bezel_insets: EdgeInsets<f32>,
    pub depth_mm: f32,
    pub material: ChassisMaterial,
    pub finish_color: [f32; 4],
    pub physical_buttons: Vec<PhysicalButton>,
}

/// Cutout alignment horizontally on the display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CutoutAlignment {
    Left,
    Center,
    Right,
}

/// Screen cutouts, camera punch-holes, and Dynamic Island configurations.
#[derive(Debug, Clone, PartialEq)]
pub enum CutoutProfile {
    None,
    TopNotch {
        width: f32,
        height: f32,
        corner_radius: f32,
    },
    Teardrop {
        width: f32,
        height: f32,
    },
    HolePunch {
        position: CutoutAlignment,
        diameter: f32,
        top_offset: f32,
    },
    DualHole {
        pill_width: f32,
        pill_height: f32,
        gap: f32,
    },
    DynamicIsland {
        resting_width: f32,
        resting_height: f32,
        top_offset: f32,
    },
}

/// Hinge orientation axis for foldable form factors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HingeAxis {
    Horizontal, // Clamshell / Flip
    Vertical,   // Book-style Foldable
}

/// Device form factor classification.
#[derive(Debug, Clone, PartialEq)]
pub enum DeviceFormFactor {
    Monolithic(ScreenProfile),
    ClamshellFlip {
        outer_cover: ScreenProfile,
        inner_unfolded: ScreenProfile,
        hinge_axis: HingeAxis,
        hinge_angle_deg: f32,
    },
    BookFoldable {
        outer_cover: ScreenProfile,
        inner_unfolded: ScreenProfile,
        hinge_axis: HingeAxis,
        hinge_angle_deg: f32,
    },
    DualScreen {
        left_screen: ScreenProfile,
        right_screen: ScreenProfile,
        seam_gap_pt: f32,
        hinge_angle_deg: f32,
    },
}

impl DeviceFormFactor {
    /// Returns the currently active screen profile based on form factor and fold state.
    pub fn active_screen(&self) -> &ScreenProfile {
        match self {
            Self::Monolithic(s) => s,
            Self::ClamshellFlip {
                outer_cover,
                inner_unfolded,
                hinge_angle_deg,
                ..
            } => {
                if *hinge_angle_deg <= 10.0 {
                    outer_cover
                } else {
                    inner_unfolded
                }
            }
            Self::BookFoldable {
                outer_cover,
                inner_unfolded,
                hinge_angle_deg,
                ..
            } => {
                if *hinge_angle_deg <= 10.0 {
                    outer_cover
                } else {
                    inner_unfolded
                }
            }
            Self::DualScreen { left_screen, .. } => left_screen,
        }
    }
}

/// Primary system navigation mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavigationType {
    Gesture,
    ThreeButton,
}

/// Complete mobile device blueprint.
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceProfile {
    pub name: String,
    pub manufacturer: DeviceManufacturer,
    pub form_factor: DeviceFormFactor,
    pub chassis: ChassisProfile,
    pub cutout: CutoutProfile,
    pub navigation_type: NavigationType,
    pub default_refresh_rate: f32,
}

impl DeviceProfile {
    /// Apple iPhone 16 Pro Max preset.
    pub fn iphone_16_pro_max() -> Self {
        let screen = ScreenProfile::new(440.0, 956.0, 3.0, AspectRatio::Ratio19_5_9, 55.0);
        Self {
            name: "iPhone 16 Pro Max".to_string(),
            manufacturer: DeviceManufacturer::Apple,
            form_factor: DeviceFormFactor::Monolithic(screen),
            chassis: ChassisProfile {
                outer_corner_radius: 58.0,
                bezel_insets: EdgeInsets::all(3.0),
                depth_mm: 8.25,
                material: ChassisMaterial::BrushedTitanium,
                finish_color: [0.18, 0.18, 0.19, 1.0], // Natural Titanium
                physical_buttons: vec![
                    PhysicalButton::Power,
                    PhysicalButton::VolumeUp,
                    PhysicalButton::VolumeDown,
                    PhysicalButton::ActionButton,
                ],
            },
            cutout: CutoutProfile::DynamicIsland {
                resting_width: 125.0,
                resting_height: 37.0,
                top_offset: 11.0,
            },
            navigation_type: NavigationType::Gesture,
            default_refresh_rate: 120.0,
        }
    }

    /// Samsung Galaxy S24 Ultra preset.
    pub fn galaxy_s24_ultra() -> Self {
        let screen = ScreenProfile::new(412.0, 919.0, 3.5, AspectRatio::Ratio19_5_9, 12.0);
        Self {
            name: "Galaxy S24 Ultra".to_string(),
            manufacturer: DeviceManufacturer::Samsung,
            form_factor: DeviceFormFactor::Monolithic(screen),
            chassis: ChassisProfile {
                outer_corner_radius: 14.0,
                bezel_insets: EdgeInsets::all(2.5),
                depth_mm: 8.6,
                material: ChassisMaterial::BrushedTitanium,
                finish_color: [0.12, 0.12, 0.13, 1.0], // Titanium Black
                physical_buttons: vec![
                    PhysicalButton::Power,
                    PhysicalButton::VolumeUp,
                    PhysicalButton::VolumeDown,
                ],
            },
            cutout: CutoutProfile::HolePunch {
                position: CutoutAlignment::Center,
                diameter: 11.0,
                top_offset: 8.0,
            },
            navigation_type: NavigationType::ThreeButton,
            default_refresh_rate: 120.0,
        }
    }

    /// Google Pixel 9 Pro preset.
    pub fn pixel_9_pro() -> Self {
        let screen = ScreenProfile::new(412.0, 923.0, 3.0, AspectRatio::Ratio20_9, 48.0);
        Self {
            name: "Pixel 9 Pro".to_string(),
            manufacturer: DeviceManufacturer::Google,
            form_factor: DeviceFormFactor::Monolithic(screen),
            chassis: ChassisProfile {
                outer_corner_radius: 52.0,
                bezel_insets: EdgeInsets::all(3.0),
                depth_mm: 8.5,
                material: ChassisMaterial::MatteAluminum,
                finish_color: [0.22, 0.23, 0.24, 1.0], // Obsidian
                physical_buttons: vec![
                    PhysicalButton::Power,
                    PhysicalButton::VolumeUp,
                    PhysicalButton::VolumeDown,
                ],
            },
            cutout: CutoutProfile::HolePunch {
                position: CutoutAlignment::Center,
                diameter: 12.0,
                top_offset: 10.0,
            },
            navigation_type: NavigationType::Gesture,
            default_refresh_rate: 120.0,
        }
    }

    /// Samsung Galaxy Z Fold 6 preset (Book-style foldable).
    pub fn galaxy_z_fold_6() -> Self {
        let cover = ScreenProfile::new(412.0, 908.0, 3.0, AspectRatio::Ratio20_9, 24.0);
        let inner = ScreenProfile::new(864.0, 768.0, 3.0, AspectRatio::Ratio6_5, 16.0);
        Self {
            name: "Galaxy Z Fold 6".to_string(),
            manufacturer: DeviceManufacturer::Samsung,
            form_factor: DeviceFormFactor::BookFoldable {
                outer_cover: cover,
                inner_unfolded: inner,
                hinge_axis: HingeAxis::Vertical,
                hinge_angle_deg: 180.0,
            },
            chassis: ChassisProfile {
                outer_corner_radius: 20.0,
                bezel_insets: EdgeInsets::all(3.2),
                depth_mm: 5.6,
                material: ChassisMaterial::MatteAluminum,
                finish_color: [0.2, 0.22, 0.25, 1.0], // Silver Shadow
                physical_buttons: vec![
                    PhysicalButton::Power,
                    PhysicalButton::VolumeUp,
                    PhysicalButton::VolumeDown,
                ],
            },
            cutout: CutoutProfile::HolePunch {
                position: CutoutAlignment::Center,
                diameter: 10.0,
                top_offset: 8.0,
            },
            navigation_type: NavigationType::Gesture,
            default_refresh_rate: 120.0,
        }
    }

    /// Samsung Galaxy Z Flip 6 preset (Clamshell / Flip phone).
    pub fn galaxy_z_flip_6() -> Self {
        let cover = ScreenProfile::new(360.0, 374.0, 2.0, AspectRatio::Ratio16_9, 32.0);
        let inner = ScreenProfile::new(412.0, 919.0, 3.0, AspectRatio::Ratio20_9, 36.0);
        Self {
            name: "Galaxy Z Flip 6".to_string(),
            manufacturer: DeviceManufacturer::Samsung,
            form_factor: DeviceFormFactor::ClamshellFlip {
                outer_cover: cover,
                inner_unfolded: inner,
                hinge_axis: HingeAxis::Horizontal,
                hinge_angle_deg: 180.0,
            },
            chassis: ChassisProfile {
                outer_corner_radius: 38.0,
                bezel_insets: EdgeInsets::all(3.0),
                depth_mm: 6.9,
                material: ChassisMaterial::MatteAluminum,
                finish_color: [0.18, 0.25, 0.28, 1.0], // Mint Blue
                physical_buttons: vec![
                    PhysicalButton::Power,
                    PhysicalButton::VolumeUp,
                    PhysicalButton::VolumeDown,
                ],
            },
            cutout: CutoutProfile::HolePunch {
                position: CutoutAlignment::Center,
                diameter: 10.0,
                top_offset: 8.0,
            },
            navigation_type: NavigationType::Gesture,
            default_refresh_rate: 120.0,
        }
    }

    /// Microsoft Surface Duo 2 preset (Dual discrete screens).
    pub fn surface_duo_2() -> Self {
        let screen = ScreenProfile::new(448.0, 672.0, 2.5, AspectRatio::Ratio16_9, 4.0);
        Self {
            name: "Surface Duo 2".to_string(),
            manufacturer: DeviceManufacturer::Microsoft,
            form_factor: DeviceFormFactor::DualScreen {
                left_screen: screen.clone(),
                right_screen: screen,
                seam_gap_pt: 18.0,
                hinge_angle_deg: 180.0,
            },
            chassis: ChassisProfile {
                outer_corner_radius: 8.0,
                bezel_insets: EdgeInsets::all(4.0),
                depth_mm: 5.5,
                material: ChassisMaterial::Ceramic,
                finish_color: [0.1, 0.1, 0.11, 1.0], // Glacier
                physical_buttons: vec![
                    PhysicalButton::Power,
                    PhysicalButton::VolumeUp,
                    PhysicalButton::VolumeDown,
                ],
            },
            cutout: CutoutProfile::None,
            navigation_type: NavigationType::ThreeButton,
            default_refresh_rate: 90.0,
        }
    }

    /// Returns a list of all official out-of-the-box presets.
    pub fn all_presets() -> Vec<Self> {
        vec![
            Self::iphone_16_pro_max(),
            Self::galaxy_s24_ultra(),
            Self::pixel_9_pro(),
            Self::galaxy_z_fold_6(),
            Self::galaxy_z_flip_6(),
            Self::surface_duo_2(),
        ]
    }
}
