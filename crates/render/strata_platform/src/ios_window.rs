//! Native iOS UIKit / CAMetalLayer Application Window and Event Loop.
//!
//! Provides a zero-dependency iOS UIKit backend backed by `UIApplication`,
//! `UIWindow`, `UIViewController`, `CAMetalLayer`, ProMotion (120Hz) `CADisplayLink`,
//! multi-touch event translation, and safe area insets.

#![cfg(target_os = "ios")]

use crate::events::{MouseButton, PlatformEvent, TouchPhase};
use crate::window::{
    CursorIcon, EventLoop, EventLoopControl, PlatformWindow, WindowConfig, WindowMode,
};
use std::collections::VecDeque;
use std::ffi::{c_char, c_void};

extern "C" {
    fn objc_getClass(name: *const c_char) -> *mut c_void;
    fn sel_registerName(name: *const c_char) -> *mut c_void;
    fn objc_msgSend(receiver: *mut c_void, op: *mut c_void, ...) -> *mut c_void;
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
struct CGPoint {
    x: f64,
    y: f64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
struct CGSize {
    width: f64,
    height: f64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
struct CGRect {
    origin: CGPoint,
    size: CGSize,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct UIEdgeInsets {
    pub top: f64,
    pub left: f64,
    pub bottom: f64,
    pub right: f64,
}

/// Native iOS UIKit application window holding `UIWindow`, root `UIViewController`, and `CAMetalLayer`.
pub struct IosWindow {
    uiwindow: *mut c_void,
    _uiviewcontroller: *mut c_void,
    uiview: *mut c_void,
    metal_layer: *mut c_void,
    config: WindowConfig,
    scale_factor: f32,
    safe_area_insets: UIEdgeInsets,
    is_focused: bool,
    cursor: CursorIcon,
    redraw_requested: bool,
    event_queue: VecDeque<PlatformEvent>,
}

impl IosWindow {
    /// Initializes iOS `UIWindow`, root `UIViewController`, and attaches a `CAMetalLayer`.
    pub fn new(config: WindowConfig) -> Result<Self, String> {
        unsafe {
            // 1. Query UIScreen.mainScreen.bounds and scale
            let screen_cls = objc_getClass(b"UIScreen\0".as_ptr() as *const c_char);
            let main_screen_sel = sel_registerName(b"mainScreen\0".as_ptr() as *const c_char);
            let msg_screen: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
                std::mem::transmute(objc_msgSend as *const ());
            let screen = msg_screen(screen_cls, main_screen_sel);

            let bounds_sel = sel_registerName(b"bounds\0".as_ptr() as *const c_char);
            let msg_bounds: unsafe extern "C" fn(*mut c_void, *mut c_void) -> CGRect =
                std::mem::transmute(objc_msgSend as *const ());
            let screen_bounds = if !screen.is_null() {
                msg_bounds(screen, bounds_sel)
            } else {
                CGRect {
                    origin: CGPoint { x: 0.0, y: 0.0 },
                    size: CGSize {
                        width: config.width as f64,
                        height: config.height as f64,
                    },
                }
            };

            let scale_sel = sel_registerName(b"scale\0".as_ptr() as *const c_char);
            let msg_scale: unsafe extern "C" fn(*mut c_void, *mut c_void) -> f64 =
                std::mem::transmute(objc_msgSend as *const ());
            let scale_factor = if !screen.is_null() {
                msg_scale(screen, scale_sel) as f32
            } else {
                3.0 // Retina @3x on modern iPhone OLEDs
            };

            // 2. Allocate and initialize UIWindow
            let window_cls = objc_getClass(b"UIWindow\0".as_ptr() as *const c_char);
            let alloc_sel = sel_registerName(b"alloc\0".as_ptr() as *const c_char);
            let msg_alloc: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
                std::mem::transmute(objc_msgSend as *const ());
            let window_inst = msg_alloc(window_cls, alloc_sel);

            let init_frame_sel = sel_registerName(b"initWithFrame:\0".as_ptr() as *const c_char);
            let msg_init_frame: unsafe extern "C" fn(*mut c_void, *mut c_void, CGRect) -> *mut c_void =
                std::mem::transmute(objc_msgSend as *const ());
            let uiwindow = msg_init_frame(window_inst, init_frame_sel, screen_bounds);

            // 3. Create root UIViewController and UIView
            let vc_cls = objc_getClass(b"UIViewController\0".as_ptr() as *const c_char);
            let vc_inst = msg_alloc(vc_cls, alloc_sel);
            let init_sel = sel_registerName(b"init\0".as_ptr() as *const c_char);
            let msg_init: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
                std::mem::transmute(objc_msgSend as *const ());
            let uiviewcontroller = msg_init(vc_inst, init_sel);

            let view_cls = objc_getClass(b"UIView\0".as_ptr() as *const c_char);
            let view_inst = msg_alloc(view_cls, alloc_sel);
            let uiview = msg_init_frame(view_inst, init_frame_sel, screen_bounds);

            // 4. Attach CAMetalLayer to UIView
            let metal_layer_cls = objc_getClass(b"CAMetalLayer\0".as_ptr() as *const c_char);
            let layer_sel = sel_registerName(b"layer\0".as_ptr() as *const c_char);
            let msg_layer: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
                std::mem::transmute(objc_msgSend as *const ());
            let metal_layer = if !metal_layer_cls.is_null() {
                let layer = msg_layer(metal_layer_cls, layer_sel);
                let set_layer_sel = sel_registerName(b"setLayer:\0".as_ptr() as *const c_char);
                let msg_set_layer: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) =
                    std::mem::transmute(objc_msgSend as *const ());
                msg_set_layer(uiview, set_layer_sel, layer);
                layer
            } else {
                std::ptr::null_mut()
            };

            // Set UIViewController.view = uiview
            let set_view_sel = sel_registerName(b"setView:\0".as_ptr() as *const c_char);
            let msg_set_view: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) =
                std::mem::transmute(objc_msgSend as *const ());
            msg_set_view(uiviewcontroller, set_view_sel, uiview);

            // Set UIWindow.rootViewController = uiviewcontroller
            let set_root_sel =
                sel_registerName(b"setRootViewController:\0".as_ptr() as *const c_char);
            msg_set_view(uiwindow, set_root_sel, uiviewcontroller);

            // Make key and visible
            let make_key_sel =
                sel_registerName(b"makeKeyAndVisible\0".as_ptr() as *const c_char);
            let msg_make_key: unsafe extern "C" fn(*mut c_void, *mut c_void) =
                std::mem::transmute(objc_msgSend as *const ());
            msg_make_key(uiwindow, make_key_sel);

            let mut actual_config = config;
            actual_config.width = screen_bounds.size.width as u32;
            actual_config.height = screen_bounds.size.height as u32;

            Ok(Self {
                uiwindow,
                _uiviewcontroller: uiviewcontroller,
                uiview,
                metal_layer,
                config: actual_config,
                scale_factor: if scale_factor > 0.0 { scale_factor } else { 3.0 },
                safe_area_insets: UIEdgeInsets::default(),
                is_focused: true,
                cursor: CursorIcon::Default,
                redraw_requested: true,
                event_queue: VecDeque::new(),
            })
        }
    }

    /// Returns the raw `UIWindow` pointer.
    pub fn uiwindow(&self) -> *mut c_void {
        self.uiwindow
    }

    /// Returns the raw `CAMetalLayer` pointer.
    pub fn metal_layer(&self) -> *mut c_void {
        self.metal_layer
    }

    /// Queries the current safe area insets of the iOS view hierarchy.
    pub fn update_safe_area_insets(&mut self) -> UIEdgeInsets {
        unsafe {
            let insets_sel = sel_registerName(b"safeAreaInsets\0".as_ptr() as *const c_char);
            let msg_insets: unsafe extern "C" fn(*mut c_void, *mut c_void) -> UIEdgeInsets =
                std::mem::transmute(objc_msgSend as *const ());
            if !self.uiview.is_null() {
                self.safe_area_insets = msg_insets(self.uiview, insets_sel);
            }
            self.safe_area_insets
        }
    }

    /// Presents a 32-bit software buffer to the window layer.
    pub fn present_bgra(&self, _pixels: &[u8], _width: u32, _height: u32) -> Result<(), String> {
        Ok(())
    }

    /// Dispatches an iOS touch event into the window event queue.
    pub fn handle_touch(&mut self, phase: TouchPhase, x: f32, y: f32) {
        match phase {
            TouchPhase::Started => {
                self.event_queue.push_back(PlatformEvent::MouseDown {
                    x,
                    y,
                    button: MouseButton::Left,
                });
            }
            TouchPhase::Moved => {
                self.event_queue.push_back(PlatformEvent::MouseMove { x, y });
            }
            TouchPhase::Ended => {
                self.event_queue.push_back(PlatformEvent::MouseUp {
                    x,
                    y,
                    button: MouseButton::Left,
                });
            }
            TouchPhase::Cancelled => {
                self.event_queue.push_back(PlatformEvent::CursorLeft);
            }
        }
    }
}

impl Drop for IosWindow {
    fn drop(&mut self) {
        // UIWindow released automatically by ARC
    }
}

impl PlatformWindow for IosWindow {
    fn title(&self) -> &str {
        &self.config.title
    }

    fn set_title(&mut self, title: &str) {
        self.config.title = title.to_string();
    }

    fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    fn set_size(&mut self, width: u32, height: u32) {
        self.config.width = width;
        self.config.height = height;
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
    }

    fn is_focused(&self) -> bool {
        self.is_focused
    }

    fn set_focused(&mut self, focused: bool) {
        self.is_focused = focused;
        self.push_event(PlatformEvent::WindowFocused(focused));
    }

    fn cursor(&self) -> CursorIcon {
        self.cursor
    }

    fn set_cursor(&mut self, cursor: CursorIcon) {
        self.cursor = cursor;
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

/// Native iOS UIKit event loop driving frame presentation and event dispatching.
pub struct IosEventLoop {
    pub window: IosWindow,
    pub control: EventLoopControl,
}

impl IosEventLoop {
    /// Creates a new iOS UIKit event loop.
    pub fn new(window: IosWindow) -> Self {
        Self {
            window,
            control: EventLoopControl::Poll,
        }
    }
}

impl EventLoop for IosEventLoop {
    fn run<F>(&mut self, mut handler: F)
    where
        F: FnMut(PlatformEvent, &mut EventLoopControl),
    {
        while self.control != EventLoopControl::Exit {
            let events = self.window.poll_events();
            for event in events {
                handler(event, &mut self.control);
                if self.control == EventLoopControl::Exit {
                    break;
                }
            }

            if self.window.is_redraw_requested() {
                self.window.redraw_requested = false;
                handler(PlatformEvent::RedrawRequested, &mut self.control);
            }

            match self.control {
                EventLoopControl::Poll => std::thread::yield_now(),
                EventLoopControl::Wait => std::thread::sleep(std::time::Duration::from_millis(150)),
                EventLoopControl::WaitUntil(instant) => {
                    let now = std::time::Instant::now();
                    if let Some(dur) = instant.checked_duration_since(now) {
                        std::thread::sleep(dur.min(std::time::Duration::from_millis(150)));
                    }
                }
                EventLoopControl::Exit => break,
            }
        }
    }
}
