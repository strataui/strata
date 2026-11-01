//! Native application menus, submenus, keyboard accelerators, and menu bar models.

use crate::events::KeyCode;

/// Keyboard modifier keys for application menu shortcuts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AcceleratorModifier {
    CmdOrCtrl,
    Cmd,
    Ctrl,
    Alt,
    Shift,
}

/// Keyboard shortcut trigger attached to an application menu item.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MenuAccelerator {
    pub modifiers: Vec<AcceleratorModifier>,
    pub key: KeyCode,
}

impl MenuAccelerator {
    pub fn new(modifiers: Vec<AcceleratorModifier>, key: KeyCode) -> Self {
        Self { modifiers, key }
    }

    /// Parses a standard accelerator string like `"CmdOrCtrl+S"`, `"Ctrl+Shift+P"`, or `"Alt+F4"`.
    pub fn parse(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.split('+').map(|p| p.trim()).collect();
        if parts.is_empty() {
            return None;
        }

        let mut modifiers = Vec::new();
        for &mod_str in &parts[..parts.len() - 1] {
            let m = match mod_str.to_lowercase().as_str() {
                "cmdorctrl" | "commandorcontrol" => AcceleratorModifier::CmdOrCtrl,
                "cmd" | "command" | "super" | "meta" => AcceleratorModifier::Cmd,
                "ctrl" | "control" => AcceleratorModifier::Ctrl,
                "alt" | "option" => AcceleratorModifier::Alt,
                "shift" => AcceleratorModifier::Shift,
                _ => return None,
            };
            modifiers.push(m);
        }

        let key_str = parts.last()?;
        let key = match key_str.to_uppercase().as_str() {
            "A" => KeyCode::KeyA, "B" => KeyCode::KeyB, "C" => KeyCode::KeyC,
            "D" => KeyCode::KeyD, "E" => KeyCode::KeyE, "F" => KeyCode::KeyF,
            "G" => KeyCode::KeyG, "H" => KeyCode::KeyH, "I" => KeyCode::KeyI,
            "J" => KeyCode::KeyJ, "K" => KeyCode::KeyK, "L" => KeyCode::KeyL,
            "M" => KeyCode::KeyM, "N" => KeyCode::KeyN, "O" => KeyCode::KeyO,
            "P" => KeyCode::KeyP, "Q" => KeyCode::KeyQ, "R" => KeyCode::KeyR,
            "S" => KeyCode::KeyS, "T" => KeyCode::KeyT, "U" => KeyCode::KeyU,
            "V" => KeyCode::KeyV, "W" => KeyCode::KeyW, "X" => KeyCode::KeyX,
            "Y" => KeyCode::KeyY, "Z" => KeyCode::KeyZ,
            "0" => KeyCode::Digit0, "1" => KeyCode::Digit1, "2" => KeyCode::Digit2,
            "3" => KeyCode::Digit3, "4" => KeyCode::Digit4, "5" => KeyCode::Digit5,
            "6" => KeyCode::Digit6, "7" => KeyCode::Digit7, "8" => KeyCode::Digit8,
            "9" => KeyCode::Digit9,
            "ESC" | "ESCAPE" => KeyCode::Escape,
            "ENTER" | "RETURN" => KeyCode::Enter,
            "TAB" => KeyCode::Tab,
            "SPACE" => KeyCode::Space,
            "BACKSPACE" => KeyCode::Backspace,
            "DELETE" | "DEL" => KeyCode::Delete,
            "UP" => KeyCode::ArrowUp,
            "DOWN" => KeyCode::ArrowDown,
            "LEFT" => KeyCode::ArrowLeft,
            "RIGHT" => KeyCode::ArrowRight,
            "F1" => KeyCode::F1, "F2" => KeyCode::F2, "F3" => KeyCode::F3,
            "F4" => KeyCode::F4, "F5" => KeyCode::F5, "F6" => KeyCode::F6,
            "F7" => KeyCode::F7, "F8" => KeyCode::F8, "F9" => KeyCode::F9,
            "F10" => KeyCode::F10, "F11" => KeyCode::F11, "F12" => KeyCode::F12,
            _ => return None,
        };

        Some(Self { modifiers, key })
    }

    /// Converts to user-facing platform string (e.g. `⌘S` on macOS, `Ctrl+S` on Windows/Linux).
    pub fn to_display_string(&self, is_macos: bool) -> String {
        let mut parts = Vec::new();
        for m in &self.modifiers {
            let label = match m {
                AcceleratorModifier::CmdOrCtrl => if is_macos { "⌘" } else { "Ctrl+" },
                AcceleratorModifier::Cmd => if is_macos { "⌘" } else { "Win+" },
                AcceleratorModifier::Ctrl => "Ctrl+",
                AcceleratorModifier::Alt => if is_macos { "⌥" } else { "Alt+" },
                AcceleratorModifier::Shift => if is_macos { "⇧" } else { "Shift+" },
            };
            parts.push(label.to_string());
        }

        let key_name = match self.key {
            KeyCode::KeyA => "A", KeyCode::KeyB => "B", KeyCode::KeyC => "C",
            KeyCode::KeyD => "D", KeyCode::KeyE => "E", KeyCode::KeyF => "F",
            KeyCode::KeyG => "G", KeyCode::KeyH => "H", KeyCode::KeyI => "I",
            KeyCode::KeyJ => "J", KeyCode::KeyK => "K", KeyCode::KeyL => "L",
            KeyCode::KeyM => "M", KeyCode::KeyN => "N", KeyCode::KeyO => "O",
            KeyCode::KeyP => "P", KeyCode::KeyQ => "Q", KeyCode::KeyR => "R",
            KeyCode::KeyS => "S", KeyCode::KeyT => "T", KeyCode::KeyU => "U",
            KeyCode::KeyV => "V", KeyCode::KeyW => "W", KeyCode::KeyX => "X",
            KeyCode::KeyY => "Y", KeyCode::KeyZ => "Z",
            KeyCode::Digit0 => "0", KeyCode::Digit1 => "1", KeyCode::Digit2 => "2",
            KeyCode::Digit3 => "3", KeyCode::Digit4 => "4", KeyCode::Digit5 => "5",
            KeyCode::Digit6 => "6", KeyCode::Digit7 => "7", KeyCode::Digit8 => "8",
            KeyCode::Digit9 => "9",
            KeyCode::Escape => "Esc",
            KeyCode::Enter => "Enter",
            KeyCode::Tab => "Tab",
            KeyCode::Space => "Space",
            KeyCode::Backspace => "Backspace",
            KeyCode::Delete => "Delete",
            KeyCode::ArrowUp => "Up",
            KeyCode::ArrowDown => "Down",
            KeyCode::ArrowLeft => "Left",
            KeyCode::ArrowRight => "Right",
            KeyCode::F1 => "F1", KeyCode::F2 => "F2", KeyCode::F3 => "F3",
            KeyCode::F4 => "F4", KeyCode::F5 => "F5", KeyCode::F6 => "F6",
            KeyCode::F7 => "F7", KeyCode::F8 => "F8", KeyCode::F9 => "F9",
            KeyCode::F10 => "F10", KeyCode::F11 => "F11", KeyCode::F12 => "F12",
            KeyCode::Home => "Home", KeyCode::End => "End",
            KeyCode::PageUp => "PageUp", KeyCode::PageDown => "PageDown",
            KeyCode::Unknown => "?",
        };

        parts.push(key_name.to_string());
        parts.join("")
    }
}

/// Standard platform system roles for menu items.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MenuItemRole {
    #[default]
    None,
    About,
    Hide,
    HideOthers,
    Unhide,
    Quit,
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    SelectAll,
    Minimize,
    Zoom,
    ToggleFullScreen,
    CloseWindow,
}

/// An individual item in an application menu.
#[derive(Debug, Clone, PartialEq)]
pub struct MenuItem {
    pub id: u64,
    pub label: String,
    pub role: MenuItemRole,
    pub accelerator: Option<MenuAccelerator>,
    pub enabled: bool,
    pub checked: bool,
    pub is_separator: bool,
    pub submenu: Option<Menu>,
}

impl MenuItem {
    pub fn item(id: u64, label: impl Into<String>) -> Self {
        Self {
            id,
            label: label.into(),
            role: MenuItemRole::None,
            accelerator: None,
            enabled: true,
            checked: false,
            is_separator: false,
            submenu: None,
        }
    }

    pub fn role(role: MenuItemRole) -> Self {
        let label = match role {
            MenuItemRole::About => "About",
            MenuItemRole::Hide => "Hide",
            MenuItemRole::HideOthers => "Hide Others",
            MenuItemRole::Unhide => "Show All",
            MenuItemRole::Quit => "Quit",
            MenuItemRole::Undo => "Undo",
            MenuItemRole::Redo => "Redo",
            MenuItemRole::Cut => "Cut",
            MenuItemRole::Copy => "Copy",
            MenuItemRole::Paste => "Paste",
            MenuItemRole::SelectAll => "Select All",
            MenuItemRole::Minimize => "Minimize",
            MenuItemRole::Zoom => "Zoom",
            MenuItemRole::ToggleFullScreen => "Toggle Full Screen",
            MenuItemRole::CloseWindow => "Close Window",
            MenuItemRole::None => "",
        };

        Self {
            id: 0,
            label: label.to_string(),
            role,
            accelerator: None,
            enabled: true,
            checked: false,
            is_separator: false,
            submenu: None,
        }
    }

    pub fn separator() -> Self {
        Self {
            id: 0,
            label: String::new(),
            role: MenuItemRole::None,
            accelerator: None,
            enabled: false,
            checked: false,
            is_separator: true,
            submenu: None,
        }
    }

    pub fn submenu(id: u64, label: impl Into<String>, menu: Menu) -> Self {
        Self {
            id,
            label: label.into(),
            role: MenuItemRole::None,
            accelerator: None,
            enabled: true,
            checked: false,
            is_separator: false,
            submenu: Some(menu),
        }
    }

    pub fn with_accelerator(mut self, acc: impl Into<String>) -> Self {
        self.accelerator = MenuAccelerator::parse(&acc.into());
        self
    }

    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn with_checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }
}

/// A top-level menu dropdown or nested submenu.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Menu {
    pub title: String,
    pub items: Vec<MenuItem>,
}

impl Menu {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            items: Vec::new(),
        }
    }

    pub fn add_item(&mut self, item: MenuItem) -> &mut Self {
        self.items.push(item);
        self
    }

    pub fn add_separator(&mut self) -> &mut Self {
        self.items.push(MenuItem::separator());
        self
    }

    pub fn add_submenu(&mut self, id: u64, title: impl Into<String>, submenu: Menu) -> &mut Self {
        self.items.push(MenuItem::submenu(id, title, submenu));
        self
    }

    pub fn find_item(&self, id: u64) -> Option<&MenuItem> {
        for item in &self.items {
            if item.id == id && !item.is_separator {
                return Some(item);
            }
            if let Some(ref sub) = item.submenu {
                if let Some(found) = sub.find_item(id) {
                    return Some(found);
                }
            }
        }
        None
    }
}

/// Complete application menu bar containing top-level menus.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MenuBar {
    pub menus: Vec<Menu>,
}

impl MenuBar {
    pub fn new() -> Self {
        Self { menus: Vec::new() }
    }

    pub fn add_menu(&mut self, menu: Menu) -> &mut Self {
        self.menus.push(menu);
        self
    }

    /// Constructs standard enterprise application menu bar with File, Edit, View, Window, Help menus.
    pub fn standard_app_menu(app_name: &str) -> Self {
        let mut bar = Self::new();

        // App Menu (macOS) / File Menu (Windows/Linux)
        let mut file_menu = Menu::new("File");
        file_menu.add_item(MenuItem::item(101, "New Window").with_accelerator("CmdOrCtrl+N"));
        file_menu.add_item(MenuItem::item(102, "Open File...").with_accelerator("CmdOrCtrl+O"));
        file_menu.add_separator();
        file_menu.add_item(MenuItem::item(103, "Save").with_accelerator("CmdOrCtrl+S"));
        file_menu.add_separator();
        file_menu.add_item(MenuItem::role(MenuItemRole::CloseWindow).with_accelerator("CmdOrCtrl+W"));
        file_menu.add_item(MenuItem::role(MenuItemRole::Quit).with_accelerator("CmdOrCtrl+Q"));
        bar.add_menu(file_menu);

        // Edit Menu
        let mut edit_menu = Menu::new("Edit");
        edit_menu.add_item(MenuItem::role(MenuItemRole::Undo).with_accelerator("CmdOrCtrl+Z"));
        edit_menu.add_item(MenuItem::role(MenuItemRole::Redo).with_accelerator("CmdOrCtrl+Shift+Z"));
        edit_menu.add_separator();
        edit_menu.add_item(MenuItem::role(MenuItemRole::Cut).with_accelerator("CmdOrCtrl+X"));
        edit_menu.add_item(MenuItem::role(MenuItemRole::Copy).with_accelerator("CmdOrCtrl+C"));
        edit_menu.add_item(MenuItem::role(MenuItemRole::Paste).with_accelerator("CmdOrCtrl+V"));
        edit_menu.add_item(MenuItem::role(MenuItemRole::SelectAll).with_accelerator("CmdOrCtrl+A"));
        bar.add_menu(edit_menu);

        // View Menu
        let mut view_menu = Menu::new("View");
        view_menu.add_item(MenuItem::item(301, "Reload").with_accelerator("CmdOrCtrl+R"));
        view_menu.add_item(MenuItem::role(MenuItemRole::ToggleFullScreen).with_accelerator("F11"));
        bar.add_menu(view_menu);

        // Window Menu
        let mut window_menu = Menu::new("Window");
        window_menu.add_item(MenuItem::role(MenuItemRole::Minimize).with_accelerator("CmdOrCtrl+M"));
        window_menu.add_item(MenuItem::role(MenuItemRole::Zoom));
        bar.add_menu(window_menu);

        // Help Menu
        let mut help_menu = Menu::new("Help");
        help_menu.add_item(MenuItem::item(501, format!("About {}", app_name)));
        bar.add_menu(help_menu);

        bar
    }
}
