# strata_platform

`strata_platform` is the native Hardware Abstraction Layer (HAL) and windowing subsystem of the Strata framework. It provides zero-dependency, pure-Rust window creation, native event loops, client-side decorations (CSD), multi-window orchestration, native desktop application menus, and reactive system tray integrations across Linux, macOS, Windows, Android, iOS, and WebAssembly.

---

## Architectural Mission

Most cross-platform Rust GUI frameworks bind heavily to sprawling C libraries such as SDL2, GLFW, GTK, or winit. While functional, these dependencies introduce fragile C-toolchain compilation dependencies, platform inconsistencies, and unnecessary abstraction layers.

`strata_platform` communicates directly with operating system kernels and display servers via pure-Rust ABI definitions:
- **Linux (Wayland & X11)**: Implements native Wayland client wire protocol serialization (`wl_display`, `wl_registry`, `wl_compositor`, `xdg_wm_base`) over Unix domain sockets with zero reliance on `libwayland-client.so`.
- **macOS (Cocoa & Metal)**: Direct Objective-C runtime ABI interop (`objc_msgSend`, `NSWindow`, `NSApplication`, `CAMetalLayer`) bypassing heavy Swift/ObjC wrapper frameworks.
- **Windows (Win32 & Direct3D 12)**: Direct Win32 message dispatching (`PeekMessageW`, `DispatchMessageW`, `WNDCLASSEXW`) paired with DXGI 1.6 Flip Model presentation (`DXGI_SWAP_EFFECT_FLIP_DISCARD`).
- **Mobile (Android & iOS)**: Native surface binding for Android `ANativeWindow` and iOS `CAMetalLayer`.

---

## Subsystem Architecture

```text
┌─────────────────────────────────────────────────────────────────┐
│                     Application Event Loop                      │
│                (EventLoop, EventLoopControl)                    │
└────────────────────────────────┬────────────────────────────────┘
                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────────┐
│                     Multi-Window Manager                        │
│   - WindowManager & WindowId Allocation                         │
│   - Parent-Child Modal Hierarchy & Interaction Blocking         │
│   - Focus Arbitration & Event Multiplexing                      │
└────────────────┬───────────────────────────────┬────────────────┘
                 │                               │
                 ▼                               ▼
┌─────────────────────────────────┐   ┌───────────────────────────┐
│    Desktop OS Integrations      │   │   Surface & Presentation  │
│   - Native MenuBar & Submenus   │   │   - SurfaceConfig & Handle│
│   - Keyboard Accelerators       │   │   - FramePacer & Refresh  │
│   - Reactive System Tray Badges │   │   - PresentMode (Fifo,    │
│   - Client-Side Decorations     │   │     Mailbox, Immediate)   │
└────────────────┬────────────────┘   └─────────────┬─────────────┘
                 │                                  │
                 └────────────────┬─────────────────┘
                                  │
                                  ▼
┌─────────────────────────────────────────────────────────────────┐
│                    Native OS ABI Drivers                        │
│   - Linux: Wayland Wire Protocol (raw Unix socket) / X11        │
│   - macOS: Cocoa NSWindow / Metal Layer Runtime ABI             │
│   - Windows: Win32 MSG Dispatch / DXGI 1.6 Flip Model           │
│   - Headless: MockPlatformWindow & MockEventLoop for CI         │
└─────────────────────────────────────────────────────────────────┘
```

---

## Native Platform HAL Specifications

### 1. Linux: Direct Wayland Wire Protocol

`strata_platform::raw_abi::wayland` implements direct byte-level serialization of the Wayland IPC wire protocol:

```rust
use strata_platform::raw_abi::wayland::{WaylandHeader, default_socket_path, WL_DISPLAY_ID, WL_DISPLAY_GET_REGISTRY};

// Connects directly to $XDG_RUNTIME_DIR/$WAYLAND_DISPLAY (e.g. /run/user/1000/wayland-0)
let socket_path = default_socket_path();

// 8-byte Wayland wire protocol header: [object_id (u32), opcode (u16), size (u16)]
let header = WaylandHeader {
    object_id: WL_DISPLAY_ID,
    opcode: WL_DISPLAY_GET_REGISTRY,
    size: 12, // 8 bytes header + 4 bytes new_id argument
};

let wire_bytes = header.to_bytes();
assert_eq!(wire_bytes.len(), 8);
```

This bypasses dynamic library loading issues across glibc and musl distributions, ensuring identical behavior across Alpine Linux, Ubuntu, Arch, and Fedora.

### 2. macOS: Cocoa and Metal Runtime ABI

On macOS, `strata_platform::raw_abi::cocoa` defines the exact bitflags and memory layouts required by Apple's Objective-C runtime and AppKit framework:

```rust
use strata_platform::raw_abi::cocoa::{
    NsRect, NS_WINDOW_STYLE_TITLED, NS_WINDOW_STYLE_CLOSABLE,
    NS_WINDOW_STYLE_MINIATURIZABLE, NS_WINDOW_STYLE_RESIZABLE,
    NS_WINDOW_STYLE_FULL_SIZE_CONTENT, NS_BACKING_STORE_BUFFERED,
};

let frame = NsRect::new(100.0, 100.0, 1440.0, 900.0);
let style_mask = NS_WINDOW_STYLE_TITLED
    | NS_WINDOW_STYLE_CLOSABLE
    | NS_WINDOW_STYLE_MINIATURIZABLE
    | NS_WINDOW_STYLE_RESIZABLE
    | NS_WINDOW_STYLE_FULL_SIZE_CONTENT;
```

Full-size content views allow Strata's custom Client-Side Decorations (CSD) to blend into the macOS traffic-light window controls.

### 3. Windows: Win32 and DXGI 1.6 Flip Model

For Windows 10 and 11, `strata_platform::raw_abi::win32` implements non-blocking message processing and high-efficiency swapchain presentation:

```rust
use strata_platform::raw_abi::win32::{
    Msg, DXGI_SWAP_EFFECT_FLIP_DISCARD, DXGI_SWAP_CHAIN_FLAG_ALLOW_TEARING,
    WM_SIZE, WM_PAINT, WM_KEYDOWN, WM_DPICHANGED,
};

// DXGI 1.6 modern swap model configuration:
// Bypasses Desktop Window Manager (DWM) redirection surfaces for near-zero presentation latency
let swap_effect = DXGI_SWAP_EFFECT_FLIP_DISCARD;
let swap_flags = DXGI_SWAP_CHAIN_FLAG_ALLOW_TEARING;
```

---

## Multi-Window Desktop Manager

Modern cloud management interfaces require auxiliary windows for log streaming, telemetry dashboards, and modal confirmation dialogs. `WindowManager` provides thread-safe lifecycle management, modal parent-child relationships, and focused window arbitration.

### Window ID Allocation and Hierarchy

Every managed window receives a globally unique `WindowId(u64)`. Windows can be spawned as independent top-level windows or modal children:

```rust
use strata_platform::{WindowManager, WindowConfig, WindowMode, WindowDecoration};

let mut wm = WindowManager::new();

// 1. Spawn primary cluster overview dashboard
let main_config = WindowConfig {
    title: "Strata Cloud Infrastructure Monitor".to_string(),
    width: 1600,
    height: 900,
    mode: WindowMode::Windowed,
    decoration: WindowDecoration::ClientSide,
    resizable: true,
    transparent: false,
};
let main_id = wm.create_window(main_config);

// 2. Spawn auxiliary log stream viewer
let logs_config = WindowConfig {
    title: "Kubernetes Ingress Log Stream".to_string(),
    width: 1000,
    height: 600,
    mode: WindowMode::Windowed,
    decoration: WindowDecoration::System,
    resizable: true,
    transparent: false,
};
let logs_id = wm.create_window(logs_config);

assert_eq!(wm.window_count(), 2);
```

### Modal Dialog Arbitration

When a modal window is opened (such as a destructive cluster node drain confirmation), `WindowManager` registers parent-child linkage and blocks user interaction on the parent:

```rust
let modal_config = WindowConfig {
    title: "Confirm Node Eviction".to_string(),
    width: 500,
    height: 280,
    mode: WindowMode::Windowed,
    decoration: WindowDecoration::System,
    resizable: false,
    transparent: false,
};

// Spawn modal child attached to main_id
let modal_id = wm.create_modal(main_id, modal_config).unwrap();

// Verify that the parent window is now blocked from receiving focus
assert!(wm.is_modal_blocked(main_id));
assert_eq!(wm.active_window(), Some(modal_id));

// Attempting to set focus back to parent is safely rejected
let focus_result = wm.set_active_window(main_id);
assert!(focus_result.is_err());

// Closing modal unblocks the parent window immediately
wm.close_window(modal_id).unwrap();
assert!(!wm.is_modal_blocked(main_id));
assert_eq!(wm.active_window(), Some(main_id));
```

---

## Native Application Menus and Accelerators

`strata_platform::menu` defines hierarchical application menus with automatic OS role mappings and cross-platform keyboard shortcut formatting.

### Menu Structure and Roles

Standard menu items map directly to native operating system roles (`MenuItemRole::Quit`, `MenuItemRole::CloseWindow`, `MenuItemRole::ToggleFullScreen`):

```rust
use strata_platform::menu::{Menu, MenuBar, MenuItem, MenuItemRole};

let mut menu_bar = MenuBar::new();

// File Menu
let mut file_menu = Menu::new("File");
file_menu.add_item(MenuItem::item(101, "Connect to Kubernetes Cluster...").with_accelerator("CmdOrCtrl+O"));
file_menu.add_item(MenuItem::item(102, "Save Telemetry Snapshot").with_accelerator("CmdOrCtrl+S"));
file_menu.add_separator();
file_menu.add_item(MenuItem::role(MenuItemRole::CloseWindow).with_accelerator("CmdOrCtrl+W"));
file_menu.add_item(MenuItem::role(MenuItemRole::Quit).with_accelerator("CmdOrCtrl+Q"));
menu_bar.add_menu(file_menu);

// View Menu
let mut view_menu = Menu::new("View");
view_menu.add_item(MenuItem::item(201, "Reload Topology").with_accelerator("CmdOrCtrl+R"));
view_menu.add_item(MenuItem::role(MenuItemRole::ToggleFullScreen).with_accelerator("F11"));
menu_bar.add_menu(view_menu);
```

### Cross-Platform Accelerator Display Strings

`MenuAccelerator` parses unified trigger strings and formats them according to platform human interface guidelines:

```rust
use strata_platform::menu::MenuAccelerator;

let acc = MenuAccelerator::parse("CmdOrCtrl+Shift+P").unwrap();

// On macOS: Uses Unicode symbol standard
assert_eq!(acc.to_display_string(true), "⌘⇧P");

// On Windows / Linux: Uses modifier label standard
assert_eq!(acc.to_display_string(false), "Ctrl+Shift+P");
```

---

## Reactive System Tray and Status Badges

For background monitoring daemons and telemetry agents, `strata_platform::tray` provides system tray (status bar) integration with reactive notification badges.

```rust
use strata_platform::tray::{TrayIcon, TrayMenu, TrayMenuItem};

// 1. Initialize system tray icon with RGBA image buffer
let icon_rgba = vec![255; 32 * 32 * 4]; // 32x32 white icon
let mut tray = TrayIcon::new("Strata Cloud Telemetry", icon_rgba, 32, 32);

// 2. Attach context menu
let mut tray_menu = TrayMenu::new();
tray_menu.add_item(1, "Open Cluster Dashboard");
tray_menu.add_item(2, "Mute Critical Alerts");
tray_menu.add_separator();
tray_menu.add_item(3, "Terminate Agent");
tray.set_menu(Some(tray_menu));

// 3. Update reactive badge upon telemetry anomaly event
tray.set_badge_count(Some(3)); // Displays "3" unacknowledged alerts
assert_eq!(tray.badge(), Some("3"));

// 4. Simulate user menu interaction
let handled = tray.trigger_menu_action(1);
assert!(handled);
assert_eq!(tray.last_action_id, Some(1));
```

---

## Client-Side Decorations (CSD)

When operating in `WindowDecoration::ClientSide` mode, Strata bypasses standard OS window title bars and borders, rendering custom window controls directly inside the application canvas.

`CsdGeometry` calculates hit-testing zones for title drag bars, resize borders, and window buttons:

```rust
use strata_platform::csd::{CsdGeometry, CsdHitZone, ControlPlacement};

let csd = CsdGeometry {
    title_height: 38.0,
    border_width: 6.0,
    controls_placement: ControlPlacement::Right, // Right for Windows/Linux, Left for macOS
};

// Hit-testing window interactions
assert_eq!(csd.hit_test(100.0, 15.0, 1200.0, 800.0), CsdHitZone::TitleBar);
assert_eq!(csd.hit_test(2.0, 400.0, 1200.0, 800.0), CsdHitZone::ResizeBorderLeft);
assert_eq!(csd.hit_test(1185.0, 15.0, 1200.0, 800.0), CsdHitZone::CloseButton);
```

---

## Surface Presentation and Frame Pacing

`SurfacePresenter` defines the hardware contract for blitting rendered framebuffers to physical OS windows.

```rust
use strata_platform::surface::{
    FramePacer, PresentMode, RefreshRate, SurfaceConfig, SwapchainFormat,
};

// Configure presentation surface
let config = SurfaceConfig {
    width: 1920,
    height: 1080,
    scale_factor: 1.5, // 4K High-DPI scaling
    format: SwapchainFormat::Bgra8Unorm,
    present_mode: PresentMode::Fifo,
    refresh_rate: RefreshRate::Hz120,
};

assert_eq!(config.physical_size(), (2880, 1620));

// Lock frame pacer to 120 FPS deadline (8.333ms per frame)
let mut pacer = FramePacer::new(RefreshRate::Hz120);
let frame_start = pacer.begin_frame();

// ... rendering execution ...

let frame_duration = pacer.end_frame(frame_start);
let sleep_time = pacer.duration_until_next_deadline();
```

---

## Complete Working Rust Example: Enterprise Multi-Window Telemetry Hub

```rust
use strata_platform::{
    PlatformTarget, WindowConfig, WindowDecoration, WindowId, WindowManager, WindowMode,
};
use strata_platform::menu::{Menu, MenuBar, MenuItem, MenuItemRole};
use strata_platform::tray::{TrayIcon, TrayMenu};
use strata_platform::surface::{FramePacer, RefreshRate};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Operating System HAL Target: {:?}", PlatformTarget::current());

    // 1. Initialize Multi-Window Manager
    let mut wm = WindowManager::new();

    // 2. Create Main Telemetry Dashboard Window
    let main_window_id = wm.create_window(WindowConfig {
        title: "Strata Telemetry Center - Production US-East".to_string(),
        width: 1440,
        height: 900,
        mode: WindowMode::Windowed,
        decoration: WindowDecoration::ClientSide,
        resizable: true,
        transparent: false,
    });

    // 3. Configure Native Application Menu Bar
    let mut menu_bar = MenuBar::standard_app_menu("Strata Telemetry");
    let mut cluster_menu = Menu::new("Cluster");
    cluster_menu.add_item(MenuItem::item(2001, "Scale Replicas...").with_accelerator("CmdOrCtrl+K"));
    cluster_menu.add_item(MenuItem::item(2002, "Drain Selected Node"));
    menu_bar.add_menu(cluster_menu);

    // 4. Configure System Tray Status Monitor
    let tray_icon_bytes = vec![30; 32 * 32 * 4];
    let mut tray = TrayIcon::new("Strata Cluster Monitor", tray_icon_bytes, 32, 32);
    let mut tray_menu = TrayMenu::new();
    tray_menu.add_item(10, "Focus Telemetry Center");
    tray_menu.add_item(11, "Silence Alerts");
    tray.set_menu(Some(tray_menu));
    tray.set_badge_count(Some(1)); // 1 active node warning

    // 5. Initialize Hardware Frame Pacer at 60 Hz
    let mut pacer = FramePacer::new(RefreshRate::Hz60);

    // 6. Simulate Event Loop Tick
    let tick_start = pacer.begin_frame();
    let events = wm.poll_all_events();
    println!("Polled {} platform events across all active windows.", events.len());

    let frame_dur = pacer.end_frame(tick_start);
    println!(
        "Frame completed in {:?}. Time to next VSync: {:?}",
        frame_dur,
        pacer.duration_until_next_deadline()
    );

    Ok(())
}
```
