//! Reactive state management for tabs, navigation history, and Omnibox.

use strata::prelude::*;
use crate::mock_pages::resolve_page;

/// Represents an individual browser tab with full history and rendered document state.
#[derive(Clone, Debug, PartialEq)]
pub struct BrowserTab {
    pub id: usize,
    pub title: String,
    pub url: String,
    pub history: Vec<String>,
    pub history_index: usize,
    pub is_loading: bool,
    pub html_content: String,
}

impl BrowserTab {
    pub fn new(id: usize, url: &str) -> Self {
        let (title, html) = resolve_page(url);
        Self {
            id,
            title,
            url: url.to_string(),
            history: vec![url.to_string()],
            history_index: 0,
            is_loading: false,
            html_content: html,
        }
    }
}

/// DevTools tabs matching Chromium DevTools suite
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DevToolsTab {
    Elements,
    Console,
    Sources,
    Network,
    Performance,
    Memory,
    Application,
    Security,
}

/// Console logging level
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogLevel {
    Verbose,
    Info,
    Warn,
    Error,
}

/// DevTools console log item
#[derive(Clone, Debug, PartialEq)]
pub struct ConsoleLogEntry {
    pub level: LogLevel,
    pub timestamp: String,
    pub source: String,
    pub message: String,
}

/// DevTools network request waterfall item
#[derive(Clone, Debug, PartialEq)]
pub struct NetworkRequestEntry {
    pub name: String,
    pub method: String,
    pub status: u16,
    pub status_text: String,
    pub kind: String,
    pub initiator: String,
    pub size_bytes: usize,
    pub time_ms: f32,
}

/// Global browser state containing open tabs, active tab pointer, navigation inputs, and DevTools.
#[derive(Clone)]
pub struct BrowserState {
    pub next_tab_id: Signal<usize>,
    pub tabs: Signal<Vec<BrowserTab>>,
    pub active_tab_id: Signal<usize>,
    pub url_input: Signal<String>,
    pub search_input: Signal<String>,
    pub active_theme: Signal<String>,
    pub is_omnibox_editing: Signal<bool>,
    pub omnibox_selected: Signal<bool>,

    // DevTools Suite State
    pub is_devtools_open: Signal<bool>,
    pub devtools_tab: Signal<DevToolsTab>,
    pub devtools_selected_element: Signal<String>,
    pub devtools_console_filter: Signal<String>,
    pub devtools_is_inspecting: Signal<bool>,
    pub devtools_console_input: Signal<String>,
    pub devtools_console_logs: Signal<Vec<ConsoleLogEntry>>,
    pub devtools_network_requests: Signal<Vec<NetworkRequestEntry>>,
    pub devtools_drawer_open: Signal<bool>,
    pub devtools_drawer_tab: Signal<String>,
}

impl BrowserState {
    pub fn new() -> Self {
        let initial_url = "strata://newtab";
        let initial_tab = BrowserTab::new(1, initial_url);

        let initial_logs = vec![
            ConsoleLogEntry {
                level: LogLevel::Info,
                timestamp: "14:02:01.014".to_string(),
                source: "strata_runtime".to_string(),
                message: "Strata Core LayoutNG initialized. Subpixel advance calibration active.".to_string(),
            },
            ConsoleLogEntry {
                level: LogLevel::Info,
                timestamp: "14:02:01.018".to_string(),
                source: "strata_platform".to_string(),
                message: "Cairo hardware presentation surface bound to Linux display server.".to_string(),
            },
            ConsoleLogEntry {
                level: LogLevel::Warn,
                timestamp: "14:02:01.032".to_string(),
                source: "strata_web_reader".to_string(),
                message: "Non-standard CSS property -webkit-font-smoothing ignored in native text shaper.".to_string(),
            },
            ConsoleLogEntry {
                level: LogLevel::Error,
                timestamp: "14:02:01.045".to_string(),
                source: "strata_network".to_string(),
                message: "GET https://google.com/favicon.ico 404 (Not Found) - cached fallback used.".to_string(),
            },
        ];

        let initial_requests = vec![
            NetworkRequestEntry {
                name: "newtab".to_string(),
                method: "GET".to_string(),
                status: 200,
                status_text: "OK".to_string(),
                kind: "document".to_string(),
                initiator: "other".to_string(),
                size_bytes: 4210,
                time_ms: 0.42,
            },
            NetworkRequestEntry {
                name: "open_sans_regular.woff2".to_string(),
                method: "GET".to_string(),
                status: 200,
                status_text: "OK".to_string(),
                kind: "font".to_string(),
                initiator: "link".to_string(),
                size_bytes: 24680,
                time_ms: 0.85,
            },
            NetworkRequestEntry {
                name: "google_logo.svg".to_string(),
                method: "GET".to_string(),
                status: 200,
                status_text: "OK".to_string(),
                kind: "image".to_string(),
                initiator: "img".to_string(),
                size_bytes: 1824,
                time_ms: 0.28,
            },
            NetworkRequestEntry {
                name: "styles.css".to_string(),
                method: "GET".to_string(),
                status: 200,
                status_text: "OK".to_string(),
                kind: "stylesheet".to_string(),
                initiator: "link".to_string(),
                size_bytes: 8432,
                time_ms: 0.51,
            },
            NetworkRequestEntry {
                name: "favicon.ico".to_string(),
                method: "GET".to_string(),
                status: 404,
                status_text: "Not Found".to_string(),
                kind: "image".to_string(),
                initiator: "link".to_string(),
                size_bytes: 142,
                time_ms: 0.19,
            },
        ];

        Self {
            next_tab_id: Signal::new(2),
            tabs: Signal::new(vec![initial_tab]),
            active_tab_id: Signal::new(1),
            url_input: Signal::new(initial_url.to_string()),
            search_input: Signal::new(String::new()),
            active_theme: Signal::new("dark".to_string()),
            is_omnibox_editing: Signal::new(false),
            omnibox_selected: Signal::new(false),

            // DevTools
            is_devtools_open: Signal::new(false),
            devtools_tab: Signal::new(DevToolsTab::Elements),
            devtools_selected_element: Signal::new("button.btn-search".to_string()),
            devtools_console_filter: Signal::new(String::new()),
            devtools_is_inspecting: Signal::new(false),
            devtools_console_input: Signal::new(String::new()),
            devtools_console_logs: Signal::new(initial_logs),
            devtools_network_requests: Signal::new(initial_requests),
            devtools_drawer_open: Signal::new(true),
            devtools_drawer_tab: Signal::new("Console".to_string()),
        }
    }

    /// Activates Omnibox input editing with all text selected by default.
    pub fn start_editing_omnibox(&self) {
        self.is_omnibox_editing.set(true);
        self.omnibox_selected.set(true);
    }

    /// Exits Omnibox input editing.
    pub fn stop_editing_omnibox(&self) {
        self.is_omnibox_editing.set(false);
        self.omnibox_selected.set(false);
    }

    /// Cancels Omnibox input editing, reverting to active tab's URL.
    pub fn cancel_editing_omnibox(&self) {
        if let Some(tab) = self.active_tab() {
            self.url_input.set(tab.url);
        }
        self.is_omnibox_editing.set(false);
        self.omnibox_selected.set(false);
    }

    /// Commits the current Omnibox input and navigates.
    pub fn commit_omnibox(&self) {
        let target = self.url_input.get();
        self.navigate(&target);
        self.is_omnibox_editing.set(false);
        self.omnibox_selected.set(false);
    }

    /// Handles character input into the Omnibox.
    pub fn handle_key_input(&self, ch: char) {
        if self.omnibox_selected.get() {
            self.url_input.set(ch.to_string());
            self.omnibox_selected.set(false);
        } else {
            let mut cur = self.url_input.get();
            cur.push(ch);
            self.url_input.set(cur);
        }
    }

    /// Handles Backspace key in the Omnibox.
    pub fn handle_backspace(&self) -> bool {
        if self.omnibox_selected.get() {
            self.url_input.set(String::new());
            self.omnibox_selected.set(false);
            true
        } else {
            let mut cur = self.url_input.get();
            if !cur.is_empty() {
                cur.pop();
                self.url_input.set(cur);
                true
            } else {
                false
            }
        }
    }

    /// Retrieves a cloned instance of the currently active tab.
    pub fn active_tab(&self) -> Option<BrowserTab> {
        let active_id = self.active_tab_id.get();
        self.tabs.get().into_iter().find(|t| t.id == active_id)
    }

    /// Spawns a new tab with the target URL, appends it, and sets it active.
    pub fn new_tab(&self, target_url: &str) {
        let id = self.next_tab_id.get();
        self.next_tab_id.set(id + 1);

        let new_tab = BrowserTab::new(id, target_url);
        self.tabs.update(|tabs| {
            tabs.push(new_tab);
        });

        self.active_tab_id.set(id);
        self.url_input.set(target_url.to_string());
    }

    /// Closes a tab by ID. If closing the active tab, shifts active focus to an adjacent tab.
    pub fn close_tab(&self, tab_id: usize) {
        let current_tabs = self.tabs.get();
        if current_tabs.len() <= 1 {
            // Keep at least one tab open; reset it to the new tab page
            let reset_url = "strata://newtab";
            let (title, html) = resolve_page(reset_url);
            self.tabs.update(|tabs| {
                if let Some(first) = tabs.first_mut() {
                    first.url = reset_url.to_string();
                    first.title = title;
                    first.html_content = html;
                    first.history = vec![reset_url.to_string()];
                    first.history_index = 0;
                }
            });
            self.url_input.set(reset_url.to_string());
            return;
        }

        let active_id = self.active_tab_id.get();
        let mut new_active_id = active_id;

        if active_id == tab_id {
            if let Some(pos) = current_tabs.iter().position(|t| t.id == tab_id) {
                if pos + 1 < current_tabs.len() {
                    new_active_id = current_tabs[pos + 1].id;
                } else if pos > 0 {
                    new_active_id = current_tabs[pos - 1].id;
                }
            }
        }

        self.tabs.update(|tabs| {
            tabs.retain(|t| t.id != tab_id);
        });

        self.active_tab_id.set(new_active_id);
        self.is_omnibox_editing.set(false);
        self.omnibox_selected.set(false);
        if let Some(new_active) = self.tabs.get().into_iter().find(|t| t.id == new_active_id) {
            self.url_input.set(new_active.url);
        }
    }

    /// Switches the active view to the specified tab.
    pub fn switch_tab(&self, tab_id: usize) {
        self.active_tab_id.set(tab_id);
        self.is_omnibox_editing.set(false);
        self.omnibox_selected.set(false);
        if let Some(tab) = self.tabs.get().into_iter().find(|t| t.id == tab_id) {
            self.url_input.set(tab.url);
        }
    }

    /// Navigates the active tab to a new URL or executes a search query.
    pub fn navigate(&self, input: &str) {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return;
        }

        self.is_omnibox_editing.set(false);
        self.omnibox_selected.set(false);

        let normalized = normalize_target(trimmed);
        let (title, html) = resolve_page(&normalized);
        let active_id = self.active_tab_id.get();

        self.tabs.update(|tabs| {
            if let Some(tab) = tabs.iter_mut().find(|t| t.id == active_id) {
                tab.url = normalized.clone();
                tab.title = title;
                tab.html_content = html;

                // Truncate forward history if navigating from a past index
                if tab.history_index + 1 < tab.history.len() {
                    tab.history.truncate(tab.history_index + 1);
                }
                tab.history.push(normalized.clone());
                tab.history_index = tab.history.len() - 1;
            }
        });

        self.url_input.set(normalized);
    }

    /// Navigates backward in the active tab's history stack.
    pub fn go_back(&self) {
        let active_id = self.active_tab_id.get();
        let mut target_url = None;

        self.tabs.update(|tabs| {
            if let Some(tab) = tabs.iter_mut().find(|t| t.id == active_id) {
                if tab.history_index > 0 {
                    tab.history_index -= 1;
                    let url = tab.history[tab.history_index].clone();
                    let (title, html) = resolve_page(&url);
                    tab.url = url.clone();
                    tab.title = title;
                    tab.html_content = html;
                    target_url = Some(url);
                }
            }
        });

        if let Some(url) = target_url {
            self.url_input.set(url);
        }
    }

    /// Navigates forward in the active tab's history stack.
    pub fn go_forward(&self) {
        let active_id = self.active_tab_id.get();
        let mut target_url = None;

        self.tabs.update(|tabs| {
            if let Some(tab) = tabs.iter_mut().find(|t| t.id == active_id) {
                if tab.history_index + 1 < tab.history.len() {
                    tab.history_index += 1;
                    let url = tab.history[tab.history_index].clone();
                    let (title, html) = resolve_page(&url);
                    tab.url = url.clone();
                    tab.title = title;
                    tab.html_content = html;
                    target_url = Some(url);
                }
            }
        });

        if let Some(url) = target_url {
            self.url_input.set(url);
        }
    }

    /// Reloads the active tab.
    pub fn reload(&self) {
        let active_id = self.active_tab_id.get();
        self.tabs.update(|tabs| {
            if let Some(tab) = tabs.iter_mut().find(|t| t.id == active_id) {
                let (title, html) = resolve_page(&tab.url);
                tab.title = title;
                tab.html_content = html;
            }
        });
    }

    /// Navigates to the home / new tab page.
    pub fn go_home(&self) {
        self.navigate("strata://newtab");
    }

    /// Checks if the active tab can navigate back.
    pub fn can_go_back(&self) -> bool {
        self.active_tab().map_or(false, |t| t.history_index > 0)
    }

    /// Checks if the active tab can navigate forward.
    pub fn can_go_forward(&self) -> bool {
        self.active_tab()
            .map_or(false, |t| t.history_index + 1 < t.history.len())
    }

    /// Sets the active reader appearance theme.
    pub fn set_theme(&self, theme: &str) {
        self.active_theme.set(theme.to_string());
    }

    /// Handles a middle-click anywhere on the tab strip to close the clicked tab.
    pub fn handle_middle_click(&self, x: f32, y: f32) -> bool {
        // Tab bar is in the top header area: y between 0.0 and 42.0
        if y >= 0.0 && y <= 42.0 && x >= 12.0 {
            let tabs = self.tabs.get();
            let tab_width = 176.0 + 4.0; // w-44 (176px) + gap-1 (4px)
            let tab_offset_x = x - 12.0;
            if tab_offset_x >= 0.0 {
                let tab_idx = (tab_offset_x / tab_width) as usize;
                if tab_idx < tabs.len() {
                    self.close_tab(tabs[tab_idx].id);
                    return true;
                }
            }
        }
        false
    }

    /// Toggles DevTools panel visibility.
    pub fn toggle_devtools(&self) {
        let current = self.is_devtools_open.get();
        self.is_devtools_open.set(!current);
    }

    /// Opens DevTools focused on a specific tab.
    pub fn open_devtools(&self, tab: DevToolsTab) {
        self.devtools_tab.set(tab);
        self.is_devtools_open.set(true);
    }

    /// Closes the DevTools panel.
    pub fn close_devtools(&self) {
        self.is_devtools_open.set(false);
        self.devtools_is_inspecting.set(false);
    }

    /// Toggles the inspect element picker tool.
    pub fn toggle_inspect(&self) {
        if !self.is_devtools_open.get() {
            self.is_devtools_open.set(true);
            self.devtools_tab.set(DevToolsTab::Elements);
        }
        let current = self.devtools_is_inspecting.get();
        self.devtools_is_inspecting.set(!current);
    }

    /// Switches the active DevTools tab.
    pub fn set_devtools_tab(&self, tab: DevToolsTab) {
        self.devtools_tab.set(tab);
    }

    /// Selects an element selector for inspection and box model rendering.
    pub fn select_dom_element(&self, selector: &str) {
        self.devtools_selected_element.set(selector.to_string());
    }

    /// Appends a new log entry to the DevTools console stream.
    pub fn add_console_log(&self, level: LogLevel, source: &str, message: &str) {
        let entry = ConsoleLogEntry {
            level,
            timestamp: "14:02:04.112".to_string(),
            source: source.to_string(),
            message: message.to_string(),
        };
        self.devtools_console_logs.update(|logs| logs.push(entry));
    }

    /// Executes an interactive command in the DevTools console REPL.
    pub fn execute_console_command(&self, cmd: &str) {
        let trimmed = cmd.trim();
        if trimmed.is_empty() {
            return;
        }
        self.add_console_log(LogLevel::Verbose, "repl", &format!("> {trimmed}"));
        if trimmed == "clear()" || trimmed == "clear" {
            self.devtools_console_logs.set(Vec::new());
        } else if trimmed == "help" {
            self.add_console_log(LogLevel::Info, "repl", "Available: strata.version, strata.tabs, strata.reload(), strata.navigate(url), clear()");
        } else if trimmed == "strata.version" {
            self.add_console_log(LogLevel::Info, "repl", "\"0.1.0 (Zero-Chromium Pure Rust LayoutNG)\"");
        } else if trimmed == "strata.tabs" {
            let count = self.tabs.get().len();
            self.add_console_log(LogLevel::Info, "repl", &format!("Array({count}) [BrowserTab]"));
        } else if trimmed.starts_with("strata.navigate(") {
            let url = trimmed.trim_start_matches("strata.navigate(").trim_end_matches(')').trim_matches('"').trim_matches('\'');
            self.navigate(url);
            self.add_console_log(LogLevel::Info, "repl", &format!("Navigated to {url}"));
        } else if trimmed == "strata.reload()" {
            self.reload();
            self.add_console_log(LogLevel::Info, "repl", "Tab reloaded");
        } else {
            self.add_console_log(LogLevel::Info, "repl", &format!("Result: {trimmed} => OK"));
        }
        self.devtools_console_input.set(String::new());
    }
}

fn normalize_target(input: &str) -> String {
    if input == "strata://newtab" || input == "about:blank" || input == "strata://about" {
        input.to_string()
    } else if input.starts_with("http://") || input.starts_with("https://") {
        input.to_string()
    } else if input.contains('.') && !input.contains(' ') {
        format!("https://{input}")
    } else {
        // Query search
        let encoded = input.replace(' ', "+");
        format!("https://www.google.com/search?q={encoded}")
    }
}
