# Electronic Design Automation (EDA) & Circuit Simulation Studio

The `eda_studio` reference application demonstrates Project Strata's capabilities for mission-critical engineering workstations, Electronic Design Automation (EDA), logic analyzers, and computer-aided circuit engineering.

It integrates high-density digital and analog waveform viewers, a built-in SPICE Modified Nodal Analysis (MNA) solver, real-time hardware protocol decoders (SPI, I2C, UART), analytical GPU-accelerated IEEE schematic drafting, and delta-ANSI headless terminal visualizers.

---

## Architectural Topology

```text
┌────────────────────────────────────────────────────────────────────────┐
│                        EDA Studio Architecture                         │
├────────────────────────────────────────────────────────────────────────┤
│                                                                        │
│   ┌────────────────────────────────────────────────────────────────┐   │
│   │             IEEE Mixed-Signal Waveform Analyzer                │   │
│   │    (Nanosecond Time Ruler, Hexagonal Bus Envelopes, Cursors)   │   │
│   └───────────────┬────────────────────────────────┬───────────────┘   │
│                   │                                │                   │
│   ┌───────────────▼────────────────┐   ┌───────────▼───────────────┐   │
│   │   Built-In SPICE MNA Solver    │   │  Serial Protocol Decoders │   │
│   │ (Transient, Ring Osc, RLC Ckt) │   │    (SPI, I2C, UART 8N1)   │   │
│   └───────────────┬────────────────┘   └───────────┬───────────────┘   │
│                   │                                │                   │
│                   └────────────────┬───────────────┘                   │
│                                    ▼                                   │
│   ┌────────────────────────────────────────────────────────────────┐   │
│   │                 Analytical Schematic Canvas                    │   │
│   │     (IEEE Standard Resistors, Caps, Inverters, Netlist Wires)  │   │
│   └────────────────────────────────┬───────────────────────────────┘   │
│                                    │                                   │
│                                    ▼                                   │
│   ┌────────────────────────────────────────────────────────────────┐   │
│   │             Headless Terminal Engine Target (TUI)              │   │
│   │        (ANSI Box Drawing & Braille Logic Analyzer Traces)      │   │
│   └────────────────────────────────────────────────────────────────┘   │
│                                                                        │
└────────────────────────────────────────────────────────────────────────┘
```

---

## 1. IEEE Mixed-Signal Waveform Analyzer (`waveform.rs`)

The waveform engine visualizes deep hardware captures sampled at nanosecond and picosecond resolutions without rendering stutter or visual aliasing:

### Channel Architecture & Classification (`ChannelKind`)

Each capture channel is classified into dedicated rendering pipelines:

```rust
pub enum ChannelKind {
    Digital, // 1-bit high/low logic trace with sharp transitions
    Analog,  // Continuous voltage waveform with dynamic V/div scaling
    Bus,     // Multi-bit packed bus rendered as continuous hexagonal envelopes
}
```

### Hexagonal Bus Envelopes (`BusPacket`)

Traditional web-based logic analyzers render digital buses using clumsy, disjointed rounded rectangles ("pill buttons"), which visually break continuous signal flows. Strata uses analytical GPU bezier geometry to render continuous hexagonal transition envelopes:

```text
    Digital Bus: 8-Bit Data Bus [D0..D7]
      /‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾\ /‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾\
     <        0x55            X        0xAA            >
      \_______________________/ \______________________/
      0.0 ns                 25.0 ns                 50.0 ns
```

Transitions converge at precise nanosecond intersections, matching high-end hardware analyzers (Saleae Logic Pro, Tektronix, Keysight).

### Dual Delta Measurement Cursors (`MeasurementCursors`)

Inspectors place interactive vertical delta cursors ($Cursor_A$ and $Cursor_B$) across the canvas to calculate:
- Time Delta: $\Delta t = |t_B - t_A|$ (nanoseconds)
- Instantaneous Frequency: $f = \frac{1}{\Delta t}$ (MHz / GHz)
- Voltage Delta: $\Delta V = |V_B - V_A|$ (volts)

---

## 2. Built-In SPICE Transient Circuit Solver (`spice.rs`)

Rather than relying on external command-line simulators like Ngspice, `eda_studio` embeds a Modified Nodal Analysis (MNA) matrix solver implemented directly in pure Rust.

### Mathematical Formulation

At each discrete time step $t_{k} = t_{k-1} + h$, the circuit satisfies Kirchhoff's Current Law (KCL) formulated as a system of nodal equations:

$$\mathbf{G} \mathbf{v}^{(k)} + \frac{\mathbf{C}}{h} \left(\mathbf{v}^{(k)} - \mathbf{v}^{(k-1)}\right) = \mathbf{i}^{(k)}$$

Where:
- $\mathbf{G}$: Conductance matrix derived from linear resistors and transistor conductances
- $\mathbf{C}$: Dynamic capacitance matrix
- $h$: Integration time step (e.g. $10\text{ ps}$)
- $\mathbf{v}^{(k)}$: Node voltage vector solved via Gaussian elimination

### Supported Elements (`CircuitElement`)

- **Independent Sources**: DC Voltage Sources with ground reference
- **Resistors**: Linear conductance $g = \frac{1}{R}$ stamped into diagonal and off-diagonal node slots
- **Capacitors**: Dynamic companion models utilizing trapezoidal numerical integration ($I = C \frac{dV}{dt}$)
- **CMOS Inverters**: Nonlinear complementary MOS inverter stages with threshold switching voltages

### Benchmark Circuits

1. **RLC Transient Circuit**: Solves underdamped and critically damped step responses across an inductor, resistor, and capacitor network.
2. **5-Stage CMOS Ring Oscillator**: Simulates an odd chain of CMOS inverters in feedback. Validates propagation delay ($\tau_{pd}$) and autonomous circuit oscillation ($f_{\text{osc}} = \frac{1}{2 N \tau_{pd}}$).

---

## 3. Protocol Analyzers & Packet Decoders (`protocol.rs`)

Raw digital transition streams are decoded on the fly into protocol event ledgers:

### Supported Protocols

- **SPI (Serial Peripheral Interface)**: Decodes clock edges (`SCK`), slave select (`CS#`), and master data (`MOSI`). Supports SPI Modes 0, 1, 2, and 3.
- **I2C (Inter-Integrated Circuit)**: Detects START and STOP bus framing conditions ($SDA$ transitions while $SCL$ is HIGH), decodes 7-bit peripheral addresses, read/write flags, ACK/NACK responses, and payload bytes.
- **UART 8N1 (Universal Asynchronous Receiver-Transmitter)**: Automatic baud-rate clock synchronization, start bit verification, 8 data bits (LSB-first), parity assertion, and stop bit validation.

---

## 4. Analytical GPU Schematic Drafting (`schematic.rs`)

Schematics are drawn using Strata's `strata_gpu_path` vector engine, evaluating analytical lines and curves directly on the GPU without CPU polygon triangulation:

### IEEE Standard Electrical Symbols

- **Resistor (`build_resistor_symbol`)**: Conforms to ANSI/IEEE standard 6-peak zig-zag wire geometry.
- **Capacitor (`build_capacitor_symbol`)**: Parallel vertical plates separated by a precise subpixel dielectric gap.
- **Ground (`build_ground_symbol`)**: 3-tier graduated horizontal ground bars connected to an orthogonal stem.
- **CMOS Inverter (`build_inverter_symbol`)**: Rightward equilateral triangle buffer with an analytical negation circle on the output terminal.
- **Orthogonal Manhattan Wiring Router (`build_orthogonal_wire`)**: Computes 90-degree orthogonal interconnect paths with solder junction dots at multi-wire intersections.

---

## 5. Headless Terminal Target (`tui_view.rs`)

For remote infrastructure debugging, headless test automation, and low-bandwidth SSH environments, `eda_studio` compiles directly to the `strata_tui` backend:

```text
▲ STRATA EDA LOGIC ANALYZER (HEADLESS TUI VIEW)
────────────────────────────────────────────────────────────────────────────────
TIME: 0ns   50ns   100ns   150ns   200ns   250ns   300ns   350ns   400ns   
────────────────────────────────────────────────────────────────────────────────
[ 0 D] CLK_100MHZ   │ ▔ ▔ ▔ ▔ ▔ ▔ ▔ ▔ ▔ ▔ ▔ ▔ ▔ ▔ ▔ ▔ ▔ ▔ ▔ ▔ ▔ ▔ ▔ ▔ ▔ ▔ ▔ ▔
[ 1 D] SPI_MOSI     │   ▔▔▔▔    ▔▔▔▔▔▔▔▔    ▔▔▔▔    ▔▔▔▔▔▔▔▔    ▔▔▔▔    
[ 2 D] SPI_CS_N     │ ▔▔        ____________________________________    
[ 3 B] DATA_BUS[7:0]│ ──[ 0x55 ]──[ 0xAA ]──[ 0x10 ]──[ 0xFF ]──[ 0x00 ]──
[ 4 A] V_CAPACITOR  │ ▄▆█▇▅▃▂                                           
────────────────────────────────────────────────────────────────────────────────
```

---

## Running the Showcase

Execute the EDA Studio application locally on desktop or terminal:

```bash
# Run the native desktop workstation UI
cargo run --release --example eda_studio

# Execute verification unit tests (SPICE solver, symbol geometry, protocol decoding)
cargo test -p eda_studio
```
