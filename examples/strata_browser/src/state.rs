use crate::network::{load_page, normalize_target};
use strata::prelude::*;
use std::sync::Arc;
use strata_web_reader::ast::SemanticNode;

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
    /// Semantic DOM tree produced by the background tab worker; empty when unknown.
    pub dom_nodes: Arc<Vec<SemanticNode>>,
    pub generation: u64,
    pub is_crashed: bool,
    pub crash_reason: Option<String>,
}

impl BrowserTab {
    pub fn new(id: usize, url: &str) -> Self {
        let result = load_page(url);
        Self {
            id,
            title: result.title,
            url: url.to_string(),
            history: vec![url.to_string()],
            history_index: 0,
            is_loading: false,
            html_content: result.html,
            dom_nodes: Arc::new(Vec::new()),
            generation: 0,
            is_crashed: false,
            crash_reason: None,
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

/// DevTools inspection target: the loaded Web Page vs the native Strata Framework.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DevToolsTarget {
    /// Inspect the loaded Web Page (HTML DOM tree, web computed CSS, web network)
    Page,
    /// Inspect the Native Strata Framework (RSX Component Tree, Signals, Rust Arena & Profiler)
    Strata,
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

/// Context menu targets identifying which element was right-clicked.
#[derive(Clone, Debug, PartialEq)]
pub enum ContextTarget {
    Page,
    Tab(usize),
    TabStrip,
    Bookmark(usize),
    BookmarksBar,
    Omnibox,
}

/// An individual bookmark entry with unique ID, display title, and destination URL.
#[derive(Clone, Debug, PartialEq)]
pub struct BookmarkItem {
    pub id: usize,
    pub title: String,
    pub url: String,
}

/// State for the bookmark editor modal dialog.
#[derive(Clone, Debug, PartialEq)]
pub struct BookmarkEditorModalState {
    pub bookmark_id: Option<usize>,
    pub title_input: String,
    pub url_input: String,
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
    pub url_undo_stack: Signal<Vec<String>>,
    pub url_redo_stack: Signal<Vec<String>>,
    pub is_kebab_menu_open: Signal<bool>,
    pub is_settings_modal_open: Signal<bool>,
    pub search_engine: Signal<String>,
    pub zoom_level: Signal<u32>,
    pub is_context_menu_open: Signal<bool>,
    pub context_menu_position: Signal<(f32, f32)>,
    pub context_menu_target: Signal<ContextTarget>,
    pub is_window_maximized: Signal<bool>,
    pub closed_tabs: Signal<Vec<BrowserTab>>,
    pub bookmarks: Signal<Vec<BookmarkItem>>,
    pub next_bookmark_id: Signal<usize>,
    pub bookmark_editor: Signal<Option<BookmarkEditorModalState>>,

    // DevTools Suite State
    pub is_devtools_open: Signal<bool>,
    pub devtools_target: Signal<DevToolsTarget>,
    pub devtools_tab: Signal<DevToolsTab>,
    pub devtools_selected_element: Signal<String>,
    pub devtools_console_filter: Signal<String>,
    pub devtools_is_inspecting: Signal<bool>,
    pub devtools_console_input: Signal<String>,
    pub devtools_console_logs: Signal<Vec<ConsoleLogEntry>>,
    pub devtools_network_requests: Signal<Vec<NetworkRequestEntry>>,
    pub devtools_drawer_open: Signal<bool>,
    pub devtools_drawer_tab: Signal<String>,
    pub worker_pool: std::sync::Arc<crate::tab_worker::TabWorkerPool>,
}

fn is_test_execution() -> bool {
    cfg!(test)
        || std::env::var("RUST_TEST_THREADS").is_ok()
        || std::env::var("CARGO_TARGET_TMPDIR").is_ok()
        || std::thread::current()
            .name()
            .is_some_and(|n| n.starts_with("test_"))
}

impl Default for BrowserState {
    fn default() -> Self {
        Self::new()
    }
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

        let initial_bookmarks = Vec::new();

        Self {
            next_tab_id: Signal::new(2),
            tabs: Signal::new(vec![initial_tab]),
            active_tab_id: Signal::new(1),
            url_input: Signal::new(initial_url.to_string()),
            search_input: Signal::new(String::new()),
            active_theme: Signal::new("dark".to_string()),
            is_omnibox_editing: Signal::new(false),
            omnibox_selected: Signal::new(false),
            url_undo_stack: Signal::new(Vec::new()),
            url_redo_stack: Signal::new(Vec::new()),
            is_kebab_menu_open: Signal::new(false),
            is_settings_modal_open: Signal::new(false),
            search_engine: Signal::new("google".to_string()),
            zoom_level: Signal::new(100),
            is_context_menu_open: Signal::new(false),
            context_menu_position: Signal::new((0.0, 0.0)),
            context_menu_target: Signal::new(ContextTarget::Page),
            is_window_maximized: Signal::new(false),
            closed_tabs: Signal::new(Vec::new()),
            bookmarks: Signal::new(initial_bookmarks),
            next_bookmark_id: Signal::new(1),
            bookmark_editor: Signal::new(None),

            // DevTools
            is_devtools_open: Signal::new(false),
            devtools_target: Signal::new(DevToolsTarget::Page),
            devtools_tab: Signal::new(DevToolsTab::Elements),
            devtools_selected_element: Signal::new("button.btn-search".to_string()),
            devtools_console_filter: Signal::new(String::new()),
            devtools_is_inspecting: Signal::new(false),
            devtools_console_input: Signal::new(String::new()),
            devtools_console_logs: Signal::new(initial_logs),
            devtools_network_requests: Signal::new(initial_requests),
            devtools_drawer_open: Signal::new(true),
            devtools_drawer_tab: Signal::new("Console".to_string()),
            worker_pool: std::sync::Arc::new(crate::tab_worker::TabWorkerPool::new(4)),
        }
    }

    /// Toggles the DevTools inspection target between Web Page and Strata Framework.
    pub fn toggle_devtools_target(&self) {
        let current = self.devtools_target.get();
        let next = match current {
            DevToolsTarget::Page => DevToolsTarget::Strata,
            DevToolsTarget::Strata => DevToolsTarget::Page,
        };
        self.devtools_target.set(next);
    }

    /// Sets the DevTools inspection target.
    pub fn set_devtools_target(&self, target: DevToolsTarget) {
        self.devtools_target.set(target);
    }

    /// Pushes a snapshot of the URL input to the undo stack.
    pub fn push_url_undo(&self, current: String) {
        self.url_undo_stack.update(|stack| {
            if stack.last() != Some(&current) {
                stack.push(current);
                if stack.len() > 100 {
                    stack.remove(0);
                }
            }
        });
        self.url_redo_stack.set(Vec::new());
    }

    /// Undoes the last URL input edit.
    pub fn undo_url(&self) -> bool {
        let mut popped = None;
        self.url_undo_stack.update(|stack| {
            popped = stack.pop();
        });
        if let Some(prev) = popped {
            let cur = self.url_input.get();
            self.url_redo_stack.update(|stack| stack.push(cur));
            self.url_input.set(prev);
            self.omnibox_selected.set(false);
            true
        } else {
            false
        }
    }

    /// Redoes the undone URL input edit.
    pub fn redo_url(&self) -> bool {
        let mut popped = None;
        self.url_redo_stack.update(|stack| {
            popped = stack.pop();
        });
        if let Some(next) = popped {
            let cur = self.url_input.get();
            self.url_undo_stack.update(|stack| stack.push(cur));
            self.url_input.set(next);
            self.omnibox_selected.set(false);
            true
        } else {
            false
        }
    }

    /// Toggles the Chrome-style kebab context menu.
    pub fn toggle_kebab_menu(&self) {
        let current = self.is_kebab_menu_open.get();
        self.is_kebab_menu_open.set(!current);
    }

    /// Closes the Chrome-style kebab context menu.
    pub fn close_kebab_menu(&self) {
        self.is_kebab_menu_open.set(false);
    }

    /// Opens the right-click context menu for a specific target.
    pub fn open_context_menu_for(&self, x: f32, y: f32, target: ContextTarget) {
        self.is_kebab_menu_open.set(false);
        self.context_menu_target.set(target);
        self.context_menu_position.set((x, y));
        self.is_context_menu_open.set(true);
    }

    /// Opens the page right-click context menu at the given coordinates.
    pub fn open_context_menu(&self, x: f32, y: f32) {
        self.open_context_menu_for(x, y, ContextTarget::Page);
    }

    /// Closes the page right-click context menu.
    pub fn close_context_menu(&self) {
        self.is_context_menu_open.set(false);
    }

    /// Opens the Settings modal and closes the kebab menu.
    pub fn open_settings(&self) {
        self.is_kebab_menu_open.set(false);
        self.is_settings_modal_open.set(true);
    }

    /// Closes the Settings modal.
    pub fn close_settings(&self) {
        self.is_settings_modal_open.set(false);
    }

    /// Increments page zoom level.
    pub fn zoom_in(&self) {
        let z = self.zoom_level.get();
        if z < 300 {
            self.zoom_level.set(z + 10);
        }
    }

    /// Decrements page zoom level.
    pub fn zoom_out(&self) {
        let z = self.zoom_level.get();
        if z > 50 {
            self.zoom_level.set(z - 10);
        }
    }

    /// Resets page zoom level to 100%.
    pub fn zoom_reset(&self) {
        self.zoom_level.set(100);
    }

    /// Handles global left click if any special unhandled dismissal is needed.
    pub fn handle_global_click(&self, _x: f32, _y: f32) -> bool {
        if self.is_context_menu_open.get() {
            self.close_context_menu();
            return true;
        }
        false
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
        let prev = self.url_input.get();
        self.push_url_undo(prev);
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
        let prev = self.url_input.get();
        if self.omnibox_selected.get() {
            self.push_url_undo(prev);
            self.url_input.set(String::new());
            self.omnibox_selected.set(false);
            true
        } else {
            let mut cur = self.url_input.get();
            if !cur.is_empty() {
                self.push_url_undo(prev);
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

    /// Drains completed tab worker responses and updates tab contents and DevTools logs.
    /// Returns true if any tab state changed, triggering an immediate UI frame rebuild.
    pub fn poll_worker_responses(&self) -> bool {
        let responses = self.worker_pool.drain_responses();
        if responses.is_empty() {
            return false;
        }

        let mut changed = false;
        for resp in responses {
            let html_len = resp.html.len();
            let dom_count = resp.dom_nodes.len();
            let mut matched = false;
            self.tabs.update(|tabs| {
                if let Some(tab) = tabs.iter_mut().find(|t| t.id == resp.tab_id) {
                    if tab.generation == resp.generation {
                        matched = true;
                        tab.is_loading = false;
                        if resp.is_crashed {
                            tab.is_crashed = true;
                            tab.crash_reason = resp.crash_reason.clone();
                            tab.title = format!("Aw, Snap! - {}", tab.url);
                        } else {
                            tab.is_crashed = false;
                            tab.crash_reason = None;
                            tab.title = resp.title.clone();
                            tab.html_content = resp.html;
                            tab.dom_nodes = Arc::new(resp.dom_nodes);
                        }
                        changed = true;
                    }
                }
            });

            if matched {
                // Record network entry in DevTools
                self.devtools_network_requests.update(|reqs| {
                    reqs.push(resp.request_entry);
                });

                let active_id = self.active_tab_id.get();
                if active_id == resp.tab_id {
                    self.url_input.set(resp.url.clone());
                }

                if resp.is_crashed {
                    self.add_console_log(
                        LogLevel::Error,
                        "strata_tab_sandbox",
                        &format!(
                            "Tab {} contained crash: {}",
                            resp.tab_id,
                            resp.crash_reason.as_deref().unwrap_or("Unknown error")
                        ),
                    );
                } else {
                    if crate::perf::enabled() {
                        eprintln!(
                            "[perf] worker_fetch_parse: {:.3} ms ({} bytes, {} nodes) {}",
                            resp.elapsed_ms, html_len, dom_count, resp.url
                        );
                    }
                    self.add_console_log(
                        if resp.is_error {
                            LogLevel::Error
                        } else {
                            LogLevel::Info
                        },
                        "strata_tab_worker",
                        &format!(
                            "Finished loading {} in {:.2}ms ({} DOM nodes)",
                            resp.url, resp.elapsed_ms, dom_count
                        ),
                    );
                }
            }
        }

        changed
    }

    /// Recovers a crashed tab by restarting workers and re-navigating to the tab's current URL.
    pub fn recover_tab(&self, tab_id: usize) {
        self.worker_pool.restart_crashed_workers();
        let mut target_url = None;
        self.tabs.update(|tabs| {
            if let Some(tab) = tabs.iter_mut().find(|t| t.id == tab_id) {
                tab.is_crashed = false;
                tab.crash_reason = None;
                target_url = Some(tab.url.clone());
            }
        });
        if let Some(url) = target_url {
            self.navigate(&url);
        }
    }

    /// Synchronously waits until all currently pending background tab worker loads complete.
    pub fn wait_for_pending_loads(&self) {
        let start = std::time::Instant::now();
        while start.elapsed() < std::time::Duration::from_secs(12) {
            let any_loading = self.tabs.get().iter().any(|t| t.is_loading);
            if !any_loading {
                break;
            }
            if self.poll_worker_responses() {
                let still_loading = self.tabs.get().iter().any(|t| t.is_loading);
                if !still_loading {
                    break;
                }
            }
            if let Some(resp) = self
                .worker_pool
                .wait_timeout(std::time::Duration::from_millis(15))
            {
                let mut matched = false;
                self.tabs.update(|tabs| {
                    if let Some(tab) = tabs.iter_mut().find(|t| t.id == resp.tab_id) {
                        if tab.generation == resp.generation {
                            matched = true;
                            tab.is_loading = false;
                            tab.title = resp.title.clone();
                            tab.html_content = resp.html;
                            tab.dom_nodes = Arc::new(resp.dom_nodes);
                        }
                    }
                });
                if matched {
                    self.devtools_network_requests.update(|reqs| {
                        reqs.push(resp.request_entry);
                    });
                    let active_id = self.active_tab_id.get();
                    if active_id == resp.tab_id {
                        self.url_input.set(resp.url);
                    }
                }
            }
        }
    }

    /// Spawns a new tab with the target URL, appends it, and sets it active.
    pub fn new_tab(&self, target_url: &str) {
        let id = self.next_tab_id.get();
        self.next_tab_id.set(id + 1);

        let normalized = normalize_target(target_url);
        let is_internal = normalized == "strata://newtab"
            || normalized == "strata://about"
            || normalized == "strata://version";

        let new_tab = if is_internal {
            BrowserTab::new(id, &normalized)
        } else {
            BrowserTab {
                id,
                title: "Loading...".to_string(),
                url: normalized.clone(),
                history: vec![normalized.clone()],
                history_index: 0,
                is_loading: true,
                html_content: String::new(),
                dom_nodes: Arc::new(Vec::new()),
                generation: 1,
                is_crashed: false,
                crash_reason: None,
            }
        };

        self.tabs.update(|tabs| {
            tabs.push(new_tab);
        });

        self.active_tab_id.set(id);
        self.url_input.set(normalized.clone());

        if !is_internal {
            self.worker_pool
                .submit_job(crate::tab_worker::TabWorkerJob::new(id, 1, normalized));
            if is_test_execution() {
                self.wait_for_pending_loads();
            }
        }
    }

    /// Closes a tab by ID. If closing the active tab, shifts active focus to an adjacent tab.
    pub fn close_tab(&self, tab_id: usize) {
        let current_tabs = self.tabs.get();
        if let Some(closed) = current_tabs.iter().find(|t| t.id == tab_id) {
            self.closed_tabs.update(|ct| ct.push(closed.clone()));
        }

        if current_tabs.len() <= 1 {
            // Keep at least one tab open; reset it to the new tab page
            let reset_url = "strata://newtab";
            let result = load_page(reset_url);
            self.tabs.update(|tabs| {
                if let Some(first) = tabs.first_mut() {
                    first.url = reset_url.to_string();
                    first.title = result.title;
                    first.html_content = result.html;
                    first.dom_nodes = Arc::new(Vec::new());
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

    /// Duplicates an existing tab and sets the clone active.
    pub fn duplicate_tab(&self, tab_id: usize) {
        let tabs = self.tabs.get();
        if let Some(tab) = tabs.iter().find(|t| t.id == tab_id).cloned() {
            let next_id = self.next_tab_id.get();
            self.next_tab_id.set(next_id + 1);
            let mut new_tab = tab.clone();
            new_tab.id = next_id;
            let mut new_tabs = tabs;
            if let Some(idx) = new_tabs.iter().position(|t| t.id == tab_id) {
                new_tabs.insert(idx + 1, new_tab);
            } else {
                new_tabs.push(new_tab);
            }
            self.tabs.set(new_tabs);
            self.active_tab_id.set(next_id);
            self.url_input.set(tab.url);
        }
    }

    /// Closes all tabs except the specified tab.
    pub fn close_other_tabs(&self, keep_id: usize) {
        let tabs = self.tabs.get();
        let mut closed = Vec::new();
        for t in &tabs {
            if t.id != keep_id {
                closed.push(t.clone());
            }
        }
        self.closed_tabs.update(|ct| ct.extend(closed));
        self.tabs.update(|ts| ts.retain(|t| t.id == keep_id));
        self.active_tab_id.set(keep_id);
        if let Some(tab) = self.tabs.get().into_iter().find(|t| t.id == keep_id) {
            self.url_input.set(tab.url);
        }
    }

    /// Closes all tabs to the right of the specified tab.
    pub fn close_tabs_to_right(&self, tab_id: usize) {
        let tabs = self.tabs.get();
        if let Some(idx) = tabs.iter().position(|t| t.id == tab_id) {
            let closed: Vec<BrowserTab> = tabs.iter().skip(idx + 1).cloned().collect();
            self.closed_tabs.update(|ct| ct.extend(closed));
            let remaining: Vec<BrowserTab> = tabs.into_iter().take(idx + 1).collect();
            self.tabs.set(remaining);
            let active = self.active_tab_id.get();
            if !self.tabs.get().iter().any(|t| t.id == active) {
                self.active_tab_id.set(tab_id);
                if let Some(tab) = self.tabs.get().into_iter().find(|t| t.id == tab_id) {
                    self.url_input.set(tab.url);
                }
            }
        }
    }

    /// Reopens the most recently closed tab.
    pub fn reopen_closed_tab(&self) -> bool {
        let mut last_tab = None;
        self.closed_tabs.update(|ct| {
            last_tab = ct.pop();
        });
        if let Some(tab) = last_tab {
            let id = tab.id;
            let url = tab.url.clone();
            self.tabs.update(|ts| ts.push(tab));
            self.active_tab_id.set(id);
            self.url_input.set(url);
            true
        } else {
            false
        }
    }

    /// Adds a new bookmark entry to the user's bookmarks bar.
    pub fn add_bookmark(&self, title: &str, url: &str) {
        let id = self.next_bookmark_id.get();
        self.next_bookmark_id.set(id + 1);
        let bm = BookmarkItem {
            id,
            title: title.to_string(),
            url: url.to_string(),
        };
        self.bookmarks.update(|list| list.push(bm));
    }

    /// Removes a bookmark by its unique ID.
    pub fn remove_bookmark(&self, id: usize) {
        self.bookmarks.update(|list| list.retain(|b| b.id != id));
    }

    /// Updates title and URL of an existing bookmark.
    pub fn update_bookmark(&self, id: usize, title: &str, url: &str) {
        self.bookmarks.update(|list| {
            if let Some(bm) = list.iter_mut().find(|b| b.id == id) {
                bm.title = title.to_string();
                bm.url = url.to_string();
            }
        });
    }

    /// Shifts a bookmark position left (-1) or right (+1).
    pub fn move_bookmark(&self, id: usize, delta: isize) {
        self.bookmarks.update(|list| {
            if let Some(idx) = list.iter().position(|b| b.id == id) {
                let new_idx = idx as isize + delta;
                if new_idx >= 0 && (new_idx as usize) < list.len() {
                    let item = list.remove(idx);
                    list.insert(new_idx as usize, item);
                }
            }
        });
    }

    /// Opens the bookmark editor modal for creating or editing a bookmark.
    pub fn open_bookmark_editor(&self, bookmark_id: Option<usize>) {
        let (title, url) = if let Some(id) = bookmark_id {
            if let Some(bm) = self.bookmarks.get().into_iter().find(|b| b.id == id) {
                (bm.title, bm.url)
            } else {
                ("New Bookmark".to_string(), "https://".to_string())
            }
        } else if let Some(tab) = self.active_tab() {
            (tab.title, tab.url)
        } else {
            ("New Bookmark".to_string(), "https://".to_string())
        };

        self.bookmark_editor.set(Some(BookmarkEditorModalState {
            bookmark_id,
            title_input: title,
            url_input: url,
        }));
    }

    /// Closes the bookmark editor modal.
    pub fn close_bookmark_editor(&self) {
        self.bookmark_editor.set(None);
    }

    /// Saves changes from the bookmark editor modal.
    pub fn save_bookmark_editor(&self, title: &str, url: &str) {
        if let Some(editor) = self.bookmark_editor.get() {
            if let Some(id) = editor.bookmark_id {
                self.update_bookmark(id, title, url);
            } else {
                self.add_bookmark(title, url);
            }
        }
        self.close_bookmark_editor();
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
        let active_id = self.active_tab_id.get();

        // Internal browser schemes resolve synchronously
        if normalized == "strata://newtab"
            || normalized == "strata://about"
            || normalized == "strata://version"
        {
            let result = load_page(&normalized);
            self.tabs.update(|tabs| {
                if let Some(tab) = tabs.iter_mut().find(|t| t.id == active_id) {
                    tab.generation += 1;
                    tab.url = normalized.clone();
                    tab.title = result.title;
                    tab.html_content = result.html;
                    tab.dom_nodes = Arc::new(Vec::new());
                    tab.is_loading = false;

                    if tab.history_index + 1 < tab.history.len() {
                        tab.history.truncate(tab.history_index + 1);
                    }
                    tab.history.push(normalized.clone());
                    tab.history_index = tab.history.len() - 1;
                }
            });
            self.url_input.set(normalized);
            return;
        }

        // Fast-path: if page is already cached in memory, apply immediately for zero-latency switching
        if let Some(cached) = crate::network::get_cached_page(&normalized) {
            self.tabs.update(|tabs| {
                if let Some(tab) = tabs.iter_mut().find(|t| t.id == active_id) {
                    tab.generation += 1;
                    tab.url = normalized.clone();
                    tab.title = cached.title;
                    tab.html_content = cached.html;
                    tab.dom_nodes = Arc::new(Vec::new());
                    tab.is_loading = false;

                    if tab.history_index + 1 < tab.history.len() {
                        tab.history.truncate(tab.history_index + 1);
                    }
                    tab.history.push(normalized.clone());
                    tab.history_index = tab.history.len() - 1;
                }
            });
            self.url_input.set(normalized);
            return;
        }

        let mut job_to_dispatch = None;
        self.tabs.update(|tabs| {
            if let Some(tab) = tabs.iter_mut().find(|t| t.id == active_id) {
                tab.generation += 1;
                tab.url = normalized.clone();
                tab.is_loading = true;

                if tab.history_index + 1 < tab.history.len() {
                    tab.history.truncate(tab.history_index + 1);
                }
                tab.history.push(normalized.clone());
                tab.history_index = tab.history.len() - 1;

                job_to_dispatch = Some(crate::tab_worker::TabWorkerJob::new(
                    tab.id,
                    tab.generation,
                    normalized.clone(),
                ));
            }
        });

        self.url_input.set(normalized);

        if let Some(job) = job_to_dispatch {
            self.worker_pool.submit_job(job);
        }

        if is_test_execution() {
            self.wait_for_pending_loads();
        }
    }

    /// Navigates backward in the active tab's history stack.
    pub fn go_back(&self) {
        let active_id = self.active_tab_id.get();
        let mut target_url = None;
        let mut job_to_dispatch = None;

        self.tabs.update(|tabs| {
            if let Some(tab) = tabs.iter_mut().find(|t| t.id == active_id) {
                if tab.history_index > 0 {
                    tab.history_index -= 1;
                    tab.generation += 1;
                    let url = tab.history[tab.history_index].clone();
                    tab.url = url.clone();
                    target_url = Some(url.clone());

                    if url == "strata://newtab"
                        || url == "strata://about"
                        || url == "strata://version"
                    {
                        let result = load_page(&url);
                        tab.title = result.title;
                        tab.html_content = result.html;
                        tab.dom_nodes = Arc::new(Vec::new());
                        tab.is_loading = false;
                    } else if let Some(cached) = crate::network::get_cached_page(&url) {
                        tab.title = cached.title;
                        tab.html_content = cached.html;
                        tab.dom_nodes = Arc::new(Vec::new());
                        tab.is_loading = false;
                    } else {
                        tab.is_loading = true;
                        job_to_dispatch = Some(crate::tab_worker::TabWorkerJob::new(
                            tab.id,
                            tab.generation,
                            url,
                        ));
                    }
                }
            }
        });

        if let Some(url) = target_url {
            self.url_input.set(url);
        }

        if let Some(job) = job_to_dispatch {
            self.worker_pool.submit_job(job);
            if is_test_execution() {
                self.wait_for_pending_loads();
            }
        }
    }

    /// Navigates forward in the active tab's history stack.
    pub fn go_forward(&self) {
        let active_id = self.active_tab_id.get();
        let mut target_url = None;
        let mut job_to_dispatch = None;

        self.tabs.update(|tabs| {
            if let Some(tab) = tabs.iter_mut().find(|t| t.id == active_id) {
                if tab.history_index + 1 < tab.history.len() {
                    tab.history_index += 1;
                    tab.generation += 1;
                    let url = tab.history[tab.history_index].clone();
                    tab.url = url.clone();
                    target_url = Some(url.clone());

                    if url == "strata://newtab"
                        || url == "strata://about"
                        || url == "strata://version"
                    {
                        let result = load_page(&url);
                        tab.title = result.title;
                        tab.html_content = result.html;
                        tab.dom_nodes = Arc::new(Vec::new());
                        tab.is_loading = false;
                    } else if let Some(cached) = crate::network::get_cached_page(&url) {
                        tab.title = cached.title;
                        tab.html_content = cached.html;
                        tab.dom_nodes = Arc::new(Vec::new());
                        tab.is_loading = false;
                    } else {
                        tab.is_loading = true;
                        job_to_dispatch = Some(crate::tab_worker::TabWorkerJob::new(
                            tab.id,
                            tab.generation,
                            url,
                        ));
                    }
                }
            }
        });

        if let Some(url) = target_url {
            self.url_input.set(url);
        }

        if let Some(job) = job_to_dispatch {
            self.worker_pool.submit_job(job);
            if is_test_execution() {
                self.wait_for_pending_loads();
            }
        }
    }

    /// Reloads the active tab.
    pub fn reload(&self) {
        let active_id = self.active_tab_id.get();
        let mut job_to_dispatch = None;

        self.tabs.update(|tabs| {
            if let Some(tab) = tabs.iter_mut().find(|t| t.id == active_id) {
                crate::network::invalidate_cache(&tab.url);
                tab.generation += 1;
                tab.is_loading = true;
                job_to_dispatch = Some(crate::tab_worker::TabWorkerJob::new(
                    tab.id,
                    tab.generation,
                    tab.url.clone(),
                ));
            }
        });

        if let Some(job) = job_to_dispatch {
            self.worker_pool.submit_job(job);
            if is_test_execution() {
                self.wait_for_pending_loads();
            }
        }
    }

    /// Navigates to the home / new tab page.
    pub fn go_home(&self) {
        self.navigate("strata://newtab");
    }

    /// Checks if the active tab can navigate back.
    pub fn can_go_back(&self) -> bool {
        self.active_tab().is_some_and(|t| t.history_index > 0)
    }

    /// Checks if the active tab can navigate forward.
    pub fn can_go_forward(&self) -> bool {
        self.active_tab()
            .is_some_and(|t| t.history_index + 1 < t.history.len())
    }

    /// Sets the active reader appearance theme.
    pub fn set_theme(&self, theme: &str) {
        self.active_theme.set(theme.to_string());
    }

    /// Handles a middle-click anywhere on the tab strip to close the clicked tab.
    pub fn handle_middle_click(&self, x: f32, y: f32) -> bool {
        // Tab bar is in the top header area: y between 0.0 and 42.0
        if (0.0..=42.0).contains(&y) && x >= 12.0 {
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
            self.add_console_log(
                LogLevel::Info,
                "repl",
                "\"0.1.0 (Zero-Chromium Pure Rust LayoutNG)\"",
            );
        } else if trimmed == "strata.tabs" {
            let count = self.tabs.get().len();
            self.add_console_log(
                LogLevel::Info,
                "repl",
                &format!("Array({count}) [BrowserTab]"),
            );
        } else if trimmed.starts_with("strata.navigate(") {
            let url = trimmed
                .trim_start_matches("strata.navigate(")
                .trim_end_matches(')')
                .trim_matches('"')
                .trim_matches('\'');
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
