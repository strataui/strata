//! Numerical Mass-Spring-Damper Physical Oscillator.

/// Configuration parameters for harmonic spring simulation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpringConfig {
    pub stiffness: f32,
    pub damping: f32,
    pub mass: f32,
    pub rest_speed: f32,
    pub rest_delta: f32,
}

impl Default for SpringConfig {
    fn default() -> Self {
        Self {
            stiffness: 170.0,
            damping: 26.0,
            mass: 1.0,
            rest_speed: 0.001,
            rest_delta: 0.001,
        }
    }
}

impl SpringConfig {
    pub fn gentle() -> Self {
        Self {
            stiffness: 120.0,
            damping: 14.0,
            mass: 1.0,
            ..Default::default()
        }
    }

    pub fn wobbly() -> Self {
        Self {
            stiffness: 180.0,
            damping: 12.0,
            mass: 1.0,
            ..Default::default()
        }
    }

    pub fn stiff() -> Self {
        Self {
            stiffness: 210.0,
            damping: 20.0,
            mass: 1.0,
            ..Default::default()
        }
    }

    pub fn slow() -> Self {
        Self {
            stiffness: 280.0,
            damping: 60.0,
            mass: 1.0,
            ..Default::default()
        }
    }
}

/// Instantaneous simulation state of a 1D harmonic oscillator.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpringState {
    pub current: f32,
    pub velocity: f32,
    pub target: f32,
}

impl SpringState {
    pub fn new(initial: f32) -> Self {
        Self {
            current: initial,
            velocity: 0.0,
            target: initial,
        }
    }

    /// Advances the spring simulation by delta time `dt` seconds using symplectic Euler integration.
    /// Returns `true` if the spring has settled at the target.
    pub fn step(&mut self, dt: f32, config: &SpringConfig) -> bool {
        let displacement = self.current - self.target;
        let spring_force = -config.stiffness * displacement;
        let damping_force = -config.damping * self.velocity;
        let acceleration = (spring_force + damping_force) / config.mass.max(0.001);

        self.velocity += acceleration * dt;
        self.current += self.velocity * dt;

        let speed = self.velocity.abs();
        let error = (self.current - self.target).abs();

        if speed <= config.rest_speed && error <= config.rest_delta {
            self.current = self.target;
            self.velocity = 0.0;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spring_oscillation_and_convergence() {
        let mut spring = SpringState::new(0.0);
        spring.target = 100.0;

        let config = SpringConfig::default();
        let dt = 1.0 / 60.0; // 60 FPS step

        let mut settled = false;
        for _ in 0..600 {
            if spring.step(dt, &config) {
                settled = true;
                break;
            }
        }

        assert!(settled);
        assert_eq!(spring.current, 100.0);
        assert_eq!(spring.velocity, 0.0);
    }
}
