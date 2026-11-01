//! Intrinsic text dimension measurement and greedy Unicode line breaking.

/// Approximates intrinsic text dimensions based on character count, font size, and line height.
pub fn measure_text_bounds(
    text: &str,
    font_size: f32,
    line_height_multiplier: f32,
    max_width: Option<f32>,
) -> (f32, f32) {
    if text.is_empty() {
        return (0.0, font_size * line_height_multiplier);
    }

    // Average character width for typical sans-serif proportional fonts (Inter/Roboto)
    let char_width = font_size * 0.55;
    let single_line_height = font_size * line_height_multiplier;

    match max_width {
        Some(max_w) if max_w > 0.0 => {
            // Greedy word wrapping
            let mut lines = 1usize;
            let mut current_line_w = 0.0f32;
            let mut max_observed_w = 0.0f32;

            for word in text.split_whitespace() {
                let word_w = word.chars().count() as f32 * char_width;
                let space_w = char_width;

                if current_line_w + word_w > max_w && current_line_w > 0.0 {
                    max_observed_w = max_observed_w.max(current_line_w);
                    lines += 1;
                    current_line_w = word_w;
                } else {
                    if current_line_w > 0.0 {
                        current_line_w += space_w;
                    }
                    current_line_w += word_w;
                }
            }

            max_observed_w = max_observed_w.max(current_line_w);
            (max_observed_w, lines as f32 * single_line_height)
        }
        _ => {
            // Single unbroken line
            let w = text.chars().count() as f32 * char_width;
            (w, single_line_height)
        }
    }
}
