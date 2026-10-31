//! IEEE standard digital and analog mixed-signal waveform visualization engine.

/// Channel classification in an EDA logic analyzer / oscilloscope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelKind {
    Digital,
    Analog,
    Bus,
}

/// Metadata configuration for a waveform channel.
#[derive(Debug, Clone)]
pub struct ChannelMeta {
    pub index: usize,
    pub name: String,
    pub kind: ChannelKind,
    pub color_hex: &'static str,
    pub voltage_scale: f32, // e.g. 1.0 V/div or 3.3V logic
    pub height_px: f32,
}

impl ChannelMeta {
    pub fn digital(index: usize, name: impl Into<String>, color_hex: &'static str) -> Self {
        Self {
            index,
            name: name.into(),
            kind: ChannelKind::Digital,
            color_hex,
            voltage_scale: 3.3,
            height_px: 44.0,
        }
    }

    pub fn analog(index: usize, name: impl Into<String>, color_hex: &'static str, voltage_scale: f32) -> Self {
        Self {
            index,
            name: name.into(),
            kind: ChannelKind::Analog,
            color_hex,
            voltage_scale,
            height_px: 88.0,
        }
    }

    pub fn bus(index: usize, name: impl Into<String>, color_hex: &'static str) -> Self {
        Self {
            index,
            name: name.into(),
            kind: ChannelKind::Bus,
            color_hex,
            voltage_scale: 0.0,
            height_px: 36.0,
        }
    }
}

/// Hexagonal bus packet bubble representing multi-bit values without separated pill buttons.
#[derive(Debug, Clone, PartialEq)]
pub struct BusPacket {
    pub start_ns: f64,
    pub end_ns: f64,
    pub label: String,
    pub value_hex: u32,
    pub status: Option<String>,
}

impl BusPacket {
    pub fn new(start_ns: f64, end_ns: f64, label: impl Into<String>, value_hex: u32) -> Self {
        Self {
            start_ns,
            end_ns,
            label: label.into(),
            value_hex,
            status: None,
        }
    }
}

/// Measurement delta cursor pair (Cursor 1 & Cursor 2).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeasurementCursors {
    pub c1_ns: f64,
    pub c2_ns: f64,
}

impl MeasurementCursors {
    pub fn new(c1_ns: f64, c2_ns: f64) -> Self {
        Self { c1_ns, c2_ns }
    }

    pub fn delta_time_ns(&self) -> f64 {
        (self.c2_ns - self.c1_ns).abs()
    }

    /// Computes instantaneous frequency f = 1 / Delta T in Hertz.
    pub fn frequency_hz(&self) -> f64 {
        let dt_sec = self.delta_time_ns() * 1e-9;
        if dt_sec > 1e-15 {
            1.0 / dt_sec
        } else {
            0.0
        }
    }

    /// Formats delta time into appropriate engineering units (ps, ns, us, ms, s).
    pub fn format_delta_time(&self) -> String {
        let ns = self.delta_time_ns();
        if ns < 1.0 {
            format!("{:.2} ps", ns * 1000.0)
        } else if ns < 1_000.0 {
            format!("{:.2} ns", ns)
        } else if ns < 1_000_000.0 {
            format!("{:.2} us", ns / 1_000.0)
        } else {
            format!("{:.2} ms", ns / 1_000_000.0)
        }
    }

    /// Formats frequency into appropriate engineering units (GHz, MHz, kHz, Hz).
    pub fn format_frequency(&self) -> String {
        let hz = self.frequency_hz();
        if hz >= 1e9 - 1e-2 {
            format!("{:.2} GHz", hz / 1e9)
        } else if hz >= 1e6 - 1e-2 {
            format!("{:.2} MHz", hz / 1e6)
        } else if hz >= 1e3 - 1e-2 {
            format!("{:.2} kHz", hz / 1e3)
        } else {
            format!("{:.2} Hz", hz)
        }
    }
}

/// Master waveform state for logic analyzer and oscilloscope presentation.
#[derive(Debug, Clone)]
pub struct WaveformState {
    pub channels: Vec<ChannelMeta>,
    pub time_start_ns: f64,
    pub time_span_ns: f64,
    pub digital_samples: Vec<Vec<(f64, bool)>>, // Per-channel (time_ns, high/low)
    pub analog_samples: Vec<Vec<(f64, f32)>>,   // Per-channel (time_ns, voltage)
    pub bus_packets: Vec<Vec<BusPacket>>,       // Per-bus channel
    pub cursors: MeasurementCursors,
    pub trigger_time_ns: f64,
}

impl Default for WaveformState {
    fn default() -> Self {
        Self::new()
    }
}

impl WaveformState {
    pub fn new() -> Self {
        Self {
            channels: Vec::new(),
            time_start_ns: 0.0,
            time_span_ns: 500.0, // 500 ns default viewport
            digital_samples: Vec::new(),
            analog_samples: Vec::new(),
            bus_packets: Vec::new(),
            cursors: MeasurementCursors::new(100.0, 250.0),
            trigger_time_ns: 0.0,
        }
    }

    pub fn add_channel(&mut self, meta: ChannelMeta) {
        match meta.kind {
            ChannelKind::Digital => {
                self.digital_samples.push(Vec::new());
            }
            ChannelKind::Analog => {
                self.analog_samples.push(Vec::new());
            }
            ChannelKind::Bus => {
                self.bus_packets.push(Vec::new());
            }
        }
        self.channels.push(meta);
    }
}
