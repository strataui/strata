//! Tests for OpenType / TrueType font parsing, authentic table metrics, and text shaping.

use strata_layout::typography::{font_database, OpenTypeFont, TextShaper};
use strata_style::types::{FontStyle, FontWeight};

#[test]
fn test_opentype_parse_system_dejavu_sans() {
    let font_bytes = match std::fs::read("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf") {
        Ok(b) => b,
        Err(_) => {
            eprintln!("DejaVuSans.ttf not found on host, skipping filesystem test");
            return;
        }
    };

    let font = OpenTypeFont::parse(&font_bytes).expect("Failed to parse DejaVuSans.ttf");
    assert_eq!(font.units_per_em, 2048);
    assert_eq!(font.weight_class, 400);
    assert!(!font.is_italic);
    assert!(!font.is_monospace);
    assert!(font.num_glyphs > 5000);

    // Verify 'cmap' mapping for basic Latin characters
    let gid_a = font.glyph_id('A');
    let gid_b = font.glyph_id('B');
    let gid_space = font.glyph_id(' ');
    assert!(gid_a > 0, "Glyph ID for 'A' must be non-zero");
    assert!(gid_b > 0, "Glyph ID for 'B' must be non-zero");
    assert_ne!(gid_a, gid_b);
    assert!(gid_space > 0, "Glyph ID for space must be non-zero");

    // Verify 'hmtx' advance width
    let adv_a = font.char_advance('A', 16.0);
    let adv_space = font.char_advance(' ', 16.0);
    assert!(adv_a > 0.0);
    assert!(adv_space > 0.0);
    assert!(adv_a > adv_space);

    // Verify authentic FontMetrics
    let metrics = font.font_metrics(16.0);
    assert_eq!(metrics.units_per_em, 2048);
    assert_eq!(metrics.font_size, 16.0);
    assert!(metrics.ascent.to_f32() > 10.0);
    assert!(metrics.descent.to_f32() > 2.0);
}

#[test]
fn test_opentype_bold_vs_regular_metrics() {
    let regular_bytes = match std::fs::read("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf") {
        Ok(b) => b,
        Err(_) => return,
    };
    let bold_bytes = match std::fs::read("/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf") {
        Ok(b) => b,
        Err(_) => return,
    };

    let regular = OpenTypeFont::parse(&regular_bytes).expect("parse regular");
    let bold = OpenTypeFont::parse(&bold_bytes).expect("parse bold");

    assert_eq!(regular.weight_class, 400);
    assert_eq!(bold.weight_class, 700);

    // Advance widths must be authentic font metrics, varying per glyph rather than a constant scalar
    let reg_h = regular.char_advance('H', 16.0);
    let bold_h = bold.char_advance('H', 16.0);
    let reg_l = regular.char_advance('l', 16.0);
    let bold_l = bold.char_advance('l', 16.0);

    assert!(bold_h > reg_h, "Bold 'H' advance must exceed regular");
    assert!(bold_l > reg_l, "Bold 'l' advance must exceed regular");

    let ratio_h = bold_h / reg_h;
    let ratio_l = bold_l / reg_l;

    // Authentic font designs do not scale all glyphs by a uniform multiplier (e.g. 1.06)
    assert!(
        (ratio_h - ratio_l).abs() > 0.02,
        "Authentic OpenType bold advances have glyph-dependent ratios: ratio_H={}, ratio_l={}",
        ratio_h,
        ratio_l
    );
}

#[test]
fn test_font_database_lookup_and_styled_measurement() {
    let db = font_database();
    let regular = db.query_font("dejavu sans", FontWeight::Normal, FontStyle::Normal);
    let bold = db.query_font("dejavu sans", FontWeight::Bold, FontStyle::Normal);

    if let (Some(reg_f), Some(bold_f)) = (regular, bold) {
        assert_eq!(reg_f.weight_class, 400);
        assert_eq!(bold_f.weight_class, 700);
    }

    let dejavu_atom = strata_primitives::atom::intern("dejavu sans");
    let (reg_w, _) = strata_layout::measure_text_bounds_styled_with_family(
        "Strata Typography Engine",
        16.0,
        1.2,
        None,
        false,
        dejavu_atom,
        FontWeight::Normal,
        FontStyle::Normal,
    );

    let (bold_w, _) = strata_layout::measure_text_bounds_styled_with_family(
        "Strata Typography Engine",
        16.0,
        1.2,
        None,
        false,
        dejavu_atom,
        FontWeight::Bold,
        FontStyle::Normal,
    );

    assert!(reg_w > 0.0);
    assert!(bold_w > 0.0);
    assert!(bold_w > reg_w, "Bold text run must be wider than regular text run");
}

#[test]
fn test_shaper_with_opentype_font() {
    let regular_bytes = match std::fs::read("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf") {
        Ok(b) => b,
        Err(_) => return,
    };
    let font = OpenTypeFont::parse(&regular_bytes).expect("parse font");
    let shaper = TextShaper::new();

    let text = "Hello OpenType!";
    let glyphs = shaper.shape_with_font(text, &font, 18.0);
    assert_eq!(glyphs.len(), text.len());

    let mut cumulative_advance = 0.0f32;
    for g in glyphs {
        cumulative_advance += g.x_advance.to_f32();
    }
    assert!(cumulative_advance > 50.0);
}

#[test]
fn test_opentype_features_evaluator() {
    let regular_bytes = match std::fs::read("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf") {
        Ok(b) => b,
        Err(_) => return,
    };
    let font = OpenTypeFont::parse(&regular_bytes).expect("parse font");
    let shaper = TextShaper::new();

    // 1. Standard ligatures (liga) and Contextual code ligatures (calt)
    let code_text = "if (a === b) return fi;";
    let default_glyphs = shaper.shape_with_font(code_text, &font, 16.0);
    let lig_count = default_glyphs.iter().filter(|g| g.is_ligature).count();
    // '===' (calt) and 'fi' (liga) must both be formed as ligatures
    assert!(lig_count >= 2, "Expected at least 2 ligatures for '===' and 'fi'");

    // 2. Discretionary ligatures (dlig)
    let dlig_features = strata_layout::typography::OpenTypeFeatures {
        discretionary: true,
        ..Default::default()
    };
    let st_glyphs = shaper.shape_with_font_and_features("first", &font, 16.0, dlig_features);
    let st_lig = st_glyphs.iter().any(|g| g.is_ligature && g.glyph_id == 0xFB06);
    assert!(st_lig, "Expected 'st' discretionary ligature with glyph_id 0xFB06");

    let no_dlig = strata_layout::typography::OpenTypeFeatures {
        discretionary: false,
        ..Default::default()
    };
    let no_st_glyphs = shaper.shape_with_font_and_features("first", &font, 16.0, no_dlig);
    let no_st = no_st_glyphs.iter().any(|g| g.is_ligature && g.glyph_id == 0xFB06);
    assert!(!no_st, "'st' should not form a ligature when discretionary ligatures are disabled");

    // 3. Small capitals (smcp)
    let smcp_features = strata_layout::typography::OpenTypeFeatures {
        small_caps: true,
        ..Default::default()
    };
    let smcp_glyphs = shaper.shape_with_font_and_features("hello", &font, 16.0, smcp_features);
    assert_eq!(smcp_glyphs.len(), 5);
    // 'h' mapped to U+029C, 'e' to U+1D08, 'l' to U+029F, 'o' to U+1D0F
    assert_ne!(smcp_glyphs[0].glyph_id, 'h' as u32);
}

#[test]
fn test_subpixel_font_metrics_parity_with_chromium() {
    let regular_bytes = match std::fs::read("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf") {
        Ok(b) => b,
        Err(_) => return,
    };
    let font = OpenTypeFont::parse(&regular_bytes).expect("parse font");
    let metrics = font.font_metrics(16.0);

    // 1. Verify 1/64th fixed-point precision (0.015625px alignment)
    let ascent_f32 = metrics.ascent.to_f32();
    let snapped_ascent = (ascent_f32 * 64.0).round() / 64.0;
    assert_eq!(
        ascent_f32, snapped_ascent,
        "Ascent must be strictly aligned to 1/64th subpixel grid (Chromium LayoutUnit)"
    );

    let descent_f32 = metrics.descent.to_f32();
    let snapped_descent = (descent_f32 * 64.0).round() / 64.0;
    assert_eq!(
        descent_f32, snapped_descent,
        "Descent must be strictly aligned to 1/64th subpixel grid"
    );

    // 2. Verify natural height and default line height
    let natural_h = metrics.natural_height();
    assert_eq!(natural_h, metrics.ascent + metrics.descent);
    let def_lh = metrics.default_line_height();
    assert_eq!(def_lh, natural_h + metrics.line_gap);

    // 3. Verify CSS 2.1 Section 10.8 half-leading strut
    let strut = strata_layout::typography::CssStrut::new(&metrics, None);
    assert_eq!(strut.line_height, def_lh);
    assert_eq!(strut.metrics.line_height(), def_lh);
}

