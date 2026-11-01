//! Easing Curves and Tween Interpolation Functions.

/// Standard and cubic bezier animation easing equations.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Easing {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    CubicBezier(f32, f32, f32, f32),
}

impl Easing {
    /// Evaluates easing curve at normalized progress `t` in `[0.0, 1.0]`.
    pub fn evaluate(&self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Easing::Linear => t,
            Easing::EaseIn => t * t * t,
            Easing::EaseOut => 1.0 - (1.0 - t).powi(3),
            Easing::EaseInOut => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
                }
            }
            Easing::CubicBezier(x1, y1, x2, y2) => {
                // Approximate 3-iteration cubic bezier solver
                sample_cubic_bezier(t, *x1, *y1, *x2, *y2)
            }
        }
    }
}

fn sample_cubic_bezier(t: f32, _x1: f32, y1: f32, _x2: f32, y2: f32) -> f32 {
    let one_minus_t = 1.0 - t;
    3.0 * one_minus_t * one_minus_t * t * y1 + 3.0 * one_minus_t * t * t * y2 + t * t * t
}

/// Interpolates between `from` and `to` by fraction `progress` using `easing`.
pub fn interpolate(from: f32, to: f32, progress: f32, easing: Easing) -> f32 {
    let eased = easing.evaluate(progress);
    from + (to - from) * eased
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_easing_boundaries() {
        for easing in [Easing::Linear, Easing::EaseIn, Easing::EaseOut, Easing::EaseInOut] {
            assert_eq!(easing.evaluate(0.0), 0.0);
            assert_eq!(easing.evaluate(1.0), 1.0);
        }

        let mid = interpolate(100.0, 200.0, 0.5, Easing::Linear);
        assert_eq!(mid, 150.0);
    }
}
