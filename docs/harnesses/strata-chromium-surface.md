# strata_chromium_surface: Isolated On-Demand Chromium Subprocess Surface & <WebView />

The `strata_chromium_surface` crate provides an isolated, on-demand Chromium worker subprocess harness and reactive `<WebView />` component for Project Strata.

While Strata executes its core user interface, CSS layout, and GPU rendering in pure, zero-Chromium Rust, real-world desktop applications occasionally require embedding legacy web pages, third-party authentication OAuth flows, payment gateways, or web extensions.

`strata_chromium_surface` fulfills this requirement with a zero-cost memory guarantee: the Chromium subprocess is spawned lazily **only** when a `<WebView />` mounts in the active component tree, and terminates immediately upon unmount, restoring Strata's sub-3MB resident memory baseline.

---

## Architectural Topology

```text
┌────────────────────────────────────────────────────────────────────────┐
│                        Strata Native Process                           │
├────────────────────────────────────────────────────────────────────────┤
│                                                                        │
│   ┌────────────────────────────────────────────────────────────────┐   │
│   │                      Component Tree (RSX)                      │   │
│   │              <WebView initial_url="https://..." />             │   │
│   └───────────────────────────────┬────────────────────────────────┘   │
│                                   │ mounts / unmounts                  │
│                                   ▼                                    │
│   ┌────────────────────────────────────────────────────────────────┐   │
│   │                    WebViewController Handle                    │   │
│   └───────────────┬────────────────────────────────┬───────────────┘   │
│                   │                                │                   │
│   ┌───────────────▼────────────────┐   ┌───────────▼───────────────┐   │
│   │     ChromiumProcessManager     │   │      ShmSurface (RAM)     │   │
│   │  (Lazy Spawn, Watchdog, Drop)  │   │  (Zero-Copy Double Buffer)│   │
│   └───────────────┬────────────────┘   └───────────▲───────────────┘   │
│                   │ Bidirectional                  │                   │
│                   │ Unix Sockets / Pipes           │ DMA / POSIX mmap  │
│                   ▼                                │                   │
├───────────────────┼────────────────────────────────┼───────────────────┤
│                   ▼                                │                   │
│   ┌────────────────────────────────────────────────┴───────────────┐   │
│   │                 Isolated Chromium Subprocess                   │   │
│   │          (Headless OSR: Blink + V8 + Software Raster)          │   │
│   └────────────────────────────────────────────────────────────────┘   │
│                                                                        │
└────────────────────────────────────────────────────────────────────────┘
```

---

## 1. Process Isolation & Lifecycle Watchdog

Unlike traditional Electron applications where Chromium and V8 are inseparable from the host process, Strata encapsulates Chromium within an external, sandboxed worker subprocess (`process.rs`):

```rust
pub enum ProcessState {
    Uninitialized,
    Spawning,
    Running { pid: u32 },
    Terminating,
    Terminated,
}
```

### Lazy Spawning Lifecycle
1. When `<WebView />` mounts, `ChromiumProcessManager::mount(initial_url)` transitions state from `Uninitialized` to `Spawning`.
2. The manager forks an isolated headless Chromium process using `--headless=new`, `--disable-gpu`, `--enable-surface-synchronization`, and `--disable-extensions`.
3. An anonymous POSIX shared memory file descriptor is passed to the child process via command-line flags (`--memfd-handle=<fd>`).
4. Once the child signals readiness over IPC, state transitions to `Running { pid }`.

### Automatic Teardown Invariant
To prevent zombie browser processes, `ChromiumProcessManager` implements the Rust `Drop` trait:

```rust
impl Drop for ChromiumProcessManager {
    fn drop(&mut self) {
        self.unmount();
    }
}
```

When the containing component unmounts or the application window closes:
1. The supervisor sends `ChromiumCommand::Terminate` to the child.
2. If the child process does not cleanly exit within a $500\text{ ms}$ timeout, the supervisor issues a `SIGKILL` signal.
3. The shared memory ring is unmapped, instantly freeing the 85+ MB memory allocation.

---

## 2. Zero-Copy Shared Memory Surface (`surface.rs`)

Rendering high-resolution video and complex web applications across process boundaries can incur massive memory copy overhead if sent over pipes or standard sockets. `strata_chromium_surface` utilizes POSIX shared memory (`shm_open`, `memfd_create`, and `mmap`) for zero-copy frame delivery:

```rust
pub enum PixelFormat {
    Rgba8,
    Bgra8,
}

pub struct ShmSurface {
    width: u32,
    height: u32,
    stride: usize,
    format: PixelFormat,
    buffer: Arc<RwLock<Vec<u8>>>,
    generation: Arc<AtomicU64>,
}
```

### Double-Buffering & Atomic Frame Synchronization
- The shared memory area is structured as a double-buffered ring (Front Buffer and Back Buffer).
- Chromium renders into the back buffer. Upon frame completion, it commits the buffer index and emits a `FrameDamage` event with dirty-rect coordinates.
- Strata's compositor swaps the buffer pointers atomically and samples the texture directly into its GPU rendering pass, avoiding CPU memory copies.

---

## 3. Bidirectional IPC & Event Routing (`ipc.rs`)

Communication between Strata and the Chromium subprocess is handled via a low-latency, binary-serialized IPC protocol:

### Host-to-Chromium Commands (`ChromiumCommand`)

Commands dispatched from the Strata runtime to the browser instance:

```rust
pub enum ChromiumCommand {
    Navigate { url: String },
    Resize { width: u32, height: u32 },
    MouseMove { x: f32, y: f32, buttons: u32 },
    MouseDown { x: f32, y: f32, button: MouseButton },
    MouseUp { x: f32, y: f32, button: MouseButton },
    MouseWheel { delta_x: f32, delta_y: f32 },
    KeyDown { key_code: u32, modifiers: Modifiers },
    KeyUp { key_code: u32, modifiers: Modifiers },
    ExecuteJavaScript { script: String },
    SetCookie { cookie: String },
    Terminate,
}
```

### Chromium-to-Host Events (`ChromiumEvent`)

Events emitted by the browser subprocess back to Strata:

```rust
pub enum ChromiumEvent {
    FrameDamaged { x: u32, y: u32, width: u32, height: u32, buffer_index: usize },
    NavigationStateChanged { state: NavigationState },
    TitleChanged { title: String },
    ConsoleMessage { level: String, message: String },
    JavaScriptResult { result: String },
    ProcessCrashed { exit_code: i32 },
}
```

---

## 4. The Reactive `<WebView />` Component (`webview.rs`)

Developers integrate web view surfaces using standard Strata RSX syntax:

```rust
use strata::prelude::*;
use strata_chromium_surface::prelude::*;

#[component]
pub fn CloudAuthDialog() -> View {
    let controller = WebViewController::new(
        WebViewConfig {
            initial_url: "https://auth.cloud-provider.internal/oauth/authorize".to_string(),
            enable_javascript: true,
            allow_cookies: true,
            ..Default::default()
        },
        800,
        600,
    );

    rsx! {
        <div class="flex flex-col w-[800px] h-[600px] rounded-xl overflow-hidden border border-neutral-800 bg-neutral-950">
            <div class="h-10 bg-neutral-900 border-b border-neutral-800 flex items-center px-4 justify-between">
                <span class="text-xs font-mono text-neutral-400">
                    {"Secure OAuth Provider Authentication"}
                </span>
            </div>

            <WebView
                controller={controller}
                class="flex-1 w-full h-full"
            />
        </div>
    }
}
```

### Programmatic Navigation and Script Injection

The `WebViewController` handle provides asynchronous methods to interact with the embedded browser:

```rust
// Programmatic URL Navigation
controller.navigate("https://console.cloud-provider.internal/metrics");

// Inject JavaScript and retrieve results
controller.execute_javascript("document.title", |result| {
    println!("Web page title: {}", result);
});

// Forward raw user input events
controller.forward_mouse_move(120.0, 45.0, 0);
controller.forward_mouse_down(120.0, 45.0, MouseButton::Left);
```

---

## Unit Testing & Verification

The crate includes tests validating lifecycle teardown invariants, IPC serialization roundtrips, and zero-copy surface blitting:

```bash
# Execute chromium surface integration and invariant tests
cargo test -p strata_chromium_surface
```
