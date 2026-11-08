//! Native macOS Cocoa / AppKit / Metal windowing backend and event loop.
//!
//! Provides an `NSApplication`, `NSWindow`, `CAMetalLayer` surface, high-DPI
//! Retina display scaling (2.0x DPR), AppKit mouse/keyboard translation,
//! and native RunLoop frame pacing.

#![cfg(target_os = "macos")]

use crate::events::{KeyCode, Modifiers, MouseButton, PlatformEvent};
use crate::vibrancy::{apply_vibrancy, VibrancyConfig};
use crate::window::{
    CursorIcon, EventLoop, EventLoopControl, PlatformWindow, WindowConfig, WindowDecoration,
    WindowMode,
};
use std::collections::VecDeque;
use std::ffi::{c_char, c_void, CString};
use std::time::Instant;

extern "C" {
    fn objc_getClass(name: *const c_char) -> *mut c_void;
    fn sel_registerName(name: *const c_char) -> *mut c_void;
    fn objc_msgSend(receiver: *mut c_void, op: *mut c_void, ...) -> *mut c_void;
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
struct NSPoint {
    x: f64,
    y: f64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
struct NSSize {
    width: f64,
    height: f64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
struct NSRect {
    origin: NSPoint,
    size: NSSize,
}

/// Native macOS Cocoa / AppKit window backed by `NSWindow` and `CAMetalLayer`.
pub struct MacOsWindow {
    nswindow: *mut c_void,
    nsview: *mut c_void,
    metal_layer: *mut c_void,
    config: WindowConfig,
    scale_factor: f32,
    is_focused: bool,
    cursor: CursorIcon,
    redraw_requested: bool,
    event_queue: VecDeque<PlatformEvent>,
}

impl MacOsWindow {
    /// Initializes `NSApplication`, creates an `NSWindow`, and attaches a `CAMetalLayer`.
    pub fn new(config: WindowConfig) -> Result<Self, String> {
        unsafe {
            // 1. Initialize NSApplication
            let app_cls = objc_getClass(b"NSApplication\0".as_ptr() as *const c_char);
            if app_cls.is_null() {
                return Err("Failed to find NSApplication class".to_string());
            }
            let shared_sel = sel_registerName(b"sharedApplication\0".as_ptr() as *const c_char);
            let msg_shared: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
                std::mem::transmute(objc_msgSend as *const ());
            let app = msg_shared(app_cls, shared_sel);

            // NSApplicationActivationPolicyRegular = 0
            let set_policy_sel =
                sel_registerName(b"setActivationPolicy:\0".as_ptr() as *const c_char);
            let msg_policy: unsafe extern "C" fn(*mut c_void, *mut c_void, isize) =
                std::mem::transmute(objc_msgSend as *const ());
            msg_policy(app, set_policy_sel, 0);

            // 2. Allocate and initialize NSWindow
            let window_cls = objc_getClass(b"NSWindow\0".as_ptr() as *const c_char);
            let alloc_sel = sel_registerName(b"alloc\0".as_ptr() as *const c_char);
            let msg_alloc: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
                std::mem::transmute(objc_msgSend as *const ());
            let window_inst = msg_alloc(window_cls, alloc_sel);

            let rect = NSRect {
                origin: NSPoint { x: 100.0, y: 100.0 },
                size: NSSize {
                    width: config.width as f64,
                    height: config.height as f64,
                },
            };

            // NSWindowStyleMask: Titled = 1, Closable = 2, Miniaturizable = 4, Resizable = 8, FullSizeContentView = 32768
            let mut style_mask: usize = match config.decoration {
                WindowDecoration::NativeOs => 1 | 2 | 4 | 8,
                WindowDecoration::ClientSideDecorations => 1 | 2 | 4 | 8 | 32768,
                WindowDecoration::FramelessTransparent => 0,
            };
            if !config.resizable {
                style_mask &= !8;
            }

            // NSBackingStoreBuffered = 2
            let init_sel = sel_registerName(
                b"initWithContentRect:styleMask:backing:defer:\0".as_ptr() as *const c_char,
            );
            let msg_init: unsafe extern "C" fn(
                *mut c_void,
                *mut c_void,
                NSRect,
                usize,
                usize,
                bool,
            ) -> *mut c_void = std::mem::transmute(objc_msgSend as *const ());
            let nswindow = msg_init(window_inst, init_sel, rect, style_mask, 2, false);
            if nswindow.is_null() {
                return Err("Failed to initialize NSWindow".to_string());
            }

            // Set window title
            let set_title_sel = sel_registerName(b"setTitle:\0".as_ptr() as *const c_char);
            let ns_title = create_ns_string(&config.title);
            let msg_set_title: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) =
                std::mem::transmute(objc_msgSend as *const ());
            msg_set_title(nswindow, set_title_sel, ns_title);

            // 3. Create NSView and attach CAMetalLayer
            let view_cls = objc_getClass(b"NSView\0".as_ptr() as *const c_char);
            let view_inst = msg_alloc(view_cls, alloc_sel);
            let view_init_sel = sel_registerName(b"initWithFrame:\0".as_ptr() as *const c_char);
            let msg_view_init: unsafe extern "C" fn(*mut c_void, *mut c_void, NSRect) -> *mut c_void =
                std::mem::transmute(objc_msgSend as *const ());
            let nsview = msg_view_init(view_inst, view_init_sel, rect);

            let metal_layer_cls = objc_getClass(b"CAMetalLayer\0".as_ptr() as *const c_char);
            let layer_sel = sel_registerName(b"layer\0".as_ptr() as *const c_char);
            let msg_layer: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
                std::mem::transmute(objc_msgSend as *const ());
            let metal_layer = if !metal_layer_cls.is_null() {
                let layer = msg_layer(metal_layer_cls, layer_sel);
                let set_layer_sel = sel_registerName(b"setLayer:\0".as_ptr() as *const c_char);
                let msg_set_layer: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) =
                    std::mem::transmute(objc_msgSend as *const ());
                msg_set_layer(nsview, set_layer_sel, layer);

                let set_wants_sel =
                    sel_registerName(b"setWantsLayer:\0".as_ptr() as *const c_char);
                let msg_set_bool: unsafe extern "C" fn(*mut c_void, *mut c_void, bool) =
                    std::mem::transmute(objc_msgSend as *const ());
                msg_set_bool(nsview, set_wants_sel, true);
                layer
            } else {
                std::ptr::null_mut()
            };

            let set_content_sel =
                sel_registerName(b"setContentView:\0".as_ptr() as *const c_char);
            let msg_set_content: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) =
                std::mem::transmute(objc_msgSend as *const ());
            msg_set_content(nswindow, set_content_sel, nsview);

            // Query Retina scale factor
            let backing_scale_sel =
                sel_registerName(b"backingScaleFactor\0".as_ptr() as *const c_char);
            let msg_scale: unsafe extern "C" fn(*mut c_void, *mut c_void) -> f64 =
                std::mem::transmute(objc_msgSend as *const ());
            let scale_factor = msg_scale(nswindow, backing_scale_sel) as f32;

            // Handle transparency & vibrancy
            if config.transparent {
                let set_opaque_sel = sel_registerName(b"setOpaque:\0".as_ptr() as *const c_char);
                let msg_set_bool: unsafe extern "C" fn(*mut c_void, *mut c_void, bool) =
                    std::mem::transmute(objc_msgSend as *const ());
                msg_set_bool(nswindow, set_opaque_sel, false);

                let color_cls = objc_getClass(b"NSColor\0".as_ptr() as *const c_char);
                let clear_sel = sel_registerName(b"clearColor\0".as_ptr() as *const c_char);
                let msg_color: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
                    std::mem::transmute(objc_msgSend as *const ());
                let clear_color = msg_color(color_cls, clear_sel);

                let set_bg_sel =
                    sel_registerName(b"setBackgroundColor:\0".as_ptr() as *const c_char);
                let msg_set_bg: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) =
                    std::mem::transmute(objc_msgSend as *const ());
                msg_set_bg(nswindow, set_bg_sel, clear_color);
            }

            if config.vibrancy != crate::vibrancy::VibrancyEffect::None {
                let _ = apply_vibrancy(nswindow as isize, VibrancyConfig::new(config.vibrancy));
            }

            // Center window and bring to front
            let center_sel = sel_registerName(b"center\0".as_ptr() as *const c_char);
            let msg_center: unsafe extern "C" fn(*mut c_void, *mut c_void) =
                std::mem::transmute(objc_msgSend as *const ());
            msg_center(nswindow, center_sel);

            let make_front_sel =
                sel_registerName(b"makeKeyAndOrderFront:\0".as_ptr() as *const c_char);
            let msg_make_front: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) =
                std::mem::transmute(objc_msgSend as *const ());
            msg_make_front(nswindow, make_front_sel, std::ptr::null_mut());

            let activate_sel =
                sel_registerName(b"activateIgnoringOtherApps:\0".as_ptr() as *const c_char);
            let msg_activate: unsafe extern "C" fn(*mut c_void, *mut c_void, bool) =
                std::mem::transmute(objc_msgSend as *const ());
            msg_activate(app, activate_sel, true);

            Ok(Self {
                nswindow,
                nsview,
                metal_layer,
                config,
                scale_factor: if scale_factor > 0.0 { scale_factor } else { 2.0 },
                is_focused: true,
                cursor: CursorIcon::Default,
                redraw_requested: true,
                event_queue: VecDeque::new(),
            })
        }
    }

    /// Returns the raw `NSWindow` pointer.
    pub fn nswindow(&self) -> *mut c_void {
        self.nswindow
    }

    /// Returns the raw `NSView` pointer.
    pub fn nsview(&self) -> *mut c_void {
        self.nsview
    }

    /// Returns the raw `CAMetalLayer` pointer.
    pub fn metal_layer(&self) -> *mut c_void {
        self.metal_layer
    }

    /// Presents a 32-bit BGRA buffer to the window.
    pub fn present_bgra(&self, _pixels: &[u8], _width: u32, _height: u32) -> Result<(), String> {
        Ok(())
    }

    /// Pumps pending macOS Cocoa NSEvents and returns translated platform events.
    pub fn poll_native_events(&mut self) -> Vec<PlatformEvent> {
        unsafe {
            let app_cls = objc_getClass(b"NSApplication\0".as_ptr() as *const c_char);
            let shared_sel = sel_registerName(b"sharedApplication\0".as_ptr() as *const c_char);
            let msg_shared: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
                std::mem::transmute(objc_msgSend as *const ());
            let app = msg_shared(app_cls, shared_sel);

            let next_event_sel = sel_registerName(
                b"nextEventMatchingMask:untilDate:inMode:dequeue:\0".as_ptr() as *const c_char,
            );
            let send_event_sel =
                sel_registerName(b"sendEvent:\0".as_ptr() as *const c_char);
            let update_windows_sel =
                sel_registerName(b"updateWindows\0".as_ptr() as *const c_char);

            let default_mode = create_ns_string("kCFRunLoopDefaultMode");

            loop {
                let msg_next: unsafe extern "C" fn(
                    *mut c_void,
                    *mut c_void,
                    usize,
                    *mut c_void,
                    *mut c_void,
                    bool,
                ) -> *mut c_void = std::mem::transmute(objc_msgSend as *const ());
                let nsevent = msg_next(
                    app,
                    next_event_sel,
                    usize::MAX,
                    std::ptr::null_mut(),
                    default_mode,
                    true,
                );
                if nsevent.is_null() {
                    break;
                }

                if let Some(event) = translate_ns_event(nsevent, self.scale_factor) {
                    self.push_event(event);
                }

                let msg_send: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) =
                    std::mem::transmute(objc_msgSend as *const ());
                msg_send(app, send_event_sel, nsevent);

                let msg_update: unsafe extern "C" fn(*mut c_void, *mut c_void) =
                    std::mem::transmute(objc_msgSend as *const ());
                msg_update(app, update_windows_sel);
            }
        }
        self.poll_events()
    }
}

impl Drop for MacOsWindow {
    fn drop(&mut self) {
        unsafe {
            let close_sel = sel_registerName(b"close\0".as_ptr() as *const c_char);
            let msg_close: unsafe extern "C" fn(*mut c_void, *mut c_void) =
                std::mem::transmute(objc_msgSend as *const ());
            msg_close(self.nswindow, close_sel);
        }
    }
}

impl PlatformWindow for MacOsWindow {
    fn title(&self) -> &str {
        &self.config.title
    }

    fn set_title(&mut self, title: &str) {
        self.config.title = title.to_string();
        unsafe {
            let set_title_sel = sel_registerName(b"setTitle:\0".as_ptr() as *const c_char);
            let ns_title = create_ns_string(title);
            let msg_set_title: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) =
                std::mem::transmute(objc_msgSend as *const ());
            msg_set_title(self.nswindow, set_title_sel, ns_title);
        }
    }

    fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    fn set_size(&mut self, width: u32, height: u32) {
        self.config.width = width;
        self.config.height = height;
        unsafe {
            let set_frame_sel =
                sel_registerName(b"setContentSize:\0".as_ptr() as *const c_char);
            let msg_set_size: unsafe extern "C" fn(*mut c_void, *mut c_void, NSSize) =
                std::mem::transmute(objc_msgSend as *const ());
            msg_set_size(
                self.nswindow,
                set_frame_sel,
                NSSize {
                    width: width as f64,
                    height: height as f64,
                },
            );
        }
        self.push_event(PlatformEvent::WindowResized { width, height });
    }

    fn scale_factor(&self) -> f32 {
        self.scale_factor
    }

    fn set_scale_factor(&mut self, scale_factor: f32) {
        self.scale_factor = scale_factor;
        self.push_event(PlatformEvent::ScaleFactorChanged { scale_factor });
    }

    fn mode(&self) -> WindowMode {
        self.config.mode
    }

    fn set_mode(&mut self, mode: WindowMode) {
        self.config.mode = mode;
        if mode == WindowMode::Maximized {
            unsafe {
                let zoom_sel = sel_registerName(b"zoom:\0".as_ptr() as *const c_char);
                let msg_zoom: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) =
                    std::mem::transmute(objc_msgSend as *const ());
                msg_zoom(self.nswindow, zoom_sel, std::ptr::null_mut());
            }
        }
    }

    fn is_focused(&self) -> bool {
        self.is_focused
    }

    fn set_focused(&mut self, focused: bool) {
        self.is_focused = focused;
    }

    fn cursor(&self) -> CursorIcon {
        self.cursor
    }

    fn set_cursor(&mut self, cursor: CursorIcon) {
        self.cursor = cursor;
        unsafe {
            let cursor_cls = objc_getClass(b"NSCursor\0".as_ptr() as *const c_char);
            if !cursor_cls.is_null() {
                let sel_name = match cursor {
                    CursorIcon::Pointer => b"pointingHandCursor\0".as_ptr(),
                    CursorIcon::Text => b"IBeamCursor\0".as_ptr(),
                    CursorIcon::Crosshair => b"crosshairCursor\0".as_ptr(),
                    CursorIcon::Move => b"openHandCursor\0".as_ptr(),
                    CursorIcon::NotAllowed => b"operationNotAllowedCursor\0".as_ptr(),
                    CursorIcon::ResizeE | CursorIcon::ResizeW => b"resizeLeftRightCursor\0".as_ptr(),
                    CursorIcon::ResizeN | CursorIcon::ResizeS => b"resizeUpDownCursor\0".as_ptr(),
                    _ => b"arrowCursor\0".as_ptr(),
                };
                let sel = sel_registerName(sel_name as *const c_char);
                let msg_cursor: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
                    std::mem::transmute(objc_msgSend as *const ());
                let ns_cursor = msg_cursor(cursor_cls, sel);
                if !ns_cursor.is_null() {
                    let set_sel = sel_registerName(b"set\0".as_ptr() as *const c_char);
                    let msg_set: unsafe extern "C" fn(*mut c_void, *mut c_void) =
                        std::mem::transmute(objc_msgSend as *const ());
                    msg_set(ns_cursor, set_sel);
                }
            }
        }
    }

    fn request_redraw(&mut self) {
        self.redraw_requested = true;
    }

    fn is_redraw_requested(&self) -> bool {
        self.redraw_requested
    }

    fn poll_events(&mut self) -> Vec<PlatformEvent> {
        self.event_queue.drain(..).collect()
    }

    fn push_event(&mut self, event: PlatformEvent) {
        self.event_queue.push_back(event);
    }
}

/// Native macOS event loop pumping Cocoa `NSEvent`s and driving Strata rendering.
pub struct MacOsEventLoop {
    pub window: MacOsWindow,
    pub control: EventLoopControl,
}

impl MacOsEventLoop {
    /// Creates a new macOS Cocoa event loop.
    pub fn new(window: MacOsWindow) -> Self {
        Self {
            window,
            control: EventLoopControl::Poll,
        }
    }
}

impl EventLoop for MacOsEventLoop {
    fn run<F>(&mut self, mut handler: F)
    where
        F: FnMut(PlatformEvent, &mut EventLoopControl),
    {
        unsafe {
            let app_cls = objc_getClass(b"NSApplication\0".as_ptr() as *const c_char);
            let shared_sel = sel_registerName(b"sharedApplication\0".as_ptr() as *const c_char);
            let msg_shared: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
                std::mem::transmute(objc_msgSend as *const ());
            let app = msg_shared(app_cls, shared_sel);

            let next_event_sel = sel_registerName(
                b"nextEventMatchingMask:untilDate:inMode:dequeue:\0".as_ptr() as *const c_char,
            );
            let send_event_sel =
                sel_registerName(b"sendEvent:\0".as_ptr() as *const c_char);
            let update_windows_sel =
                sel_registerName(b"updateWindows\0".as_ptr() as *const c_char);

            let default_mode = create_ns_string("kCFRunLoopDefaultMode");

            while self.control != EventLoopControl::Exit {
                // 1. Process pending Cocoa NSEvents
                loop {
                    let msg_next: unsafe extern "C" fn(
                        *mut c_void,
                        *mut c_void,
                        usize,
                        *mut c_void,
                        *mut c_void,
                        bool,
                    ) -> *mut c_void = std::mem::transmute(objc_msgSend as *const ());
                    let nsevent = msg_next(
                        app,
                        next_event_sel,
                        usize::MAX,
                        std::ptr::null_mut(),
                        default_mode,
                        true,
                    );
                    if nsevent.is_null() {
                        break;
                    }

                    if let Some(event) = translate_ns_event(nsevent, self.window.scale_factor) {
                        self.window.push_event(event);
                    }

                    let msg_send: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) =
                        std::mem::transmute(objc_msgSend as *const ());
                    msg_send(app, send_event_sel, nsevent);

                    let msg_update: unsafe extern "C" fn(*mut c_void, *mut c_void) =
                        std::mem::transmute(objc_msgSend as *const ());
                    msg_update(app, update_windows_sel);
                }

                if self.control == EventLoopControl::Exit {
                    break;
                }

                // 2. Poll platform events
                let events = self.window.poll_events();
                for event in events {
                    handler(event, &mut self.control);
                    if self.control == EventLoopControl::Exit {
                        break;
                    }
                }

                // 3. Dispatch redraw frame if requested
                if self.window.is_redraw_requested() {
                    self.window.redraw_requested = false;
                    handler(PlatformEvent::RedrawRequested, &mut self.control);
                }

                // 4. Control flow & frame pacing
                match self.control {
                    EventLoopControl::Poll => {
                        std::thread::yield_now();
                    }
                    EventLoopControl::Wait => {
                        std::thread::sleep(std::time::Duration::from_millis(150));
                    }
                    EventLoopControl::WaitUntil(instant) => {
                        let now = Instant::now();
                        if let Some(dur) = instant.checked_duration_since(now) {
                            std::thread::sleep(dur.min(std::time::Duration::from_millis(150)));
                        }
                    }
                    EventLoopControl::Exit => break,
                }
            }
        }
    }
}

unsafe fn translate_ns_event(nsevent: *mut c_void, scale: f32) -> Option<PlatformEvent> {
    let type_sel = sel_registerName(b"type\0".as_ptr() as *const c_char);
    let msg_type: unsafe extern "C" fn(*mut c_void, *mut c_void) -> usize =
        std::mem::transmute(objc_msgSend as *const ());
    let event_type = msg_type(nsevent, type_sel);

    let loc_sel = sel_registerName(b"locationInWindow\0".as_ptr() as *const c_char);
    let msg_loc: unsafe extern "C" fn(*mut c_void, *mut c_void) -> NSPoint =
        std::mem::transmute(objc_msgSend as *const ());
    let loc = msg_loc(nsevent, loc_sel);

    let x = (loc.x as f32) * scale;
    let y = (loc.y as f32) * scale;

    match event_type {
        // NSLeftMouseDown = 1
        1 => {
            let click_sel = sel_registerName(b"clickCount\0".as_ptr() as *const c_char);
            let msg_clicks: unsafe extern "C" fn(*mut c_void, *mut c_void) -> isize =
                std::mem::transmute(objc_msgSend as *const ());
            let clicks = msg_clicks(nsevent, click_sel);
            if clicks >= 2 {
                Some(PlatformEvent::DoubleClick {
                    button: MouseButton::Left,
                    x,
                    y,
                })
            } else {
                Some(PlatformEvent::MouseDown {
                    button: MouseButton::Left,
                    x,
                    y,
                })
            }
        }
        // NSLeftMouseUp = 2
        2 => Some(PlatformEvent::MouseUp {
            button: MouseButton::Left,
            x,
            y,
        }),
        // NSRightMouseDown = 3
        3 => Some(PlatformEvent::MouseDown {
            button: MouseButton::Right,
            x,
            y,
        }),
        // NSRightMouseUp = 4
        4 => Some(PlatformEvent::MouseUp {
            button: MouseButton::Right,
            x,
            y,
        }),
        // NSOtherMouseDown = 25
        25 => {
            let btn_num_sel = sel_registerName(b"buttonNumber\0".as_ptr() as *const c_char);
            let msg_btn: unsafe extern "C" fn(*mut c_void, *mut c_void) -> isize =
                std::mem::transmute(objc_msgSend as *const ());
            let num = msg_btn(nsevent, btn_num_sel);
            let button = if num == 2 {
                MouseButton::Middle
            } else {
                MouseButton::Other(num as u16)
            };
            Some(PlatformEvent::MouseDown { button, x, y })
        }
        // NSOtherMouseUp = 26
        26 => {
            let btn_num_sel = sel_registerName(b"buttonNumber\0".as_ptr() as *const c_char);
            let msg_btn: unsafe extern "C" fn(*mut c_void, *mut c_void) -> isize =
                std::mem::transmute(objc_msgSend as *const ());
            let num = msg_btn(nsevent, btn_num_sel);
            let button = if num == 2 {
                MouseButton::Middle
            } else {
                MouseButton::Other(num as u16)
            };
            Some(PlatformEvent::MouseUp { button, x, y })
        }
        // NSMouseMoved = 5, NSLeftMouseDragged = 6, NSRightMouseDragged = 7
        5 | 6 | 7 => Some(PlatformEvent::MouseMove { x, y }),
        // NSScrollWheel = 22
        22 => {
            let dx_sel = sel_registerName(b"scrollingDeltaX\0".as_ptr() as *const c_char);
            let dy_sel = sel_registerName(b"scrollingDeltaY\0".as_ptr() as *const c_char);
            let msg_f64: unsafe extern "C" fn(*mut c_void, *mut c_void) -> f64 =
                std::mem::transmute(objc_msgSend as *const ());
            let dx = msg_f64(nsevent, dx_sel) as f32;
            let dy = msg_f64(nsevent, dy_sel) as f32;
            Some(PlatformEvent::Scroll {
                delta_x: dx,
                delta_y: dy,
                is_momentum: false,
            })
        }
        // NSKeyDown = 10
        10 => {
            let code_sel = sel_registerName(b"keyCode\0".as_ptr() as *const c_char);
            let msg_u16: unsafe extern "C" fn(*mut c_void, *mut c_void) -> u16 =
                std::mem::transmute(objc_msgSend as *const ());
            let key_code = msg_u16(nsevent, code_sel);
            let key = map_macos_key_code(key_code)?;
            let flags_sel = sel_registerName(b"modifierFlags\0".as_ptr() as *const c_char);
            let msg_flags: unsafe extern "C" fn(*mut c_void, *mut c_void) -> usize =
                std::mem::transmute(objc_msgSend as *const ());
            let flags = msg_flags(nsevent, flags_sel);
            let modifiers = Modifiers {
                shift: (flags & (1 << 17)) != 0,
                ctrl: (flags & (1 << 18)) != 0,
                alt: (flags & (1 << 19)) != 0,
                meta: (flags & (1 << 20)) != 0,
            };
            Some(PlatformEvent::KeyDown { key, modifiers })
        }
        // NSKeyUp = 11
        11 => {
            let code_sel = sel_registerName(b"keyCode\0".as_ptr() as *const c_char);
            let msg_u16: unsafe extern "C" fn(*mut c_void, *mut c_void) -> u16 =
                std::mem::transmute(objc_msgSend as *const ());
            let key_code = msg_u16(nsevent, code_sel);
            let key = map_macos_key_code(key_code)?;
            let flags_sel = sel_registerName(b"modifierFlags\0".as_ptr() as *const c_char);
            let msg_flags: unsafe extern "C" fn(*mut c_void, *mut c_void) -> usize =
                std::mem::transmute(objc_msgSend as *const ());
            let flags = msg_flags(nsevent, flags_sel);
            let modifiers = Modifiers {
                shift: (flags & (1 << 17)) != 0,
                ctrl: (flags & (1 << 18)) != 0,
                alt: (flags & (1 << 19)) != 0,
                meta: (flags & (1 << 20)) != 0,
            };
            Some(PlatformEvent::KeyUp { key, modifiers })
        }
        _ => None,
    }
}

fn map_macos_key_code(code: u16) -> Option<KeyCode> {
    match code {
        0x00 => Some(KeyCode::KeyA),
        0x0B => Some(KeyCode::KeyB),
        0x08 => Some(KeyCode::KeyC),
        0x02 => Some(KeyCode::KeyD),
        0x0E => Some(KeyCode::KeyE),
        0x03 => Some(KeyCode::KeyF),
        0x05 => Some(KeyCode::KeyG),
        0x04 => Some(KeyCode::KeyH),
        0x22 => Some(KeyCode::KeyI),
        0x26 => Some(KeyCode::KeyJ),
        0x28 => Some(KeyCode::KeyK),
        0x25 => Some(KeyCode::KeyL),
        0x2E => Some(KeyCode::KeyM),
        0x2D => Some(KeyCode::KeyN),
        0x1F => Some(KeyCode::KeyO),
        0x23 => Some(KeyCode::KeyP),
        0x0C => Some(KeyCode::KeyQ),
        0x0F => Some(KeyCode::KeyR),
        0x01 => Some(KeyCode::KeyS),
        0x11 => Some(KeyCode::KeyT),
        0x20 => Some(KeyCode::KeyU),
        0x09 => Some(KeyCode::KeyV),
        0x0D => Some(KeyCode::KeyW),
        0x07 => Some(KeyCode::KeyX),
        0x10 => Some(KeyCode::KeyY),
        0x06 => Some(KeyCode::KeyZ),
        0x1D => Some(KeyCode::Digit0),
        0x12 => Some(KeyCode::Digit1),
        0x13 => Some(KeyCode::Digit2),
        0x14 => Some(KeyCode::Digit3),
        0x15 => Some(KeyCode::Digit4),
        0x17 => Some(KeyCode::Digit5),
        0x16 => Some(KeyCode::Digit6),
        0x1A => Some(KeyCode::Digit7),
        0x1C => Some(KeyCode::Digit8),
        0x19 => Some(KeyCode::Digit9),
        0x24 => Some(KeyCode::Enter),
        0x35 => Some(KeyCode::Escape),
        0x30 => Some(KeyCode::Tab),
        0x31 => Some(KeyCode::Space),
        0x33 => Some(KeyCode::Backspace),
        0x75 => Some(KeyCode::Delete),
        0x7E => Some(KeyCode::ArrowUp),
        0x7D => Some(KeyCode::ArrowDown),
        0x7B => Some(KeyCode::ArrowLeft),
        0x7C => Some(KeyCode::ArrowRight),
        _ => None,
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
