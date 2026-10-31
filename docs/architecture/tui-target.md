# Interactive Terminal (TUI) Target

Strata can compile the exact same declarative RSX component code directly to an interactive **Terminal User Interface (TUI)** (`strata_tui` / `--target tui`). You can deploy enterprise interfaces to SSH sessions, headless cloud servers, and terminal emulators without writing specialized terminal code.

---

## Double-Buffered Delta ANSI Engine

Traditional terminal applications redraw the entire console screen every frame, leading to noticeable flicker, cursor drifting, and high CPU usage.

Strata implements a **double-buffered differential screen buffer**:
1. **Back Buffer**: The layout engine calculates glyph characters, 24-bit TrueColor foregrounds, and background styles for each terminal cell `(col, row)`.
2. **Delta Differential Pass**: The renderer compares the back buffer against the front buffer (the current screen state).
3. **Optimized ANSI Run Emission**: Only cells that changed between frames are emitted as cursor jump sequences (`\x1b[{row};{col}H`) and ANSI color escape codes.
4. **Synchronized Updates**: Strata wraps frame emissions with the standard Synchronized Output terminal protocol (`\x1b[?2025h` ... `\x1b[?2025l`), ensuring atomic screen refreshes without visual tearing on modern terminals (iTerm2, Alacritty, Kitty, WezTerm, Windows Terminal).

---

## Hare-Niemeyer Integer Quantization

Converting floating-point CSS layout coordinates (e.g., `width: 33.333%`) into discrete terminal character grid cells creates rounding errors. Naive rounding algorithms (`round()` or `floor()`) frequently produce 1-character gaps or right-edge overflow clipping.

Strata implements the **Hare-Niemeyer Largest Remainder Method**:
- Track allocations calculate exact fractional remainders.
- Surplus integer cells are distributed systematically to tracks with the largest fractional remainders.
- Flex and grid layouts always fill 100% of the terminal console width without gaps or overflows.

---

## Unicode Box Junction Synthesis

Strata automatically analyzes adjacent border boundaries and synthesizes clean Unicode junctions:

```text
┌──────────────┬──────────────┐
│ Left Panel   │ Right Panel  │
├──────────────┼──────────────┤
│ Bottom Cell  │ Status Bar   │
└──────────────┴──────────────┘
```

The engine resolves single-line (`┌┬┐├┼┤└┴┘`), double-line (`╔╦╗╠╬╣╚╩╝`), and rounded corners (`╭─╮│╰─╯`) based on the component's Tailwind border utility classes (`border`, `border-double`, `rounded-lg`).

---

## High-Resolution Image Protocols

When running in modern terminals that support graphics extensions:
- **Kitty Graphics Protocol**: High-resolution RGB images are transferred directly into terminal VRAM using base64 chunked protocols.
- **Sixel Protocol**: Automatic palette quantization renders images on legacy DEC terminal emulators.
- **Braille Matrix Fallback**: On terminals without pixel protocol support, images and vector icons are downsampled into 2x4 Unicode braille dot matrices (`⠋`, `⠙`, `⠹`, `⠸`).

---

## Compile-Time TUI Diagnostics

Certain graphical features (such as 3D WebGL scenes or Gaussian blur filters) cannot be represented in a terminal. Strata's compiler analyzes your component tree at build time:
- Emits helpful compiler warnings with line numbers when graphics-only features are detected.
- Provides `<TuiFallback>` components that render alternate text representations when compiling to `--target tui`.

```rust
rsx! {
    <div>
        <TuiFallback
            desktop={rsx! { <Canvas3D ... /> }}
            terminal={rsx! {
                <div class="p-4 border border-neutral-700 text-neutral-400 font-mono">
                    [ 3D CAD MODEL: Rendered as ASCII Wireframe ]
                </div>
            }}
        />
    </div>
}
```
