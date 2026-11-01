//! Desktop System Tray abstraction (`TrayIcon`, `TrayMenu`, `TrayMenuItem`).

/// An individual item or separator inside a system tray context menu.
#[derive(Debug, Clone, PartialEq)]
pub struct TrayMenuItem {
    pub id: u64,
    pub label: String,
    pub enabled: bool,
    pub checked: bool,
    pub is_separator: bool,
    pub children: Vec<TrayMenuItem>,
}

impl TrayMenuItem {
    /// Creates a standard clickable menu item.
    pub fn item(id: u64, label: impl Into<String>) -> Self {
        Self {
            id,
            label: label.into(),
            enabled: true,
            checked: false,
            is_separator: false,
            children: Vec::new(),
        }
    }

    /// Creates a horizontal separator.
    pub fn separator() -> Self {
        Self {
            id: 0,
            label: String::new(),
            enabled: false,
            checked: false,
            is_separator: true,
            children: Vec::new(),
        }
    }

    /// Creates a parent item containing nested sub-menu items.
    pub fn submenu(id: u64, label: impl Into<String>, children: Vec<TrayMenuItem>) -> Self {
        Self {
            id,
            label: label.into(),
            enabled: true,
            checked: false,
            is_separator: false,
            children,
        }
    }

    /// Sets the checked state of this item.
    pub fn with_checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    /// Sets the enabled state of this item.
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

/// A context menu attached to a desktop system tray icon.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TrayMenu {
    pub items: Vec<TrayMenuItem>,
}

impl TrayMenu {
    /// Creates an empty tray menu.
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    /// Appends a clickable menu item.
    pub fn add_item(&mut self, id: u64, label: impl Into<String>) -> &mut Self {
        self.items.push(TrayMenuItem::item(id, label));
        self
    }

    /// Appends a separator.
    pub fn add_separator(&mut self) -> &mut Self {
        self.items.push(TrayMenuItem::separator());
        self
    }

    /// Appends a submenu.
    pub fn add_submenu(&mut self, id: u64, label: impl Into<String>, children: Vec<TrayMenuItem>) -> &mut Self {
        self.items.push(TrayMenuItem::submenu(id, label, children));
        self
    }

    /// Recursively searches for an item by ID.
    pub fn find_item(&self, id: u64) -> Option<&TrayMenuItem> {
        Self::find_in_slice(&self.items, id)
    }

    /// Recursively searches for a mutable item by ID.
    pub fn find_item_mut(&mut self, id: u64) -> Option<&mut TrayMenuItem> {
        Self::find_in_slice_mut(&mut self.items, id)
    }

    fn find_in_slice<'a>(items: &'a [TrayMenuItem], id: u64) -> Option<&'a TrayMenuItem> {
        for item in items {
            if !item.is_separator && item.id == id {
                return Some(item);
            }
            if let Some(found) = Self::find_in_slice(&item.children, id) {
                return Some(found);
            }
        }
        None
    }

    fn find_in_slice_mut<'a>(items: &'a mut [TrayMenuItem], id: u64) -> Option<&'a mut TrayMenuItem> {
        for item in items {
            if !item.is_separator && item.id == id {
                return Some(item);
            }
            if let Some(found) = Self::find_in_slice_mut(&mut item.children, id) {
                return Some(found);
            }
        }
        None
    }
}

/// System tray icon abstraction.
#[derive(Debug, Clone)]
pub struct TrayIcon {
    pub tooltip: String,
    pub icon_rgba: Vec<u8>,
    pub icon_width: u32,
    pub icon_height: u32,
    pub menu: Option<TrayMenu>,
    pub visible: bool,
    pub click_count: usize,
    pub last_action_id: Option<u64>,
}

impl TrayIcon {
    /// Creates a new system tray icon.
    pub fn new(
        tooltip: impl Into<String>,
        icon_rgba: Vec<u8>,
        icon_width: u32,
        icon_height: u32,
    ) -> Self {
        Self {
            tooltip: tooltip.into(),
            icon_rgba,
            icon_width,
            icon_height,
            menu: None,
            visible: true,
            click_count: 0,
            last_action_id: None,
        }
    }

    /// Sets the hover tooltip text.
    pub fn set_tooltip(&mut self, tooltip: impl Into<String>) {
        self.tooltip = tooltip.into();
    }

    /// Sets or clears the tray context menu.
    pub fn set_menu(&mut self, menu: Option<TrayMenu>) {
        self.menu = menu;
    }

    /// Sets whether the icon is visible in the status bar/system tray.
    pub fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    /// Simulates or executes a primary click on the tray icon.
    pub fn trigger_click(&mut self) {
        self.click_count += 1;
    }

    /// Simulates or executes a menu item action by ID. Returns true if the item was found and enabled.
    pub fn trigger_menu_action(&mut self, id: u64) -> bool {
        if let Some(menu) = &self.menu {
            if let Some(item) = menu.find_item(id) {
                if item.enabled {
                    self.last_action_id = Some(id);
                    return true;
                }
            }
        }
        false
    }
}
