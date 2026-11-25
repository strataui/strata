//! Showcase Example: Strata Voice Studio — Live Microphone & Real-Time Noise Cancellation DSP.
//!
//! Demonstrates:
//! - Full-duplex microphone enumeration (`strata::audio::list_input_devices`)
//! - Real-time zero-alloc `VoicePipeline` with Adaptive Noise Cancellation (`MCRA + MMSE-STSA`),
//!   Acoustic Echo Cancellation (`PBFDAF`), multi-feature VAD, and AGC
//! - Live reactive dBFS, SNR, and A-weighted SPL bridged into `strata::device::use_ambient_spl()`

use std::f32::consts::PI;
use strata::audio::{
    list_input_devices, list_output_devices, AudioInputConfig, MockAudioInput,
    NoiseSuppressionPreset, VoicePipeline, VoicePipelineConfig,
};
use strata::prelude::*;

fn main() {
    strata::register_universal_io_bridges();

    println!("=== Strata Voice Studio (Universal Core Audio + Real-Time DSP) ===");
    println!("Input devices discovered:  {}", list_input_devices().len());
    println!("Output devices discovered: {}", list_output_devices().len());

    let sample_rate = 48_000;
    let mut pipeline = VoicePipeline::new(VoicePipelineConfig::telephony(sample_rate));
    pipeline.set_noise_suppression(NoiseSuppressionPreset::High);
    println!(
        "VoicePipeline algorithmic latency: {:.2} ms",
        pipeline.algorithmic_latency_ms()
    );

    // Run a deterministic capture -> DSP -> meter -> SPL bridge pass
    let mic = MockAudioInput::new(AudioInputConfig {
        sample_rate,
        channels: 1,
        ..Default::default()
    });
    mic.set_processor(Some(pipeline.into_capture_processor()));

    // Feed 100 ms of synthetic voiced speech (440 Hz + harmonics)
    let mut speech = Vec::with_capacity(4800);
    for i in 0..4800 {
        let t = i as f32 / sample_rate as f32;
        let s = 0.25 * (2.0 * PI * 220.0 * t).sin() + 0.15 * (2.0 * PI * 660.0 * t).sin();
        speech.push(s);
    }
    mic.inject_samples(&speech);

    let snap = mic.meter().snapshot();
    let spl = use_ambient_spl().get();
    println!(
        "Live Meter -> RMS={:.4}, dBFS={:.1}, A-weighted SPL={:.1} dBA (device bridge={:.1} dBA)",
        snap.rms, snap.dbfs, snap.spl_dba, spl.spl_dba
    );

    // Render declarative Strata UI preview with `<audio>` tag and export Vector PDF
    let view = rsx! {
        <div class="w-[640px] h-[360px] bg-slate-950 p-6 flex flex-col gap-4">
            <h1 class="text-2xl font-bold text-slate-100">"Strata Voice Studio"</h1>
            <p class="text-sm text-sky-400">"MCRA + Ephraim-Malah Noise Cancellation & PBFDAF AEC Active"</p>
            <audio id="monitor" controls="true" volume="0.85" />
        </div>
    };
    let mut runtime = AppRuntime::new(view, 640.0, 360.0, 1.0);
    let pdf = runtime.export_pdf_bytes(&strata::render::PdfExportOptions::default());
    println!("Vector PDF 1.7 snapshot compiled: {} bytes", pdf.len());
}
