//! Spring-Physics Driven Reactive Scalar Signals (`SpringSignal`).
//!
//! Provides second-order mass-spring-damper physical simulation driven by analytical
//! and sub-stepped numerical integration, binding directly into the fine-grained reactive graph.

use crate::reactive::Signal;
use std::cell::RefCell;
use std::rc::Rc;

/// Physical configuration parameters for a damped harmonic oscillator.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpringConfig {
    pub stiffness: f32, // Tension (k)
    pub damping: f32,   // Friction (c)
    pub mass: f32,      // Mass (m)
    pub rest_displacement_threshold: f32,
    pub rest_velocity_threshold: f32,
}

impl Default for SpringConfig {
    fn default() -> Self {
        Self {
            stiffness: 170.0,
            damping: 26.0,
            mass: 1.0,
            rest_displacement_threshold: 0.001,
            rest_velocity_threshold: 0.001,
        }
    }
}

impl SpringConfig {
    /// Crisp, snappy preset with minimal overshoot.
    pub const fn stiff() -> Self {
        Self {
            stiffness: 210.0,
            damping: 20.0,
            mass: 1.0,
            rest_displacement_threshold: 0.001,
            rest_velocity_threshold: 0.001,
        }
    }

    /// Soft, smooth deceleration curve.
    pub const fn gentle() -> Self {
        Self {
            stiffness: 120.0,
            damping: 14.0,
            mass: 1.0,
            rest_displacement_threshold: 0.001,
            rest_velocity_threshold: 0.001,
        }
    }

    /// Playful, expressive bounce with noticeable oscillation.
    pub const fn wobbly() -> Self {
        Self {
            stiffness: 180.0,
            damping: 12.0,
            mass: 1.0,
            rest_displacement_threshold: 0.001,
            rest_velocity_threshold: 0.001,
        }
    }

    /// High energy oscillatory spring.
    pub const fn bouncy() -> Self {
        Self {
            stiffness: 240.0,
            damping: 10.0,
            mass: 1.0,
            rest_displacement_threshold: 0.001,
            rest_velocity_threshold: 0.001,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct SpringState {
    position: f32,
    velocity: f32,
    target: f32,
}

/// Physics-driven reactive scalar signal.
///
/// Integrates a mass-spring-damper differential equation on each simulation tick,
/// notifying reactive subscribers whenever the physical position mutates.
#[derive(Clone)]
pub struct SpringSignal {
    config: SpringConfig,
    state: Rc<RefCell<SpringState>>,
    signal: Signal<f32>,
}

impl SpringSignal {
    /// Creates a new spring signal initialized at rest at `initial`.
    pub fn new(initial: f32) -> Self {
        Self::with_config(initial, SpringConfig::default())
    }

    /// Creates a new spring signal with customized spring parameters.
    pub fn with_config(initial: f32, config: SpringConfig) -> Self {
        Self {
            config,
            state: Rc::new(RefCell::new(SpringState {
                position: initial,
                velocity: 0.0,
                target: initial,
            })),
            signal: Signal::new(initial),
        }
    }

    /// Reactively reads the current physical position, registering dependency in reactive runtime.
    #[inline]
    pub fn get(&self) -> f32 {
        self.signal.get()
    }

    /// Reads current physical position without tracking reactive dependency.
    #[inline]
    pub fn untracked_get(&self) -> f32 {
        self.signal.untracked_get()
    }

    /// Returns the target equilibrium position.
    #[inline]
    pub fn target(&self) -> f32 {
        self.state.borrow().target
    }

    /// Returns the current physical velocity.
    #[inline]
    pub fn velocity(&self) -> f32 {
        self.state.borrow().velocity
    }

    /// Updates the equilibrium target position, waking up oscillation if resting.
    pub fn set_target(&self, target: f32) {
        let mut st = self.state.borrow_mut();
        st.target = target;
    }

    /// Immediately teleports position and target to `value` without oscillation.
    pub fn snap_to(&self, value: f32) {
        {
            let mut st = self.state.borrow_mut();
            st.position = value;
            st.target = value;
            st.velocity = 0.0;
        }
        self.signal.set(value);
    }

    /// Returns true if the spring is at rest within thresholds.
    pub fn is_at_rest(&self) -> bool {
        let st = self.state.borrow();
        (st.position - st.target).abs() < self.config.rest_displacement_threshold
            && st.velocity.abs() < self.config.rest_velocity_threshold
    }

    /// Advances the spring simulation by `dt` seconds using sub-stepped semi-implicit Euler integration.
    ///
    /// Returns `true` if the spring is still in motion, or `false` if it reached rest.
    pub fn tick(&self, dt: f32) -> bool {
        let mut st = self.state.borrow_mut();
        if (st.position - st.target).abs() < self.config.rest_displacement_threshold
            && st.velocity.abs() < self.config.rest_velocity_threshold
        {
            if st.position != st.target || st.velocity != 0.0 {
                st.position = st.target;
                st.velocity = 0.0;
                drop(st);
                self.signal.set(self.target());
            }
            return false;
        }

        // Sub-stepping for numerical stability
        const SUB_STEPS: usize = 4;
        let sub_dt = dt.clamp(0.0001, 0.1) / (SUB_STEPS as f32);

        for _ in 0..SUB_STEPS {
            let displacement = st.position - st.target;
            let spring_force = -self.config.stiffness * displacement;
            let damping_force = -self.config.damping * st.velocity;
            let total_force = spring_force + damping_force;
            let acceleration = total_force / self.config.mass.max(0.001);

            st.velocity += acceleration * sub_dt;
            st.position += st.velocity * sub_dt;
        }

        // Check if now reached equilibrium
        if (st.position - st.target).abs() < self.config.rest_displacement_threshold
            && st.velocity.abs() < self.config.rest_velocity_threshold
        {
            st.position = st.target;
            st.velocity = 0.0;
            let final_pos = st.position;
            drop(st);
            self.signal.set(final_pos);
            return false;
        }

        let new_pos = st.position;
        drop(st);
        self.signal.set(new_pos);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spring_signal_rest_convergence() {
        let spring = SpringSignal::new(0.0);
        assert_eq!(spring.get(), 0.0);
        assert!(spring.is_at_rest());

        spring.set_target(100.0);
        assert!(!spring.is_at_rest());
        assert_eq!(spring.target(), 100.0);

        // Advance simulation at 60 FPS
        let mut steps = 0;
        while steps < 200 && spring.tick(0.016) {
            steps += 1;
        }

        assert!(spring.is_at_rest(), "Spring must converge to rest");
        assert!((spring.get() - 100.0).abs() < 1e-3, "Position must reach target 100.0");
        assert_eq!(spring.velocity(), 0.0);
    }

    #[test]
    fn test_spring_signal_snap_to() {
        let spring = SpringSignal::new(10.0);
        spring.snap_to(500.0);

        assert_eq!(spring.get(), 500.0);
        assert_eq!(spring.target(), 500.0);
        assert_eq!(spring.velocity(), 0.0);
        assert!(spring.is_at_rest());
    }
}
