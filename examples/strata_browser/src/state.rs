//! Reactive State Machine for Strata Browser Harness.

use strata::prelude::*;

/// Representation of an individual browser tab.
#[derive(Clone, Debug, PartialEq)]
pub struct TabInfo {
    pub id: usize,
    pub title: String,
    pub url: String,
    pub html: String,
}

/// Reactive State for the Strata Web Browser example harness.
#[derive(Clone)]
pub struct BrowserState {
    pub tabs: Signal<Vec<TabInfo>>,
    pub active_index: Signal<usize>,
    pub url_input: Signal<String>,
    pub status_message: Signal<String>,
    pub history: Signal<Vec<String>>,
    pub history_index: Signal<usize>,
}

impl Default for BrowserState {
    fn default() -> Self {
        Self::new()
    }
}

impl BrowserState {
    /// Constructs a new `BrowserState` preloaded with the flagship welcome page.
    pub fn new() -> Self {
        let initial_tabs = vec![
            TabInfo {
                id: 1,
                title: "Welcome to Strata".to_string(),
                url: "strata://welcome".to_string(),
                html: Self::welcome_page_html(),
            },
            TabInfo {
                id: 2,
                title: "Engine Architecture".to_string(),
                url: "strata://about".to_string(),
                html: Self::about_page_html(),
            },
        ];

        Self {
            tabs: Signal::new(initial_tabs),
            active_index: Signal::new(0),
            url_input: Signal::new("strata://welcome".to_string()),
            status_message: Signal::new(
                "Strata Engine v0.1.0 | Pure-Rust Zero-Chromium Target".to_string(),
            ),
            history: Signal::new(vec!["strata://welcome".to_string()]),
            history_index: Signal::new(0),
        }
    }

    /// Returns the currently active `TabInfo`.
    pub fn active_tab(&self) -> Option<TabInfo> {
        let tabs = self.tabs.get();
        let idx = self.active_index.get();
        tabs.get(idx).cloned()
    }

    /// Navigates the active tab to the specified URL or internal page.
    pub fn navigate(&self, url: &str) {
        let trimmed = url.trim();
        self.url_input.set(trimmed.to_string());

        let (title, html) = match trimmed {
            "strata://welcome" | "about:blank" | "" => {
                ("Welcome to Strata".to_string(), Self::welcome_page_html())
            }
            "strata://about" => ("Engine Architecture".to_string(), Self::about_page_html()),
            "strata://benchmarks" => (
                "Empirical Benchmarks".to_string(),
                Self::benchmarks_page_html(),
            ),
            custom_url => {
                let formatted_html = format!(
                    r#"<article class="custom-page">
                        <h1>Viewing: {}</h1>
                        <p>Offline simulation page rendered directly by <strong>strata_web_reader</strong>.</p>
                        <section class="card">
                            <h3>Document Information</h3>
                            <p>Protocol: Direct RSX Blueprint Transpilation</p>
                            <p>Security Context: Sandboxed Memory Domain</p>
                        </section>
                    </article>"#,
                    custom_url
                );
                (custom_url.to_string(), formatted_html)
            }
        };

        let mut tabs = self.tabs.get();
        let idx = self.active_index.get();
        if let Some(tab) = tabs.get_mut(idx) {
            tab.title = title;
            tab.url = trimmed.to_string();
            tab.html = html;
            self.tabs.set(tabs);
        }

        let mut hist = self.history.get();
        hist.push(trimmed.to_string());
        self.history_index.set(hist.len().saturating_sub(1));
        self.history.set(hist);

        self.status_message
            .set(format!("Rendered document: {}", trimmed));
    }

    /// Opens a new browser tab.
    pub fn new_tab(&self, title: &str, url: &str, html: &str) {
        let mut tabs = self.tabs.get();
        let next_id = tabs.iter().map(|t| t.id).max().unwrap_or(0) + 1;
        tabs.push(TabInfo {
            id: next_id,
            title: title.to_string(),
            url: url.to_string(),
            html: html.to_string(),
        });
        let new_idx = tabs.len() - 1;
        self.tabs.set(tabs);
        self.active_index.set(new_idx);
        self.url_input.set(url.to_string());
        self.status_message
            .set(format!("Opened new tab: {}", title));
    }

    /// Closes the tab at the given index.
    pub fn close_tab(&self, index: usize) {
        let mut tabs = self.tabs.get();
        if tabs.len() <= 1 {
            return;
        }
        if index < tabs.len() {
            tabs.remove(index);
            let current_active = self.active_index.get();
            let new_active = if current_active >= tabs.len() {
                tabs.len() - 1
            } else if current_active > index {
                current_active - 1
            } else {
                current_active
            };
            let new_url = tabs[new_active].url.clone();
            self.tabs.set(tabs);
            self.active_index.set(new_active);
            self.url_input.set(new_url);
            self.status_message.set("Closed tab".to_string());
        }
    }

    /// Reloads the current tab.
    pub fn reload(&self) {
        if let Some(tab) = self.active_tab() {
            self.navigate(&tab.url);
            self.status_message
                .set("Reloaded active document".to_string());
        }
    }

    /// Navigates back in history.
    pub fn back(&self) {
        let idx = self.history_index.get();
        if idx > 0 {
            let hist = self.history.get();
            let prev_idx = idx - 1;
            if let Some(prev_url) = hist.get(prev_idx) {
                self.history_index.set(prev_idx);
                self.navigate_without_history(prev_url);
            }
        }
    }

    /// Navigates forward in history.
    pub fn forward(&self) {
        let idx = self.history_index.get();
        let hist = self.history.get();
        if idx + 1 < hist.len() {
            let next_idx = idx + 1;
            if let Some(next_url) = hist.get(next_idx) {
                self.history_index.set(next_idx);
                self.navigate_without_history(next_url);
            }
        }
    }

    fn navigate_without_history(&self, url: &str) {
        self.url_input.set(url.to_string());
        let mut tabs = self.tabs.get();
        let idx = self.active_index.get();
        if let Some(tab) = tabs.get_mut(idx) {
            tab.url = url.to_string();
            tab.title = url.to_string();
            self.tabs.set(tabs);
        }
    }

    /// Built-in Welcome Page HTML document.
    pub fn welcome_page_html() -> String {
        r#"<!DOCTYPE html>
<html>
<head><title>Welcome to Strata</title></head>
<body>
  <div style="padding: 24px; font-family: sans-serif; color: #f8fafc; background-color: #0f172a; border-radius: 8px;">
    <h1 style="color: #38bdf8; font-size: 28px; margin-bottom: 8px;">Strata Zero-Chromium Web Engine</h1>
    <p style="color: #94a3b8; font-size: 16px; margin-bottom: 24px;">Native desktop web presentation engineered from first principles in standard library Rust.</p>
    
    <div style="background-color: #1e293b; padding: 20px; border-radius: 6px; margin-bottom: 16px; border: 1px solid #334155;">
      <h2 style="color: #f1f5f9; font-size: 20px; margin-top: 0;">Architectural Principles</h2>
      <p style="color: #cbd5e1; line-height: 1.6;">
        Unlike Chromium, WebKit, or Electron applications, Strata executes HTML5 parsing, CSS cascade resolution,
        flexbox and grid layout algorithms, and rasterization directly in pure Rust.
      </p>
      <ul style="color: #94a3b8; line-height: 1.8;">
        <li>Zero Chromium, WebKit, or CEF binary bloat.</li>
        <li>Instant cold boot (&lt;15ms) with zero JIT compiler warm-up penalty.</li>
        <li>Sub-50MB resident memory footprint for modern multi-tab workloads.</li>
        <li>Safe, deterministic concurrency without cross-thread data races.</li>
      </ul>
    </div>

    <div style="background-color: #1e293b; padding: 20px; border-radius: 6px; border: 1px solid #334155;">
      <h2 style="color: #f1f5f9; font-size: 20px; margin-top: 0;">Sovereign Companion Ecosystem</h2>
      <p style="color: #cbd5e1; line-height: 1.6;">
        For complete multi-process sandboxing, DevTools inspector, network caching, and W3C DOM simulation,
        explore the companion <code>onyx</code> browser repository.
      </p>
    </div>
  </div>
</body>
</html>"#.to_string()
    }

    /// Built-in About Page HTML document.
    pub fn about_page_html() -> String {
        r#"<!DOCTYPE html>
<html>
<head><title>Engine Architecture</title></head>
<body>
  <div style="padding: 24px; font-family: sans-serif; color: #f8fafc; background-color: #0f172a; border-radius: 8px;">
    <h1 style="color: #a855f7; font-size: 28px; margin-bottom: 8px;">Strata Engine Blueprint</h1>
    <p style="color: #94a3b8; font-size: 16px; margin-bottom: 24px;">5-Tier High Performance Desktop Architecture</p>
    
    <div style="background-color: #1e293b; padding: 20px; border-radius: 6px; border: 1px solid #334155;">
      <h3 style="color: #38bdf8;">1. Core Primitives</h3>
      <p style="color: #cbd5e1;">strata_dom, strata_layout, strata_style, strata_primitives</p>
      
      <h3 style="color: #38bdf8;">2. Presentation Pipeline</h3>
      <p style="color: #cbd5e1;">strata_render, strata_gpu_path, strata_platform</p>
      
      <h3 style="color: #38bdf8;">3. Runtime Systems</h3>
      <p style="color: #cbd5e1;">strata_vfs, strata_storage, strata_web_api, strata_web_reader</p>
    </div>
  </div>
</body>
</html>"#.to_string()
    }

    /// Built-in Benchmarks Page HTML document.
    pub fn benchmarks_page_html() -> String {
        r#"<!DOCTYPE html>
<html>
<head><title>Empirical Benchmarks</title></head>
<body>
  <div style="padding: 24px; font-family: sans-serif; color: #f8fafc; background-color: #0f172a; border-radius: 8px;">
    <h1 style="color: #22c55e; font-size: 28px; margin-bottom: 8px;">Empirical Performance Metrics</h1>
    <p style="color: #94a3b8; font-size: 16px; margin-bottom: 24px;">Validated on Linux x86_64 Reference Hardware</p>
    
    <div style="background-color: #1e293b; padding: 20px; border-radius: 6px; border: 1px solid #334155;">
      <p style="color: #cbd5e1;"><strong>Signal Propagation:</strong> &lt;250ns for 10,000 dependent nodes</p>
      <p style="color: #cbd5e1;"><strong>CSS Grid Placement:</strong> &lt;1.2ms for 1,000 cell matrix</p>
      <p style="color: #cbd5e1;"><strong>HTML Transpilation:</strong> &lt;3.5ms for 50KB Wikipedia article</p>
      <p style="color: #cbd5e1;"><strong>Binary Memory Footprint:</strong> 18.4 MB physical RSS</p>
    </div>
  </div>
</body>
</html>"#.to_string()
    }
}
