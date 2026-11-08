//! Native Windows Win32 windowing backend and hardware event loop.
//!
//! Provides a zero-dependency Win32 window implementation backed by
//! `CreateWindowExW`, standard Windows message pump (`PeekMessageW`, `DispatchMessageW`),
//! high-DPI scaling (`WM_DPICHANGED`), raw keyboard/mouse translation, and
//! hardware surface lifecycle management.

#![cfg(windows)]

use crate::events::{KeyCode, Modifiers, MouseButton, PlatformEvent};
use crate::vibrancy::{apply_vibrancy, VibrancyConfig};
use crate::window::{
    CursorIcon, EventLoop, EventLoopControl, PlatformWindow, WindowConfig, WindowDecoration,
    WindowMode,
};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{BeginPaint, EndPaint, InvalidateRect, PAINTSTRUCT};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, SetFocus, VK_BACK, VK_CONTROL, VK_DELETE, VK_DOWN, VK_ESCAPE, VK_LEFT, VK_MENU,
    VK_RETURN, VK_RIGHT, VK_SHIFT, VK_SPACE, VK_TAB, VK_UP,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRectEx, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    LoadCursorW, MsgWaitForMultipleObjectsEx, PeekMessageW, PostQuitMessage,
    RegisterClassExW, SetCursor, SetWindowPos, SetWindowTextW, ShowWindow, TranslateMessage,
    WaitMessage, CS_DBLCLKS, CS_HREDRAW, CS_VREDRAW, IDC_ARROW, IDC_CROSS, IDC_HAND, IDC_HELP,
    IDC_IBEAM, IDC_NO, IDC_SIZEALL, IDC_SIZENESW, IDC_SIZENS, IDC_SIZENWSE, IDC_SIZEWE, IDC_WAIT,
    MWMO_INPUTAVAILABLE, PM_REMOVE, QS_ALLINPUT, SWP_NOMOVE, SWP_NOZORDER, SW_SHOW,
    WM_CHAR, WM_CLOSE, WM_DESTROY, WM_DPICHANGED, WM_KEYDOWN, WM_KEYUP, WM_KILLFOCUS,
    WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEHWHEEL,
    WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_PAINT, WM_QUIT, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SETFOCUS,
    WM_SIZE, WNDCLASSEXW, WS_EX_ACCEPTFILES, WS_EX_APPWINDOW, WS_EX_NOREDIRECTIONBITMAP,
    WS_OVERLAPPEDWINDOW, WS_POPUP, WS_VISIBLE,
};

/// Internal mutable window state shared with the static window procedure.
pub struct Win32WindowState {
    pub config: WindowConfig,
    pub scale_factor: f32,
    pub is_focused: bool,
    pub cursor: CursorIcon,
    pub redraw_requested: bool,
    pub event_queue: VecDeque<PlatformEvent>,
}

static WINDOW_REGISTRY: OnceLock<Mutex<HashMap<isize, Arc<Mutex<Win32WindowState>>>>> =
    OnceLock::new();

fn get_registry() -> &'static Mutex<HashMap<isize, Arc<Mutex<Win32WindowState>>>> {
    WINDOW_REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Native Win32 window implementation.
pub struct Win32Window {
    hwnd: HWND,
    state: Arc<Mutex<Win32WindowState>>,
}

impl Win32Window {
    /// Creates a new native Win32 window backed by `CreateWindowExW`.
    pub fn new(config: WindowConfig) -> Result<Self, String> {
        unsafe {
            let instance = GetModuleHandleW(std::ptr::null());
            let class_name: Vec<u16> = "StrataEngineWindowClass\0".encode_utf16().collect();

            static REGISTER_ONCE: OnceLock<()> = OnceLock::new();
            REGISTER_ONCE.get_or_init(|| {
                let wnd_class = WNDCLASSEXW {
                    cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                    style: CS_HREDRAW | CS_VREDRAW | CS_DBLCLKS,
                    lpfnWndProc: Some(win32_wnd_proc),
                    cbClsExtra: 0,
                    cbWndExtra: 0,
                    hInstance: instance,
                    hIcon: 0,
                    hCursor: LoadCursorW(0, IDC_ARROW),
                    hbrBackground: 0,
                    lpszMenuName: std::ptr::null(),
                    lpszClassName: class_name.as_ptr(),
                    hIconSm: 0,
                };
                RegisterClassExW(&wnd_class);
            });

            let style = match config.decoration {
                WindowDecoration::NativeOs => WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                WindowDecoration::ClientSideDecorations | WindowDecoration::FramelessTransparent => {
                    WS_POPUP | WS_VISIBLE
                }
            };

            let ex_style = if config.transparent {
                WS_EX_APPWINDOW | WS_EX_NOREDIRECTIONBITMAP | WS_EX_ACCEPTFILES
            } else {
                WS_EX_APPWINDOW | WS_EX_ACCEPTFILES
            };

            let mut rect = RECT {
                left: 0,
                top: 0,
                right: config.width as i32,
                bottom: config.height as i32,
            };
            AdjustWindowRectEx(&mut rect, style, 0, ex_style);
            let window_w = rect.right - rect.left;
            let window_h = rect.bottom - rect.top;

            let wide_title: Vec<u16> = config.title.encode_utf16().chain(std::iter::once(0)).collect();

            let hwnd = CreateWindowExW(
                ex_style,
                class_name.as_ptr(),
                wide_title.as_ptr(),
                style,
                0x80000000u32 as i32, // CW_USEDEFAULT
                0x80000000u32 as i32, // CW_USEDEFAULT
                window_w,
                window_h,
                0 as HWND,
                0,
                instance,
                std::ptr::null(),
            );

            if hwnd == 0 {
                return Err("CreateWindowExW failed to create Win32 window".to_string());
            }

            let hdc = windows_sys::Win32::Graphics::Gdi::GetDC(hwnd);
            let dpi = if hdc != 0 {
                let caps = windows_sys::Win32::Graphics::Gdi::GetDeviceCaps(
                    hdc,
                    windows_sys::Win32::Graphics::Gdi::LOGPIXELSY as i32,
                );
                windows_sys::Win32::Graphics::Gdi::ReleaseDC(hwnd, hdc);
                caps
            } else {
                96
            };
            let scale_factor = if dpi > 0 {
                dpi as f32 / 96.0
            } else {
                1.0
            };

            let state = Arc::new(Mutex::new(Win32WindowState {
                config: config.clone(),
                scale_factor,
                is_focused: true,
                cursor: CursorIcon::Default,
                redraw_requested: true,
                event_queue: VecDeque::new(),
            }));

            // Register window in thread-safe registry for static wnd_proc routing
            get_registry().lock().unwrap().insert(hwnd as isize, state.clone());

            // Apply backdrop vibrancy if configured
            if config.vibrancy != crate::vibrancy::VibrancyEffect::None {
                let _ = apply_vibrancy(hwnd as isize, VibrancyConfig::new(config.vibrancy));
            }

            ShowWindow(hwnd, SW_SHOW);

            Ok(Self { hwnd, state })
        }
    }

    /// Returns the raw Win32 `HWND` handle.
    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }

    /// Presents a 32-bit BGRA buffer directly to the window client area via GDI.
    pub fn present_bgra(&self, pixels: &[u8], width: u32, height: u32) -> Result<(), String> {
        if pixels.is_empty() || width == 0 || height == 0 {
            return Ok(());
        }
        unsafe {
            let hdc = windows_sys::Win32::Graphics::Gdi::GetDC(self.hwnd);
            if hdc == 0 {
                return Err("Failed to acquire window device context (GetDC returned 0)".to_string());
            }

            let bmi = windows_sys::Win32::Graphics::Gdi::BITMAPINFO {
                bmiHeader: windows_sys::Win32::Graphics::Gdi::BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<windows_sys::Win32::Graphics::Gdi::BITMAPINFOHEADER>() as u32,
                    biWidth: width as i32,
                    biHeight: -(height as i32), // top-down DIB
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: windows_sys::Win32::Graphics::Gdi::BI_RGB,
                    biSizeImage: (width * height * 4) as u32,
                    biXPelsPerMeter: 0,
                    biYPelsPerMeter: 0,
                    biClrUsed: 0,
                    biClrImportant: 0,
                },
                bmiColors: [windows_sys::Win32::Graphics::Gdi::RGBQUAD { rgbBlue: 0, rgbGreen: 0, rgbRed: 0, rgbReserved: 0 }],
            };

            windows_sys::Win32::Graphics::Gdi::SetDIBitsToDevice(
                hdc,
                0,
                0,
                width,
                height,
                0,
                0,
                0,
                height,
                pixels.as_ptr() as *const _,
                &bmi,
                windows_sys::Win32::Graphics::Gdi::DIB_RGB_COLORS,
            );

            windows_sys::Win32::Graphics::Gdi::ReleaseDC(self.hwnd, hdc);
        }
        Ok(())
    }

    /// Pumps pending Win32 messages and returns any translated platform events.
    pub fn poll_native_events(&mut self) -> Vec<PlatformEvent> {
        let mut msg = unsafe { std::mem::zeroed() };
        unsafe {
            while PeekMessageW(&mut msg, self.hwnd, 0, 0, PM_REMOVE) != 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        self.poll_events()
    }
}

impl Drop for Win32Window {
    fn drop(&mut self) {
        get_registry().lock().unwrap().remove(&(self.hwnd as isize));
        unsafe {
            DestroyWindow(self.hwnd);
        }
    }
}

impl PlatformWindow for Win32Window {
    fn title(&self) -> &str {
        let lock = self.state.lock().unwrap();
        // Return title clone or reference from config
        // Because of signature &str, we use config title
        unsafe {
            let ptr = &lock.config.title as *const String;
            &*ptr
        }
    }

    fn set_title(&mut self, title: &str) {
        let mut lock = self.state.lock().unwrap();
        lock.config.title = title.to_string();
        let wide: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe {
            SetWindowTextW(self.hwnd, wide.as_ptr());
        }
    }

    fn size(&self) -> (u32, u32) {
        let lock = self.state.lock().unwrap();
        (lock.config.width, lock.config.height)
    }

    fn set_size(&mut self, width: u32, height: u32) {
        {
            let mut lock = self.state.lock().unwrap();
            lock.config.width = width;
            lock.config.height = height;
            lock.event_queue
                .push_back(PlatformEvent::WindowResized { width, height });
        }
        unsafe {
            SetWindowPos(
                self.hwnd,
                0 as HWND,
                0,
                0,
                width as i32,
                height as i32,
                SWP_NOMOVE | SWP_NOZORDER,
            );
        }
    }

    fn scale_factor(&self) -> f32 {
        self.state.lock().unwrap().scale_factor
    }

    fn set_scale_factor(&mut self, scale_factor: f32) {
        let mut lock = self.state.lock().unwrap();
        lock.scale_factor = scale_factor;
        lock.event_queue
            .push_back(PlatformEvent::ScaleFactorChanged { scale_factor });
    }

    fn mode(&self) -> WindowMode {
        self.state.lock().unwrap().config.mode
    }

    fn set_mode(&mut self, mode: WindowMode) {
        self.state.lock().unwrap().config.mode = mode;
    }

    fn is_focused(&self) -> bool {
        self.state.lock().unwrap().is_focused
    }

    fn set_focused(&mut self, focused: bool) {
        if focused {
            unsafe {
                SetFocus(self.hwnd);
            }
        }
    }

    fn cursor(&self) -> CursorIcon {
        self.state.lock().unwrap().cursor
    }

    fn set_cursor(&mut self, cursor: CursorIcon) {
        self.state.lock().unwrap().cursor = cursor;
        unsafe {
            let win32_cursor = match cursor {
                CursorIcon::Default => LoadCursorW(0, IDC_ARROW),
                CursorIcon::Pointer => LoadCursorW(0, IDC_HAND),
                CursorIcon::Text => LoadCursorW(0, IDC_IBEAM),
                CursorIcon::Crosshair => LoadCursorW(0, IDC_CROSS),
                CursorIcon::Move => LoadCursorW(0, IDC_SIZEALL),
                CursorIcon::NotAllowed => LoadCursorW(0, IDC_NO),
                CursorIcon::Wait => LoadCursorW(0, IDC_WAIT),
                CursorIcon::ResizeN | CursorIcon::ResizeS => LoadCursorW(0, IDC_SIZENS),
                CursorIcon::ResizeE | CursorIcon::ResizeW => LoadCursorW(0, IDC_SIZEWE),
                CursorIcon::ResizeNe | CursorIcon::ResizeSw => LoadCursorW(0, IDC_SIZENESW),
                CursorIcon::ResizeNw | CursorIcon::ResizeSe => LoadCursorW(0, IDC_SIZENWSE),
                CursorIcon::Help => LoadCursorW(0, IDC_HELP),
                _ => LoadCursorW(0, IDC_ARROW),
            };
            SetCursor(win32_cursor);
        }
    }

    fn request_redraw(&mut self) {
        self.state.lock().unwrap().redraw_requested = true;
        unsafe {
            InvalidateRect(self.hwnd, std::ptr::null(), 0);
        }
    }

    fn is_redraw_requested(&self) -> bool {
        self.state.lock().unwrap().redraw_requested
    }

    fn poll_events(&mut self) -> Vec<PlatformEvent> {
        let mut lock = self.state.lock().unwrap();
        lock.event_queue.drain(..).collect()
    }

    fn push_event(&mut self, event: PlatformEvent) {
        self.state.lock().unwrap().event_queue.push_back(event);
    }
}

/// Win32 native event loop processing OS messages and driving Strata rendering.
pub struct Win32EventLoop {
    pub window: Win32Window,
    pub control: EventLoopControl,
}

impl Win32EventLoop {
    /// Creates a new Win32 event loop driving the given window.
    pub fn new(window: Win32Window) -> Self {
        Self {
            window,
            control: EventLoopControl::Poll,
        }
    }
}

impl EventLoop for Win32EventLoop {
    fn run<F>(&mut self, mut handler: F)
    where
        F: FnMut(PlatformEvent, &mut EventLoopControl),
    {
        let mut msg = unsafe { std::mem::zeroed() };
        while self.control != EventLoopControl::Exit {
            // 1. Process Win32 OS messages non-blockingly
            while unsafe { PeekMessageW(&mut msg, 0 as HWND, 0, 0, PM_REMOVE) } != 0 {
                if msg.message == WM_QUIT {
                    self.control = EventLoopControl::Exit;
                    break;
                }
                unsafe {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }

            if self.control == EventLoopControl::Exit {
                break;
            }

            // 2. Poll platform events generated by window procedure
            let events = self.window.poll_events();
            for event in events {
                handler(event, &mut self.control);
                if self.control == EventLoopControl::Exit {
                    break;
                }
            }

            // 3. Dispatch redraw frame if requested
            if self.window.is_redraw_requested() {
                self.window.state.lock().unwrap().redraw_requested = false;
                handler(
                    PlatformEvent::RedrawRequested,
                    &mut self.control,
                );
            }

            // 4. Control flow and frame pacing
            match self.control {
                EventLoopControl::Poll => {
                    std::thread::yield_now();
                }
                EventLoopControl::Wait => {
                    unsafe {
                        WaitMessage();
                    }
                }
                EventLoopControl::WaitUntil(instant) => {
                    let now = Instant::now();
                    if let Some(dur) = instant.checked_duration_since(now) {
                        let ms = dur.as_millis().min(u32::MAX as u128) as u32;
                        unsafe {
                            MsgWaitForMultipleObjectsEx(
                                0,
                                std::ptr::null(),
                                ms,
                                QS_ALLINPUT,
                                MWMO_INPUTAVAILABLE,
                            );
                        }
                    }
                }
                EventLoopControl::Exit => break,
            }
        }
    }
}

unsafe extern "system" fn win32_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let state_arc = {
        let registry = get_registry().lock().unwrap();
        registry.get(&(hwnd as isize)).cloned()
    };

    if let Some(state) = state_arc {
        match msg {
            WM_PAINT => {
                let mut ps: PAINTSTRUCT = std::mem::zeroed();
                let _hdc = BeginPaint(hwnd, &mut ps);
                state.lock().unwrap().redraw_requested = true;
                EndPaint(hwnd, &ps);
                return 0;
            }
            WM_SIZE => {
                let width = (lparam & 0xFFFF) as u32;
                let height = ((lparam >> 16) & 0xFFFF) as u32;
                let mut lock = state.lock().unwrap();
                lock.config.width = width;
                lock.config.height = height;
                lock.redraw_requested = true;
                lock.event_queue
                    .push_back(PlatformEvent::WindowResized { width, height });
                return 0;
            }
            WM_DPICHANGED => {
                let dpi_y = ((wparam >> 16) & 0xFFFF) as u32;
                let scale_factor = dpi_y as f32 / 96.0;
                let mut lock = state.lock().unwrap();
                lock.scale_factor = scale_factor;
                lock.redraw_requested = true;
                lock.event_queue
                    .push_back(PlatformEvent::ScaleFactorChanged { scale_factor });
                return 0;
            }
            WM_SETFOCUS => {
                let mut lock = state.lock().unwrap();
                lock.is_focused = true;
                lock.event_queue
                    .push_back(PlatformEvent::WindowFocused(true));
                return 0;
            }
            WM_KILLFOCUS => {
                let mut lock = state.lock().unwrap();
                lock.is_focused = false;
                lock.event_queue
                    .push_back(PlatformEvent::WindowFocused(false));
                return 0;
            }
            WM_MOUSEMOVE => {
                let x = (lparam & 0xFFFF) as i16 as f32;
                let y = ((lparam >> 16) & 0xFFFF) as i16 as f32;
                state
                    .lock()
                    .unwrap()
                    .event_queue
                    .push_back(PlatformEvent::MouseMove { x, y });
                return 0;
            }
            WM_LBUTTONDBLCLK => {
                let x = (lparam & 0xFFFF) as i16 as f32;
                let y = ((lparam >> 16) & 0xFFFF) as i16 as f32;
                state
                    .lock()
                    .unwrap()
                    .event_queue
                    .push_back(PlatformEvent::DoubleClick {
                        button: MouseButton::Left,
                        x,
                        y,
                    });
                return 0;
            }
            WM_LBUTTONDOWN => {
                let x = (lparam & 0xFFFF) as i16 as f32;
                let y = ((lparam >> 16) & 0xFFFF) as i16 as f32;
                state.lock().unwrap().event_queue.push_back(PlatformEvent::MouseDown {
                    button: MouseButton::Left,
                    x,
                    y,
                });
                return 0;
            }
            WM_LBUTTONUP => {
                let x = (lparam & 0xFFFF) as i16 as f32;
                let y = ((lparam >> 16) & 0xFFFF) as i16 as f32;
                state.lock().unwrap().event_queue.push_back(PlatformEvent::MouseUp {
                    button: MouseButton::Left,
                    x,
                    y,
                });
                return 0;
            }
            WM_RBUTTONDOWN => {
                let x = (lparam & 0xFFFF) as i16 as f32;
                let y = ((lparam >> 16) & 0xFFFF) as i16 as f32;
                state.lock().unwrap().event_queue.push_back(PlatformEvent::MouseDown {
                    button: MouseButton::Right,
                    x,
                    y,
                });
                return 0;
            }
            WM_RBUTTONUP => {
                let x = (lparam & 0xFFFF) as i16 as f32;
                let y = ((lparam >> 16) & 0xFFFF) as i16 as f32;
                state.lock().unwrap().event_queue.push_back(PlatformEvent::MouseUp {
                    button: MouseButton::Right,
                    x,
                    y,
                });
                return 0;
            }
            WM_MBUTTONDOWN => {
                let x = (lparam & 0xFFFF) as i16 as f32;
                let y = ((lparam >> 16) & 0xFFFF) as i16 as f32;
                state.lock().unwrap().event_queue.push_back(PlatformEvent::MouseDown {
                    button: MouseButton::Middle,
                    x,
                    y,
                });
                return 0;
            }
            WM_MBUTTONUP => {
                let x = (lparam & 0xFFFF) as i16 as f32;
                let y = ((lparam >> 16) & 0xFFFF) as i16 as f32;
                state.lock().unwrap().event_queue.push_back(PlatformEvent::MouseUp {
                    button: MouseButton::Middle,
                    x,
                    y,
                });
                return 0;
            }
            WM_MOUSEWHEEL => {
                let delta = ((wparam >> 16) & 0xFFFF) as i16 as f32 / 120.0;
                state
                    .lock()
                    .unwrap()
                    .event_queue
                    .push_back(PlatformEvent::Scroll {
                        delta_x: 0.0,
                        delta_y: -delta,
                        is_momentum: false,
                    });
                return 0;
            }
            WM_MOUSEHWHEEL => {
                let delta = ((wparam >> 16) & 0xFFFF) as i16 as f32 / 120.0;
                state
                    .lock()
                    .unwrap()
                    .event_queue
                    .push_back(PlatformEvent::Scroll {
                        delta_x: delta,
                        delta_y: 0.0,
                        is_momentum: false,
                    });
                return 0;
            }
            WM_KEYDOWN => {
                if let Some(key) = map_virtual_key(wparam as u32) {
                    let modifiers = get_current_modifiers();
                    state
                        .lock()
                        .unwrap()
                        .event_queue
                        .push_back(PlatformEvent::KeyDown { key, modifiers });
                }
                return 0;
            }
            WM_KEYUP => {
                if let Some(key) = map_virtual_key(wparam as u32) {
                    let modifiers = get_current_modifiers();
                    state
                        .lock()
                        .unwrap()
                        .event_queue
                        .push_back(PlatformEvent::KeyUp { key, modifiers });
                }
                return 0;
            }
            WM_CHAR => {
                if let Some(ch) = char::from_u32(wparam as u32) {
                    if !ch.is_control() {
                        state
                            .lock()
                            .unwrap()
                            .event_queue
                            .push_back(PlatformEvent::ReceivedCharacter(ch));
                    }
                }
                return 0;
            }
            WM_CLOSE => {
                state
                    .lock()
                    .unwrap()
                    .event_queue
                    .push_back(PlatformEvent::WindowCloseRequested);
                return 0;
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                return 0;
            }
            _ => {}
        }
    }

    DefWindowProcW(hwnd, msg, wparam, lparam)
}

unsafe fn get_current_modifiers() -> Modifiers {
    Modifiers {
        shift: (GetKeyState(VK_SHIFT as i32) as u16 & 0x8000) != 0,
        ctrl: (GetKeyState(VK_CONTROL as i32) as u16 & 0x8000) != 0,
        alt: (GetKeyState(VK_MENU as i32) as u16 & 0x8000) != 0,
        meta: false,
    }
}

fn map_virtual_key(vk: u32) -> Option<KeyCode> {
    match vk as u16 {
        VK_RETURN => Some(KeyCode::Enter),
        VK_ESCAPE => Some(KeyCode::Escape),
        VK_BACK => Some(KeyCode::Backspace),
        VK_TAB => Some(KeyCode::Tab),
        VK_SPACE => Some(KeyCode::Space),
        VK_LEFT => Some(KeyCode::ArrowLeft),
        VK_UP => Some(KeyCode::ArrowUp),
        VK_RIGHT => Some(KeyCode::ArrowRight),
        VK_DOWN => Some(KeyCode::ArrowDown),
        VK_DELETE => Some(KeyCode::Delete),
        0x30 => Some(KeyCode::Digit0),
        0x31 => Some(KeyCode::Digit1),
        0x32 => Some(KeyCode::Digit2),
        0x33 => Some(KeyCode::Digit3),
        0x34 => Some(KeyCode::Digit4),
        0x35 => Some(KeyCode::Digit5),
        0x36 => Some(KeyCode::Digit6),
        0x37 => Some(KeyCode::Digit7),
        0x38 => Some(KeyCode::Digit8),
        0x39 => Some(KeyCode::Digit9),
        0x41 => Some(KeyCode::KeyA),
        0x42 => Some(KeyCode::KeyB),
        0x43 => Some(KeyCode::KeyC),
        0x44 => Some(KeyCode::KeyD),
        0x45 => Some(KeyCode::KeyE),
        0x46 => Some(KeyCode::KeyF),
        0x47 => Some(KeyCode::KeyG),
        0x48 => Some(KeyCode::KeyH),
        0x49 => Some(KeyCode::KeyI),
        0x4A => Some(KeyCode::KeyJ),
        0x4B => Some(KeyCode::KeyK),
        0x4C => Some(KeyCode::KeyL),
        0x4D => Some(KeyCode::KeyM),
        0x4E => Some(KeyCode::KeyN),
        0x4F => Some(KeyCode::KeyO),
        0x50 => Some(KeyCode::KeyP),
        0x51 => Some(KeyCode::KeyQ),
        0x52 => Some(KeyCode::KeyR),
        0x53 => Some(KeyCode::KeyS),
        0x54 => Some(KeyCode::KeyT),
        0x55 => Some(KeyCode::KeyU),
        0x56 => Some(KeyCode::KeyV),
        0x57 => Some(KeyCode::KeyW),
        0x58 => Some(KeyCode::KeyX),
        0x59 => Some(KeyCode::KeyY),
        0x5A => Some(KeyCode::KeyZ),
        _ => None,
    }
}
