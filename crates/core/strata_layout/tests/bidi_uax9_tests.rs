//! Comprehensive W3C / UAX #9 Unicode Bidirectional Algorithm integration tests.

use strata_layout::typography::{bidi_mirror, BidiClass, BidiEngine};

#[test]
fn test_uax9_directional_embeddings_and_overrides() {
    let bidi = BidiEngine::new();

    // RLE (Right-to-Left Embedding): \u{202B} raises embedding level to 1; Latin characters inside resolve to level 2 per Rule I2
    let text_with_rle = "English \u{202B}embedded latin\u{202C} English";
    let rle_runs = bidi.resolve_runs(text_with_rle);
    assert!(rle_runs.len() >= 3);
    let embedded_run = &rle_runs[1];
    assert_eq!(embedded_run.level, 2, "Rule I2: L characters in odd embedding level resolve to level 2");

    // RLO (Right-to-Left Override): \u{202E} overrides characters to R, resolving to odd level 1 (RTL)
    let text_with_rlo = "English \u{202E}forced rtl\u{202C} English";
    let rlo_runs = bidi.resolve_runs(text_with_rlo);
    assert!(rlo_runs.len() >= 3);
    let rlo_middle = rlo_runs.iter().find(|r| r.is_rtl).expect("RLO must produce RTL run");
    assert_eq!(rlo_middle.level, 1);
    assert!(rlo_middle.is_rtl);

    // LRO (Left-to-Right Override): \u{202D} forces Arabic text into LTR until PDF
    let text_with_lro = "Latin \u{202D}مرحبا\u{202C} Latin";
    let lro_runs = bidi.resolve_runs(text_with_lro);
    for r in &lro_runs {
        assert!(!r.is_rtl, "LRO must override text to LTR");
    }
}

#[test]
fn test_uax9_weak_type_numbers_and_separators() {
    let bidi = BidiEngine::new();

    // In Arabic text, European digits after AL become AN (Arabic Number, level 2 in RTL)
    let arabic_with_digits = "السعر 150 دولار";
    let runs = bidi.resolve_runs(arabic_with_digits);

    // Paragraph base level is 1 (RTL)
    assert_eq!(BidiEngine::detect_base_level(arabic_with_digits), 1);
    assert!(!runs.is_empty());

    // Currency symbols ($) and numbers
    assert_eq!(BidiClass::classify('$'), BidiClass::ET);
    assert_eq!(BidiClass::classify('+'), BidiClass::ES);
    assert_eq!(BidiClass::classify('.'), BidiClass::CS);
}

#[test]
fn test_uax9_paired_brackets_rule_n0() {
    let bidi = BidiEngine::new();

    // Paired brackets surrounding Hebrew text in an RTL paragraph must adopt RTL
    let text = "שלום (עולם) שלום";
    let runs = bidi.resolve_runs(text);

    // In pure RTL context with brackets, brackets should not create an isolated LTR run
    assert_eq!(runs.len(), 1, "Uniform RTL paragraph with brackets should form a single RTL run");
    assert!(runs[0].is_rtl);
}

#[test]
fn test_uax9_visual_reordering_rule_l2() {
    let bidi = BidiEngine::new();

    // Mixed English and Hebrew: "first שלום second"
    // Logical order: LTR 'first ', RTL 'שלום', LTR ' second'
    // In LTR paragraph (base level 0):
    // Levels: 'first ' = 0, 'שלום' = 1, ' second' = 0
    // Max level = 1. Level 1 sequence 'שלום' reverses glyphs internally.
    // Run visual sequence: Run 0 ('first '), Run 1 ('שלום'), Run 2 (' second').
    let text = "first שלום second";
    let runs = bidi.resolve_runs(text);

    assert_eq!(runs.len(), 3);
    assert!(!runs[0].is_rtl);
    assert!(runs[1].is_rtl);
    assert!(!runs[2].is_rtl);

    assert_eq!(&text[runs[0].start..runs[0].end], "first ");
    assert_eq!(&text[runs[1].start..runs[1].end], "שלום");
    assert_eq!(&text[runs[2].start..runs[2].end], " second");
}

#[test]
fn test_uax9_glyph_mirroring_rule_l4() {
    assert_eq!(bidi_mirror('('), ')');
    assert_eq!(bidi_mirror(')'), '(');
    assert_eq!(bidi_mirror('['), ']');
    assert_eq!(bidi_mirror(']'), '[');
    assert_eq!(bidi_mirror('{'), '}');
    assert_eq!(bidi_mirror('}'), '{');
    assert_eq!(bidi_mirror('<'), '>');
    assert_eq!(bidi_mirror('>'), '<');
    assert_eq!(bidi_mirror('«'), '»');
    assert_eq!(bidi_mirror('»'), '«');
    assert_eq!(bidi_mirror('A'), 'A');
}

#[test]
fn test_uax9_directional_isolates_and_neutral_resolution() {
    let bidi = BidiEngine::new();

    // 1. RLI (Right-to-Left Isolate: \u{2067}) and PDI (Pop Directional Isolate: \u{2069})
    // Isolated Hebrew inside English
    let text_with_rli = "User \u{2067}שלום\u{2069} logged in";
    let runs = bidi.resolve_runs(text_with_rli);
    assert!(runs.len() >= 3);
    let hebrew_run = runs.iter().find(|r| r.is_rtl).expect("RLI must isolate RTL run");
    assert_eq!(hebrew_run.level, 1);
    assert_eq!(&text_with_rli[hebrew_run.start..hebrew_run.end], "שלום");

    // 2. FSI (First Strong Isolate: \u{2068}) auto-detects direction based on first strong character
    let text_fsi_arabic = "Status: \u{2068}مكتمل\u{2069} done";
    let fsi_runs = bidi.resolve_runs(text_fsi_arabic);
    let rtl_run = fsi_runs.iter().find(|r| r.is_rtl).expect("FSI with Arabic must isolate as RTL");
    assert_eq!(&text_fsi_arabic[rtl_run.start..rtl_run.end], "مكتمل");

    // 3. Rule N1: Neutral characters between matching numbers adopt direction
    // In LTR context: "a 10 / 20 b" -> "/" resolves to LTR
    let n1_ltr = "a 10 / 20 b";
    let n1_runs = bidi.resolve_runs(n1_ltr);
    assert_eq!(n1_runs.len(), 1, "Uniform LTR with numbers and neutrals must stay a single LTR run");
    assert!(!n1_runs[0].is_rtl);

    // 4. Delimiter search skipping BN under Rule N1/N2
    let text_with_bn = "Alpha\u{200B} - \u{200B}Beta";
    let bn_runs = bidi.resolve_runs(text_with_bn);
    assert_eq!(bn_runs.len(), 1, "Neutrals surrounded by L (even with intervening BN) must resolve to L");
}
