//! Comprehensive W3C / UAX #14 Unicode Line Breaking Algorithm tests.

use strata_layout::typography::{
    can_break_between_chars, classify_line_break, is_prohibited_end, is_prohibited_start,
    InlineFormattingContext, InlineLayoutOptions, LineBreakClass, WhiteSpaceMode,
};

#[test]
fn test_uax14_character_classification() {
    assert_eq!(classify_line_break('\n'), LineBreakClass::LF);
    assert_eq!(classify_line_break('\u{200B}'), LineBreakClass::ZW);
    assert_eq!(classify_line_break('\u{2060}'), LineBreakClass::WJ);
    assert_eq!(classify_line_break('\u{00A0}'), LineBreakClass::GL);
    assert_eq!(classify_line_break(' '), LineBreakClass::SP);

    // Open Punctuation
    assert_eq!(classify_line_break('('), LineBreakClass::OP);
    assert_eq!(classify_line_break('「'), LineBreakClass::OP);
    assert_eq!(classify_line_break('（'), LineBreakClass::OP);

    // Close Punctuation
    assert_eq!(classify_line_break(')'), LineBreakClass::CL);
    assert_eq!(classify_line_break('」'), LineBreakClass::CL);
    assert_eq!(classify_line_break('）'), LineBreakClass::CL);
    assert_eq!(classify_line_break('。'), LineBreakClass::CL);
    assert_eq!(classify_line_break('、'), LineBreakClass::CL);

    // Non-starter
    assert_eq!(classify_line_break('っ'), LineBreakClass::NS);
    assert_eq!(classify_line_break('ー'), LineBreakClass::NS);

    // CJK Ideographs
    assert_eq!(classify_line_break('漢'), LineBreakClass::ID);
    assert_eq!(classify_line_break('あ'), LineBreakClass::ID);
    assert_eq!(classify_line_break('ア'), LineBreakClass::ID);
}

#[test]
fn test_uax14_kinsoku_shori_pair_rules() {
    // Gyoto Kinsoku: Never break before closing punctuation or non-starters
    assert!(!can_break_between_chars('漢', '。'));
    assert!(!can_break_between_chars('漢', '、'));
    assert!(!can_break_between_chars('漢', '」'));
    assert!(!can_break_between_chars('漢', '）'));
    assert!(!can_break_between_chars('漢', '！'));
    assert!(!can_break_between_chars('あ', 'っ'));

    // Gyomatsu Kinsoku: Never break after opening punctuation
    assert!(!can_break_between_chars('「', '漢'));
    assert!(!can_break_between_chars('（', '漢'));

    // Ideographic break: Break allowed between two ideographs
    assert!(can_break_between_chars('漢', '字'));
    assert!(can_break_between_chars('日', '本'));
    assert!(can_break_between_chars('東', '京'));

    // Prohibited start/end helpers
    assert!(is_prohibited_start('。'));
    assert!(is_prohibited_start('、'));
    assert!(is_prohibited_start('」'));
    assert!(!is_prohibited_start('漢'));

    assert!(is_prohibited_end('「'));
    assert!(is_prohibited_end('（'));
    assert!(!is_prohibited_end('漢'));
}

#[test]
fn test_cjk_continuous_text_auto_wrapping() {
    let ifc = InlineFormattingContext::new();

    // Continuous Japanese text without spaces
    let cjk_text = "日本語の文章は空白文字を含まないが適切な位置で折り返される必要があります。";

    let opt = InlineLayoutOptions {
        font_size: 16.0,
        line_height: Some(24.0),
        max_width: Some(150.0), // Force wrapping into multiple lines
        white_space: WhiteSpaceMode::Normal,
        ..Default::default()
    };

    let result = ifc.layout_text(cjk_text, &opt);

    assert!(
        result.line_count >= 3,
        "Continuous Japanese text must wrap into multiple lines, got {}",
        result.line_count
    );
    assert!(result.total_width <= 155.0);

    // Verify Kinsoku Shori: No line starts with a closing punctuation mark ('。' or '、')
    for (idx, line) in result.lines.iter().enumerate() {
        if idx > 0 && !line.text_fragments.is_empty() {
            let first_frag = &line.text_fragments[0];
            let first_char = first_frag.text.chars().next().unwrap_or(' ');
            assert!(
                !is_prohibited_start(first_char),
                "Line {} starts with prohibited character '{}'",
                idx,
                first_char
            );
        }
    }
}

#[test]
fn test_word_joiner_and_zwsp_breaking() {
    let ifc = InlineFormattingContext::new();

    // 1. Word Joiner prohibits breaking between words
    let wj_text = "Unbreakable\u{2060}Word\u{2060}Sequence";
    let wj_opt = InlineLayoutOptions {
        font_size: 16.0,
        line_height: Some(24.0),
        max_width: Some(100.0), // smaller than full word
        white_space: WhiteSpaceMode::Normal,
        ..Default::default()
    };
    let wj_res = ifc.layout_text(wj_text, &wj_opt);
    // Should not break internally at Word Joiners
    assert_eq!(wj_res.line_count, 1);

    // 2. Zero Width Space permits breaking
    let zwsp_text = "Break\u{200B}Here\u{200B}Now";
    let zwsp_opt = InlineLayoutOptions {
        font_size: 16.0,
        line_height: Some(24.0),
        max_width: Some(50.0),
        white_space: WhiteSpaceMode::Normal,
        ..Default::default()
    };
    let zwsp_res = ifc.layout_text(zwsp_text, &zwsp_opt);
    assert!(zwsp_res.line_count >= 3);
}
