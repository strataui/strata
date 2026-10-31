//! Reactive Signal-Driven Motion Value.

use strata_dom::Signal;
use crate::spring::{SpringConfig, SpringState};

/// Reactive motion variable animated smoothly towards a moving target value.
#[derive(Clone)]
pub struct MotionValue {
    pub value: Signal<f32>,
    pub velocity: Signal<f32>,
    pub target: Signal<f32>,
    pub is_animating: Signal<bool>,
    pub spring_config: SpringConfig,
}

impl MotionValue {
    /// Creates a motion value initialized at `initial` with default spring dynamics.
    pub fn new(initial: f32) -> Self {
        Self::with_config(initial, SpringConfig::default())
    }

    /// Creates a motion value with customized spring parameters.
    pub fn with_config(initial: f32, spring_config: SpringConfig) -> Self {
        Self {
            value: Signal::new(initial),
            velocity: Signal::new(0.0),
            target: Signal::new(initial),
            is_animating: Signal::new(false),
            spring_config,
        }
    }

    /// Animates towards the new target value.
    pub fn animate_to(&self, new_target: f32) {
        if (new_target - self.target.get()).abs() > f32::EPSILON {
            self.target.set(new_target);
            self.is_animating.set(true);
        }
    }

    /// Instantly snaps the value to a position without animation.
    pub fn jump_to(&self, new_value: f32) {
        self.value.set(new_value);
        self.target.set(new_value);
        self.velocity.set(0.0);
        self.is_animating.set(false);
    }

    /// Advances the simulation by delta time `dt`. Returns `true` if animation has finished settling.
    pub fn tick(&self, dt: f32) -> bool {
        if !self.is_animating.get() {
            return true;
        }

        let mut state = SpringState {
            current: self.value.get(),
            velocity: self.velocity.get(),
            target: self.target.get(),
        };

        let settled = state.step(dt, &self.spring_config);
        self.value.set(state.current);
        self.velocity.set(state.velocity);

        if settled {
            self.is_animating.set(false);
        }

        settled
    }
}

/// Convenience hook to create a reactive motion value.
pub fn use_motion_value(initial: f32) -> MotionValue {
    MotionValue::new(initial)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_motion_value_reactive_target_tick() {
        let mv = use_motion_value(0.0);
        assert_eq!(mv.value.get(), 0.0);
        assert!(!mv.is_animating.get());

        mv.animate_to(50.0);
        assert!(mv.is_animating.get());
        assert_eq!(mv.target.get(), 50.0);

        // Step simulation
        let mut settled = false;
        for _ in 0..300 {
            if mv.tick(1.0 / 60.0) {
                settled = true;
                break;
            }
        }

        assert!(settled);
        assert!(!mv.is_animating.get());
        assert_eq!(mv.value.get(), 50.0);
    }
}
