//! Native Android NDK NativeActivity Windowing and Event Loop.
//!
//! Provides a zero-dependency Android NDK NativeActivity implementation backed by
//! `ANativeWindow`, `android_app` lifecycle state machine (`APP_CMD_INIT_WINDOW`,
//! `APP_CMD_TERM_WINDOW`), touch motion event translation (`AMotionEvent`),
//! and Vulkan / software surface swapchain presentation.

#![cfg(target_os = "android")]

use crate::events::{KeyCode, Modifiers, MouseButton, PlatformEvent};
use crate::window::{
    CursorIcon, EventLoop, EventLoopControl, PlatformWindow, WindowConfig, WindowMode,
};
use std::collections::VecDeque;
use std::ffi::c_void;

// Android NDK lifecycle command constants
pub const APP_CMD_INPUT_CHANGED: i32 = 0;
pub const APP_CMD_INIT_WINDOW: i32 = 1;
pub const APP_CMD_TERM_WINDOW: i32 = 2;
pub const APP_CMD_WINDOW_RESIZED: i32 = 3;
pub const APP_CMD_WINDOW_REDRAW_NEEDED: i32 = 4;
pub const APP_CMD_CONTENT_RECT_CHANGED: i32 = 5;
pub const APP_CMD_GAINED_FOCUS: i32 = 6;
pub const APP_CMD_LOST_FOCUS: i32 = 7;
pub const APP_CMD_CONFIG_CHANGED: i32 = 8;
pub const APP_CMD_LOW_MEMORY: i32 = 9;
pub const APP_CMD_START: i32 = 10;
pub const APP_CMD_RESUME: i32 = 11;
pub const APP_CMD_SAVE_STATE: i32 = 12;
pub const APP_CMD_PAUSE: i32 = 13;
pub const APP_CMD_STOP: i32 = 14;
pub const APP_CMD_DESTROY: i32 = 15;

// Android MotionEvent actions
pub const AMOTION_EVENT_ACTION_MASK: i32 = 0xff;
pub const AMOTION_EVENT_ACTION_DOWN: i32 = 0;
pub const AMOTION_EVENT_ACTION_UP: i32 = 1;
pub const AMOTION_EVENT_ACTION_MOVE: i32 = 2;
pub const AMOTION_EVENT_ACTION_CANCEL: i32 = 3;
pub const AMOTION_EVENT_ACTION_POINTER_DOWN: i32 = 5;
pub const AMOTION_EVENT_ACTION_POINTER_UP: i32 = 6;

// Android KeyEvent actions
pub const AKEY_EVENT_ACTION_DOWN: i32 = 0;
pub const AKEY_EVENT_ACTION_UP: i32 = 1;

// Android KeyCodes
pub const AKEYCODE_BACK: i32 = 4;
pub const AKEYCODE_VOLUME_UP: i32 = 24;
pub const AKEYCODE_VOLUME_DOWN: i32 = 25;
pub const AKEYCODE_POWER: i32 = 26;

/// Raw Android NDK `ANativeWindow_Buffer` structure.
#[repr(C)]
pub struct ANativeWindowBuffer {
    pub width: i32,
    pub height: i32,
    pub stride: i32,
    pub format: i32,
    pub bits: *mut c_void,
    pub reserved: [u32; 6],
}

/// Raw Android NDK `ARect` structure.
#[repr(C)]
pub struct ARect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

extern "C" {
    fn ANativeWindow_getWidth(window: *mut c_void) -> i32;
    fn ANativeWindow_getHeight(window: *mut c_void) -> i32;
    fn ANativeWindow_setBuffersGeometry(
        window: *mut c_void,
        width: i32,
        height: i32,
        format: i32,
    ) -> i32;
    fn ANativeWindow_lock(
        window: *mut c_void,
        out_buffer: *mut ANativeWindowBuffer,
        in_out_dirty_bounds: *mut ARect,
    ) -> i32;
    fn ANativeWindow_unlockAndPost(window: *mut c_void) -> i32;
    fn ANativeWindow_release(window: *mut c_void);
}

/// Native Android application window holding active `ANativeWindow` handle and lifecycle state.
pub struct AndroidWindow {
    native_window: *mut c_void,
    config: WindowConfig,
    scale_factor: f32,
    is_focused: bool,
    cursor: CursorIcon,
    redraw_requested: bool,
    event_queue: VecDeque<PlatformEvent>,
}

impl AndroidWindow {
    /// Creates a new Android platform window instance wrapping the given `ANativeWindow`.
    pub fn new(native_window: *mut c_void, config: WindowConfig) -> Result<Self, String> {
        let (width, height) = if !native_window.is_null() {
            unsafe {
                let w = ANativeWindow_getWidth(native_window);
                let h = ANativeWindow_getHeight(native_window);
                (w.max(1) as u32, h.max(1) as u32)
            }
        } else {
            (config.width, config.height)
        };

        let mut actual_config = config;
        actual_config.width = width;
        actual_config.height = height;

        Ok(Self {
            native_window,
            config: actual_config,
            scale_factor: 2.625, // Standard Android xxhdpi default
            is_focused: true,
            cursor: CursorIcon::Default,
            redraw_requested: true,
            event_queue: VecDeque::new(),
        })
    }

    /// Returns the raw `ANativeWindow` pointer for EGL / Vulkan surface creation.
    pub fn raw_window(&self) -> *mut c_void {
        self.native_window
    }

    /// Updates the active `ANativeWindow` pointer on `APP_CMD_INIT_WINDOW` or `APP_CMD_TERM_WINDOW`.
    pub fn set_native_window(&mut self, window: *mut c_void) {
        self.native_window = window;
        if !window.is_null() {
            unsafe {
                let w = ANativeWindow_getWidth(window) as u32;
                let h = ANativeWindow_getHeight(window) as u32;
                if w > 0 && h > 0 {
                    self.config.width = w;
                    self.config.height = h;
                    self.event_queue
                        .push_back(PlatformEvent::WindowResized { width: w, height: h });
                }
            }
        }
    }

    /// Presents a 32-bit RGBA/BGRA software buffer directly to the `ANativeWindow`.
    pub fn present_bgra(&self, pixels: &[u8], width: u32, height: u32) -> Result<(), String> {
        if self.native_window.is_null() || pixels.is_empty() {
            return Ok(());
        }

        unsafe {
            // WINDOW_FORMAT_RGBA_8888 = 1
            ANativeWindow_setBuffersGeometry(self.native_window, width as i32, height as i32, 1);

            let mut buffer: ANativeWindowBuffer = std::mem::zeroed();
            let res = ANativeWindow_lock(self.native_window, &mut buffer, std::ptr::null_mut());
            if res != 0 {
                return Err(format!("ANativeWindow_lock failed with code {}", res));
            }

            if !buffer.bits.is_null() && buffer.width > 0 && buffer.height > 0 {
                let dst_stride = buffer.stride as usize * 4;
                let src_stride = width as usize * 4;
                let copy_w = src_stride.min(dst_stride);
                let copy_h = (height as usize).min(buffer.height as usize);

                let dst_ptr = buffer.bits as *mut u8;
                for row in 0..copy_h {
                    let src_row = &pixels[row * src_stride..row * src_stride + copy_w];
                    let dst_row = dst_ptr.add(row * dst_stride);
                    std::ptr::copy_nonoverlapping(src_row.as_ptr(), dst_row, copy_w);
                }
            }

            ANativeWindow_unlockAndPost(self.native_window);
        }

        Ok(())
    }

    /// Dispatches an Android lifecycle command (`APP_CMD_*`) into the window event queue.
    pub fn handle_app_cmd(&mut self, cmd: i32) {
        match cmd {
            APP_CMD_GAINED_FOCUS => {
                self.is_focused = true;
                self.event_queue.push_back(PlatformEvent::WindowFocused(true));
            }
            APP_CMD_LOST_FOCUS => {
                self.is_focused = false;
                self.event_queue.push_back(PlatformEvent::WindowFocused(false));
            }
            APP_CMD_WINDOW_REDRAW_NEEDED => {
                self.redraw_requested = true;
                self.event_queue.push_back(PlatformEvent::RedrawRequested);
            }
            APP_CMD_WINDOW_RESIZED | APP_CMD_CONFIG_CHANGED => {
                if !self.native_window.is_null() {
                    unsafe {
                        let w = ANativeWindow_getWidth(self.native_window) as u32;
                        let h = ANativeWindow_getHeight(self.native_window) as u32;
                        if w > 0 && h > 0 {
                            self.config.width = w;
                            self.config.height = h;
                            self.event_queue
                                .push_back(PlatformEvent::WindowResized { width: w, height: h });
                        }
                    }
                }
            }
            APP_CMD_DESTROY => {
                self.event_queue.push_back(PlatformEvent::WindowCloseRequested);
            }
            _ => {}
        }
    }

    /// Dispatches an Android motion touch event into the window event queue.
    pub fn handle_touch(&mut self, action: i32, x: f32, y: f32) {
        let action_masked = action & AMOTION_EVENT_ACTION_MASK;
        match action_masked {
            AMOTION_EVENT_ACTION_DOWN | AMOTION_EVENT_ACTION_POINTER_DOWN => {
                self.event_queue.push_back(PlatformEvent::MouseDown {
                    x,
                    y,
                    button: MouseButton::Left,
                });
            }
            AMOTION_EVENT_ACTION_MOVE => {
                self.event_queue.push_back(PlatformEvent::MouseMove { x, y });
            }
            AMOTION_EVENT_ACTION_UP | AMOTION_EVENT_ACTION_POINTER_UP => {
                self.event_queue.push_back(PlatformEvent::MouseUp {
                    x,
                    y,
                    button: MouseButton::Left,
                });
            }
            AMOTION_EVENT_ACTION_CANCEL => {
                self.event_queue.push_back(PlatformEvent::CursorLeft);
            }
            _ => {}
        }
    }

    /// Dispatches an Android key event into the window event queue.
    pub fn handle_key(&mut self, action: i32, keycode: i32) {
        if keycode == AKEYCODE_BACK {
            if action == AKEY_EVENT_ACTION_DOWN {
                self.event_queue.push_back(PlatformEvent::KeyDown {
                    key: KeyCode::Escape,
                    modifiers: Modifiers::default(),
                });
            } else if action == AKEY_EVENT_ACTION_UP {
                self.event_queue.push_back(PlatformEvent::KeyUp {
                    key: KeyCode::Escape,
                    modifiers: Modifiers::default(),
                });
            }
        }
    }
}

impl Drop for AndroidWindow {
    fn drop(&mut self) {
        if !self.native_window.is_null() {
            unsafe {
                ANativeWindow_release(self.native_window);
            }
        }
    }
}

impl PlatformWindow for AndroidWindow {
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

/// Native Android NDK event loop driving event dispatch and frame pacing.
pub struct AndroidEventLoop {
    pub window: AndroidWindow,
    pub control: EventLoopControl,
}

impl AndroidEventLoop {
    /// Creates a new event loop driving the given Android window.
    pub fn new(window: AndroidWindow) -> Self {
        Self {
            window,
            control: EventLoopControl::Poll,
        }
    }
}

impl EventLoop for AndroidEventLoop {
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
