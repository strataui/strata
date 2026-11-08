//! Authentic Inline Sixel Graphics Engine with Color Quantization and Run-Length Encoding.
//! Sixel translates bitmapped images into DEC terminal raster sequences.

use std::collections::HashMap;
use strata_primitives::color::Rgba;

/// Options for Sixel image rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SixelOptions {
    /// Terminal column coordinate (0-based).
    pub col: u16,
    /// Terminal row coordinate (0-based).
    pub row: u16,
    /// Pixel aspect ratio numerator / denominator (default 1:1).
    pub aspect_ratio: (u16, u16),
}

impl Default for SixelOptions {
    fn default() -> Self {
        Self {
            col: 0,
            row: 0,
            aspect_ratio: (1, 1),
        }
    }
}

/// Encodes an RGBA image buffer into an optimized DEC Sixel raster sequence.
pub fn encode_sixel(
    pixels: &[Rgba],
    width: u32,
    height: u32,
    opts: &SixelOptions,
) -> String {
    if width == 0 || height == 0 || pixels.len() < (width * height) as usize {
        return String::new();
    }

    let mut out = String::with_capacity((width * height) as usize);

    // Position cursor at destination character cell (1-based)
    out.push_str(&format!("\x1b[{};{}H", opts.row + 1, opts.col + 1));

    // DCS Sixel Introducer: \x1bPq
    // Raster attributes: "Pan;Pad;Ph;Pv (pixel aspect ratio and image dimensions)
    out.push_str(&format!(
        "\x1bPq\"{};{};{};{}",
        opts.aspect_ratio.0, opts.aspect_ratio.1, width, height
    ));

    // 1. Quantize 24-bit RGB colors to a 256-color palette
    // 6x6x6 color cube (216 colors) + 24 grayscale steps = 240 colors
    let mut palette: Vec<(u8, u8, u8)> = Vec::with_capacity(256);
    let mut color_to_idx: HashMap<(u8, u8, u8), u8> = HashMap::new();

    // Map each pixel to a palette index
    let mut indexed_pixels = Vec::with_capacity((width * height) as usize);
    for p in &pixels[..(width * height) as usize] {
        if p.a < 128 {
            // Transparent / background
            indexed_pixels.push(None);
            continue;
        }

        // Quantize RGB to 6 levels: 0, 51, 102, 153, 204, 255
        let qr = quantize_channel_6(p.r);
        let qg = quantize_channel_6(p.g);
        let qb = quantize_channel_6(p.b);
        let q_color = (qr, qg, qb);

        let idx = if let Some(&idx) = color_to_idx.get(&q_color) {
            idx
        } else if palette.len() < 255 {
            let new_idx = palette.len() as u8;
            palette.push(q_color);
            color_to_idx.insert(q_color, new_idx);
            new_idx
        } else {
            0
        };

        indexed_pixels.push(Some(idx));
    }

    // 2. Emit Sixel Palette Definitions (#color;2;r%;g%;b%)
    for (idx, &(r, g, b)) in palette.iter().enumerate() {
        let r_pct = ((r as u32) * 100 + 127) / 255;
        let g_pct = ((g as u32) * 100 + 127) / 255;
        let b_pct = ((b as u32) * 100 + 127) / 255;
        out.push_str(&format!("#{idx};2;{r_pct};{g_pct};{b_pct}"));
    }

    // 3. Encode Sixel 6-pixel vertical bands
    let num_bands = height.div_ceil(6);

    for band in 0..num_bands {
        let y_start = band * 6;

        // Find which colors appear in this 6-pixel horizontal band
        let mut active_colors = Vec::new();
        for dy in 0..6 {
            let y = y_start + dy;
            if y >= height {
                break;
            }
            let row_offset = (y * width) as usize;
            for x in 0..width {
                if let Some(col_idx) = indexed_pixels[row_offset + (x as usize)] {
                    if !active_colors.contains(&col_idx) {
                        active_colors.push(col_idx);
                    }
                }
            }
        }

        for &col_idx in &active_colors {
            // Select active color
            out.push_str(&format!("#{col_idx}"));

            // Build sixel characters for each column
            let mut line_sixels = Vec::with_capacity(width as usize);
            for x in 0..width {
                let mut sixel_bits = 0u8;
                for bit in 0..6 {
                    let y = y_start + bit;
                    if y < height {
                        let idx = (y * width + x) as usize;
                        if indexed_pixels[idx] == Some(col_idx) {
                            sixel_bits |= 1 << bit;
                        }
                    }
                }
                // Sixel character = 0x3F ('?') + 6-bit value
                line_sixels.push((0x3F + sixel_bits) as char);
            }

            // Run-length encode consecutive identical sixel characters
            emit_run_length_encoded(&line_sixels, &mut out);

            // Carriage return within same band ($)
            out.push('$');
        }

        // Newline to next 6-pixel band (-)
        out.push('-');
    }

    // Sixel DCS Terminator: \x1b\
    out.push_str("\x1b\\");
    out
}

/// Quantizes an 8-bit channel to 6 uniform levels (0..=5).
fn quantize_channel_6(val: u8) -> u8 {
    let step = (val as u32 * 5 + 127) / 255;
    (step * 51) as u8
}

/// Emits run-length encoded Sixel stream (!count<char>).
fn emit_run_length_encoded(chars: &[char], out: &mut String) {
    if chars.is_empty() {
        return;
    }

    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        let mut count = 1;
        while i + count < chars.len() && chars[i + count] == ch {
            count += 1;
        }

        if count >= 4 {
            out.push_str(&format!("!{count}{ch}"));
        } else {
            for _ in 0..count {
                out.push(ch);
            }
        }
        i += count;
    }
}
