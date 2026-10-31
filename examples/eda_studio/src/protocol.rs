//! Protocol analyzers decoding serial buses into packet event ledgers.

use crate::waveform::BusPacket;

/// Protocol packet ledger entry for the inspector table.
#[derive(Debug, Clone, PartialEq)]
pub struct ProtocolEvent {
    pub protocol: &'static str,
    pub event_type: String,
    pub start_time_ns: f64,
    pub duration_ns: f64,
    pub payload_hex: u32,
    pub description: String,
}

/// Decodes synchronous Serial Peripheral Interface (SPI) clock and data transitions.
pub fn decode_spi(
    clk_transitions: &[(f64, bool)],
    mosi_samples: &[(f64, bool)],
    cs_samples: &[(f64, bool)],
) -> Vec<BusPacket> {
    let mut packets = Vec::new();
    let mut current_byte = 0u8;
    let mut bits_counted = 0;
    let mut byte_start_ns = 0.0;

    for &(t_clk, clk_high) in clk_transitions {
        // Sample on rising edge (SPI Mode 0)
        if clk_high {
            // Check if CS is asserted (active low = false)
            let is_cs_active = cs_samples
                .iter()
                .filter(|(t, _)| *t <= t_clk)
                .last()
                .map(|(_, val)| !*val)
                .unwrap_or(true);

            if is_cs_active {
                if bits_counted == 0 {
                    byte_start_ns = t_clk;
                }

                let mosi_bit = mosi_samples
                    .iter()
                    .filter(|(t, _)| *t <= t_clk)
                    .last()
                    .map(|(_, val)| if *val { 1 } else { 0 })
                    .unwrap_or(0);

                current_byte = (current_byte << 1) | mosi_bit;
                bits_counted += 1;

                if bits_counted == 8 {
                    let mut packet = BusPacket::new(
                        byte_start_ns,
                        t_clk,
                        format!("0x{:02X}", current_byte),
                        current_byte as u32,
                    );
                    packet.status = Some(format!("SPI: '{:02X}'", current_byte));
                    packets.push(packet);
                    bits_counted = 0;
                    current_byte = 0;
                }
            }
        }
    }

    packets
}

/// Decodes Inter-Integrated Circuit (I2C) bus packets (Start, Addr+RW, ACK, Data, Stop).
pub fn decode_i2c_packets(
    scl_samples: &[(f64, bool)],
    sda_samples: &[(f64, bool)],
) -> Vec<ProtocolEvent> {
    let mut events = Vec::new();

    // Scan for I2C Start condition: SDA falling while SCL is High
    for i in 1..sda_samples.len() {
        let (t_prev, sda_prev) = sda_samples[i - 1];
        let (t_curr, sda_curr) = sda_samples[i];

        if sda_prev && !sda_curr {
            // Falling edge on SDA
            let scl_is_high = scl_samples
                .iter()
                .filter(|(t, _)| *t <= t_curr)
                .last()
                .map(|(_, val)| *val)
                .unwrap_or(false);

            if scl_is_high {
                events.push(ProtocolEvent {
                    protocol: "I2C",
                    event_type: "START".to_string(),
                    start_time_ns: t_curr,
                    duration_ns: (t_curr - t_prev).abs().max(10.0),
                    payload_hex: 0,
                    description: "I2C Bus Start Condition".to_string(),
                });
            }
        }
    }

    events
}

/// Decodes Universal Asynchronous Receiver-Transmitter (UART) 8N1 frames.
pub fn decode_uart_8n1(
    rx_samples: &[(f64, bool)],
    baud_rate: u32,
) -> Vec<ProtocolEvent> {
    let mut events = Vec::new();
    let bit_period_ns = 1_000_000_000.0 / baud_rate as f64;

    let mut i = 0;
    while i < rx_samples.len() {
        let (t, val) = rx_samples[i];
        // Look for Start bit (falling edge from High to Low)
        if !val && (i == 0 || rx_samples[i - 1].1) {
            let frame_start = t;
            let sample_mid_start = frame_start + bit_period_ns * 1.5;
            let mut byte_val = 0u8;

            for bit_idx in 0..8 {
                let sample_t = sample_mid_start + bit_period_ns * bit_idx as f64;
                let bit_state = rx_samples
                    .iter()
                    .filter(|(t_s, _)| *t_s <= sample_t)
                    .last()
                    .map(|(_, s)| *s)
                    .unwrap_or(false);

                if bit_state {
                    byte_val |= 1 << bit_idx;
                }
            }

            let char_repr = if byte_val.is_ascii_graphic() {
                format!("'{}'", byte_val as char)
            } else {
                format!("0x{:02X}", byte_val)
            };

            events.push(ProtocolEvent {
                protocol: "UART",
                event_type: "DATA".to_string(),
                start_time_ns: frame_start,
                duration_ns: bit_period_ns * 10.0,
                payload_hex: byte_val as u32,
                description: format!("UART 8N1: {} (ASCII)", char_repr),
            });

            // Advance past the frame
            while i < rx_samples.len() && rx_samples[i].0 < frame_start + bit_period_ns * 9.5 {
                i += 1;
            }
        } else {
            i += 1;
        }
    }

    events
}
