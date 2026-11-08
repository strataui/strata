//! Intrinsic text dimension measurement, OpenType shaping, and line breaking facade.

use crate::typography::{
    InlineFormattingContext, InlineLayoutOptions, InlineLayoutResult, VerticalAlignment,
    WhiteSpaceMode,
};

/// Global text measurer callback type for exact platform font metrics measurement.
pub type TextMeasurementCallback = fn(&str, f32, f32, Option<f32>, bool) -> (f32, f32);

/// Global styled text measurer callback for exact font metrics measurement with weight and style.
pub type StyledTextMeasurementCallback = fn(
    &str,
    f32,
    f32,
    Option<f32>,
    bool,
    strata_style::types::FontWeight,
    strata_style::types::FontStyle,
) -> (f32, f32);

/// Global text measurer callback that also receives the requested font family atom.
pub type FamilyTextMeasurementCallback =
    fn(&str, f32, f32, Option<f32>, bool, strata_primitives::AtomId) -> (f32, f32);

/// Global styled text measurer callback that also receives the requested font family atom.
pub type FamilyStyledTextMeasurementCallback = fn(
    &str,
    f32,
    f32,
    Option<f32>,
    bool,
    strata_primitives::AtomId,
    strata_style::types::FontWeight,
    strata_style::types::FontStyle,
) -> (f32, f32);

/// Global font metrics callback returning `(ascent_px, descent_px, line_gap_px)`.
pub type FontMetricsCallback = fn(strata_primitives::AtomId, f32, bool) -> (f32, f32, f32);

static TEXT_MEASURER: std::sync::RwLock<Option<TextMeasurementCallback>> =
    std::sync::RwLock::new(None);
static STYLED_TEXT_MEASURER: std::sync::RwLock<Option<StyledTextMeasurementCallback>> =
    std::sync::RwLock::new(None);
static FAMILY_TEXT_MEASURER: std::sync::RwLock<Option<FamilyTextMeasurementCallback>> =
    std::sync::RwLock::new(None);
static FAMILY_STYLED_TEXT_MEASURER: std::sync::RwLock<Option<FamilyStyledTextMeasurementCallback>> =
    std::sync::RwLock::new(None);
static FONT_METRICS: std::sync::RwLock<Option<FontMetricsCallback>> =
    std::sync::RwLock::new(None);
static FONT_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// Returns the current font configuration generation. Caches key on this value
/// so registering new platform measurers invalidates every measurement.
pub fn font_generation() -> u64 {
    FONT_GENERATION.load(std::sync::atomic::Ordering::Relaxed)
}

fn bump_font_generation() {
    FONT_GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

/// Registers a global platform text measurement callback (e.g. PangoCairo).
pub fn register_text_measurer(measurer: TextMeasurementCallback) {
    if let Ok(mut lock) = TEXT_MEASURER.write() {
        *lock = Some(measurer);
    }
    bump_font_generation();
}

/// Registers a global platform styled text measurement callback (e.g. PangoCairo).
pub fn register_styled_text_measurer(measurer: StyledTextMeasurementCallback) {
    if let Ok(mut lock) = STYLED_TEXT_MEASURER.write() {
        *lock = Some(measurer);
    }
    bump_font_generation();
}

/// Registers a global platform text measurement callback that is family aware.
pub fn register_family_text_measurer(measurer: FamilyTextMeasurementCallback) {
    if let Ok(mut lock) = FAMILY_TEXT_MEASURER.write() {
        *lock = Some(measurer);
    }
    bump_font_generation();
}

/// Registers a global platform styled text measurement callback that is family aware.
pub fn register_family_styled_text_measurer(measurer: FamilyStyledTextMeasurementCallback) {
    if let Ok(mut lock) = FAMILY_STYLED_TEXT_MEASURER.write() {
        *lock = Some(measurer);
    }
    bump_font_generation();
}

/// Registers a global platform font metrics callback (ascent, descent, line gap).
pub fn register_font_metrics(metrics: FontMetricsCallback) {
    if let Ok(mut lock) = FONT_METRICS.write() {
        *lock = Some(metrics);
    }
    bump_font_generation();
}

/// Retrieves the active registered text measurer callback, if any.
pub fn get_text_measurer() -> Option<TextMeasurementCallback> {
    TEXT_MEASURER.read().ok().and_then(|lock| *lock)
}

/// Retrieves the active registered styled text measurer callback, if any.
pub fn get_styled_text_measurer() -> Option<StyledTextMeasurementCallback> {
    STYLED_TEXT_MEASURER.read().ok().and_then(|lock| *lock)
}

/// Retrieves the active registered family-aware text measurer callback, if any.
pub fn get_family_text_measurer() -> Option<FamilyTextMeasurementCallback> {
    FAMILY_TEXT_MEASURER.read().ok().and_then(|lock| *lock)
}

/// Retrieves the active registered family-aware styled text measurer callback, if any.
pub fn get_family_styled_text_measurer() -> Option<FamilyStyledTextMeasurementCallback> {
    FAMILY_STYLED_TEXT_MEASURER.read().ok().and_then(|lock| *lock)
}

/// Retrieves the active registered font metrics callback, if any.
pub fn get_font_metrics() -> Option<FontMetricsCallback> {
    FONT_METRICS.read().ok().and_then(|lock| *lock)
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

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct StyledMeasureKey {
    text_hash: u64,
    text_len: usize,
    font_size_bits: u32,
    line_height_bits: u32,
    max_width_bits: u32,
    flags: u32,
    font_family: u32,
    font_weight: u8,
    font_style: u8,
    generation: u64,
}

const STYLED_CACHE_LIMIT: usize = 16_384;

thread_local! {
    static MEASURE_CACHE: std::cell::RefCell<[Option<MeasureCacheEntry>; CACHE_SIZE]> = const { std::cell::RefCell::new([None; CACHE_SIZE]) };
    static STYLED_MEASURE_CACHE: std::cell::RefCell<std::collections::HashMap<StyledMeasureKey, (f32, f32)>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

fn hash_text(text: &str) -> u64 {
    use std::hash::Hasher;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    hasher.write(text.as_bytes());
    hasher.finish()
}

#[allow(clippy::too_many_arguments)]
fn styled_measure_key(
    text_hash: u64,
    text_len: usize,
    font_size: f32,
    line_height_multiplier: f32,
    max_width: Option<f32>,
    is_monospace: bool,
    font_family: Option<strata_primitives::AtomId>,
    font_weight: strata_style::types::FontWeight,
    font_style: strata_style::types::FontStyle,
) -> StyledMeasureKey {
    let flags = (is_monospace as u32) | ((max_width.is_some() as u32) << 1);
    StyledMeasureKey {
        text_hash,
        text_len,
        font_size_bits: font_size.to_bits(),
        line_height_bits: line_height_multiplier.to_bits(),
        max_width_bits: max_width.map(|w| w.to_bits()).unwrap_or(u32::MAX),
        flags,
        font_family: font_family.map(|f| f.0).unwrap_or(u32::MAX),
        font_weight: font_weight as u8,
        font_style: font_style as u8,
        generation: font_generation(),
    }
}

fn styled_cache_get(key: &StyledMeasureKey) -> Option<(f32, f32)> {
    STYLED_MEASURE_CACHE.with(|cache| cache.borrow().get(key).copied())
}

fn styled_cache_put(key: StyledMeasureKey, bounds: (f32, f32)) {
    STYLED_MEASURE_CACHE.with(|cache| {
        let mut map = cache.borrow_mut();
        if map.len() >= STYLED_CACHE_LIMIT {
            map.clear();
        }
        map.insert(key, bounds);
    });
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

/// Measures intrinsic text bounds honoring the CSS `white-space` mode.
///
/// `white-space: nowrap` and the pre* modes must not soft-wrap, so the
/// measurement pass has to receive the same line-breaking mode as the final
/// inline layout. Without this, a `truncate` element is measured as multiple
/// lines and then painted as a single one.
pub fn measure_text_bounds_with_white_space(
    text: &str,
    font_size: f32,
    line_height_multiplier: f32,
    max_width: Option<f32>,
    is_monospace: bool,
    white_space: strata_style::types::WhiteSpace,
) -> (f32, f32) {
    if text.is_empty() {
        return (0.0, font_size * line_height_multiplier);
    }
    let mode = match white_space {
        strata_style::types::WhiteSpace::Normal => WhiteSpaceMode::Normal,
        strata_style::types::WhiteSpace::NoWrap => WhiteSpaceMode::NoWrap,
        strata_style::types::WhiteSpace::Pre => WhiteSpaceMode::Pre,
        strata_style::types::WhiteSpace::PreWrap => WhiteSpaceMode::PreWrap,
        strata_style::types::WhiteSpace::PreLine => WhiteSpaceMode::PreLine,
    };
    let ifc = InlineFormattingContext::new();
    let options = InlineLayoutOptions {
        font_size,
        line_height: Some(font_size * line_height_multiplier),
        max_width,
        white_space: mode,
        vertical_align: VerticalAlignment::Baseline,
        is_monospace,
        exclusion_space: None,
        bfc_offset_y: 0.0,
    };
    let result = ifc.layout_text(text, &options);
    let line_h = font_size * line_height_multiplier;
    let total_height = result.line_count.max(1) as f32 * line_h;
    (result.total_width, total_height)
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
        let key = styled_measure_key(
            hash_text(text),
            text.len(),
            font_size,
            line_height_multiplier,
            max_width,
            is_monospace,
            None,
            strata_style::types::FontWeight::Normal,
            strata_style::types::FontStyle::Normal,
        );
        if let Some(hit) = styled_cache_get(&key) {
            return hit;
        }
        let bounds = measurer(
            text,
            font_size,
            line_height_multiplier,
            max_width,
            is_monospace,
        );
        styled_cache_put(key, bounds);
        return bounds;
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
    let idx =
        ((text_hash ^ (font_size_bits as u64) ^ ((max_width_bits as u64) << 16) ^ (mono_bit << 24))
            as usize)
            % CACHE_SIZE;

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

/// Measures intrinsic text bounds while requesting a specific font family.
///
/// Prefers the registered family-aware measurer so caret and selection metrics
/// match the physical face used for paint. Falls back to the legacy measurement
/// path when no family-aware measurer is registered.
pub fn measure_text_bounds_with_family(
    text: &str,
    font_size: f32,
    line_height_multiplier: f32,
    max_width: Option<f32>,
    is_monospace: bool,
    font_family: strata_primitives::AtomId,
) -> (f32, f32) {
    if text.is_empty() {
        return (0.0, font_size * line_height_multiplier);
    }

    if let Some(measurer) = get_family_text_measurer() {
        let key = styled_measure_key(
            hash_text(text),
            text.len(),
            font_size,
            line_height_multiplier,
            max_width,
            is_monospace,
            Some(font_family),
            strata_style::types::FontWeight::Normal,
            strata_style::types::FontStyle::Normal,
        );
        if let Some(hit) = styled_cache_get(&key) {
            return hit;
        }
        let bounds = measurer(
            text,
            font_size,
            line_height_multiplier,
            max_width,
            is_monospace,
            font_family,
        );
        styled_cache_put(key, bounds);
        return bounds;
    }

    measure_text_bounds_ext(
        text,
        font_size,
        line_height_multiplier,
        max_width,
        is_monospace,
    )
}

/// Measures intrinsic text bounds with family, weight, and style information.
///
/// Prefers the registered family-aware styled measurer (e.g. PangoCairo) so
/// intrinsic sizing does not have to run a full inline shaping pass.
#[allow(clippy::too_many_arguments)]
pub fn measure_text_bounds_styled_with_family(
    text: &str,
    font_size: f32,
    line_height_multiplier: f32,
    max_width: Option<f32>,
    is_monospace: bool,
    font_family: strata_primitives::AtomId,
    font_weight: strata_style::types::FontWeight,
    font_style: strata_style::types::FontStyle,
) -> (f32, f32) {
    if text.is_empty() {
        return (0.0, font_size * line_height_multiplier);
    }

    if let Some(measurer) = get_family_styled_text_measurer() {
        let key = styled_measure_key(
            hash_text(text),
            text.len(),
            font_size,
            line_height_multiplier,
            max_width,
            is_monospace,
            Some(font_family),
            font_weight,
            font_style,
        );
        if let Some(hit) = styled_cache_get(&key) {
            return hit;
        }
        let bounds = measurer(
            text,
            font_size,
            line_height_multiplier,
            max_width,
            is_monospace,
            font_family,
            font_weight,
            font_style,
        );
        styled_cache_put(key, bounds);
        return bounds;
    }

    if let Some(font) =
        crate::typography::opentype::lookup_font_for_style(font_family, font_weight, font_style)
    {
        let key = styled_measure_key(
            hash_text(text),
            text.len(),
            font_size,
            line_height_multiplier,
            max_width,
            is_monospace,
            Some(font_family),
            font_weight,
            font_style,
        );
        if let Some(hit) = styled_cache_get(&key) {
            return hit;
        }

        let mut total_w = 0.0f32;
        let mut prev_gid: Option<u16> = None;
        for c in text.chars() {
            let gid = font.glyph_id(c);
            let adv = font.glyph_advance(gid, font_size);
            if let Some(pg) = prev_gid {
                let kern = font.pair_kerning(pg, gid, font_size);
                total_w += kern;
            }
            total_w += adv;
            prev_gid = Some(gid);
        }

        let total_h = font_size * line_height_multiplier;
        let bounds = (total_w, total_h);
        styled_cache_put(key, bounds);
        return bounds;
    }

    measure_text_bounds_with_family(
        text,
        font_size,
        line_height_multiplier,
        max_width,
        is_monospace,
        font_family,
    )
}

/// Advanced text layout returning detailed line boxes, glyph runs, baselines, and geometry.
pub fn layout_inline_text(text: &str, options: &InlineLayoutOptions) -> InlineLayoutResult {
    let ifc = InlineFormattingContext::new();
    ifc.layout_text(text, options)
}
