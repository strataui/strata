//! Global navigation systems: 3-Button navigation bar and modern gesture navigation.

/// Actions emitted by the navigation system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavAction {
    Back,
    Home,
    Recents,
}

/// 3-Button navigation order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonLayout {
    /// Classic Android / Pixel order: [Back] [Home] [Recents]
    AndroidClassic,
    /// Samsung One UI reversed order: [Recents] [Home] [Back]
    SamsungFlipped,
}

/// Button aesthetic shapes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonShape {
    /// Traditional geometric shapes: Triangle, Circle, Square
    Geometric,
    /// Modern glyphs: Left Chevron, Home House, Triple Stack Lines
    Glyphs,
    /// Minimalist dots
    Dots,
}

/// 3-Button navigation bar subsystem.
#[derive(Debug, Clone, PartialEq)]
pub struct ThreeButtonNavBar {
    pub layout: ButtonLayout,
    pub shape: ButtonShape,
    pub is_hidden_immersive: bool,
    pub background_opacity: f32,
    pub is_dark_content: bool,
}

impl Default for ThreeButtonNavBar {
    fn default() -> Self {
        Self::new(ButtonLayout::AndroidClassic)
    }
}

impl ThreeButtonNavBar {
    pub fn new(layout: ButtonLayout) -> Self {
        Self {
            layout,
            shape: ButtonShape::Geometric,
            is_hidden_immersive: false,
            background_opacity: 0.25,
            is_dark_content: false,
        }
    }

    /// Evaluates dynamic luminance contrast to ensure WCAG AAA accessibility.
    ///
    /// If underlying average luminance > 0.5, switches to dark glyphs; otherwise light glyphs.
    pub fn update_luminance_contrast(&mut self, luminance: f32) {
        self.is_dark_content = luminance > 0.5;
    }

    /// Resolves the ordered button actions for rendering.
    pub fn button_order(&self) -> [NavAction; 3] {
        match self.layout {
            ButtonLayout::AndroidClassic => [NavAction::Back, NavAction::Home, NavAction::Recents],
            ButtonLayout::SamsungFlipped => [NavAction::Recents, NavAction::Home, NavAction::Back],
        }
    }
}

/// Modern full-screen gesture navigation engine.
#[derive(Debug, Clone, PartialEq)]
pub struct GestureNavEngine {
    pub pill_width: f32,
    pub pill_height: f32,
    pub is_dark_content: bool,
    pub drag_y_offset: f32,
    pub drag_x_offset: f32,
    pub is_holding_recents: bool,
}

impl Default for GestureNavEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl GestureNavEngine {
    pub fn new() -> Self {
        Self {
            pill_width: 134.0,
            pill_height: 5.0,
            is_dark_content: false,
            drag_y_offset: 0.0,
            drag_x_offset: 0.0,
            is_holding_recents: false,
        }
    }

    /// Processes edge swipe gesture from left or right screen border (triggers Back).
    pub fn on_edge_swipe_back(&self) -> NavAction {
        NavAction::Back
    }

    /// Processes bottom flick upwards (triggers Home).
    pub fn on_home_flick(&mut self) -> NavAction {
        self.drag_y_offset = 0.0;
        self.is_holding_recents = false;
        NavAction::Home
    }

    /// Processes upward drag-and-hold (triggers Recents Multitasking).
    pub fn on_recents_hold(&mut self) -> NavAction {
        self.is_holding_recents = true;
        NavAction::Recents
    }

    /// Updates dynamic luminance contrast.
    pub fn update_luminance_contrast(&mut self, luminance: f32) {
        self.is_dark_content = luminance > 0.5;
    }
}
