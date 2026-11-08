//! Pure-Rust Wayland Client Protocol Backend and Hardware Event Loop.
//!
//! Provides zero-dependency native Wayland windowing over UNIX domain sockets
//! (`$XDG_RUNTIME_DIR/$WAYLAND_DISPLAY` or `wayland-0`), wire protocol encoding/decoding,
//! POSIX shared memory surface presentation (`wl_shm`), XDG Shell window lifecycle
//! (`xdg_wm_base`, `xdg_surface`, `xdg_toplevel`), and input event translation (`wl_seat`).

#![cfg(target_os = "linux")]

use crate::events::{KeyCode, Modifiers, MouseButton, PlatformEvent};
use crate::raw_abi::wayland::WaylandHeader;
use crate::window::{
    CursorIcon, EventLoop, EventLoopControl, PlatformWindow, WindowConfig, WindowMode,
};
use std::collections::VecDeque;
use std::io::Read;
use std::os::unix::io::{AsRawFd, RawFd};
use std::os::unix::net::UnixStream;

// Standard Wayland wire format constants
pub const WL_DISPLAY_ID: u32 = 1;

// wl_display opcodes
pub const WL_DISPLAY_SYNC: u16 = 0;
pub const WL_DISPLAY_GET_REGISTRY: u16 = 1;

// wl_registry opcodes
pub const WL_REGISTRY_BIND: u16 = 0;

// wl_compositor opcodes
pub const WL_COMPOSITOR_CREATE_SURFACE: u16 = 0;

// wl_shm opcodes & formats
pub const WL_SHM_CREATE_POOL: u16 = 0;
pub const WL_SHM_FORMAT_ARGB8888: u32 = 0;
pub const WL_SHM_FORMAT_XRGB8888: u32 = 1;

// wl_shm_pool opcodes
pub const WL_SHM_POOL_CREATE_BUFFER: u16 = 0;
pub const WL_SHM_POOL_DESTROY: u16 = 1;
pub const WL_SHM_POOL_RESIZE: u16 = 2;

// wl_surface opcodes
pub const WL_SURFACE_DESTROY: u16 = 0;
pub const WL_SURFACE_ATTACH: u16 = 1;
pub const WL_SURFACE_DAMAGE: u16 = 2;
pub const WL_SURFACE_COMMIT: u16 = 6;

// xdg_wm_base opcodes
pub const XDG_WM_BASE_DESTROY: u16 = 0;
pub const XDG_WM_BASE_GET_XDG_SURFACE: u16 = 2;
pub const XDG_WM_BASE_PONG: u16 = 3;

// xdg_surface opcodes
pub const XDG_SURFACE_DESTROY: u16 = 0;
pub const XDG_SURFACE_GET_TOPLEVEL: u16 = 1;
pub const XDG_SURFACE_ACK_CONFIGURE: u16 = 4;

// xdg_toplevel opcodes
pub const XDG_TOPLEVEL_DESTROY: u16 = 0;
pub const XDG_TOPLEVEL_SET_TITLE: u16 = 2;
pub const XDG_TOPLEVEL_SET_APP_ID: u16 = 3;
pub const XDG_TOPLEVEL_SET_MAXIMIZED: u16 = 9;
pub const XDG_TOPLEVEL_UNSET_MAXIMIZED: u16 = 10;
pub const XDG_TOPLEVEL_SET_MINIMIZED: u16 = 13;

// wl_seat opcodes
pub const WL_SEAT_GET_POINTER: u16 = 0;
pub const WL_SEAT_GET_KEYBOARD: u16 = 1;

/// Low-level Wayland wire protocol message encoder.
#[derive(Debug, Default)]
pub struct WaylandEncoder {
    buffer: Vec<u8>,
}

impl WaylandEncoder {
    /// Creates a new empty message encoder.
    pub fn new() -> Self {
        Self { buffer: Vec::with_capacity(128) }
    }

    /// Clears the encoder buffer.
    pub fn clear(&mut self) {
        self.buffer.clear();
    }

    /// Returns the encoded message slice.
    pub fn as_bytes(&self) -> &[u8] {
        &self.buffer
    }

    /// Reserves space for the 8-byte Wayland header.
    pub fn begin_message(&mut self) {
        self.buffer.clear();
        self.buffer.extend_from_slice(&[0u8; 8]);
    }

    /// Finalizes the header with the target object ID and opcode.
    pub fn finish_message(&mut self, object_id: u32, opcode: u16) {
        let size = self.buffer.len() as u16;
        let header = WaylandHeader {
            object_id,
            opcode,
            size,
        };
        self.buffer[0..8].copy_from_slice(&header.to_bytes());
    }

    /// Appends a 32-bit unsigned integer.
    pub fn push_u32(&mut self, val: u32) {
        self.buffer.extend_from_slice(&val.to_ne_bytes());
    }

    /// Appends a 32-bit signed integer.
    pub fn push_i32(&mut self, val: i32) {
        self.buffer.extend_from_slice(&val.to_ne_bytes());
    }

    /// Appends a 24.8 signed fixed-point value.
    pub fn push_fixed(&mut self, val: f64) {
        let fixed = (val * 256.0) as i32;
        self.push_i32(fixed);
    }

    /// Appends a length-prefixed, null-terminated, 4-byte aligned string.
    pub fn push_string(&mut self, s: &str) {
        let bytes = s.as_bytes();
        let len = (bytes.len() + 1) as u32; // including trailing null
        self.push_u32(len);
        self.buffer.extend_from_slice(bytes);
        self.buffer.push(0); // trailing null
        // Align to 4 bytes
        while !self.buffer.len().is_multiple_of(4) {
            self.buffer.push(0);
        }
    }
}

/// Low-level Wayland wire protocol message decoder.
#[derive(Debug)]
pub struct WaylandDecoder<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> WaylandDecoder<'a> {
    /// Creates a decoder for the specified message payload (after the 8-byte header).
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }

    /// Remaining undecoded bytes.
    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.offset)
    }

    /// Reads a 32-bit unsigned integer.
    pub fn read_u32(&mut self) -> Option<u32> {
        if self.remaining() < 4 {
            return None;
        }
        let val = u32::from_ne_bytes(self.data[self.offset..self.offset + 4].try_into().unwrap());
        self.offset += 4;
        Some(val)
    }

    /// Reads a 32-bit signed integer.
    pub fn read_i32(&mut self) -> Option<i32> {
        if self.remaining() < 4 {
            return None;
        }
        let val = i32::from_ne_bytes(self.data[self.offset..self.offset + 4].try_into().unwrap());
        self.offset += 4;
        Some(val)
    }

    /// Reads a 24.8 signed fixed-point number as f64.
    pub fn read_fixed(&mut self) -> Option<f64> {
        self.read_i32().map(|fixed| fixed as f64 / 256.0)
    }

    /// Reads a length-prefixed, 4-byte aligned string.
    pub fn read_string(&mut self) -> Option<String> {
        let len = self.read_u32()? as usize;
        if len == 0 || self.remaining() < len {
            return None;
        }
        let raw_str = &self.data[self.offset..self.offset + len];
        let aligned_len = (len + 3) & !3;
        self.offset += aligned_len.min(self.remaining());

        let trimmed = if let Some(null_pos) = raw_str.iter().position(|&b| b == 0) {
            &raw_str[..null_pos]
        } else {
            raw_str
        };
        String::from_utf8(trimmed.to_vec()).ok()
    }
}

/// Helper to send bytes with an optional file descriptor via `sendmsg` (`SCM_RIGHTS`).
fn sendmsg_with_fd(socket_fd: RawFd, data: &[u8], fd_to_send: Option<RawFd>) -> std::io::Result<()> {
    unsafe {
        let mut iov = libc::iovec {
            iov_base: data.as_ptr() as *mut libc::c_void,
            iov_len: data.len(),
        };

        let mut msg: libc::msghdr = std::mem::zeroed();
        msg.msg_iov = &mut iov;
        msg.msg_iovlen = 1;

        if let Some(fd) = fd_to_send {
            let cmsg_space = libc::CMSG_SPACE(std::mem::size_of::<RawFd>() as u32) as usize;
            let mut cmsg_buf = vec![0u8; cmsg_space];

            msg.msg_control = cmsg_buf.as_mut_ptr() as *mut libc::c_void;
            msg.msg_controllen = cmsg_space;

            let cmsg = libc::CMSG_FIRSTHDR(&msg);
            (*cmsg).cmsg_level = libc::SOL_SOCKET;
            (*cmsg).cmsg_type = libc::SCM_RIGHTS;
            (*cmsg).cmsg_len = libc::CMSG_LEN(std::mem::size_of::<RawFd>() as u32) as usize;

            let fd_ptr = libc::CMSG_DATA(cmsg) as *mut RawFd;
            std::ptr::write_unaligned(fd_ptr, fd);

            let ret = libc::sendmsg(socket_fd, &msg, 0);
            if ret < 0 {
                return Err(std::io::Error::last_os_error());
            }
        } else {
            let ret = libc::sendmsg(socket_fd, &msg, 0);
            if ret < 0 {
                return Err(std::io::Error::last_os_error());
            }
        }
    }
    Ok(())
}

/// Creates a POSIX shared memory file descriptor of the requested byte size.
fn create_shm_fd(size: usize) -> std::io::Result<RawFd> {
    unsafe {
        let name = std::ffi::CString::new("strata-wayland-shm").unwrap();
        let mut fd = libc::memfd_create(name.as_ptr(), libc::MFD_CLOEXEC | libc::MFD_ALLOW_SEALING);
        if fd < 0 {
            // Fallback: temp file in XDG_RUNTIME_DIR or /tmp
            let path = if let Ok(runtime) = std::env::var("XDG_RUNTIME_DIR") {
                format!("{}/strata-shm-XXXXXX\0", runtime)
            } else {
                "/tmp/strata-shm-XXXXXX\0".to_string()
            };
            let mut template = path.into_bytes();
            fd = libc::mkostemp(template.as_mut_ptr() as *mut libc::c_char, libc::O_CLOEXEC);
            if fd >= 0 {
                libc::unlink(template.as_ptr() as *const libc::c_char);
            }
        }
        if fd < 0 {
            return Err(std::io::Error::last_os_error());
        }

        if libc::ftruncate(fd, size as libc::off_t) < 0 {
            libc::close(fd);
            return Err(std::io::Error::last_os_error());
        }

        Ok(fd)
    }
}

/// Shared memory buffer mapping for software surface swapchain presentation.
pub struct ShmBuffer {
    pub fd: RawFd,
    pub pool_id: u32,
    pub buffer_id: u32,
    pub width: u32,
    pub height: u32,
    pub size: usize,
    pub ptr: *mut u8,
}

impl ShmBuffer {
    /// Allocates and mmaps a new shared memory pixel buffer.
    pub fn new(pool_id: u32, buffer_id: u32, width: u32, height: u32) -> Result<Self, String> {
        let size = (width * height * 4) as usize;
        let fd = create_shm_fd(size)
            .map_err(|e| format!("Failed to create shm fd: {}", e))?;

        let ptr = unsafe {
            let p = libc::mmap(
                std::ptr::null_mut(),
                size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                0,
            );
            if p == libc::MAP_FAILED {
                libc::close(fd);
                return Err("Failed to mmap shm fd".to_string());
            }
            p as *mut u8
        };

        Ok(Self {
            fd,
            pool_id,
            buffer_id,
            width,
            height,
            size,
            ptr,
        })
    }

    /// Copies BGRA pixels into the mapped shared memory area.
    pub fn write_pixels(&mut self, pixels: &[u8]) {
        let copy_len = pixels.len().min(self.size);
        unsafe {
            std::ptr::copy_nonoverlapping(pixels.as_ptr(), self.ptr, copy_len);
        }
    }
}

impl Drop for ShmBuffer {
    fn drop(&mut self) {
        unsafe {
            libc::munmap(self.ptr as *mut libc::c_void, self.size);
            libc::close(self.fd);
        }
    }
}

/// Discovered global Wayland objects in compositor registry.
#[derive(Debug, Default, Clone)]
pub struct WaylandGlobals {
    pub compositor_name: Option<u32>,
    pub shm_name: Option<u32>,
    pub xdg_wm_base_name: Option<u32>,
    pub seat_name: Option<u32>,
}

/// Pure-Rust Wayland window implementation backed by UNIX domain socket IPC.
pub struct WaylandWindow {
    stream: Option<UnixStream>,
    config: WindowConfig,
    scale_factor: f32,
    is_focused: bool,
    cursor: CursorIcon,
    pub redraw_requested: bool,
    event_queue: VecDeque<PlatformEvent>,
    encoder: WaylandEncoder,
    next_id: u32,
    pub globals: WaylandGlobals,
    // Bound object IDs
    pub registry_id: u32,
    pub compositor_id: u32,
    pub shm_id: u32,
    pub xdg_wm_base_id: u32,
    pub seat_id: u32,
    pub surface_id: u32,
    pub xdg_surface_id: u32,
    pub xdg_toplevel_id: u32,
    pub pointer_id: u32,
    pub keyboard_id: u32,
    // Swapchain Shm Buffer
    pub shm_buffer: Option<ShmBuffer>,
    read_buffer: Vec<u8>,
    pub cursor_pos: (f32, f32),
    pub last_click: Option<(std::time::Instant, f32, f32)>,
}

impl WaylandWindow {
    /// Attempts to connect to the active Wayland compositor and instantiate a native window.
    pub fn new(config: WindowConfig) -> Result<Self, String> {
        let socket_path = crate::raw_abi::wayland::default_socket_path();
        let stream = UnixStream::connect(&socket_path)
            .map_err(|e| format!("Cannot connect to Wayland socket \"{}\": {}", socket_path, e))?;
        stream
            .set_nonblocking(true)
            .map_err(|e| format!("Failed to set nonblocking stream: {}", e))?;

        let mut window = Self {
            stream: Some(stream),
            config,
            scale_factor: 1.0,
            is_focused: true,
            cursor: CursorIcon::Default,
            redraw_requested: true,
            event_queue: VecDeque::new(),
            encoder: WaylandEncoder::new(),
            next_id: 2,
            globals: WaylandGlobals::default(),
            registry_id: 2,
            compositor_id: 3,
            shm_id: 4,
            xdg_wm_base_id: 5,
            seat_id: 6,
            surface_id: 7,
            xdg_surface_id: 8,
            xdg_toplevel_id: 9,
            pointer_id: 10,
            keyboard_id: 11,
            shm_buffer: None,
            read_buffer: Vec::with_capacity(4096),
            cursor_pos: (0.0, 0.0),
            last_click: None,
        };

        window.init_wayland_connection()?;
        Ok(window)
    }

    /// Allocates an internal object ID.
    fn allocate_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Sends encoded message down the UNIX socket.
    fn send_message(&mut self, fd_to_send: Option<RawFd>) -> Result<(), String> {
        if let Some(ref mut stream) = self.stream {
            let bytes = self.encoder.as_bytes();
            sendmsg_with_fd(stream.as_raw_fd(), bytes, fd_to_send)
                .map_err(|e| format!("Wayland sendmsg error: {}", e))?;
            self.encoder.clear();
            Ok(())
        } else {
            Ok(())
        }
    }

    /// Performs the initial Wayland connection handshake: binds registry, waits for globals,
    /// creates surface and toplevel window.
    fn init_wayland_connection(&mut self) -> Result<(), String> {
        // 1. Get registry (object 1, opcode 1 -> registry_id 2)
        self.registry_id = self.allocate_id();
        self.encoder.begin_message();
        self.encoder.push_u32(self.registry_id);
        self.encoder.finish_message(WL_DISPLAY_ID, WL_DISPLAY_GET_REGISTRY);
        self.send_message(None)?;

        // 2. Sync to receive initial registry globals
        let sync_cb = self.allocate_id();
        self.encoder.begin_message();
        self.encoder.push_u32(sync_cb);
        self.encoder.finish_message(WL_DISPLAY_ID, WL_DISPLAY_SYNC);
        self.send_message(None)?;

        // Read until sync callback completes
        self.roundtrip(sync_cb)?;

        // 3. Bind required global interfaces
        if let Some(comp_name) = self.globals.compositor_name {
            self.compositor_id = self.allocate_id();
            self.encoder.begin_message();
            self.encoder.push_u32(comp_name);
            self.encoder.push_string("wl_compositor");
            self.encoder.push_u32(4); // version 4
            self.encoder.push_u32(self.compositor_id);
            self.encoder.finish_message(self.registry_id, WL_REGISTRY_BIND);
            self.send_message(None)?;
        } else {
            return Err("Wayland compositor does not export wl_compositor".to_string());
        }

        if let Some(shm_name) = self.globals.shm_name {
            self.shm_id = self.allocate_id();
            self.encoder.begin_message();
            self.encoder.push_u32(shm_name);
            self.encoder.push_string("wl_shm");
            self.encoder.push_u32(1); // version 1
            self.encoder.push_u32(self.shm_id);
            self.encoder.finish_message(self.registry_id, WL_REGISTRY_BIND);
            self.send_message(None)?;
        } else {
            return Err("Wayland compositor does not export wl_shm".to_string());
        }

        if let Some(wm_name) = self.globals.xdg_wm_base_name {
            self.xdg_wm_base_id = self.allocate_id();
            self.encoder.begin_message();
            self.encoder.push_u32(wm_name);
            self.encoder.push_string("xdg_wm_base");
            self.encoder.push_u32(1); // version 1
            self.encoder.push_u32(self.xdg_wm_base_id);
            self.encoder.finish_message(self.registry_id, WL_REGISTRY_BIND);
            self.send_message(None)?;
        } else {
            return Err("Wayland compositor does not export xdg_wm_base".to_string());
        }

        if let Some(seat_name) = self.globals.seat_name {
            self.seat_id = self.allocate_id();
            self.encoder.begin_message();
            self.encoder.push_u32(seat_name);
            self.encoder.push_string("wl_seat");
            self.encoder.push_u32(5); // version 5
            self.encoder.push_u32(self.seat_id);
            self.encoder.finish_message(self.registry_id, WL_REGISTRY_BIND);
            self.send_message(None)?;

            // Bind pointer and keyboard
            self.pointer_id = self.allocate_id();
            self.encoder.begin_message();
            self.encoder.push_u32(self.pointer_id);
            self.encoder.finish_message(self.seat_id, WL_SEAT_GET_POINTER);
            self.send_message(None)?;

            self.keyboard_id = self.allocate_id();
            self.encoder.begin_message();
            self.encoder.push_u32(self.keyboard_id);
            self.encoder.finish_message(self.seat_id, WL_SEAT_GET_KEYBOARD);
            self.send_message(None)?;
        }

        // 4. Create surface
        self.surface_id = self.allocate_id();
        self.encoder.begin_message();
        self.encoder.push_u32(self.surface_id);
        self.encoder.finish_message(self.compositor_id, WL_COMPOSITOR_CREATE_SURFACE);
        self.send_message(None)?;

        // 5. Create xdg_surface
        self.xdg_surface_id = self.allocate_id();
        self.encoder.begin_message();
        self.encoder.push_u32(self.xdg_surface_id);
        self.encoder.push_u32(self.surface_id);
        self.encoder.finish_message(self.xdg_wm_base_id, XDG_WM_BASE_GET_XDG_SURFACE);
        self.send_message(None)?;

        // 6. Create xdg_toplevel
        self.xdg_toplevel_id = self.allocate_id();
        self.encoder.begin_message();
        self.encoder.push_u32(self.xdg_toplevel_id);
        self.encoder.finish_message(self.xdg_surface_id, XDG_SURFACE_GET_TOPLEVEL);
        self.send_message(None)?;

        // Set title and app_id
        self.encoder.begin_message();
        self.encoder.push_string(&self.config.title);
        self.encoder.finish_message(self.xdg_toplevel_id, XDG_TOPLEVEL_SET_TITLE);
        self.send_message(None)?;

        self.encoder.begin_message();
        self.encoder.push_string("strata");
        self.encoder.finish_message(self.xdg_toplevel_id, XDG_TOPLEVEL_SET_APP_ID);
        self.send_message(None)?;

        // Initial commit to request configure event
        self.encoder.begin_message();
        self.encoder.finish_message(self.surface_id, WL_SURFACE_COMMIT);
        self.send_message(None)?;

        // Allocate initial SHM buffer
        self.recreate_shm_buffer(self.config.width, self.config.height)?;

        Ok(())
    }

    /// Re-allocates the shared memory buffer and registers it with the compositor.
    fn recreate_shm_buffer(&mut self, width: u32, height: u32) -> Result<(), String> {
        let pool_id = self.allocate_id();
        let buffer_id = self.allocate_id();
        let buffer = ShmBuffer::new(pool_id, buffer_id, width, height)?;

        // Send wl_shm.create_pool with buffer fd
        self.encoder.begin_message();
        self.encoder.push_u32(pool_id);
        self.encoder.push_i32(buffer.size as i32);
        self.encoder.finish_message(self.shm_id, WL_SHM_CREATE_POOL);
        self.send_message(Some(buffer.fd))?;

        // Send wl_shm_pool.create_buffer
        self.encoder.begin_message();
        self.encoder.push_u32(buffer_id);
        self.encoder.push_i32(0); // offset
        self.encoder.push_i32(width as i32);
        self.encoder.push_i32(height as i32);
        self.encoder.push_i32((width * 4) as i32); // stride
        self.encoder.push_u32(WL_SHM_FORMAT_ARGB8888);
        self.encoder.finish_message(pool_id, WL_SHM_POOL_CREATE_BUFFER);
        self.send_message(None)?;

        self.shm_buffer = Some(buffer);
        Ok(())
    }

    /// Blocks until a specific `wl_callback` fires `done`.
    fn roundtrip(&mut self, target_callback_id: u32) -> Result<(), String> {
        let mut done = false;
        let start = std::time::Instant::now();
        while !done {
            if start.elapsed() > std::time::Duration::from_secs(5) {
                return Err("Wayland roundtrip timed out".to_string());
            }
            self.read_and_dispatch(&mut done, target_callback_id)?;
            if !done {
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        }
        Ok(())
    }

    /// Reads available bytes from socket and dispatches Wayland events.
    fn read_and_dispatch(&mut self, sync_done: &mut bool, target_callback_id: u32) -> Result<(), String> {
        if let Some(ref mut stream) = self.stream {
            let mut chunk = [0u8; 4096];
            loop {
                match stream.read(&mut chunk) {
                    Ok(0) => return Err("Wayland server closed connection".to_string()),
                    Ok(n) => {
                        self.read_buffer.extend_from_slice(&chunk[..n]);
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(e) => return Err(format!("Wayland socket read error: {}", e)),
                }
            }

            // Parse complete messages from buffer
            while self.read_buffer.len() >= 8 {
                let header = WaylandHeader::from_bytes(&self.read_buffer[..8]).unwrap();
                let msg_len = header.size as usize;
                if self.read_buffer.len() < msg_len {
                    break;
                }

                let payload = self.read_buffer[8..msg_len].to_vec();
                self.read_buffer.drain(..msg_len);
                self.handle_event(header.object_id, header.opcode, &payload, sync_done, target_callback_id);
            }
        }
        Ok(())
    }

    /// Internal event dispatcher handling registry, ping/pong, configure, and input events.
    fn handle_event(
        &mut self,
        object_id: u32,
        opcode: u16,
        payload: &[u8],
        sync_done: &mut bool,
        target_callback_id: u32,
    ) {
        // 1. Sync callback done event (opcode 0)
        if object_id == target_callback_id && opcode == 0 {
            *sync_done = true;
            return;
        }

        // 2. Registry global advertisement (registry_id, opcode 0)
        if object_id == self.registry_id && opcode == 0 {
            let mut decoder = WaylandDecoder::new(payload);
            if let (Some(name), Some(interface), _version) = (
                decoder.read_u32(),
                decoder.read_string(),
                decoder.read_u32(),
            ) {
                match interface.as_str() {
                    "wl_compositor" => self.globals.compositor_name = Some(name),
                    "wl_shm" => self.globals.shm_name = Some(name),
                    "xdg_wm_base" => self.globals.xdg_wm_base_name = Some(name),
                    "wl_seat" => self.globals.seat_name = Some(name),
                    _ => {}
                }
            }
            return;
        }

        // 3. xdg_wm_base ping event (opcode 0)
        if object_id == self.xdg_wm_base_id && opcode == 0 {
            let mut decoder = WaylandDecoder::new(payload);
            if let Some(serial) = decoder.read_u32() {
                // Respond immediately with pong
                self.encoder.begin_message();
                self.encoder.push_u32(serial);
                self.encoder.finish_message(self.xdg_wm_base_id, XDG_WM_BASE_PONG);
                let _ = self.send_message(None);
            }
            return;
        }

        // 4. xdg_surface configure event (opcode 0)
        if object_id == self.xdg_surface_id && opcode == 0 {
            let mut decoder = WaylandDecoder::new(payload);
            if let Some(serial) = decoder.read_u32() {
                // Ack configure
                self.encoder.begin_message();
                self.encoder.push_u32(serial);
                self.encoder.finish_message(self.xdg_surface_id, XDG_SURFACE_ACK_CONFIGURE);
                let _ = self.send_message(None);
                self.redraw_requested = true;
            }
            return;
        }

        // 5. xdg_toplevel configure event (opcode 0)
        if object_id == self.xdg_toplevel_id && opcode == 0 {
            let mut decoder = WaylandDecoder::new(payload);
            if let (Some(w), Some(h)) = (decoder.read_i32(), decoder.read_i32()) {
                if w > 0 && h > 0 && (w as u32 != self.config.width || h as u32 != self.config.height) {
                    let (width, height) = (w as u32, h as u32);
                    self.config.width = width;
                    self.config.height = height;
                    let _ = self.recreate_shm_buffer(width, height);
                    self.event_queue.push_back(PlatformEvent::WindowResized { width, height });
                }
            }
            return;
        }

        // 6. xdg_toplevel close event (opcode 1)
        if object_id == self.xdg_toplevel_id && opcode == 1 {
            self.event_queue.push_back(PlatformEvent::WindowCloseRequested);
            return;
        }

        // 7. Pointer events
        if object_id == self.pointer_id {
            match opcode {
                // enter(serial, surface, surface_x, surface_y)
                0 => {
                    let mut decoder = WaylandDecoder::new(payload);
                    let _serial = decoder.read_u32();
                    let _surface = decoder.read_u32();
                    if let (Some(x), Some(y)) = (decoder.read_fixed(), decoder.read_fixed()) {
                        let px = x as f32;
                        let py = y as f32;
                        self.cursor_pos = (px, py);
                        self.event_queue.push_back(PlatformEvent::MouseMove {
                            x: px,
                            y: py,
                        });
                    }
                }
                // leave(serial, surface)
                1 => {
                    self.event_queue.push_back(PlatformEvent::CursorLeft);
                }
                // motion(time, surface_x, surface_y)
                2 => {
                    let mut decoder = WaylandDecoder::new(payload);
                    let _time = decoder.read_u32();
                    if let (Some(x), Some(y)) = (decoder.read_fixed(), decoder.read_fixed()) {
                        let px = x as f32;
                        let py = y as f32;
                        self.cursor_pos = (px, py);
                        self.event_queue.push_back(PlatformEvent::MouseMove {
                            x: px,
                            y: py,
                        });
                    }
                }
                // button(serial, time, button, state)
                3 => {
                    let mut decoder = WaylandDecoder::new(payload);
                    let _serial = decoder.read_u32();
                    let _time = decoder.read_u32();
                    if let (Some(btn), Some(state)) = (decoder.read_u32(), decoder.read_u32()) {
                        let button = match btn {
                            0x110 => MouseButton::Left,
                            0x111 => MouseButton::Right,
                            0x112 => MouseButton::Middle,
                            _ => MouseButton::Left,
                        };
                        let (x, y) = self.cursor_pos;
                        if state == 1 {
                            if button == MouseButton::Left {
                                let now = std::time::Instant::now();
                                if let Some((prev_time, px, py)) = self.last_click {
                                    if now.duration_since(prev_time).as_millis() < 400
                                        && (px - x).abs() < 6.0
                                        && (py - y).abs() < 6.0
                                    {
                                        self.event_queue.push_back(PlatformEvent::DoubleClick {
                                            button,
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
                            }
                            self.event_queue.push_back(PlatformEvent::MouseDown { x, y, button });
                        } else {
                            self.event_queue.push_back(PlatformEvent::MouseUp { x, y, button });
                        }
                    }
                }
                // axis(time, axis, value)
                4 => {
                    let mut decoder = WaylandDecoder::new(payload);
                    let _time = decoder.read_u32();
                    if let (Some(axis), Some(value)) = (decoder.read_u32(), decoder.read_fixed()) {
                        let (delta_x, delta_y) = if axis == 0 {
                            (0.0, value as f32)
                        } else {
                            (value as f32, 0.0)
                        };
                        self.event_queue.push_back(PlatformEvent::Scroll {
                            delta_x,
                            delta_y,
                            is_momentum: false,
                        });
                    }
                }
                _ => {}
            }
            return;
        }

        // 8. Keyboard key event
        if object_id == self.keyboard_id && opcode == 3 {
            let mut decoder = WaylandDecoder::new(payload);
            let _serial = decoder.read_u32();
            let _time = decoder.read_u32();
            if let (Some(key), Some(state)) = (decoder.read_u32(), decoder.read_u32()) {
                let (key_code, ch) = match key {
                    1 => (KeyCode::Escape, None),
                    2 => (KeyCode::Digit1, Some('1')),
                    3 => (KeyCode::Digit2, Some('2')),
                    4 => (KeyCode::Digit3, Some('3')),
                    5 => (KeyCode::Digit4, Some('4')),
                    6 => (KeyCode::Digit5, Some('5')),
                    7 => (KeyCode::Digit6, Some('6')),
                    8 => (KeyCode::Digit7, Some('7')),
                    9 => (KeyCode::Digit8, Some('8')),
                    10 => (KeyCode::Digit9, Some('9')),
                    11 => (KeyCode::Digit0, Some('0')),
                    14 => (KeyCode::Backspace, None),
                    15 => (KeyCode::Tab, Some('\t')),
                    16 => (KeyCode::KeyQ, Some('q')),
                    17 => (KeyCode::KeyW, Some('w')),
                    18 => (KeyCode::KeyE, Some('e')),
                    19 => (KeyCode::KeyR, Some('r')),
                    20 => (KeyCode::KeyT, Some('t')),
                    21 => (KeyCode::KeyY, Some('y')),
                    22 => (KeyCode::KeyU, Some('u')),
                    23 => (KeyCode::KeyI, Some('i')),
                    24 => (KeyCode::KeyO, Some('o')),
                    25 => (KeyCode::KeyP, Some('p')),
                    28 => (KeyCode::Enter, Some('\n')),
                    30 => (KeyCode::KeyA, Some('a')),
                    31 => (KeyCode::KeyS, Some('s')),
                    32 => (KeyCode::KeyD, Some('d')),
                    33 => (KeyCode::KeyF, Some('f')),
                    34 => (KeyCode::KeyG, Some('g')),
                    35 => (KeyCode::KeyH, Some('h')),
                    36 => (KeyCode::KeyJ, Some('j')),
                    37 => (KeyCode::KeyK, Some('k')),
                    38 => (KeyCode::KeyL, Some('l')),
                    44 => (KeyCode::KeyZ, Some('z')),
                    45 => (KeyCode::KeyX, Some('x')),
                    46 => (KeyCode::KeyC, Some('c')),
                    47 => (KeyCode::KeyV, Some('v')),
                    48 => (KeyCode::KeyB, Some('b')),
                    49 => (KeyCode::KeyN, Some('n')),
                    50 => (KeyCode::KeyM, Some('m')),
                    57 => (KeyCode::Space, Some(' ')),
                    103 => (KeyCode::ArrowUp, None),
                    105 => (KeyCode::ArrowLeft, None),
                    106 => (KeyCode::ArrowRight, None),
                    108 => (KeyCode::ArrowDown, None),
                    111 => (KeyCode::Delete, None),
                    _ => (KeyCode::Unknown, None),
                };
                if state == 1 {
                    self.event_queue.push_back(PlatformEvent::KeyDown {
                        key: key_code,
                        modifiers: Modifiers::default(),
                    });
                    if let Some(c) = ch {
                        self.event_queue.push_back(PlatformEvent::ReceivedCharacter(c));
                    }
                } else {
                    self.event_queue.push_back(PlatformEvent::KeyUp {
                        key: key_code,
                        modifiers: Modifiers::default(),
                    });
                }
            }
        }
    }

    /// Presents a 32-bit BGRA buffer directly to the Wayland surface via `wl_shm`.
    pub fn present_bgra(&mut self, pixels: &[u8], width: u32, height: u32) -> Result<(), String> {
        if let Some(ref mut shm) = self.shm_buffer {
            if shm.width != width || shm.height != height {
                self.recreate_shm_buffer(width, height)?;
            }
        } else {
            self.recreate_shm_buffer(width, height)?;
        }

        if let Some(ref mut shm) = self.shm_buffer {
            shm.write_pixels(pixels);

            // Attach buffer
            self.encoder.begin_message();
            self.encoder.push_u32(shm.buffer_id);
            self.encoder.push_i32(0); // x
            self.encoder.push_i32(0); // y
            self.encoder.finish_message(self.surface_id, WL_SURFACE_ATTACH);
            self.send_message(None)?;

            // Damage surface
            self.encoder.begin_message();
            self.encoder.push_i32(0);
            self.encoder.push_i32(0);
            self.encoder.push_i32(width as i32);
            self.encoder.push_i32(height as i32);
            self.encoder.finish_message(self.surface_id, WL_SURFACE_DAMAGE);
            self.send_message(None)?;

            // Commit surface
            self.encoder.begin_message();
            self.encoder.finish_message(self.surface_id, WL_SURFACE_COMMIT);
            self.send_message(None)?;
        }

        Ok(())
    }

    /// Pumps incoming Wayland socket events and returns translated platform events.
    pub fn poll_native_events(&mut self) -> Vec<PlatformEvent> {
        let mut dummy = false;
        let _ = self.read_and_dispatch(&mut dummy, 0);
        self.poll_events()
    }
}

impl PlatformWindow for WaylandWindow {
    fn title(&self) -> &str {
        &self.config.title
    }

    fn set_title(&mut self, title: &str) {
        self.config.title = title.to_string();
        self.encoder.begin_message();
        self.encoder.push_string(title);
        self.encoder.finish_message(self.xdg_toplevel_id, XDG_TOPLEVEL_SET_TITLE);
        let _ = self.send_message(None);
    }

    fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    fn set_size(&mut self, width: u32, height: u32) {
        self.config.width = width;
        self.config.height = height;
        let _ = self.recreate_shm_buffer(width, height);
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
        match mode {
            WindowMode::Maximized => {
                self.encoder.begin_message();
                self.encoder.finish_message(self.xdg_toplevel_id, XDG_TOPLEVEL_SET_MAXIMIZED);
                let _ = self.send_message(None);
            }
            WindowMode::Minimized => {
                self.encoder.begin_message();
                self.encoder.finish_message(self.xdg_toplevel_id, XDG_TOPLEVEL_SET_MINIMIZED);
                let _ = self.send_message(None);
            }
            WindowMode::Windowed => {
                self.encoder.begin_message();
                self.encoder.finish_message(self.xdg_toplevel_id, XDG_TOPLEVEL_UNSET_MAXIMIZED);
                let _ = self.send_message(None);
            }
            _ => {}
        }
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

/// Pure-Rust Wayland native event loop driving event dispatch and frame pacing.
pub struct WaylandEventLoop {
    pub window: WaylandWindow,
    pub control: EventLoopControl,
}

impl WaylandEventLoop {
    /// Creates a new event loop driving the given Wayland window.
    pub fn new(window: WaylandWindow) -> Self {
        Self {
            window,
            control: EventLoopControl::Poll,
        }
    }
}

impl EventLoop for WaylandEventLoop {
    fn run<F>(&mut self, mut handler: F)
    where
        F: FnMut(PlatformEvent, &mut EventLoopControl),
    {
        while self.control != EventLoopControl::Exit {
            let events = self.window.poll_native_events();
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
