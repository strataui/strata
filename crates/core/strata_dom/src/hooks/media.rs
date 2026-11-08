//! Media query and system preference hooks (responsive queries, color scheme, reduced motion, contrast).

use std::cell::RefCell;
use crate::reactive::Signal;

thread_local! {
    static VIEWPORT_DIMENSIONS: RefCell<Signal<(f32, f32)>> = RefCell::new(Signal::new((1920.0, 1080.0)));
    static COLOR_SCHEME: RefCell<Signal<ColorScheme>> = RefCell::new(Signal::new(ColorScheme::Dark));
    static REDUCED_MOTION: RefCell<Signal<bool>> = RefCell::new(Signal::new(false));
    static CONTRAST_PREFERENCE: RefCell<Signal<ContrastPreference>> = RefCell::new(Signal::new(ContrastPreference::Normal));
    static ACTIVE_MEDIA_QUERIES: RefCell<Vec<(Vec<MediaCondition>, Signal<bool>)>> = const { RefCell::new(Vec::new()) };
}

/// Preferred system color scheme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorScheme {
    Light,
    #[default]
    Dark,
}

/// Accessibility contrast preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ContrastPreference {
    #[default]
    Normal,
    High,
    Low,
}

/// Helper function to re-evaluate all active media queries when environment changes.
fn reevaluate_active_queries() {
    let (width, height) = VIEWPORT_DIMENSIONS.with(|d| d.borrow().get());
    let cs = COLOR_SCHEME.with(|cs| cs.borrow().get());
    let rm = REDUCED_MOTION.with(|rm| rm.borrow().get());

    ACTIVE_MEDIA_QUERIES.with(|queries| {
        for (conditions, sig) in queries.borrow().iter() {
            let matched = conditions.iter().all(|c| c.evaluate(width, height, cs, rm));
            sig.set(matched);
        }
    });
}

/// Sets the current viewport dimensions (width, height) in device pixels and updates media queries.
pub fn set_viewport_dimensions(width: f32, height: f32) {
    VIEWPORT_DIMENSIONS.with(|dims| {
        dims.borrow().set((width, height));
    });
    reevaluate_active_queries();
}

/// Updates the active system color scheme and re-evaluates queries.
pub fn set_system_color_scheme(scheme: ColorScheme) {
    COLOR_SCHEME.with(|cs| {
        cs.borrow().set(scheme);
    });
    reevaluate_active_queries();
}

/// Updates the active reduced motion system preference and re-evaluates queries.
pub fn set_system_reduced_motion(reduced: bool) {
    REDUCED_MOTION.with(|rm| {
        rm.borrow().set(reduced);
    });
    reevaluate_active_queries();
}

/// Updates the active contrast system preference.
pub fn set_system_contrast(contrast: ContrastPreference) {
    CONTRAST_PREFERENCE.with(|cp| {
        cp.borrow().set(contrast);
    });
}

/// Headless hook returning the active system color scheme (`ColorScheme::Light` or `ColorScheme::Dark`).
pub fn use_color_scheme() -> Signal<ColorScheme> {
    COLOR_SCHEME.with(|cs| cs.borrow().clone())
}

/// Headless hook tracking whether user has requested reduced motion (`prefers-reduced-motion: reduce`).
pub fn use_reduced_motion() -> Signal<bool> {
    REDUCED_MOTION.with(|rm| rm.borrow().clone())
}

/// Headless hook tracking the system contrast preference (`prefers-contrast`).
pub fn use_contrast() -> Signal<ContrastPreference> {
    CONTRAST_PREFERENCE.with(|cp| cp.borrow().clone())
}

/// Parsed CSS media query condition.
#[derive(Debug, Clone, PartialEq)]
enum MediaCondition {
    MinWidth(f32),
    MaxWidth(f32),
    MinHeight(f32),
    MaxHeight(f32),
    OrientationPortrait,
    OrientationLandscape,
    PrefersColorScheme(ColorScheme),
    PrefersReducedMotion(bool),
}

impl MediaCondition {
    fn evaluate(&self, width: f32, height: f32, color_scheme: ColorScheme, reduced_motion: bool) -> bool {
        match self {
            Self::MinWidth(min) => width >= *min,
            Self::MaxWidth(max) => width <= *max,
            Self::MinHeight(min) => height >= *min,
            Self::MaxHeight(max) => height <= *max,
            Self::OrientationPortrait => height >= width,
            Self::OrientationLandscape => width > height,
            Self::PrefersColorScheme(cs) => color_scheme == *cs,
            Self::PrefersReducedMotion(rm) => reduced_motion == *rm,
        }
    }
}

/// Parses a simple CSS media query string (e.g. `(min-width: 768px)`, `(max-width: 1024px)`).
fn parse_media_query(query: &str) -> Vec<MediaCondition> {
    let mut conditions = Vec::new();
    let cleaned = query.trim().trim_start_matches('@').trim_start_matches("media").trim();

    for part in cleaned.split("and") {
        let p = part.trim().trim_matches(|c| c == '(' || c == ')').trim();
        if let Some((prop, val)) = p.split_once(':') {
            let prop_name = prop.trim().to_ascii_lowercase();
            let val_str = val.trim().to_ascii_lowercase();
            let val_clean = val_str.trim_end_matches("px").trim();

            match prop_name.as_str() {
                "min-width" => {
                    if let Ok(num) = val_clean.parse::<f32>() {
                        conditions.push(MediaCondition::MinWidth(num));
                    }
                }
                "max-width" => {
                    if let Ok(num) = val_clean.parse::<f32>() {
                        conditions.push(MediaCondition::MaxWidth(num));
                    }
                }
                "min-height" => {
                    if let Ok(num) = val_clean.parse::<f32>() {
                        conditions.push(MediaCondition::MinHeight(num));
                    }
                }
                "max-height" => {
                    if let Ok(num) = val_clean.parse::<f32>() {
                        conditions.push(MediaCondition::MaxHeight(num));
                    }
                }
                "orientation" => {
                    if val_str == "portrait" {
                        conditions.push(MediaCondition::OrientationPortrait);
                    } else if val_str == "landscape" {
                        conditions.push(MediaCondition::OrientationLandscape);
                    }
                }
                "prefers-color-scheme" => {
                    if val_str == "light" {
                        conditions.push(MediaCondition::PrefersColorScheme(ColorScheme::Light));
                    } else if val_str == "dark" {
                        conditions.push(MediaCondition::PrefersColorScheme(ColorScheme::Dark));
                    }
                }
                "prefers-reduced-motion" => {
                    if val_str == "reduce" {
                        conditions.push(MediaCondition::PrefersReducedMotion(true));
                    } else if val_str == "no-preference" {
                        conditions.push(MediaCondition::PrefersReducedMotion(false));
                    }
                }
                _ => {}
            }
        }
    }

    conditions
}

/// Headless hook evaluating a CSS media query against the current reactive viewport and preferences.
pub fn use_media_query(query: &str) -> Signal<bool> {
    let conditions = parse_media_query(query);
    let (w, h) = VIEWPORT_DIMENSIONS.with(|d| d.borrow().get());
    let cs = COLOR_SCHEME.with(|cs| cs.borrow().get());
    let rm = REDUCED_MOTION.with(|rm| rm.borrow().get());

    let initial_match = conditions.iter().all(|c| c.evaluate(w, h, cs, rm));
    let result_sig = Signal::new(initial_match);

    ACTIVE_MEDIA_QUERIES.with(|queries| {
        queries.borrow_mut().push((conditions, result_sig.clone()));
    });

    result_sig
}
