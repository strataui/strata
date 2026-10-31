# Tailwind CSS JIT Engine

Strata features a built-in, pure-Rust implementation of the **Tailwind CSS v3/v4** utility engine (`strata_style`). You get the full velocity of modern utility-first CSS without needing Node.js, PostCSS, or external JavaScript bundlers.

---

## Zero-Dependency Compilation

In traditional web development, Tailwind CSS relies on PostCSS, scanning source files with Node.js and generating bulky stylesheets.

In Strata:
1. The `rsx!` macro scans class strings at compile time.
2. The `strata_style` parser resolves utility classes directly into **256-bit SIMD style bitmasks**.
3. Duplicate classes are merged and specificity conflicts are resolved in constant time ($O(1)$).
4. Zero CSS string parsing occurs at runtime on your user's device.

---

## Supported Utility Classes

### 1. Flexbox & Grid
Strata maps Tailwind flex and grid utilities directly to its layout solver:

```html
<!-- Responsive 12-column layout with gap and alignment -->
<div class="grid grid-cols-12 gap-6 items-center">
    <div class="col-span-8 flex flex-col justify-between h-64 p-6">
        <!-- Content -->
    </div>
    <div class="col-span-4 flex items-center justify-center">
        <!-- Visualizer -->
    </div>
</div>
```

### 2. Spacing & Dimensions
- **Padding & Margin**: `p-1` through `p-96`, `px-*`, `py-*`, `pt-*`, `pr-*`, `pb-*`, `pl-*`, `m-*`, `-m-*` (negative margins).
- **Dimensions**: `w-full`, `w-screen`, `w-64`, `h-full`, `h-screen`, `min-w-0`, `max-w-7xl`.
- **Arbitrary Values**: `w-[420px]`, `h-[calc(100vh-64px)]`, `top-[12.5px]`.

### 3. Surface & Color Hierarchy
Colors follow the standard Tailwind palette with 50–950 tonal steps:

```html
<div class="bg-neutral-950 text-neutral-100 border border-neutral-800/80">
    <button class="bg-indigo-600 hover:bg-indigo-500 text-white font-medium px-4 py-2 rounded-lg">
        Execute
    </button>
</div>
```

### 4. Backdrop Blur & Visual Effects
Hardware-accelerated graphical effects compiled directly into GPU shader passes:
- `backdrop-blur-sm`, `backdrop-blur-md`, `backdrop-blur-xl`
- `shadow-sm`, `shadow-lg`, `shadow-2xl`
- `opacity-0` through `opacity-100`
- `transition-all`, `duration-200`, `ease-out`

### 5. Interaction States & Pseudo-Variants
Variants are supported seamlessly:
- **Hover**: `hover:bg-neutral-800`, `hover:border-neutral-700`
- **Focus & Focus-Within**: `focus:ring-2`, `focus:ring-indigo-500`
- **Active**: `active:scale-[0.98]`
- **Disabled**: `disabled:opacity-50`, `disabled:cursor-not-allowed`
- **Responsive Breakpoints**: `sm:`, `md:`, `lg:`, `xl:`, `2xl:`

```html
<button class="px-5 py-2.5 rounded-xl bg-neutral-900 border border-neutral-800 text-neutral-300 transition-all duration-200 hover:bg-neutral-800 hover:text-white hover:border-neutral-700 active:scale-[0.98] disabled:opacity-40">
    Run Diagnostics
</button>
```

---

## Custom Theme Configuration

Themes and custom tokens can be declared in `strata.toml`:

```toml
[theme.colors]
brand-primary = "#5e6ad2"
brand-surface = "#0f1012"
brand-border = "rgba(255, 255, 255, 0.08)"

[theme.font_family]
sans = ["Inter", "system-ui", "sans-serif"]
mono = ["JetBrains Mono", "monospace"]
```

These tokens become available as utility classes at compile time:
`bg-brand-surface`, `text-brand-primary`, `border-brand-border`.
