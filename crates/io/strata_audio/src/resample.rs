//! Dynamic sample rate conversion and interpolation filters.
//!
//! Provides linear, Catmull-Rom cubic spline, and windowed band-limited sinc interpolation
//! to resample PCM audio streams between arbitrary sample rates without audible aliasing.

/// Resampling interpolation quality mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResampleQuality {
    /// Fast 2-point linear interpolation ($O(1)$ per sample).
    Linear,
    /// 4-point Catmull-Rom cubic polynomial interpolation ($C^1$ continuity).
    #[default]
    Cubic,
    /// Windowed band-limited sinc interpolation with anti-aliasing cutoff and Blackman-Harris window.
    Sinc {
        /// Half-width number of taps (e.g. 16 or 32 for 33 or 65 total filter kernel points).
        taps: usize,
    },
}

/// Sinc function $\sin(\pi x) / (\pi x)$ with $\text{sinc}(0) = 1.0$.
#[inline]
pub fn sinc(x: f32) -> f32 {
    if x.abs() < 1e-6 {
        1.0
    } else {
        let px = std::f32::consts::PI * x;
        px.sin() / px
    }
}

/// 4-term Blackman-Harris window function over normalized domain $[-1.0, 1.0]$.
#[inline]
pub fn blackman_harris_window(x: f32) -> f32 {
    if x.abs() > 1.0 {
        return 0.0;
    }
    let u = (x + 1.0) * 0.5;
    let pi2 = 2.0 * std::f32::consts::PI * u;
    let pi4 = 4.0 * std::f32::consts::PI * u;
    let pi6 = 6.0 * std::f32::consts::PI * u;

    0.35875 - 0.48829 * pi2.cos() + 0.14128 * pi4.cos() - 0.01168 * pi6.cos()
}

/// Evaluates linear interpolation at continuous position `t`.
#[inline]
pub fn interpolate_linear(samples: &[f32], t: f32) -> f32 {
    let n = samples.len();
    if n == 0 {
        return 0.0;
    }
    if n == 1 || t <= 0.0 {
        return samples[0];
    }
    if t >= (n - 1) as f32 {
        return samples[n - 1];
    }
    let idx = t.floor() as usize;
    let frac = t - idx as f32;
    let s0 = samples[idx];
    let s1 = samples[(idx + 1).min(n - 1)];
    s0 + (s1 - s0) * frac
}

/// Evaluates Catmull-Rom cubic Hermite spline interpolation at continuous position `t`.
#[inline]
pub fn interpolate_cubic(samples: &[f32], t: f32) -> f32 {
    let n = samples.len();
    if n == 0 {
        return 0.0;
    }
    if n == 1 || t <= 0.0 {
        return samples[0];
    }
    if t >= (n - 1) as f32 {
        return samples[n - 1];
    }
    let idx = t.floor() as isize;
    let frac = t - idx as f32;

    let get_clamped = |i: isize| -> f32 {
        let clamped = i.clamp(0, (n - 1) as isize) as usize;
        samples[clamped]
    };

    let ym1 = get_clamped(idx - 1);
    let y0 = get_clamped(idx);
    let y1 = get_clamped(idx + 1);
    let y2 = get_clamped(idx + 2);

    let a0 = -0.5 * ym1 + 1.5 * y0 - 1.5 * y1 + 0.5 * y2;
    let a1 = ym1 - 2.5 * y0 + 2.0 * y1 - 0.5 * y2;
    let a2 = -0.5 * ym1 + 0.5 * y1;
    let a3 = y0;

    ((a0 * frac + a1) * frac + a2) * frac + a3
}

/// Evaluates windowed band-limited sinc interpolation at continuous position `t`.
#[inline]
pub fn interpolate_sinc(samples: &[f32], t: f32, taps: usize, cutoff: f32) -> f32 {
    let n = samples.len();
    if n == 0 {
        return 0.0;
    }
    if n == 1 || t <= 0.0 {
        return samples[0];
    }
    if t >= (n - 1) as f32 {
        return samples[n - 1];
    }

    let taps = taps.max(2);
    let center = t.floor() as isize;
    let mut sum = 0.0f32;
    let mut weight_sum = 0.0f32;

    let k_min = center - taps as isize;
    let k_max = center + taps as isize + 1;

    for k in k_min..=k_max {
        let delta = t - k as f32;
        let window_arg = delta / taps as f32;
        if window_arg.abs() <= 1.0 {
            let win = blackman_harris_window(window_arg);
            let s = sinc(cutoff * delta) * cutoff;
            let weight = win * s;

            let sample_idx = k.clamp(0, (n - 1) as isize) as usize;
            sum += samples[sample_idx] * weight;
            weight_sum += weight;
        }
    }

    if weight_sum.abs() > 1e-6 {
        sum / weight_sum
    } else {
        sum
    }
}

/// Resamples a single PCM audio channel from `source_rate` to `target_rate` using the requested quality.
pub fn resample_channel(
    channel: &[f32],
    source_rate: u32,
    target_rate: u32,
    quality: ResampleQuality,
) -> Vec<f32> {
    if channel.is_empty() || source_rate == 0 || target_rate == 0 {
        return Vec::new();
    }
    if source_rate == target_rate {
        return channel.to_vec();
    }

    let ratio = source_rate as f64 / target_rate as f64;
    let target_len =
        ((channel.len() as f64) * (target_rate as f64 / source_rate as f64)).round() as usize;
    if target_len == 0 {
        return Vec::new();
    }

    let mut output = Vec::with_capacity(target_len);

    match quality {
        ResampleQuality::Linear => {
            for m in 0..target_len {
                let t = (m as f64 * ratio) as f32;
                output.push(interpolate_linear(channel, t));
            }
        }
        ResampleQuality::Cubic => {
            for m in 0..target_len {
                let t = (m as f64 * ratio) as f32;
                output.push(interpolate_cubic(channel, t));
            }
        }
        ResampleQuality::Sinc { taps } => {
            let cutoff = ((target_rate as f32) / (source_rate as f32)).min(1.0);
            for m in 0..target_len {
                let t = (m as f64 * ratio) as f32;
                output.push(interpolate_sinc(channel, t, taps, cutoff));
            }
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sinc_and_window_properties() {
        assert_eq!(sinc(0.0), 1.0);
        assert!(sinc(1.0).abs() < 1e-6);
        assert!(sinc(2.0).abs() < 1e-6);

        // Window symmetry
        assert!((blackman_harris_window(-0.5) - blackman_harris_window(0.5)).abs() < 1e-6);
        assert!(blackman_harris_window(0.0) > 0.9);
        assert_eq!(blackman_harris_window(1.5), 0.0);
    }

    #[test]
    fn test_resample_dc_signal() {
        let dc = vec![0.75f32; 100];
        let resampled_cubic = resample_channel(&dc, 44100, 48000, ResampleQuality::Cubic);
        assert_eq!(resampled_cubic.len(), 109);
        for &s in &resampled_cubic[2..107] {
            assert!(
                (s - 0.75).abs() < 1e-3,
                "DC level must be preserved: got {}",
                s
            );
        }

        let resampled_sinc =
            resample_channel(&dc, 48000, 44100, ResampleQuality::Sinc { taps: 16 });
        assert_eq!(resampled_sinc.len(), 92);
        for &s in &resampled_sinc[5..85] {
            assert!(
                (s - 0.75).abs() < 1e-3,
                "DC level must be preserved: got {}",
                s
            );
        }
    }
}
