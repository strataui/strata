# strata_parity_tauri: Three-Engine Visual Parity

`strata_parity_tauri` is the visual regression harness for the Strata renderer.
It captures the same inline HTML fixture in three engines at a fixed 1280x720
viewport, writes the captures and perceptual diffs, and gates on the
Chromium-vs-Strata result.

## Command

```bash
cargo run -p strata_parity_tauri --release -- --parity-suite
```

The default run captures every fixture twice into `run_a/` and `run_b/` so the
harness can verify run-to-run determinism. Use `--single-run` while iterating on
a fixture.

```bash
cargo run -p strata_parity_tauri --release -- --fixture grid_tracks --single-run
cargo run -p strata_parity_tauri --release -- --list-fixtures
cargo run -p strata_parity_tauri --release -- --parity-suite --thresholds 0.90,90
```

## Engines

| Engine | Role | Identity |
| --- | --- | --- |
| Chromium | Visual ground truth | Headless Google Chrome / Chromium |
| Tauri | Informational control | Tauri 2 on WebKitGTK on Linux, WebView2 on Windows, WKWebView on macOS |
| Strata | Native engine under test | Strata layout, compositor, and Cairo presentation |

On Linux the Tauri control is WebKitGTK, not Chromium. Chromium remains the
oracle for every threshold decision. Tauri numbers are reported and must be real
captures, but they do not decide the pass gate. The report records the detected
WebKitGTK version from `pkg-config --modversion webkit2gtk-4.1` or
`webkit2gtk-4.0`.

## Determinism

All browser captures receive an injected style and script:

- animations and transitions are disabled, and reduced-motion rules are present;
- `caret-color` is transparent and `scroll-behavior` is `auto`;
- `Date`, `Date.now`, `performance.now`, and animation frame timestamps are frozen;
- the document scroll offset and the fixture scroll container are set to the
  fixed captured offset.

Chromium is launched with a fresh profile under
`target/parity/chrome-profile/`, a fixed 1280x720 window, device scale factor
1.0, sRGB color, no GPU, no LCD text, and font hinting disabled. Chromium uses
`--virtual-time-budget=2000`; Tauri waits 1500 ms after opening the window.

The Tauri window is undecorated, non-resizable, and not maximized. Window-scoped
capture can include a few rows of window-manager padding below the page. The
harness removes that deterministic padding by cropping the capture to the
top-left 1280x720 content area before hashing and comparison. The crop note is
written to `report.json` as `tauri_capture_normalization`. A capture that is
smaller than 1280x720, empty, or not a valid image is reported as unavailable
with the exact error instead of being fabricated.

The pass gate compares the SHA-256 of `chromium.png` and `strata.png` between
`run_a/` and `run_b/`. Tauri hashes are compared and reported separately but do
not gate.

## Output Layout

The default output root is `target/parity/`:

```text
target/parity/<fixture>/
  chromium.png
  tauri.png
  strata.png
  diff_chromium_strata.png
  diff_chromium_tauri.png
  report.json
  hashes.txt
  run_a/
    chromium.png
    tauri.png
    strata.png
    diff_chromium_strata.png
    diff_chromium_tauri.png
    input.html
  run_b/
    ...
```

The top-level capture files are the canonical copies from `run_a`. Each
`report.json` records the fixture name, engine labels, viewport, per-engine
SHA-256 for both runs, SSIM, exact and perceptual percentages, mean and maximum
color distance, significant-difference pixel count and bounding box, thresholds,
known-gap notes, Tauri availability, and pass/fail. `hashes.txt` is a compact
per-run hash and determinism summary.

Diff images use Chromium as the reference. Pixels within a color distance of 0.5
are dimmed context, distances from 0.5 to 15 are amber, and distances above 15
are red.

## Thresholds

The default threshold is SSIM `>= 0.95` and perceptual match `>= 95%` against
Chromium. Each fixture may carry its own thresholds. `--thresholds
<ssim>,<perceptual>` overrides the effective threshold for the run and prints a
prominent warning; it never changes a fixture definition.

A fixture is in scope by default. A fixture that exposes a known renderer gap is
marked `in_scope: false` and carries a `known_gap` note in `components.rs` and
`report.json`. It is still captured and diffed, but it does not fail the gate.
Current out-of-scope categories include native form controls, element opacity,
border patterns, and default line-height resolution.

The command exits non-zero if any in-scope fixture misses its thresholds, or if
the Chromium or Strata capture is unavailable or non-deterministic.

## Adding a Fixture

Add a function to `src/components.rs` that returns a `Fixture` and register it in
`suite_fixtures()`.

```rust
Fixture {
    name: "my_fixture",
    description: "What the fixture exercises",
    html: r#"<!DOCTYPE html>... "#,
    in_scope: true,
    ssim_threshold: DEFAULT_SSIM_THRESHOLD,
    perceptual_threshold: DEFAULT_PERCEPTUAL_THRESHOLD,
    scroll_y: 0.0,
    known_gap: None,
}
```

Requirements for a fixture:

- inline HTML and CSS only, with no network resources;
- explicit `background` on both `html` and `body`;
- fixed pixel sizes and no animations or transitions;
- `font-family: "DejaVu Sans", sans-serif;` on the document and
  `"DejaVu Sans Mono", monospace` for code;
- a fixed `scroll_y` when the fixture exercises scrolling. A dedicated
  `#parity-scroll-root` container is scrolled in both browser engines and in
  Strata.

## Existing Modes

The harness keeps the earlier modes available:

```bash
cargo run -p strata_parity_tauri --release -- --chromium-parity
cargo run -p strata_parity_tauri --release -- --suite
cargo run -p strata_parity_tauri --release -- --tauri-only --level 2
cargo run -p strata_parity_tauri --release -- --strata-only --level 2
cargo run -p strata_parity_tauri --release -- --diff-only --level 2
cargo run -p strata_parity_tauri --release -- --bench-raw
cargo run -p strata_parity_tauri --release -- --bench-damage
cargo run -p strata_parity_tauri --release -- --scrolling-bench
```
