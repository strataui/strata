//! Verifies that the layout engine forwards the requested `font-family` atom to
//! the registered measurement callbacks instead of dropping it.

use std::sync::atomic::{AtomicU32, Ordering};

use strata_layout::box_model::inline_style_from_computed_style;
use strata_layout::typography::{
    InlineFormattingContext, InlineItem, InlineLayoutOptions, InlineTextStyle,
};
use strata_primitives::AtomId;

static LAST_FAMILY: AtomicU32 = AtomicU32::new(u32::MAX);

fn family_probe_measurer(
    _text: &str,
    _font_size: f32,
    _line_height_multiplier: f32,
    _max_width: Option<f32>,
    _is_monospace: bool,
    font_family: AtomId,
) -> (f32, f32) {
    LAST_FAMILY.store(font_family.0, Ordering::SeqCst);
    if strata_primitives::atom::resolve(font_family).as_deref() == Some("DejaVu Sans") {
        (700.0, 21.0)
    } else {
        (640.0, 21.0)
    }
}

#[test]
fn family_aware_measurer_receives_the_requested_atom() {
    strata_layout::text::register_family_text_measurer(family_probe_measurer);

    let dejavu = strata_primitives::atom::intern("DejaVu Sans");
    let measurer = strata_layout::text::get_family_text_measurer()
        .expect("family-aware measurer must be registered");

    let (family_width, _) = measurer("probe", 18.0, 1.0, None, false, dejavu);
    assert!(family_width > 0.0, "family measurement must be non-empty");
    assert_eq!(LAST_FAMILY.load(Ordering::SeqCst), dejavu.0);

    let (default_width, _) = measurer("probe", 18.0, 1.0, None, false, AtomId::FONT_SANS);
    assert!(default_width > 0.0, "default measurement must be non-empty");
    // The probe callback deliberately returns different advances per family so
    // the dispatch is observable on every host, independent of installed fonts.
    assert_ne!(
        family_width, default_width,
        "the family-aware path must not fall back to the default family width"
    );
}

#[test]
fn inline_token_width_uses_the_family_aware_path() {
    strata_layout::text::register_family_text_measurer(family_probe_measurer);

    let dejavu = strata_primitives::atom::intern("DejaVu Sans");
    let ifc = InlineFormattingContext::new();
    let items = vec![InlineItem::text(
        "probe",
        InlineTextStyle {
            font_family: dejavu,
            font_size: 18.0,
            ..Default::default()
        },
    )];
    let options = InlineLayoutOptions {
        font_size: 18.0,
        line_height: Some(21.0),
        max_width: None,
        ..Default::default()
    };

    let result = ifc.layout_inline_stream(&items, &options);
    let fragment = result.lines[0]
        .text_fragments
        .first()
        .expect("probe text must produce a fragment");

    assert_eq!(fragment.font_family, dejavu);
    assert!(
        (fragment.bounds.width - 700.0).abs() < 0.01,
        "expected the DejaVu Sans probe advance, got {}",
        fragment.bounds.width
    );
}

#[test]
fn computed_style_family_is_copied_into_inline_style() {
    let mut computed = strata_style::types::ComputedStyle::default();
    let dejavu = strata_primitives::atom::intern("DejaVu Sans");
    computed.font_family = dejavu;

    let inline = inline_style_from_computed_style(&computed);
    assert_eq!(inline.font_family, dejavu);
    assert!(!inline.is_monospace);

    assert_eq!(
        InlineTextStyle::default().font_family,
        AtomId::FONT_SANS,
        "the default inline style must use the pre-baked sans atom"
    );
}
