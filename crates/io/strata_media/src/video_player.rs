//! Synchronized A/V master clock (`AvClock`), pluggable `VideoDecoder` trait, and `VideoPlayer`
//! exporting directly to `strata-render::FrameBuffer` texture slots.

use crate::pixel_format::VideoFrame;
use std::sync::Arc;
use std::time::Duration;
use strata_render::FrameBuffer;

/// Synchronization decision comparing a decoded video frame PTS against the master A/V clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameSyncDecision {
    /// Frame is within presentation tolerance (`±15 ms`) — display immediately.
    PresentNow,
    /// Frame is ahead of the master clock — wait `us` microseconds before presenting.
    WaitUs(u64),
    /// Frame is too far behind the master clock (`> 45 ms` late) — drop to preserve lip-sync.
    DropLateFrame,
}

/// Master audio/video presentation clock with drift tracking and playback rate scaling.
#[derive(Debug, Clone)]
pub struct AvClock {
    position_us: u64,
    playback_rate: f64,
    running: bool,
    present_window_us: i64,
    drop_threshold_us: i64,
}

impl Default for AvClock {
    fn default() -> Self {
        Self::new()
    }
}

impl AvClock {
    pub fn new() -> Self {
        Self {
            position_us: 0,
            playback_rate: 1.0,
            running: false,
            present_window_us: 15_000, // ±15 ms lip-sync tolerance
            drop_threshold_us: 45_000, // drop frame if >45 ms late
        }
    }

    pub fn play(&mut self) {
        self.running = true;
    }

    pub fn pause(&mut self) {
        self.running = false;
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    pub fn set_playback_rate(&mut self, rate: f64) {
        self.playback_rate = rate.clamp(0.25, 4.0);
    }

    pub fn playback_rate(&self) -> f64 {
        self.playback_rate
    }

    pub fn seek(&mut self, position: Duration) {
        self.position_us = position.as_micros() as u64;
    }

    /// Synchronizes the video clock directly to an external hardware audio clock timestamp.
    pub fn sync_to_audio_pts(&mut self, audio_pts_us: u64) {
        self.position_us = audio_pts_us;
    }

    /// Advances the clock by `wall_delta` scaled by `playback_rate` when running.
    pub fn advance(&mut self, wall_delta: Duration) {
        if self.running {
            let scaled = (wall_delta.as_micros() as f64 * self.playback_rate).round() as u64;
            self.position_us = self.position_us.saturating_add(scaled);
        }
    }

    pub fn position_us(&self) -> u64 {
        self.position_us
    }

    pub fn position(&self) -> Duration {
        Duration::from_micros(self.position_us)
    }

    /// Evaluates whether a frame with presentation timestamp `frame_pts_us` should be presented,
    /// held briefly, or dropped to keep A/V lip-sync tight.
    pub fn evaluate_frame(&self, frame_pts_us: u64) -> FrameSyncDecision {
        let delta = frame_pts_us as i64 - self.position_us as i64;
        if delta < -self.drop_threshold_us {
            FrameSyncDecision::DropLateFrame
        } else if delta > self.present_window_us {
            FrameSyncDecision::WaitUs(delta as u64)
        } else {
            FrameSyncDecision::PresentNow
        }
    }
}

/// Pluggable video decoder interface yielding timestamped RGBA [`VideoFrame`]s.
pub trait VideoDecoder: Send {
    fn duration(&self) -> Duration;
    fn next_frame(&mut self) -> Option<VideoFrame>;
    fn seek(&mut self, position: Duration);
}

/// Built-in zero-dependency frame sequence decoder for animations, recordings, and tests.
#[derive(Debug, Clone)]
pub struct FrameSequenceDecoder {
    frames: Vec<VideoFrame>,
    index: usize,
    duration: Duration,
}

impl FrameSequenceDecoder {
    pub fn new(frames: Vec<VideoFrame>) -> Self {
        let max_pts = frames.iter().map(|f| f.pts_us).max().unwrap_or(0);
        Self {
            frames,
            index: 0,
            duration: Duration::from_micros(max_pts),
        }
    }
}

impl VideoDecoder for FrameSequenceDecoder {
    fn duration(&self) -> Duration {
        self.duration
    }

    fn next_frame(&mut self) -> Option<VideoFrame> {
        let frame = self.frames.get(self.index)?.clone();
        self.index += 1;
        Some(frame)
    }

    fn seek(&mut self, position: Duration) {
        let target_us = position.as_micros() as u64;
        self.index = self
            .frames
            .iter()
            .position(|f| f.pts_us >= target_us)
            .unwrap_or(self.frames.len());
    }
}

/// Synchronized `<video>` player combining [`AvClock`], pluggable [`VideoDecoder`],
/// optional synchronized [`strata_audio::AudioPlayer`], and `strata-render` [`FrameBuffer`] output.
pub struct VideoPlayer {
    clock: AvClock,
    decoder: Option<Box<dyn VideoDecoder>>,
    queued_frame: Option<VideoFrame>,
    current_frame: Option<Arc<VideoFrame>>,
    looping: bool,
    dropped_frames: u64,
    #[cfg(feature = "audio-sync")]
    audio: Option<strata_audio::AudioPlayer>,
}

impl Default for VideoPlayer {
    fn default() -> Self {
        Self::new()
    }
}

impl VideoPlayer {
    pub fn new() -> Self {
        Self {
            clock: AvClock::new(),
            decoder: None,
            queued_frame: None,
            current_frame: None,
            looping: false,
            dropped_frames: 0,
            #[cfg(feature = "audio-sync")]
            audio: None,
        }
    }

    pub fn load_decoder(&mut self, decoder: Box<dyn VideoDecoder>) {
        self.decoder = Some(decoder);
        self.queued_frame = None;
        self.current_frame = None;
        self.dropped_frames = 0;
        self.clock.seek(Duration::ZERO);
    }

    #[cfg(feature = "audio-sync")]
    pub fn attach_audio_player(&mut self, player: strata_audio::AudioPlayer) {
        self.audio = Some(player);
    }

    pub fn play(&mut self) {
        self.clock.play();
        #[cfg(feature = "audio-sync")]
        if let Some(ref mut a) = self.audio {
            a.play();
        }
    }

    pub fn pause(&mut self) {
        self.clock.pause();
        #[cfg(feature = "audio-sync")]
        if let Some(ref mut a) = self.audio {
            a.pause();
        }
    }

    pub fn set_looping(&mut self, looping: bool) {
        self.looping = looping;
        #[cfg(feature = "audio-sync")]
        if let Some(ref mut a) = self.audio {
            a.set_looping(looping);
        }
    }

    pub fn seek(&mut self, position: Duration) {
        self.clock.seek(position);
        self.queued_frame = None;
        if let Some(ref mut dec) = self.decoder {
            dec.seek(position);
        }
        #[cfg(feature = "audio-sync")]
        if let Some(ref mut a) = self.audio {
            a.seek(position);
        }
    }

    pub fn clock(&self) -> &AvClock {
        &self.clock
    }

    pub fn dropped_frames(&self) -> u64 {
        self.dropped_frames
    }

    /// Advances playback by `dt`, evaluates A/V sync, drops overly late frames, and returns
    /// the newly presented frame if updated on this tick.
    pub fn tick(&mut self, dt: Duration) -> Option<Arc<VideoFrame>> {
        let synced_from_audio = {
            #[cfg(feature = "audio-sync")]
            {
                if let Some(ref a) = self.audio {
                    if a.state() == strata_audio::PlaybackState::Playing {
                        self.clock
                            .sync_to_audio_pts(a.current_time().as_micros() as u64);
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            #[cfg(not(feature = "audio-sync"))]
            {
                false
            }
        };
        if !synced_from_audio {
            self.clock.advance(dt);
        }

        if !self.clock.is_running() {
            return None;
        }

        let mut presented = None;
        loop {
            if self.queued_frame.is_none() {
                let next = self.decoder.as_mut().and_then(|d| d.next_frame());
                match next {
                    Some(f) => self.queued_frame = Some(f),
                    None => {
                        if self.looping {
                            self.seek(Duration::ZERO);
                            self.queued_frame = self.decoder.as_mut().and_then(|d| d.next_frame());
                            if self.queued_frame.is_none() {
                                break;
                            }
                        } else {
                            self.clock.pause();
                            break;
                        }
                    }
                }
            }

            let Some(ref candidate) = self.queued_frame else {
                break;
            };
            match self.clock.evaluate_frame(candidate.pts_us) {
                FrameSyncDecision::PresentNow => {
                    let frame = Arc::new(self.queued_frame.take().unwrap());
                    self.current_frame = Some(frame.clone());
                    presented = Some(frame);
                    break;
                }
                FrameSyncDecision::DropLateFrame => {
                    self.queued_frame = None;
                    self.dropped_frames += 1;
                }
                FrameSyncDecision::WaitUs(_) => {
                    break;
                }
            }
        }

        presented
    }

    /// Returns the most recently presented [`VideoFrame`].
    pub fn current_frame(&self) -> Option<Arc<VideoFrame>> {
        self.current_frame.clone()
    }

    /// Renders the active frame directly into a [`strata_render::FrameBuffer`].
    pub fn current_framebuffer(&self) -> Option<FrameBuffer> {
        self.current_frame.as_ref().map(|f| f.to_framebuffer())
    }
}
