//! Emulated MCP Cursor State and Animation Engine.
//!
//! Manages position interpolation, automatic pointer/text/default cursor styling,
//! click states, and the 2.5-second inactivity fade-out physics for AI agent interaction.

use std::time::Instant;
use crate::window::CursorIcon;

/// Emulated cursor driven by the Model Context Protocol (MCP) AI agent.
#[derive(Debug, Clone)]
pub struct McpCursorState {
    /// Current rendered X coordinate on the viewport surface.
    pub current_x: f32,
    /// Current rendered Y coordinate on the viewport surface.
    pub current_y: f32,
    /// Target destination X coordinate.
    pub target_x: f32,
    /// Target destination Y coordinate.
    pub target_y: f32,
    /// Starting X position before smooth animation began.
    pub start_x: f32,
    /// Starting Y position before smooth animation began.
    pub start_y: f32,
    /// Timestamp when current movement animation began.
    pub anim_start: Instant,
    /// Total duration of the smooth movement animation in milliseconds.
    pub anim_duration_ms: f32,
    /// Whether the cursor is currently interpolating between start and target.
    pub is_moving: bool,
    /// Timestamp of the latest interaction or move command.
    pub last_activity: Instant,
    /// Current visual opacity [0.0, 1.0] reflecting the 2.5s inactivity fade timer.
    pub fade_opacity: f32,
    /// Whether any mouse button is currently held down.
    pub is_down: bool,
    /// Active cursor icon shape resolved from the element underneath the pointer.
    pub cursor_icon: CursorIcon,
}

impl Default for McpCursorState {
    fn default() -> Self {
        Self::new(100.0, 100.0)
    }
}

impl McpCursorState {
    /// Creates a new emulated MCP cursor at the given initial viewport coordinates.
    pub fn new(x: f32, y: f32) -> Self {
        Self {
            current_x: x,
            current_y: y,
            target_x: x,
            target_y: y,
            start_x: x,
            start_y: y,
            anim_start: Instant::now(),
            anim_duration_ms: 180.0,
            is_moving: false,
            last_activity: Instant::now(),
            fade_opacity: 1.0,
            is_down: false,
            cursor_icon: CursorIcon::Default,
        }
    }

    /// Moves the emulated cursor towards `(target_x, target_y)`.
    ///
    /// If `smooth` is true (recommended by default), smoothly animates using cubic ease-out
    /// over a proportional duration. If `smooth` is false, jumps immediately to destination.
    pub fn move_to(&mut self, target_x: f32, target_y: f32, smooth: bool) {
        self.last_activity = Instant::now();
        self.fade_opacity = 1.0;

        if !smooth {
            self.current_x = target_x;
            self.current_y = target_y;
            self.target_x = target_x;
            self.target_y = target_y;
            self.is_moving = false;
        } else {
            self.start_x = self.current_x;
            self.start_y = self.current_y;
            self.target_x = target_x;
            self.target_y = target_y;
            self.anim_start = Instant::now();

            let dx = target_x - self.start_x;
            let dy = target_y - self.start_y;
            let dist = (dx * dx + dy * dy).sqrt();

            // Smooth dynamic duration: between 140ms and 280ms depending on travel distance
            self.anim_duration_ms = (dist * 0.35).clamp(140.0, 280.0);
            self.is_moving = true;
        }
    }

    /// Updates mouse button pressed state and resets the 2.5s inactivity timer.
    pub fn set_down(&mut self, is_down: bool) {
        self.is_down = is_down;
        self.last_activity = Instant::now();
        self.fade_opacity = 1.0;
    }

    /// Notifies the cursor of any interaction (e.g. typing, clicking) to keep it visible.
    pub fn touch_activity(&mut self) {
        self.last_activity = Instant::now();
        self.fade_opacity = 1.0;
    }

    /// Per-frame animation tick updating motion interpolation and the 2.5s inactivity fade.
    ///
    /// Returns true if cursor position or opacity changed and requires a redraw.
    pub fn poll_tick(&mut self) -> bool {
        let mut changed = false;

        // 1. Motion interpolation
        if self.is_moving {
            let elapsed = self.anim_start.elapsed().as_secs_f32() * 1000.0;
            let t = (elapsed / self.anim_duration_ms).clamp(0.0, 1.0);

            // Cubic ease-out curve: 1 - (1 - t)^3
            let ease = 1.0 - (1.0 - t).powi(3);

            self.current_x = self.start_x + (self.target_x - self.start_x) * ease;
            self.current_y = self.start_y + (self.target_y - self.start_y) * ease;
            changed = true;

            if t >= 1.0 {
                self.current_x = self.target_x;
                self.current_y = self.target_y;
                self.is_moving = false;
            }
        }

        // 2. Inactivity fade: after 2.5s with zero activity, fade out over 300ms
        let idle_secs = self.last_activity.elapsed().as_secs_f32();
        if idle_secs >= 2.5 {
            let fade_elapsed = idle_secs - 2.5;
            let fade_duration = 0.30; // 300ms fade duration
            let target_opacity = (1.0 - fade_elapsed / fade_duration).clamp(0.0, 1.0);
            if (self.fade_opacity - target_opacity).abs() > 0.005 {
                self.fade_opacity = target_opacity;
                changed = true;
            }
        } else if self.fade_opacity < 0.999 {
            self.fade_opacity = 1.0;
            changed = true;
        }

        changed
    }

    /// Returns true if the cursor has any visible opacity.
    pub fn is_visible(&self) -> bool {
        self.fade_opacity > 0.001
    }
}
