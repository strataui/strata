//! Font metrics, FontHeight envelope, and CSS half-leading baseline mathematics.
//!
//! Implements Blink LayoutNG font metrics and CSS 2.1 Section 10.8 line height calculations.

use strata_primitives::geom::LayoutUnit;

/// Font metrics extracted from OpenType / TrueType font tables (HHEA, OS/2, HEAD).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FontMetrics {
    /// Distance from baseline to highest ascender.
    pub ascent: LayoutUnit,
    /// Distance from baseline to lowest descender (positive scalar).
    pub descent: LayoutUnit,
    /// Typographic line gap suggested by font designer.
    pub line_gap: LayoutUnit,
    /// Height of lowercase letter 'x'.
    pub x_height: LayoutUnit,
    /// Height of uppercase letter 'H'.
    pub cap_height: LayoutUnit,
    /// Font design grid resolution (typically 1000 or 2048).
    pub units_per_em: u16,
    /// Resolved font size in CSS pixels.
    pub font_size: f32,
}

impl FontMetrics {
    /// Creates font metrics with proportional defaults for standard modern sans-serif fonts (e.g., Inter, Roboto).
    pub fn default_sans(font_size: f32) -> Self {
        let ascent = LayoutUnit::from_f32(font_size * 0.82);
        let descent = LayoutUnit::from_f32(font_size * 0.22);
        let line_gap = LayoutUnit::from_f32(font_size * 0.06);
        let x_height = LayoutUnit::from_f32(font_size * 0.52);
        let cap_height = LayoutUnit::from_f32(font_size * 0.72);

        Self {
            ascent,
            descent,
            line_gap,
            x_height,
            cap_height,
            units_per_em: 1000,
            font_size,
        }
    }

    /// Creates font metrics for developer monospace fonts (e.g., JetBrains Mono, Fira Code).
    pub fn default_mono(font_size: f32) -> Self {
        let ascent = LayoutUnit::from_f32(font_size * 0.85);
        let descent = LayoutUnit::from_f32(font_size * 0.25);
        let line_gap = LayoutUnit::from_f32(font_size * 0.0);
        let x_height = LayoutUnit::from_f32(font_size * 0.54);
        let cap_height = LayoutUnit::from_f32(font_size * 0.74);

        Self {
            ascent,
            descent,
            line_gap,
            x_height,
            cap_height,
            units_per_em: 1000,
            font_size,
        }
    }

    /// Natural font height without added leading: ascent + descent.
    #[inline(always)]
    pub fn natural_height(&self) -> LayoutUnit {
        self.ascent + self.descent
    }

    /// Chromium standard default line height when line-height: normal (~1.2x font size).
    #[inline(always)]
    pub fn default_line_height(&self) -> LayoutUnit {
        self.natural_height() + self.line_gap
    }
}

/// Represents vertical ascent and descent bounds for inline items, boxes, and line boxes.
/// Mirrors Chromium's `FontHeight` in `third_party/blink/renderer/platform/fonts/font_height.h`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct FontHeight {
    pub ascent: LayoutUnit,
    pub descent: LayoutUnit,
}

impl FontHeight {
    pub const EMPTY: Self = Self {
        ascent: LayoutUnit(i32::MIN),
        descent: LayoutUnit(i32::MIN),
    };

    pub const ZERO: Self = Self {
        ascent: LayoutUnit::ZERO,
        descent: LayoutUnit::ZERO,
    };

    #[inline(always)]
    pub fn new(ascent: LayoutUnit, descent: LayoutUnit) -> Self {
        Self { ascent, descent }
    }

    #[inline(always)]
    pub fn from_metrics(metrics: &FontMetrics) -> Self {
        Self {
            ascent: metrics.ascent,
            descent: metrics.descent,
        }
    }

    #[inline(always)]
    pub fn is_empty(self) -> bool {
        self.ascent.0 == i32::MIN && self.descent.0 == i32::MIN
    }

    /// Total vertical height: ascent + descent.
    #[inline(always)]
    pub fn line_height(self) -> LayoutUnit {
        if self.is_empty() {
            LayoutUnit::ZERO
        } else {
            self.ascent + self.descent
        }
    }

    /// Add half-leading above and below.
    #[inline(always)]
    pub fn add_leading(&mut self, leading: FontHeight) {
        if !self.is_empty() {
            self.ascent = self.ascent + leading.ascent;
            self.descent = self.descent + leading.descent;
        }
    }

    /// Move bounds vertically in line coordinates.
    #[inline(always)]
    pub fn move_by(&mut self, delta: LayoutUnit) {
        if !self.is_empty() {
            self.ascent = self.ascent - delta;
            self.descent = self.descent + delta;
        }
    }

    /// Unite bounds to encompass another inline box.
    #[inline(always)]
    pub fn unite(&mut self, other: FontHeight) {
        if other.is_empty() {
            return;
        }
        if self.is_empty() {
            *self = other;
            return;
        }
        if other.ascent > self.ascent {
            self.ascent = other.ascent;
        }
        if other.descent > self.descent {
            self.descent = other.descent;
        }
    }
}

/// Computes CSS 2.1 Section 10.8 half-leading above and below for a given font and target line-height.
///
/// Leading = line_height - (ascent + descent)
/// Half-leading above = floor(leading / 2)
/// Half-leading below = leading - half-leading above
pub fn compute_half_leading(
    line_height: LayoutUnit,
    ascent: LayoutUnit,
    descent: LayoutUnit,
) -> (LayoutUnit, LayoutUnit) {
    let font_height = ascent + descent;
    let leading = line_height - font_height;
    let leading_above = LayoutUnit(leading.0 / 2);
    let leading_below = leading - leading_above;
    (leading_above, leading_below)
}

/// Imaginary zero-width strut establishing baseline and default line-box height in an IFC.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CssStrut {
    pub metrics: FontHeight,
    pub line_height: LayoutUnit,
}

impl CssStrut {
    pub fn new(font_metrics: &FontMetrics, explicit_line_height: Option<f32>) -> Self {
        let line_height = match explicit_line_height {
            Some(lh) if lh > 0.0 => LayoutUnit::from_f32(lh),
            _ => font_metrics.default_line_height(),
        };

        let (leading_above, leading_below) = compute_half_leading(
            line_height,
            font_metrics.ascent,
            font_metrics.descent,
        );

        let metrics = FontHeight::new(
            font_metrics.ascent + leading_above,
            font_metrics.descent + leading_below,
        );

        Self {
            metrics,
            line_height,
        }
    }
}
