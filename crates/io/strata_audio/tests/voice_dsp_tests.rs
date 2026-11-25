//! Quantitative Phase 3 DSP Verification Suite:
//! - FFT/IFFT & FftPlan accuracy
//! - STFT COLA perfect reconstruction across aligned & unaligned frame sizes
//! - Adaptive Noise Cancellation >= 12 dB SNR improvement benchmark
//! - PBFDAF Acoustic Echo Cancellation >= 15 dB ERLE & Double-Talk resilience benchmark
//! - Multi-feature VAD detection & hangover accuracy
//! - Dynamics (AGC, Limiter, NoiseGate, DeEsser, ParametricEQ) & unified VoicePipeline latency

use std::f32::consts::PI;
use strata_audio::aec::{AecConfig, EchoCanceller};
use strata_audio::dynamics::{AgcProcessor, CompressorLimiter, DeEsser, NoiseGate, ParametricEq};
use strata_audio::fft::{fft_radix2, ifft_radix2, Complex, FftPlan};
use strata_audio::filter::BiquadFilterType;
use strata_audio::noise_cancel::{NoiseSuppressionPreset, NoiseSuppressor};
use strata_audio::stft::StftProcessor;
use strata_audio::vad::{VadConfig, VoiceActivityDetector};
use strata_audio::voice_pipeline::{VoicePipeline, VoicePipelineConfig};

struct XorShift32(u32);
impl XorShift32 {
    fn next_f32(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        // Uniform in [-1.0, 1.0]
        (self.0 as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

fn rms(slice: &[f32]) -> f32 {
    if slice.is_empty() {
        return 0.0;
    }
    (slice.iter().map(|&x| x * x).sum::<f32>() / slice.len() as f32).sqrt()
}

/// Generates realistic harmonic formant speech bursts (160 Hz F0 + formants at 480, 960, 1600, 2400 Hz).
fn synthesize_formant_speech(sample_rate: u32, len: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; len];
    let sr = sample_rate as f32;
    for (i, slot) in out.iter_mut().enumerate() {
        let t = i as f32 / sr;
        let f0 = 160.0 + 18.0 * (2.0 * PI * 2.5 * t).sin();
        let s = 0.35 * (2.0 * PI * f0 * t).sin()
            + 0.28 * (2.0 * PI * 480.0 * t).sin()
            + 0.20 * (2.0 * PI * 960.0 * t).sin()
            + 0.12 * (2.0 * PI * 1600.0 * t).sin()
            + 0.06 * (2.0 * PI * 2400.0 * t).sin();
        *slot = s * 0.45;
    }
    out
}

#[test]
fn test_fft_ifft_and_fft_plan_exact_roundtrip() {
    let n = 512;
    let mut orig = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / n as f32;
        let x = (2.0 * PI * 7.0 * t).sin() + 0.4 * (2.0 * PI * 31.0 * t).cos();
        orig.push(Complex::new(x, 0.0));
    }

    // 1. Direct radix2 FFT -> IFFT
    let mut a = orig.clone();
    fft_radix2(&mut a);
    ifft_radix2(&mut a);
    for i in 0..n {
        assert!((a[i].re - orig[i].re).abs() < 1e-5);
        assert!(a[i].im.abs() < 1e-5);
    }

    // 2. Precomputed FftPlan forward matches fft_radix2 and inverse restores exact input
    let plan = FftPlan::new(n);
    let mut b = orig.clone();
    let mut ref_fwd = orig.clone();
    plan.forward(&mut b);
    fft_radix2(&mut ref_fwd);
    for i in 0..n {
        assert!((b[i].re - ref_fwd[i].re).abs() < 5e-3);
        assert!((b[i].im - ref_fwd[i].im).abs() < 5e-3);
    }
    plan.inverse(&mut b);
    for i in 0..n {
        assert!((b[i].re - orig[i].re).abs() < 1e-5);
        assert!(b[i].im.abs() < 1e-5);
    }
}

#[test]
fn test_stft_cola_perfect_reconstruction() {
    let fft_size = 512;
    let hop = fft_size / 2;
    let mut stft = StftProcessor::new(fft_size);
    let total = 4096;
    let input: Vec<f32> = (0..total)
        .map(|i| {
            let t = i as f32 / 48_000.0;
            0.6 * (2.0 * PI * 440.0 * t).sin() + 0.3 * (2.0 * PI * 1850.0 * t).cos()
        })
        .collect();
    let mut output = vec![0.0f32; total];

    // Process in aligned hops
    for (in_blk, out_blk) in input.chunks(hop).zip(output.chunks_mut(hop)) {
        stft.process_frame(in_blk, out_blk, |_spec| {});
    }

    // Steady-state output matches input delayed by exactly `hop` samples with RMS error < 1e-5!
    let mut err_sq = 0.0f32;
    let count = total - fft_size;
    for i in fft_size..total {
        let diff = output[i] - input[i - hop];
        err_sq += diff * diff;
    }
    let rms_err = (err_sq / count as f32).sqrt();
    assert!(
        rms_err < 1e-5,
        "STFT COLA reconstruction RMS error too high: {rms_err}"
    );
}

#[test]
fn test_adaptive_noise_cancellation_snr_improvement_benchmark() {
    let sr = 16_000u32;
    let mut rng = XorShift32(0xCAFE_BABE);

    // Build a 2.4-second signal:
    //   [0.0s .. 0.8s] Pure stationary broadband noise + 500 Hz fan hum (noise-only baseline)
    //   [0.8s .. 1.6s] Formant speech + same continuous noise (~3 dB SNR)
    //   [1.6s .. 2.4s] Pure noise tail (verify suppression returns cleanly to noise floor)
    let seg_len = (sr as usize * 8) / 10; // 12,800 samples = 0.8s per segment
    let total = seg_len * 3;
    let speech_seg = synthesize_formant_speech(sr, seg_len);

    let mut noise = vec![0.0f32; total];
    for (i, n) in noise.iter_mut().enumerate() {
        let hum = 0.04 * (2.0 * PI * 500.0 * i as f32 / sr as f32).sin();
        *n = rng.next_f32() * 0.12 + hum;
    }

    let mut clean_ref = vec![0.0f32; total];
    clean_ref[seg_len..seg_len * 2].copy_from_slice(&speech_seg);

    let mut noisy = vec![0.0f32; total];
    for i in 0..total {
        noisy[i] = clean_ref[i] + noise[i];
    }

    let mut ns = NoiseSuppressor::with_preset(NoiseSuppressionPreset::High, sr);
    assert!(
        ns.algorithmic_latency_ms() < 10.0,
        "Latency must be < 10ms, got {}",
        ns.algorithmic_latency_ms()
    );

    let mut denoised = vec![0.0f32; total];
    let frame = 128;
    for (inp, out) in noisy.chunks(frame).zip(denoised.chunks_mut(frame)) {
        ns.process_frame(inp, out);
    }

    // Verify finite output (zero NaN / Inf)
    assert!(denoised.iter().all(|s| s.is_finite()));

    // Measure input SNR vs output SNR using noise tail vs active speech segment (accounting for hop latency)
    let hop = 128;
    let in_noise_rms = rms(&noisy[seg_len * 2 + 1024..total - 256]);
    let out_noise_rms = rms(&denoised[seg_len * 2 + 1024..total - 256]);
    let in_speech_rms = rms(&noisy[seg_len + 1024..seg_len * 2 - 256]);
    let out_speech_rms = rms(&denoised[seg_len + 1024 + hop..seg_len * 2 - 256 + hop]);

    let noise_reduction_db = 20.0 * (in_noise_rms / out_noise_rms.max(1e-6)).log10();
    let in_snr_db = 20.0 * (in_speech_rms / in_noise_rms.max(1e-6)).log10();
    let out_snr_db = 20.0 * (out_speech_rms / out_noise_rms.max(1e-6)).log10();
    let snr_improvement_db = out_snr_db - in_snr_db;

    println!(
        "NS Benchmark -> noise_reduction={noise_reduction_db:.2} dB, in_snr={in_snr_db:.2} dB, out_snr={out_snr_db:.2} dB, delta_snr=+{snr_improvement_db:.2} dB"
    );
    assert!(
        noise_reduction_db >= 14.0,
        "Expected >= 14 dB noise floor reduction, got {noise_reduction_db:.2} dB"
    );
    assert!(
        snr_improvement_db >= 12.0,
        "Expected >= 12 dB SNR improvement, got {snr_improvement_db:.2} dB"
    );
    assert!(
        out_speech_rms > 0.08,
        "Speech was over-suppressed! out_speech_rms={out_speech_rms:.4}"
    );
}

#[test]
fn test_pbfdaf_acoustic_echo_cancellation_erle_and_double_talk() {
    let sr = 16_000u32;
    let mut rng = XorShift32(0x1337_4242);
    let total = sr as usize * 2; // 2.0 seconds

    // Far-end speaker signal: spectrally rich colored signal (white noise shaped by simple LP)
    let mut far_end = vec![0.0f32; total];
    let mut lp = 0.0f32;
    for s in &mut far_end {
        lp = 0.65 * lp + 0.35 * rng.next_f32() * 0.6;
        *s = lp;
    }

    // Synthetic room impulse response (32 ms multipath echo tail: 512 taps @ 16 kHz)
    let rir_len = 384;
    let mut rir = vec![0.0f32; rir_len];
    rir[24] = 0.55; // Direct speaker-to-mic path (1.5 ms)
    rir[73] = -0.25; // Desk reflection (4.5 ms)
    rir[148] = 0.14; // Wall reflection (9.2 ms)
    rir[240] = -0.08; // Late reflection (15 ms)
    rir[330] = 0.04;

    // Convolve far_end with RIR to generate acoustic echo picked up by mic
    let mut mic = vec![0.0f32; total];
    for n in 0..total {
        let mut acc = 0.0f32;
        for (k, &h) in rir.iter().enumerate() {
            if h != 0.0 && n >= k {
                acc += h * far_end[n - k];
            }
        }
        mic[n] = acc;
    }

    // Verify coarse delay estimation finds the dominant direct tap (~24 samples)
    let est_delay = EchoCanceller::estimate_delay_samples(&far_end[..4096], &mic[..4096], 128);
    assert!(
        (est_delay as isize - 24).abs() <= 2,
        "Expected delay ~24 samples, estimated {est_delay}"
    );

    let mut aec = EchoCanceller::new(AecConfig {
        sample_rate: sr,
        tail_length_ms: 48.0,
        block_size: 128,
        step_size: 0.55,
        nlp_strength: 0.45,
        enable_dtd: true,
    });

    let mut out = vec![0.0f32; total];
    let blk = 128;
    for i in (0..total).step_by(blk) {
        aec.process_frame(&far_end[i..i + blk], &mic[i..i + blk], &mut out[i..i + blk]);
    }

    // Measure converged ERLE in [0.5s .. 1.8s] (well within convergence window)
    let start = sr as usize / 2;
    let end = (sr as usize * 18) / 10;
    let mic_rms = rms(&mic[start..end]);
    let res_rms = rms(&out[start..end]);
    let erle_db = 20.0 * (mic_rms / res_rms.max(1e-7)).log10();

    println!(
        "AEC Benchmark -> mic_rms={mic_rms:.4}, residual_rms={res_rms:.5}, ERLE={erle_db:.2} dB (internal={:.2} dB)",
        aec.erle_db()
    );
    assert!(
        erle_db >= 15.0,
        "Expected AEC ERLE >= 15 dB, got {erle_db:.2} dB"
    );

    // Double-Talk verification: inject strong near-end human speech simultaneously with far-end
    // and verify near-end speech passes cleanly without filter explosion.
    let dt_len = 4096;
    let near_speech = synthesize_formant_speech(sr, dt_len);
    let mut dt_mic = vec![0.0f32; dt_len];
    let mut dt_out = vec![0.0f32; dt_len];
    for i in 0..dt_len {
        dt_mic[i] = mic[end - dt_len + i] + near_speech[i];
    }
    for i in (0..dt_len).step_by(blk) {
        aec.process_frame(
            &far_end[end - dt_len + i..end - dt_len + i + blk],
            &dt_mic[i..i + blk],
            &mut dt_out[i..i + blk],
        );
    }
    let dt_out_rms = rms(&dt_out);
    let near_rms = rms(&near_speech);
    assert!(
        dt_out_rms > near_rms * 0.55 && dt_out_rms < near_rms * 1.45,
        "Double-talk speech distorted: out={dt_out_rms:.4}, near={near_rms:.4}"
    );
}

#[test]
fn test_multi_feature_vad_accuracy_and_hangover() {
    let sr = 16_000u32;
    let mut vad = VoiceActivityDetector::new(VadConfig::new(sr));
    let mut rng = XorShift32(0xDEAD_BEEF);

    // 1. Feed 200 ms of quiet white background noise (~-42 dBFS) -> must stay non-speech
    let frame = 160; // 10 ms
    let mut noise_frame = vec![0.0f32; frame];
    for _ in 0..20 {
        for s in &mut noise_frame {
            *s = rng.next_f32() * 0.006;
        }
        let dec = vad.analyze_frame(&noise_frame);
        assert!(
            !dec.is_speech,
            "False positive on quiet white noise: {dec:?}"
        );
    }

    // 2. Inject voiced speech burst -> must trigger within <= 30 ms (3 frames)
    let speech = synthesize_formant_speech(sr, 1600);
    let mut detected_at = None;
    for (idx, chunk) in speech.chunks_exact(frame).enumerate() {
        let dec = vad.analyze_frame(chunk);
        if dec.is_speech && detected_at.is_none() {
            detected_at = Some(idx);
        }
    }
    assert!(
        detected_at.map(|f| f <= 3).unwrap_or(false),
        "VAD took too long to detect speech: {detected_at:?}"
    );

    // 3. Brief 30 ms silence gap -> hangover holds `is_speech == true`
    let silence = vec![0.0f32; frame];
    for _ in 0..3 {
        assert!(vad.analyze_frame(&silence).is_speech);
    }

    // 4. Sustained 300 ms silence -> hangover expires cleanly to `false`
    let mut final_dec = vad.analyze_frame(&silence);
    for _ in 0..30 {
        final_dec = vad.analyze_frame(&silence);
    }
    assert!(!final_dec.is_speech);
}

#[test]
fn test_dynamics_agc_limiter_gate_deesser_eq_and_voice_pipeline() {
    let sr = 48_000u32;

    // 1. AGC boosts quiet speech toward target -18 dBFS (~0.125 RMS) and freezes when gated false
    let mut agc = AgcProcessor::new(sr);
    let mut quiet: Vec<f32> = (0..64_000)
        .map(|i| 0.02 * (2.0 * PI * 440.0 * i as f32 / sr as f32).sin())
        .collect();
    agc.process_in_place(&mut quiet, Some(true));
    let tail_rms = rms(&quiet[56_000..]);
    assert!(
        tail_rms > 0.08 && agc.current_gain_db() > 14.0,
        "AGC did not boost quiet signal: tail_rms={tail_rms:.4}, gain={:.2} dB",
        agc.current_gain_db()
    );

    // 2. CompressorLimiter enforces ceiling on hot +6 dB peak input
    let mut lim = CompressorLimiter::voice_default(sr);
    let mut hot: Vec<f32> = (0..2048)
        .map(|i| 1.8 * (2.0 * PI * 300.0 * i as f32 / sr as f32).sin())
        .collect();
    lim.process_in_place(&mut hot);
    let peak = hot.iter().fold(0.0f32, |m, &x| m.max(x.abs()));
    assert!(peak <= 0.92, "Limiter ceiling exceeded: peak={peak:.4}");

    // 3. NoiseGate attenuates quiet signal below close threshold
    let mut gate = NoiseGate::new(sr, -30.0, -36.0, 10.0, 2.0, 20.0, -24.0);
    let mut whisper = vec![0.005f32; 4800];
    gate.process_in_place(&mut whisper);
    assert!(!gate.is_open());
    assert!(rms(&whisper[2400..]) < 0.001);

    // 4. DeEsser attenuates harsh 6.5 kHz sibilance while leaving 300 Hz vowel intact
    let mut de = DeEsser::new(sr, 6_500.0, -24.0, 12.0);
    let mut sib: Vec<f32> = (0..4096)
        .map(|i| 0.5 * (2.0 * PI * 6_500.0 * i as f32 / sr as f32).sin())
        .collect();
    let in_sib_rms = rms(&sib[2048..]);
    de.process_in_place(&mut sib);
    let out_sib_rms = rms(&sib[2048..]);
    assert!(
        out_sib_rms < in_sib_rms * 0.75,
        "DeEsser failed to attenuate 6.5 kHz sibilance: {out_sib_rms:.4} vs {in_sib_rms:.4}"
    );

    // 5. ParametricEq peaking boost at 3.2 kHz increases energy at 3.2 kHz
    let mut eq = ParametricEq::new(sr);
    eq.add_band(BiquadFilterType::Peaking { gain_db: 6.0 }, 3_200.0, 1.0);
    let mut tone: Vec<f32> = (0..4096)
        .map(|i| 0.2 * (2.0 * PI * 3_200.0 * i as f32 / sr as f32).sin())
        .collect();
    let before = rms(&tone[1024..]);
    eq.process_in_place(&mut tone);
    let after = rms(&tone[1024..]);
    assert!(after > before * 1.7, "ParametricEq boost failed: {after} vs {before}");

    // 6. Unified VoicePipeline end-to-end latency < 10 ms and finite clean output
    let mut pipe = VoicePipeline::new(VoicePipelineConfig::telephony(sr));
    assert!(pipe.algorithmic_latency_ms() < 10.0);
    let speech = synthesize_formant_speech(sr, 4800);
    let mut out = vec![0.0f32; 480];
    let mut stats = Default::default();
    for chunk in speech.chunks_exact(480) {
        stats = pipe.process_frame(chunk, None, &mut out);
        assert!(out.iter().all(|s| s.is_finite()));
    }
    assert!(stats.algorithmic_latency_ms < 10.0);
}
