//! Integration Tests: HarfBuzz OpenType Text Shaping, BiDi UAX#9, and Unicode Line Breaking.

use strata_layout::prelude::*;
use strata_primitives::geom::LayoutUnit;

#[test]
fn test_font_metrics_and_css_half_leading() {
    let font_size = 20.0f32;
    let metrics = FontMetrics::default_sans(font_size);

    assert_eq!(metrics.font_size, 20.0);
    assert!(metrics.ascent > metrics.descent);

    // Target line-height = 32px
    let line_height = LayoutUnit::from_f32(32.0);
    let (leading_above, leading_below) = compute_half_leading(
        line_height,
        metrics.ascent,
        metrics.descent,
    );

    // Half-leading invariant: ascent + descent + leading_above + leading_below == line_height
    let total_reconstructed = metrics.ascent + metrics.descent + leading_above + leading_below;
    assert_eq!(total_reconstructed, line_height);

    // CSS Strut invariant
    let strut = CssStrut::new(&metrics, Some(32.0));
    assert_eq!(strut.line_height, line_height);
    assert_eq!(strut.metrics.line_height(), line_height);

    // FontHeight unite
    let mut h1 = FontHeight::new(LayoutUnit::from_f32(14.0), LayoutUnit::from_f32(4.0));
    let h2 = FontHeight::new(LayoutUnit::from_f32(18.0), LayoutUnit::from_f32(2.0));
    h1.unite(h2);
    assert_eq!(h1.ascent, LayoutUnit::from_f32(18.0));
    assert_eq!(h1.descent, LayoutUnit::from_f32(4.0));
}

#[test]
fn test_opentype_ligatures_and_pair_kerning() {
    let shaper = TextShaper::new();
    let metrics = FontMetrics::default_sans(16.0);

    // 1. Code ligatures in proportional text
    let code_text = "if (a === b && c != d) => run();";
    let glyphs = shaper.shape(code_text, &metrics, false);

    let has_triple_eq = glyphs.iter().any(|g| g.is_ligature && g.glyph_id == 0xF002);
    let has_not_eq = glyphs.iter().any(|g| g.is_ligature && g.glyph_id == 0xF007);
    let has_arrow = glyphs.iter().any(|g| g.is_ligature && g.glyph_id == 0xF00A);

    assert!(has_triple_eq, "Must identify '===' code ligature");
    assert!(has_not_eq, "Must identify '!=' code ligature");
    assert!(has_arrow, "Must identify '=>' code ligature");

    // 2. Typographic ligatures
    let typo_text = "difficult coffee flow";
    let typo_glyphs = shaper.shape(typo_text, &metrics, false);
    let has_ffi = typo_glyphs.iter().any(|g| g.is_ligature && g.glyph_id == 0xFB03);
    let has_ff = typo_glyphs.iter().any(|g| g.is_ligature && g.glyph_id == 0xFB00);
    let has_fl = typo_glyphs.iter().any(|g| g.is_ligature && g.glyph_id == 0xFB02);

    assert!(has_ffi, "Must identify 'ffi' ligature in difficult");
    assert!(has_ff, "Must identify 'ff' ligature in coffee");
    assert!(has_fl, "Must identify 'fl' ligature in flow");

    // 3. Monospace disables ligatures
    let mono_glyphs = shaper.shape(code_text, &metrics, true);
    let mono_has_lig = mono_glyphs.iter().any(|g| g.is_ligature);
    assert!(!mono_has_lig, "Monospace code fonts must retain individual glyph clusters");

    // 4. Optical pair kerning
    let pair_kerned = shaper.shape("AV", &metrics, false);
    let isolated_a = shaper.shape("A", &metrics, false);
    let isolated_v = shaper.shape("V", &metrics, false);

    let pair_w = pair_kerned[0].x_advance + pair_kerned[1].x_advance;
    let sum_isolated = isolated_a[0].x_advance + isolated_v[0].x_advance;
    assert!(
        pair_w < sum_isolated,
        "Optical pair kerning must reduce advance width for 'AV' pair"
    );
}

#[test]
fn test_unicode_bidi_uax9_mixed_scripts() {
    let bidi = BidiEngine::new();

    // 1. Pure LTR Latin
    let ltr_text = "Distributed Cluster US-East-1";
    let ltr_runs = bidi.resolve_runs(ltr_text);
    assert_eq!(ltr_runs.len(), 1);
    assert!(!ltr_runs[0].is_rtl);
    assert_eq!(ltr_runs[0].level, 0);

    // 2. Pure Hebrew (RTL)
    let hebrew_text = "שלום עולם";
    let hebrew_runs = bidi.resolve_runs(hebrew_text);
    assert_eq!(hebrew_runs.len(), 1);
    assert!(hebrew_runs[0].is_rtl);
    assert_eq!(hebrew_runs[0].level, 1);

    // 3. Mixed LTR and RTL: "Cluster שלום Node"
    let mixed_text = "Cluster שלום Node";
    let mixed_runs = bidi.resolve_runs(mixed_text);
    assert!(mixed_runs.len() >= 3, "Mixed text must decompose into distinct directional runs");

    let has_rtl = mixed_runs.iter().any(|r| r.is_rtl);
    let has_ltr = mixed_runs.iter().any(|r| !r.is_rtl);
    assert!(has_rtl && has_ltr, "Must contain both LTR and RTL runs");
}

#[test]
fn test_unicode_line_breaking_and_whitespace_modes() {
    let ifc = InlineFormattingContext::new();

    let text = "First line with words\nSecond line with words";

    // 1. WhiteSpaceMode::Normal wraps on bounds and preserves explicit newlines
    let normal_opt = InlineLayoutOptions {
        font_size: 16.0,
        line_height: Some(24.0),
        max_width: Some(120.0),
        white_space: WhiteSpaceMode::Normal,
        ..Default::default()
    };
    let normal_res = ifc.layout_text(text, &normal_opt);
    assert!(normal_res.line_count >= 2);
    assert!(normal_res.total_width <= 120.0);

    // 2. WhiteSpaceMode::NoWrap suppresses soft line breaks
    let nowrap_opt = InlineLayoutOptions {
        font_size: 16.0,
        line_height: Some(24.0),
        max_width: Some(60.0),
        white_space: WhiteSpaceMode::NoWrap,
        ..Default::default()
    };
    let nowrap_res = ifc.layout_text(text, &nowrap_opt);
    // Explicit newline produces exactly 2 lines despite tiny max_width 60.0
    assert_eq!(nowrap_res.line_count, 2);
    assert!(nowrap_res.total_width > 60.0);

    // 3. Soft hyphen and Zero-Width Space breaking
    let zwsp_text = "Super\u{200B}cali\u{200B}fragi\u{200B}listic";
    let zwsp_opt = InlineLayoutOptions {
        font_size: 16.0,
        line_height: Some(24.0),
        max_width: Some(50.0),
        white_space: WhiteSpaceMode::Normal,
        ..Default::default()
    };
    let zwsp_res = ifc.layout_text(zwsp_text, &zwsp_opt);
    assert!(zwsp_res.line_count > 1, "Zero-width spaces must provide valid break points");
}

#[test]
fn test_inline_formatting_context_vertical_alignments() {
    let ifc = InlineFormattingContext::new();
    let text = "Metrics Alignment Baseline";

    let opt_baseline = InlineLayoutOptions {
        font_size: 16.0,
        line_height: Some(30.0),
        vertical_align: VerticalAlignment::Baseline,
        ..Default::default()
    };
    let res_baseline = ifc.layout_text(text, &opt_baseline);

    let opt_super = InlineLayoutOptions {
        font_size: 16.0,
        line_height: Some(30.0),
        vertical_align: VerticalAlignment::Super,
        ..Default::default()
    };
    let res_super = ifc.layout_text(text, &opt_super);

    let opt_sub = InlineLayoutOptions {
        font_size: 16.0,
        line_height: Some(30.0),
        vertical_align: VerticalAlignment::Sub,
        ..Default::default()
    };
    let res_sub = ifc.layout_text(text, &opt_sub);

    // Superscript shifts baseline upward (lower numeric y in top-down coordinates)
    assert!(
        res_super.lines[0].baseline < res_baseline.lines[0].baseline,
        "Superscript must elevate baseline upwards"
    );

    // Subscript shifts baseline downward (higher numeric y in top-down coordinates)
    assert!(
        res_sub.lines[0].baseline > res_baseline.lines[0].baseline,
        "Subscript must lower baseline downwards"
    );
}

#[test]
fn test_multi_span_inline_stream_line_breaking() {
    let ifc = InlineFormattingContext::new();

    let items = vec![
        InlineItem::text("The Rust programming language ", InlineTextStyle {
            font_size: 16.0,
            font_weight: strata_style::types::FontWeight::Normal,
            ..Default::default()
        }),
        InlineItem::text("provides memory safety without a garbage collector ", InlineTextStyle {
            font_size: 16.0,
            font_weight: strata_style::types::FontWeight::Bold,
            ..Default::default()
        }),
        InlineItem::text("and high performance concurrency.", InlineTextStyle {
            font_size: 16.0,
            font_style: strata_style::types::FontStyle::Italic,
            ..Default::default()
        }),
    ];

    let options = InlineLayoutOptions {
        font_size: 16.0,
        line_height: Some(24.0),
        max_width: Some(300.0),
        white_space: WhiteSpaceMode::Normal,
        ..Default::default()
    };

    let result = ifc.layout_inline_stream(&items, &options);

    assert!(result.line_count >= 2, "Prose stream must break into multiple lines");
    assert!(result.total_width <= 300.0, "Total width must fit within container constraint 300.0");
    assert!(result.total_height >= 48.0, "Total height must encompass all stacked line boxes");

    // Check that line boxes have text fragments
    let all_frags: Vec<&TextFragment> = result.lines.iter().flat_map(|l| &l.text_fragments).collect();
    assert!(all_frags.len() >= 3, "Must have visual fragments for each styling run");
}

#[test]
fn test_multi_line_span_fragmentation() {
    let ifc = InlineFormattingContext::new();

    // A single long bold item spanning across lines
    let items = vec![
        InlineItem::text("Prefix ", InlineTextStyle { font_size: 16.0, ..Default::default() }),
        InlineItem::text("extremely long continuous bold phrasing that definitely spans across multiple visual lines cleanly", InlineTextStyle {
            font_size: 16.0,
            font_weight: strata_style::types::FontWeight::Bold,
            color: strata_style::types::Color::new(255, 0, 0, 255),
            ..Default::default()
        }),
        InlineItem::text(" Suffix", InlineTextStyle { font_size: 16.0, ..Default::default() }),
    ];

    let options = InlineLayoutOptions {
        font_size: 16.0,
        line_height: Some(24.0),
        max_width: Some(220.0),
        white_space: WhiteSpaceMode::Normal,
        ..Default::default()
    };

    let result = ifc.layout_inline_stream(&items, &options);

    assert!(result.line_count >= 3, "Must fragment into at least 3 lines");

    // Check item 1 fragments: item_index 1 must appear on multiple lines
    let item1_frags: Vec<&TextFragment> = result
        .lines
        .iter()
        .flat_map(|l| &l.text_fragments)
        .filter(|f| f.item_index == 1)
        .collect();

    assert!(
        item1_frags.len() >= 2,
        "Bold run (item 1) must be fragmented into at least 2 distinct visual fragments"
    );
    for frag in &item1_frags {
        assert_eq!(frag.style.font_weight, strata_style::types::FontWeight::Bold);
        assert_eq!(frag.style.color, strata_style::types::Color::new(255, 0, 0, 255));
    }
}

#[test]
fn test_block_container_ifc_auto_wrapping() {
    // Create a block container <p> with inline children
    let mut block = LayoutNode::new(strata_style::types::ComputedStyle {
        display: strata_style::types::Display::Block,
        width: strata_style::types::Dimension::Px(250.0),
        height: strata_style::types::Dimension::Auto,
        font_size: 16.0,
        line_height: 1.5,
        ..Default::default()
    });

    let mut span1 = LayoutNode::new(strata_style::types::ComputedStyle {
        display: strata_style::types::Display::Inline,
        font_size: 16.0,
        line_height: 1.5,
        ..Default::default()
    });
    span1.text = Some("Modern computing systems require ".to_string());

    let mut span2 = LayoutNode::new(strata_style::types::ComputedStyle {
        display: strata_style::types::Display::Inline,
        font_size: 16.0,
        font_weight: strata_style::types::FontWeight::Bold,
        line_height: 1.5,
        ..Default::default()
    });
    span2.text = Some("uncompromising safety, speed, and concurrency.".to_string());

    block.add_child(span1);
    block.add_child(span2);

    block.compute_layout(250.0, 1000.0);

    assert!(!block.line_boxes.is_empty(), "Block container must establish an IFC and produce line boxes");
    assert!(block.line_boxes.len() >= 2, "Prose must wrap into multiple line boxes");
    assert!(block.geometry.border_box.height >= 40.0, "Height must encompass all lines");
}

#[test]
fn test_complex_script_shaping_arabic_indic_thai() {
    let shaper = TextShaper::new();
    let metrics = FontMetrics::default_sans(16.0);

    // 1. Arabic contextual joining: كتب (Kaf + Teh + Beh)
    // Initial Kaf (0xFEDB), Medial Teh (0xFE98), Final Beh (0xFE90)
    let kataba = "كتب";
    let glyphs = shaper.shape(kataba, &metrics, false);
    assert_eq!(glyphs.len(), 3);
    assert_eq!(glyphs[0].glyph_id, 0xFEDB, "Kaf must be Initial form");
    assert_eq!(glyphs[1].glyph_id, 0xFE98, "Teh must be Medial form");
    assert_eq!(glyphs[2].glyph_id, 0xFE90, "Beh must be Final form");

    // 2. Arabic Lam-Alef mandatory ligature: سلام (Seen + Lam + Alef + Meem)
    // Initial Seen (0xFEB3), Lam-Alef Final ligature (0xFEFC), Isolated Meem (0xFEE1)
    let salam = "سلام";
    let salam_glyphs = shaper.shape(salam, &metrics, false);
    assert_eq!(salam_glyphs.len(), 3, "Lam + Alef must merge into a single ligature glyph");
    assert_eq!(salam_glyphs[0].glyph_id, 0xFEB3, "Seen must be Initial");
    assert_eq!(salam_glyphs[1].glyph_id, 0xFEFC, "Lam-Alef must be Final ligature form");
    assert!(salam_glyphs[1].is_ligature, "Lam-Alef must be marked as ligature");
    assert_eq!(salam_glyphs[2].glyph_id, 0xFEE1, "Meem must be Isolated form");

    // 3. Arabic transparent Harakat / Tashkeel: كَتَبَ
    let kataba_vowels = "كَتَبَ";
    let vowel_glyphs = shaper.shape(kataba_vowels, &metrics, false);
    assert_eq!(vowel_glyphs.len(), 6);
    let marks: Vec<&ShapedGlyph> = vowel_glyphs.iter().filter(|g| g.is_mark).collect();
    assert_eq!(marks.len(), 3);
    for mark in marks {
        assert_eq!(mark.x_advance, LayoutUnit::ZERO, "Tashkeel vowel must have zero horizontal advance");
        assert!(mark.y_offset > LayoutUnit::ZERO, "Fatha must have positive y_offset above baseline");
    }

    // 4. Indic complex conjunct: क्ष (Ksha: Ka + Virama + Ssa) and त्र (Tra: Ta + Virama + Ra)
    let indic_text = "क्षेत्र";
    let indic_glyphs = shaper.shape(indic_text, &metrics, false);
    let has_ksha = indic_glyphs.iter().any(|g| g.is_ligature && g.glyph_id == 0xF050);
    let has_tra = indic_glyphs.iter().any(|g| g.is_ligature && g.glyph_id == 0xF051);
    assert!(has_ksha, "Must shape Indic conjunct 'क्ष' (Ksha)");
    assert!(has_tra, "Must shape Indic conjunct 'त्र' (Tra)");

    // 5. Thai vertical tone mark and vowel stacking
    let thai_text = "น้ำ";
    let thai_glyphs = shaper.shape(thai_text, &metrics, false);
    let thai_mark = thai_glyphs.iter().find(|g| g.glyph_id == '\u{0E49}' as u32);
    assert!(thai_mark.is_some(), "Thai tone mark Mai Tho must be present");
    let mark = thai_mark.unwrap();
    assert!(mark.is_mark);
    assert_eq!(mark.x_advance, LayoutUnit::ZERO, "Thai tone mark must have zero advance for vertical stacking");
    assert!(mark.y_offset > LayoutUnit::ZERO, "Thai tone mark must stack vertically above base");

    // 6. Extended code operator ligatures
    let op_text = "a ||= b &&= c ??= d";
    let op_glyphs = shaper.shape(op_text, &metrics, false);
    assert!(op_glyphs.iter().any(|g| g.is_ligature && g.glyph_id == 0xF010), "Must shape '||=' ligature");
    assert!(op_glyphs.iter().any(|g| g.is_ligature && g.glyph_id == 0xF011), "Must shape '&&=' ligature");
    assert!(op_glyphs.iter().any(|g| g.is_ligature && g.glyph_id == 0xF012), "Must shape '??=' ligature");

    // 7. Multi-glyph Arabic ligatures: الله (Allah), محمد (Muhammad), أكبر (Akbar)
    let allah_text = "الله أكبر";
    let allah_glyphs = shaper.shape(allah_text, &metrics, false);
    assert!(allah_glyphs.iter().any(|g| g.is_ligature && g.glyph_id == 0xFDF2), "Must shape Allah ligature (0xFDF2)");
    assert!(allah_glyphs.iter().any(|g| g.is_ligature && g.glyph_id == 0xFDF3), "Must shape Akbar ligature (0xFDF3)");

    let muhammad_text = "محمد";
    let muhammad_glyphs = shaper.shape(muhammad_text, &metrics, false);
    assert_eq!(muhammad_glyphs.len(), 1, "Muhammad must shape to a single multi-glyph ligature");
    assert_eq!(muhammad_glyphs[0].glyph_id, 0xFDF4, "Must shape Muhammad ligature (0xFDF4)");

    // 8. Indic Universal Shaping Engine (USE) pre-base vowel reordering
    // Input: "कि" (Ka \u{0915} + Short I matra \u{093F})
    // USE output order: Matra \u{093F} FIRST, followed by Ka \u{0915}
    let ki_text = "कि";
    let ki_glyphs = shaper.shape(ki_text, &metrics, false);
    assert_eq!(ki_glyphs.len(), 2, "Must produce 2 glyphs for 'कि'");
    assert_eq!(ki_glyphs[0].glyph_id, 0x093F, "Pre-base vowel sign 'ि' must be reordered before consonant");
    assert_eq!(ki_glyphs[1].glyph_id, 0x0915, "Consonant 'क' must follow reordered pre-base vowel");

    // 9. Indic Reph handling: "धर्म" (Dha \u{0927} + Ra \u{0930} + Virama \u{094D} + Ma \u{092E})
    let dharma_text = "धर्म";
    let dharma_glyphs = shaper.shape(dharma_text, &metrics, false);
    assert!(dharma_glyphs.len() >= 3, "Must produce shaped cluster for 'धर्म'");
    assert_eq!(dharma_glyphs[0].glyph_id, 0x0927, "Base consonant 'ध'");
    // Reph mark attaches above 'म'
    assert!(dharma_glyphs.iter().any(|g| g.glyph_id == 0x094D && g.is_mark), "Reph mark must attach above base consonant");
}
