//! Reactive Mathematical Coordinate Scales for Data Visualization.

/// Linear mathematical scale mapping a numeric continuous domain to a physical coordinate range.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScaleLinear {
    pub domain_min: f32,
    pub domain_max: f32,
    pub range_min: f32,
    pub range_max: f32,
}

impl ScaleLinear {
    pub fn new(domain: (f32, f32), range: (f32, f32)) -> Self {
        Self {
            domain_min: domain.0,
            domain_max: domain.1,
            range_min: range.0,
            range_max: range.1,
        }
    }

    /// Map a domain value to pixel coordinates.
    pub fn map(&self, value: f32) -> f32 {
        let span = self.domain_max - self.domain_min;
        if span.abs() < f32::EPSILON {
            return self.range_min;
        }
        let normalized = (value - self.domain_min) / span;
        self.range_min + normalized * (self.range_max - self.range_min)
    }

    /// Invert a pixel coordinate back to the domain value.
    pub fn invert(&self, coord: f32) -> f32 {
        let range_span = self.range_max - self.range_min;
        if range_span.abs() < f32::EPSILON {
            return self.domain_min;
        }
        let normalized = (coord - self.range_min) / range_span;
        self.domain_min + normalized * (self.domain_max - self.domain_min)
    }

    /// Generate evenly spaced grid tick values.
    pub fn ticks(&self, count: usize) -> Vec<f32> {
        let count = count.max(2);
        let step = (self.domain_max - self.domain_min) / (count - 1) as f32;
        (0..count).map(|i| self.domain_min + i as f32 * step).collect()
    }
}

/// Band scale mapping discrete categorical names to uniform slots with spacing gaps.
#[derive(Debug, Clone, PartialEq)]
pub struct ScaleBand {
    domain: Vec<String>,
    range_min: f32,
    range_max: f32,
    padding: f32,
}

impl ScaleBand {
    pub fn new(domain: Vec<String>, range: (f32, f32), padding: f32) -> Self {
        Self {
            domain,
            range_min: range.0,
            range_max: range.1,
            padding: padding.clamp(0.0, 1.0),
        }
    }

    pub fn bandwidth(&self) -> f32 {
        let n = self.domain.len();
        if n == 0 {
            return 0.0;
        }
        let total = self.range_max - self.range_min;
        let step = total / n as f32;
        step * (1.0 - self.padding)
    }

    pub fn map(&self, item: &str) -> Option<f32> {
        let idx = self.domain.iter().position(|d| d == item)?;
        let n = self.domain.len();
        let total = self.range_max - self.range_min;
        let step = total / n as f32;
        Some(self.range_min + idx as f32 * step + (step * self.padding / 2.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scale_linear_mapping() {
        // Map 0..100 to 0..500 px
        let scale = ScaleLinear::new((0.0, 100.0), (0.0, 500.0));
        assert_eq!(scale.map(50.0), 250.0);
        assert_eq!(scale.map(0.0), 0.0);
        assert_eq!(scale.map(100.0), 500.0);
        assert_eq!(scale.invert(250.0), 50.0);

        let ticks = scale.ticks(5);
        assert_eq!(ticks, vec![0.0, 25.0, 50.0, 75.0, 100.0]);
    }

    #[test]
    fn test_scale_band_mapping() {
        let domain = vec!["Mon".to_string(), "Tue".to_string(), "Wed".to_string()];
        let scale = ScaleBand::new(domain, (0.0, 300.0), 0.2);
        assert_eq!(scale.bandwidth(), 80.0);
        assert_eq!(scale.map("Mon"), Some(10.0));
        assert_eq!(scale.map("Tue"), Some(110.0));
    }
}
