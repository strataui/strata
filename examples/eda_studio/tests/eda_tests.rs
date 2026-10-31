use eda_studio::{
    build_capacitor_symbol, build_ground_symbol, build_inverter_symbol, build_resistor_symbol,
    decode_i2c_packets, decode_spi, decode_uart_8n1, render_tui_logic_analyzer,
    ChannelMeta, MeasurementCursors, Point2D, SpiceCircuit, WaveformState,
};

#[test]
fn test_spice_rlc_transient_charging() {
    // 5V step through 1k ohm into 1uF cap
    let mut ckt = SpiceCircuit::create_rlc_circuit(1000.0, 1e-6, 1e-5);
    let initial_v = ckt.node_voltages[2];
    assert_eq!(initial_v, 0.0);

    // Run 500 steps (5 ms = 5 RC time constants, should reach ~99% of 5V)
    for _ in 0..500 {
        ckt.step();
    }

    let final_v = ckt.node_voltages[2];
    assert!(final_v > 4.5, "Capacitor should charge towards 5V (got {})", final_v);
    assert!(final_v <= 5.01);
}

#[test]
fn test_spice_ring_oscillator_oscillation() {
    let mut ckt = SpiceCircuit::create_ring_oscillator(3.3, 1e-11);
    let mut voltages_stage1 = Vec::new();

    for _ in 0..500 {
        ckt.step();
        voltages_stage1.push(ckt.node_voltages[1]);
    }

    // Must exhibit voltage transitions across the simulation
    let min_v = voltages_stage1.iter().cloned().fold(f64::INFINITY, f64::min);
    let max_v = voltages_stage1.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

    assert!(max_v > min_v, "Ring oscillator must produce non-static voltage signals");
    assert!(max_v <= 3.35);
}

#[test]
fn test_waveform_delta_cursors() {
    // 100 ns delta = 10 MHz
    let cursors = MeasurementCursors::new(100.0, 200.0);
    assert_eq!(cursors.delta_time_ns(), 100.0);
    assert!((cursors.frequency_hz() - 10_000_000.0).abs() < 1e-3);
    assert_eq!(cursors.format_delta_time(), "100.00 ns");
    assert_eq!(cursors.format_frequency(), "10.00 MHz");

    // 1 ns delta = 1 GHz
    let fast_cursors = MeasurementCursors::new(0.0, 1.0);
    assert_eq!(fast_cursors.delta_time_ns(), 1.0);
    assert!((fast_cursors.frequency_hz() - 1_000_000_000.0).abs() < 1e-3);
    assert_eq!(fast_cursors.format_frequency(), "1.00 GHz");
}

#[test]
fn test_serial_protocol_spi_and_i2c_decoding() {
    // Generate synthetic SPI 8-bit byte 0xAA (10101010b)
    let clk = vec![
        (10.0, false), (20.0, true), (30.0, false), (40.0, true),
        (50.0, false), (60.0, true), (70.0, false), (80.0, true),
        (90.0, false), (100.0, true), (110.0, false), (120.0, true),
        (130.0, false), (140.0, true), (150.0, false), (160.0, true),
    ];
    let mosi = vec![
        (0.0, true), (25.0, false), (45.0, true), (65.0, false),
        (85.0, true), (105.0, false), (125.0, true), (145.0, false),
    ];
    let cs = vec![(0.0, false), (200.0, true)];

    let packets = decode_spi(&clk, &mosi, &cs);
    assert_eq!(packets.len(), 1);
    assert_eq!(packets[0].value_hex, 0xAA);

    // I2C start condition
    let sda = vec![(10.0, true), (15.0, false)];
    let scl = vec![(0.0, true), (30.0, true)];
    let i2c_events = decode_i2c_packets(&scl, &sda);
    assert_eq!(i2c_events.len(), 1);
    assert_eq!(i2c_events[0].event_type, "START");

    // UART 8N1 test (char 'A' = 0x41)
    let bit_period = 100.0; // 10 MHz baud rate for test
    let mut rx = Vec::new();
    rx.push((0.0, true)); // Idle
    rx.push((10.0, false)); // Start bit
    // 0x41 = 0b01000001 (LSB first: 1, 0, 0, 0, 0, 0, 1, 0)
    let bits = [true, false, false, false, false, false, true, false];
    for (idx, b) in bits.iter().enumerate() {
        rx.push((10.0 + (idx as f64 + 1.0) * bit_period, *b));
    }
    rx.push((10.0 + 9.0 * bit_period, true)); // Stop bit

    let uart_events = decode_uart_8n1(&rx, 10_000_000);
    assert_eq!(uart_events.len(), 1);
    assert_eq!(uart_events[0].payload_hex, 0x41);
}

#[test]
fn test_ieee_schematic_symbols() {
    let r_path = build_resistor_symbol(Point2D::new(0.0, 0.0), 70.0);
    assert!(!r_path.verbs.is_empty());
    assert!(r_path.bounding_box().is_some());

    let c_path = build_capacitor_symbol(Point2D::new(50.0, 50.0), 30.0, 8.0);
    assert!(!c_path.verbs.is_empty());

    let g_path = build_ground_symbol(Point2D::new(100.0, 100.0));
    assert!(!g_path.verbs.is_empty());

    let inv_path = build_inverter_symbol(Point2D::new(20.0, 20.0), 40.0, 30.0);
    assert!(!inv_path.verbs.is_empty());
}

#[test]
fn test_tui_logic_analyzer_view() {
    let mut state = WaveformState::new();
    state.add_channel(ChannelMeta::digital(0, "CLK", "#f59e0b"));
    state.add_channel(ChannelMeta::digital(1, "MOSI", "#10b981"));

    state.digital_samples[0] = vec![(0.0, false), (50.0, true), (100.0, false)];
    state.digital_samples[1] = vec![(0.0, true), (60.0, false), (120.0, true)];

    let rendered = render_tui_logic_analyzer(&state, 80);
    assert!(rendered.contains("STRATA EDA LOGIC ANALYZER"));
    assert!(rendered.contains("[ 0 D] CLK"));
    assert!(rendered.contains("[ 1 D] MOSI"));
    assert!(rendered.contains("CURSORS:"));
}
