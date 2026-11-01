//! Radix-2 Cooley-Tukey Fast Fourier Transform (FFT) and spectral magnitude analyzer.

use std::f32::consts::PI;

/// Complex number for FFT computations.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Complex {
    pub re: f32,
    pub im: f32,
}

impl Complex {
    pub const fn new(re: f32, im: f32) -> Self {
        Self { re, im }
    }

    pub fn add(self, other: Complex) -> Complex {
        Complex::new(self.re + other.re, self.im + other.im)
    }

    pub fn sub(self, other: Complex) -> Complex {
        Complex::new(self.re - other.re, self.im - other.im)
    }

    pub fn mul(self, other: Complex) -> Complex {
        Complex::new(
            self.re * other.re - self.im * other.im,
            self.re * other.im + self.im * other.re,
        )
    }

    pub fn magnitude(self) -> f32 {
        (self.re * self.re + self.im * self.im).sqrt()
    }
}

/// In-place Radix-2 Decimation-In-Time (DIT) Fast Fourier Transform.
/// Length of `buffer` MUST be a power of two.
pub fn fft_radix2(buffer: &mut [Complex]) {
    let n = buffer.len();
    if n <= 1 {
        return;
    }
    assert!(n.is_power_of_two(), "FFT size must be a power of two");

    // 1. Bit-reversal permutation
    let mut j = 0;
    for i in 0..n {
        if i < j {
            buffer.swap(i, j);
        }
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
    }

    // 2. Cooley-Tukey Butterfly stages
    let mut len = 2;
    while len <= n {
        let half = len / 2;
        let angle = -2.0 * PI / len as f32;
        let w_unit = Complex::new(angle.cos(), angle.sin());

        let mut i = 0;
        while i < n {
            let mut w = Complex::new(1.0, 0.0);
            for k in 0..half {
                let u = buffer[i + k];
                let v = buffer[i + k + half].mul(w);
                buffer[i + k] = u.add(v);
                buffer[i + k + half] = u.sub(v);
                w = w.mul(w_unit);
            }
            i += len;
        }
        len <<= 1;
    }
}

/// Computes the magnitude spectrum of an audio sample buffer (first N/2 bins).
pub fn compute_magnitude_spectrum(samples: &[f32], fft_size: usize) -> Vec<f32> {
    assert!(fft_size.is_power_of_two(), "FFT size must be a power of two");
    let mut complex_buf: Vec<Complex> = Vec::with_capacity(fft_size);

    // Apply Hann window and pad or truncate to fft_size
    for i in 0..fft_size {
        if i < samples.len() {
            let window = 0.5 * (1.0 - (2.0 * PI * i as f32 / (fft_size - 1) as f32).cos());
            complex_buf.push(Complex::new(samples[i] * window, 0.0));
        } else {
            complex_buf.push(Complex::default());
        }
    }

    fft_radix2(&mut complex_buf);

    let num_bins = fft_size / 2;
    let mut magnitudes = Vec::with_capacity(num_bins);
    let norm = 2.0 / fft_size as f32;

    for i in 0..num_bins {
        magnitudes.push(complex_buf[i].magnitude() * norm);
    }

    magnitudes
}

/// Converts magnitude bins to decibels: 20 * log10(magnitude.max(1e-5)).
pub fn magnitudes_to_decibels(magnitudes: &[f32]) -> Vec<f32> {
    magnitudes
        .iter()
        .map(|&m| 20.0 * (m.max(1e-5)).log10())
        .collect()
}
