//! Terminal Character Grid Types, 24-Bit TrueColor, and Text Attributes.

use strata_primitives::color::Rgba;

/// 24-bit TrueColor representation with ANSI fallback tiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Color24 {
    #[default]
    Default,
    Ansi16(u8),
    Ansi256(u8),
    Rgb(u8, u8, u8),
}

impl Color24 {
    pub const BLACK: Self = Self::Rgb(0, 0, 0);
    pub const WHITE: Self = Self::Rgb(255, 255, 255);

    /// Converts an RGBA color to 24-bit TrueColor.
    pub fn from_rgba(rgba: Rgba) -> Self {
        Self::Rgb(rgba.r, rgba.g, rgba.b)
    }

    /// Emits ANSI SGR escape sequence for foreground color.
    pub fn write_fg_ansi(&self, out: &mut String) {
        match self {
            Self::Default => out.push_str("\x1b[39m"),
            Self::Ansi16(c) => {
                if *c < 8 {
                    out.push_str(&format!("\x1b[{}m", 30 + c));
                } else {
                    out.push_str(&format!("\x1b[{}m", 90 + (c - 8)));
                }
            }
            Self::Ansi256(c) => out.push_str(&format!("\x1b[38;5;{}m", c)),
            Self::Rgb(r, g, b) => out.push_str(&format!("\x1b[38;2;{};{};{}m", r, g, b)),
        }
    }

    /// Emits ANSI SGR escape sequence for background color.
    pub fn write_bg_ansi(&self, out: &mut String) {
        match self {
            Self::Default => out.push_str("\x1b[49m"),
            Self::Ansi16(c) => {
                if *c < 8 {
                    out.push_str(&format!("\x1b[{}m", 40 + c));
                } else {
                    out.push_str(&format!("\x1b[{}m", 100 + (c - 8)));
                }
            }
            Self::Ansi256(c) => out.push_str(&format!("\x1b[48;5;{}m", c)),
            Self::Rgb(r, g, b) => out.push_str(&format!("\x1b[48;2;{};{};{}m", r, g, b)),
        }
    }
}

/// Bit flags for text formatting.
pub struct AttrBits;

impl AttrBits {
    pub const NONE: u8 = 0;
    pub const BOLD: u8 = 1 << 0;
    pub const DIM: u8 = 1 << 1;
    pub const ITALIC: u8 = 1 << 2;
    pub const UNDERLINE: u8 = 1 << 3;
    pub const REVERSE: u8 = 1 << 4;
    pub const STRIKETHROUGH: u8 = 1 << 5;
}

/// A single cell in the terminal character grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CharCell {
    pub ch: char,
    pub fg: Color24,
    pub bg: Color24,
    pub style: u8,
    pub is_wide_continuation: bool,
    pub z_index: i16,
}

impl Default for CharCell {
    fn default() -> Self {
        Self {
            ch: ' ',
            fg: Color24::Default,
            bg: Color24::Default,
            style: AttrBits::NONE,
            is_wide_continuation: false,
            z_index: 0,
        }
    }
}
