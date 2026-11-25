//! Unicode Box Drawing and 4-Way Border Junction Mask Solver.

use crate::screen::TerminalScreen;
use crate::types::{AttrBits, Color24};

/// Cardinal direction flags for border junction evaluation.
pub mod cardinal {
    pub const NORTH: u8 = 1 << 0;
    pub const SOUTH: u8 = 1 << 1;
    pub const EAST: u8 = 1 << 2;
    pub const WEST: u8 = 1 << 3;
}

pub use cardinal as Cardinal;

/// Border styling variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BorderJunctionStyle {
    #[default]
    Solid,
    Rounded,
    Double,
    Heavy,
}

/// Resolves an 8-bit cardinal junction mask into a Unicode box-drawing character.
pub fn resolve_border_junction(mask: u8, style: BorderJunctionStyle) -> char {
    use cardinal::*;

    match style {
        BorderJunctionStyle::Rounded => match mask & 0x0F {
            // Corners
            m if m == (SOUTH | EAST) => '╭',
            m if m == (SOUTH | WEST) => '╮',
            m if m == (NORTH | EAST) => '╰',
            m if m == (NORTH | WEST) => '╯',
            // Straights
            m if m == (EAST | WEST) => '─',
            m if m == (NORTH | SOUTH) => '│',
            // T-junctions
            m if m == (NORTH | SOUTH | EAST) => '├',
            m if m == (NORTH | SOUTH | WEST) => '┤',
            m if m == (SOUTH | EAST | WEST) => '┬',
            m if m == (NORTH | EAST | WEST) => '┴',
            // 4-way cross
            m if m == (NORTH | SOUTH | EAST | WEST) => '┼',
            // Endpoints
            m if (m & (NORTH | SOUTH)) != 0 => '│',
            _ => '─',
        },
        BorderJunctionStyle::Solid => match mask & 0x0F {
            m if m == (SOUTH | EAST) => '┌',
            m if m == (SOUTH | WEST) => '┐',
            m if m == (NORTH | EAST) => '└',
            m if m == (NORTH | WEST) => '┘',
            m if m == (EAST | WEST) => '─',
            m if m == (NORTH | SOUTH) => '│',
            m if m == (NORTH | SOUTH | EAST) => '├',
            m if m == (NORTH | SOUTH | WEST) => '┤',
            m if m == (SOUTH | EAST | WEST) => '┬',
            m if m == (NORTH | EAST | WEST) => '┴',
            m if m == (NORTH | SOUTH | EAST | WEST) => '┼',
            m if (m & (NORTH | SOUTH)) != 0 => '│',
            _ => '─',
        },
        BorderJunctionStyle::Double => match mask & 0x0F {
            m if m == (SOUTH | EAST) => '╔',
            m if m == (SOUTH | WEST) => '╗',
            m if m == (NORTH | EAST) => '╚',
            m if m == (NORTH | WEST) => '╝',
            m if m == (EAST | WEST) => '═',
            m if m == (NORTH | SOUTH) => '║',
            m if m == (NORTH | SOUTH | EAST) => '╠',
            m if m == (NORTH | SOUTH | WEST) => '╣',
            m if m == (SOUTH | EAST | WEST) => '╦',
            m if m == (NORTH | EAST | WEST) => '╩',
            m if m == (NORTH | SOUTH | EAST | WEST) => '╬',
            m if (m & (NORTH | SOUTH)) != 0 => '║',
            _ => '═',
        },
        BorderJunctionStyle::Heavy => match mask & 0x0F {
            m if m == (SOUTH | EAST) => '┏',
            m if m == (SOUTH | WEST) => '┓',
            m if m == (NORTH | EAST) => '┗',
            m if m == (NORTH | WEST) => '┛',
            m if m == (EAST | WEST) => '━',
            m if m == (NORTH | SOUTH) => '┃',
            m if m == (NORTH | SOUTH | EAST) => '┣',
            m if m == (NORTH | SOUTH | WEST) => '┫',
            m if m == (SOUTH | EAST | WEST) => '┳',
            m if m == (NORTH | EAST | WEST) => '┻',
            m if m == (NORTH | SOUTH | EAST | WEST) => '╋',
            m if (m & (NORTH | SOUTH)) != 0 => '┃',
            _ => '━',
        },
    }
}

/// Draws an empty rectangular bordered box onto the terminal screen.
pub fn draw_box(
    screen: &mut TerminalScreen,
    col: u16,
    row: u16,
    width: u16,
    height: u16,
    style: BorderJunctionStyle,
    fg: Color24,
    bg: Color24,
    z_index: i16,
) {
    if width < 2 || height < 2 {
        return;
    }

    use cardinal::*;

    let tl = resolve_border_junction(SOUTH | EAST, style);
    let tr = resolve_border_junction(SOUTH | WEST, style);
    let bl = resolve_border_junction(NORTH | EAST, style);
    let br = resolve_border_junction(NORTH | WEST, style);
    let h = resolve_border_junction(EAST | WEST, style);
    let v = resolve_border_junction(NORTH | SOUTH, style);

    let right = col + width - 1;
    let bottom = row + height - 1;

    // Corners
    screen.put_char(col, row, tl, fg, bg, AttrBits::NONE, z_index);
    screen.put_char(right, row, tr, fg, bg, AttrBits::NONE, z_index);
    screen.put_char(col, bottom, bl, fg, bg, AttrBits::NONE, z_index);
    screen.put_char(right, bottom, br, fg, bg, AttrBits::NONE, z_index);

    // Horizontal edges
    for c in (col + 1)..right {
        screen.put_char(c, row, h, fg, bg, AttrBits::NONE, z_index);
        screen.put_char(c, bottom, h, fg, bg, AttrBits::NONE, z_index);
    }

    // Vertical edges
    for r in (row + 1)..bottom {
        screen.put_char(col, r, v, fg, bg, AttrBits::NONE, z_index);
        screen.put_char(right, r, v, fg, bg, AttrBits::NONE, z_index);
    }
}
