//! Zero-dependency color science engine: sRGB, HSLA, Oklch, hex parsing, blending, and WCAG contrast.

/// 8-bit per channel RGBA Color.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const TRANSPARENT: Self = Self::new(0, 0, 0, 0);
    pub const BLACK: Self = Self::new(0, 0, 0, 255);
    pub const WHITE: Self = Self::new(255, 255, 255, 255);
    pub const RED: Self = Self::new(255, 0, 0, 255);
    pub const GREEN: Self = Self::new(0, 255, 0, 255);
    pub const BLUE: Self = Self::new(0, 0, 255, 255);

    #[inline(always)]
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    #[inline(always)]
    #[allow(clippy::self_named_constructors)]
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    #[inline(always)]
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    /// Converts normalized [0.0, 1.0] float components to 8-bit RGBA.
    pub fn from_f32(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self {
            r: (r.clamp(0.0, 1.0) * 255.0).round() as u8,
            g: (g.clamp(0.0, 1.0) * 255.0).round() as u8,
            b: (b.clamp(0.0, 1.0) * 255.0).round() as u8,
            a: (a.clamp(0.0, 1.0) * 255.0).round() as u8,
        }
    }

    /// Converts to normalized [0.0, 1.0] float tuple.
    #[inline(always)]
    pub fn to_f32(self) -> (f32, f32, f32, f32) {
        (
            self.r as f32 / 255.0,
            self.g as f32 / 255.0,
            self.b as f32 / 255.0,
            self.a as f32 / 255.0,
        )
    }

    #[inline(always)]
    pub fn r(self) -> f32 { self.r as f32 / 255.0 }
    #[inline(always)]
    pub fn g(self) -> f32 { self.g as f32 / 255.0 }
    #[inline(always)]
    pub fn b(self) -> f32 { self.b as f32 / 255.0 }
    #[inline(always)]
    pub fn a(self) -> f32 { self.a as f32 / 255.0 }

    #[inline(always)]
    pub fn to_rgba8(self) -> (u8, u8, u8, u8) {
        (self.r, self.g, self.b, self.a)
    }

    /// Parses CSS hex color strings: `#RGB`, `#RGBA`, `#RRGGBB`, `#RRGGBBAA`.
    /// Operates with zero heap allocations.
    pub fn parse_hex(hex: &str) -> Option<Self> {
        let s = hex.strip_prefix('#').unwrap_or(hex);
        let bytes = s.as_bytes();

        match bytes.len() {
            // #RGB (3 hex digits)
            3 => {
                let r = parse_hex_digit(bytes[0])?;
                let g = parse_hex_digit(bytes[1])?;
                let b = parse_hex_digit(bytes[2])?;
                Some(Self::new(r * 17, g * 17, b * 17, 255))
            }
            // #RGBA (4 hex digits)
            4 => {
                let r = parse_hex_digit(bytes[0])?;
                let g = parse_hex_digit(bytes[1])?;
                let b = parse_hex_digit(bytes[2])?;
                let a = parse_hex_digit(bytes[3])?;
                Some(Self::new(r * 17, g * 17, b * 17, a * 17))
            }
            // #RRGGBB (6 hex digits)
            6 => {
                let r = parse_hex_pair(bytes[0], bytes[1])?;
                let g = parse_hex_pair(bytes[2], bytes[3])?;
                let b = parse_hex_pair(bytes[4], bytes[5])?;
                Some(Self::new(r, g, b, 255))
            }
            // #RRGGBBAA (8 hex digits)
            8 => {
                let r = parse_hex_pair(bytes[0], bytes[1])?;
                let g = parse_hex_pair(bytes[2], bytes[3])?;
                let b = parse_hex_pair(bytes[4], bytes[5])?;
                let a = parse_hex_pair(bytes[6], bytes[7])?;
                Some(Self::new(r, g, b, a))
            }
            _ => None,
        }
    }

    /// Linear Porter-Duff Source-Over alpha composite over a background color.
    pub fn blend_over(self, bg: Self) -> Self {
        let (sr, sg, sb, sa) = self.to_f32();
        let (br, bg_g, bb, ba) = bg.to_f32();

        let out_a = sa + ba * (1.0 - sa);
        if out_a <= 0.0 {
            return Self::TRANSPARENT;
        }

        let out_r = (sr * sa + br * ba * (1.0 - sa)) / out_a;
        let out_g = (sg * sa + bg_g * ba * (1.0 - sa)) / out_a;
        let out_b = (sb * sa + bb * ba * (1.0 - sa)) / out_a;

        Self::from_f32(out_r, out_g, out_b, out_a)
    }

    /// Computes relative luminance according to WCAG 2.1 formula.
    pub fn relative_luminance(self) -> f32 {
        let (r, g, b, _) = self.to_f32();
        let r_lin = to_linear_srgb(r);
        let g_lin = to_linear_srgb(g);
        let b_lin = to_linear_srgb(b);

        0.2126 * r_lin + 0.7152 * g_lin + 0.0722 * b_lin
    }

    /// Computes WCAG 2.1 contrast ratio between two colors (range: 1.0 to 21.0).
    pub fn contrast_ratio(self, other: Self) -> f32 {
        let l1 = self.relative_luminance();
        let l2 = other.relative_luminance();

        let lighter = l1.max(l2);
        let darker = l1.min(l2);

        (lighter + 0.05) / (darker + 0.05)
    }

    /// Packs into 32-bit integer: 0xAABBGGRR (Little-Endian / Vulkan format).
    #[inline(always)]
    pub const fn to_u32_le(self) -> u32 {
        (self.r as u32) | ((self.g as u32) << 8) | ((self.b as u32) << 16) | ((self.a as u32) << 24)
    }

    /// Formats as standard `#rrggbbaa` hex string.
    pub fn to_hex_string(self) -> String {
        format!("#{:02x}{:02x}{:02x}{:02x}", self.r, self.g, self.b, self.a)
    }
}

fn to_linear_srgb(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn parse_hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn parse_hex_pair(hi: u8, lo: u8) -> Option<u8> {
    let h = parse_hex_digit(hi)?;
    let l = parse_hex_digit(lo)?;
    Some((h << 4) | l)
}

/// Hue, Saturation, Lightness, Alpha color representation.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Hsla {
    pub h: f32, // 0.0 ..= 360.0 degrees
    pub s: f32, // 0.0 ..= 1.0
    pub l: f32, // 0.0 ..= 1.0
    pub a: f32, // 0.0 ..= 1.0
}

impl Hsla {
    #[inline(always)]
    pub const fn new(h: f32, s: f32, l: f32, a: f32) -> Self {
        Self { h, s, l, a }
    }

    pub fn to_rgba(self) -> Rgba {
        if self.s <= 0.0 {
            return Rgba::from_f32(self.l, self.l, self.l, self.a);
        }

        let q = if self.l < 0.5 {
            self.l * (1.0 + self.s)
        } else {
            self.l + self.s - (self.l * self.s)
        };
        let p = 2.0 * self.l - q;

        let hk = (self.h % 360.0) / 360.0;
        let r = hue_to_rgb(p, q, hk + 1.0 / 3.0);
        let g = hue_to_rgb(p, q, hk);
        let b = hue_to_rgb(p, q, hk - 1.0 / 3.0);

        Rgba::from_f32(r, g, b, self.a)
    }

    pub fn from_rgba(rgba: Rgba) -> Self {
        let (r, g, b, a) = rgba.to_f32();
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let delta = max - min;

        let l = (max + min) / 2.0;

        if delta.abs() < 1e-5 {
            return Self::new(0.0, 0.0, l, a);
        }

        let s = if l < 0.5 {
            delta / (max + min)
        } else {
            delta / (2.0 - max - min)
        };

        let mut h = if (max - r).abs() < 1e-5 {
            (g - b) / delta + (if g < b { 6.0 } else { 0.0 })
        } else if (max - g).abs() < 1e-5 {
            (b - r) / delta + 2.0
        } else {
            (r - g) / delta + 4.0
        };

        h *= 60.0;
        Self::new(h, s, l, a)
    }
}

fn hue_to_rgb(p: f32, q: f32, mut t: f32) -> f32 {
    if t < 0.0 { t += 1.0; }
    if t > 1.0 { t -= 1.0; }
    if t < 1.0 / 6.0 { return p + (q - p) * 6.0 * t; }
    if t < 1.0 / 2.0 { return q; }
    if t < 2.0 / 3.0 { return p + (q - p) * (2.0 / 3.0 - t) * 6.0; }
    p
}

/// Oklch Perceptually Uniform Color (CSS Color Module Level 4).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Oklch {
    pub l: f32, // Perceptual lightness: 0.0 ..= 1.0
    pub c: f32, // Chroma: 0.0 ..= ~0.4
    pub h: f32, // Hue angle: 0.0 ..= 360.0 degrees
    pub a: f32, // Alpha: 0.0 ..= 1.0
}

impl Oklch {
    #[inline(always)]
    pub const fn new(l: f32, c: f32, h: f32, a: f32) -> Self {
        Self { l, c, h, a }
    }

    /// Converts Oklch to standard sRGB Rgba.
    #[allow(clippy::excessive_precision)]
    pub fn to_rgba(self) -> Rgba {
        let h_rad = self.h.to_radians();
        let a_star = self.c * h_rad.cos();
        let b_star = self.c * h_rad.sin();

        // Oklab to linear LMS
        let l_ = self.l + 0.3963377774 * a_star + 0.2158037573 * b_star;
        let m_ = self.l - 0.1055613458 * a_star - 0.0638541728 * b_star;
        let s_ = self.l - 0.0894841775 * a_star - 1.2914855480 * b_star;

        let l = l_ * l_ * l_;
        let m = m_ * m_ * m_;
        let s = s_ * s_ * s_;

        // Linear LMS to linear sRGB
        let r_lin = 4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s;
        let g_lin = -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s;
        let b_lin = -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s;

        Rgba::from_f32(from_linear_srgb(r_lin), from_linear_srgb(g_lin), from_linear_srgb(b_lin), self.a)
    }
}

fn from_linear_srgb(c: f32) -> f32 {
    if c <= 0.0031308 {
        12.92 * c
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}
