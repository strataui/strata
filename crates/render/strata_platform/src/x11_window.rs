//! Native Linux X11 Windowing, Event Loop, and Software Surface Presentation.
//!
//! Provides zero-overhead desktop window creation, native event processing,
//! and hardware presentation on Linux X11 display servers.

#![cfg(target_os = "linux")]

use std::collections::VecDeque;
use std::ffi::CString;
use std::os::raw::{c_char, c_int, c_ulong};
use std::time::Duration;
use x11::xlib;

use crate::events::{KeyCode, Modifiers, MouseButton, PlatformEvent};
use crate::surface::PresentError;
use crate::window::{
    CursorIcon, EventLoop, EventLoopControl, PlatformWindow, WindowConfig, WindowMode,
};

/// Native Linux X11 window instance managing the X server connection, window lifecycle, and frame presentation.
pub struct X11Window {
    pub display: *mut x11::xlib::Display,
    pub window: x11::xlib::Window,
    pub gc: x11::xlib::GC,
    pub wm_delete_window: x11::xlib::Atom,
    pub config: WindowConfig,
    pub scale_factor: f32,
    pub is_focused: bool,
    pub cursor: CursorIcon,
    pub redraw_requested: bool,
    pub event_queue: VecDeque<PlatformEvent>,
    pub last_click: Option<(std::time::Instant, f32, f32)>,
}

impl X11Window {
    /// Creates a native X11 desktop window according to the provided configuration.
    pub fn new(config: WindowConfig) -> Result<Self, String> {
        unsafe {
            let display = xlib::XOpenDisplay(std::ptr::null());
            if display.is_null() {
                return Err(
                    "Failed to open X11 display. Ensure DISPLAY environment variable is valid."
                        .to_string(),
                );
            }

            let screen = xlib::XDefaultScreen(display);
            let root = xlib::XRootWindow(display, screen);
            let black_pixel = xlib::XBlackPixel(display, screen);

            let width = config.width.max(100);
            let height = config.height.max(100);

            let window = xlib::XCreateSimpleWindow(
                display,
                root,
                80,
                80,
                width,
                height,
                0,
                black_pixel,
                black_pixel,
            );

            if window == 0 {
                xlib::XCloseDisplay(display);
                return Err("Failed to create X11 window.".to_string());
            }

            // Set Window Title
            let c_title = CString::new(config.title.as_str())
                .unwrap_or_else(|_| CString::new("Strata Application").unwrap());
            xlib::XStoreName(display, window, c_title.as_ptr());

            // Intercept window close (X button) via WM_DELETE_WINDOW
            let wm_delete_str = CString::new("WM_DELETE_WINDOW").unwrap();
            let wm_delete_window = xlib::XInternAtom(display, wm_delete_str.as_ptr(), 0);
            let mut protocols = [wm_delete_window];
            xlib::XSetWMProtocols(display, window, protocols.as_mut_ptr(), 1);

            // Select X11 event mask
            let event_mask = xlib::ExposureMask
                | xlib::KeyPressMask
                | xlib::KeyReleaseMask
                | xlib::ButtonPressMask
                | xlib::ButtonReleaseMask
                | xlib::PointerMotionMask
                | xlib::StructureNotifyMask
                | xlib::FocusChangeMask;
            xlib::XSelectInput(display, window, event_mask);

            // Create graphics context
            let gc = xlib::XCreateGC(display, window, 0, std::ptr::null_mut());

            // Map and flush to display on screen
            xlib::XMapWindow(display, window);
            xlib::XFlush(display);

            Ok(Self {
                display,
                window,
                gc,
                wm_delete_window,
                config,
                scale_factor: 1.0,
                is_focused: true,
                cursor: CursorIcon::Default,
                redraw_requested: true,
                event_queue: VecDeque::new(),
                last_click: None,
            })
        }
    }

    /// Presents a BGRA8 pixel slice directly to the X11 window.
    pub fn present_bgra(
        &mut self,
        buffer: &[u8],
        width: u32,
        height: u32,
    ) -> Result<(), PresentError> {
        if self.display.is_null() || self.window == 0 {
            return Err(PresentError::SurfaceLost);
        }

        unsafe {
            let screen = xlib::XDefaultScreen(self.display);
            let visual = xlib::XDefaultVisual(self.display, screen);
            let depth = xlib::XDefaultDepth(self.display, screen);

            let data_ptr = buffer.as_ptr() as *mut c_char;

            let image = xlib::XCreateImage(
                self.display,
                visual,
                depth as u32,
                xlib::ZPixmap,
                0,
                data_ptr,
                width,
                height,
                32,
                (width * 4) as i32,
            );

            if image.is_null() {
                return Err(PresentError::Custom("XCreateImage failed".to_string()));
            }

            xlib::XPutImage(
                self.display,
                self.window,
                self.gc,
                image,
                0,
                0,
                0,
                0,
                width,
                height,
            );

            xlib::XFlush(self.display);

            // Detach buffer pointer before destruction so Xlib does not free Rust's slice
            (*image).data = std::ptr::null_mut();
            xlib::XDestroyImage(image);
        }

        self.redraw_requested = false;
        Ok(())
    }

    /// Polls pending X11 events without blocking and converts them to Strata PlatformEvents.
    pub fn poll_native_events(&mut self) -> Vec<PlatformEvent> {
        let mut events = Vec::new();
        if self.display.is_null() {
            return events;
        }

        unsafe {
            while xlib::XPending(self.display) > 0 {
                let mut xevent: xlib::XEvent = std::mem::zeroed();
                xlib::XNextEvent(self.display, &mut xevent);

                match xevent.get_type() {
                    xlib::Expose => {
                        self.redraw_requested = true;
                    }
                    xlib::ConfigureNotify => {
                        let conf = xevent.configure;
                        let w = conf.width as u32;
                        let h = conf.height as u32;
                        if w != self.config.width || h != self.config.height {
                            self.config.width = w;
                            self.config.height = h;
                            self.redraw_requested = true;
                            events.push(PlatformEvent::WindowResized {
                                width: w,
                                height: h,
                            });
                        }
                    }
                    xlib::ButtonPress => {
                        let bp = xevent.button;
                        let x = bp.x as f32;
                        let y = bp.y as f32;
                        match bp.button {
                            1 => {
                                let now = std::time::Instant::now();
                                if let Some((prev_time, px, py)) = self.last_click {
                                    if now.duration_since(prev_time).as_millis() < 400
                                        && (px - x).abs() < 6.0
                                        && (py - y).abs() < 6.0
                                    {
                                        events.push(PlatformEvent::DoubleClick {
                                            button: MouseButton::Left,
                                            x,
                                            y,
                                        });
                                        self.last_click = None;
                                    } else {
                                        self.last_click = Some((now, x, y));
                                    }
                                } else {
                                    self.last_click = Some((now, x, y));
                                }
                                events.push(PlatformEvent::MouseDown {
                                    button: MouseButton::Left,
                                    x,
                                    y,
                                });
                            }
                            2 => {
                                events.push(PlatformEvent::MouseDown {
                                    button: MouseButton::Middle,
                                    x,
                                    y,
                                });
                            }
                            3 => {
                                events.push(PlatformEvent::MouseDown {
                                    button: MouseButton::Right,
                                    x,
                                    y,
                                });
                            }
                            4 => {
                                events.push(PlatformEvent::Scroll {
                                    delta_x: 0.0,
                                    delta_y: -1.0,
                                    is_momentum: false,
                                });
                            }
                            5 => {
                                events.push(PlatformEvent::Scroll {
                                    delta_x: 0.0,
                                    delta_y: 1.0,
                                    is_momentum: false,
                                });
                            }
                            6 => {
                                events.push(PlatformEvent::Scroll {
                                    delta_x: -1.0,
                                    delta_y: 0.0,
                                    is_momentum: false,
                                });
                            }
                            7 => {
                                events.push(PlatformEvent::Scroll {
                                    delta_x: 1.0,
                                    delta_y: 0.0,
                                    is_momentum: false,
                                });
                            }
                            b => {
                                events.push(PlatformEvent::MouseDown {
                                    button: MouseButton::Other(b as u16),
                                    x,
                                    y,
                                });
                            }
                        }
                    }
                    xlib::ButtonRelease => {
                        let bp = xevent.button;
                        let x = bp.x as f32;
                        let y = bp.y as f32;
                        match bp.button {
                            1 => events.push(PlatformEvent::MouseUp {
                                button: MouseButton::Left,
                                x,
                                y,
                            }),
                            2 => events.push(PlatformEvent::MouseUp {
                                button: MouseButton::Middle,
                                x,
                                y,
                            }),
                            3 => events.push(PlatformEvent::MouseUp {
                                button: MouseButton::Right,
                                x,
                                y,
                            }),
                            4..=7 => {}
                            b => events.push(PlatformEvent::MouseUp {
                                button: MouseButton::Other(b as u16),
                                x,
                                y,
                            }),
                        }
                    }
                    xlib::MotionNotify => {
                        let mn = xevent.motion;
                        events.push(PlatformEvent::MouseMove {
                            x: mn.x as f32,
                            y: mn.y as f32,
                        });
                    }
                    xlib::KeyPress => {
                        let mut kp = xevent.key;
                        let mut buf = [0u8; 32];
                        let mut keysym: xlib::KeySym = 0;
                        let count = xlib::XLookupString(
                            &mut kp,
                            buf.as_mut_ptr() as *mut c_char,
                            buf.len() as c_int,
                            &mut keysym,
                            std::ptr::null_mut(),
                        );
                        let code = map_x11_keysym(keysym);
                        events.push(PlatformEvent::KeyDown {
                            key: code,
                            modifiers: Modifiers::default(),
                        });
                        if count > 0 {
                            if let Ok(s) = std::str::from_utf8(&buf[..count as usize]) {
                                for ch in s.chars() {
                                    events.push(PlatformEvent::ReceivedCharacter(ch));
                                }
                            }
                        }
                    }
                    xlib::KeyRelease => {
                        let mut kp = xevent.key;
                        let mut buf = [0u8; 32];
                        let mut keysym: xlib::KeySym = 0;
                        let _ = xlib::XLookupString(
                            &mut kp,
                            buf.as_mut_ptr() as *mut c_char,
                            buf.len() as c_int,
                            &mut keysym,
                            std::ptr::null_mut(),
                        );
                        let code = map_x11_keysym(keysym);
                        events.push(PlatformEvent::KeyUp {
                            key: code,
                            modifiers: Modifiers::default(),
                        });
                    }
                    xlib::FocusIn => {
                        self.is_focused = true;
                        events.push(PlatformEvent::WindowFocused(true));
                    }
                    xlib::FocusOut => {
                        self.is_focused = false;
                        events.push(PlatformEvent::WindowFocused(false));
                    }
                    xlib::ClientMessage => {
                        let cm = xevent.client_message;
                        if cm.data.get_long(0) as c_ulong == self.wm_delete_window {
                            events.push(PlatformEvent::WindowCloseRequested);
                        }
                    }
                    _ => {}
                }
            }
        }
        events
    }
}

impl Drop for X11Window {
    fn drop(&mut self) {
        unsafe {
            if !self.display.is_null() {
                if !self.gc.is_null() {
                    xlib::XFreeGC(self.display, self.gc);
                }
                if self.window != 0 {
                    xlib::XDestroyWindow(self.display, self.window);
                }
                xlib::XCloseDisplay(self.display);
            }
        }
    }
}

impl PlatformWindow for X11Window {
    fn title(&self) -> &str {
        &self.config.title
    }

    fn set_title(&mut self, title: &str) {
        self.config.title = title.to_string();
        if !self.display.is_null() && self.window != 0 {
            if let Ok(c_title) = CString::new(title) {
                unsafe {
                    xlib::XStoreName(self.display, self.window, c_title.as_ptr());
                    xlib::XFlush(self.display);
                }
            }
        }
    }

    fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    fn set_size(&mut self, width: u32, height: u32) {
        self.config.width = width;
        self.config.height = height;
        if !self.display.is_null() && self.window != 0 {
            unsafe {
                xlib::XResizeWindow(self.display, self.window, width, height);
                xlib::XFlush(self.display);
            }
        }
        self.request_redraw();
    }

    fn scale_factor(&self) -> f32 {
        self.scale_factor
    }

    fn set_scale_factor(&mut self, scale_factor: f32) {
        self.scale_factor = scale_factor;
        self.request_redraw();
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
        let mut events = self.poll_native_events();
        while let Some(ev) = self.event_queue.pop_front() {
            events.push(ev);
        }
        events
    }

    fn push_event(&mut self, event: PlatformEvent) {
        self.event_queue.push_back(event);
    }
}

/// Native Linux X11 Event Loop running against a live X11 window.
pub struct X11EventLoop {
    pub window: X11Window,
}

impl X11EventLoop {
    pub fn new(window: X11Window) -> Self {
        Self { window }
    }
}

impl EventLoop for X11EventLoop {
    fn run<F>(&mut self, mut handler: F)
    where
        F: FnMut(PlatformEvent, &mut EventLoopControl),
    {
        let mut control = EventLoopControl::Poll;

        while control != EventLoopControl::Exit {
            let events = self.window.poll_events();
            for event in events {
                handler(event, &mut control);
                if control == EventLoopControl::Exit {
                    return;
                }
            }

            match control {
                EventLoopControl::Poll => std::thread::yield_now(),
                EventLoopControl::Wait => std::thread::sleep(Duration::from_millis(150)),
                EventLoopControl::WaitUntil(instant) => {
                    let now = std::time::Instant::now();
                    if let Some(dur) = instant.checked_duration_since(now) {
                        std::thread::sleep(dur.min(Duration::from_millis(150)));
                    }
                }
                EventLoopControl::Exit => break,
            }
        }
    }
}

fn map_x11_keysym(keysym: xlib::KeySym) -> KeyCode {
    match keysym {
        0xFF08 => KeyCode::Backspace,
        0xFF09 => KeyCode::Tab,
        0xFF0D => KeyCode::Enter,
        0xFF1B => KeyCode::Escape,
        0xFF51 => KeyCode::ArrowLeft,
        0xFF52 => KeyCode::ArrowUp,
        0xFF53 => KeyCode::ArrowRight,
        0xFF54 => KeyCode::ArrowDown,
        0xFFFF => KeyCode::Delete,
        0x0020 => KeyCode::Space,
        0x0030..=0x0039 => match keysym - 0x0030 {
            0 => KeyCode::Digit0,
            1 => KeyCode::Digit1,
            2 => KeyCode::Digit2,
            3 => KeyCode::Digit3,
            4 => KeyCode::Digit4,
            5 => KeyCode::Digit5,
            6 => KeyCode::Digit6,
            7 => KeyCode::Digit7,
            8 => KeyCode::Digit8,
            _ => KeyCode::Digit9,
        },
        0x0041..=0x005A | 0x0061..=0x007A => {
            let c = if keysym >= 0x0061 {
                keysym - 0x0061
            } else {
                keysym - 0x0041
            };
            match c {
                0 => KeyCode::KeyA,
                1 => KeyCode::KeyB,
                2 => KeyCode::KeyC,
                3 => KeyCode::KeyD,
                4 => KeyCode::KeyE,
                5 => KeyCode::KeyF,
                6 => KeyCode::KeyG,
                7 => KeyCode::KeyH,
                8 => KeyCode::KeyI,
                9 => KeyCode::KeyJ,
                10 => KeyCode::KeyK,
                11 => KeyCode::KeyL,
                12 => KeyCode::KeyM,
                13 => KeyCode::KeyN,
                14 => KeyCode::KeyO,
                15 => KeyCode::KeyP,
                16 => KeyCode::KeyQ,
                17 => KeyCode::KeyR,
                18 => KeyCode::KeyS,
                19 => KeyCode::KeyT,
                20 => KeyCode::KeyU,
                21 => KeyCode::KeyV,
                22 => KeyCode::KeyW,
                23 => KeyCode::KeyX,
                24 => KeyCode::KeyY,
                _ => KeyCode::KeyZ,
            }
        }
        _ => KeyCode::Unknown,
    }
}

