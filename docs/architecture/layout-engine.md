# Layout Engine & Chromium Parity

Strata's layout engine (`strata_layout`) is a ground-up, pure-Rust implementation of the W3C CSS specifications. It achieves subpixel layout parity with Chromium's Blink engine without running a single line of C++ or JavaScript.

---

## Supported W3C Specifications

### 1. CSS Box Model
- **Box Sizing**: Full support for `box-sizing: border-box` (default) and `content-box`.
- **Margin Collapsing**: Accurate implementation of vertical margin collapsing between adjacent siblings, parent-child margins, and empty blocks.
- **Percentages & Auto**: Correct circular percentage resolution for margins, padding, and min/max constraints against containing blocks.

### 2. CSS Flexible Box Layout (Level 1)
- **Flex Directions**: `row`, `row-reverse`, `column`, `column-reverse`.
- **Multi-Line Flex Wrapping**: `flex-wrap: wrap | wrap-reverse | nowrap` with cross-axis line distribution.
- **Main Axis Alignment**: `justify-content: flex-start | flex-end | center | space-between | space-around | space-evenly`.
- **Cross Axis Alignment**: `align-items` and `align-self: stretch | flex-start | flex-end | center | baseline`.
- **Flex Factors**: Accurate iterative distribution of positive flexibility (`flex-grow`) and negative flexibility (`flex-shrink`) with min/max constraint clamping.

### 3. CSS Grid Layout (Level 2)
- **Track Sizing**: Fixed pixel sizes (`px`), percentages (`%`), auto-sizing (`auto`), and flexible fractional units (`fr`).
- **Dynamic Track Sizing**: `minmax(min, max)`, `fit-content(limit)`.
- **Grid Placement**: Named grid lines, grid template areas (`grid-template-areas`), spanning (`span N`).
- **Auto Placement Algorithm**: Dense vs. sparse packing with automatic implicit grid generation.
- **Subgrid (Level 2)**: Inherited track definitions for nested child grids.

---

## Subpixel Coordinate Precision

Web browsers often suffer from 1-pixel rounding errors (visual seams, blurry text, or jitter during smooth animations) due to premature coordinate rounding.

Strata computes all layout positions and dimensions using 32-bit floating-point numbers (`f32`):
1. **Layout Space**: Measured in abstract CSS Points (1/96th inch).
2. **Device Scale Factor (DPI)**: Layout coordinates are multiplied by the physical display scale factor (e.g., 2.0x for Retina, 1.5x for Windows 4K).
3. **Hardware Quantization**: Rasterization boundaries are snapped to physical device subpixels during the display list generation pass, guaranteeing crisp hairline borders and zero subpixel text blurring.

---

## High-Throughput Text Shaping

Text measurement is integrated directly into the layout pass using HarfBuzz-grade shaping algorithms:
- **Kerning & Ligatures**: Advanced OpenType glyph substitution and positioning.
- **Multi-Line Breaking**: Unicode Standard Annex #14 line breaking algorithm.
- **Ellipsis & Truncation**: Configurable text truncation (`text-overflow: ellipsis`) with zero layout reflows.
- **Baseline Alignment**: Inline flex and grid items align accurately along their typographic baselines across mixed font families and sizes.
