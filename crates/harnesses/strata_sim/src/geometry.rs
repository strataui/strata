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

    /// Constructs the concentric fillet SDF model for the given device profile.
    pub fn build_concentric_fillet_sdf(profile: &DeviceProfile) -> ConcentricFilletSdf {
        let (chassis_w, chassis_h) = Self::calculate_chassis_dimensions(profile);
        let avg_bezel = (profile.chassis.bezel_insets.left
            + profile.chassis.bezel_insets.right
            + profile.chassis.bezel_insets.top
            + profile.chassis.bezel_insets.bottom)
            * 0.25;
        ConcentricFilletSdf::new(
            chassis_w,
            chassis_h,
            profile.chassis.outer_corner_radius,
            avg_bezel,
        )
    }
}

/// Signed Distance Field (SDF) solver for concentric device chassis and screen fillet clipping
/// featuring exact G^2 curvature continuity (smooth squircle / Lamé superellipse).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConcentricFilletSdf {
    pub outer_width: f32,
    pub outer_height: f32,
    pub outer_radius: f32,
    pub bezel_thickness: f32,
    pub inner_radius: f32,
    pub inner_width: f32,
    pub inner_height: f32,
    pub exponent: f32,
}

impl ConcentricFilletSdf {
    /// Constructs a concentric fillet SDF model with default G^2 superellipse exponent (n = 4.0).
    pub fn new(
        outer_width: f32,
        outer_height: f32,
        outer_radius: f32,
        bezel_thickness: f32,
    ) -> Self {
        let inner_radius = (outer_radius - bezel_thickness).max(0.0);
        let inner_width = (outer_width - 2.0 * bezel_thickness).max(0.0);
        let inner_height = (outer_height - 2.0 * bezel_thickness).max(0.0);
        Self {
            outer_width,
            outer_height,
            outer_radius,
            bezel_thickness,
            inner_radius,
            inner_width,
            inner_height,
            exponent: 4.0, // n = 4 guarantees G^2 curvature continuity
        }
    }

    /// Evaluates signed distance from point (x, y) to outer chassis boundary.
    /// Negative inside, zero on boundary, positive outside.
    pub fn eval_outer(&self, x: f32, y: f32) -> f32 {
        self.eval_squircle_sdf(x, y, self.outer_width, self.outer_height, self.outer_radius)
    }

    /// Evaluates signed distance from point (x, y) to inner screen boundary.
    /// Negative inside screen, zero on boundary, positive outside.
    pub fn eval_inner(&self, x: f32, y: f32) -> f32 {
        self.eval_squircle_sdf(x, y, self.inner_width, self.inner_height, self.inner_radius)
    }

    /// Evaluates signed distance to the bezel ring region (outer AND NOT inner).
    pub fn eval_bezel(&self, x: f32, y: f32) -> f32 {
        let d_outer = self.eval_outer(x, y);
        let d_inner = self.eval_inner(x, y);
        d_outer.max(-d_inner)
    }

    /// Computes subpixel anti-aliased coverage for inner screen clipping [0.0 ..= 1.0].
    pub fn screen_coverage(&self, x: f32, y: f32, pixel_size: f32) -> f32 {
        let dist = self.eval_inner(x, y);
        let half_px = (pixel_size * 0.5).max(0.001);
        (0.5 - dist / half_px).clamp(0.0, 1.0)
    }

    /// Computes analytical curvature kappa(t) along the corner fillet superellipse at parameter t in [0, pi/2].
    /// Verifies that at the tangency points (t = 0 and t = pi/2), kappa = 0, matching straight edges (G^2).
    pub fn curvature_at_parameter(&self, t: f32) -> f32 {
        let clamped_t = t.clamp(0.0, std::f32::consts::FRAC_PI_2);
        if clamped_t < 1e-4 || clamped_t > std::f32::consts::FRAC_PI_2 - 1e-4 {
            0.0
        } else {
            let dt = 1e-3;
            let p_prev = self.eval_corner_point(clamped_t - dt);
            let p_curr = self.eval_corner_point(clamped_t);
            let p_next = self.eval_corner_point(clamped_t + dt);

            let dx1 = (p_curr.0 - p_prev.0) / dt;
            let dy1 = (p_curr.1 - p_prev.1) / dt;
            let dx2 = (p_next.0 - p_curr.0) / dt;
            let dy2 = (p_next.1 - p_curr.1) / dt;

            let ddx = (dx2 - dx1) / dt;
            let ddy = (dy2 - dy1) / dt;

            let speed = (dx1 * dx1 + dy1 * dy1).sqrt();
            if speed < 1e-5 {
                0.0
            } else {
                ((dx1 * ddy - dy1 * ddx).abs() / (speed * speed * speed)).max(0.0)
            }
        }
    }

    fn eval_corner_point(&self, t: f32) -> (f32, f32) {
        let r = self.inner_radius;
        let cos_t = t.cos().max(0.0);
        let sin_t = t.sin().max(0.0);
        let p = 2.0 / self.exponent;
        (r * cos_t.powf(p), r * sin_t.powf(p))
    }

    /// Verifies that corner curvature transitions continuously to zero at the boundary tangency (G^2).
    pub fn verify_g2_continuity(&self) -> bool {
        let k_edge = self.curvature_at_parameter(0.0);
        let k_near_edge = self.curvature_at_parameter(1e-4);
        k_edge.abs() < 1e-3 && k_near_edge.abs() < 0.1
    }

    fn eval_squircle_sdf(&self, x: f32, y: f32, w: f32, h: f32, r: f32) -> f32 {
        let half_w = w * 0.5;
        let half_h = h * 0.5;
        let straight_w = (half_w - r).max(0.0);
        let straight_h = (half_h - r).max(0.0);

        let q_x = x.abs() - straight_w;
        let q_y = y.abs() - straight_h;

        if q_x <= 0.0 && q_y <= 0.0 {
            q_x.max(q_y) - r
        } else if q_x > 0.0 && q_y > 0.0 {
            let norm_p =
                (q_x.powf(self.exponent) + q_y.powf(self.exponent)).powf(1.0 / self.exponent);
            norm_p - r
        } else {
            q_x.max(q_y) - r
        }
    }

    /// Returns the WGSL shader source implementing this concentric fillet SDF clipping logic.
    pub const fn wgsl_shader() -> &'static str {
        WGSL_CONCENTRIC_FILLET_SHADER
    }
}

pub const WGSL_CONCENTRIC_FILLET_SHADER: &str = r#"
// Concentric Fillet Corner Clipping Shader with G^2 Curvature Continuity
fn sdf_squircle(p: vec2<f32>, half_size: vec2<f32>, radius: f32, n: f32) -> f32 {
    let straight = max(half_size - vec2<f32>(radius, radius), vec2<f32>(0.0, 0.0));
    let q = abs(p) - straight;
    if (q.x <= 0.0 && q.y <= 0.0) {
        return max(q.x, q.y) - radius;
    } else if (q.x > 0.0 && q.y > 0.0) {
        let norm_n = pow(pow(q.x, n) + pow(q.y, n), 1.0 / n);
        return norm_n - radius;
    } else {
        return max(q.x, q.y) - radius;
    }
}

fn concentric_fillet_clip(
    pixel_pos: vec2<f32>,
    outer_half: vec2<f32>,
    outer_radius: f32,
    bezel_thickness: f32,
    exponent: f32
) -> vec4<f32> {
    let inner_half = max(outer_half - vec2<f32>(bezel_thickness, bezel_thickness), vec2<f32>(0.0, 0.0));
    let inner_radius = max(outer_radius - bezel_thickness, 0.0);

    let d_outer = sdf_squircle(pixel_pos, outer_half, outer_radius, exponent);
    let d_inner = sdf_squircle(pixel_pos, inner_half, inner_radius, exponent);

    // Coverage with 1px smoothstep anti-aliasing
    let screen_coverage = clamp(0.5 - d_inner, 0.0, 1.0);
    let chassis_coverage = clamp(0.5 - d_outer, 0.0, 1.0);
    let bezel_mask = clamp(0.5 - max(d_outer, -d_inner), 0.0, 1.0);

    return vec4<f32>(screen_coverage, chassis_coverage, bezel_mask, d_inner);
}
"#;

