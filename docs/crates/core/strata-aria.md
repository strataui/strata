# Crate `strata_aria`: Headless Accessibility & Collision Primitives

The `strata_aria` crate delivers zero-runtime-overhead headless UI primitives, accessible state machines, and viewport collision detection algorithms for Project Strata.

Modeled after modern headless design systems (Radix UI, React Aria, and Floating UI), `strata_aria` decouples behavior, semantic ARIA contracts, and keyboard navigation from visual styling. Developers retain absolute styling freedom while guaranteeing accessibility compliance out of the box.

---

## Architectural Systems

```text
┌────────────────────────────────────────────────────────────────────────┐
│                              strata_aria                               │
└───────┬──────────────┬──────────────┬──────────────┬─────────────┬─────┘
        │              │              │              │             │
        ▼              ▼              ▼              ▼             ▼
┌──────────────┐┌──────────────┐┌──────────────┐┌─────────────┐┌─────────┐
│  dialog.rs   ││ accordion.rs ││  tooltip.rs  ││focus_trap.rs││floating.│
│ Modal State  ││ Single/Multi ││ Delay Pacing ││ Tab Loop    ││Viewport │
│ Escape/Click ││ Disclosure   ││ DescribedBy  ││ Restore     ││Collision│
└──────────────┘└──────────────┘└──────────────┘└─────────────┘└─────────┘
```

---

## WAI-ARIA Semantic State Machines

### 1. Dialog & Modal State Machine (`dialog`)

Manages accessible dialogs, alerts, and drawer overlays conforming to W3C WAI-ARIA Authoring Practices:

```rust
pub struct DialogConfig {
    pub id: String,
    pub close_on_escape: bool,
    pub close_on_outside_click: bool,
}

pub struct DialogState {
    pub open: Signal<bool>,
    pub id: String,
    pub title_id: String,
    pub description_id: String,
    pub close_on_escape: bool,
    pub close_on_outside_click: bool,
}
```

#### Behavior & Interaction Contracts
- **Escape Key Handling**: When open, pressing `Escape` closes the dialog and returns `true`.
- **Outside Click Handling**: Clicking outside dialog boundaries closes the modal if `close_on_outside_click` is true.
- **ARIA Attribute Generation**:
  - `container_attrs()`:
    - `role="dialog"`
    - `aria-modal="true"`
    - `aria-labelledby="{id}-title"`
    - `aria-describedby="{id}-desc"`
  - `trigger_attrs()`:
    - `aria-haspopup="dialog"`
    - `aria-expanded="true|false"`
    - `aria-controls="{id}"`

```rust
use strata_aria::dialog::{use_dialog, DialogConfig};

let dialog = use_dialog(DialogConfig {
    id: "incident-remediation-modal".into(),
    close_on_escape: true,
    close_on_outside_click: true,
});

dialog.open_dialog();
assert!(dialog.open.get());

// Escape key triggers dismiss
let handled = dialog.handle_key("Escape");
assert!(handled);
assert!(!dialog.open.get());
```

---

### 2. Disclosure & Accordions (`accordion`)

Manages single or multi-panel expandable disclosure sections:

```rust
pub enum AccordionType {
    Single,
    Multiple,
}

pub struct AccordionState {
    pub accordion_type: AccordionType,
    pub collapsible: bool,
    pub value: Signal<Vec<String>>,
}
```

#### Disclosure Contracts
- **`AccordionType::Single`**: Expanding an item collapses any previously active item. If `collapsible: true`, clicking an already open panel collapses it.
- **`AccordionType::Multiple`**: Independent toggling of multiple disclosure panels simultaneously.
- **ARIA Semantics**:
  - `trigger_attrs(item, panel_id)`:
    - `role="button"`
    - `aria-expanded="true|false"`
    - `aria-controls="{panel_id}"`
  - `panel_attrs(item, trigger_id)`:
    - `role="region"`
    - `aria-labelledby="{trigger_id}"`
    - `aria-hidden="true|false"`

```rust
use strata_aria::accordion::{use_accordion, AccordionType};

let acc = use_accordion(AccordionType::Single, true, vec!["cluster-overview".into()]);
assert!(acc.is_expanded("cluster-overview"));

// Toggle second panel closes the first in Single mode
acc.toggle("network-ingress");
assert!(!acc.is_expanded("cluster-overview"));
assert!(acc.is_expanded("network-ingress"));
```

---

### 3. Tooltips (`tooltip`)

Coordinates hover and keyboard focus tooltips with configurable delay pacing:

```rust
pub struct TooltipConfig {
    pub id: String,
    pub delay_ms: u32,
}

pub struct TooltipState {
    pub is_open: Signal<bool>,
    pub id: String,
    pub delay_ms: u32,
}
```

- `trigger_attrs()`: Generates `aria-describedby="{id}"` and `data-state="open|closed"`.
- `tooltip_attrs()`: Generates `role="tooltip"`, `id="{id}"`, and `aria-hidden="true|false"`.

---

## Keyboard Navigation & Focus Management

### 1. Tab Focus Containment (`FocusTrap`)

When modal dialogs or alert overlays mount, keyboard focus must be trapped within the overlay so users cannot Tab out into the background document.

```rust
pub struct FocusTrap {
    pub active: Signal<bool>,
    elements: Vec<String>,
    current_index: Signal<usize>,
    previous_focused_id: Option<String>,
}
```

```text
 Forward Tab Cycle:
 ┌────────────────────────────────────────────────────────┐
 │ [First Input] ──► [Second Input] ──► [Submit Button]  │
 │       ▲                                      │         │
 │       └──────────────────────────────────────┘         │
 └────────────────────────────────────────────────────────┘
```

#### Focus Cycle Mechanics
- `activate(previously_focused)`: Stores the ID of the element that had focus before modal launch and activates containment.
- `deactivate()`: Returns `Option<String>` containing the previous element ID to restore keyboard focus back to the page.
- `handle_key("Tab", shift)`:
  - `Tab`: Advances forward, wrapping from the last item back to the first.
  - `Shift + Tab`: Moves backward, wrapping from the first item to the last.

```rust
use strata_aria::focus_trap::FocusTrap;

let mut trap = FocusTrap::new(vec!["action-select".into(), "confirm-btn".into(), "cancel-btn".into()]);
trap.activate(Some("open-incident-btn".into()));

assert_eq!(trap.current_target(), Some("action-select".into()));
let next = trap.handle_key("Tab", false);
assert_eq!(next, Some("confirm-btn".into()));

// Deactivation returns original trigger ID for restoration
let restore_id = trap.deactivate();
assert_eq!(restore_id, Some("open-incident-btn".into()));
```

---

### 2. Roving Tabindex (`RovingTabIndex`)

Implements the WAI-ARIA Roving Tabindex pattern for toolbars, menus, tab lists, and radio groups. Only one element within the widget maintains `tabindex="0"`, while all inactive siblings receive `tabindex="-1"`.

```rust
pub enum Orientation {
    Horizontal,
    Vertical,
    Both,
}

pub struct RovingTabIndex {
    pub orientation: Orientation,
    pub loop_navigation: bool,
    items: Vec<String>,
    disabled_indices: Vec<usize>,
    selected_index: Signal<usize>,
}
```

#### Keyboard Navigation Rules
- **Horizontal**: `ArrowRight` advances forward; `ArrowLeft` moves backward.
- **Vertical**: `ArrowDown` advances forward; `ArrowUp` moves backward.
- **Home / End**: `Home` selects first non-disabled item; `End` selects last non-disabled item.
- **Disabled Skipping**: Automatically skips indices marked as disabled via `set_disabled()`.

```rust
use strata_aria::roving_tabindex::{Orientation, RovingTabIndex};

let items = vec!["tab-cpu".into(), "tab-memory".into(), "tab-network".into()];
let mut roving = RovingTabIndex::new(items, Orientation::Horizontal);

assert_eq!(roving.tab_index_for(0), 0);
assert_eq!(roving.tab_index_for(1), -1);

// ArrowRight advances active tab
roving.handle_key("ArrowRight");
assert_eq!(roving.tab_index_for(0), -1);
assert_eq!(roving.tab_index_for(1), 0);
```

---

## Floating Element Positioning & Collision Solver (`floating`)

`strata_aria::floating` calculates subpixel coordinates for floating elements (tooltips, dropdowns, popovers) relative to anchor rectangles, preventing clipping against viewport bounds.

### Placement Geometry

Supports 12 distinct anchor orientations:
- `Top`, `TopStart`, `TopEnd`
- `Bottom`, `BottomStart`, `BottomEnd`
- `Left`, `LeftStart`, `LeftEnd`
- `Right`, `RightStart`, `RightEnd`

```rust
pub struct ComputePositionOptions {
    pub placement: Placement,
    pub offset: f32,
    pub flip: bool,
    pub shift: bool,
    pub viewport: FloatRect,
}
```

### Collision Resolution Algorithms

```text
 1. Initial Coordinate Placement (Anchor + Placement + Offset)
                            │
                            ▼
 2. Viewport Boundary Check (Does floating rect overflow viewport?)
                            │
                            ▼
 3. Flip Middleware (If flipped side fits better, reverse side: Top <-> Bottom)
                            │
                            ▼
 4. Shift Middleware (Shift along cross axis to clamp within viewport margins)
```

```rust
use strata_aria::floating::{compute_position, ComputePositionOptions, FloatRect, Placement};

let anchor = FloatRect::new(100.0, 10.0, 120.0, 40.0);    // Near top edge of screen
let floating = FloatRect::new(0.0, 0.0, 150.0, 60.0);
let viewport = FloatRect::new(0.0, 0.0, 1920.0, 1080.0);

let result = compute_position(
    anchor,
    floating,
    ComputePositionOptions {
        placement: Placement::Top, // Initial requested placement
        offset: 8.0,
        flip: true,                 // Automatically flips to Bottom because y=10 overflows top
        shift: true,
        viewport,
    },
);

assert_eq!(result.placement, Placement::Bottom);
assert_eq!(result.y, 58.0); // 10.0 + 40.0 (anchor height) + 8.0 (offset)
```

---

## Production Cloud Infrastructure Example: Security Incident Remediation Modal

The following example implements an accessible security incident remediation dialog featuring focus trapping, roving tabindex action lists, and collision-aware metric tooltips:

```rust
use strata_aria::prelude::*;

pub struct SecurityIncidentModal {
    pub dialog: DialogState,
    pub focus_trap: FocusTrap,
    pub action_roving: RovingTabIndex,
    pub tooltip: TooltipState,
}

impl SecurityIncidentModal {
    pub fn new() -> Self {
        let dialog = use_dialog(DialogConfig {
            id: "security-incident-alert".into(),
            close_on_escape: true,
            close_on_outside_click: true,
        });

        let focusable_ids = vec![
            "remediate-action-list".into(),
            "execute-mitigation-btn".into(),
            "dismiss-btn".into(),
        ];
        let focus_trap = FocusTrap::new(focusable_ids);

        let actions = vec![
            "isolate-pod-network".into(),
            "rotate-service-credentials".into(),
            "scale-down-deployment".into(),
        ];
        let action_roving = RovingTabIndex::new(actions, Orientation::Vertical);

        let tooltip = use_tooltip(TooltipConfig {
            id: "severity-score-tip".into(),
            delay_ms: 100,
        });

        Self {
            dialog,
            focus_trap,
            action_roving,
            tooltip,
        }
    }

    /// Triggers display of remediation dialog and arms keyboard focus trap
    pub fn open_incident(&mut self, trigger_button_id: &str) {
        self.dialog.open_dialog();
        self.focus_trap.activate(Some(trigger_button_id.to_string()));
    }

    /// Handles keyboard event routing across accessibility subsystems
    pub fn on_key_down(&mut self, key: &str, shift: bool) {
        // 1. Check escape dismiss
        if self.dialog.handle_key(key) {
            let restore_target = self.focus_trap.deactivate();
            println!("Dialog dismissed. Focus restored to: {:?}", restore_target);
            return;
        }

        // 2. Focus trap tab cycles
        if key == "Tab" {
            let next_focus = self.focus_trap.handle_key(key, shift);
            println!("Tab focus directed to: {:?}", next_focus);
            return;
        }

        // 3. Roving tabindex item selection
        if let Some(selected_index) = self.action_roving.handle_key(key) {
            println!("Remediation action selected: index {}", selected_index);
        }
    }
}
```
