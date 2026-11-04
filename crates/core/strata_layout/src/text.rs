//! Intrinsic text dimension measurement, OpenType shaping, and line breaking facade.

use crate::typography::{
    InlineFormattingContext, InlineLayoutOptions, InlineLayoutResult, WhiteSpaceMode,
    VerticalAlignment,
};

/// Global text measurer callback type for exact platform font metrics measurement.
pub type TextMeasurementCallback = fn(&str, f32, f32, Option<f32>, bool) -> (f32, f32);

static TEXT_MEASURER: std::sync::atomic::AtomicPtr<()> = std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());

/// Registers a global platform text measurement callback (e.g. PangoCairo).
pub fn register_text_measurer(measurer: TextMeasurementCallback) {
    TEXT_MEASURER.store(measurer as *mut (), std::sync::atomic::Ordering::SeqCst);
}

/// Retrieves the active registered text measurer callback, if any.
pub fn get_text_measurer() -> Option<TextMeasurementCallback> {
    let ptr = TEXT_MEASURER.load(std::sync::atomic::Ordering::SeqCst);
    if ptr.is_null() {
        None
    } else {
        Some(unsafe { std::mem::transmute(ptr) })
    }
}

pub fn is_monospace_family(font_family: strata_primitives::AtomId) -> bool {
    if font_family == strata_primitives::AtomId::FONT_MONO {
        return true;
    }
    if let Some(family_str) = strata_primitives::atom::resolve(font_family) {
        family_str.contains("mono")
            || family_str.contains("monospace")
            || family_str.contains("Courier")
            || family_str.contains("Consolas")
            || family_str.contains("DejaVu Sans Mono")
    } else {
        false
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct MeasureCacheKey {
    text_hash: u64,
    text_len: usize,
    font_size_bits: u32,
    line_height_bits: u32,
    max_width_bits: u32,
    is_monospace: bool,
}

#[derive(Clone, Copy)]
struct MeasureCacheEntry {
    key: MeasureCacheKey,
    bounds: (f32, f32),
}

const CACHE_SIZE: usize = 1024;

thread_local! {
    static MEASURE_CACHE: std::cell::RefCell<[Option<MeasureCacheEntry>; CACHE_SIZE]> = const { std::cell::RefCell::new([None; CACHE_SIZE]) };
}

/// Measures intrinsic text bounds (width, height) using subpixel typography and line breaking.
pub fn measure_text_bounds(
    text: &str,
    font_size: f32,
    line_height_multiplier: f32,
    max_width: Option<f32>,
) -> (f32, f32) {
    measure_text_bounds_ext(text, font_size, line_height_multiplier, max_width, false)
}

/// Measures intrinsic text bounds with explicit monospace font flag.
pub fn measure_text_bounds_ext(
    text: &str,
    font_size: f32,
    line_height_multiplier: f32,
    max_width: Option<f32>,
    is_monospace: bool,
) -> (f32, f32) {
    if text.is_empty() {
        return (0.0, font_size * line_height_multiplier);
    }

    if let Some(measurer) = get_text_measurer() {
        return measurer(text, font_size, line_height_multiplier, max_width, is_monospace);
    }

    use std::hash::Hasher;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    hasher.write(text.as_bytes());
    let text_hash = hasher.finish();

    let font_size_bits = font_size.to_bits();
    let line_height_bits = line_height_multiplier.to_bits();
    let max_width_bits = max_width.map(|w| w.to_bits()).unwrap_or(u32::MAX);

    let key = MeasureCacheKey {
        text_hash,
        text_len: text.len(),
        font_size_bits,
        line_height_bits,
        max_width_bits,
        is_monospace,
    };

    let mono_bit = if is_monospace { 1u64 } else { 0u64 };
    let idx = ((text_hash ^ (font_size_bits as u64) ^ ((max_width_bits as u64) << 16) ^ (mono_bit << 24)) as usize) % CACHE_SIZE;

    let cached = MEASURE_CACHE.with(|cache| {
        let cell = cache.borrow();
        if let Some(entry) = &cell[idx] {
            if entry.key == key {
                return Some(entry.bounds);
            }
        }
        None
    });

    if let Some(bounds) = cached {
        return bounds;
    }

    let ifc = InlineFormattingContext::new();
    let explicit_lh = font_size * line_height_multiplier;
    let options = InlineLayoutOptions {
        font_size,
        line_height: Some(explicit_lh),
        max_width,
        white_space: WhiteSpaceMode::Normal,
        vertical_align: VerticalAlignment::Baseline,
        is_monospace,
        exclusion_space: None,
        bfc_offset_y: 0.0,
    };

    let result = ifc.layout_text(text, &options);
    let single_line_height = font_size * line_height_multiplier;
    let total_height = result.line_count.max(1) as f32 * single_line_height;
    let bounds = (result.total_width, total_height);

    MEASURE_CACHE.with(|cache| {
        let mut cell = cache.borrow_mut();
        cell[idx] = Some(MeasureCacheEntry { key, bounds });
    });

    bounds
}

/// Advanced text layout returning detailed line boxes, glyph runs, baselines, and geometry.
pub fn layout_inline_text(text: &str, options: &InlineLayoutOptions) -> InlineLayoutResult {
    let ifc = InlineFormattingContext::new();
    ifc.layout_text(text, options)
}
