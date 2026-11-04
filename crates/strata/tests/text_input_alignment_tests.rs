use strata::AppRuntime;

#[test]
fn test_exact_char_index_at_offset_end_of_text() {
    let text = "https://news.ycombinator.com";
    let font_size = 14.0f32;
    let line_height = 1.4f32;
    let is_mono = false;

    let (full_w, _) = strata_layout::text::measure_text_bounds_ext(text, font_size, line_height, None, is_mono);
    assert!(full_w > 0.0);

    // Clicking exactly at the end of the text should give full length
    let idx_at_end = AppRuntime::calculate_exact_char_index_at_offset(text, full_w, font_size, line_height, is_mono);
    assert_eq!(idx_at_end, text.len());

    // Clicking far beyond the end of the text (e.g. empty space in input) should clamp to full length
    let idx_beyond = AppRuntime::calculate_exact_char_index_at_offset(text, full_w + 200.0, font_size, line_height, is_mono);
    assert_eq!(idx_beyond, text.len());

    // Clicking before start should give 0
    let idx_start = AppRuntime::calculate_exact_char_index_at_offset(text, 0.0, font_size, line_height, is_mono);
    assert_eq!(idx_start, 0);
    let idx_neg = AppRuntime::calculate_exact_char_index_at_offset(text, -10.0, font_size, line_height, is_mono);
    assert_eq!(idx_neg, 0);
}

#[test]
fn test_exact_char_index_monospace_and_proportional_alignment() {
    let text = "Search Google or type a URL";
    let font_size = 12.0f32;
    let line_height = 1.2f32;

    for is_mono in [false, true] {
        let chars: Vec<char> = text.chars().collect();
        for i in 0..chars.len() {
            let prefix: String = chars[..i].iter().collect();
            let (w, _) = strata_layout::text::measure_text_bounds_ext(&prefix, font_size, line_height, None, is_mono);
            
            // Clicking right at the boundary w should yield index i
            let resolved_idx = AppRuntime::calculate_exact_char_index_at_offset(text, w, font_size, line_height, is_mono);
            assert_eq!(resolved_idx, i, "Index mismatch for char index {} with is_mono={}", i, is_mono);
        }
    }
}
