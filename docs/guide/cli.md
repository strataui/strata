# CLI Toolchain Reference

The **Strata CLI** (`strata`) is the unified command-line orchestrator for creating, developing, testing, profiling, and bundling Strata applications across all native and web targets.

---

## Global Options

```text
Usage: strata <COMMAND> [OPTIONS]

Commands:
  new       Scaffold a new project from official production templates
  dev       Launch the local development runtime with sub-3ms HMR
  build     Compile optimized binaries for targeted platforms
  package   Generate production installers (DMG, MSI, DEB, AppImage, APK)
  doctor    Diagnose system dependencies, GPU backends, and toolchains
  sim       Launch the dogfooded native mobile device simulator
  bench     Execute automated latency, memory, and layout benchmarks
  help      Print this message or the help of the given subcommand(s)

Options:
  -v, --verbose    Enable verbose diagnostic logging
  -q, --quiet      Suppress non-critical console output
  -h, --help       Print help information
  -V, --version    Print version information
```

---

## `strata new`

Creates a new Strata project workspace with boilerplate configuration, assets, and component directories.

```bash
strata new <PROJECT_NAME> [OPTIONS]
```

### Options
- `--template <NAME>`: Template to scaffold (`blank`, `saas`, `workbench`, `studio`).
- `--no-git`: Skip initializing a git repository.
- `--package-manager <MGR>`: Select package manager if compiling for hybrid web targets.

---

## `strata dev`

Starts the live development environment with the incremental AST hot-patcher and in-process DevTools HUD.

```bash
strata dev [OPTIONS]
```

### Options
- `--target <TARGET>`: Target execution backend (`desktop` [default], `tui`, `wasm`, `js`).
- `--port <PORT>`: Port for the local development server when targeting Web (default: `3000`).
- `--devtools`: Launch with the embedded Chrome DevTools Protocol (CDP) bridge enabled on port `9222`.
- `--release`: Run in release mode with optimizations enabled.
- `--fps-overlay`: Display live 120 FPS vsync and frame budget telemetry HUD.

```bash
# Example: Run in terminal console
strata dev --target tui

# Example: Run web target with live profiling
strata dev --target wasm --devtools --port 8080
```

---

## `strata build`

Compiles the application into an optimized static binary or deployment bundle.

```bash
strata build [OPTIONS]
```

### Options
- `--release`: Build with maximum link-time optimization (LTO) and binary stripping.
- `--target <TARGET>`: Compilation target:
  - `desktop`: Native operating system binary (`macOS`, `Windows`, `Linux`).
  - `tui`: Standalone terminal executable with raw ANSI VT100/Kitty output.
  - `wasm`: Optimized WebAssembly module with WebGPU/WebGL2 canvas bindings.
  - `js`: Zero-WASM JavaScript ESM bundle via `strata_codegen_js`.
  - `mobile`: Mobile library artifacts for iOS (`.a` / `.xcframework`) or Android (`.so` / NDK).
- `--features <LIST>`: Space or comma-separated list of Cargo features to enable.
- `--out-dir <PATH>`: Destination directory for generated artifacts (default: `dist/`).

---

## `strata package`

Generates ready-to-distribute signed installers and platform distribution packages.

```bash
strata package [OPTIONS]
```

### Supported Formats
- **macOS**: `.dmg`, `.app` bundle (notarization and codesigning supported).
- **Windows**: `.msi` (WiX toolset), standalone portable `.exe`.
- **Linux**: `.deb`, `.rpm`, `.AppImage`, Flatpak manifest.
- **Android**: Signed release `.apk` or `.aab` (Android App Bundle).
- **iOS**: `.ipa` archive configured with provisioning profiles.

---

## `strata doctor`

Inspects the host workstation to verify required system compilers, GPU driver capabilities, Vulkan/Metal runtimes, and platform toolchains.

```bash
strata doctor
```

```text
▲ Strata Environment Diagnostics v0.1.0
─────────────────────────────────────────────────────────────

[√] Rust Toolchain: rustc 1.80.0 (stable-x86_64-unknown-linux-gnu)
[√] Cargo Manifest: cargo 1.80.0
[√] Graphics Hardware: NVIDIA GeForce RTX 4090 / Vulkan 1.3.275
[√] Windowing Protocol: Wayland compositor active (X11 fallback available)
[√] System C Compiler: gcc 13.2.0 (x86_64-linux-gnu)
[√] Terminal Capabilities: 24-bit TrueColor + Sixel Graphic Protocol
[√] Node.js Runtime: v24.21.0 (Optional Web Toolchain)

√ All core platform dependencies satisfied.
```

---

## Configuration File (`strata.toml`)

Project configuration is declared in `strata.toml` at the project root:

```toml
[package]
name = "strata-app"
version = "0.1.0"
authors = ["larvance <contact@larvance.com>"]
edition = "2021"

[window]
title = "Strata Application"
width = 1280
height = 800
min_width = 800
min_height = 600
resizable = true
decorations = "frameless"       # "native" | "frameless" | "transparent"
translucency = "mica"           # "none" | "mica" | "acrylic" | "vibrancy"

[rendering]
preferred_backend = "auto"     # "vulkan" | "metal" | "d3d12" | "gl"
target_fps = 120
vsync = true
msaa_samples = 4
font_engine = "slug"            # "slug" (GPU curve) | "cpu" (Atlas)

[devtools]
enabled_in_debug = true
cdp_port = 9222
hud_default_visible = false

[storage]
backend = "bitcask"            # "bitcask" | "memory"
keystore_enabled = true
```
