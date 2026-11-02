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

/// Global browser state containing open tabs, active tab pointer, and navigation inputs.
#[derive(Clone)]
pub struct BrowserState {
    pub next_tab_id: Signal<usize>,
    pub tabs: Signal<Vec<BrowserTab>>,
    pub active_tab_id: Signal<usize>,
    pub url_input: Signal<String>,
    pub search_input: Signal<String>,
    pub active_theme: Signal<String>,
}

impl BrowserState {
    pub fn new() -> Self {
        let initial_url = "strata://newtab";
        let initial_tab = BrowserTab::new(1, initial_url);

        Self {
            next_tab_id: Signal::new(2),
            tabs: Signal::new(vec![initial_tab]),
            active_tab_id: Signal::new(1),
            url_input: Signal::new(initial_url.to_string()),
            search_input: Signal::new(String::new()),
            active_theme: Signal::new("dark".to_string()),
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
        if let Some(new_active) = self.tabs.get().into_iter().find(|t| t.id == new_active_id) {
            self.url_input.set(new_active.url);
        }
    }

    /// Switches the active view to the specified tab.
    pub fn switch_tab(&self, tab_id: usize) {
        self.active_tab_id.set(tab_id);
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
        // Window traffic lights take the first ~60px
        if y >= 0.0 && y <= 42.0 && x > 60.0 {
            let tabs = self.tabs.get();
            let tab_width = 176.0 + 4.0; // w-44 (176px) + gap-1 (4px)
            let tab_offset_x = x - 64.0;
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
