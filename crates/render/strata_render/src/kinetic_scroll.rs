//! Kinetic Momentum Scrolling, Touch Physics, Rubber-Banding, and Scrollbar Geometry.
//!
//! Provides a 120 FPS inertial physics simulation with exponential velocity decay,
//! critically damped overscroll spring recovery, and draggable scrollbar state machines.

use strata_layout::Rect;

/// Deceleration constant $\lambda$ for exponential velocity decay ($v(t) = v_0 e^{-\lambda t}$).
pub const DEFAULT_FRICTION: f32 = 4.2;

/// Velocity cutoff threshold in pixels per second.
pub const REST_VELOCITY_THRESHOLD: f32 = 0.5;

/// Maximum rubber-band displacement limit in pixels.
pub const RUBBER_BAND_MAX_LIMIT: f32 = 120.0;

/// Damped harmonic spring stiffness coefficient for boundary snapback.
pub const SPRING_STIFFNESS: f32 = 280.0;

/// Damped harmonic spring damping coefficient (critically damped: $2 \sqrt{k \cdot m}$).
pub const SPRING_DAMPING: f32 = 33.5;

/// State machine for a single 1D scrolling dimension (horizontal or vertical).
#[derive(Debug, Clone, PartialEq)]
pub struct ScrollAxis {
    pub offset: f32,
    pub velocity: f32,
    pub content_size: f32,
    pub viewport_size: f32,
    pub is_dragging: bool,
    pub is_coasting: bool,
    pub is_bouncing_back: bool,
}

impl ScrollAxis {
    pub fn new(viewport_size: f32, content_size: f32) -> Self {
        Self {
            offset: 0.0,
            velocity: 0.0,
            content_size: content_size.max(0.0),
            viewport_size: viewport_size.max(0.0),
            is_dragging: false,
            is_coasting: false,
            is_bouncing_back: false,
        }
    }

    #[inline]
    pub fn max_scroll(&self) -> f32 {
        (self.content_size - self.viewport_size).max(0.0)
    }

    #[inline]
    pub fn can_scroll(&self) -> bool {
        self.content_size > self.viewport_size
    }

    #[inline]
    pub fn scroll_ratio(&self) -> f32 {
        let max = self.max_scroll();
        if max <= 0.0 {
            0.0
        } else {
            (self.offset / max).clamp(0.0, 1.0)
        }
    }

    /// Handles pointer drag movement with rubber-band tension scaling when past boundaries.
    pub fn handle_drag_delta(&mut self, delta: f32) {
        self.is_dragging = true;
        self.is_coasting = false;
        self.is_bouncing_back = false;

        let max = self.max_scroll();

        if self.offset < 0.0 && delta < 0.0 {
            // Already overscrolled at top and continuing to drag past top
            let overscroll = -self.offset;
            let resistance = (1.0 - (overscroll / RUBBER_BAND_MAX_LIMIT).min(0.95)).max(0.05);
            self.offset += delta * resistance;
        } else if self.offset > max && delta > 0.0 {
            // Already overscrolled at bottom and continuing to drag past bottom
            let overscroll = self.offset - max;
            let resistance = (1.0 - (overscroll / RUBBER_BAND_MAX_LIMIT).min(0.95)).max(0.05);
            self.offset += delta * resistance;
        } else {
            // Moving towards boundary or within valid range
            let target = self.offset + delta;
            if target < 0.0 {
                let overscroll = -target;
                let resistance = (1.0 - (overscroll / RUBBER_BAND_MAX_LIMIT).min(0.95)).max(0.05);
                self.offset = -overscroll * resistance;
            } else if target > max {
                let overscroll = target - max;
                let resistance = (1.0 - (overscroll / RUBBER_BAND_MAX_LIMIT).min(0.95)).max(0.05);
                self.offset = max + overscroll * resistance;
            } else {
                self.offset = target;
            }
        }
    }

    /// Applies an instantaneous fling or wheel impulse.
    pub fn apply_impulse(&mut self, delta_velocity: f32) {
        self.velocity += delta_velocity;
        self.is_coasting = true;
        self.is_bouncing_back = false;
    }

    /// Initiates drag tracking.
    pub fn start_drag(&mut self) {
        self.is_dragging = true;
        self.is_coasting = false;
        self.is_bouncing_back = false;
        self.velocity = 0.0;
    }

    /// Terminates pointer dragging, transitioning to overscroll spring or inertial coasting.
    pub fn end_drag(&mut self, release_velocity: f32) {
        self.is_dragging = false;
        let max = self.max_scroll();

        if self.offset < 0.0 || self.offset > max {
            self.is_bouncing_back = true;
            self.is_coasting = false;
            self.velocity = release_velocity;
        } else if release_velocity.abs() > REST_VELOCITY_THRESHOLD {
            self.is_coasting = true;
            self.is_bouncing_back = false;
            self.velocity = release_velocity;
        } else {
            self.is_coasting = false;
            self.is_bouncing_back = false;
            self.velocity = 0.0;
        }
    }

    /// Integrates physics simulation by delta time `dt` in seconds.
    pub fn tick(&mut self, dt: f32) {
        if self.is_dragging {
            return;
        }

        let max = self.max_scroll();

        if self.is_bouncing_back {
            let target = if self.offset < 0.0 { 0.0 } else { max };
            let displacement = self.offset - target;

            // Damped harmonic oscillator: F = -k * x - c * v
            let spring_force = -SPRING_STIFFNESS * displacement;
            let damping_force = -SPRING_DAMPING * self.velocity;
            let acceleration = spring_force + damping_force;

            self.velocity += acceleration * dt;
            self.offset += self.velocity * dt;

            // Rest condition
            if displacement.abs() < 0.25 && self.velocity.abs() < 1.0 {
                self.offset = target;
                self.velocity = 0.0;
                self.is_bouncing_back = false;
            }
        } else if self.is_coasting {
            self.offset += self.velocity * dt;

            // Exponential velocity friction decay: v(t) = v0 * exp(-lambda * dt)
            let decay = (-DEFAULT_FRICTION * dt).exp();
            self.velocity *= decay;

            // Boundary overshoot triggers snapback
            if self.offset < 0.0 || self.offset > max {
                self.is_coasting = false;
                self.is_bouncing_back = true;
            } else if self.velocity.abs() < REST_VELOCITY_THRESHOLD {
                self.velocity = 0.0;
                self.is_coasting = false;
            }
        }
    }

    #[inline]
    pub fn is_animating(&self) -> bool {
        self.is_coasting || self.is_bouncing_back
    }
}

/// Interactive 2D Kinetic Momentum Scroll Controller.
#[derive(Debug, Clone, PartialEq)]
pub struct KineticScrollController {
    pub x: ScrollAxis,
    pub y: ScrollAxis,
    pub last_interaction_time: f32,
    pub scrollbar_fade_delay: f32,
    pub scrollbar_fade_duration: f32,
}

impl KineticScrollController {
    pub fn new(
        viewport_width: f32,
        viewport_height: f32,
        content_width: f32,
        content_height: f32,
    ) -> Self {
        Self {
            x: ScrollAxis::new(viewport_width, content_width),
            y: ScrollAxis::new(viewport_height, content_height),
            last_interaction_time: 0.0,
            scrollbar_fade_delay: 1.2,
            scrollbar_fade_duration: 0.35,
        }
    }

    pub fn update_dimensions(
        &mut self,
        viewport_width: f32,
        viewport_height: f32,
        content_width: f32,
        content_height: f32,
    ) {
        self.x.viewport_size = viewport_width.max(0.0);
        self.x.content_size = content_width.max(0.0);
        self.y.viewport_size = viewport_height.max(0.0);
        self.y.content_size = content_height.max(0.0);
    }

    #[inline]
    pub fn scroll_offset(&self) -> (f32, f32) {
        (self.x.offset, self.y.offset)
    }

    #[inline]
    pub fn set_scroll_offset(&mut self, x: f32, y: f32) {
        self.x.offset = x;
        self.y.offset = y;
        self.x.velocity = 0.0;
        self.y.velocity = 0.0;
        self.x.is_coasting = false;
        self.y.is_coasting = false;
        self.x.is_bouncing_back = false;
        self.y.is_bouncing_back = false;
    }

    /// Handles pointer/touch drag start.
    pub fn on_pointer_down(&mut self, current_time: f32) {
        self.last_interaction_time = current_time;
        self.x.start_drag();
        self.y.start_drag();
    }

    /// Handles pointer/touch drag movement.
    pub fn on_pointer_move(&mut self, dx: f32, dy: f32, current_time: f32) {
        self.last_interaction_time = current_time;
        self.x.handle_drag_delta(dx);
        self.y.handle_drag_delta(dy);
    }

    /// Handles pointer/touch release with initial fling velocity (pixels/second).
    pub fn on_pointer_up(&mut self, vx: f32, vy: f32, current_time: f32) {
        self.last_interaction_time = current_time;
        self.x.end_drag(vx);
        self.y.end_drag(vy);
    }

    /// Handles wheel event delta.
    pub fn on_wheel(&mut self, dx: f32, dy: f32, current_time: f32) {
        self.last_interaction_time = current_time;
        if dx.abs() > 0.0 {
            self.x.apply_impulse(dx * 12.0);
        }
        if dy.abs() > 0.0 {
            self.y.apply_impulse(dy * 12.0);
        }
    }

    /// Advances simulation by time step `dt` (in seconds).
    pub fn tick(&mut self, dt: f32) {
        self.x.tick(dt);
        self.y.tick(dt);
    }

    #[inline]
    pub fn is_animating(&self) -> bool {
        self.x.is_animating() || self.y.is_animating()
    }

    /// Calculates scrollbar opacity [0.0, 1.0] with auto-fadeout after inactivity.
    pub fn scrollbar_opacity(&self, current_time: f32) -> f32 {
        if self.x.is_dragging || self.y.is_dragging || self.is_animating() {
            return 1.0;
        }

        let elapsed = current_time - self.last_interaction_time;
        if elapsed < self.scrollbar_fade_delay {
            1.0
        } else {
            let fade_elapsed = elapsed - self.scrollbar_fade_delay;
            (1.0 - (fade_elapsed / self.scrollbar_fade_duration)).clamp(0.0, 1.0)
        }
    }

    /// Computes the vertical scrollbar track and thumb bounding boxes.
    pub fn vertical_scrollbar_geometry(&self, track_width: f32, margin: f32) -> Option<(Rect, Rect)> {
        if !self.y.can_scroll() {
            return None;
        }

        let vw = self.x.viewport_size;
        let vh = self.y.viewport_size;
        let ch = self.y.content_size;

        let track_rect = Rect::new(vw - track_width - margin, margin, track_width, vh - margin * 2.0);
        let min_thumb_len = 24.0f32;
        let thumb_height = ((vh / ch) * track_rect.height).max(min_thumb_len).min(track_rect.height);
        let available_travel = track_rect.height - thumb_height;
        let thumb_y = track_rect.y + self.y.scroll_ratio() * available_travel;

        let thumb_rect = Rect::new(track_rect.x, thumb_y, track_width, thumb_height);
        Some((track_rect, thumb_rect))
    }

    /// Computes the horizontal scrollbar track and thumb bounding boxes.
    pub fn horizontal_scrollbar_geometry(&self, track_height: f32, margin: f32) -> Option<(Rect, Rect)> {
        if !self.x.can_scroll() {
            return None;
        }

        let vw = self.x.viewport_size;
        let vh = self.y.viewport_size;
        let cw = self.x.content_size;

        let track_rect = Rect::new(margin, vh - track_height - margin, vw - margin * 2.0, track_height);
        let min_thumb_len = 24.0f32;
        let thumb_width = ((vw / cw) * track_rect.width).max(min_thumb_len).min(track_rect.width);
        let available_travel = track_rect.width - thumb_width;
        let thumb_x = track_rect.x + self.x.scroll_ratio() * available_travel;

        let thumb_rect = Rect::new(thumb_x, track_rect.y, thumb_width, track_height);
        Some((track_rect, thumb_rect))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scroll_axis_basic_drag() {
        let mut axis = ScrollAxis::new(500.0, 2000.0);
        assert_eq!(axis.max_scroll(), 1500.0);
        assert_eq!(axis.offset, 0.0);

        // Drag downwards (content moves up)
        axis.handle_drag_delta(200.0);
        assert_eq!(axis.offset, 200.0);
        assert_eq!(axis.scroll_ratio(), 200.0 / 1500.0);

        // Drag past top boundary -> rubber-band resistance
        axis.handle_drag_delta(-300.0);
        assert!(axis.offset < 0.0);
        assert!(axis.offset > -100.0, "Rubber banding must resist: {}", axis.offset);
    }

    #[test]
    fn test_scroll_axis_inertia_decay() {
        let mut axis = ScrollAxis::new(500.0, 2000.0);
        axis.offset = 100.0;
        axis.end_drag(1000.0); // 1000 px/sec downwards fling

        assert!(axis.is_coasting);
        assert!(!axis.is_bouncing_back);

        // Step 16ms (60 FPS tick)
        axis.tick(0.016);
        assert!(axis.offset > 100.0);
        assert!(axis.velocity.abs() < 1000.0);

        // Step for 2 seconds -> should come to complete rest
        for _ in 0..120 {
            axis.tick(0.016);
        }
        assert!(!axis.is_coasting);
        assert_eq!(axis.velocity, 0.0);
    }

    #[test]
    fn test_scroll_axis_overscroll_spring_bounce() {
        let mut axis = ScrollAxis::new(500.0, 2000.0);
        axis.offset = -60.0; // Overscrolled 60px past top
        axis.end_drag(0.0);

        assert!(axis.is_bouncing_back);

        // Step harmonic oscillator to rest
        for _ in 0..100 {
            axis.tick(0.016);
        }

        assert_eq!(axis.offset, 0.0);
        assert!(!axis.is_bouncing_back);
    }

    #[test]
    fn test_scrollbar_geometry_calculation() {
        let mut controller = KineticScrollController::new(800.0, 600.0, 800.0, 2400.0);
        controller.set_scroll_offset(0.0, 900.0); // 50% through 1800px max scroll

        let (track, thumb) = controller.vertical_scrollbar_geometry(8.0, 4.0).unwrap();
        assert_eq!(track.x, 800.0 - 8.0 - 4.0);
        assert_eq!(thumb.width, 8.0);
        assert!(thumb.height >= 24.0);

        // Thumb should be roughly centered on the track
        let track_mid = track.y + track.height * 0.5;
        let thumb_mid = thumb.y + thumb.height * 0.5;
        assert!((track_mid - thumb_mid).abs() < 5.0);
    }
}
