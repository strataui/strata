# Contributing to Strata

Thank you for your interest in contributing to Strata! Strata is a high-performance, pure-Rust universal UI presentation engine and application framework designed for desktop, mobile, web, and terminal targets without Chromium, WebViews, or heavy third-party runtimes.

This document outlines the engineering standards, architecture, and contribution workflows required to maintain high code quality across the workspace.

---

## 1. Architectural Principles

Strata adheres to strict first-principles engineering:
1. **Pure Rust Core:** We avoid heavy C/C++ dependencies and bloated macro crates (`syn`, `quote`, `proc-macro2`, `rstml`). Where AST parsing or serialization is needed, we prefer hand-crafted recursive-descent parsers or lean standard library primitives.
2. **Zero Chromium & WebViews:** The engine implements its own W3C CSS box model, flexbox/grid layout, text shaping, 2D rasterization, and 3D GPU rendering directly on top of native OS surfaces (Wayland, X11, Win32, Cocoa, Android NDK, iOS Metal).
3. **Cross-Platform Parity:** Core primitives, reactive signals, layout nodes, and styling rules behave deterministically across all supported platforms.
4. **Deterministic Performance:** Zero unnecessary heap allocations in hot render paths, cache-aligned data structures, and sub-millisecond layout passes.

---

## 2. Repository Structure

The Strata workspace is organized into modular crate categories:

```text
strata/
├── crates/
│   ├── core/         # Primitives, DOM reactive DAG, CSS style parser, layout engine
│   ├── render/       # Software rasterizer, WebGPU/wgpu pipeline, 3D viewport, audio engine, platform backends
│   ├── runtime/      # Safe JS interpreter (Mica), Web APIs, VFS, bitcask storage, IPC
│   ├── ui/           # High-level widget components, animations, accessibility (ARIA)
│   ├── tooling/      # CLI packager, DevTools server, hot reload server, code generators
│   └── harnesses/    # Benchmark suites, parity verification harnesses
├── examples/         # Reference applications (SaaS dashboard, Creative Studio, Browser)
└── docs/             # Technical specifications and API documentation
```

---

## 3. Development Workflow & Quality Gates

All contributions must pass strict automated quality gates prior to merging:

### 3.1. Compilation and Linting
The workspace enforces zero compiler warnings and zero Clippy warnings:

```bash
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
```

Both commands must exit with code 0.

### 3.2. Automated Testing
Unit tests and integration tests across all workspace crates must pass with 100% success rate:

```bash
cargo test --workspace
```

### 3.3. Executable Documentation Tests
Public APIs must contain runnable, tested examples in their docstrings:

```bash
cargo test --doc
```

---

## 4. Coding & Documentation Standards

1. **Zero Decorative Emojis:**
   To maintain professional engineering standards, do not include emojis in source code, inline comments, commit messages, CLI output, or documentation. Use clean text markers (e.g., `[OK]`, `[ERROR]`, `[INFO]`, `->`, `*`).

2. **Descriptive Milestone Terminology:**
   Describe work by its functional scope, crate boundaries, and architectural capabilities rather than arbitrary development phases.

3. **Memory Safety & Unsafe Code:**
   Keep `unsafe` blocks minimal, isolated, and documented with an explicit `// SAFETY:` explanation detailing invariants and caller guarantees.

4. **Conventional Commits:**
   Follow the Conventional Commits specification:
   - `feat(<scope>): description`
   - `fix(<scope>): description`
   - `docs(<scope>): description`
   - `chore(<scope>): description`
   - `perf(<scope>): description`
   - `test(<scope>): description`

---

## 5. Submitting Changes

1. Create a descriptive feature branch from `main`.
2. Ensure your changes include appropriate unit tests covering edge cases and regression scenarios.
3. Run the full verification suite (`cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`).
4. Submit a pull request detailing the problem solved, architectural approach, and benchmark or test verification results.
