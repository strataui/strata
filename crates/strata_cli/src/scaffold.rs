//! Project scaffolding engine generating compilable Strata projects from templates.

use std::fs;
use std::path::{Path, PathBuf};
use crate::args::TemplateKind;

/// Summary report of a completed scaffolding operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScaffoldReport {
    pub name: String,
    pub template: TemplateKind,
    pub root_path: PathBuf,
    pub created_files: Vec<PathBuf>,
}

/// Project Scaffolder engine.
pub struct ProjectScaffolder;

impl ProjectScaffolder {
    /// Scaffolds a new Strata project at `target_dir / name`.
    pub fn scaffold(
        name: &str,
        template: TemplateKind,
        target_dir: &Path,
    ) -> Result<ScaffoldReport, String> {
        let path = Path::new(name);
        let (project_root, pkg_name) = if path.is_absolute() {
            let pkg = path.file_name().and_then(|s| s.to_str()).unwrap_or(name);
            (path.to_path_buf(), pkg.to_string())
        } else if path.components().count() > 1 {
            let pkg = path.file_name().and_then(|s| s.to_str()).unwrap_or(name);
            (target_dir.join(path), pkg.to_string())
        } else {
            (target_dir.join(name), name.to_string())
        };

        if project_root.exists() {
            let is_not_empty = fs::read_dir(&project_root)
                .map(|mut entries| entries.next().is_some())
                .unwrap_or(false);
            if is_not_empty {
                return Err(format!(
                    "Target directory '{}' already exists and is not empty. Please choose a different project name or empty the directory.",
                    project_root.display()
                ));
            }
        }

        let src_dir = project_root.join("src");

        fs::create_dir_all(&src_dir)
            .map_err(|e| format!("Failed to create project directory '{:?}': {}", src_dir, e))?;

        let mut created_files = Vec::new();

        // 1. Cargo.toml
        let cargo_toml_path = project_root.join("Cargo.toml");
        let cargo_content = Self::generate_cargo_toml(&pkg_name);
        fs::write(&cargo_toml_path, cargo_content)
            .map_err(|e| format!("Failed to write Cargo.toml: {}", e))?;
        created_files.push(cargo_toml_path);

        // 2. strata.toml
        let strata_toml_path = project_root.join("strata.toml");
        let strata_content = Self::generate_strata_toml(&pkg_name, template);
        fs::write(&strata_toml_path, strata_content)
            .map_err(|e| format!("Failed to write strata.toml: {}", e))?;
        created_files.push(strata_toml_path);

        // 3. src/main.rs
        let main_rs_path = src_dir.join("main.rs");
        let main_content = Self::generate_main_rs(template);
        fs::write(&main_rs_path, main_content)
            .map_err(|e| format!("Failed to write src/main.rs: {}", e))?;
        created_files.push(main_rs_path);

        // 4. README.md
        let readme_path = project_root.join("README.md");
        let readme_content = Self::generate_readme(name, template);
        fs::write(&readme_path, readme_content)
            .map_err(|e| format!("Failed to write README.md: {}", e))?;
        created_files.push(readme_path);

        // 5. .gitignore
        let gitignore_path = project_root.join(".gitignore");
        let gitignore_content = Self::generate_gitignore();
        fs::write(&gitignore_path, gitignore_content)
            .map_err(|e| format!("Failed to write .gitignore: {}", e))?;
        created_files.push(gitignore_path);

        Ok(ScaffoldReport {
            name: name.to_string(),
            template,
            root_path: project_root,
            created_files,
        })
    }

    pub fn generate_cargo_toml(name: &str) -> String {
        let strata_path = concat!(env!("CARGO_MANIFEST_DIR"), "/../strata");
        format!(
            r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2021"

[dependencies]
strata = {{ path = "{strata_path}", version = "0.1.0" }}
"#
        )
    }

    pub fn generate_strata_toml(name: &str, template: TemplateKind) -> String {
        format!(
            r#"[app]
name = "{name}"
version = "0.1.0"
template = "{template}"

[window]
title = "{name}"
width = 1280
height = 800
resizable = true

[web]
target = "web-js"
"#,
            template = template.as_str()
        )
    }

    pub fn generate_main_rs(template: TemplateKind) -> String {
        match template {
            TemplateKind::Blank => r#"use strata::prelude::*;

#[component]
fn App() -> View {
    rsx! {
        <div class="flex flex-col items-center justify-center h-screen bg-[#09090b] text-white">
            <h1 class="text-3xl font-bold mb-4">{"Welcome to Strata"}</h1>
            <p class="text-zinc-400 mb-6">{"Zero Chromium. Pure Rust. Pixel-Perfect."}</p>
            <div class="px-4 py-2 bg-indigo-600 hover:bg-indigo-500 rounded-md font-semibold transition cursor-pointer">
                {"Get Started with Strata"}
            </div>
        </div>
    }
}

fn main() {
    strata::launch(App);
}
"#.to_string(),

            TemplateKind::Saas => r#"use strata::prelude::*;

#[component]
fn MetricCard(title: String, value: String, delta: String) -> View {
    rsx! {
        <div class="p-6 bg-zinc-900 border border-zinc-800 rounded-xl flex flex-col justify-between">
            <span class="text-sm font-medium text-zinc-400">{title}</span>
            <div class="text-2xl font-bold text-white mt-2">{value}</div>
            <span class="text-xs text-emerald-400 font-semibold mt-1">{delta}</span>
        </div>
    }
}

#[component]
fn App() -> View {
    rsx! {
        <div class="flex flex-col h-screen bg-black text-white p-8">
            <header class="flex justify-between items-center mb-8 pb-4 border-b border-zinc-800">
                <h1 class="text-xl font-bold tracking-tight">{"Cloud Infrastructure Health"}</h1>
                <span class="px-3 py-1 bg-emerald-950 text-emerald-400 text-xs font-semibold rounded-full border border-emerald-800">{"All Systems Operational"}</span>
            </header>
            <div class="grid grid-cols-3 gap-6">
                <MetricCard title="Active Compute Nodes".into() value="1,024".into() delta="+12% from last hour".into() />
                <MetricCard title="Average Latency".into() value="1.42 ms".into() delta="-0.2ms improvement".into() />
                <MetricCard title="Bandwidth Ingress".into() value="48.2 Gbps".into() delta="+5.4 Gbps burst".into() />
            </div>
        </div>
    }
}

fn main() {
    strata::launch(App);
}
"#.to_string(),

            TemplateKind::Workbench => r#"use strata::prelude::*;

#[component]
fn FileTreeItem(name: String, is_active: bool) -> View {
    let bg = if is_active { "bg-zinc-800 text-white" } else { "text-zinc-400 hover:text-zinc-200 hover:bg-zinc-900" };
    rsx! {
        <li class={format!("px-2 py-1.5 rounded text-xs font-mono cursor-pointer transition flex items-center justify-between {}", bg)}>
            <span>{name}</span>
        </li>
    }
}

#[component]
fn App() -> View {
    rsx! {
        <div class="flex flex-col h-screen bg-[#09090b] text-zinc-100 font-sans select-none">
            <!-- Top Command Ribbon / Header -->
            <header class="h-10 border-b border-zinc-800 px-4 flex items-center justify-between bg-zinc-950">
                <div class="flex items-center space-x-3 text-xs">
                    <span class="font-bold text-cyan-400">{"STRATA"}</span>
                    <span class="text-zinc-600">{"/"}</span>
                    <span class="text-zinc-300 font-medium">{"Workbench"}</span>
                </div>
                <div class="px-3 py-1 bg-zinc-900 border border-zinc-800 rounded text-xs text-zinc-400 flex items-center space-x-2">
                    <span>{"Search or run command..."}</span>
                    <kbd class="px-1.5 py-0.5 bg-zinc-800 border border-zinc-700 rounded text-[10px] text-zinc-300 font-mono">{"Ctrl+K"}</kbd>
                </div>
            </header>

            <!-- Main Work Area -->
            <div class="flex flex-1 overflow-hidden">
                <!-- Sidebar File Explorer -->
                <aside class="w-60 border-r border-zinc-800 bg-zinc-950/80 p-3 flex flex-col justify-between">
                    <div>
                        <div class="text-[11px] font-bold text-zinc-500 uppercase tracking-wider mb-2">{"Explorer"}</div>
                        <ul class="space-y-0.5">
                            <FileTreeItem name="src/main.rs".into() is_active=true />
                            <FileTreeItem name="src/components.rs".into() is_active=false />
                            <FileTreeItem name="strata.toml".into() is_active=false />
                            <FileTreeItem name="Cargo.toml".into() is_active=false />
                        </ul>
                    </div>
                    <div class="text-[11px] text-zinc-600 border-t border-zinc-900 pt-2 flex items-center justify-between">
                        <span>{"Rust 2021"}</span>
                        <span>{"UTF-8"}</span>
                    </div>
                </aside>

                <!-- Center Tabbed Workspace & Document Body -->
                <main class="flex-1 flex flex-col bg-[#0d0d10]">
                    <!-- Tabs -->
                    <div class="h-9 border-b border-zinc-800 flex items-center px-2 space-x-1 bg-zinc-950/40">
                        <div class="px-3 py-1.5 bg-[#0d0d10] border-t-2 border-cyan-400 text-xs font-mono text-white flex items-center space-x-2">
                            <span>{"src/main.rs"}</span>
                        </div>
                    </div>

                    <!-- Editor Body -->
                    <div class="flex-1 p-6 font-mono text-sm leading-relaxed overflow-auto text-zinc-300">
                        <div class="text-zinc-600 mb-2">{"// Reactive Developer Workbench initialized"}</div>
                        <div><span class="text-cyan-400">{"use"}</span> {" strata::prelude::*;"}</div>
                        <div class="mt-4"><span class="text-purple-400">{"fn"}</span> <span class="text-blue-400">{"main"}</span>{"() {"}</div>
                        <div class="pl-4"><span class="text-zinc-400">{"println!(\"Running at 120 FPS\");"}</span></div>
                        <div>{"}"}</div>
                    </div>

                    <!-- Bottom Diagnostic Console Drawer -->
                    <div class="h-32 border-t border-zinc-800 bg-black/60 p-3 font-mono text-xs">
                        <div class="text-zinc-500 text-[11px] uppercase tracking-wider mb-1">{"Diagnostics & Output"}</div>
                        <div class="text-emerald-400">{"[ok] GPU Pipeline: Direct Hardware Swapchain active"}</div>
                        <div class="text-zinc-400">{"[info] Frame reconcile: 1 µs | Cold start: 14 µs"}</div>
                    </div>
                </main>
            </div>

            <!-- Status Bar -->
            <footer class="h-6 border-t border-zinc-800 bg-zinc-950 px-3 flex items-center justify-between text-[11px] text-zinc-400 font-mono">
                <div class="flex items-center space-x-4">
                    <span>{"git:(main)"}</span>
                    <span>{"0 errors, 0 warnings"}</span>
                </div>
                <div class="flex items-center space-x-4">
                    <span class="text-cyan-400">{"Strata Engine v0.1.0"}</span>
                </div>
            </footer>
        </div>
    }
}

fn main() {
    strata::launch(App);
}
"#.to_string(),

            TemplateKind::Ecommerce => r#"use strata::prelude::*;

#[component]
fn ProductCard(name: String, price: String, tag: String) -> View {
    rsx! {
        <div class="p-4 bg-zinc-900 border border-zinc-800 rounded-lg flex flex-col">
            <div class="h-48 bg-zinc-800 rounded-md mb-4 flex items-center justify-center text-zinc-500 font-semibold">
                {tag}
            </div>
            <h3 class="font-bold text-white text-lg">{name}</h3>
            <span class="text-emerald-400 font-semibold mt-1">{price}</span>
            <button class="mt-4 w-full py-2 bg-white text-black font-semibold rounded hover:bg-zinc-200 transition">
                {"Add to Cart"}
            </button>
        </div>
    }
}

#[component]
fn App() -> View {
    rsx! {
        <div class="min-h-screen bg-[#09090b] text-white p-8">
            <h1 class="text-2xl font-bold mb-6">{"Featured Collection"}</h1>
            <div class="grid grid-cols-2 gap-6">
                <ProductCard name="Strata Velocity Elite".into() price="$185.00".into() tag="Pro Apparel".into() />
                <ProductCard name="AeroShield Windbreaker".into() price="$220.00".into() tag="Waterproof".into() />
            </div>
        </div>
    }
}

fn main() {
    strata::launch(App);
}
"#.to_string(),

            TemplateKind::Studio => r#"use strata::prelude::*;

#[component]
fn App() -> View {
    rsx! {
        <div class="flex h-screen bg-[#0c0c0e] text-white">
            <aside class="w-64 border-r border-zinc-800 p-4 flex flex-col justify-between">
                <div>
                    <h2 class="font-bold text-sm tracking-wide text-zinc-400 uppercase mb-4">{"Layers"}</h2>
                    <ul class="space-y-1 text-sm text-zinc-300">
                        <li class="px-2 py-1 bg-zinc-800 rounded">{"Vector Canvas"}</li>
                        <li class="px-2 py-1 hover:bg-zinc-900 rounded">{"Lighting Rig"}</li>
                    </ul>
                </div>
                <div class="text-xs text-zinc-600">{"Strata Studio v0.1"}</div>
            </aside>
            <main class="flex-1 flex items-center justify-center bg-black">
                <div class="text-zinc-600 font-mono text-sm">{"Interactive 3D / 2D Viewport"}</div>
            </main>
        </div>
    }
}

fn main() {
    strata::launch(App);
}
"#.to_string(),
        }
    }

    pub fn generate_readme(name: &str, template: TemplateKind) -> String {
        format!(
            r#"# {name}

Generated with **Strata** using the `{template}` template.

## Getting Started

Run the interactive local development server with instant Hot Module Replacement (HMR):
```bash
strata dev
```

Build an optimized production binary:
```bash
strata build --release
```

Package for distribution:
```bash
strata package
```
"#,
            name = name,
            template = template.as_str()
        )
    }

    pub fn generate_gitignore() -> String {
        r#"/target
/dist
**/*.rs.bk
.DS_Store
"#
        .to_string()
    }
}
