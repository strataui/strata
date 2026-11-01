# strata_a11y: Native Accessibility Bridge & WCAG 2.1 AAA Compliance Auditor

The `strata_a11y` crate provides native operating system accessibility integration and automated WCAG 2.1 AAA compliance auditing for the Strata framework. Connecting directly with AccessKit, `strata_a11y` translates Strata's `UiIrNode` scene graph into OS accessibility trees across macOS (VoiceOver / NSAccessibility), Windows (Narrator / UI Automation), and Linux (Orca / AT-SPI2). Furthermore, it embeds an automated compile-time and runtime accessibility auditor enforcing accessible names, image alt text, touch target boundaries, and mathematical relative luminance contrast ratios.

---

## Architectural Principles

Accessibility in Strata is treated as a first-class rendering pipeline target parallel to the GPU presentation pipeline:

```text
┌─────────────────────────────────────────────────────────────┐
│                    Strata Component Tree                    │
│             (Components, RSX, Attributes, Bounds)           │
└──────────────────────────────┬──────────────────────────────┘
                               │
                ┌──────────────┴──────────────┐
                ▼                             ▼
┌─────────────────────────────┐ ┌─────────────────────────────┐
│    GPU Rendering Pipeline   │ │     strata_a11y Bridge      │
│  (Display lists, shaders)   │ │  (A11yTree & AccessKit)     │
└─────────────────────────────┘ └─────────────┬───────────────┘
                                              │
                               ▼──────────────┴──────────────▼
                ┌─────────────────────────────┐ ┌─────────────────────────────┐
                │   Differential Tree Updates │ │  WCAG 2.1 AAA Audit Engine  │
                │  (Only mutated nodes pushed)│ │(Luminance, touch target, ...)│
                └─────────────┬───────────────┘ └─────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                 Native OS Assistive APIs                    │
│      macOS NSAccessibility • Windows UIA • Linux AT-SPI     │
└─────────────────────────────────────────────────────────────┘
```

1. **Differential Tree Serialization**: The `A11yBridge` tracks node generations, emitting minimal delta updates (`A11yTreeUpdate`) containing only mutated nodes and pruned IDs to reduce IPC overhead.
2. **Bidirectional Semantic Action Dispatch**: OS screen-reader actions (`A11yAction::Click`, `SetValue`, `Increment`, `Focus`) are translated into internal Strata synthetic events without pointer emulation.
3. **Automated WCAG 2.1 AAA Verification**: Mathematical validation runs continuously in test suites and development builds, flagging contrast regressions or missing accessible labels before shipping to production.

---

## Semantic Roles: `A11yRole`

`A11yRole` maps both standard HTML tags and explicit ARIA role attributes to canonical assistive technology roles:

```rust
use strata_a11y::A11yRole;

// Automatic tag resolution
assert_eq!(A11yRole::from_tag_or_role("button", None), A11yRole::Button);
assert_eq!(A11yRole::from_tag_or_role("input", None), A11yRole::TextInput);

// Explicit ARIA role override
assert_eq!(A11yRole::from_tag_or_role("div", Some("button")), A11yRole::Button);
assert_eq!(A11yRole::from_tag_or_role("span", Some("slider")), A11yRole::Slider);
assert_eq!(A11yRole::from_tag_or_role("div", Some("dialog")), A11yRole::Dialog);
```

#### Core Semantic Roles

- `A11yRole::RootWebArea`: Document and window root container.
- `A11yRole::Button`: Action triggers and toggle buttons.
- `A11yRole::TextInput`: Single-line or multiline text entry controls.
- `A11yRole::Checkbox`: Binary or tri-state toggle selections.
- `A11yRole::Slider`: Continuous or discrete numeric range adjustments.
- `A11yRole::Dialog`: Modal overlay panels and alerts.
- `A11yRole::Heading`: Structured outline landmarks.
- `A11yRole::Image`: Graphical assets requiring textual alternatives.
- `A11yRole::Navigation`: Landmark containing primary site or app links.
- `A11yRole::Table` / `Row` / `Cell`: Tabular data structures.

---

## Accessibility Node and Tree: `A11yNode` and `A11yTree`

Nodes capture spatial boundaries, descriptive text, state flags, and supported interactive actions:

```rust
use strata_a11y::{A11yAction, A11yNode, A11yNodeId, A11yRect, A11yRole, A11yTree};

// Root container node
let root_node = A11yNode::new(A11yNodeId(1), A11yRole::RootWebArea);
let mut tree = A11yTree::new(A11yNodeId(1), root_node);

// Interactive button node
let mut btn = A11yNode::new(A11yNodeId(2), A11yRole::Button)
    .with_name("Drain Ingress Gateway")
    .with_description("Evicts active proxy sessions and diverts traffic to standby nodes");

btn.bounds = Some(A11yRect::new(20.0, 40.0, 160.0, 44.0));
btn.focusable = true;
btn.supported_actions = vec![A11yAction::Click, A11yAction::Focus];

// Register into accessibility tree
tree.insert_node(btn);
tree.get_node_mut(A11yNodeId(1)).unwrap().add_child(A11yNodeId(2));
```

#### Node Properties Specification

| Property | Type | Description |
| :--- | :--- | :--- |
| `id` | `A11yNodeId` | Unique 64-bit identifier corresponding to the layout node. |
| `role` | `A11yRole` | Accessibility semantic role. |
| `bounds` | `Option<A11yRect>` | Physical screen coordinates `(x, y, width, height)`. |
| `name` | `Option<String>` | Primary accessible label read by screen readers. |
| `description` | `Option<String>` | Extended tooltip or helper text read during accessibility inspection. |
| `value` | `Option<String>` | Textual representation of current value (e.g. for text fields). |
| `numeric_value` | `Option<f64>` | Quantitative position for sliders and progress bars. |
| `disabled` | `bool` | Informs screen reader that the control cannot be activated. |
| `expanded` | `Option<bool>` | Accordion and dropdown disclosure state. |
| `selected` | `bool` | Tab or table row selection status. |
| `supported_actions` | `Vec<A11yAction>` | List of operations the node accepts from assistive technologies. |

---

## Differential Updates: `A11yBridge`

The `A11yBridge` maintains prior tree topology to compute precise differential updates, avoiding costly full-tree reserialization across IPC boundaries:

```rust
use strata_a11y::{A11yBridge, A11yNode, A11yNodeId, A11yRole, A11yTree};

let mut bridge = A11yBridge::new();

// Initial frame tree submission (sends all 2 nodes)
let mut tree1 = A11yTree::new(A11yNodeId(1), A11yNode::new(A11yNodeId(1), A11yRole::RootWebArea));
tree1.insert_node(A11yNode::new(A11yNodeId(2), A11yRole::Button).with_name("Deploy Canary"));
tree1.get_node_mut(A11yNodeId(1)).unwrap().add_child(A11yNodeId(2));

let update1 = bridge.update(tree1);
assert_eq!(update1.nodes.len(), 2); // Initial full push

// Subsequent frame: Button label updates to "Deploying..." (Root is unchanged)
let mut tree2 = A11yTree::new(A11yNodeId(1), A11yNode::new(A11yNodeId(1), A11yRole::RootWebArea));
tree2.insert_node(A11yNode::new(A11yNodeId(2), A11yRole::Button).with_name("Deploying..."));
tree2.get_node_mut(A11yNodeId(1)).unwrap().add_child(A11yNodeId(2));

let update2 = bridge.update(tree2);
assert_eq!(update2.nodes.len(), 1); // Only mutated Button is transmitted
assert_eq!(update2.nodes[0].1.name.as_deref(), Some("Deploying..."));
```

---

## WCAG 2.1 AAA Contrast Calculations

Relative luminance and contrast calculations follow the official W3C formulas precisely:

$$L = 0.2126 \cdot R_{\text{lin}} + 0.7152 \cdot G_{\text{lin}} + 0.0722 \cdot B_{\text{lin}}$$

$$\text{Contrast Ratio} = \frac{L_{\text{lighter}} + 0.05}{L_{\text{darker}} + 0.05}$$

```rust
use strata_a11y::{
    calculate_contrast_ratio, evaluate_normal_text_contrast,
    relative_luminance, WcagCompliance,
};

// Pure white (255, 255, 255) on pure black (0, 0, 0)
let max_contrast = calculate_contrast_ratio((255, 255, 255), (0, 0, 0));
assert!((max_contrast - 21.0).abs() < 0.1);
assert_eq!(evaluate_normal_text_contrast(max_contrast), WcagCompliance::Aaa);

// Strata dark theme: Sky Cyan (#38bdf8) on Void Black (#08090a)
let strata_contrast = calculate_contrast_ratio((56, 189, 248), (8, 9, 10));
assert!(strata_contrast >= 7.0); // 10.4:1 ratio
assert_eq!(evaluate_normal_text_contrast(strata_contrast), WcagCompliance::Aaa);

// Low contrast gray failing WCAG (< 4.5:1)
let failing_contrast = calculate_contrast_ratio((119, 119, 119), (136, 136, 136));
assert_eq!(evaluate_normal_text_contrast(failing_contrast), WcagCompliance::Fail);
```

#### Contrast Compliance Standards

| Text Type | WCAG AA Requirement | WCAG AAA Requirement |
| :--- | :--- | :--- |
| **Normal Body Text** (< 18pt or < 14pt bold) | $\ge 4.5 : 1$ | $\ge 7.0 : 1$ |
| **Large Text** ($\ge 18\text{pt}$ or $\ge 14\text{pt}$ bold) | $\ge 3.0 : 1$ | $\ge 4.5 : 1$ |
| **UI Components & Graphical Objects** | $\ge 3.0 : 1$ | $\ge 4.5 : 1$ |

---

## Automated Compliance Auditor: `audit_tree`

The compliance auditor inspects tree hierarchies for structural accessibility defects:

```rust
use strata_a11y::{audit_tree, A11yNode, A11yNodeId, A11yRole, A11yTree};

let mut tree = A11yTree::new(A11yNodeId(1), A11yNode::new(A11yNodeId(1), A11yRole::RootWebArea));

// 1. Button missing accessible name (Violates RULE 001)
let unnamed_button = A11yNode::new(A11yNodeId(2), A11yRole::Button);

// 2. Image missing alt text (Violates RULE 002)
let unlabelled_image = A11yNode::new(A11yNodeId(3), A11yRole::Image);

// 3. Slider missing bounds (Violates RULE 004)
let broken_slider = A11yNode::new(A11yNodeId(4), A11yRole::Slider).with_name("CPU Target");

tree.insert_node(unnamed_button);
tree.insert_node(unlabelled_image);
tree.insert_node(broken_slider);

let violations = audit_tree(&tree);
assert_eq!(violations.len(), 3);

for violation in violations {
    println!("Violation [{}]: {}", violation.rule, violation.message);
}
```

#### Standard Audit Rules

| Rule Identifier | Severity | Description |
| :--- | :--- | :--- |
| `A11Y_RULE_001_MISSING_ACCESSIBLE_NAME` | `Error` | Interactive elements (buttons, inputs, links) must have non-empty `name` labels. |
| `A11Y_RULE_002_IMAGE_MISSING_ALT` | `Error` | Graphic nodes must specify an accessible name alternative. |
| `A11Y_RULE_003_TOUCH_TARGET_TOO_SMALL` | `Warning` | Interactive touch targets must meet the minimum $44 \times 44\text{px}$ physical size. |
| `A11Y_RULE_004_SLIDER_MISSING_BOUNDS` | `Error` | Slider and progress controls must define valid `min_numeric_value` and `max_numeric_value`. |
