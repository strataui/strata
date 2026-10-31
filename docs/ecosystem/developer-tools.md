# DevTools HUD & CDP Bridge

Strata embeds enterprise-grade diagnostic tooling directly into every debug build (`strata_devtools`). Developers can inspect UI hierarchies, track reactive signal dependencies, and profile GPU frame times without external browser extensions.

---

## In-Process DevTools HUD

Pressing `F12` or `Cmd+Shift+I` toggles the in-process **Strata HUD overlay**:

```text
┌─────────────────────────────────────────────────────────────┐
│ STRATA HUD // 119.8 FPS // 8.2ms Frame // 2.8MB RSS         │
├─────────────────────────────────────────────────────────────┤
│ [DOM Tree]      │ [Layout Box Model]    │ [Signal Graph]    │
│ ▶ div#root      │ Width:  1024.00 px    │ #1 count: 42      │
│   ▼ aside       │ Height:  768.00 px    │   Subscribers: 2  │
│     TabButton   │ Margin:    0.00 px    │   Last update: 1ms│
│     TabButton   │ Padding:  24.00 px    │                   │
│   ▶ main        │ Flex:     1 1 auto    │ #2 theme: "dark"  │
└─────────────────────────────────────────────────────────────┘
```

### Key HUD Capabilities
1. **Interactive Element Picker**: Click any UI element to inspect its computed CSS properties, resolved Tailwind utility bitmasks, and subpixel dimensions.
2. **Visual Margin & Padding Overlays**: Highlights margin (amber), border (yellow), padding (emerald), and content (blue) overlays directly on the running GPU canvas.
3. **Signal Dependency Graph**: Visualizes live signal subscribers and pinpoints unnecessary state invalidations.
4. **Frame Time Profiler**: Microsecond-resolution timeline tracking layout duration, parallel Rayon thread utilization, display list generation, and GPU submission times.

---

## Chrome DevTools Protocol (CDP) Bridge

Strata implements an embedded HTTP and WebSocket server compliant with the **Chrome DevTools Protocol (CDP)**:

When launching with `--devtools`:
```bash
strata dev --devtools
```

Strata opens a CDP endpoint on `http://127.0.0.1:9222`.

### Connecting Any Chromium Browser
1. Open Google Chrome, Brave, or Microsoft Edge.
2. Navigate to `chrome://inspect` in the URL bar.
3. Strata applications appear automatically under **Remote Target**.
4. Click **Inspect** to use the standard Elements panel, Console, Performance timeline, and Network monitor against your running native Rust binary.

---

## Enabling DevTools in Code

DevTools can be toggled programmatically in your application builder:

```rust
use strata::prelude::*;

fn main() -> Result<(), strata::Error> {
    strata::App::new()
        .title("Strata Application")
        .devtools(true)             // Enable HUD and CDP in debug builds
        .cdp_port(9222)              // Custom CDP port
        .root(App)
        .run()
}
```
