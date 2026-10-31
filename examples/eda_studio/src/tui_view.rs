//! Headless terminal logic analyzer view with Unicode box drawing and braille traces.

use crate::waveform::WaveformState;

/// Renders waveform state to an ANSI terminal screen buffer.
pub fn render_tui_logic_analyzer(state: &WaveformState, screen_width: usize) -> String {
    let mut out = String::new();

    // Top Title Bar
    out.push_str("▲ STRATA EDA LOGIC ANALYZER (HEADLESS TUI VIEW)\n");
    out.push_str(&"─".repeat(screen_width.min(80)));
    out.push('\n');

    // Time Ruler Header
    out.push_str("TIME: ");
    let time_step_ns = state.time_span_ns / 8.0;
    for i in 0..=8 {
        let t = state.time_start_ns + i as f64 * time_step_ns;
        out.push_str(&format!("{:.0}ns   ", t));
    }
    out.push('\n');
    out.push_str(&"─".repeat(screen_width.min(80)));
    out.push('\n');

    // Channels
    for (idx, ch) in state.channels.iter().enumerate() {
        let badge = match ch.kind {
            crate::waveform::ChannelKind::Digital => format!("{} D", ch.index),
            crate::waveform::ChannelKind::Analog => format!("{} A", ch.index),
            crate::waveform::ChannelKind::Bus => format!("{} B", ch.index),
        };

        out.push_str(&format!("[{:>4}] {:12} │ ", badge, ch.name));

        // Render digital wave line or analog bar
        if let Some(samples) = state.digital_samples.get(idx) {
            let trace_cols = screen_width.saturating_sub(25).min(55);
            let mut trace = String::with_capacity(trace_cols);

            for col in 0..trace_cols {
                let t_col = state.time_start_ns + (col as f64 / trace_cols as f64) * state.time_span_ns;
                let val = samples
                    .iter()
                    .filter(|(t, _)| *t <= t_col)
                    .last()
                    .map(|(_, v)| *v)
                    .unwrap_or(false);

                if val {
                    trace.push('▔'); // High digital state
                } else {
                    trace.push(' '); // Low digital state
                }
            }
            out.push_str(&trace);
        } else {
            out.push_str(&"─".repeat(50));
        }

        out.push('\n');
    }

    out.push_str(&"─".repeat(screen_width.min(80)));
    out.push('\n');

    // Measurement Cursors Footer
    out.push_str(&format!(
        "CURSORS: C1 = {:.1} ns │ C2 = {:.1} ns │ ΔT = {} │ f = {}\n",
        state.cursors.c1_ns,
        state.cursors.c2_ns,
        state.cursors.format_delta_time(),
        state.cursors.format_frequency()
    ));

    out
}
