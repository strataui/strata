//! Pure-Rust MIDI 1.0 real-time byte-stream parser, serializer, cross-platform rawmidi
//! port discovery, and deterministic `MockMidiBus` loopback for headless CI.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// Direction of a MIDI port.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MidiPortDirection {
    Input,
    Output,
}

/// Metadata describing a discovered MIDI 1.0 input or output endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MidiPortInfo {
    pub id: String,
    pub name: String,
    pub direction: MidiPortDirection,
}

/// Parsed MIDI 1.0 channel, system common, SysEx, or real-time message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MidiMessage {
    NoteOff { channel: u8, note: u8, velocity: u8 },
    NoteOn { channel: u8, note: u8, velocity: u8 },
    PolyAftertouch { channel: u8, note: u8, pressure: u8 },
    ControlChange { channel: u8, controller: u8, value: u8 },
    ProgramChange { channel: u8, program: u8 },
    ChannelAftertouch { channel: u8, pressure: u8 },
    /// 14-bit signed pitch bend centered at `0` (`-8192..=8191`).
    PitchBend { channel: u8, value: i16 },
    /// System Exclusive payload (excluding `0xF0` start and `0xF7` end framing bytes).
    SysEx(Vec<u8>),
    TimeCodeQuarterFrame(u8),
    SongPosition(u16),
    SongSelect(u8),
    TuneRequest,
    TimingClock,
    Start,
    Continue,
    Stop,
    ActiveSensing,
    SystemReset,
}

impl MidiMessage {
    /// Serializes this [`MidiMessage`] into standard MIDI 1.0 wire bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        match self {
            Self::NoteOff {
                channel,
                note,
                velocity,
            } => vec![0x80 | (channel & 0x0F), note & 0x7F, velocity & 0x7F],
            Self::NoteOn {
                channel,
                note,
                velocity,
            } => vec![0x90 | (channel & 0x0F), note & 0x7F, velocity & 0x7F],
            Self::PolyAftertouch {
                channel,
                note,
                pressure,
            } => vec![0xA0 | (channel & 0x0F), note & 0x7F, pressure & 0x7F],
            Self::ControlChange {
                channel,
                controller,
                value,
            } => vec![0xB0 | (channel & 0x0F), controller & 0x7F, value & 0x7F],
            Self::ProgramChange { channel, program } => {
                vec![0xC0 | (channel & 0x0F), program & 0x7F]
            }
            Self::ChannelAftertouch { channel, pressure } => {
                vec![0xD0 | (channel & 0x0F), pressure & 0x7F]
            }
            Self::PitchBend { channel, value } => {
                let raw = (*value as i32 + 8192).clamp(0, 16383) as u16;
                vec![
                    0xE0 | (channel & 0x0F),
                    (raw & 0x7F) as u8,
                    ((raw >> 7) & 0x7F) as u8,
                ]
            }
            Self::SysEx(payload) => {
                let mut out = Vec::with_capacity(payload.len() + 2);
                out.push(0xF0);
                for &b in payload {
                    out.push(b & 0x7F);
                }
                out.push(0xF7);
                out
            }
            Self::TimeCodeQuarterFrame(v) => vec![0xF1, v & 0x7F],
            Self::SongPosition(pos) => {
                vec![0xF2, (*pos & 0x7F) as u8, ((*pos >> 7) & 0x7F) as u8]
            }
            Self::SongSelect(song) => vec![0xF3, song & 0x7F],
            Self::TuneRequest => vec![0xF6],
            Self::TimingClock => vec![0xF8],
            Self::Start => vec![0xFA],
            Self::Continue => vec![0xFB],
            Self::Stop => vec![0xFC],
            Self::ActiveSensing => vec![0xFE],
            Self::SystemReset => vec![0xFF],
        }
    }
}

/// Spec-compliant stateful MIDI 1.0 byte-stream parser handling running status,
/// interleaved System Real-Time messages (`0xF8..=0xFF`) mid-SysEx/channel message,
/// and `NoteOn(velocity == 0) -> NoteOff` normalization.
#[derive(Debug, Default)]
pub struct MidiParser {
    running_status: u8,
    data: [u8; 2],
    data_len: usize,
    in_sysex: bool,
    sysex_buf: Vec<u8>,
}

impl MidiParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds raw MIDI bytes and emits all completed [`MidiMessage`]s via `on_message`.
    pub fn feed_bytes<F: FnMut(MidiMessage)>(&mut self, bytes: &[u8], mut on_message: F) {
        for &b in bytes {
            self.feed_byte(b, &mut on_message);
        }
    }

    /// Feeds a single MIDI wire byte into the parser state machine.
    pub fn feed_byte<F: FnMut(MidiMessage)>(&mut self, byte: u8, on_message: &mut F) {
        // System Real-Time bytes (0xF8..=0xFF) can appear ANYWHERE, even mid-SysEx,
        // without altering running status or interrupting an active message!
        if byte >= 0xF8 {
            match byte {
                0xF8 => on_message(MidiMessage::TimingClock),
                0xFA => on_message(MidiMessage::Start),
                0xFB => on_message(MidiMessage::Continue),
                0xFC => on_message(MidiMessage::Stop),
                0xFE => on_message(MidiMessage::ActiveSensing),
                0xFF => on_message(MidiMessage::SystemReset),
                _ => {}
            }
            return;
        }

        if byte == 0xF0 {
            self.in_sysex = true;
            self.sysex_buf.clear();
            self.running_status = 0;
            self.data_len = 0;
            return;
        }

        if byte == 0xF7 {
            if self.in_sysex {
                let payload = std::mem::take(&mut self.sysex_buf);
                self.in_sysex = false;
                on_message(MidiMessage::SysEx(payload));
            }
            self.running_status = 0;
            self.data_len = 0;
            return;
        }

        if byte & 0x80 != 0 {
            // New status byte terminates any unclosed SysEx cleanly.
            self.in_sysex = false;
            self.sysex_buf.clear();
            self.data_len = 0;

            if byte == 0xF6 {
                self.running_status = 0;
                on_message(MidiMessage::TuneRequest);
            } else if byte >= 0xF1 {
                // System Common cancels running status for future channel voice messages,
                // stored temporarily while collecting its own 1 or 2 data bytes.
                self.running_status = byte;
            } else {
                // Channel Voice status (0x80..=0xEF) sets persistent running status.
                self.running_status = byte;
            }
            return;
        }

        // Data byte (0x00..=0x7F)
        if self.in_sysex {
            self.sysex_buf.push(byte);
            return;
        }

        let status = self.running_status;
        if status == 0 {
            return;
        }

        let expected = expected_data_bytes(status);
        if expected == 0 {
            return;
        }

        self.data[self.data_len] = byte;
        self.data_len += 1;

        if self.data_len >= expected {
            self.data_len = 0;
            let d0 = self.data[0];
            let d1 = self.data[1];
            let kind = status & 0xF0;
            let ch = status & 0x0F;

            match kind {
                0x80 => on_message(MidiMessage::NoteOff {
                    channel: ch,
                    note: d0,
                    velocity: d1,
                }),
                0x90 => {
                    if d1 == 0 {
                        on_message(MidiMessage::NoteOff {
                            channel: ch,
                            note: d0,
                            velocity: 0,
                        });
                    } else {
                        on_message(MidiMessage::NoteOn {
                            channel: ch,
                            note: d0,
                            velocity: d1,
                        });
                    }
                }
                0xA0 => on_message(MidiMessage::PolyAftertouch {
                    channel: ch,
                    note: d0,
                    pressure: d1,
                }),
                0xB0 => on_message(MidiMessage::ControlChange {
                    channel: ch,
                    controller: d0,
                    value: d1,
                }),
                0xC0 => on_message(MidiMessage::ProgramChange {
                    channel: ch,
                    program: d0,
                }),
                0xD0 => on_message(MidiMessage::ChannelAftertouch {
                    channel: ch,
                    pressure: d0,
                }),
                0xE0 => {
                    let raw = (d0 as u16) | ((d1 as u16) << 7);
                    on_message(MidiMessage::PitchBend {
                        channel: ch,
                        value: raw as i16 - 8192,
                    });
                }
                0xF0 => {
                    match status {
                        0xF1 => on_message(MidiMessage::TimeCodeQuarterFrame(d0)),
                        0xF2 => {
                            let pos = (d0 as u16) | ((d1 as u16) << 7);
                            on_message(MidiMessage::SongPosition(pos));
                        }
                        0xF3 => on_message(MidiMessage::SongSelect(d0)),
                        _ => {}
                    }
                    // System common clears running status after completion.
                    self.running_status = 0;
                }
                _ => {}
            }
        }
    }
}

#[inline]
const fn expected_data_bytes(status: u8) -> usize {
    match status & 0xF0 {
        0x80 | 0x90 | 0xA0 | 0xB0 | 0xE0 => 2,
        0xC0 | 0xD0 => 1,
        0xF0 => match status {
            0xF1 | 0xF3 => 1,
            0xF2 => 2,
            _ => 0,
        },
        _ => 0,
    }
}

/// Discovers native host MIDI 1.0 endpoints (Linux `/dev/snd/midi*` rawmidi; empty on headless CI).
pub fn list_midi_ports() -> Vec<MidiPortInfo> {
    let mut ports = Vec::new();
    #[cfg(target_os = "linux")]
    {
        if let Ok(entries) = std::fs::read_dir("/dev/snd") {
            for entry in entries.flatten() {
                let fname = entry.file_name();
                let name = fname.to_string_lossy();
                if name.starts_with("midi") {
                    let dev_path = format!("/dev/snd/{name}");
                    ports.push(MidiPortInfo {
                        id: format!("in:{dev_path}"),
                        name: format!("ALSA RawMIDI {name} (In)"),
                        direction: MidiPortDirection::Input,
                    });
                    ports.push(MidiPortInfo {
                        id: format!("out:{dev_path}"),
                        name: format!("ALSA RawMIDI {name} (Out)"),
                        direction: MidiPortDirection::Output,
                    });
                }
            }
        }
    }
    ports
}

/// Deterministic in-memory MIDI 1.0 loopback bus for testing synth/controller pipelines without hardware.
#[derive(Clone, Default)]
pub struct MockMidiBus {
    queue: Arc<Mutex<VecDeque<u8>>>,
}

impl MockMidiBus {
    pub fn new() -> Self {
        Self::default()
    }

    /// Sends a structured [`MidiMessage`] over the mock MIDI wire.
    pub fn send_message(&self, msg: &MidiMessage) {
        self.send_raw(&msg.to_bytes());
    }

    /// Sends raw MIDI bytes onto the loopback wire.
    pub fn send_raw(&self, bytes: &[u8]) {
        if let Ok(mut q) = self.queue.lock() {
            q.extend(bytes.iter().copied());
        }
    }

    /// Drains all pending wire bytes through `parser` and returns all parsed [`MidiMessage`]s.
    pub fn receive_messages(&self, parser: &mut MidiParser) -> Vec<MidiMessage> {
        let mut raw = Vec::new();
        if let Ok(mut q) = self.queue.lock() {
            raw.extend(q.drain(..));
        }
        let mut out = Vec::new();
        parser.feed_bytes(&raw, |m| out.push(m));
        out
    }
}
