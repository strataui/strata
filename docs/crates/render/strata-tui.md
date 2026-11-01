# strata_tui

`strata_tui` is the pure-Rust Terminal User Interface (TUI) target backend of the Strata framework. It provides high-performance, double-buffered terminal rendering, 24-bit TrueColor output, minimal delta ANSI sequence flushing, synchronized updates (DEC Mode 2025), Unicode box-drawing with 4-way cardinal junction resolution, and high-density terminal graphics using half-block and Braille matrix degradation.

---

## Architectural Mission

Traditional terminal UI libraries (such as ncurses or early termion abstractions) rely on blocking I/O, partial full-screen clearing, and lack support for modern terminal capabilities like synchronized frame updates and TrueColor RGB palettes.

`strata_tui` is designed as a first-class compilation and execution target for Project Strata:
- **Zero-Flicker Double Buffering**: Uses front and back cell buffers with dirty line tracking.
- **Synchronized Terminal Updates**: Emits DEC Mode 2025 escape sequences (`\x1b[?2025h` and `\x1b[?2025l`), instructing terminal emulators (Ghostty, Alacritty, Kitty, WezTerm, iTerm2) to render frames atomically without tearing.
- **Adaptive Resolution Matrix**: Uses Half-Block characters (`▀`) to double vertical resolution ($1 \text{ cell} = 1 \times 2 \text{ pixels}$) and Braille Unicode characters (`⠀` to `⣿`) to quadruple resolution ($1 \text{ cell} = 2 \times 4 \text{ dots}$) for real-time telemetry sparklines.
- **Declarative Target Isolation**: Supports responsive target-conditional components (`tui_only!`, `no_tui!`, `tui_fallback!`) and integer layout quantization (`largest_remainder_quantization`).

---

## Subsystem Architecture

```text
┌─────────────────────────────────────────────────────────────────┐
│                    Declarative Strata RSX                       │
│      (Components, Inline Layout, Target-Conditional Blocks)     │
└────────────────────────────────┬────────────────────────────────┘
                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────────┐
│                 Largest Remainder Quantization                  │
│       - Converts Fractional Subpixel Floats to Integer Grid     │
│       - Preserves Total Dimensions Without Off-by-One Drift     │
└────────────────────────────────┬────────────────────────────────┘
                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────────┐
│                     TerminalScreen Buffer                       │
│   - Front & Back Buffer Matrices (CharCell)                     │
│   - Cell Attributes: 24-bit RGB Color, AttrBits (Bold, Dim)     │
│   - Z-Index Depth Stacking Per Cell                             │
│   - Per-Row Dirty Line Tracking                                 │
└────────────────┬───────────────────────────────┬────────────────┘
                 │                               │
                 ▼                               ▼
┌─────────────────────────────────┐   ┌───────────────────────────┐
│       Graphics Degradation      │   │   Unicode Box Drawing     │
│   - Half-Block (2x vertical)    │   │   - 4-Way Cardinal Solver │
│   - Braille 2x4 Dot Matrix      │   │   - Rounded, Solid, Heavy │
└────────────────┬────────────────┘   └─────────────┬─────────────┘
                 │                                  │
                 └────────────────┬─────────────────┘
                                  │
                                  ▼
┌─────────────────────────────────────────────────────────────────┐
│                      Delta ANSI Serializer                      │
│   - DEC Mode 2025 Synchronized Atomic Frame Wrapper             │
│   - Cursor Jump Optimization (\x1b[row;colH)                    │
│   - Minimal Style & Color State Transition Sequences            │
└─────────────────────────────────────────────────────────────────┘
```

---

## 24-Bit TrueColor and Character Attributes

`strata_tui::types` defines the memory layout of each terminal character grid cell.

### Memory Representation: `CharCell`

```rust
use strata_tui::types::{AttrBits, Color24};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CharCell {
    pub ch: char,
    pub fg: Color24,
    pub bg: Color24,
    pub style: u8,
    pub is_wide_continuation: bool,
    pub z_index: i16,
}
```

### Color Representation: `Color24`

`Color24` supports automatic fallback across terminal color tiers:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Color24 {
    #[default]
    Default,
    Ansi16(u8),                  // Standard 16-color ANSI (30-37, 90-97)
    Ansi256(u8),                 // 256-color palette (38;5;n)
    Rgb(u8, u8, u8),             // 24-bit TrueColor (38;2;r;g;b)
}
```

Escape sequence emissions are optimized to eliminate redundant bytes:

```rust
let mut buf = String::new();
let fg = Color24::Rgb(59, 130, 246); // Tailwind Blue 500
fg.write_fg_ansi(&mut buf);
assert_eq!(buf, "\x1b[38;2;59;130;246m");
```

### Text Attributes: `AttrBits`

Text styling is encoded as a compact 8-bit mask:

```rust
pub struct AttrBits;

impl AttrBits {
    pub const NONE: u8          = 0;
    pub const BOLD: u8          = 1 << 0;
    pub const DIM: u8           = 1 << 1;
    pub const ITALIC: u8        = 1 << 2;
    pub const UNDERLINE: u8     = 1 << 3;
    pub const REVERSE: u8       = 1 << 4;
    pub const STRIKETHROUGH: u8 = 1 << 5;
}
```

---

## Double-Buffered Screen & Delta ANSI Flushing

The `TerminalScreen` maintains two complete frame allocations:
1. **Front Buffer**: The canvas currently being written by layout and components.
2. **Back Buffer**: The exact visual state currently present on the terminal emulator screen.

### Synchronized Updates (DEC Mode 2025)

To prevent screen tearing during fast telemetry updates, `render_delta_ansi` wraps changes in DEC Mode 2025 protocols:
- `\x1b[?2025h`: Signals the terminal to hold rendering buffer updates.
- `\x1b[?2025l`: Signals the terminal to present all modified cells atomically.

### Minimal Cursor Movement Algorithm

When scanning modified lines, the renderer avoids writing identical cells:
- Skips unchanged runs in $O(1)$ by checking `dirty_lines[r]`.
- Jumps directly to discontinuous dirty columns using `\x1b[{row};{col}H`.
- Emits style resets (`\x1b[0m`) only when text attributes change.

```rust
use strata_tui::TerminalScreen;
use strata_tui::types::{AttrBits, Color24};

let mut screen = TerminalScreen::new(80, 24);

// Write header to front buffer
screen.put_str(
    2, 1,
    "STRATA CLUSTER TELEMETRY",
    Color24::Rgb(255, 255, 255),
    Color24::Default,
    AttrBits::BOLD,
    1,
);

// Render delta ANSI stream
let ansi_output = screen.render_delta_ansi();
assert!(ansi_output.starts_with("\x1b[?2025h"));
assert!(ansi_output.ends_with("\x1b[0m\x1b[?2025l"));
```

---

## Unicode Box Drawing & 4-Way Border Junction Mask Solver

Connecting adjacent boxes, dialog panels, and table cells in traditional TUIs often leads to visual anomalies (e.g. intersecting lines rendered as overlapping dashes).

`strata_tui::borders` implements an exact bitmask junction solver using 4 cardinal direction bits:

$$\text{Mask} = (\text{North} \times 1) + (\text{South} \times 2) + (\text{East} \times 4) + (\text{West} \times 8)$$

```rust
use strata_tui::borders::{resolve_border_junction, BorderJunctionStyle, Cardinal};

// Connect North, South, and East lines (T-junction pointing right: ├)
let mask = Cardinal::NORTH | Cardinal::SOUTH | Cardinal::EAST;

assert_eq!(resolve_border_junction(mask, BorderJunctionStyle::Solid), '├');
assert_eq!(resolve_border_junction(mask, BorderJunctionStyle::Double), '╠');
assert_eq!(resolve_border_junction(mask, BorderJunctionStyle::Heavy), '┣');

// 4-Way Intersection (+)
let cross_mask = Cardinal::NORTH | Cardinal::SOUTH | Cardinal::EAST | Cardinal::WEST;
assert_eq!(resolve_border_junction(cross_mask, BorderJunctionStyle::Solid), '┼');
assert_eq!(resolve_border_junction(cross_mask, BorderJunctionStyle::Rounded), '┼');
```

The `draw_box` function draws complete rounded, solid, double, or heavy bordered panels:

```rust
use strata_tui::borders::{draw_box, BorderJunctionStyle};
use strata_tui::TerminalScreen;
use strata_tui::types::Color24;

let mut screen = TerminalScreen::new(80, 24);

// Render a rounded border panel at col 10, row 5 with width 40, height 10
draw_box(
    &mut screen,
    10, 5, 40, 10,
    BorderJunctionStyle::Rounded,
    Color24::Rgb(148, 163, 184),
    Color24::Default,
    0,
);
```

---

## High-Density Graphics Degradation Matrix

Because terminals cannot address individual physical pixels directly, `strata_tui::graphics` maps pixel matrices into high-density Unicode glyphs.

### 1. Half-Block Rendering (`▀` U+2580)

Half-block rendering places the top pixel color into the character foreground (`fg`) and the bottom pixel color into the character background (`bg`). This effectively **doubles the vertical resolution**:

```rust
use strata_tui::graphics::render_half_block;
use strata_tui::TerminalScreen;
use strata_primitives::color::Rgba;

let mut screen = TerminalScreen::new(80, 24);

// 4x4 RGBA bitmap image
let pixels = vec![
    Rgba::new(255, 0, 0, 255), Rgba::new(255, 0, 0, 255), Rgba::new(0, 255, 0, 255), Rgba::new(0, 255, 0, 255),
    Rgba::new(0, 0, 255, 255), Rgba::new(0, 0, 255, 255), Rgba::new(255, 255, 0, 255), Rgba::new(255, 255, 0, 255),
    Rgba::new(255, 0, 255, 255), Rgba::new(255, 0, 255, 255), Rgba::new(0, 255, 255, 255), Rgba::new(0, 255, 255, 255),
    Rgba::new(255, 255, 255, 255), Rgba::new(255, 255, 255, 255), Rgba::new(0, 0, 0, 255), Rgba::new(0, 0, 0, 255),
];

// Renders into 4 cols x 2 rows
render_half_block(&pixels, 4, 4, &mut screen, 0, 0, 0);
```

### 2. Braille Dot Matrix (`U+2800`..`U+28FF`)

Unicode Braille patterns provide a $2 \times 4$ dot grid per character cell. This delivers **$4\times$ vertical and $2\times$ horizontal resolution**, ideal for telemetry sparklines and latency histograms:

$$\text{Unicode Code} = 0\text{x}2800 + \sum_{k=0}^{7} (\text{dot}_k \cdot 2^k)$$

```rust
use strata_tui::graphics::{braille_char_from_dots, render_braille};
use strata_tui::TerminalScreen;
use strata_tui::types::Color24;

let mut dots = [false; 8];
dots[0] = true; // Top-left
dots[7] = true; // Bottom-right
let ch = braille_char_from_dots(dots);
assert_eq!(ch, '⢁');

// Render a 16x8 monochrome telemetry sparkline bitmap into 8x2 character cells
let sparkline_bitmap = vec![false; 16 * 8];
let mut screen = TerminalScreen::new(80, 24);

render_braille(
    &sparkline_bitmap, 16, 8,
    &mut screen, 10, 5,
    Color24::Rgb(16, 185, 129), // Emerald 500
    Color24::Default,
    0,
);
```

---

## Fractional Layout Quantization

When converting responsive CSS layouts (which calculate positions in floating-point subpixels) to discrete terminal character coordinates, simple rounding can accumulate errors, causing off-by-one gaps or border clipping.

`strata_tui::quantization::largest_remainder_quantization` implements the Hare-Niemeyer largest remainder method:

```rust
use strata_tui::quantization::largest_remainder_quantization;

// Allocate 80 terminal columns across 3 dashboard panels: 33.3%, 33.3%, 33.4%
let fractional_widths = vec![26.666, 26.666, 26.668];
let total_cols = 80;

let integer_widths = largest_remainder_quantization(&fractional_widths, total_cols);

assert_eq!(integer_widths, vec![27, 26, 27]);
assert_eq!(integer_widths.iter().sum::<u16>(), 80);
```

---

## Complete Working Rust Example: Production TUI Cluster Telemetry Monitor

```rust
use strata_tui::borders::{draw_box, BorderJunctionStyle};
use strata_tui::graphics::render_braille;
use strata_tui::types::{AttrBits, Color24};
use strata_tui::TerminalScreen;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    const COLS: u16 = 80;
    const ROWS: u16 = 24;

    // 1. Initialize Double-Buffered Screen
    let mut screen = TerminalScreen::new(COLS, ROWS);

    // 2. Render Outer Chassis Box
    draw_box(
        &mut screen,
        0, 0, COLS, ROWS,
        BorderJunctionStyle::Rounded,
        Color24::Rgb(71, 85, 105), // Slate 600
        Color24::Default,
        0,
    );

    // 3. Render Header Banner
    screen.put_str(
        3, 1,
        "STRATA CLOUD INFRASTRUCTURE MONITOR - US-EAST-1",
        Color24::Rgb(248, 250, 252),
        Color24::Default,
        AttrBits::BOLD,
        1,
    );

    // 4. Render Telemetry Cards
    draw_box(&mut screen, 2, 3, 36, 9, BorderJunctionStyle::Solid, Color24::Rgb(51, 65, 85), Color24::Default, 1);
    screen.put_str(4, 4, "INGRESS GATEWAY", Color24::Rgb(56, 189, 248), Color24::Default, AttrBits::BOLD, 2);
    screen.put_str(4, 6, "Throughput: 42.8 Gbps", Color24::Rgb(203, 213, 225), Color24::Default, AttrBits::NONE, 2);
    screen.put_str(4, 7, "P99 Latency: 1.42 ms", Color24::Rgb(203, 213, 225), Color24::Default, AttrBits::NONE, 2);
    screen.put_str(4, 8, "Status: HEALTHY", Color24::Rgb(52, 211, 153), Color24::Default, AttrBits::BOLD, 2);

    draw_box(&mut screen, 40, 3, 38, 9, BorderJunctionStyle::Solid, Color24::Rgb(51, 65, 85), Color24::Default, 1);
    screen.put_str(42, 4, "KUBERNETES WORKER POOL", Color24::Rgb(168, 85, 247), Color24::Default, AttrBits::BOLD, 2);
    screen.put_str(42, 6, "Active Nodes: 128 / 128", Color24::Rgb(203, 213, 225), Color24::Default, AttrBits::NONE, 2);
    screen.put_str(42, 7, "CPU Load: 64.2%", Color24::Rgb(203, 213, 225), Color24::Default, AttrBits::NONE, 2);
    screen.put_str(42, 8, "Memory Alloc: 412 / 512 GB", Color24::Rgb(203, 213, 225), Color24::Default, AttrBits::NONE, 2);

    // 5. Render High-Density Braille Real-Time Telemetry Sparkline
    screen.put_str(4, 13, "LIVE TELEMETRY SPARKLINE (2x4 Braille Matrix):", Color24::Rgb(148, 163, 184), Color24::Default, AttrBits::NONE, 1);

    // Create a 72x16 monochrome bitmap representing a sinusoidal telemetry load trace
    let mut sparkline = vec![false; 72 * 16];
    for x in 0..72 {
        let normalized_x = (x as f32) / 72.0 * std::f32::consts::PI * 4.0;
        let y = (((normalized_x.sin() + 1.0) * 0.5) * 15.0).round() as usize;
        let clamped_y = y.min(15);
        sparkline[clamped_y * 72 + x] = true;
    }

    render_braille(
        &sparkline, 72, 16,
        &mut screen,
        4, 15,
        Color24::Rgb(14, 165, 233), // Sky 500
        Color24::Default,
        2,
    );

    // 6. Output Atomic Synchronized ANSI Escape Stream
    let delta_ansi = screen.render_delta_ansi();
    print!("{}", delta_ansi);

    println!("\nRendered {} bytes of delta ANSI escape sequences to stdout.", delta_ansi.len());
    Ok(())
}
```
