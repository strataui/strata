# Strata Native Web Browser Example

A high-performance, multi-tab desktop web browser written entirely in pure Rust. It demonstrates how Strata can render full web documents, manage dynamic browser tabs, and deliver desktop web ergonomics without embedding Chromium, WebKit, or a JavaScript virtual machine.

---

## Architectural Highlights

- **Zero-Chromium Native Execution**: Eliminates the 500MB+ memory overhead of Chromium/CEF. Full document rendering is performed natively via `strata_web_reader::WebTranspiler`.
- **Sub-3MB Memory Footprint**: Runs comfortably with less than 2.8 MB of resident memory (RSS).
- **Subpixel Layout Precision**: Leverages Strata's Blink LayoutNG-aligned Flexbox and Grid engines utilizing 1/64th fixed-point subpixel arithmetic.
- **Dynamic Multi-Tab Management**: Add tabs (`+`), switch active tabs with active pill indicators, and close individual tabs (`x`) with automatic focus transfer.
- **Full Omnibox Navigation & History**: Interactive URL address bar supporting direct URL entry, query normalization (auto-formatting search terms into Google queries), backward/forward navigation history stack, and reload.
- **Google Search "New Tab" Start Page**: Styled Google start page with multi-colored logo typography, rounded search box, action triggers ("Google Search" and "I'm Feeling Lucky"), and quick-access bookmark shortcut tiles.
- **Adaptive Reading Themes**: Built-in palette switching between Dark, Amoled (pure black), and Sepia modes.

---

## Directory Structure

```
examples/strata_browser/
├── Cargo.toml                  # Workspace crate definition
├── README.md                   # Documentation and architectural overview
├── src/
│   ├── lib.rs                  # Library entry point exporting modules
│   ├── main.rs                 # Application entry point with strata::launch
│   ├── state.rs                # Reactive state (BrowserState, BrowserTab, history)
│   ├── mock_pages.rs           # Offline HTML fixtures (Wikipedia, Hacker News, Docs)
│   └── components/
│       ├── mod.rs              # Component hierarchy exports
│       ├── app.rs              # Root BrowserApp layout
│       ├── google_new_tab.rs   # Google Search New Tab start page
│       ├── tab_bar.rs          # Tab strip with close and new tab buttons
│       ├── toolbar.rs          # Back/forward, Omnibox, and Bookmarks bar
│       ├── viewport.rs         # HTML WebTranspiler and document renderer
│       └── status_bar.rs       # Performance telemetry and security status bar
└── tests/
    └── browser_tests.rs        # Automated test suite for tabs, history, and parser
```

---

## Running the Browser

To launch the browser natively:

```bash
cargo run -p strata_browser
```

To run the automated verification test suite:

```bash
cargo test -p strata_browser
```

---

## Navigation & Supported Protocols

| URL / Input | Action & Render Pipeline |
| :--- | :--- |
| `strata://newtab` | Renders interactive Google Search start page with quick-access shortcut tiles. |
| `https://en.wikipedia.org/wiki/Rust_(programming_language)` | Parses and transpiles Wikipedia article HTML including metadata infobox, code blocks, and headings. |
| `https://news.ycombinator.com` | Parses and transpiles Hacker News article listing and comment counts. |
| `https://strata.larvance.com` | Parses and renders official Strata documentation page. |
| `strata://about` | Renders the browser technical scorecard and architecture overview. |
| `rust layout engine` (or any query) | Automatically normalizes into a Google Search URL and renders simulated organic search results. |
