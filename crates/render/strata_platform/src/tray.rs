//! Desktop System Tray abstraction (`TrayIcon`, `TrayMenu`, `TrayMenuItem`).
//!
//! On Linux the live backend talks to the Ayatana AppIndicator library through
//! runtime dynamic loading (`dlopen`/`dlsym`). The library is never a build-time
//! or hard runtime dependency: when it is missing, or when the caller is not on
//! the GTK main thread, the handle falls back to the deterministic in-process
//! model and reports `backend_name() == "handle-only"`.

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
    pub fn add_submenu(
        &mut self,
        id: u64,
        label: impl Into<String>,
        children: Vec<TrayMenuItem>,
    ) -> &mut Self {
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

    fn find_in_slice_mut<'a>(
        items: &'a mut [TrayMenuItem],
        id: u64,
    ) -> Option<&'a mut TrayMenuItem> {
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
    pub badge: Option<String>,
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
            badge: None,
        }
    }

    /// Sets the notification badge text (e.g. "99+" or "NEW").
    pub fn set_badge(&mut self, badge: Option<String>) {
        self.badge = badge;
    }

    /// Sets the notification badge as a numeric count (or clears it if None or 0).
    pub fn set_badge_count(&mut self, count: Option<u32>) {
        self.badge = count.and_then(|c| if c > 0 { Some(c.to_string()) } else { None });
    }

    /// Returns current badge text if present.
    pub fn badge(&self) -> Option<&str> {
        self.badge.as_deref()
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

    /// Spawns an OS-native system tray handle dispatching events through the provided event callback.
    ///
    /// On Linux the native indicator is created immediately when this is called
    /// on the GTK main thread after `gtk::init()`. Callers on any other thread,
    /// and callers where the Ayatana AppIndicator library is absent, receive a
    /// handle-only controller that still dispatches events from the in-process
    /// model.
    pub fn spawn_native<F>(self, event_sink: F) -> NativeTrayHandle
    where
        F: Fn(crate::events::PlatformEvent) + Send + Sync + 'static,
    {
        NativeTrayHandle::new(self, std::sync::Arc::new(event_sink))
    }
}

/// Thread-safe tray event sink callback.
pub type TrayEventSink =
    std::sync::Arc<dyn Fn(crate::events::PlatformEvent) + Send + Sync + 'static>;

/// Shared menu and event state used by the handle and by native menu callbacks.
struct SharedTrayState {
    event_sink: TrayEventSink,
    menu: std::sync::Mutex<Option<TrayMenu>>,
}

impl SharedTrayState {
    fn new(event_sink: TrayEventSink, menu: Option<TrayMenu>) -> Self {
        Self {
            event_sink,
            menu: std::sync::Mutex::new(menu),
        }
    }

    fn set_menu(&self, menu: Option<TrayMenu>) {
        let mut slot = match self.menu.lock() {
            Ok(slot) => slot,
            Err(poisoned) => poisoned.into_inner(),
        };
        *slot = menu;
    }

    fn dispatch_click(&self) {
        (self.event_sink)(crate::events::PlatformEvent::TrayIconClicked);
    }

    fn dispatch_menu_action(&self, id: u64) -> bool {
        let enabled = {
            let slot = match self.menu.lock() {
                Ok(slot) => slot,
                Err(poisoned) => poisoned.into_inner(),
            };
            slot.as_ref()
                .and_then(|menu| menu.find_item(id))
                .map(|item| item.enabled)
                .unwrap_or(false)
        };
        if !enabled {
            return false;
        }
        (self.event_sink)(crate::events::PlatformEvent::TrayMenuItemClicked(id));
        true
    }
}

/// Operating-system native desktop system tray controller.
pub struct NativeTrayHandle {
    icon: TrayIcon,
    shared: std::sync::Arc<SharedTrayState>,
    #[cfg(target_os = "linux")]
    live: Option<linux_backend::LiveIndicator>,
}

impl NativeTrayHandle {
    /// Creates and registers an OS-native system tray icon.
    ///
    /// See [`TrayIcon::spawn_native`] for the Linux main-thread contract.
    pub fn new(icon: TrayIcon, event_sink: TrayEventSink) -> Self {
        let shared = std::sync::Arc::new(SharedTrayState::new(event_sink, icon.menu.clone()));
        #[cfg(target_os = "linux")]
        let live = linux_backend::LiveIndicator::new(&icon, shared.clone());
        Self {
            icon,
            shared,
            #[cfg(target_os = "linux")]
            live,
        }
    }

    /// Returns the active native backend identifier.
    ///
    /// `"ayatana-appindicator"` means a live StatusNotifierItem indicator is
    /// registered through the dynamically loaded Ayatana library.
    /// `"handle-only"` means no live platform indicator exists; events still
    /// flow through the in-process controller.
    pub fn backend_name(&self) -> &'static str {
        #[cfg(target_os = "linux")]
        {
            if self.live.is_some() {
                "ayatana-appindicator"
            } else {
                "handle-only"
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            "handle-only"
        }
    }

    /// Updates tooltip text on the system tray icon.
    pub fn set_tooltip(&mut self, tooltip: impl Into<String>) {
        self.icon.set_tooltip(tooltip);
        #[cfg(target_os = "linux")]
        {
            let tooltip = self.icon.tooltip.clone();
            if let Some(live) = &mut self.live {
                live.set_tooltip(tooltip);
            }
        }
    }

    /// Sets or updates the tray context menu.
    pub fn set_menu(&mut self, menu: Option<TrayMenu>) {
        self.icon.set_menu(menu.clone());
        self.shared.set_menu(menu.clone());
        #[cfg(target_os = "linux")]
        if let Some(live) = &mut self.live {
            live.set_menu(menu.as_ref());
        }
    }

    /// Sets visibility of the system tray icon.
    pub fn set_visible(&mut self, visible: bool) {
        self.icon.set_visible(visible);
        #[cfg(target_os = "linux")]
        if let Some(live) = &mut self.live {
            live.set_visible(visible);
        }
    }

    /// Sets or clears the notification badge on the tray icon.
    pub fn set_badge(&mut self, badge: Option<String>) {
        self.icon.set_badge(badge.clone());
        #[cfg(target_os = "linux")]
        if let Some(live) = &mut self.live {
            live.set_badge(badge);
        }
    }

    /// Dispatches a simulated or native tray icon click event.
    pub fn trigger_click(&mut self) {
        self.icon.trigger_click();
        self.shared.dispatch_click();
    }

    /// Dispatches a simulated or native menu action event.
    pub fn trigger_menu_action(&mut self, id: u64) -> bool {
        self.shared.set_menu(self.icon.menu.clone());
        if self.shared.dispatch_menu_action(id) {
            self.icon.last_action_id = Some(id);
            true
        } else {
            false
        }
    }

    /// Returns a reference to the underlying `TrayIcon` state.
    pub fn icon(&self) -> &TrayIcon {
        &self.icon
    }

    /// Returns a mutable reference to the underlying `TrayIcon` state.
    pub fn icon_mut(&mut self) -> &mut TrayIcon {
        &mut self.icon
    }
}

#[cfg(target_os = "linux")]
mod linux_backend {
    //! Ayatana AppIndicator backend loaded at runtime.
    //!
    //! Lifecycle and threading contract:
    //! - `NativeTrayHandle::new` must be called on the GTK main thread after
    //!   `gtk::init()`; otherwise no indicator is created and the handle reports
    //!   `backend_name() == "handle-only"`.
    //! - Ayatana AppIndicator owns the `GtkMenu` lifecycle. The Rust side keeps a
    //!   strong reference to the same menu so the widget cannot be collected
    //!   before the indicator releases it.
    //! - The library handle is loaded once per process and intentionally kept for
    //!   the process lifetime because GObject type registration cannot be safely
    //!   unloaded while the type system still references the library.
    //! - AppIndicator reports activation by exporting its menu over DBus to the
    //!   panel; this version of the library does not deliver a primary-click
    //!   callback itself. The backend connects the root `GtkMenu` `show` signal
    //!   to `TrayIconClicked` once per open as the closest available mapping.
    //!   On desktops that render the exported DBusMenu without showing the
    //!   `GtkMenu`, the signal may not fire and clicks then arrive only through
    //!   the in-process `trigger_click` controller.

    use super::{SharedTrayState, TrayIcon, TrayMenu, TrayMenuItem};
    use gtk::glib::translate::ToGlibPtr;
    use std::ffi::{c_char, c_int, c_void, CString};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, OnceLock};

    type Indicator = c_void;

    const RTLD_NOW: c_int = 2;
    const APP_INDICATOR_CATEGORY_APPLICATION_STATUS: c_int = 0;
    const APP_INDICATOR_STATUS_PASSIVE: c_int = 0;
    const APP_INDICATOR_STATUS_ACTIVE: c_int = 1;

    type NewFn = unsafe extern "C" fn(*const c_char, *const c_char, c_int) -> *mut Indicator;
    type SetStatusFn = unsafe extern "C" fn(*mut Indicator, c_int);
    type SetIconFullFn = unsafe extern "C" fn(*mut Indicator, *const c_char, *const c_char);
    type SetMenuFn = unsafe extern "C" fn(*mut Indicator, *mut c_void);
    type SetTitleFn = unsafe extern "C" fn(*mut Indicator, *const c_char);
    type SetLabelFn = unsafe extern "C" fn(*mut Indicator, *const c_char, *const c_char);

    extern "C" {
        fn dlopen(filename: *const c_char, flag: c_int) -> *mut c_void;
        fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    }

    struct Api {
        new: NewFn,
        set_status: SetStatusFn,
        set_icon_full: SetIconFullFn,
        set_menu: SetMenuFn,
        set_title: SetTitleFn,
        set_label: SetLabelFn,
    }

    static API: OnceLock<Option<Api>> = OnceLock::new();

    fn api() -> Option<&'static Api> {
        API.get_or_init(Api::load).as_ref()
    }

    impl Api {
        fn load() -> Option<Self> {
            const CANDIDATES: [&[u8]; 2] = [
                b"libayatana-appindicator3.so.1\0",
                b"libappindicator3.so.1\0",
            ];
            for candidate in CANDIDATES {
                let handle = unsafe { dlopen(candidate.as_ptr() as *const c_char, RTLD_NOW) };
                if handle.is_null() {
                    continue;
                }
                unsafe {
                    let new: NewFn = match resolve(handle, b"app_indicator_new\0") {
                        Some(symbol) => symbol,
                        None => continue,
                    };
                    let set_status: SetStatusFn =
                        match resolve(handle, b"app_indicator_set_status\0") {
                            Some(symbol) => symbol,
                            None => continue,
                        };
                    let set_icon_full: SetIconFullFn =
                        match resolve(handle, b"app_indicator_set_icon_full\0") {
                            Some(symbol) => symbol,
                            None => continue,
                        };
                    let set_menu: SetMenuFn = match resolve(handle, b"app_indicator_set_menu\0") {
                        Some(symbol) => symbol,
                        None => continue,
                    };
                    let set_title: SetTitleFn = match resolve(handle, b"app_indicator_set_title\0")
                    {
                        Some(symbol) => symbol,
                        None => continue,
                    };
                    let set_label: SetLabelFn = match resolve(handle, b"app_indicator_set_label\0")
                    {
                        Some(symbol) => symbol,
                        None => continue,
                    };
                    return Some(Self {
                        new,
                        set_status,
                        set_icon_full,
                        set_menu,
                        set_title,
                        set_label,
                    });
                }
            }
            None
        }
    }

    unsafe fn resolve<T: Copy>(handle: *mut c_void, name: &[u8]) -> Option<T> {
        let symbol = dlsym(handle, name.as_ptr() as *const c_char);
        if symbol.is_null() {
            None
        } else {
            Some(std::mem::transmute_copy(&symbol))
        }
    }

    static ICON_COUNTER: AtomicU64 = AtomicU64::new(0);

    pub(super) struct LiveIndicator {
        api: &'static Api,
        indicator: *mut Indicator,
        menu: Option<gtk::Menu>,
        icon_path: Option<PathBuf>,
        shared: Arc<SharedTrayState>,
        tooltip: String,
        visible: bool,
        badge: Option<String>,
    }

    impl LiveIndicator {
        pub(super) fn new(icon: &TrayIcon, shared: Arc<SharedTrayState>) -> Option<Self> {
            if !gtk::is_initialized() || !gtk::is_initialized_main_thread() {
                return None;
            }
            let api = api()?;
            let icon_path = write_icon_png(icon);
            let id = CString::new(indicator_id()).ok()?;
            let icon_name = match &icon_path {
                Some(path) => CString::new(path.to_string_lossy().as_bytes()).ok()?,
                None => CString::new("application-x-executable").ok()?,
            };
            let indicator = unsafe {
                (api.new)(
                    id.as_ptr(),
                    icon_name.as_ptr(),
                    APP_INDICATOR_CATEGORY_APPLICATION_STATUS,
                )
            };
            if indicator.is_null() {
                remove_icon_file(icon_path.as_ref());
                return None;
            }

            let mut live = Self {
                api,
                indicator,
                menu: None,
                icon_path,
                shared,
                tooltip: icon.tooltip.clone(),
                visible: icon.visible,
                badge: icon.badge.clone(),
            };
            live.apply_tooltip();
            live.apply_status();
            live.apply_badge();
            if let Some(menu) = &icon.menu {
                live.set_menu(Some(menu));
            }
            Some(live)
        }

        pub(super) fn set_tooltip(&mut self, tooltip: String) {
            self.tooltip = tooltip;
            self.apply_tooltip();
        }

        pub(super) fn set_visible(&mut self, visible: bool) {
            self.visible = visible;
            self.apply_status();
        }

        pub(super) fn set_badge(&mut self, badge: Option<String>) {
            self.badge = badge;
            self.apply_badge();
        }

        pub(super) fn set_menu(&mut self, menu: Option<&TrayMenu>) {
            // AppIndicator asserts that the menu pointer is a valid GtkMenu, so
            // clearing a menu is represented by an empty menu rather than null.
            let (gtk_menu, model) = match menu {
                Some(model) => (build_menu(model, &self.shared), Some(model.clone())),
                None => (
                    build_menu(&TrayMenu::new(), &self.shared),
                    Some(TrayMenu::new()),
                ),
            };
            let menu_ptr: *mut gtk::ffi::GtkMenu = gtk_menu.to_glib_none().0;
            self.shared.set_menu(model);
            unsafe {
                (self.api.set_menu)(self.indicator, menu_ptr as *mut c_void);
            }
            self.menu = Some(gtk_menu);
        }

        fn apply_tooltip(&self) {
            let Ok(title) = CString::new(self.tooltip.as_str()) else {
                return;
            };
            let icon_name = match &self.icon_path {
                Some(path) => CString::new(path.to_string_lossy().as_bytes()),
                None => CString::new("application-x-executable"),
            };
            let Ok(icon_name) = icon_name else {
                return;
            };
            unsafe {
                (self.api.set_title)(self.indicator, title.as_ptr());
                (self.api.set_icon_full)(self.indicator, icon_name.as_ptr(), title.as_ptr());
            }
        }

        fn apply_status(&self) {
            let status = if self.visible {
                APP_INDICATOR_STATUS_ACTIVE
            } else {
                APP_INDICATOR_STATUS_PASSIVE
            };
            unsafe {
                (self.api.set_status)(self.indicator, status);
            }
        }

        fn apply_badge(&self) {
            match &self.badge {
                Some(badge) => {
                    const GUIDE: &[u8] = b"1\0";
                    let Ok(label) = CString::new(badge.as_str()) else {
                        return;
                    };
                    unsafe {
                        (self.api.set_label)(
                            self.indicator,
                            label.as_ptr(),
                            GUIDE.as_ptr() as *const c_char,
                        );
                    }
                }
                None => unsafe {
                    (self.api.set_label)(self.indicator, std::ptr::null(), std::ptr::null());
                },
            }
        }
    }

    impl Drop for LiveIndicator {
        fn drop(&mut self) {
            unsafe {
                gtk::glib::gobject_ffi::g_object_unref(
                    self.indicator as *mut gtk::glib::gobject_ffi::GObject,
                );
            }
            self.menu = None;
            remove_icon_file(self.icon_path.as_ref());
        }
    }

    fn build_menu(model: &TrayMenu, shared: &Arc<SharedTrayState>) -> gtk::Menu {
        use gtk::prelude::*;

        let menu = gtk::Menu::new();
        append_items(&menu, &model.items, shared);

        let sink = shared.clone();
        menu.connect_show(move |_| sink.dispatch_click());

        menu.show_all();
        menu
    }

    fn append_items(parent: &gtk::Menu, items: &[TrayMenuItem], shared: &Arc<SharedTrayState>) {
        use gtk::prelude::*;

        for item in items {
            if item.is_separator {
                let separator = gtk::SeparatorMenuItem::new();
                parent.append(&separator);
                separator.show();
                continue;
            }

            if item.children.is_empty() {
                if item.checked {
                    let menu_item = gtk::CheckMenuItem::with_label(&item.label);
                    menu_item.set_active(true);
                    menu_item.set_sensitive(item.enabled);
                    let sink = shared.clone();
                    let id = item.id;
                    menu_item.connect_activate(move |_| {
                        sink.dispatch_menu_action(id);
                    });
                    parent.append(&menu_item);
                    menu_item.show();
                } else {
                    let menu_item = gtk::MenuItem::with_label(&item.label);
                    menu_item.set_sensitive(item.enabled);
                    let sink = shared.clone();
                    let id = item.id;
                    menu_item.connect_activate(move |_| {
                        sink.dispatch_menu_action(id);
                    });
                    parent.append(&menu_item);
                    menu_item.show();
                }
            } else {
                let menu_item = gtk::MenuItem::with_label(&item.label);
                menu_item.set_sensitive(item.enabled);
                let submenu = gtk::Menu::new();
                append_items(&submenu, &item.children, shared);
                submenu.show_all();
                menu_item.set_submenu(Some(&submenu));
                parent.append(&menu_item);
                menu_item.show();
            }
        }
    }

    fn indicator_id() -> String {
        format!("strata-tray-{}", std::process::id())
    }

    fn write_icon_png(icon: &TrayIcon) -> Option<PathBuf> {
        let width = icon.icon_width;
        let height = icon.icon_height;
        let pixel_count = (width as usize).checked_mul(height as usize)?;
        if width == 0 || height == 0 || icon.icon_rgba.len() < pixel_count * 4 {
            return None;
        }

        // Cairo ARGB32 stores premultiplied BGRA on little-endian hosts.
        let mut bgra = Vec::with_capacity(pixel_count * 4);
        for pixel in icon.icon_rgba.chunks_exact(4).take(pixel_count) {
            bgra.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
        }

        let surface = cairo::ImageSurface::create_for_data(
            bgra,
            cairo::Format::ARgb32,
            width as i32,
            height as i32,
            (width * 4) as i32,
        )
        .ok()?;

        let directory = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .filter(|path| path.is_dir())
            .unwrap_or_else(|| PathBuf::from("/tmp"));
        let sequence = ICON_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = directory.join(format!(
            "strata-tray-{}-{}.png",
            std::process::id(),
            sequence
        ));
        let mut file = std::fs::File::create(&path).ok()?;
        surface.write_to_png(&mut file).ok()?;
        Some(path)
    }

    fn remove_icon_file(path: Option<&PathBuf>) {
        if let Some(path) = path {
            let _ = std::fs::remove_file(path);
        }
    }
}
