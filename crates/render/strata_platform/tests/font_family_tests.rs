#![cfg(target_os = "linux")]

use strata_primitives::AtomId;

#[test]
fn resolves_first_family_and_strips_quotes() {
    let family = strata_primitives::atom::intern("\"DejaVu Sans\", sans-serif");
    assert_eq!(
        strata_platform::resolved_font_family(family, false),
        "DejaVu Sans"
    );

    let single_quoted = strata_primitives::atom::intern("'DejaVu Sans Mono'");
    assert_eq!(
        strata_platform::resolved_font_family(single_quoted, true),
        "DejaVu Sans Mono"
    );
}

#[test]
fn maps_generic_families_to_pinned_host_faces() {
    let generic_sans = strata_primitives::atom::intern("system-ui, sans-serif");
    assert_eq!(
        strata_platform::resolved_font_family(generic_sans, false),
        "DejaVu Sans"
    );

    let generic_mono = strata_primitives::atom::intern("monospace");
    assert_eq!(
        strata_platform::resolved_font_family(generic_mono, true),
        "DejaVu Sans Mono"
    );

    let serif = strata_primitives::atom::intern("serif");
    assert_eq!(strata_platform::resolved_font_family(serif, false), "serif");
}

#[test]
fn dejavu_sans_18px_metrics_match_pango() {
    strata_platform::init_pango_text_measurement();
    let dejavu = strata_primitives::atom::intern("DejaVu Sans");
    let (ascent, descent, line_gap) = strata_platform::pango_font_metrics(dejavu, 18.0, false);

    assert!(
        (ascent - 17.0).abs() <= 1.0,
        "expected DejaVu Sans 18px ascent near 17, got {ascent}"
    );
    assert!(
        (descent - 5.0).abs() <= 1.0,
        "expected DejaVu Sans 18px descent near 5, got {descent}"
    );
    assert!(line_gap >= 0.0, "line gap must not be negative");
}

#[test]
fn empty_and_missing_atoms_use_pinned_defaults() {
    assert_eq!(
        strata_platform::resolved_font_family(AtomId::NULL, false),
        "DejaVu Sans"
    );
    assert_eq!(
        strata_platform::resolved_font_family(AtomId::NULL, true),
        "DejaVu Sans Mono"
    );
}
