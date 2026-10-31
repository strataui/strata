//! Flagship Electronic Design Automation (EDA) Circuit Studio & Waveform Analyzer in Strata.

pub mod protocol;
pub mod schematic;
pub mod spice;
pub mod tui_view;
pub mod waveform;

pub use protocol::{decode_i2c_packets, decode_spi, decode_uart_8n1, ProtocolEvent};
pub use schematic::{
    build_capacitor_symbol, build_ground_symbol, build_inverter_symbol, build_orthogonal_wire,
    build_resistor_symbol,
};
pub use spice::{CircuitElement, SpiceCircuit};
pub use tui_view::render_tui_logic_analyzer;
pub use waveform::{
    BusPacket, ChannelKind, ChannelMeta, MeasurementCursors, WaveformState,
};
pub use strata_gpu_path::Point2D;
