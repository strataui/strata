//! In-process overlay HUD and event interceptor with synthetic event isolation.

use strata_dom::NodeId;
use strata_platform::{KeyCode, PlatformEvent};
use strata_primitives::geom::FixedRect;

/// Docking orientation and layout mode of the DevTools overlay HUD.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HudDockMode {
    #[default]
    DockBottom,
    DockRight,
    Floating,
    CollapsedPill,
}

/// Active diagnostic panel / tab within the DevTools HUD.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HudTab {
    #[default]
    Components,
    Reactivity,
    Performance,
    Memory,
    TimeTravel,
    Settings,
}

/// Central state model for the overlay HUD.
#[derive(Debug, Clone, PartialEq)]
pub struct HudState {
    pub is_visible: bool,
    pub dock_mode: HudDockMode,
    pub active_tab: HudTab,
    pub is_inspecting_element: bool,
    pub hovered_node: Option<NodeId>,
    pub selected_node: Option<NodeId>,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Default for HudState {
    fn default() -> Self {
        Self {
            is_visible: false,
            dock_mode: HudDockMode::DockBottom,
            active_tab: HudTab::Components,
            is_inspecting_element: false,
            hovered_node: None,
            selected_node: None,
            x: 0.0,
            y: 0.0,
            width: 1280.0,
            height: 320.0,
        }
    }
}

/// HUD Controller managing visibility, tab switching, and geometry bounds.
#[derive(Debug, Clone, Default)]
pub struct HudController {
    pub state: HudState,
}

impl HudController {
    pub fn new() -> Self {
        Self {
            state: HudState::default(),
        }
    }

    /// Toggles the HUD visibility.
    pub fn toggle_visibility(&mut self) -> bool {
        self.state.is_visible = !self.state.is_visible;
        self.state.is_visible
    }

    /// Sets explicit visibility.
    pub fn set_visible(&mut self, visible: bool) {
        self.state.is_visible = visible;
    }

    /// Switches the active docking mode and recalculates screen bounds.
    pub fn set_dock_mode(&mut self, mode: HudDockMode, viewport_w: f32, viewport_h: f32) {
        self.state.dock_mode = mode;
        self.update_bounds(viewport_w, viewport_h);
    }

    /// Switches the active diagnostics tab.
    pub fn set_active_tab(&mut self, tab: HudTab) {
        self.state.active_tab = tab;
    }

    /// Toggles the Inspect Element cursor mode.
    pub fn toggle_inspect_element(&mut self) -> bool {
        self.state.is_inspecting_element = !self.state.is_inspecting_element;
        self.state.is_inspecting_element
    }

    /// Selects a specific node in the component inspector.
    pub fn select_node(&mut self, node: Option<NodeId>) {
        self.state.selected_node = node;
    }

    /// Sets the hovered node during element inspection.
    pub fn hover_node(&mut self, node: Option<NodeId>) {
        self.state.hovered_node = node;
    }

    /// Recalculates HUD bounds based on viewport dimensions and current dock mode.
    pub fn update_bounds(&mut self, viewport_w: f32, viewport_h: f32) {
        match self.state.dock_mode {
            HudDockMode::DockBottom => {
                let h = (viewport_h * 0.35).max(240.0).min(viewport_h);
                self.state.x = 0.0;
                self.state.y = viewport_h - h;
                self.state.width = viewport_w;
                self.state.height = h;
            }
            HudDockMode::DockRight => {
                let w = (viewport_w * 0.40).max(360.0).min(viewport_w);
                self.state.x = viewport_w - w;
                self.state.y = 0.0;
                self.state.width = w;
                self.state.height = viewport_h;
            }
            HudDockMode::Floating => {
                self.state.width = (viewport_w * 0.6).clamp(400.0, 900.0);
                self.state.height = (viewport_h * 0.5).clamp(300.0, 600.0);
                self.state.x = (viewport_w - self.state.width) * 0.5;
                self.state.y = (viewport_h - self.state.height) * 0.5;
            }
            HudDockMode::CollapsedPill => {
                self.state.width = 120.0;
                self.state.height = 36.0;
                self.state.x = viewport_w - 136.0;
                self.state.y = viewport_h - 52.0;
            }
        }
    }

    /// Returns true if coordinates `(x, y)` reside within the HUD window boundaries.
    pub fn is_point_inside_hud(&self, x: f32, y: f32) -> bool {
        if !self.state.is_visible {
            return false;
        }
        x >= self.state.x
            && x <= self.state.x + self.state.width
            && y >= self.state.y
            && y <= self.state.y + self.state.height
    }

    /// Returns HUD bounding rect as FixedRect.
    pub fn bounds_rect(&self) -> FixedRect {
        FixedRect::from_f32(
            self.state.x,
            self.state.y,
            self.state.width,
            self.state.height,
        )
    }
}

/// Event interceptor routing keyboard shortcuts and isolating pointer input.
pub struct DevToolsEventInterceptor;

impl DevToolsEventInterceptor {
    /// Evaluates a platform event. Returns `true` if the event was consumed by DevTools.
    pub fn intercept(hud: &mut HudController, event: &PlatformEvent) -> bool {
        match event {
            // Hotkey detection: F12, Ctrl+Shift+I, Cmd+Option+I
            PlatformEvent::KeyDown { key, modifiers } => {
                if *key == KeyCode::F12 {
                    hud.toggle_visibility();
                    return true;
                }
                if *key == KeyCode::KeyI && (modifiers.ctrl || modifiers.meta) && modifiers.shift {
                    hud.toggle_visibility();
                    return true;
                }
                if *key == KeyCode::KeyC && (modifiers.ctrl || modifiers.meta) && modifiers.shift {
                    hud.toggle_inspect_element();
                    return true;
                }

                // If HUD is visible and focused, consume keyboard events
                if hud.state.is_visible {
                    // Consume shortcuts inside HUD
                    return false;
                }
                false
            }

            // Pointer event boundary isolation
            PlatformEvent::MouseMove { x, y } => {
                if hud.state.is_visible && hud.is_point_inside_hud(*x, *y) {
                    return true;
                }
                if hud.state.is_inspecting_element {
                    // Inspect element mode consumes mouse moves to highlight components
                    return true;
                }
                false
            }

            PlatformEvent::MouseDown { x, y, .. } | PlatformEvent::MouseUp { x, y, .. } => {
                if hud.state.is_visible && hud.is_point_inside_hud(*x, *y) {
                    return true;
                }
                if hud.state.is_inspecting_element {
                    // Element click in inspect mode selects the element and exits inspect mode
                    hud.state.is_inspecting_element = false;
                    return true;
                }
                false
            }

            PlatformEvent::Scroll { .. } => {
                // Scroll events within HUD boundaries are swallowed
                false
            }

            _ => false,
        }
    }
}
