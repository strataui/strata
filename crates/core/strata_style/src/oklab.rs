//! Perceptually uniform Oklab color space conversion and continuous interpolation (W3C CSS Color 4).

#![allow(clippy::excessive_precision)]

use strata_primitives::color::Rgba;

/// Converts an sRGB color component in `[0.0, 1.0]` to linear gamma sRGB.
#[inline(always)]
pub fn srgb_to_linear(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Converts a linear gamma sRGB component in `[0.0, 1.0]` to standard gamma sRGB.
#[inline(always)]
pub fn linear_to_srgb(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.0031308 {
        12.92 * c
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// Converts an RGBA color to Oklab coordinates `(L, a, b, alpha)`.
///
/// `L` represents perceived lightness in `[0.0, 1.0]`.
/// `a` represents the green-red axis (approx `-0.4` to `+0.4`).
/// `b` represents the blue-yellow axis (approx `-0.4` to `+0.4`).
pub fn rgba_to_oklab(c: Rgba) -> (f32, f32, f32, f32) {
    let (r, g, b, alpha) = c.to_f32();
    let r_lin = srgb_to_linear(r);
    let g_lin = srgb_to_linear(g);
    let b_lin = srgb_to_linear(b);

    // Approximate LMS cone response from linear sRGB
    let l = 0.4122214708 * r_lin + 0.5363325363 * g_lin + 0.0514459929 * b_lin;
    let m = 0.2119034982 * r_lin + 0.6806995451 * g_lin + 0.1073969566 * b_lin;
    let s = 0.0883024619 * r_lin + 0.2817188376 * g_lin + 0.6299787005 * b_lin;

    let l_ = l.max(0.0).cbrt();
    let m_ = m.max(0.0).cbrt();
    let s_ = s.max(0.0).cbrt();

    let lab_l = 0.2104542553 * l_ + 0.7936177850 * m_ - 0.0040720468 * s_;
    let lab_a = 1.9779984951 * l_ - 2.4285922050 * m_ + 0.4505937099 * s_;
    let lab_b = 0.0259040371 * l_ + 0.7827717662 * m_ - 0.8086757660 * s_;

    (lab_l, lab_a, lab_b, alpha)
}

/// Converts Oklab coordinates `(L, a, b, alpha)` back to an RGBA color with gamut clamping.
pub fn oklab_to_rgba(l: f32, a: f32, b: f32, alpha: f32) -> Rgba {
    let l_ = l + 0.3963377774 * a + 0.2158037573 * b;
    let m_ = l - 0.1055613458 * a - 0.0638541728 * b;
    let s_ = l - 0.0894841775 * a - 1.2914855480 * b;

    let l_lin = l_ * l_ * l_;
    let m_lin = m_ * m_ * m_;
    let s_lin = s_ * s_ * s_;

    let r_lin = 4.0767439362 * l_lin - 3.3077115913 * m_lin + 0.2309699292 * s_lin;
    let g_lin = -1.2684380046 * l_lin + 2.6097574011 * m_lin - 0.3413193965 * s_lin;
    let b_lin = -0.0041960863 * l_lin - 0.7034186147 * m_lin + 1.7076147010 * s_lin;

    let r = linear_to_srgb(r_lin);
    let g = linear_to_srgb(g_lin);
    let b = linear_to_srgb(b_lin);

    Rgba::from_f32(r, g, b, alpha.clamp(0.0, 1.0))
}

/// Continuous color interpolation performed in the perceptually uniform Oklab space (W3C CSS Color 4).
///
/// Eliminates the muddy gray/brown artifacts of naïve sRGB interpolation and maintains perceptual
/// brightness consistency across hue transitions. Employs premultiplied alpha weighting.
pub fn interpolate_color_oklab(from: Rgba, to: Rgba, t: f32) -> Rgba {
    let t = t.clamp(0.0, 1.0);
    let (l1, a1, b1, alpha1) = rgba_to_oklab(from);
    let (l2, a2, b2, alpha2) = rgba_to_oklab(to);

    // Alpha channel interpolation
    let out_alpha = alpha1 + (alpha2 - alpha1) * t;
    if out_alpha <= 1e-4 {
        return Rgba::TRANSPARENT;
    }

    // Premultiplied coordinate weighting prevents dark halos during opacity fades
    let (out_l, out_a, out_b) = if (alpha1 - 1.0).abs() < 1e-4 && (alpha2 - 1.0).abs() < 1e-4 {
        (
            l1 + (l2 - l1) * t,
            a1 + (a2 - a1) * t,
            b1 + (b2 - b1) * t,
        )
    } else {
        (
            (l1 * alpha1 * (1.0 - t) + l2 * alpha2 * t) / out_alpha,
            (a1 * alpha1 * (1.0 - t) + a2 * alpha2 * t) / out_alpha,
            (b1 * alpha1 * (1.0 - t) + b2 * alpha2 * t) / out_alpha,
        )
    };

    oklab_to_rgba(out_l, out_a, out_b, out_alpha)
}
