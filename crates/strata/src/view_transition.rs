//! CSS View Transitions Module Level 1 & Level 2 snapshot and morphing engine.

use std::collections::HashMap;
use strata_primitives::geom::Rect;

/// A captured snapshot pairing old and new layout bounds for an element with `view-transition-name`.
#[derive(Debug, Clone, PartialEq)]
pub struct ViewTransitionSnapshot {
    pub name: String,
    pub old_bounds: Rect,
    pub new_bounds: Rect,
    pub start_time_ms: f64,
    pub duration_ms: f32,
    pub is_finished: bool,
}

/// Coordinates same-document view transitions, cross-fading and interpolating geometry between layout states.
#[derive(Debug, Clone, Default)]
pub struct ViewTransitionManager {
    pub active_transitions: HashMap<String, ViewTransitionSnapshot>,
    pub pending_old_snapshots: HashMap<String, Rect>,
}

impl ViewTransitionManager {
    /// Creates a new, empty `ViewTransitionManager`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Captures the current bounding boxes of all named view-transition elements before a DOM mutation.
    pub fn capture_old_state(&mut self, named_elements: &[(String, Rect)]) {
        for (name, rect) in named_elements {
            if name != "none" {
                self.pending_old_snapshots.insert(name.clone(), *rect);
            }
        }
    }

    /// Starts view transitions by pairing previously captured old bounds with newly computed layout rects.
    pub fn start_transition(&mut self, new_elements: &[(String, Rect)], now_ms: f64, duration_ms: f32) {
        for (name, new_rect) in new_elements {
            if let Some(old_rect) = self.pending_old_snapshots.remove(name) {
                self.active_transitions.insert(
                    name.clone(),
                    ViewTransitionSnapshot {
                        name: name.clone(),
                        old_bounds: old_rect,
                        new_bounds: *new_rect,
                        start_time_ms: now_ms,
                        duration_ms: duration_ms.max(10.0),
                        is_finished: false,
                    },
                );
            }
        }
        self.pending_old_snapshots.clear();
    }

    /// Advances active view transitions at timestamp `now_ms`. Returns true if any transition remains active.
    pub fn step(&mut self, now_ms: f64) -> bool {
        let mut active = false;
        for trans in self.active_transitions.values_mut() {
            let elapsed = (now_ms - trans.start_time_ms) as f32;
            if elapsed >= trans.duration_ms {
                trans.is_finished = true;
            } else {
                active = true;
            }
        }
        self.active_transitions.retain(|_, t| !t.is_finished);
        active
    }

    /// Samples current interpolated bounding rect and progress `t` in [0.0, 1.0] for a named transition.
    pub fn sample(&self, name: &str, now_ms: f64) -> Option<(Rect, f32)> {
        let trans = self.active_transitions.get(name)?;
        let elapsed = (now_ms - trans.start_time_ms).max(0.0) as f32;
        let t = (elapsed / trans.duration_ms).clamp(0.0, 1.0);
        let old_r = &trans.old_bounds;
        let new_r = &trans.new_bounds;
        let current_rect = Rect::from_xywh(
            old_r.origin.x + (new_r.origin.x - old_r.origin.x) * t,
            old_r.origin.y + (new_r.origin.y - old_r.origin.y) * t,
            old_r.size.width + (new_r.size.width - old_r.size.width) * t,
            old_r.size.height + (new_r.size.height - old_r.size.height) * t,
        );
        Some((current_rect, t))
    }

    /// Returns true if there are any active view transitions in progress.
    pub fn has_active_transitions(&self) -> bool {
        !self.active_transitions.is_empty()
    }
}
