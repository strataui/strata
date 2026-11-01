//! Reactive `<WebView />` component, controller, and event forwarder.

use strata_dom::{Attribute, NodeBlueprint, Tag};
use crate::ipc::{ChromiumCommand, KeyAction, Modifiers, MouseAction, MouseButton, NavigationState};
use crate::process::{ChromiumProcessManager, ProcessState};
use crate::surface::ShmSurface;

/// Configuration options for launching a `<WebView />` session.
#[derive(Debug, Clone, PartialEq)]
pub struct WebViewConfig {
    /// Initial URL to load on component mount.
    pub initial_url: String,
    /// Optional custom User-Agent header override.
    pub user_agent: Option<String>,
    /// Whether JavaScript execution is allowed.
    pub enable_javascript: bool,
    /// Whether cookie capture and session persistence are enabled.
    pub allow_cookies: bool,
    /// Custom HTTP request headers.
    pub custom_headers: Vec<(String, String)>,
}

impl Default for WebViewConfig {
    fn default() -> Self {
        Self {
            initial_url: "about:blank".to_string(),
            user_agent: None,
            enable_javascript: true,
            allow_cookies: true,
            custom_headers: Vec::new(),
        }
    }
}

/// Controller handle for programmatically interacting with an embedded browser instance.
#[derive(Clone)]
pub struct WebViewController {
    config: WebViewConfig,
    manager: ChromiumProcessManager,
}

impl WebViewController {
    /// Creates and mounts a new WebView instance.
    pub fn new(config: WebViewConfig, width: u32, height: u32) -> Self {
        let manager = ChromiumProcessManager::new(width, height);
        manager.mount(&config.initial_url);

        Self { config, manager }
    }

    /// Accesses the underlying process manager.
    pub fn manager(&self) -> &ChromiumProcessManager {
        &self.manager
    }

    /// Accesses the shared memory raster surface.
    pub fn surface(&self) -> ShmSurface {
        self.manager.surface()
    }

    /// Returns the current lifecycle state of the browser worker.
    pub fn state(&self) -> ProcessState {
        self.manager.state()
    }

    /// Returns the current URL navigation status.
    pub fn navigation_state(&self) -> NavigationState {
        self.manager.navigation_state()
    }

    /// Directs the browser to navigate to a new URL.
    pub fn navigate(&self, url: impl Into<String>) {
        self.manager.send_command(ChromiumCommand::Navigate {
            url: url.into(),
        });
    }

    /// Reloads the current page.
    pub fn reload(&self) {
        self.manager.send_command(ChromiumCommand::Reload);
    }

    /// Evaluates JavaScript within the page context.
    pub fn execute_js(&self, script: impl Into<String>) {
        self.manager.send_command(ChromiumCommand::ExecuteJs {
            request_id: 1,
            script: script.into(),
        });
    }

    /// Forwards mouse cursor movement or click interactions.
    pub fn forward_mouse(
        &self,
        action: MouseAction,
        x: f32,
        y: f32,
        button: MouseButton,
        modifiers: Modifiers,
    ) {
        self.manager.send_command(ChromiumCommand::MouseEvent {
            action,
            x,
            y,
            button,
            modifiers,
            delta_x: 0.0,
            delta_y: 0.0,
        });
    }

    /// Forwards keyboard interaction.
    pub fn forward_key(
        &self,
        action: KeyAction,
        key_code: u32,
        text: Option<String>,
        modifiers: Modifiers,
    ) {
        self.manager.send_command(ChromiumCommand::KeyEvent {
            action,
            key_code,
            text,
            modifiers,
        });
    }

    /// Generates a reactive `NodeBlueprint` representing the WebView surface.
    pub fn render_blueprint(&self, class_style: &str) -> NodeBlueprint {
        let mut classes = vec![
            "relative overflow-hidden bg-slate-900 border border-slate-800 rounded-lg".to_string(),
        ];
        if !class_style.is_empty() {
            classes.push(class_style.to_string());
        }

        let ctrl = self.clone();

        NodeBlueprint::element(
            Tag::Div,
            classes,
            vec![
                Attribute::property("data-component", "strata-webview"),
                Attribute::property("data-url", self.config.initial_url.clone()),
                Attribute::event_listener(strata_dom::EventType::Click, move |_| {
                    ctrl.forward_mouse(
                        MouseAction::Down,
                        0.0,
                        0.0,
                        MouseButton::Left,
                        Modifiers::default(),
                    );
                }),
            ],
            vec![
                // Canvas or OSR presentation placeholder
                NodeBlueprint::element(
                    Tag::Canvas,
                    vec!["w-full h-full block".to_string()],
                    Vec::new(),
                    Vec::new(),
                ),
            ],
        )
    }
}
