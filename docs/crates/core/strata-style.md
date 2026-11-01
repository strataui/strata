# Crate `strata_style`: CSS Engine, Cascade Solver & Tailwind JIT

The `strata_style` crate provides a zero-dependency styling engine for Project Strata. It encompasses a CSS parser, a W3C-compliant specificity and cascade solver, a high-throughput Tailwind CSS v3/v4 JIT compiler, a mathematical `calc()` AST engine, and a cubic-bezier `@keyframes` animator.

---

## Architectural Systems

```text
┌────────────────────────────────────────────────────────────────────────┐
│                              strata_style                              │
└───────┬──────────────┬──────────────┬──────────────┬─────────────┬─────┘
        │              │              │              │             │
        ▼              ▼              ▼              ▼             ▼
┌──────────────┐┌──────────────┐┌──────────────┐┌─────────────┐┌─────────┐
│  parser.rs   ││specificity.rs││ tailwind.rs  ││  calc.rs    ││keyframe.│
│ CSS Lexer &  ││cascade.rs    ││ Tailwind JIT ││ calc() AST  ││Cubic-   │
│ Declarations ││ CascadeSolver││ Modifiers    ││ Evaluator   ││ Bezier  │
└──────────────┘└──────────────┘└──────────────┘└─────────────┘└─────────┘
```

---

## CSS Lexer & Stylesheet Parser (`parser`)

The CSS parser processes standard stylesheets, extracts selectors and declaration blocks, strips comments, and produces typed AST elements with zero external dependencies.

```rust
pub struct Declaration {
    pub property: String,
    pub value: String,
    pub important: bool,
}

pub struct StyleRule {
    pub selectors: Vec<Selector>,
    pub declarations: Vec<Declaration>,
}

pub struct StyleSheet {
    pub rules: Vec<StyleRule>,
}
```

### Parsing Pipeline
1. **Comment Stripping**: Removes standard block comments (`/* ... */`).
2. **Selector Tokenization**: Identifies tag, class (`.`), ID (`#`), and pseudo-class (`:hover`, `:focus`) tokens.
3. **Declaration Block Parsing**: Decodes key-value pairs, stripping whitespace and detecting `!important` markers.

```rust
use strata_style::parser::StyleSheet;

let css = r#"
    .telemetry-card {
        background-color: #18181b;
        padding: 16px;
        border-radius: 8px;
    }
    .telemetry-card:hover {
        background-color: #27272a !important;
    }
"#;

let stylesheet = StyleSheet::parse(css).expect("Valid CSS");
assert_eq!(stylesheet.rules.len(), 2);
```

---

## Specificity & Cascade Solver (`specificity`, `cascade`)

The cascade solver evaluates conflicting CSS declarations per the W3C Selectors Level 4 specification.

### The Specificity 4-Tuple

Specificity is represented as a lexicographically ordered 4-tuple:

$$\mathbf{S} = (I, A, B, C)$$

- $I$ (`inline`): Set to 1 for inline `style="..."` attributes, 0 otherwise.
- $A$ (`id_count`): Number of ID selectors (`#telemetry-header`).
- $B$ (`class_count`): Number of class selectors (`.active`), attribute selectors (`[data-mode]`), and pseudo-classes (`:hover`).
- $C$ (`type_count`): Number of tag selectors (`div`, `span`) and pseudo-elements.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
pub struct Specificity {
    pub inline: u8,
    pub id_count: u16,
    pub class_count: u16,
    pub type_count: u16,
}
```

Because `Specificity` derives `Ord`, comparing two rules follows CSS priority naturally:

$$(0, 1, 0, 0) > (0, 0, 12, 4) \quad \implies \text{ID beats 12 classes}$$

### Cascade Resolution Algorithm (`CascadeSolver::resolve`)

When determining the computed style of an element, `CascadeSolver` executes a multi-step resolution pipeline:

```text
 1. Base Initialization (Inherit from parent ComputedStyle or default)
                            │
                            ▼
 2. Rule Matching (Collect matching rules from stylesheets)
                            │
                            ▼
 3. Inline Styles (Inject inline style declarations with Specificity::INLINE)
                            │
                            ▼
 4. Priority Ordering:
    Sort candidates by: (!important, Specificity, Source Order)
                            │
                            ▼
 5. Property Application (Mutate target ComputedStyle in sorted order)
```

```rust
use strata_style::cascade::CascadeSolver;
use strata_style::selector::PseudoState;
use strata_style::types::ComputedStyle;

let parent_style = ComputedStyle::default();
let mut pseudo = PseudoState::default();
pseudo.is_hovered = true;

let computed = CascadeSolver::resolve(
    "div",
    &["telemetry-card"],
    Some("node-panel"),
    Some("border-width: 1px;"),
    &[&stylesheet],
    Some(&parent_style),
    &pseudo,
);
```

---

## Tailwind CSS v3/v4 JIT Engine (`tailwind`)

`TailwindEngine` provides zero-regex, fast compilation of utility classes directly into `ComputedStyle` properties.

### Supported Utility Categories

| Category | Tailwind Classes | Resolved `ComputedStyle` |
| :--- | :--- | :--- |
| **Display** | `flex`, `grid`, `block`, `inline-flex`, `hidden` | `Display::Flex`, `Display::Grid`, `Display::None` |
| **Flexbox** | `flex-row`, `flex-col`, `items-center`, `justify-between`, `gap-4` | `flex_direction`, `align_items`, `justify_content`, `gap` |
| **Grid** | `grid-cols-3`, `grid-rows-2`, `col-span-2` | `grid_template_columns`, `grid_column_span` |
| **Sizing** | `w-full`, `h-64`, `max-w-md`, `min-h-screen` | `Dimension::Percent(100.0)`, `Dimension::Px(256.0)` |
| **Colors** | `bg-neutral-900`, `text-cyan-400`, `border-emerald-500` | Hex lookup in pre-baked Tailwind palette |
| **Typography** | `text-xs`, `text-lg`, `font-mono`, `font-bold` | `font_size: 12.0`, `font_weight: 700` |
| **Borders** | `rounded-lg`, `border`, `border-2` | `border_radius: 8.0`, `border_width: 1.0` |

### Modifier Support (`resolve_classes_with_state`)
Modifiers conditionally activate utilities based on runtime element states:
- `hover:`: Active when cursor hovers over the element.
- `focus:`: Active when element possesses keyboard focus.
- `dark:`: Active when application theme is in dark mode.

```rust
use strata_style::tailwind::TailwindEngine;

let classes = "flex items-center p-4 bg-neutral-900 hover:bg-neutral-800 text-white";
let style = TailwindEngine::resolve_classes_with_state(
    classes,
    true,  // is_hovered
    false, // is_focused
    true,  // is_dark_mode
);

assert_eq!(style.padding.left, 16.0);
assert_eq!(style.padding.top, 16.0);
```

---

## Mathematical `calc()` AST Engine (`calc`)

CSS requires resolving complex dimensional expressions with mixed units, such as `calc(100% - 24px + 1.5rem)`. `strata_style::calc` compiles these into an expression AST and caches them via `CalcId`.

### Expression AST Representation

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum CalcExpr {
    Value(Dimension),
    Add(Box<CalcExpr>, Box<CalcExpr>),
    Sub(Box<CalcExpr>, Box<CalcExpr>),
    Mul(Box<CalcExpr>, f32),
    Div(Box<CalcExpr>, f32),
}
```

### Contextual Resolution (`CalcContext`)

Because percentages, `rem`, `em`, `vw`, and `vh` depend on external layout environments, evaluation takes a `CalcContext`:

```rust
pub struct CalcContext {
    pub parent_size: f32,
    pub root_font_size: f32,
    pub current_font_size: f32,
    pub viewport_width: f32,
    pub viewport_height: f32,
}
```

```rust
use strata_style::calc::{parse_calc, CalcContext};

let expr = parse_calc("100% - 32px").expect("Valid expression");
let ctx = CalcContext {
    parent_size: 1000.0,
    root_font_size: 16.0,
    current_font_size: 16.0,
    viewport_width: 1920.0,
    viewport_height: 1080.0,
};

let resolved_pixels = expr.eval(&ctx);
assert_eq!(resolved_pixels, 968.0); // 1000.0 - 32.0
```

---

## Declarative Keyframe Animations (`keyframes`)

The keyframes subsystem provides cubic-bezier timing curves and timeline property interpolation for smooth 60/120 FPS transitions.

### Newton-Raphson Cubic-Bezier Solver

Implements the standard parametric cubic-bezier timing function:

$$x(t) = 3(1-t)^2 t x_1 + 3(1-t) t^2 x_2 + t^3$$
$$y(t) = 3(1-t)^2 t y_1 + 3(1-t) t^2 y_2 + t^3$$

To solve for output $y$ given input time progress $x \in [0.0, 1.0]$, `CubicBezier::solve` executes Newton-Raphson iteration with a binary search bisection fallback for numerical stability:

```rust
pub struct CubicBezier {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
}
```

Standard easing curves are available as constants:
- `CubicBezier::linear()`: `(0.0, 0.0, 1.0, 1.0)`
- `CubicBezier::ease()`: `(0.25, 0.1, 0.25, 1.0)`
- `CubicBezier::ease_in()`: `(0.42, 0.0, 1.0, 1.0)`
- `CubicBezier::ease_out()`: `(0.0, 0.0, 0.58, 1.0)`
- `CubicBezier::ease_in_out()`: `(0.42, 0.0, 0.58, 1.0)`

### Animation Sampling

```rust
use strata_style::keyframes::{CubicBezier, Keyframe, KeyframeAnimation, KeyframeProperty};

let anim = KeyframeAnimation::new(
    "pulse-alert",
    1.0, // 1.0 second duration
    CubicBezier::ease_in_out(),
    vec![
        Keyframe::new(0.0, vec![KeyframeProperty::Opacity(1.0), KeyframeProperty::Scale(1.0)]),
        Keyframe::new(0.5, vec![KeyframeProperty::Opacity(0.4), KeyframeProperty::Scale(1.05)]),
        Keyframe::new(1.0, vec![KeyframeProperty::Opacity(1.0), KeyframeProperty::Scale(1.0)]),
    ],
);

// Sample properties at t = 0.5s
let props = anim.sample(0.5);
assert_eq!(props[0], KeyframeProperty::Opacity(0.4));
```

---

## Visual Effects (`effects`)

Provides modern compositing operations:
- `FilterOp`: `Blur(px)`, `Brightness(factor)`, `Contrast(factor)`, `Grayscale(amount)`.
- `BackdropFilter`: Applies real-time GPU frosted glass blur behind surfaces.
- `ClipPath`: Geometry mask clipping (`Circle`, `Inset`, `Polygon`).

---

## Production Cloud Infrastructure Example: Infrastructure Health Indicator

The following example combines Tailwind utility compilation, dynamic `calc()` width resolution, and keyframe property sampling to render an animated server status monitor:

```rust
use strata_style::prelude::*;

pub struct ServerStatusWidget {
    pub base_style: ComputedStyle,
    pub pulse_animation: KeyframeAnimation,
}

impl ServerStatusWidget {
    pub fn new() -> Self {
        // Resolve container style using Tailwind JIT
        let base_style = TailwindEngine::resolve_classes(
            "flex flex-col p-4 bg-neutral-900 border border-neutral-800 rounded-lg gap-3"
        );

        // Define pulse keyframes for heartbeat monitor
        let pulse_animation = KeyframeAnimation::new(
            "heartbeat",
            1.5,
            CubicBezier::ease_in_out(),
            vec![
                Keyframe::new(0.0, vec![KeyframeProperty::Opacity(1.0)]),
                Keyframe::new(0.5, vec![KeyframeProperty::Opacity(0.3)]),
                Keyframe::new(1.0, vec![KeyframeProperty::Opacity(1.0)]),
            ],
        );

        Self {
            base_style,
            pulse_animation,
        }
    }

    /// Evaluates dynamic bar width subtracting fixed 32px badge offset using calc()
    pub fn compute_memory_bar_width(&self, parent_width: f32, fill_ratio: f32) -> f32 {
        let calc_str = format!("{}% - 32px", fill_ratio * 100.0);
        let expr = parse_calc(&calc_str).unwrap_or(CalcExpr::Value(Dimension::Px(0.0)));
        let ctx = CalcContext {
            parent_size: parent_width,
            ..Default::default()
        };
        expr.eval(&ctx).max(0.0)
    }

    /// Samples live opacity for heartbeat indicator
    pub fn current_pulse_opacity(&self, elapsed_seconds: f32) -> f32 {
        let props = self.pulse_animation.sample(elapsed_seconds);
        props.first().map(|p| p.value()).unwrap_or(1.0)
    }
}
```
