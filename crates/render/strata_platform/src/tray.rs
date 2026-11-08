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

    fn find_in_slice(items: &[TrayMenuItem], id: u64) -> Option<&TrayMenuItem> {
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

    fn find_in_slice_mut(
        items: &mut [TrayMenuItem],
        id: u64,
    ) -> Option<&mut TrayMenuItem> {
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
    #[cfg(windows)]
    live: Option<windows_backend::LiveTrayIcon>,
    #[cfg(target_os = "macos")]
    live: Option<macos_backend::LiveStatusItem>,
}

impl NativeTrayHandle {
    /// Creates and registers an OS-native system tray icon.
    ///
    /// See [`TrayIcon::spawn_native`] for the Linux main-thread contract.
    pub fn new(icon: TrayIcon, event_sink: TrayEventSink) -> Self {
        let shared = std::sync::Arc::new(SharedTrayState::new(event_sink, icon.menu.clone()));
        #[cfg(target_os = "linux")]
        let live = linux_backend::LiveIndicator::new(&icon, shared.clone());
        #[cfg(windows)]
        let live = windows_backend::LiveTrayIcon::new(&icon, shared.clone());
        #[cfg(target_os = "macos")]
        let live = macos_backend::LiveStatusItem::new(&icon, shared.clone());
        Self {
            icon,
            shared,
            #[cfg(target_os = "linux")]
            live,
            #[cfg(windows)]
            live,
            #[cfg(target_os = "macos")]
            live,
        }
    }

    /// Returns the active native backend identifier.
    ///
    /// - `"ayatana-appindicator"`: Registered via Linux Ayatana AppIndicator
    /// - `"win32-tray"`: Registered via Windows Shell_NotifyIconW
    /// - `"cocoa-nsstatusbar"`: Registered via macOS NSStatusBar
    /// - `"handle-only"`: Fallback in-process controller
    pub fn backend_name(&self) -> &'static str {
        #[cfg(target_os = "linux")]
        {
            if self.live.is_some() {
                "ayatana-appindicator"
            } else {
                "handle-only"
            }
        }
        #[cfg(windows)]
        {
            if self.live.is_some() {
                "win32-tray"
            } else {
                "handle-only"
            }
        }
        #[cfg(target_os = "macos")]
        {
            if self.live.is_some() {
                "cocoa-nsstatusbar"
            } else {
                "handle-only"
            }
        }
        #[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
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
        #[cfg(windows)]
        {
            let tooltip = self.icon.tooltip.clone();
            if let Some(live) = &mut self.live {
                live.set_tooltip(tooltip);
            }
        }
        #[cfg(target_os = "macos")]
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
        #[cfg(windows)]
        if let Some(live) = &mut self.live {
            live.set_menu(menu.as_ref());
        }
        #[cfg(target_os = "macos")]
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
        #[cfg(windows)]
        if let Some(live) = &mut self.live {
            live.set_visible(visible);
        }
        #[cfg(target_os = "macos")]
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
        #[cfg(windows)]
        if let Some(live) = &mut self.live {
            live.set_badge(badge);
        }
        #[cfg(target_os = "macos")]
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
        for pixel in icon.icon_rgba.as_chunks::<4>().0.iter().take(pixel_count) {
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

#[cfg(windows)]
mod windows_backend {
    //! Win32 System Tray implementation backed by `Shell_NotifyIconW`.
    use super::{SharedTrayState, TrayIcon, TrayMenu, TrayMenuItem};
    use std::sync::mpsc::{channel, Sender};
    use std::sync::Arc;
    use std::thread::JoinHandle;
    use windows_sys::Win32::Foundation::{HWND, POINT};
    use windows_sys::Win32::Graphics::Gdi::{CreateBitmap, DeleteObject};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::Shell::{
        Shell_NotifyIconW, NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE,
        NIM_MODIFY, NOTIFYICONDATAW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, CreateIconIndirect, CreatePopupMenu, CreateWindowExW, DefWindowProcW,
        DestroyIcon, DestroyMenu, DestroyWindow, DispatchMessageW, GetCursorPos, HICON, HMENU,
        ICONINFO, MF_CHECKED, MF_GRAYED, MF_POPUP, MF_SEPARATOR, MF_STRING, MF_UNCHECKED, MSG,
        PeekMessageW, PM_REMOVE, PostMessageW, RegisterClassExW, SetForegroundWindow,
        TrackPopupMenuEx, TPM_BOTTOMALIGN, TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON, WM_APP,
        WM_CONTEXTMENU, WM_LBUTTONUP, WM_NULL, WM_RBUTTONUP, WNDCLASSEXW,
    };

    enum TrayCommand {
        SetTooltip(String),
        SetMenu(Option<TrayMenu>),
        SetVisible(bool),
        SetBadge(Option<String>),
        Destroy,
    }

    pub struct LiveTrayIcon {
        command_tx: Sender<TrayCommand>,
        hwnd: HWND,
        thread_handle: Option<JoinHandle<()>>,
    }

    impl LiveTrayIcon {
        pub fn new(icon: &TrayIcon, shared: Arc<SharedTrayState>) -> Option<Self> {
            let (tx, rx) = channel();
            let (init_tx, init_rx) = channel();
            let initial_tooltip = icon.tooltip.clone();
            let initial_visible = icon.visible;
            let icon_rgba = icon.icon_rgba.clone();
            let icon_width = icon.icon_width;
            let icon_height = icon.icon_height;

            let thread_handle = std::thread::spawn(move || {
                unsafe {
                    let instance = GetModuleHandleW(std::ptr::null());
                    let class_name: Vec<u16> = "StrataTrayWindowClass\0".encode_utf16().collect();
                    let wnd_class = WNDCLASSEXW {
                        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                        style: 0,
                        lpfnWndProc: Some(DefWindowProcW),
                        cbClsExtra: 0,
                        cbWndExtra: 0,
                        hInstance: instance,
                        hIcon: 0,
                        hCursor: 0,
                        hbrBackground: 0,
                        lpszMenuName: std::ptr::null(),
                        lpszClassName: class_name.as_ptr(),
                        hIconSm: 0,
                    };
                    RegisterClassExW(&wnd_class);

                    let window_title: Vec<u16> = "StrataTray\0".encode_utf16().collect();
                    // -3 is HWND_MESSAGE in Win32
                    let hwnd = CreateWindowExW(
                        0,
                        class_name.as_ptr(),
                        window_title.as_ptr(),
                        0,
                        0,
                        0,
                        0,
                        0,
                        -3,
                        0,
                        instance,
                        std::ptr::null(),
                    );

                    if hwnd == 0 {
                        let _ = init_tx.send(Err(()));
                        return;
                    }

                    let hicon = create_native_hicon(&icon_rgba, icon_width, icon_height);

                    let mut nid = std::mem::zeroed::<NOTIFYICONDATAW>();
                    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
                    nid.hWnd = hwnd;
                    nid.uID = 1;
                    nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
                    nid.uCallbackMessage = WM_APP + 1;
                    nid.hIcon = hicon;
                    copy_to_wide_buf(&initial_tooltip, &mut nid.szTip);

                    if initial_visible {
                        Shell_NotifyIconW(NIM_ADD, &nid);
                    }

                    let _ = init_tx.send(Ok(hwnd));

                    let mut msg: MSG = std::mem::zeroed();
                    'tray_loop: loop {
                        while let Ok(cmd) = rx.try_recv() {
                            match cmd {
                                TrayCommand::SetTooltip(tip) => {
                                    copy_to_wide_buf(&tip, &mut nid.szTip);
                                    nid.uFlags = NIF_TIP;
                                    Shell_NotifyIconW(NIM_MODIFY, &nid);
                                }
                                TrayCommand::SetMenu(menu) => {
                                    shared.set_menu(menu);
                                }
                                TrayCommand::SetVisible(visible) => {
                                    if visible {
                                        nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
                                        Shell_NotifyIconW(NIM_ADD, &nid);
                                    } else {
                                        Shell_NotifyIconW(NIM_DELETE, &nid);
                                    }
                                }
                                TrayCommand::SetBadge(badge) => {
                                    if let Some(b) = badge {
                                        nid.uFlags = NIF_INFO;
                                        copy_to_wide_buf(&b, &mut nid.szInfo);
                                        copy_to_wide_buf(&initial_tooltip, &mut nid.szInfoTitle);
                                        Shell_NotifyIconW(NIM_MODIFY, &nid);
                                    }
                                }
                                TrayCommand::Destroy => {
                                    break 'tray_loop;
                                }
                            }
                        }

                        while PeekMessageW(&mut msg, hwnd, 0, 0, PM_REMOVE) != 0 {
                            if msg.message == WM_APP + 1 {
                                let event = (msg.lParam as u32) & 0xFFFF;
                                match event {
                                    WM_LBUTTONUP => {
                                        shared.dispatch_click();
                                    }
                                    WM_RBUTTONUP | WM_CONTEXTMENU => {
                                        SetForegroundWindow(hwnd);
                                        let hmenu = CreatePopupMenu();
                                        if hmenu != 0 {
                                            if let Ok(guard) = shared.menu.lock() {
                                                if let Some(menu) = guard.as_ref() {
                                                    populate_win32_menu(hmenu, &menu.items);
                                                }
                                            }
                                            let mut pt = POINT { x: 0, y: 0 };
                                            GetCursorPos(&mut pt);
                                            let cmd = TrackPopupMenuEx(
                                                hmenu,
                                                TPM_RIGHTBUTTON
                                                    | TPM_BOTTOMALIGN
                                                    | TPM_NONOTIFY
                                                    | TPM_RETURNCMD,
                                                pt.x,
                                                pt.y,
                                                hwnd,
                                                std::ptr::null(),
                                            );
                                            if cmd > 0 {
                                                shared.dispatch_menu_action(cmd as u64);
                                            }
                                            DestroyMenu(hmenu);
                                            PostMessageW(hwnd, WM_NULL, 0, 0);
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            DispatchMessageW(&msg);
                        }

                        std::thread::sleep(std::time::Duration::from_millis(16));
                    }

                    Shell_NotifyIconW(NIM_DELETE, &nid);
                    if hicon != 0 {
                        DestroyIcon(hicon);
                    }
                    DestroyWindow(hwnd);
                }
            });

            match init_rx.recv().ok()? {
                Ok(hwnd) => Some(Self {
                    command_tx: tx,
                    hwnd,
                    thread_handle: Some(thread_handle),
                }),
                Err(_) => None,
            }
        }

        pub fn set_tooltip(&mut self, tooltip: String) {
            let _ = self.command_tx.send(TrayCommand::SetTooltip(tooltip));
            unsafe {
                PostMessageW(self.hwnd, WM_NULL, 0, 0);
            }
        }

        pub fn set_menu(&mut self, menu: Option<&TrayMenu>) {
            let _ = self.command_tx.send(TrayCommand::SetMenu(menu.cloned()));
            unsafe {
                PostMessageW(self.hwnd, WM_NULL, 0, 0);
            }
        }

        pub fn set_visible(&mut self, visible: bool) {
            let _ = self.command_tx.send(TrayCommand::SetVisible(visible));
            unsafe {
                PostMessageW(self.hwnd, WM_NULL, 0, 0);
            }
        }

        pub fn set_badge(&mut self, badge: Option<String>) {
            let _ = self.command_tx.send(TrayCommand::SetBadge(badge));
            unsafe {
                PostMessageW(self.hwnd, WM_NULL, 0, 0);
            }
        }
    }

    impl Drop for LiveTrayIcon {
        fn drop(&mut self) {
            let _ = self.command_tx.send(TrayCommand::Destroy);
            unsafe {
                PostMessageW(self.hwnd, WM_NULL, 0, 0);
            }
            if let Some(handle) = self.thread_handle.take() {
                let _ = handle.join();
            }
        }
    }

    unsafe fn populate_win32_menu(parent: HMENU, items: &[TrayMenuItem]) {
        for item in items {
            if item.is_separator {
                AppendMenuW(parent, MF_SEPARATOR, 0, std::ptr::null());
            } else if !item.children.is_empty() {
                let sub = CreatePopupMenu();
                populate_win32_menu(sub, &item.children);
                let wide: Vec<u16> = item
                    .label
                    .encode_utf16()
                    .chain(std::iter::once(0))
                    .collect();
                AppendMenuW(parent, MF_POPUP, sub as usize, wide.as_ptr());
            } else {
                let mut flags = MF_STRING;
                if !item.enabled {
                    flags |= MF_GRAYED;
                }
                if item.checked {
                    flags |= MF_CHECKED;
                } else {
                    flags |= MF_UNCHECKED;
                }
                let wide: Vec<u16> = item
                    .label
                    .encode_utf16()
                    .chain(std::iter::once(0))
                    .collect();
                AppendMenuW(parent, flags, item.id as usize, wide.as_ptr());
            }
        }
    }

    fn copy_to_wide_buf(src: &str, dest: &mut [u16]) {
        let wide: Vec<u16> = src.encode_utf16().collect();
        let len = wide.len().min(dest.len().saturating_sub(1));
        dest[..len].copy_from_slice(&wide[..len]);
        dest[len] = 0;
    }

    unsafe fn create_native_hicon(rgba: &[u8], width: u32, height: u32) -> HICON {
        if width == 0 || height == 0 || rgba.len() < (width * height * 4) as usize {
            return windows_sys::Win32::UI::WindowsAndMessaging::LoadIconW(
                0,
                windows_sys::Win32::UI::WindowsAndMessaging::IDI_APPLICATION,
            );
        }
        let mask = CreateBitmap(width as i32, height as i32, 1, 1, std::ptr::null());
        let color = CreateBitmap(
            width as i32,
            height as i32,
            1,
            32,
            rgba.as_ptr() as *const _,
        );
        let icon_info = ICONINFO {
            fIcon: 1,
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: mask,
            hbmColor: color,
        };
        let hicon = CreateIconIndirect(&icon_info);
        DeleteObject(mask);
        DeleteObject(color);
        if hicon != 0 {
            hicon
        } else {
            windows_sys::Win32::UI::WindowsAndMessaging::LoadIconW(
                0,
                windows_sys::Win32::UI::WindowsAndMessaging::IDI_APPLICATION,
            )
        }
    }
}

#[cfg(target_os = "macos")]
mod macos_backend {
    //! macOS Cocoa System Tray implementation backed by `NSStatusBar` & `NSMenu`.
    use super::{SharedTrayState, TrayIcon, TrayMenu, TrayMenuItem};
    use std::ffi::{c_char, c_void, CString};
    use std::sync::Arc;

    extern "C" {
        fn objc_getClass(name: *const c_char) -> *mut c_void;
        fn sel_registerName(name: *const c_char) -> *mut c_void;
        fn objc_msgSend(receiver: *mut c_void, op: *mut c_void, ...) -> *mut c_void;
    }

    pub struct LiveStatusItem {
        status_item: *mut c_void,
        shared: Arc<SharedTrayState>,
    }

    impl LiveStatusItem {
        pub fn new(icon: &TrayIcon, shared: Arc<SharedTrayState>) -> Option<Self> {
            unsafe {
                let status_bar_cls = objc_getClass(b"NSStatusBar\0".as_ptr() as *const c_char);
                if status_bar_cls.is_null() {
                    return None;
                }
                let system_sb_sel =
                    sel_registerName(b"systemStatusBar\0".as_ptr() as *const c_char);
                let msg_sb: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
                    std::mem::transmute(objc_msgSend as *const ());
                let status_bar = msg_sb(status_bar_cls, system_sb_sel);
                if status_bar.is_null() {
                    return None;
                }

                // -1.0 represents NSVariableStatusItemLength in AppKit
                let item_sel =
                    sel_registerName(b"statusItemWithLength:\0".as_ptr() as *const c_char);
                let msg_item: unsafe extern "C" fn(*mut c_void, *mut c_void, f64) -> *mut c_void =
                    std::mem::transmute(objc_msgSend as *const ());
                let status_item = msg_item(status_bar, item_sel, -1.0);
                if status_item.is_null() {
                    return None;
                }

                let retain_sel = sel_registerName(b"retain\0".as_ptr() as *const c_char);
                let msg_retain: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
                    std::mem::transmute(objc_msgSend as *const ());
                msg_retain(status_item, retain_sel);

                let mut this = Self {
                    status_item,
                    shared,
                };
                this.set_tooltip(icon.tooltip.clone());
                this.set_menu(icon.menu.as_ref());
                this.set_badge(icon.badge.clone());
                this.set_visible(icon.visible);
                Some(this)
            }
        }

        pub fn set_tooltip(&mut self, tooltip: String) {
            unsafe {
                let btn_sel = sel_registerName(b"button\0".as_ptr() as *const c_char);
                let msg_btn: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
                    std::mem::transmute(objc_msgSend as *const ());
                let button = msg_btn(self.status_item, btn_sel);
                if !button.is_null() {
                    let tip_sel = sel_registerName(b"setToolTip:\0".as_ptr() as *const c_char);
                    let ns_str = create_ns_string(&tooltip);
                    let msg_tip: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) =
                        std::mem::transmute(objc_msgSend as *const ());
                    msg_tip(button, tip_sel, ns_str);
                }
            }
        }

        pub fn set_menu(&mut self, menu: Option<&TrayMenu>) {
            self.shared.set_menu(menu.cloned());
            unsafe {
                if let Some(menu) = menu {
                    let ns_menu = build_ns_menu(&menu.items, self.shared.clone());
                    let set_menu_sel =
                        sel_registerName(b"setMenu:\0".as_ptr() as *const c_char);
                    let msg_set_menu: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) =
                        std::mem::transmute(objc_msgSend as *const ());
                    msg_set_menu(self.status_item, set_menu_sel, ns_menu);
                } else {
                    let set_menu_sel =
                        sel_registerName(b"setMenu:\0".as_ptr() as *const c_char);
                    let msg_set_menu: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) =
                        std::mem::transmute(objc_msgSend as *const ());
                    msg_set_menu(self.status_item, set_menu_sel, std::ptr::null_mut());
                }
            }
        }

        pub fn set_visible(&mut self, visible: bool) {
            unsafe {
                let vis_sel = sel_registerName(b"setVisible:\0".as_ptr() as *const c_char);
                let msg_vis: unsafe extern "C" fn(*mut c_void, *mut c_void, bool) =
                    std::mem::transmute(objc_msgSend as *const ());
                msg_vis(self.status_item, vis_sel, visible);
            }
        }

        pub fn set_badge(&mut self, badge: Option<String>) {
            unsafe {
                let btn_sel = sel_registerName(b"button\0".as_ptr() as *const c_char);
                let msg_btn: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
                    std::mem::transmute(objc_msgSend as *const ());
                let button = msg_btn(self.status_item, btn_sel);
                if !button.is_null() {
                    let title_sel = sel_registerName(b"setTitle:\0".as_ptr() as *const c_char);
                    let ns_str = create_ns_string(badge.as_deref().unwrap_or(""));
                    let msg_title: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) =
                        std::mem::transmute(objc_msgSend as *const ());
                    msg_title(button, title_sel, ns_str);
                }
            }
        }
    }

    impl Drop for LiveStatusItem {
        fn drop(&mut self) {
            unsafe {
                let status_bar_cls =
                    objc_getClass(b"NSStatusBar\0".as_ptr() as *const c_char);
                if !status_bar_cls.is_null() {
                    let system_sb_sel =
                        sel_registerName(b"systemStatusBar\0".as_ptr() as *const c_char);
                    let msg_sb: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
                        std::mem::transmute(objc_msgSend as *const ());
                    let status_bar = msg_sb(status_bar_cls, system_sb_sel);
                    if !status_bar.is_null() {
                        let rm_sel =
                            sel_registerName(b"removeStatusItem:\0".as_ptr() as *const c_char);
                        let msg_rm: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) =
                            std::mem::transmute(objc_msgSend as *const ());
                        msg_rm(status_bar, rm_sel, self.status_item);
                    }
                }
                let release_sel = sel_registerName(b"release\0".as_ptr() as *const c_char);
                let msg_rel: unsafe extern "C" fn(*mut c_void, *mut c_void) =
                    std::mem::transmute(objc_msgSend as *const ());
                msg_rel(self.status_item, release_sel);
            }
        }
    }

    unsafe fn create_ns_string(s: &str) -> *mut c_void {
        let cls = objc_getClass(b"NSString\0".as_ptr() as *const c_char);
        let sel = sel_registerName(b"stringWithUTF8String:\0".as_ptr() as *const c_char);
        let cstr = CString::new(s).unwrap_or_default();
        let msg: unsafe extern "C" fn(*mut c_void, *mut c_void, *const c_char) -> *mut c_void =
            std::mem::transmute(objc_msgSend as *const ());
        msg(cls, sel, cstr.as_ptr())
    }

    unsafe fn build_ns_menu(
        items: &[TrayMenuItem],
        shared: Arc<SharedTrayState>,
    ) -> *mut c_void {
        let menu_cls = objc_getClass(b"NSMenu\0".as_ptr() as *const c_char);
        let alloc_sel = sel_registerName(b"alloc\0".as_ptr() as *const c_char);
        let init_sel = sel_registerName(b"initWithTitle:\0".as_ptr() as *const c_char);
        let msg_alloc: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
            std::mem::transmute(objc_msgSend as *const ());
        let msg_init: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) -> *mut c_void =
            std::mem::transmute(objc_msgSend as *const ());
        let menu_inst = msg_alloc(menu_cls, alloc_sel);
        let title_str = create_ns_string("StrataTrayMenu");
        let ns_menu = msg_init(menu_inst, init_sel, title_str);

        let add_item_sel = sel_registerName(b"addItem:\0".as_ptr() as *const c_char);
        let msg_add_item: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) =
            std::mem::transmute(objc_msgSend as *const ());

        for item in items {
            if item.is_separator {
                let item_cls = objc_getClass(b"NSMenuItem\0".as_ptr() as *const c_char);
                let sep_sel = sel_registerName(b"separatorItem\0".as_ptr() as *const c_char);
                let msg_sep: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
                    std::mem::transmute(objc_msgSend as *const ());
                let sep = msg_sep(item_cls, sep_sel);
                msg_add_item(ns_menu, add_item_sel, sep);
                continue;
            }

            let item_cls = objc_getClass(b"NSMenuItem\0".as_ptr() as *const c_char);
            let item_alloc = msg_alloc(item_cls, alloc_sel);
            let item_init_sel =
                sel_registerName(b"initWithTitle:action:keyEquivalent:\0".as_ptr() as *const c_char);
            let msg_item_init: unsafe extern "C" fn(
                *mut c_void,
                *mut c_void,
                *mut c_void,
                *mut c_void,
                *mut c_void,
            ) -> *mut c_void = std::mem::transmute(objc_msgSend as *const ());
            let label_str = create_ns_string(&item.label);
            let empty_str = create_ns_string("");
            let ns_item = msg_item_init(
                item_alloc,
                item_init_sel,
                label_str,
                std::ptr::null_mut(),
                empty_str,
            );

            let set_tag_sel = sel_registerName(b"setTag:\0".as_ptr() as *const c_char);
            let msg_tag: unsafe extern "C" fn(*mut c_void, *mut c_void, isize) =
                std::mem::transmute(objc_msgSend as *const ());
            msg_tag(ns_item, set_tag_sel, item.id as isize);

            let set_enabled_sel = sel_registerName(b"setEnabled:\0".as_ptr() as *const c_char);
            let msg_enabled: unsafe extern "C" fn(*mut c_void, *mut c_void, bool) =
                std::mem::transmute(objc_msgSend as *const ());
            msg_enabled(ns_item, set_enabled_sel, item.enabled);

            let set_state_sel = sel_registerName(b"setState:\0".as_ptr() as *const c_char);
            let msg_state: unsafe extern "C" fn(*mut c_void, *mut c_void, isize) =
                std::mem::transmute(objc_msgSend as *const ());
            msg_state(ns_item, set_state_sel, if item.checked { 1 } else { 0 });

            if !item.children.is_empty() {
                let sub_menu = build_ns_menu(&item.children, shared.clone());
                let set_sub_sel = sel_registerName(b"setSubmenu:\0".as_ptr() as *const c_char);
                let msg_sub: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) =
                    std::mem::transmute(objc_msgSend as *const ());
                msg_sub(ns_item, set_sub_sel, sub_menu);
            }

            msg_add_item(ns_menu, add_item_sel, ns_item);
        }

        ns_menu
    }
}

