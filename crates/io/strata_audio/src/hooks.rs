//! Reactive Strata DOM hooks for live audio devices, microphone capture, and level metering.

use crate::capture::{AudioInputConfig, AudioInputStream, AudioMeterSnapshot, MockAudioInput};
use crate::device::{list_audio_devices, AudioDeviceInfo, AudioError};
use std::cell::RefCell;
use std::sync::Arc;
use strata_dom::Signal;

thread_local! {
    static DEVICES_SIGNAL: RefCell<Option<Signal<Vec<AudioDeviceInfo>>>> = const { RefCell::new(None) };
    static LEVEL_SIGNAL: RefCell<Option<Signal<AudioMeterSnapshot>>> = const { RefCell::new(None) };
}

fn devices_signal() -> Signal<Vec<AudioDeviceInfo>> {
    DEVICES_SIGNAL.with(|cell| {
        let mut borrow = cell.borrow_mut();
        if let Some(ref sig) = *borrow {
            sig.clone()
        } else {
            let sig = Signal::new(list_audio_devices());
            *borrow = Some(sig.clone());
            sig
        }
    })
}

fn level_signal() -> Signal<AudioMeterSnapshot> {
    LEVEL_SIGNAL.with(|cell| {
        let mut borrow = cell.borrow_mut();
        if let Some(ref sig) = *borrow {
            sig.clone()
        } else {
            let sig = Signal::new(AudioMeterSnapshot::default());
            *borrow = Some(sig.clone());
            sig
        }
    })
}

/// Refreshes the thread-local reactive audio device list from the host OS.
pub fn refresh_audio_devices() -> Vec<AudioDeviceInfo> {
    let devices = list_audio_devices();
    devices_signal().set(devices.clone());
    devices
}

/// Publishes an updated [`AudioMeterSnapshot`] into the reactive [`use_audio_level`] signal.
pub fn publish_audio_level_snapshot(snapshot: AudioMeterSnapshot) {
    level_signal().set(snapshot);
}

/// Reactive hook returning all discovered host audio input/output devices.
pub fn use_audio_devices() -> Signal<Vec<AudioDeviceInfo>> {
    devices_signal()
}

/// Reactive hook returning the latest microphone/input audio meter snapshot (RMS, peak, dBFS, SPL, FFT).
pub fn use_audio_level() -> Signal<AudioMeterSnapshot> {
    level_signal()
}

/// Underlying backend backing a [`MicrophoneHandle`].
enum MicBackend {
    Hardware(AudioInputStream),
    Mock(Arc<MockAudioInput>),
    Stopped,
}

/// Reactive controller handle returned by [`use_microphone`].
pub struct MicrophoneHandle {
    config: AudioInputConfig,
    backend: RefCell<MicBackend>,
    is_recording: Signal<bool>,
    is_muted: Signal<bool>,
    is_speaking: Signal<bool>,
    level: Signal<AudioMeterSnapshot>,
}

impl MicrophoneHandle {
    /// Creates a new microphone handle without immediately opening hardware until [`Self::start`] is called.
    pub fn new(config: AudioInputConfig) -> Self {
        Self {
            config,
            backend: RefCell::new(MicBackend::Stopped),
            is_recording: Signal::new(false),
            is_muted: Signal::new(false),
            is_speaking: Signal::new(false),
            level: Signal::new(AudioMeterSnapshot::default()),
        }
    }

    /// Starts capturing from the configured hardware microphone (falling back to mock if requested).
    pub fn start(&self) -> Result<(), AudioError> {
        let stream = AudioInputStream::open(self.config.clone())?;
        stream.set_muted(self.is_muted.get());
        *self.backend.borrow_mut() = MicBackend::Hardware(stream);
        self.is_recording.set(true);
        Ok(())
    }

    /// Starts capturing using a deterministic [`MockAudioInput`] (ideal for headless tests/previews).
    pub fn start_mock(&self) -> Arc<MockAudioInput> {
        let mock = Arc::new(MockAudioInput::new(self.config.clone()));
        mock.set_muted(self.is_muted.get());
        *self.backend.borrow_mut() = MicBackend::Mock(mock.clone());
        self.is_recording.set(true);
        mock
    }

    /// Stops capture and releases the active hardware stream.
    pub fn stop(&self) {
        *self.backend.borrow_mut() = MicBackend::Stopped;
        self.is_recording.set(false);
        self.is_speaking.set(false);
    }

    /// Mutes or unmutes the active microphone stream.
    pub fn set_muted(&self, muted: bool) {
        self.is_muted.set(muted);
        match &*self.backend.borrow() {
            MicBackend::Hardware(s) => s.set_muted(muted),
            MicBackend::Mock(m) => m.set_muted(muted),
            MicBackend::Stopped => {}
        }
    }

    /// Polls the live audio meter and updates reactive `level`, `is_speaking`, and global [`use_audio_level`].
    pub fn poll_meter(&self) -> AudioMeterSnapshot {
        let snap = match &*self.backend.borrow() {
            MicBackend::Hardware(s) => s.meter().snapshot(),
            MicBackend::Mock(m) => m.meter().snapshot(),
            MicBackend::Stopped => AudioMeterSnapshot::default(),
        };
        let speaking = !self.is_muted.get() && snap.dbfs > -42.0;
        self.is_speaking.set(speaking);
        self.level.set(snap.clone());
        publish_audio_level_snapshot(snap.clone());
        snap
    }

    /// Reads interleaved PCM samples from the active capture stream.
    pub fn read_samples(&self, out: &mut [f32]) -> usize {
        match &*self.backend.borrow() {
            MicBackend::Hardware(s) => s.read_samples(out),
            MicBackend::Mock(m) => m.read_samples(out),
            MicBackend::Stopped => 0,
        }
    }

    pub fn is_recording(&self) -> Signal<bool> {
        self.is_recording.clone()
    }

    pub fn is_muted(&self) -> Signal<bool> {
        self.is_muted.clone()
    }

    pub fn is_speaking(&self) -> Signal<bool> {
        self.is_speaking.clone()
    }

    pub fn level(&self) -> Signal<AudioMeterSnapshot> {
        self.level.clone()
    }
}

/// Reactive hook creating a managed [`MicrophoneHandle`] for the given [`AudioInputConfig`].
pub fn use_microphone(config: AudioInputConfig) -> MicrophoneHandle {
    MicrophoneHandle::new(config)
}

/// Reactive hook returning all discovered host MIDI 1.0 input & output ports.
pub fn use_midi_ports() -> Signal<Vec<crate::midi::MidiPortInfo>> {
    Signal::new(crate::midi::list_midi_ports())
}
