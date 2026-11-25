//! Hard Real-Time Zero-Allocation Verification Gate.
//!
//! Uses a custom `#[global_allocator]` counting allocator to prove mathematically that
//! `AudioMixer::render_interleaved_f32` and `VoicePipeline::process_frame` perform
//! **strictly zero heap allocations** during steady-state real-time audio callbacks.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use strata_audio::device::AudioMixer;
use strata_audio::sample::AudioBuffer;
use strata_audio::voice_pipeline::{VoicePipeline, VoicePipelineConfig};

struct CountingAllocator;

static COUNTING_ACTIVE: AtomicBool = AtomicBool::new(false);
static ALLOC_COUNT: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if COUNTING_ACTIVE.load(Ordering::Relaxed) {
            ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

#[test]
fn test_realtime_mixer_and_voice_pipeline_perform_zero_heap_allocations() {
    // Pre-initialize all structures BEFORE enabling allocation counting.
    let mut mixer = AudioMixer::new(48_000, 2);
    let buf = AudioBuffer {
        sample_rate: 48_000,
        channels: vec![vec![0.25; 512], vec![-0.25; 512]],
    };
    mixer.enqueue(buf, 0.8, true);

    let mut pipe = VoicePipeline::new(VoicePipelineConfig::telephony(48_000));
    let mic = vec![0.15f32; 256];
    let far = vec![0.08f32; 256];
    let mut mix_out = vec![0.0f32; 512];
    let mut voice_out = vec![0.0f32; 256];

    // Warm up 1 block
    mixer.render_interleaved_f32(&mut mix_out);
    let _ = pipe.process_frame(&mic, Some(&far), &mut voice_out);

    // Enable strict zero-allocation gate
    ALLOC_COUNT.store(0, Ordering::SeqCst);
    COUNTING_ACTIVE.store(true, Ordering::SeqCst);

    for _ in 0..32 {
        mixer.render_interleaved_f32(&mut mix_out);
        let _ = pipe.process_frame(&mic, Some(&far), &mut voice_out);
    }

    COUNTING_ACTIVE.store(false, Ordering::SeqCst);
    let allocs = ALLOC_COUNT.load(Ordering::SeqCst);
    assert_eq!(
        allocs, 0,
        "Real-time audio path must perform 0 heap allocations, observed {allocs}"
    );
}
