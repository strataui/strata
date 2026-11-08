//! Zero-dependency raw Native OS ABI protocols and descriptors for Windows, Wayland, and macOS.
//!
//! Provides pure-Rust structures, message constants, and wire protocol parsers
//! allowing Strata to communicate directly with OS display servers without heavy C library bindings.

// ============================================================================
// 1. Windows Win32 & DXGI 1.6 Flip Model Protocol
// ============================================================================

pub mod win32 {
    /// Opaque window handle.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct Hwnd(pub usize);

    /// Device context handle.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct Hdc(pub usize);

    /// Application instance handle.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct Hinstance(pub usize);

    /// Pointer-sized message result.
    pub type LResult = isize;

    // Standard Window Messages
    pub const WM_NULL: u32 = 0x0000;
    pub const WM_CREATE: u32 = 0x0001;
    pub const WM_DESTROY: u32 = 0x0002;
    pub const WM_MOVE: u32 = 0x0003;
    pub const WM_SIZE: u32 = 0x0005;
    pub const WM_ACTIVATE: u32 = 0x0006;
    pub const WM_SETFOCUS: u32 = 0x0007;
    pub const WM_KILLFOCUS: u32 = 0x0008;
    pub const WM_PAINT: u32 = 0x000F;
    pub const WM_CLOSE: u32 = 0x0010;
    pub const WM_QUIT: u32 = 0x0012;
    pub const WM_ERASEBKGND: u32 = 0x0014;
    pub const WM_KEYDOWN: u32 = 0x0100;
    pub const WM_KEYUP: u32 = 0x0101;
    pub const WM_CHAR: u32 = 0x0102;
    pub const WM_MOUSEMOVE: u32 = 0x0200;
    pub const WM_LBUTTONDOWN: u32 = 0x0201;
    pub const WM_LBUTTONUP: u32 = 0x0202;
    pub const WM_RBUTTONDOWN: u32 = 0x0204;
    pub const WM_RBUTTONUP: u32 = 0x0205;
    pub const WM_MBUTTONDOWN: u32 = 0x0207;
    pub const WM_MBUTTONUP: u32 = 0x0208;
    pub const WM_MOUSEWHEEL: u32 = 0x020A;
    pub const WM_DPICHANGED: u32 = 0x02E0;

    // DXGI 1.6 Swapchain Flags & Swap Effects
    pub const DXGI_SWAP_EFFECT_DISCARD: u32 = 0;
    pub const DXGI_SWAP_EFFECT_SEQUENTIAL: u32 = 1;
    pub const DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL: u32 = 3;
    pub const DXGI_SWAP_EFFECT_FLIP_DISCARD: u32 = 4;
    pub const DXGI_SWAP_CHAIN_FLAG_ALLOW_TEARING: u32 = 2048;

    /// 2D Point structure matching Win32 `POINT`.
    #[repr(C)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct Point {
        pub x: i32,
        pub y: i32,
    }

    /// Rectangle structure matching Win32 `RECT`.
    #[repr(C)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct Rect {
        pub left: i32,
        pub top: i32,
        pub right: i32,
        pub bottom: i32,
    }

    impl From<Point> for strata_primitives::geom::Point<f32> {
        fn from(p: Point) -> Self {
            strata_primitives::geom::Point::new(p.x as f32, p.y as f32)
        }
    }

    impl From<strata_primitives::geom::Point<f32>> for Point {
        fn from(p: strata_primitives::geom::Point<f32>) -> Self {
            Point {
                x: p.x as i32,
                y: p.y as i32,
            }
        }
    }

    impl From<Rect> for strata_primitives::geom::Rect<f32> {
        fn from(r: Rect) -> Self {
            strata_primitives::geom::Rect::from_xywh(
                r.left as f32,
                r.top as f32,
                (r.right - r.left).max(0) as f32,
                (r.bottom - r.top).max(0) as f32,
            )
        }
    }

    impl From<strata_primitives::geom::Rect<f32>> for Rect {
        fn from(r: strata_primitives::geom::Rect<f32>) -> Self {
            Rect {
                left: r.min_x() as i32,
                top: r.min_y() as i32,
                right: r.max_x() as i32,
                bottom: r.max_y() as i32,
            }
        }
    }

    /// Native message structure matching Win32 `MSG`.
    #[repr(C)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct Msg {
        pub hwnd: Hwnd,
        pub message: u32,
        pub wparam: usize,
        pub lparam: isize,
        pub time: u32,
        pub pt: Point,
    }

    /// Window class structure matching Win32 `WNDCLASSEXW`.
    #[repr(C)]
    pub struct WndClassExW {
        pub cb_size: u32,
        pub style: u32,
        pub lpfn_wnd_proc: usize,
        pub cb_cls_extra: i32,
        pub cb_wnd_extra: i32,
        pub h_instance: Hinstance,
        pub h_icon: usize,
        pub h_cursor: usize,
        pub hbr_background: usize,
        pub lpsz_menu_name: *const u16,
        pub lpsz_class_name: *const u16,
        pub h_icon_sm: usize,
    }
}

// ============================================================================
// 2. Linux Wayland Client Wire Protocol
// ============================================================================

pub mod wayland {
    /// Wayland socket path resolver.
    pub fn default_socket_path() -> String {
        if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
            let display =
                std::env::var("WAYLAND_DISPLAY").unwrap_or_else(|_| "wayland-0".to_string());
            format!("{}/{}", runtime_dir, display)
        } else {
            "/run/user/1000/wayland-0".to_string()
        }
    }

    /// 8-byte Wayland wire protocol message header: [object_id (u32), opcode (u16), size (u16)].
    #[repr(C)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct WaylandHeader {
        pub object_id: u32,
        pub opcode: u16,
        pub size: u16,
    }

    impl WaylandHeader {
        /// Encodes header into big-endian / native byte buffer.
        pub fn to_bytes(&self) -> [u8; 8] {
            let mut buf = [0u8; 8];
            buf[0..4].copy_from_slice(&self.object_id.to_ne_bytes());
            buf[4..6].copy_from_slice(&self.opcode.to_ne_bytes());
            buf[6..8].copy_from_slice(&self.size.to_ne_bytes());
            buf
        }

        /// Parses header from 8-byte wire slice.
        pub fn from_bytes(slice: &[u8]) -> Option<Self> {
            if slice.len() < 8 {
                return None;
            }
            let object_id = u32::from_ne_bytes(slice[0..4].try_into().ok()?);
            let opcode = u16::from_ne_bytes(slice[4..6].try_into().ok()?);
            let size = u16::from_ne_bytes(slice[6..8].try_into().ok()?);
            Some(Self {
                object_id,
                opcode,
                size,
            })
        }
    }

    // Protocol object IDs & standard opcodes
    pub const WL_DISPLAY_ID: u32 = 1;

    // wl_display opcodes
    pub const WL_DISPLAY_SYNC: u16 = 0;
    pub const WL_DISPLAY_GET_REGISTRY: u16 = 1;

    // wl_registry opcodes
    pub const WL_REGISTRY_BIND: u16 = 0;

    // wl_compositor opcodes
    pub const WL_COMPOSITOR_CREATE_SURFACE: u16 = 0;
    pub const WL_COMPOSITOR_CREATE_REGION: u16 = 1;

    // xdg_wm_base opcodes
    pub const XDG_WM_BASE_DESTROY: u16 = 0;
    pub const XDG_WM_BASE_GET_XDG_SURFACE: u16 = 2;
    pub const XDG_WM_BASE_PONG: u16 = 3;
}

// ============================================================================
// 3. Apple macOS Cocoa & Metal Runtime ABI
// ============================================================================

pub mod cocoa {
    /// Objective-C opaque object pointer (`id`).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct Id(pub usize);

    /// Objective-C selector pointer (`SEL`).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct Sel(pub usize);

    /// Objective-C class pointer (`Class`).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct Class(pub usize);

    // NSWindowStyleMask bitflags
    pub const NS_WINDOW_STYLE_BORDERLESS: usize = 0;
    pub const NS_WINDOW_STYLE_TITLED: usize = 1 << 0;
    pub const NS_WINDOW_STYLE_CLOSABLE: usize = 1 << 1;
    pub const NS_WINDOW_STYLE_MINIATURIZABLE: usize = 1 << 2;
    pub const NS_WINDOW_STYLE_RESIZABLE: usize = 1 << 3;
    pub const NS_WINDOW_STYLE_FULL_SIZE_CONTENT: usize = 1 << 15;

    // NSBackingStoreType
    pub const NS_BACKING_STORE_BUFFERED: usize = 2;

    /// 2D origin and size matching Cocoa `NSRect`.
    #[repr(C)]
    #[derive(Debug, Clone, Copy, PartialEq, Default)]
    pub struct NsRect {
        pub x: f64,
        pub y: f64,
        pub width: f64,
        pub height: f64,
    }

    impl NsRect {
        pub fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
            Self {
                x,
                y,
                width,
                height,
            }
        }
    }
}
