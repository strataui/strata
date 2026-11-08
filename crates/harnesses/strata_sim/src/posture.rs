//! Foldable hinge mechanics, continuous posture classification, and W3C Viewport Segments API.

use crate::profile::HingeAxis;
use strata_layout::Rect;

/// Discrete operational postures derived from continuous hinge angle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DevicePosture {
    /// Completely closed (0°..=5°). Outer cover display active; inner display powered down.
    Closed,
    /// Partial angle (6°..=59°). Transitional peek state.
    Peeking,
    /// Half-folded horizontal clamshell (60°..=125°). Rests upright on a surface.
    TabletopFlex,
    /// Inverted V-posture (60°..=125° upside down).
    Tent,
    /// Partially folded vertical book (60°..=125° along vertical axis).
    BookMode,
    /// Fully unfolded flat slate (126°..=180°). Expansive continuous display.
    Flat,
}

/// State and geometry of a foldable device display.
#[derive(Debug, Clone, PartialEq)]
pub struct FoldableState {
    pub hinge_angle_deg: f32,
    pub hinge_axis: HingeAxis,
    pub posture: DevicePosture,
    pub viewport_segments: Vec<Rect>,
    pub crease_width_pt: f32,
}

impl FoldableState {
    /// Constructs a new foldable state at the specified hinge angle.
    pub fn new(hinge_axis: HingeAxis, initial_angle_deg: f32, width: f32, height: f32) -> Self {
        let mut state = Self {
            hinge_angle_deg: initial_angle_deg.clamp(0.0, 180.0),
            hinge_axis,
            posture: DevicePosture::Flat,
            viewport_segments: Vec::new(),
            crease_width_pt: 4.0,
        };
        state.update_posture(false, width, height);
        state
    }

    /// Sets the hinge angle in degrees [0.0 ..= 180.0] and recomputes posture and viewport segments.
    pub fn set_hinge_angle(&mut self, angle: f32, is_inverted: bool, width: f32, height: f32) {
        self.hinge_angle_deg = angle.clamp(0.0, 180.0);
        self.update_posture(is_inverted, width, height);
    }

    /// Re-evaluates the discrete posture and bifurcated viewport segments.
    fn update_posture(&mut self, is_inverted: bool, width: f32, height: f32) {
        let angle = self.hinge_angle_deg;

        self.posture = if angle <= 5.0 {
            DevicePosture::Closed
        } else if angle <= 59.0 {
            DevicePosture::Peeking
        } else if angle <= 125.0 {
            if is_inverted {
                DevicePosture::Tent
            } else {
                match self.hinge_axis {
                    HingeAxis::Horizontal => DevicePosture::TabletopFlex,
                    HingeAxis::Vertical => DevicePosture::BookMode,
                }
            }
        } else {
            DevicePosture::Flat
        };

        self.compute_viewport_segments(width, height);
    }

    /// Computes discrete viewport segments according to W3C Foldables and Dual Screen specifications.
    fn compute_viewport_segments(&mut self, width: f32, height: f32) {
        self.viewport_segments.clear();

        match self.posture {
            DevicePosture::Closed => {
                // Outer cover display active
                self.viewport_segments.push(Rect::new(0.0, 0.0, width, height));
            }
            DevicePosture::TabletopFlex => {
                // Horizontal split: Upper presentation pane, Lower control pane
                let half_h = (height - self.crease_width_pt) / 2.0;
                let top_pane = Rect::new(0.0, 0.0, width, half_h);
                let bottom_pane = Rect::new(0.0, half_h + self.crease_width_pt, width, half_h);
                self.viewport_segments.push(top_pane);
                self.viewport_segments.push(bottom_pane);
            }
            DevicePosture::BookMode => {
                // Vertical split: Left page, Right page
                let half_w = (width - self.crease_width_pt) / 2.0;
                let left_pane = Rect::new(0.0, 0.0, half_w, height);
                let right_pane = Rect::new(half_w + self.crease_width_pt, 0.0, half_w, height);
                self.viewport_segments.push(left_pane);
                self.viewport_segments.push(right_pane);
            }
            DevicePosture::Flat | DevicePosture::Peeking | DevicePosture::Tent => {
                // Single unified continuous surface
                self.viewport_segments.push(Rect::new(0.0, 0.0, width, height));
            }
        }
    }

    /// Query W3C CSS environment variable: `env(viewport-segment-width x y)`
    pub fn env_segment_width(&self, x: usize, _y: usize) -> f32 {
        self.viewport_segments.get(x).map(|s| s.width).unwrap_or(0.0)
    }

    /// Query W3C CSS environment variable: `env(viewport-segment-height x y)`
    pub fn env_segment_height(&self, _x: usize, y: usize) -> f32 {
        self.viewport_segments.get(y).map(|s| s.height).unwrap_or(0.0)
    }

    /// Query W3C CSS environment variable: `env(viewport-segment-top x y)`
    pub fn env_segment_top(&self, _x: usize, y: usize) -> f32 {
        self.viewport_segments.get(y).map(|s| s.y).unwrap_or(0.0)
    }

    /// Query W3C CSS environment variable: `env(viewport-segment-left x y)`
    pub fn env_segment_left(&self, x: usize, _y: usize) -> f32 {
        self.viewport_segments.get(x).map(|s| s.x).unwrap_or(0.0)
    }

    /// Returns the active number of horizontal viewport segments (W3C Viewport Segments Level 1).
    pub fn horizontal_viewport_segments(&self) -> u32 {
        match self.posture {
            DevicePosture::BookMode => 2,
            _ => 1,
        }
    }

    /// Returns the active number of vertical viewport segments (W3C Viewport Segments Level 1).
    pub fn vertical_viewport_segments(&self) -> u32 {
        match self.posture {
            DevicePosture::TabletopFlex => 2,
            _ => 1,
        }
    }

    /// Maps the internal physical foldable posture to W3C CSS Device Posture Level 1.
    pub fn to_device_posture_query(&self) -> strata_style::DevicePostureQuery {
        match self.posture {
            DevicePosture::Flat | DevicePosture::Closed => {
                strata_style::DevicePostureQuery::Continuous
            }
            DevicePosture::TabletopFlex
            | DevicePosture::BookMode
            | DevicePosture::Tent
            | DevicePosture::Peeking => strata_style::DevicePostureQuery::Folded,
        }
    }

    /// Applies foldable viewport segments and posture to a CSS MediaContext.
    pub fn apply_to_media_context(&self, ctx: &mut strata_style::MediaContext) {
        ctx.horizontal_viewport_segments = self.horizontal_viewport_segments();
        ctx.vertical_viewport_segments = self.vertical_viewport_segments();
        ctx.device_posture = self.to_device_posture_query();
    }
}
