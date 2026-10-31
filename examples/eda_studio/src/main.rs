//! Main entry point for Strata EDA Circuit Studio & Waveform Analyzer.

use eda_studio::{
    decode_spi, render_tui_logic_analyzer, ChannelMeta, MeasurementCursors, SpiceCircuit,
    WaveformState,
};

fn main() {
    println!("▲ Strata EDA Circuit Studio & Waveform Analyzer v0.1.0");
    println!("─────────────────────────────────────────────────────────────");

    // 1. Initialize SPICE Ring Oscillator
    let mut ckt = SpiceCircuit::create_ring_oscillator(3.3, 1e-11);
    let mut clk_samples = Vec::new();
    let mut mosi_samples = Vec::new();
    let mut cs_samples = Vec::new();

    // Run 1000 transient steps
    for step in 0..1000 {
        ckt.step();
        let time_ns = ckt.current_time * 1e9;
        let v1 = ckt.node_voltages[1];
        let v2 = ckt.node_voltages[2];

        clk_samples.push((time_ns, v1 > 1.65));
        mosi_samples.push((time_ns, v2 > 1.65));
        cs_samples.push((time_ns, step > 900)); // Active Low
    }

    // 2. Decode SPI packets
    let spi_packets = decode_spi(&clk_samples, &mosi_samples, &cs_samples);
    println!("√ Decoded {} SPI bus packet(s)", spi_packets.len());

    // 3. Assemble Waveform State
    let mut state = WaveformState::new();
    state.add_channel(ChannelMeta::digital(0, "CLK (Ring Osc)", "#f59e0b"));
    state.add_channel(ChannelMeta::digital(1, "MOSI (Stage 2)", "#10b981"));
    state.add_channel(ChannelMeta::digital(2, "CS# (Chip Select)", "#06b6d4"));
    state.add_channel(ChannelMeta::bus(3, "SPI0 Bus Decoded", "#8b5cf6"));

    state.digital_samples[0] = clk_samples;
    state.digital_samples[1] = mosi_samples;
    state.digital_samples[2] = cs_samples;
    state.bus_packets[0] = spi_packets;

    state.cursors = MeasurementCursors::new(2.5, 7.5);

    // 4. Render Headless TUI Logic Analyzer View
    let tui_output = render_tui_logic_analyzer(&state, 80);
    println!("\n{}", tui_output);
}
