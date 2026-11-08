//! Graphics Degradation Matrix: Half-Block (2x vertical resolution) and Braille (2x4 dot matrix) renderers.

use crate::screen::TerminalScreen;
use crate::types::{AttrBits, Color24};
use strata_primitives::color::Rgba;

/// Renders an image using Unicode Half-Block characters (`▀` U+2580),
/// effectively doubling the vertical resolution of terminal character cells.
pub fn render_half_block(
    pixels: &[Rgba],
    img_width: u32,
    img_height: u32,
    screen: &mut TerminalScreen,
    start_col: u16,
    start_row: u16,
    z_index: i16,
) {
    if img_width == 0 || img_height == 0 {
        return;
    }

    let cell_rows = img_height.div_ceil(2);

    for r in 0..cell_rows {
        let dest_row = start_row + (r as u16);
        if dest_row >= screen.rows() {
            break;
        }

        let py_top = r * 2;
        let py_bottom = py_top + 1;

        for c in 0..img_width {
            let dest_col = start_col + (c as u16);
            if dest_col >= screen.cols() {
                break;
            }

            let top_idx = (py_top * img_width + c) as usize;
            let top_color = if top_idx < pixels.len() {
                Color24::from_rgba(pixels[top_idx])
            } else {
                Color24::Default
            };

            let bottom_color = if py_bottom < img_height {
                let bot_idx = (py_bottom * img_width + c) as usize;
                if bot_idx < pixels.len() {
                    Color24::from_rgba(pixels[bot_idx])
                } else {
                    Color24::Default
                }
            } else {
                Color24::Default
            };

            screen.put_char(
                dest_col,
                dest_row,
                '▀',
                top_color,
                bottom_color,
                AttrBits::NONE,
                z_index,
            );
        }
    }
}

/// Encodes a 2x4 dot binary matrix into a single Unicode Braille character (U+2800..U+28FF).
pub fn braille_char_from_dots(dots: [bool; 8]) -> char {
    let mut code: u32 = 0x2800;
    if dots[0] { code |= 0x01; } // Row 0, Col 0
    if dots[1] { code |= 0x02; } // Row 1, Col 0
    if dots[2] { code |= 0x04; } // Row 2, Col 0
    if dots[3] { code |= 0x08; } // Row 0, Col 1
    if dots[4] { code |= 0x10; } // Row 1, Col 1
    if dots[5] { code |= 0x20; } // Row 2, Col 1
    if dots[6] { code |= 0x40; } // Row 3, Col 0
    if dots[7] { code |= 0x80; } // Row 3, Col 1

    char::from_u32(code).unwrap_or(' ')
}

/// Renders a monochrome bitmap using Unicode Braille patterns (4x vertical, 2x horizontal resolution).
pub fn render_braille(
    bitmap: &[bool],
    img_width: u32,
    img_height: u32,
    screen: &mut TerminalScreen,
    start_col: u16,
    start_row: u16,
    fg: Color24,
    bg: Color24,
    z_index: i16,
) {
    let char_cols = img_width.div_ceil(2);
    let char_rows = img_height.div_ceil(4);

    for r in 0..char_rows {
        let dest_row = start_row + (r as u16);
        if dest_row >= screen.rows() {
            break;
        }

        let base_y = r * 4;

        for c in 0..char_cols {
            let dest_col = start_col + (c as u16);
            if dest_col >= screen.cols() {
                break;
            }

            let base_x = c * 2;
            let mut dots = [false; 8];

            // Sample 2x4 dot grid
            let get_pixel = |x: u32, y: u32| -> bool {
                if x < img_width && y < img_height {
                    let idx = (y * img_width + x) as usize;
                    bitmap.get(idx).copied().unwrap_or(false)
                } else {
                    false
                }
            };

            dots[0] = get_pixel(base_x, base_y);
            dots[1] = get_pixel(base_x, base_y + 1);
            dots[2] = get_pixel(base_x, base_y + 2);
            dots[6] = get_pixel(base_x, base_y + 3);

            dots[3] = get_pixel(base_x + 1, base_y);
            dots[4] = get_pixel(base_x + 1, base_y + 1);
            dots[5] = get_pixel(base_x + 1, base_y + 2);
            dots[7] = get_pixel(base_x + 1, base_y + 3);

            let ch = braille_char_from_dots(dots);
            screen.put_char(dest_col, dest_row, ch, fg, bg, AttrBits::NONE, z_index);
        }
    }
}
