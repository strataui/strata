//! Declarative CSS @keyframes animation engine, cubic-bezier easing solver, and property interpolation.

/// Parametric cubic-bezier timing curve P1(x1, y1), P2(x2, y2).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CubicBezier {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
}

impl CubicBezier {
    pub const fn new(x1: f32, y1: f32, x2: f32, y2: f32) -> Self {
        Self { x1, y1, x2, y2 }
    }

    pub const fn linear() -> Self {
        Self::new(0.0, 0.0, 1.0, 1.0)
    }

    pub const fn ease() -> Self {
        Self::new(0.25, 0.1, 0.25, 1.0)
    }

    pub const fn ease_in() -> Self {
        Self::new(0.42, 0.0, 1.0, 1.0)
    }

    pub const fn ease_out() -> Self {
        Self::new(0.0, 0.0, 0.58, 1.0)
    }

    pub const fn ease_in_out() -> Self {
        Self::new(0.42, 0.0, 0.58, 1.0)
    }

    /// Solves for y given x in [0.0, 1.0] using De Casteljau's algorithm and root finding.
    #[inline(always)]
    pub fn solve(&self, x: f32) -> f32 {
        crate::bezier::cubic_bezier_solve(x, self.x1, self.y1, self.x2, self.y2)
    }
}

/// An animatable style property.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum KeyframeProperty {
    Opacity(f32),
    TranslateX(f32),
    TranslateY(f32),
    Scale(f32),
    RotateDeg(f32),
    Width(f32),
    Height(f32),
}

impl KeyframeProperty {
    pub fn property_id(&self) -> u8 {
        match self {
            Self::Opacity(_) => 0,
            Self::TranslateX(_) => 1,
            Self::TranslateY(_) => 2,
            Self::Scale(_) => 3,
            Self::RotateDeg(_) => 4,
            Self::Width(_) => 5,
            Self::Height(_) => 6,
        }
    }

    pub fn value(&self) -> f32 {
        match *self {
            Self::Opacity(v)
            | Self::TranslateX(v)
            | Self::TranslateY(v)
            | Self::Scale(v)
            | Self::RotateDeg(v)
            | Self::Width(v)
            | Self::Height(v) => v,
        }
    }

    pub fn with_value(&self, new_val: f32) -> Self {
        match self {
            Self::Opacity(_) => Self::Opacity(new_val),
            Self::TranslateX(_) => Self::TranslateX(new_val),
            Self::TranslateY(_) => Self::TranslateY(new_val),
            Self::Scale(_) => Self::Scale(new_val),
            Self::RotateDeg(_) => Self::RotateDeg(new_val),
            Self::Width(_) => Self::Width(new_val),
            Self::Height(_) => Self::Height(new_val),
        }
    }
}

/// A single keyframe point in a CSS animation timeline.
#[derive(Debug, Clone, PartialEq)]
pub struct Keyframe {
    /// Offset fraction along the timeline (0.0 to 1.0).
    pub offset: f32,
    /// Properties specified at this keyframe.
    pub properties: Vec<KeyframeProperty>,
}

impl Keyframe {
    pub fn new(offset: f32, properties: Vec<KeyframeProperty>) -> Self {
        Self { offset, properties }
    }
}

/// Complete definition of a named CSS @keyframes animation.
#[derive(Debug, Clone, PartialEq)]
pub struct KeyframeAnimation {
    pub name: String,
    pub duration_seconds: f32,
    pub timing_function: CubicBezier,
    pub keyframes: Vec<Keyframe>,
    pub iteration_count: Option<u32>, // None for infinite
}

impl KeyframeAnimation {
    pub fn new(
        name: impl Into<String>,
        duration_seconds: f32,
        timing_function: CubicBezier,
        mut keyframes: Vec<Keyframe>,
    ) -> Self {
        keyframes.sort_by(|a, b| {
            a.offset
                .partial_cmp(&b.offset)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Self {
            name: name.into(),
            duration_seconds,
            timing_function,
            keyframes,
            iteration_count: Some(1),
        }
    }

    /// Evaluates and samples animated properties at a given elapsed time.
    pub fn sample(&self, elapsed_seconds: f32) -> Vec<KeyframeProperty> {
        if self.keyframes.is_empty() || self.duration_seconds <= 0.0 {
            return Vec::new();
        }

        let raw_t = elapsed_seconds / self.duration_seconds;
        let t = match self.iteration_count {
            Some(count) if count > 0 => {
                if raw_t >= count as f32 {
                    1.0
                } else {
                    raw_t % 1.0
                }
            }
            _ => raw_t % 1.0,
        };

        let progress = self.timing_function.solve(t);

        // Find bounding keyframes
        let first = &self.keyframes[0];
        let last = &self.keyframes[self.keyframes.len() - 1];

        if progress <= first.offset {
            return first.properties.clone();
        }
        if progress >= last.offset {
            return last.properties.clone();
        }

        let mut k1 = first;
        let mut k2 = last;

        for i in 0..self.keyframes.len() - 1 {
            if self.keyframes[i].offset <= progress && progress <= self.keyframes[i + 1].offset {
                k1 = &self.keyframes[i];
                k2 = &self.keyframes[i + 1];
                break;
            }
        }

        let span = k2.offset - k1.offset;
        let alpha = if span > 0.0 {
            (progress - k1.offset) / span
        } else {
            0.0
        };

        let mut result = Vec::new();

        for prop1 in &k1.properties {
            let pid = prop1.property_id();
            if let Some(prop2) = k2.properties.iter().find(|p| p.property_id() == pid) {
                let v1 = prop1.value();
                let v2 = prop2.value();
                let interpolated = v1 + (v2 - v1) * alpha;
                result.push(prop1.with_value(interpolated));
            } else {
                result.push(*prop1);
            }
        }

        result
    }
}
