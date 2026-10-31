# Quick Start

Get up and running with Strata in minutes. This guide walks through installing the CLI toolchain, scaffolding a new project, running the local development server with instantaneous Hot Module Reloading, and compiling for production.

---

## Prerequisites

Strata requires:
- **Rust Toolchain**: Rust 1.80.0 or higher (`rustup update stable`)
- **Platform Dependencies**:
  - **Linux**: Standard build tools, `pkg-config`, `libx11-dev`, `libwayland-dev`
  - **macOS**: Xcode Command Line Tools (`xcode-select --install`)
  - **Windows**: Visual Studio 2022 C++ Build Tools

---

## 1. Installing the Strata CLI

Install the official Strata CLI toolchain using the automated installation script:

```bash
curl -fsSL https://strata.larvance.com/install.sh | sh
```

Or build and install directly from Cargo:

```bash
cargo install --path crates/strata_cli --locked
```

Verify your installation:

```bash
strata --version
```

```text
Strata CLI v0.1.0 (Zero Chromium Pure Rust Engine)
```

---

## 2. Scaffolding a New Application

Generate a new application using the interactive project wizard:

```bash
strata new strata-app
```

The CLI wizard will prompt you to select an application template:

```text
▲ Strata v0.1.0 (Zero Chromium Pure Rust Engine)
─────────────────────────────────────────────────────────────

√ What is your project named? › strata-app

? Which template would you like to start with?
  › 1) Blank Canvas           (Minimal zero-dependency reactive canvas)
    2) SaaS Dashboard         (AI Cloud & telemetry infrastructure portal)
    3) Developer Workbench    (Multi-pane desktop workspace with command palette)
    4) Creative Studio        (Vector canvas & node properties editor)

? Target platforms to initialize?
  › [X] Desktop (macOS, Windows, Linux)
    [X] Web (WebAssembly + Canvas)
    [ ] Mobile (iOS, Android)
    [X] Terminal (Interactive TUI)

√ Initializing project workspace...
√ Created strata-app/Cargo.toml
√ Created strata-app/strata.toml
√ Created strata-app/src/main.rs
√ Created strata-app/src/components/mod.rs
√ Successfully initialized strata-app!
```

---

## 3. Project Structure

A newly created Strata workspace features a clean, idiomatic Rust layout:

```text
strata-app/
├── Cargo.toml          # Rust crate manifest and workspace dependencies
├── strata.toml         # Strata engine configuration and build targets
├── assets/             # Static fonts, icons, and textures
│   └── icons/
└── src/
    ├── main.rs         # Application entry point and window lifecycle
    └── components/     # Declarative RSX components
        ├── mod.rs
        ├── metric_card.rs
        └── navbar.rs
```

### The Application Entry Point (`src/main.rs`)

```rust
use strata::prelude::*;

#[component]
fn App() -> View {
    let (count, set_count) = use_state(|| 0);

    rsx! {
        <div class="flex flex-col items-center justify-center min-h-screen bg-neutral-950 text-white font-sans p-8">
            <div class="p-8 rounded-2xl bg-neutral-900 border border-neutral-800 shadow-2xl flex flex-col items-center gap-6 max-w-md w-full">
                <div class="flex items-center gap-2">
                    <span class="text-sm font-mono tracking-widest text-indigo-400 font-semibold uppercase">
                        Strata Engine
                    </span>
                </div>

                <h1 class="text-3xl font-bold tracking-tight text-center">
                    Zero Chromium. Pure Rust.
                </h1>

                <p class="text-neutral-400 text-center text-sm">
                    Reactive signals, Tailwind JIT emulation, and sub-3MB resident memory footprint.
                </p>

                <div class="flex items-center gap-4">
                    <Button
                        variant="primary"
                        on_click={move |_| set_count.update(|c| *c += 1)}
                    >
                        {format!("Count: {}", count())}
                    </Button>
                    <Button
                        variant="secondary"
                        on_click={move |_| set_count.set(0)}
                    >
                        "Reset"
                    </Button>
                </div>
            </div>
        </div>
    }
}

fn main() -> Result<(), strata::Error> {
    strata::App::new()
        .title("Strata Application")
        .size(1024, 768)
        .root(App)
        .run()
}
```

---

## 4. Running the Development Server

Start the local development server with instant Hot Module Reloading (HMR):

```bash
cd strata-app
strata dev
```

Strata's incremental compiler monitors file changes and patches AST subtrees directly into the running GPU swapchain in under **3 milliseconds**, preserving application state without page refreshes.

### Launching in Terminal (TUI) Mode

Execute the exact same application code directly inside your interactive console:

```bash
strata dev --target tui
```

### Launching for the Web (WASM)

Compile and serve the WebAssembly build with local live reload:

```bash
strata dev --target wasm
```

---

## 5. Building for Production

Compile optimized release binaries:

```bash
# Build native desktop release binary
strata build --release

# Build standalone Web bundle (WASM + HTML loader)
strata build --target wasm --release

# Build zero-WASM pure JavaScript ESM bundle
strata build --target js --release
```

Compiled desktop binaries are placed in `dist/desktop/strata-app` with all unused font glyphs, icons, and CSS utilities automatically pruned at compile-time.
