//! Zero-allocation real-time audio dynamics & tonal shaping:
//! Automatic Gain Control (`AgcProcessor`), Soft-Knee `CompressorLimiter`,
//! Hysteresis `NoiseGate`, Split-Band `DeEsser`, and Multi-Band `ParametricEq`.

use crate::filter::{BiquadFilter, BiquadFilterType};

#[inline]
fn db_to_linear(db: f32) -> f32 {
    10.0f32.powf(db / 20.0)
}

#[inline]
fn linear_to_db(lin: f32) -> f32 {
    20.0 * lin.max(1e-6).log10()
}

#[inline]
fn time_coeff(ms: f32, sample_rate: u32) -> f32 {
    let samples = (ms.max(0.1) * 0.001) * sample_rate.max(8_000) as f32;
    (-1.0 / samples).exp()
}

/// Smooth RMS Automatic Gain Control (AGC) with VAD gating so background silence is never boosted.
#[derive(Debug, Clone)]
pub struct AgcProcessor {
    pub target_dbfs: f32,
    pub max_gain_db: f32,
    pub min_gain_db: f32,
    pub speech_gate_dbfs: f32,
    attack_coeff: f32,
    release_coeff: f32,
    env_rms_sq: f32,
    current_gain_db: f32,
}

impl AgcProcessor {
    pub fn new(sample_rate: u32) -> Self {
        Self::with_params(sample_rate, -18.0, 18.0, -12.0, -45.0, 25.0, 350.0)
    }

    pub fn with_params(
        sample_rate: u32,
        target_dbfs: f32,
        max_gain_db: f32,
        min_gain_db: f32,
        speech_gate_dbfs: f32,
        attack_ms: f32,
        release_ms: f32,
    ) -> Self {
        Self {
            target_dbfs,
            max_gain_db,
            min_gain_db,
            speech_gate_dbfs,
            attack_coeff: time_coeff(attack_ms, sample_rate),
            release_coeff: time_coeff(release_ms, sample_rate),
            env_rms_sq: 1e-6,
            current_gain_db: 0.0,
        }
    }

    /// Returns the active applied AGC gain in decibels.
    #[inline]
    pub const fn current_gain_db(&self) -> f32 {
        self.current_gain_db
    }

    /// Processes a mono slice in-place. When `is_speech == Some(false)`, gain freezes smoothly.
    pub fn process_in_place(&mut self, samples: &mut [f32], is_speech: Option<bool>) {
        let allow_adapt = is_speech.unwrap_or(true);
        for s in samples.iter_mut() {
            let x = *s;
            let sq = x * x;
            let env_c = if sq > self.env_rms_sq {
                self.attack_coeff
            } else {
                self.release_coeff
            };
            self.env_rms_sq = env_c * self.env_rms_sq + (1.0 - env_c) * sq;

            let rms = self.env_rms_sq.sqrt();
            let level_db = linear_to_db(rms);

            if allow_adapt && level_db > self.speech_gate_dbfs {
                let desired_gain = (self.target_dbfs - level_db)
                    .clamp(self.min_gain_db, self.max_gain_db);
                let g_coeff = if desired_gain < self.current_gain_db {
                    self.attack_coeff
                } else {
                    self.release_coeff
                };
                self.current_gain_db =
                    g_coeff * self.current_gain_db + (1.0 - g_coeff) * desired_gain;
            }

            let gain_lin = db_to_linear(self.current_gain_db);
            *s = (x * gain_lin).clamp(-1.0, 1.0);
        }
    }

    pub fn reset(&mut self) {
        self.env_rms_sq = 1e-6;
        self.current_gain_db = 0.0;
    }
}

/// Feed-forward soft-knee compressor paired with a brickwall true-peak safety limiter.
#[derive(Debug, Clone)]
pub struct CompressorLimiter {
    pub threshold_db: f32,
    pub ratio: f32,
    pub knee_db: f32,
    pub makeup_db: f32,
    pub ceiling_db: f32,
    attack_coeff: f32,
    release_coeff: f32,
    env_db: f32,
    gain_reduction_db: f32,
}

impl CompressorLimiter {
    pub fn voice_default(sample_rate: u32) -> Self {
        Self::new(sample_rate, -18.0, 3.0, 6.0, 8.0, 120.0, 2.0, -1.0)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        sample_rate: u32,
        threshold_db: f32,
        ratio: f32,
        knee_db: f32,
        attack_ms: f32,
        release_ms: f32,
        makeup_db: f32,
        ceiling_db: f32,
    ) -> Self {
        Self {
            threshold_db,
            ratio: ratio.max(1.0),
            knee_db: knee_db.max(0.0),
            makeup_db,
            ceiling_db: ceiling_db.min(0.0),
            attack_coeff: time_coeff(attack_ms, sample_rate),
            release_coeff: time_coeff(release_ms, sample_rate),
            env_db: -120.0,
            gain_reduction_db: 0.0,
        }
    }

    /// Returns the most recent gain reduction in dB (`<= 0.0`).
    #[inline]
    pub const fn gain_reduction_db(&self) -> f32 {
        self.gain_reduction_db
    }

    #[inline]
    pub fn process_sample(&mut self, input: f32) -> f32 {
        let in_db = linear_to_db(input.abs());
        let coeff = if in_db > self.env_db {
            self.attack_coeff
        } else {
            self.release_coeff
        };
        self.env_db = coeff * self.env_db + (1.0 - coeff) * in_db;

        let half_knee = self.knee_db * 0.5;
        let over = self.env_db - self.threshold_db;
        let compressed_over = if self.knee_db <= 0.001 {
            if over > 0.0 {
                over / self.ratio
            } else {
                over
            }
        } else if over <= -half_knee {
            over
        } else if over >= half_knee {
            over / self.ratio
        } else {
            let x = over + half_knee;
            over + ((1.0 / self.ratio) - 1.0) * (x * x) / (2.0 * self.knee_db)
        };

        let gr_db = compressed_over - over; // <= 0.0
        self.gain_reduction_db = gr_db;
        let total_gain = db_to_linear(gr_db + self.makeup_db);
        let ceiling = db_to_linear(self.ceiling_db);
        let y = input * total_gain;
        // Soft hyperbolic-tangent brickwall safety limiter near ceiling.
        if y.abs() > ceiling {
            y.signum() * ceiling * (y.abs() / ceiling).tanh().min(1.0)
        } else {
            y
        }
    }

    pub fn process_in_place(&mut self, samples: &mut [f32]) {
        for s in samples.iter_mut() {
            *s = self.process_sample(*s);
        }
    }

    pub fn reset(&mut self) {
        self.env_db = -120.0;
        self.gain_reduction_db = 0.0;
    }
}

/// Smooth hysteresis noise gate with hold timer and configurable attenuation floor.
#[derive(Debug, Clone)]
pub struct NoiseGate {
    pub open_threshold_db: f32,
    pub close_threshold_db: f32,
    pub range_db: f32,
    hold_samples: usize,
    hold_remaining: usize,
    attack_coeff: f32,
    release_coeff: f32,
    env_db: f32,
    gate_gain: f32,
    is_open: bool,
}

impl NoiseGate {
    pub fn new(
        sample_rate: u32,
        open_threshold_db: f32,
        close_threshold_db: f32,
        hold_ms: f32,
        attack_ms: f32,
        release_ms: f32,
        range_db: f32,
    ) -> Self {
        let hold_samples = ((hold_ms.max(0.0) * 0.001) * sample_rate.max(8_000) as f32) as usize;
        Self {
            open_threshold_db,
            close_threshold_db: close_threshold_db.min(open_threshold_db),
            range_db: range_db.min(0.0),
            hold_samples,
            hold_remaining: 0,
            attack_coeff: time_coeff(attack_ms, sample_rate),
            release_coeff: time_coeff(release_ms, sample_rate),
            env_db: -120.0,
            gate_gain: db_to_linear(range_db.min(0.0)),
            is_open: false,
        }
    }

    #[inline]
    pub const fn is_open(&self) -> bool {
        self.is_open
    }

    #[inline]
    pub fn process_sample(&mut self, input: f32) -> f32 {
        let level_db = linear_to_db(input.abs());
        let env_c = if level_db > self.env_db { 0.85 } else { 0.995 };
        self.env_db = env_c * self.env_db + (1.0 - env_c) * level_db;

        if self.env_db >= self.open_threshold_db {
            self.is_open = true;
            self.hold_remaining = self.hold_samples;
        } else if self.env_db < self.close_threshold_db {
            if self.hold_remaining > 0 {
                self.hold_remaining -= 1;
            } else {
                self.is_open = false;
            }
        }

        let target = if self.is_open {
            1.0
        } else {
            db_to_linear(self.range_db)
        };
        let c = if target > self.gate_gain {
            self.attack_coeff
        } else {
            self.release_coeff
        };
        self.gate_gain = c * self.gate_gain + (1.0 - c) * target;
        input * self.gate_gain
    }

    pub fn process_in_place(&mut self, samples: &mut [f32]) {
        for s in samples.iter_mut() {
            *s = self.process_sample(*s);
        }
    }
}

/// Split-band sibilance De-Esser targeting harsh 5.5–9 kHz vocal consonants ("s", "sh", "t").
#[derive(Debug, Clone)]
pub struct DeEsser {
    detector_bp: BiquadFilter,
    pub threshold_db: f32,
    pub max_reduction_db: f32,
    attack_coeff: f32,
    release_coeff: f32,
    env_db: f32,
}

impl DeEsser {
    pub fn new(sample_rate: u32, center_hz: f32, threshold_db: f32, max_reduction_db: f32) -> Self {
        Self {
            detector_bp: BiquadFilter::new(
                BiquadFilterType::Bandpass,
                center_hz.clamp(3_500.0, (sample_rate as f32 * 0.42).max(4_000.0)),
                2.2,
                sample_rate,
            ),
            threshold_db,
            max_reduction_db: max_reduction_db.abs(),
            attack_coeff: time_coeff(2.0, sample_rate),
            release_coeff: time_coeff(40.0, sample_rate),
            env_db: -120.0,
        }
    }

    #[inline]
    pub fn process_sample(&mut self, input: f32) -> f32 {
        let sib = self.detector_bp.process_sample(input);
        let sib_db = linear_to_db(sib.abs());
        let c = if sib_db > self.env_db {
            self.attack_coeff
        } else {
            self.release_coeff
        };
        self.env_db = c * self.env_db + (1.0 - c) * sib_db;

        let excess = (self.env_db - self.threshold_db).max(0.0);
        let red_db = (excess * 0.75).min(self.max_reduction_db);
        if red_db <= 0.05 {
            return input;
        }
        let atten = 1.0 - db_to_linear(-red_db);
        input - sib * atten
    }

    pub fn process_in_place(&mut self, samples: &mut [f32]) {
        for s in samples.iter_mut() {
            *s = self.process_sample(*s);
        }
    }
}

/// Multi-band zero-allocation parametric equalizer built from cascaded RBJ biquad sections.
#[derive(Debug, Clone)]
pub struct ParametricEq {
    sample_rate: u32,
    bands: Vec<BiquadFilter>,
}

impl ParametricEq {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate: sample_rate.max(8_000),
            bands: Vec::new(),
        }
    }

    /// Creates a studio voice presence EQ preset:
    /// 85 Hz HPF (rumble cut), 250 Hz `-2.0 dB` mud trim, 3.2 kHz `+2.5 dB` vocal clarity,
    /// 10 kHz `+1.5 dB` air shelf.
    pub fn voice_presence_preset(sample_rate: u32) -> Self {
        let mut eq = Self::new(sample_rate);
        eq.add_band(BiquadFilterType::Highpass, 85.0, 0.707);
        eq.add_band(BiquadFilterType::Peaking { gain_db: -2.0 }, 250.0, 1.2);
        eq.add_band(BiquadFilterType::Peaking { gain_db: 2.5 }, 3200.0, 1.1);
        if sample_rate >= 24_000 {
            eq.add_band(BiquadFilterType::HighShelf { gain_db: 1.5 }, 10_000.0, 0.707);
        }
        eq
    }

    pub fn add_band(&mut self, filter_type: BiquadFilterType, freq_hz: f32, q: f32) {
        self.bands
            .push(BiquadFilter::new(filter_type, freq_hz, q, self.sample_rate));
    }

    pub fn band_count(&self) -> usize {
        self.bands.len()
    }

    #[inline]
    pub fn process_sample(&mut self, mut sample: f32) -> f32 {
        for band in &mut self.bands {
            sample = band.process_sample(sample);
        }
        sample
    }

    pub fn process_in_place(&mut self, samples: &mut [f32]) {
        for band in &mut self.bands {
            band.process_slice_in_place(samples);
        }
    }

    pub fn reset(&mut self) {
        for band in &mut self.bands {
            band.reset_state();
        }
    }
}
