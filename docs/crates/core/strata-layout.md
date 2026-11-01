# Crate `strata_layout`: Layout Engine, Typography & Sizing Solvers

The `strata_layout` crate delivers a high-precision, specification-compliant layout engine built with Chromium Blink LayoutNG architectural parity. It features multi-pass solvers for W3C Flexible Box Layout Level 1, CSS Grid Layout Level 2, Intrinsic Min/Max Sizing (`ComputeMinMaxSizes`), OpenType glyph shaping with code ligatures, and Unicode Bidirectional Algorithm (UAX#9) text reordering.

---

## Architectural Systems

```text
┌────────────────────────────────────────────────────────────────────────┐
│                             strata_layout                              │
└───────┬──────────────┬──────────────┬──────────────┬─────────────┬─────┘
        │              │              │              │             │
        ▼              ▼              ▼              ▼             ▼
┌──────────────┐┌──────────────┐┌──────────────┐┌─────────────┐┌─────────┐
│ flexbox.rs   ││   grid.rs    ││ intrinsic.rs ││ typography/ ││box_model│
│ Flexbox L1   ││ CSS Grid L2  ││ MinMaxSizes  ││ HarfBuzz-   ││Margin   │
│ Multi-Pass   ││ Occupancy    ││ ShrinkToFit  ││  tier Shaper││Collapse │
│ Line Solver  ││ Tracks (Fr)  ││ LayoutNG     ││ BiDi UAX#9  ││Geometry │
└──────────────┘└──────────────┘└──────────────┘└─────────────┘└─────────┘
```

---

## The Box Model & Geometry (`box_model`, `geometry`)

The layout engine resolves nested geometries into four concentric rectangular bounding boxes per the CSS Box Model specification:

```text
┌──────────────────────── Margin Box ────────────────────────┐
│  Margin Top                                                │
│  ┌───────────────────── Border Box ─────────────────────┐  │
│  │  Border Top                                          │  │
│  │  ┌────────────────── Padding Box ─────────────────┐  │  │
│  │  │  Padding Top                                   │  │  │
│  │  │  ┌─────────────── Content Box ──────────────┐  │  │  │
│  │  │  │                                          │  │  │  │
│  │  │  │           Width x Height                 │  │  │  │
│  │  │  │                                          │  │  │  │
│  │  │  └──────────────────────────────────────────┘  │  │  │
│  │  │  Padding Bottom                                │  │  │
│  │  └────────────────────────────────────────────────┘  │  │
│  │  Border Bottom                                       │  │
│  └──────────────────────────────────────────────────────┘  │
│  Margin Bottom                                             │
└────────────────────────────────────────────────────────────┘
```

### `LayoutGeometry`
Stores resolved floating-point boundaries:
```rust
pub struct LayoutGeometry {
    pub content_box: Rect,
    pub padding_box: Rect,
    pub border_box: Rect,
    pub margin_box: Rect,
}
```

### Dimension Resolution & Clamping
Converts generic CSS dimensions (`Px`, `Percent`, `Auto`, `Calc`) into concrete pixel measurements:
```rust
pub fn resolve_dimension(dim: Dimension, available_space: f32) -> Option<f32>;
pub fn clamp_dimension(val: f32, min: Dimension, max: Dimension, available_space: f32) -> f32;
```

### Margin Collapsing (`collapse_margins`)
Adjacent in-flow block elements collapse their vertical margins according to W3C rules:
- Two positive margins collapse into the maximum of the two: $\max(m_1, m_2)$.
- Positive and negative margins sum algebraically: $m_{\text{pos}} + m_{\text{neg}}$.
- Two negative margins collapse into the most negative value: $\min(m_1, m_2)$.

---

## W3C Flexbox Level 1 Solver (`flexbox`)

`strata_layout::flexbox::solve_flexbox` implements the complete W3C Flexible Box Layout Level 1 multi-pass specification.

### Flexbox Execution Stages

```text
 1. Container Axis Determination (Row vs. Column, Main & Cross Bounds)
                            │
                            ▼
 2. Hypothetical Sizing (Resolve flex-basis, intrinsic min/max clamps)
                            │
                            ▼
 3. Line Formation (Break items into FlexLines if flex-wrap is enabled)
                            │
                            ▼
 4. Main Axis Flex Resolution:
    - Compute Remaining Free Space (container_main - sum(items) - gaps)
    - If positive: Distribute among items proportional to flex-grow
    - If negative: Shrink items proportional to (flex-shrink * flex-basis)
                            │
                            ▼
 5. Cross Axis Alignment (align-items, align-self, stretch cross sizes)
                            │
                            ▼
 6. Absolute Positioning & Translation
```

### Key Structs
```rust
struct FlexItemMeta {
    index: usize,
    hypothetical_main: f32,
    hypothetical_cross: f32,
    resolved_main: f32,
    resolved_cross: f32,
    flex_grow: f32,
    flex_shrink: f32,
    align_self: AlignSelf,
    margin_main_start: f32,
    margin_main_end: f32,
    margin_cross_start: f32,
    margin_cross_end: f32,
}

struct FlexLine {
    items: Vec<FlexItemMeta>,
    cross_size: f32,
}
```

---

## W3C CSS Grid Layout Level 2 Solver (`grid`)

`strata_layout::grid::solve_grid` implements two-dimensional grid track layout and automatic placement.

### Track Resolution Algorithm
Columns and rows are specified using `GridTrack` tokens:
- `GridTrack::Px(val)`: Fixed pixel dimension.
- `GridTrack::Percent(pct)`: Relative proportion of inner container space.
- `GridTrack::Fr(fraction)`: Fractional flex unit.
- `GridTrack::Auto`: Content-based shrink-to-fit sizing.

Track sizing proceeds through two resolution passes:
1. **Fixed & Percent Allocations**: Subtracts concrete sizes and gaps from available space to calculate remaining unallocated space.
2. **Fractional Track Division**: Sums total `Fr` units and distributes remaining space proportionally ($W_{\text{track}} = \text{Remaining} \cdot \frac{\text{track\_fr}}{\sum \text{Fr}}$).

### Grid Auto-Placement with 2D Occupancy Matrix
When child items omit explicit `grid_column` or `grid_row` coordinates, the auto-placement engine maintains a two-dimensional boolean matrix `grid_occupancy[row][col]`:

```rust
// Auto-placement iterates row-by-row and column-by-column
let span_col = child.style.grid_column_span.unwrap_or(1);
let span_row = child.style.grid_row_span.unwrap_or(1);

// Finds the first available rectangular sub-matrix of unoccupied cells
// Marks cells true and positions item geometry accordingly
```

---

## Intrinsic Sizing & LayoutNG Parity (`intrinsic`)

Following Chromium Blink's LayoutNG architecture (`third_party/blink/renderer/core/layout/min_max_sizes.h`), `strata_layout::intrinsic` calculates intrinsic dimension bounds prior to structural layout passes.

```rust
pub struct MinMaxSizes {
    pub min_content: f32,
    pub max_content: f32,
}
```

- **`min_content`**: The narrowest inline size an element can accept without incurring unhandled text overflow (e.g. width of the longest unbreakable word).
- **`max_content`**: The ideal inline size assuming unconstrained space without soft line wrapping.

### CSS Shrink-to-Fit Formula
Computes the exact inline size for `inline-block`, floating, and fit-content elements:

$$\text{ShrinkToFit}(\text{Available}) = \min(\text{max\_content}, \max(\text{min\_content}, \text{Available}))$$

```rust
use strata_layout::intrinsic::MinMaxSizes;

let sizes = MinMaxSizes::new(120.0, 480.0);

// With ample 800px space, element takes ideal max-content:
assert_eq!(sizes.shrink_to_fit(800.0), 480.0);

// In constrained 300px space, element shrinks to fit available space:
assert_eq!(sizes.shrink_to_fit(300.0), 300.0);

// In severe 80px space, element clamps at min-content:
assert_eq!(sizes.shrink_to_fit(80.0), 120.0);
```

---

## Typography, Shaping & BiDi Subsystems (`typography`)

`strata_layout::typography` provides a pure-Rust OpenType text layout pipeline operating with HarfBuzz-tier behavioral parity.

### 1. OpenType Glyph Shaping (`TextShaper`)
Transforms UTF-8 character strings into positioned glyphs with subpixel advances:
- **GSUB Code Ligatures**: Replaces code symbol clusters (`===`, `!==`, `==`, `!=`, `<=`, `>=`, `=>`, `->`, `<-`, `<!--`, `-->`, `::`) with single ligature glyphs.
- **Typographic Ligatures**: Resolves Latin standard typographic ligatures (`fi`, `fl`, `ff`, `ffi`, `ffl`).
- **GPOS Pair Kerning**: Adjusts spacing between optical kerning pairs (`AV`, `VA`, `AW`, `AT`, `To`, `We`, `Yo`, etc.).

```rust
pub struct ShapedGlyph {
    pub glyph_id: u32,
    pub cluster: u32,
    pub source_bytes: u16,
    pub x_advance: LayoutUnit,
    pub y_advance: LayoutUnit,
    pub x_offset: LayoutUnit,
    pub y_offset: LayoutUnit,
    pub is_ligature: bool,
    pub is_space: bool,
}
```

### 2. Unicode Bidirectional Algorithm UAX#9 (`bidi`)
Full implementation of Unicode Standard Annex #9 for rendering mixed Left-to-Right (Latin/Cyrillic) and Right-to-Left (Arabic/Hebrew) scripts.
- **Classification (`BidiClass`)**: Maps unicode characters to types: `L`, `R`, `AL`, `EN`, `ES`, `ET`, `AN`, `CS`, `NSM`, `BN`, `B`, `S`, `WS`, `ON`.
- **Paragraph Direction Heuristic**: Detects primary direction from initial strong directional codepoints.
- **Visual Run Reordering (`BidiRun`)**: Segments text into runs of uniform embedding levels and reorders them visually prior to line breaking.

### 3. Inline Formatting Context (`InlineFormattingContext`)
Computes line boxes, soft wrapping (`LineBreaker`), and CSS half-leading adjustments based on font ascent and descent metrics:

$$\text{Half-Leading} = \frac{\text{Line Height} - (\text{Ascent} + \text{Descent})}{2}$$

---

## Production Cloud Infrastructure Example: Multi-Rack Cluster Explorer

The following production example constructs a multi-column server rack topology explorer utilizing CSS Grid, Flexbox, and text shaping:

```rust
use strata_layout::prelude::*;
use strata_style::types::*;

pub fn build_cluster_rack_layout(rack_id: &str, node_names: &[&str]) -> LayoutNode {
    // 1. Root Container (Grid with 3 columns)
    let mut root_style = ComputedStyle::default();
    root_style.display = Display::Grid;
    root_style.grid_template_columns = vec![
        GridTrack::Fr(1.0),
        GridTrack::Fr(1.0),
        GridTrack::Fr(1.0),
    ];
    root_style.gap = 16.0;
    root_style.padding = EdgeInsets::all(20.0);
    root_style.width = Dimension::Px(1200.0);

    let mut children = Vec::new();

    for (i, name) in node_names.iter().enumerate() {
        // 2. Rack Item Card (Flexbox Column)
        let mut card_style = ComputedStyle::default();
        card_style.display = Display::Flex;
        card_style.flex_direction = FlexDirection::Column;
        card_style.padding = EdgeInsets::all(12.0);
        card_style.gap = 8.0;

        // Code ligature telemetry string e.g. "STATUS: OK => PORT::443"
        let status_text = format!("NODE-0{}: ACTIVE => PORT::{}", i + 1, 8080 + i);

        let title_node = LayoutNode::with_text(ComputedStyle::default(), name.to_string());
        let status_node = LayoutNode::with_text(ComputedStyle::default(), status_text);

        let card_node = LayoutNode::with_children(card_style, vec![title_node, status_node]);
        children.push(card_node);
    }

    let mut root_node = LayoutNode::with_children(root_style, children);

    // 3. Compute layout across 1200px viewport
    root_node.compute_layout(1200.0, 800.0);
    root_node
}
```
