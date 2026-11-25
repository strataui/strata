//! Radix-2 Cooley-Tukey Fast Fourier Transform (FFT/IFFT), precomputed zero-alloc `FftPlan`,
//! and spectral magnitude analyzer.

use std::f32::consts::PI;

/// Complex number for FFT and frequency-domain DSP computations.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Complex {
    pub re: f32,
    pub im: f32,
}

impl Complex {
    #[inline]
    pub const fn new(re: f32, im: f32) -> Self {
        Self { re, im }
    }

    #[inline]
    pub const fn zero() -> Self {
        Self { re: 0.0, im: 0.0 }
    }

    #[inline]
    pub fn conj(self) -> Self {
        Self {
            re: self.re,
            im: -self.im,
        }
    }

    #[inline]
    pub fn norm_sq(self) -> f32 {
        self.re * self.re + self.im * self.im
    }

    #[inline]
    pub fn magnitude(self) -> f32 {
        self.norm_sq().sqrt()
    }

    #[inline]
    pub fn scale(self, s: f32) -> Self {
        Self {
            re: self.re * s,
            im: self.im * s,
        }
    }
}

impl std::ops::Add for Complex {
    type Output = Self;
    #[inline]
    fn add(self, other: Self) -> Self {
        Self::new(self.re + other.re, self.im + other.im)
    }
}

impl std::ops::AddAssign for Complex {
    #[inline]
    fn add_assign(&mut self, other: Self) {
        self.re += other.re;
        self.im += other.im;
    }
}

impl std::ops::Sub for Complex {
    type Output = Self;
    #[inline]
    fn sub(self, other: Self) -> Self {
        Self::new(self.re - other.re, self.im - other.im)
    }
}

impl std::ops::SubAssign for Complex {
    #[inline]
    fn sub_assign(&mut self, other: Self) {
        self.re -= other.re;
        self.im -= other.im;
    }
}

impl std::ops::Mul for Complex {
    type Output = Self;
    #[inline]
    fn mul(self, other: Self) -> Self {
        Self::new(
            self.re * other.re - self.im * other.im,
            self.re * other.im + self.im * other.re,
        )
    }
}

impl std::ops::Mul<f32> for Complex {
    type Output = Self;
    #[inline]
    fn mul(self, s: f32) -> Self {
        self.scale(s)
    }
}

/// In-place Radix-2 Decimation-In-Time (DIT) Fast Fourier Transform.
/// Length of `buffer` MUST be a power of two. Performs zero heap allocations.
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
                let v = buffer[i + k + half] * w;
                buffer[i + k] = u + v;
                buffer[i + k + half] = u - v;
                w = w * w_unit;
            }
            i += len;
        }
        len <<= 1;
    }
}

/// In-place Radix-2 Inverse Fast Fourier Transform (normalized by `1 / N`).
/// Length of `buffer` MUST be a power of two. Performs zero heap allocations.
pub fn ifft_radix2(buffer: &mut [Complex]) {
    let n = buffer.len();
    if n <= 1 {
        return;
    }
    assert!(n.is_power_of_two(), "IFFT size must be a power of two");

    for c in buffer.iter_mut() {
        c.im = -c.im;
    }
    fft_radix2(buffer);
    let inv_n = 1.0 / n as f32;
    for c in buffer.iter_mut() {
        c.re *= inv_n;
        c.im = -c.im * inv_n;
    }
}

/// Precomputed bit-reversal and twiddle-factor FFT/IFFT plan for zero-trig,
/// zero-allocation real-time audio callbacks.
#[derive(Debug, Clone)]
pub struct FftPlan {
    size: usize,
    bit_rev: Vec<usize>,
    /// Precomputed twiddles for each stage (stage half-size = 1, 2, 4, ..., size/2).
    fwd_twiddles: Vec<Complex>,
    inv_twiddles: Vec<Complex>,
}

impl FftPlan {
    /// Builds a reusable FFT/IFFT plan for a power-of-two `size`.
    pub fn new(size: usize) -> Self {
        assert!(
            size >= 2 && size.is_power_of_two(),
            "FftPlan size must be a power of two >= 2"
        );

        let bits = size.trailing_zeros();
        let mut bit_rev = Vec::with_capacity(size);
        for i in 0..size {
            let rev = i.reverse_bits() >> (usize::BITS - bits);
            bit_rev.push(rev);
        }

        // Total twiddles across all stages = 1 + 2 + 4 + ... + size/2 = size - 1.
        let mut fwd_twiddles = Vec::with_capacity(size);
        let mut inv_twiddles = Vec::with_capacity(size);
        let mut len = 2;
        while len <= size {
            let half = len / 2;
            let base_angle = 2.0 * PI / len as f32;
            for k in 0..half {
                let a = base_angle * k as f32;
                let (sin_a, cos_a) = a.sin_cos();
                fwd_twiddles.push(Complex::new(cos_a, -sin_a));
                inv_twiddles.push(Complex::new(cos_a, sin_a));
            }
            len <<= 1;
        }

        Self {
            size,
            bit_rev,
            fwd_twiddles,
            inv_twiddles,
        }
    }

    #[inline]
    pub const fn size(&self) -> usize {
        self.size
    }

    /// Executes an in-place forward FFT with zero allocations and zero trig calls.
    pub fn forward(&self, buffer: &mut [Complex]) {
        self.run_inner(buffer, &self.fwd_twiddles, false);
    }

    /// Executes an in-place normalized (`1 / N`) inverse FFT with zero allocations and zero trig calls.
    pub fn inverse(&self, buffer: &mut [Complex]) {
        self.run_inner(buffer, &self.inv_twiddles, true);
    }

    fn run_inner(&self, buffer: &mut [Complex], twiddles: &[Complex], normalize: bool) {
        let n = self.size;
        debug_assert_eq!(buffer.len(), n);

        for i in 0..n {
            let j = self.bit_rev[i];
            if i < j {
                buffer.swap(i, j);
            }
        }

        let mut tw_offset = 0usize;
        let mut len = 2usize;
        while len <= n {
            let half = len >> 1;
            let stage_tw = &twiddles[tw_offset..tw_offset + half];
            let mut i = 0;
            while i < n {
                for k in 0..half {
                    let u = buffer[i + k];
                    let v = buffer[i + k + half] * stage_tw[k];
                    buffer[i + k] = u + v;
                    buffer[i + k + half] = u - v;
                }
                i += len;
            }
            tw_offset += half;
            len <<= 1;
        }

        if normalize {
            let inv_n = 1.0 / n as f32;
            for c in buffer.iter_mut() {
                c.re *= inv_n;
                c.im *= inv_n;
            }
        }
    }
}

/// Computes the magnitude spectrum of an audio sample buffer (first N/2 bins).
pub fn compute_magnitude_spectrum(samples: &[f32], fft_size: usize) -> Vec<f32> {
    assert!(
        fft_size.is_power_of_two(),
        "FFT size must be a power of two"
    );
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
    let norm = 2.0 / fft_size as f32;

    complex_buf[..num_bins]
        .iter()
        .map(|c| c.magnitude() * norm)
        .collect()
}

/// Converts magnitude bins to decibels: `20 * log10(magnitude.max(1e-5))`.
pub fn magnitudes_to_decibels(magnitudes: &[f32]) -> Vec<f32> {
    magnitudes
        .iter()
        .map(|&m| 20.0 * (m.max(1e-5)).log10())
        .collect()
}
