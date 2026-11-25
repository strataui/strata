//! Pure-Rust Linux X11 Windowing, Event Loop, and MIT-SHM Shared Memory Presentation.
//!
//! Provides zero-dependency native X11 wire protocol communication over `/tmp/.X11-unix/X0`,
//! hardware event decoding, POSIX/SysV shared memory surface presentation (`MIT-SHM`),
//! and legacy xlib compatibility fallback.

#![cfg(target_os = "linux")]

use std::collections::VecDeque;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

#[cfg(feature = "xlib")]
use std::ffi::CString;
#[cfg(feature = "xlib")]
use std::os::raw::{c_char, c_int, c_ulong};
#[cfg(feature = "xlib")]
use x11::xlib;

use crate::events::{KeyCode, Modifiers, MouseButton, PlatformEvent};
use crate::surface::PresentError;
use crate::window::{
    CursorIcon, EventLoop, EventLoopControl, NativeSurfaceWindow, PlatformWindow, WindowConfig,
    WindowMode,
};

// Core X11 Protocol Request Opcodes
pub const X11_OPCODE_CREATE_WINDOW: u8 = 1;
pub const X11_OPCODE_CHANGE_WINDOW_ATTRIBUTES: u8 = 2;
pub const X11_OPCODE_MAP_WINDOW: u8 = 8;
pub const X11_OPCODE_INTERN_ATOM: u8 = 16;
pub const X11_OPCODE_CHANGE_PROPERTY: u8 = 18;
pub const X11_OPCODE_SEND_EVENT: u8 = 25;
pub const X11_OPCODE_CREATE_GC: u8 = 55;
pub const X11_OPCODE_PUT_IMAGE: u8 = 72;
pub const X11_OPCODE_QUERY_EXTENSION: u8 = 98;

// Standard X11 Predefined Atoms
pub const XA_STRING: u32 = 31;
pub const XA_WM_NAME: u32 = 39;

// MIT-SHM Extension Request Opcodes (Relative to Extension Major Opcode)
pub const X11_SHM_OPCODE_ATTACH: u8 = 1;
pub const X11_SHM_OPCODE_PUT_IMAGE: u8 = 3;

/// Resolves the default X11 UNIX domain socket path from the `$DISPLAY` environment variable.
pub fn default_x11_socket_path() -> String {
    let display = std::env::var("DISPLAY").unwrap_or_else(|_| ":0".to_string());
    let display_num = display
        .rsplit(':')
        .next()
        .unwrap_or("0")
        .split('.')
        .next()
        .unwrap_or("0")
        .parse::<u32>()
        .unwrap_or(0);
    format!("/tmp/.X11-unix/X{}", display_num)
}

/// Pure-Rust X11 wire protocol request encoder.
#[derive(Debug, Default)]
pub struct X11WireEncoder {
    buffer: Vec<u8>,
}

impl X11WireEncoder {
    /// Creates a new encoder with default capacity.
    pub fn new() -> Self {
        Self {
            buffer: Vec::with_capacity(256),
        }
    }

    /// Clears the encoder buffer.
    pub fn clear(&mut self) {
        self.buffer.clear();
    }

    /// Returns the encoded byte slice.
    pub fn as_bytes(&self) -> &[u8] {
        &self.buffer
    }

    /// Reserves space for the 4-byte X11 request header: [opcode, detail, len_lo, len_hi].
    pub fn begin_request(&mut self, opcode: u8, detail: u8) {
        self.buffer.clear();
        self.buffer.push(opcode);
        self.buffer.push(detail);
        self.buffer.push(0);
        self.buffer.push(0);
    }

    /// Finalizes the request header by calculating total length in 4-byte words, padding as needed.
    pub fn finish_request(&mut self) {
        while !self.buffer.len().is_multiple_of(4) {
            self.buffer.push(0);
        }
        let len_words = (self.buffer.len() / 4) as u16;
        let len_bytes = len_words.to_ne_bytes();
        self.buffer[2] = len_bytes[0];
        self.buffer[3] = len_bytes[1];
    }

    /// Appends an 8-bit unsigned integer.
    pub fn push_u8(&mut self, val: u8) {
        self.buffer.push(val);
    }

    /// Appends a 16-bit unsigned integer in native byte order.
    pub fn push_u16(&mut self, val: u16) {
        self.buffer.extend_from_slice(&val.to_ne_bytes());
    }

    /// Appends a 16-bit signed integer in native byte order.
    pub fn push_i16(&mut self, val: i16) {
        self.buffer.extend_from_slice(&val.to_ne_bytes());
    }

    /// Appends a 32-bit unsigned integer in native byte order.
    pub fn push_u32(&mut self, val: u32) {
        self.buffer.extend_from_slice(&val.to_ne_bytes());
    }

    /// Appends arbitrary raw bytes.
    pub fn push_bytes(&mut self, bytes: &[u8]) {
        self.buffer.extend_from_slice(bytes);
    }

    /// Appends a string and aligns the buffer to a 4-byte boundary.
    pub fn push_padded_string(&mut self, s: &str) {
        let bytes = s.as_bytes();
        self.buffer.extend_from_slice(bytes);
        while !self.buffer.len().is_multiple_of(4) {
            self.buffer.push(0);
        }
    }
}

/// Pure-Rust X11 wire protocol reply and event decoder.
#[derive(Debug)]
pub struct X11WireDecoder<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> X11WireDecoder<'a> {
    /// Creates a decoder over the given buffer slice.
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }

    /// Returns the number of remaining bytes.
    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.offset)
    }

    /// Reads an 8-bit unsigned integer.
    pub fn read_u8(&mut self) -> Option<u8> {
        if self.remaining() < 1 {
            return None;
        }
        let val = self.data[self.offset];
        self.offset += 1;
        Some(val)
    }

    /// Reads a 16-bit unsigned integer in native byte order.
    pub fn read_u16(&mut self) -> Option<u16> {
        if self.remaining() < 2 {
            return None;
        }
        let val = u16::from_ne_bytes(self.data[self.offset..self.offset + 2].try_into().unwrap());
        self.offset += 2;
        Some(val)
    }

    /// Reads a 16-bit signed integer in native byte order.
    pub fn read_i16(&mut self) -> Option<i16> {
        if self.remaining() < 2 {
            return None;
        }
        let val = i16::from_ne_bytes(self.data[self.offset..self.offset + 2].try_into().unwrap());
        self.offset += 2;
        Some(val)
    }

    /// Reads a 32-bit unsigned integer in native byte order.
    pub fn read_u32(&mut self) -> Option<u32> {
        if self.remaining() < 4 {
            return None;
        }
        let val = u32::from_ne_bytes(self.data[self.offset..self.offset + 4].try_into().unwrap());
        self.offset += 4;
        Some(val)
    }

    /// Reads `n` raw bytes.
    pub fn read_bytes(&mut self, n: usize) -> Option<&'a [u8]> {
        if self.remaining() < n {
            return None;
        }
        let slice = &self.data[self.offset..self.offset + n];
        self.offset += n;
        Some(slice)
    }
}

/// Discovered display server properties extracted from initial X11 connection setup reply.
#[derive(Debug, Clone)]
pub struct X11ServerInfo {
    pub resource_id_base: u32,
    pub resource_id_mask: u32,
    pub root_window: u32,
    pub root_visual: u32,
    pub root_depth: u8,
    pub black_pixel: u32,
    pub white_pixel: u32,
    pub width_in_pixels: u16,
    pub height_in_pixels: u16,
}

/// SysV Shared Memory Segment backing MIT-SHM zero-copy presentation.
pub struct X11ShmSegment {
    pub shmid: libc::c_int,
    pub shmseg_id: u32,
    pub size: usize,
    pub ptr: *mut u8,
}

impl X11ShmSegment {
    /// Allocates and maps a new shared memory segment.
    pub fn new(shmseg_id: u32, size: usize) -> Result<Self, String> {
        unsafe {
            let shmid = libc::shmget(libc::IPC_PRIVATE, size, libc::IPC_CREAT | 0o777);
            if shmid < 0 {
                return Err(format!(
                    "shmget allocation failed: {}",
                    std::io::Error::last_os_error()
                ));
            }

            let ptr = libc::shmat(shmid, std::ptr::null(), 0);
            if ptr == -1isize as *mut libc::c_void {
                libc::shmctl(shmid, libc::IPC_RMID, std::ptr::null_mut());
                return Err(format!(
                    "shmat mapping failed: {}",
                    std::io::Error::last_os_error()
                ));
            }

            // Immediately mark for removal on disconnect so resources are automatically reclaimed
            libc::shmctl(shmid, libc::IPC_RMID, std::ptr::null_mut());

            Ok(Self {
                shmid,
                shmseg_id,
                size,
                ptr: ptr as *mut u8,
            })
        }
    }

    /// Copies a slice of BGRA8 pixel bytes into the shared memory buffer.
    pub fn write_pixels(&mut self, pixels: &[u8]) {
        let copy_len = pixels.len().min(self.size);
        unsafe {
            std::ptr::copy_nonoverlapping(pixels.as_ptr(), self.ptr, copy_len);
        }
    }
}

impl Drop for X11ShmSegment {
    fn drop(&mut self) {
        unsafe {
            libc::shmdt(self.ptr as *const libc::c_void);
        }
    }
}

/// Pure-Rust X11 client connection manager communicating directly over UNIX domain sockets.
pub struct PureX11Client {
    pub stream: UnixStream,
    pub info: X11ServerInfo,
    pub next_resource_id: u32,
    pub mit_shm_opcode: Option<u8>,
    pub read_buffer: Vec<u8>,
    pub encoder: X11WireEncoder,
}

impl PureX11Client {
    /// Establishes connection to the active X server and performs the protocol handshake.
    pub fn connect(socket_path: &str) -> Result<Self, String> {
        let mut stream = UnixStream::connect(socket_path)
            .map_err(|e| format!("Cannot connect to X11 socket \"{}\": {}", socket_path, e))?;

        // 1. Connection setup packet (12 bytes)
        // Little-endian byte-order: 'l' (0x6C)
        let mut setup_packet = [0u8; 12];
        setup_packet[0] = b'l';
        setup_packet[2..4].copy_from_slice(&11u16.to_ne_bytes()); // X11 Major Version
        setup_packet[4..6].copy_from_slice(&0u16.to_ne_bytes()); // Minor Version
        stream
            .write_all(&setup_packet)
            .map_err(|e| format!("Failed to send X11 connection setup: {}", e))?;

        // 2. Read setup reply header (8 bytes)
        let mut reply_header = [0u8; 8];
        stream
            .read_exact(&mut reply_header)
            .map_err(|e| format!("Failed to read X11 setup reply header: {}", e))?;

        let status = reply_header[0];
        let extra_length = u16::from_ne_bytes(reply_header[6..8].try_into().unwrap()) as usize * 4;

        let mut extra_data = vec![0u8; extra_length];
        stream
            .read_exact(&mut extra_data)
            .map_err(|e| format!("Failed to read X11 setup extra data: {}", e))?;

        if status != 1 {
            let reason = String::from_utf8_lossy(&extra_data);
            return Err(format!("X11 server refused connection: {}", reason));
        }

        let info = Self::parse_setup_reply(&extra_data)?;

        stream
            .set_nonblocking(true)
            .map_err(|e| format!("Failed to set nonblocking stream: {}", e))?;

        let mut client = Self {
            stream,
            info,
            next_resource_id: 0,
            mit_shm_opcode: None,
            read_buffer: Vec::with_capacity(4096),
            encoder: X11WireEncoder::new(),
        };

        // Query MIT-SHM extension
        client.mit_shm_opcode = client.query_extension("MIT-SHM").ok().flatten();

        Ok(client)
    }

    /// Parses the variable-length setup reply extra data into `X11ServerInfo`.
    fn parse_setup_reply(data: &[u8]) -> Result<X11ServerInfo, String> {
        if data.len() < 32 {
            return Err("Setup reply payload too short".to_string());
        }

        let mut decoder = X11WireDecoder::new(data);
        let _release = decoder.read_u32().unwrap();
        let resource_id_base = decoder.read_u32().unwrap();
        let resource_id_mask = decoder.read_u32().unwrap();
        let _motion_buffer_size = decoder.read_u32().unwrap();
        let vendor_len = decoder.read_u16().unwrap() as usize;
        let _max_req_len = decoder.read_u16().unwrap();
        let _num_screens = decoder.read_u8().unwrap();
        let num_formats = decoder.read_u8().unwrap() as usize;

        // Skip image formats and keycodes (10 bytes)
        let _ = decoder.read_bytes(10);

        // Skip vendor string padded to 4 bytes
        let vendor_padded = (vendor_len + 3) & !3;
        let _ = decoder.read_bytes(vendor_padded);

        // Skip formats (8 bytes each)
        let formats_len = num_formats * 8;
        let _ = decoder.read_bytes(formats_len);

        // Read Screen 0
        let root_window = decoder
            .read_u32()
            .ok_or_else(|| "Failed to read root window ID".to_string())?;
        let _default_colormap = decoder.read_u32();
        let white_pixel = decoder.read_u32().unwrap_or(0xFFFFFF);
        let black_pixel = decoder.read_u32().unwrap_or(0x000000);
        let _current_input_masks = decoder.read_u32();
        let width_in_pixels = decoder.read_u16().unwrap_or(1920);
        let height_in_pixels = decoder.read_u16().unwrap_or(1080);
        let _width_in_mm = decoder.read_u16();
        let _height_in_mm = decoder.read_u16();
        let _min_maps = decoder.read_u16();
        let _max_maps = decoder.read_u16();
        let root_visual = decoder.read_u32().unwrap_or(0);
        let _backing_stores = decoder.read_u8();
        let _save_unders = decoder.read_u8();
        let root_depth = decoder.read_u8().unwrap_or(24);

        Ok(X11ServerInfo {
            resource_id_base,
            resource_id_mask,
            root_window,
            root_visual,
            root_depth,
            black_pixel,
            white_pixel,
            width_in_pixels,
            height_in_pixels,
        })
    }

    /// Allocates a unique X11 resource ID.
    pub fn allocate_id(&mut self) -> u32 {
        let id = self.info.resource_id_base | (self.next_resource_id & self.info.resource_id_mask);
        self.next_resource_id += 1;
        id
    }

    /// Sends current encoder contents down the socket.
    pub fn flush_encoder(&mut self) -> Result<(), String> {
        let bytes = self.encoder.as_bytes();
        self.stream
            .write_all(bytes)
            .map_err(|e| format!("X11 socket write error: {}", e))?;
        self.encoder.clear();
        Ok(())
    }

    /// Queries the server for an extension by name.
    pub fn query_extension(&mut self, name: &str) -> Result<Option<u8>, String> {
        self.encoder.begin_request(X11_OPCODE_QUERY_EXTENSION, 0);
        self.encoder.push_u16(name.len() as u16);
        self.encoder.push_u16(0); // Pad
        self.encoder.push_padded_string(name);
        self.encoder.finish_request();
        self.flush_encoder()?;

        // Read 32-byte reply
        let mut reply = [0u8; 32];
        let mut bytes_read = 0;
        let start = std::time::Instant::now();
        while bytes_read < 32 {
            if start.elapsed() > Duration::from_millis(500) {
                return Ok(None);
            }
            match self.stream.read(&mut reply[bytes_read..]) {
                Ok(0) => return Err("Server closed socket while querying extension".to_string()),
                Ok(n) => bytes_read += n,
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(1));
                }
                Err(e) => return Err(format!("Error reading extension reply: {}", e)),
            }
        }

        let present = reply[8];
        if present == 1 {
            let major_opcode = reply[9];
            Ok(Some(major_opcode))
        } else {
            Ok(None)
        }
    }

    /// Sends a `CreateWindow` request.
    pub fn create_window(
        &mut self,
        window_id: u32,
        parent: u32,
        width: u16,
        height: u16,
    ) -> Result<(), String> {
        self.encoder.begin_request(X11_OPCODE_CREATE_WINDOW, 0); // depth 0 = copy from parent
        self.encoder.push_u32(window_id);
        self.encoder.push_u32(parent);
        self.encoder.push_i16(0); // x
        self.encoder.push_i16(0); // y
        self.encoder.push_u16(width);
        self.encoder.push_u16(height);
        self.encoder.push_u16(0); // border width
        self.encoder.push_u16(1); // class: InputOutput
        self.encoder.push_u32(0); // visual: CopyFromParent

        // Value mask: CWBackPixel (0x02) | CWEventMask (0x800)
        let value_mask = 0x0000_0802u32;
        self.encoder.push_u32(value_mask);
        self.encoder.push_u32(self.info.black_pixel);

        // Event mask: Exposure | KeyPress | KeyRelease | ButtonPress | ButtonRelease | Enter | Leave | Motion | StructureNotify | FocusChange
        let event_mask = 0x0022_807Fu32;
        self.encoder.push_u32(event_mask);

        self.encoder.finish_request();
        self.flush_encoder()
    }

    /// Sends a `MapWindow` request to display the window on screen.
    pub fn map_window(&mut self, window_id: u32) -> Result<(), String> {
        self.encoder.begin_request(X11_OPCODE_MAP_WINDOW, 0);
        self.encoder.push_u32(window_id);
        self.encoder.finish_request();
        self.flush_encoder()
    }

    /// Sends a `CreateGC` request for software blitting.
    pub fn create_gc(&mut self, gc_id: u32, drawable_id: u32) -> Result<(), String> {
        self.encoder.begin_request(X11_OPCODE_CREATE_GC, 0);
        self.encoder.push_u32(gc_id);
        self.encoder.push_u32(drawable_id);
        self.encoder.push_u32(0); // value mask: none
        self.encoder.finish_request();
        self.flush_encoder()
    }

    /// Sets the window title via `ChangeProperty` (`WM_NAME`).
    pub fn set_wm_name(&mut self, window_id: u32, title: &str) -> Result<(), String> {
        self.encoder.begin_request(X11_OPCODE_CHANGE_PROPERTY, 0); // 0 = Replace mode
        self.encoder.push_u32(window_id);
        self.encoder.push_u32(XA_WM_NAME);
        self.encoder.push_u32(XA_STRING);
        self.encoder.push_u8(8); // format: 8-bit
        self.encoder.push_u8(0); // pad
        self.encoder.push_u16(0); // pad
        self.encoder.push_u32(title.len() as u32);
        self.encoder.push_padded_string(title);
        self.encoder.finish_request();
        self.flush_encoder()
    }

    /// Attaches a shared memory segment with the X server via `MIT-SHM ShmAttach`.
    pub fn shm_attach(
        &mut self,
        shm_major: u8,
        shmseg_id: u32,
        shmid: libc::c_int,
    ) -> Result<(), String> {
        self.encoder.begin_request(shm_major, X11_SHM_OPCODE_ATTACH);
        self.encoder.push_u32(shmseg_id);
        self.encoder.push_u32(shmid as u32);
        self.encoder.push_u32(0); // read_only = false
        self.encoder.finish_request();
        self.flush_encoder()
    }

    /// Dispatches a zero-copy blit from shared memory via `MIT-SHM ShmPutImage`.
    pub fn shm_put_image(
        &mut self,
        shm_major: u8,
        drawable: u32,
        gc: u32,
        shmseg: u32,
        width: u16,
        height: u16,
    ) -> Result<(), String> {
        self.encoder
            .begin_request(shm_major, X11_SHM_OPCODE_PUT_IMAGE);
        self.encoder.push_u32(drawable);
        self.encoder.push_u32(gc);
        self.encoder.push_u16(width);
        self.encoder.push_u16(height);
        self.encoder.push_u16(0); // src_x
        self.encoder.push_u16(0); // src_y
        self.encoder.push_u16(width); // src_w
        self.encoder.push_u16(height); // src_h
        self.encoder.push_i16(0); // dst_x
        self.encoder.push_i16(0); // dst_y
        self.encoder.push_u8(24); // depth
        self.encoder.push_u8(2); // format: 2 = ZPixmap
        self.encoder.push_u8(0); // send_event = 0
        self.encoder.push_u8(0); // bpad
        self.encoder.push_u32(shmseg);
        self.encoder.push_u32(0); // offset
        self.encoder.finish_request();
        self.flush_encoder()
    }

    /// Dispatches a streaming pixel blit via standard `PutImage` (ZPixmap).
    pub fn put_image(
        &mut self,
        drawable: u32,
        gc: u32,
        width: u16,
        height: u16,
        pixels: &[u8],
    ) -> Result<(), String> {
        self.encoder.begin_request(X11_OPCODE_PUT_IMAGE, 2); // 2 = ZPixmap
        self.encoder.push_u32(drawable);
        self.encoder.push_u32(gc);
        self.encoder.push_u16(width);
        self.encoder.push_u16(height);
        self.encoder.push_i16(0); // dst_x
        self.encoder.push_i16(0); // dst_y
        self.encoder.push_u8(0); // left_pad
        self.encoder.push_u8(24); // depth
        self.encoder.push_u16(0); // pad
        self.encoder.push_bytes(pixels);
        self.encoder.finish_request();
        self.flush_encoder()
    }

    /// Queries or registers an interned atom by name.
    pub fn intern_atom(&mut self, name: &str, only_if_exists: bool) -> Result<u32, String> {
        self.encoder
            .begin_request(X11_OPCODE_INTERN_ATOM, if only_if_exists { 1 } else { 0 });
        self.encoder.push_u16(name.len() as u16);
        self.encoder.push_u16(0); // Pad
        self.encoder.push_padded_string(name);
        self.encoder.finish_request();
        self.flush_encoder()?;

        let mut reply = [0u8; 32];
        let mut bytes_read = 0;
        let start = std::time::Instant::now();
        while bytes_read < 32 {
            if start.elapsed() > Duration::from_millis(500) {
                return Err("Timeout reading InternAtom reply".to_string());
            }
            match self.stream.read(&mut reply[bytes_read..]) {
                Ok(0) => return Err("Server closed socket while reading InternAtom".to_string()),
                Ok(n) => bytes_read += n,
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(1));
                }
                Err(e) => return Err(format!("Error reading InternAtom reply: {}", e)),
            }
        }
        if reply[0] != 1 {
            return Err("InternAtom returned error reply".to_string());
        }
        let atom = u32::from_ne_bytes(reply[8..12].try_into().unwrap());
        Ok(atom)
    }

    /// Sends a synthetic 32-bit ClientMessage event to a destination window.
    pub fn send_client_message(
        &mut self,
        dest_window: u32,
        event_mask: u32,
        target_window: u32,
        message_type: u32,
        data: [u32; 5],
    ) -> Result<(), String> {
        self.encoder.begin_request(X11_OPCODE_SEND_EVENT, 0); // propagate = false
        self.encoder.push_u32(dest_window);
        self.encoder.push_u32(event_mask);
        // 32-byte ClientMessage packet
        self.encoder.push_u8(33); // ClientMessage opcode
        self.encoder.push_u8(32); // 32-bit format
        self.encoder.push_u16(0); // Sequence number
        self.encoder.push_u32(target_window);
        self.encoder.push_u32(message_type);
        for &d in &data {
            self.encoder.push_u32(d);
        }
        self.encoder.finish_request();
        self.flush_encoder()
    }
}

/// Pure-Rust X11 native desktop window instance with MIT-SHM support.
pub struct PureX11Window {
    pub client: PureX11Client,
    pub window_id: u32,
    pub gc_id: u32,
    pub config: WindowConfig,
    pub scale_factor: f32,
    pub is_focused: bool,
    pub cursor: CursorIcon,
    pub redraw_requested: bool,
    pub shm: Option<X11ShmSegment>,
    pub event_queue: VecDeque<PlatformEvent>,
    pub last_click: Option<(std::time::Instant, f32, f32)>,
    pub moveresize_atom: Option<u32>,
}

impl PureX11Window {
    /// Creates a native pure-Rust X11 window over the default local display socket.
    pub fn new(config: WindowConfig) -> Result<Self, String> {
        let socket_path = default_x11_socket_path();
        Self::connect_to_socket(&socket_path, config)
    }

    /// Connects to a specific socket path and initializes the window and graphics context.
    pub fn connect_to_socket(socket_path: &str, config: WindowConfig) -> Result<Self, String> {
        let mut client = PureX11Client::connect(socket_path)?;

        let window_id = client.allocate_id();
        let gc_id = client.allocate_id();

        let w = config.width.max(100) as u16;
        let h = config.height.max(100) as u16;

        client.create_window(window_id, client.info.root_window, w, h)?;
        client.set_wm_name(window_id, &config.title)?;
        client.create_gc(gc_id, window_id)?;
        client.map_window(window_id)?;

        let mut window = Self {
            client,
            window_id,
            gc_id,
            config,
            scale_factor: 1.0,
            is_focused: true,
            cursor: CursorIcon::Default,
            redraw_requested: true,
            shm: None,
            event_queue: VecDeque::new(),
            last_click: None,
            moveresize_atom: None,
        };

        window.init_shm_buffer(w as u32, h as u32)?;
        Ok(window)
    }

    /// Dispatches an EWMH `_NET_WM_MOVERESIZE` client message delegating window drag or resize.
    pub fn send_moveresize(
        &mut self,
        x: f32,
        y: f32,
        direction: u32,
        button: u32,
    ) -> Result<(), String> {
        let atom = if let Some(a) = self.moveresize_atom {
            a
        } else {
            let a = self.client.intern_atom("_NET_WM_MOVERESIZE", false)?;
            self.moveresize_atom = Some(a);
            a
        };
        let root = self.client.info.root_window;
        let data = [
            x as u32, y as u32, direction, button, 1, // source indication: normal application
        ];
        self.client
            .send_client_message(root, 0x0018_0000, self.window_id, atom, data)
    }

    /// Dispatches an EWMH `_NET_WM_STATE` client message for window maximize or restore.
    pub fn set_wm_state_maximized(&mut self, maximized: bool) -> Result<(), String> {
        let wm_state = self.client.intern_atom("_NET_WM_STATE", false)?;
        let max_vert = self
            .client
            .intern_atom("_NET_WM_STATE_MAXIMIZED_VERT", false)?;
        let max_horz = self
            .client
            .intern_atom("_NET_WM_STATE_MAXIMIZED_HORZ", false)?;
        let root = self.client.info.root_window;
        let action = if maximized { 1u32 } else { 0u32 };
        let data = [action, max_vert, max_horz, 1, 0];
        self.client
            .send_client_message(root, 0x0018_0000, self.window_id, wm_state, data)
    }

    /// Allocates or resizes the shared memory segment if MIT-SHM is supported.
    fn init_shm_buffer(&mut self, width: u32, height: u32) -> Result<(), String> {
        if let Some(shm_opcode) = self.client.mit_shm_opcode {
            let shmseg_id = self.client.allocate_id();
            let size = (width * height * 4) as usize;
            let shm = X11ShmSegment::new(shmseg_id, size)?;
            self.client.shm_attach(shm_opcode, shmseg_id, shm.shmid)?;
            self.shm = Some(shm);
        }
        Ok(())
    }

    /// Presents a BGRA8 pixel buffer directly via MIT-SHM (or streaming PutImage fallback).
    pub fn present_bgra(
        &mut self,
        pixels: &[u8],
        width: u32,
        height: u32,
    ) -> Result<(), PresentError> {
        if let Some(ref mut shm) = self.shm {
            if shm.size < (width * height * 4) as usize {
                let _ = self.init_shm_buffer(width, height);
            }
        }

        if let (Some(shm_opcode), Some(ref mut shm)) =
            (self.client.mit_shm_opcode, self.shm.as_mut())
        {
            shm.write_pixels(pixels);
            self.client
                .shm_put_image(
                    shm_opcode,
                    self.window_id,
                    self.gc_id,
                    shm.shmseg_id,
                    width as u16,
                    height as u16,
                )
                .map_err(PresentError::Custom)?;
        } else {
            self.client
                .put_image(
                    self.window_id,
                    self.gc_id,
                    width as u16,
                    height as u16,
                    pixels,
                )
                .map_err(PresentError::Custom)?;
        }

        self.redraw_requested = false;
        Ok(())
    }

    /// Reads pending 32-byte event packets from the socket and translates them into PlatformEvents.
    pub fn poll_native_events(&mut self) -> Vec<PlatformEvent> {
        let mut chunk = [0u8; 4096];
        loop {
            match self.client.stream.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => {
                    self.client.read_buffer.extend_from_slice(&chunk[..n]);
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(_) => break,
            }
        }

        // Process 32-byte event packets
        while self.client.read_buffer.len() >= 32 {
            let event_bytes = self.client.read_buffer.drain(..32).collect::<Vec<u8>>();
            self.handle_32_byte_event(&event_bytes);
        }

        let mut events = Vec::new();
        while let Some(ev) = self.event_queue.pop_front() {
            events.push(ev);
        }
        events
    }

    /// Decodes a single 32-byte X11 event packet into platform events.
    pub fn handle_32_byte_event(&mut self, pkt: &[u8]) {
        let code = pkt[0] & 0x7F;
        match code {
            // Expose (12)
            12 => {
                self.redraw_requested = true;
            }
            // ConfigureNotify (22)
            22 => {
                if pkt.len() >= 24 {
                    let w = u16::from_ne_bytes(pkt[20..22].try_into().unwrap()) as u32;
                    let h = u16::from_ne_bytes(pkt[22..24].try_into().unwrap()) as u32;
                    if w != self.config.width || h != self.config.height {
                        self.config.width = w;
                        self.config.height = h;
                        self.redraw_requested = true;
                        self.event_queue.push_back(PlatformEvent::WindowResized {
                            width: w,
                            height: h,
                        });
                    }
                }
            }
            // ButtonPress (4)
            4 => {
                let button_num = pkt[1];
                let x = i16::from_ne_bytes(pkt[24..26].try_into().unwrap()) as f32;
                let y = i16::from_ne_bytes(pkt[26..28].try_into().unwrap()) as f32;
                match button_num {
                    1 => {
                        let now = std::time::Instant::now();
                        if let Some((prev_time, px, py)) = self.last_click {
                            if now.duration_since(prev_time).as_millis() < 400
                                && (px - x).abs() < 6.0
                                && (py - y).abs() < 6.0
                            {
                                self.event_queue.push_back(PlatformEvent::DoubleClick {
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
                        self.event_queue.push_back(PlatformEvent::MouseDown {
                            button: MouseButton::Left,
                            x,
                            y,
                        });
                    }
                    2 => self.event_queue.push_back(PlatformEvent::MouseDown {
                        button: MouseButton::Middle,
                        x,
                        y,
                    }),
                    3 => self.event_queue.push_back(PlatformEvent::MouseDown {
                        button: MouseButton::Right,
                        x,
                        y,
                    }),
                    4 => self.event_queue.push_back(PlatformEvent::Scroll {
                        delta_x: 0.0,
                        delta_y: -1.0,
                        is_momentum: false,
                    }),
                    5 => self.event_queue.push_back(PlatformEvent::Scroll {
                        delta_x: 0.0,
                        delta_y: 1.0,
                        is_momentum: false,
                    }),
                    6 => self.event_queue.push_back(PlatformEvent::Scroll {
                        delta_x: -1.0,
                        delta_y: 0.0,
                        is_momentum: false,
                    }),
                    7 => self.event_queue.push_back(PlatformEvent::Scroll {
                        delta_x: 1.0,
                        delta_y: 0.0,
                        is_momentum: false,
                    }),
                    b => self.event_queue.push_back(PlatformEvent::MouseDown {
                        button: MouseButton::Other(b as u16),
                        x,
                        y,
                    }),
                }
            }
            // ButtonRelease (5)
            5 => {
                let button_num = pkt[1];
                let x = i16::from_ne_bytes(pkt[24..26].try_into().unwrap()) as f32;
                let y = i16::from_ne_bytes(pkt[26..28].try_into().unwrap()) as f32;
                let button = match button_num {
                    1 => MouseButton::Left,
                    2 => MouseButton::Middle,
                    3 => MouseButton::Right,
                    b => MouseButton::Other(b as u16),
                };
                self.event_queue
                    .push_back(PlatformEvent::MouseUp { button, x, y });
            }
            // MotionNotify (6)
            6 => {
                let x = i16::from_ne_bytes(pkt[24..26].try_into().unwrap()) as f32;
                let y = i16::from_ne_bytes(pkt[26..28].try_into().unwrap()) as f32;
                self.event_queue
                    .push_back(PlatformEvent::MouseMove { x, y });
            }
            // EnterNotify (7)
            7 => {
                self.is_focused = true;
                self.event_queue
                    .push_back(PlatformEvent::WindowFocused(true));
            }
            // LeaveNotify (8)
            8 => {
                self.is_focused = false;
                self.event_queue.push_back(PlatformEvent::CursorLeft);
                self.event_queue
                    .push_back(PlatformEvent::WindowFocused(false));
            }
            // KeyPress (2)
            2 => {
                let keycode = pkt[1];
                let (key, ch) = map_x11_pure_keycode(keycode);
                self.event_queue.push_back(PlatformEvent::KeyDown {
                    key,
                    modifiers: Modifiers::default(),
                });
                if let Some(c) = ch {
                    self.event_queue
                        .push_back(PlatformEvent::ReceivedCharacter(c));
                }
            }
            // KeyRelease (3)
            3 => {
                let keycode = pkt[1];
                let (key, _) = map_x11_pure_keycode(keycode);
                self.event_queue.push_back(PlatformEvent::KeyUp {
                    key,
                    modifiers: Modifiers::default(),
                });
            }
            // ClientMessage (33)
            33 => {
                self.event_queue
                    .push_back(PlatformEvent::WindowCloseRequested);
            }
            _ => {}
        }
    }
}

impl PlatformWindow for PureX11Window {
    fn title(&self) -> &str {
        &self.config.title
    }

    fn set_title(&mut self, title: &str) {
        self.config.title = title.to_string();
        let _ = self.client.set_wm_name(self.window_id, title);
    }

    fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    fn set_size(&mut self, width: u32, height: u32) {
        self.config.width = width;
        self.config.height = height;
        let _ = self.init_shm_buffer(width, height);
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
        let prev = self.config.mode;
        self.config.mode = mode;
        if prev != mode {
            self.redraw_requested = true;
            let _ = self.set_wm_state_maximized(mode == WindowMode::Maximized);
            if mode == WindowMode::Maximized {
                self.push_event(PlatformEvent::WindowMaximized(true));
            } else if prev == WindowMode::Maximized {
                self.push_event(PlatformEvent::WindowMaximized(false));
            }
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
        self.poll_native_events()
    }

    fn push_event(&mut self, event: PlatformEvent) {
        self.event_queue.push_back(event);
    }

    fn begin_window_drag(&mut self, x: f32, y: f32) {
        let _ = self.send_moveresize(x, y, 8, 1);
    }

    fn begin_window_resize(&mut self, edge: crate::csd::CsdHitZone, x: f32, y: f32) {
        if let Some(dir) = edge.to_x11_moveresize_direction() {
            let _ = self.send_moveresize(x, y, dir, 1);
        }
    }
}

impl NativeSurfaceWindow for PureX11Window {
    fn present_surface(
        &mut self,
        pixels: &[u8],
        width: u32,
        height: u32,
    ) -> Result<(), PresentError> {
        self.present_bgra(pixels, width, height)
    }
}

#[cfg(feature = "xlib")]
/// Inner xlib-backed window managing legacy XOpenDisplay handle when active.
pub struct XlibWindowInner {
    pub display: *mut xlib::Display,
    pub window: xlib::Window,
    pub gc: xlib::GC,
    pub wm_delete_window: xlib::Atom,
    pub config: WindowConfig,
    pub scale_factor: f32,
    pub is_focused: bool,
    pub cursor: CursorIcon,
    pub redraw_requested: bool,
    pub event_queue: VecDeque<PlatformEvent>,
    pub last_click: Option<(std::time::Instant, f32, f32)>,
}

#[cfg(feature = "xlib")]
impl XlibWindowInner {
    pub fn new(config: WindowConfig) -> Result<Self, String> {
        unsafe {
            let display = xlib::XOpenDisplay(std::ptr::null());
            if display.is_null() {
                return Err("Failed to open X11 display via xlib".to_string());
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
                return Err("Failed to create X11 window via xlib".to_string());
            }

            let c_title = CString::new(config.title.as_str())
                .unwrap_or_else(|_| CString::new("Strata Application").unwrap());
            xlib::XStoreName(display, window, c_title.as_ptr());

            let wm_delete_str = CString::new("WM_DELETE_WINDOW").unwrap();
            let wm_delete_window = xlib::XInternAtom(display, wm_delete_str.as_ptr(), 0);
            let mut protocols = [wm_delete_window];
            xlib::XSetWMProtocols(display, window, protocols.as_mut_ptr(), 1);

            let event_mask = xlib::ExposureMask
                | xlib::KeyPressMask
                | xlib::KeyReleaseMask
                | xlib::ButtonPressMask
                | xlib::ButtonReleaseMask
                | xlib::PointerMotionMask
                | xlib::StructureNotifyMask
                | xlib::FocusChangeMask;
            xlib::XSelectInput(display, window, event_mask);

            let gc = xlib::XCreateGC(display, window, 0, std::ptr::null_mut());
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
            (*image).data = std::ptr::null_mut();
            xlib::XDestroyImage(image);
        }

        self.redraw_requested = false;
        Ok(())
    }

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
                            2 => events.push(PlatformEvent::MouseDown {
                                button: MouseButton::Middle,
                                x,
                                y,
                            }),
                            3 => events.push(PlatformEvent::MouseDown {
                                button: MouseButton::Right,
                                x,
                                y,
                            }),
                            4 => events.push(PlatformEvent::Scroll {
                                delta_x: 0.0,
                                delta_y: -1.0,
                                is_momentum: false,
                            }),
                            5 => events.push(PlatformEvent::Scroll {
                                delta_x: 0.0,
                                delta_y: 1.0,
                                is_momentum: false,
                            }),
                            6 => events.push(PlatformEvent::Scroll {
                                delta_x: -1.0,
                                delta_y: 0.0,
                                is_momentum: false,
                            }),
                            7 => events.push(PlatformEvent::Scroll {
                                delta_x: 1.0,
                                delta_y: 0.0,
                                is_momentum: false,
                            }),
                            b => events.push(PlatformEvent::MouseDown {
                                button: MouseButton::Other(b as u16),
                                x,
                                y,
                            }),
                        }
                    }
                    xlib::ButtonRelease => {
                        let bp = xevent.button;
                        let x = bp.x as f32;
                        let y = bp.y as f32;
                        let button = match bp.button {
                            1 => MouseButton::Left,
                            2 => MouseButton::Middle,
                            3 => MouseButton::Right,
                            b => MouseButton::Other(b as u16),
                        };
                        events.push(PlatformEvent::MouseUp { button, x, y });
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

#[cfg(feature = "xlib")]
impl Drop for XlibWindowInner {
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

/// Unified Linux X11 desktop window instance.
/// Transparently uses `PureX11Window` by default, falling back to legacy `xlib` if requested and available.
pub enum X11Window {
    Pure(PureX11Window),
    #[cfg(feature = "xlib")]
    Xlib(XlibWindowInner),
}

impl X11Window {
    /// Instantiates an X11 window according to the provided configuration.
    pub fn new(config: WindowConfig) -> Result<Self, String> {
        // 1. Try pure-Rust X11 client first
        match PureX11Window::new(config.clone()) {
            Ok(pure) => Ok(Self::Pure(pure)),
            Err(pure_err) => {
                #[cfg(feature = "xlib")]
                {
                    match XlibWindowInner::new(config) {
                        Ok(xlib_win) => Ok(Self::Xlib(xlib_win)),
                        Err(xlib_err) => Err(format!(
                            "X11 pure client error: {}; xlib fallback error: {}",
                            pure_err, xlib_err
                        )),
                    }
                }
                #[cfg(not(feature = "xlib"))]
                {
                    Err(format!("Pure X11 client error: {}", pure_err))
                }
            }
        }
    }

    /// Returns true if the window is driven by pure-Rust wire protocols without C xlib.
    pub fn is_pure(&self) -> bool {
        matches!(self, Self::Pure(_))
    }

    /// Presents a BGRA8 pixel slice directly to the X11 surface.
    pub fn present_bgra(
        &mut self,
        buffer: &[u8],
        width: u32,
        height: u32,
    ) -> Result<(), PresentError> {
        match self {
            Self::Pure(w) => w.present_bgra(buffer, width, height),
            #[cfg(feature = "xlib")]
            Self::Xlib(w) => w.present_bgra(buffer, width, height),
        }
    }

    /// Polls pending platform events without blocking.
    pub fn poll_native_events(&mut self) -> Vec<PlatformEvent> {
        match self {
            Self::Pure(w) => w.poll_native_events(),
            #[cfg(feature = "xlib")]
            Self::Xlib(w) => {
                let mut events = w.poll_native_events();
                while let Some(ev) = w.event_queue.pop_front() {
                    events.push(ev);
                }
                events
            }
        }
    }
}

impl PlatformWindow for X11Window {
    fn title(&self) -> &str {
        match self {
            Self::Pure(w) => w.title(),
            #[cfg(feature = "xlib")]
            Self::Xlib(w) => &w.config.title,
        }
    }

    fn set_title(&mut self, title: &str) {
        match self {
            Self::Pure(w) => w.set_title(title),
            #[cfg(feature = "xlib")]
            Self::Xlib(w) => {
                w.config.title = title.to_string();
                if !w.display.is_null() && w.window != 0 {
                    if let Ok(c_title) = CString::new(title) {
                        unsafe {
                            xlib::XStoreName(w.display, w.window, c_title.as_ptr());
                            xlib::XFlush(w.display);
                        }
                    }
                }
            }
        }
    }

    fn size(&self) -> (u32, u32) {
        match self {
            Self::Pure(w) => w.size(),
            #[cfg(feature = "xlib")]
            Self::Xlib(w) => (w.config.width, w.config.height),
        }
    }

    fn set_size(&mut self, width: u32, height: u32) {
        match self {
            Self::Pure(w) => w.set_size(width, height),
            #[cfg(feature = "xlib")]
            Self::Xlib(w) => {
                w.config.width = width;
                w.config.height = height;
                if !w.display.is_null() && w.window != 0 {
                    unsafe {
                        xlib::XResizeWindow(w.display, w.window, width, height);
                        xlib::XFlush(w.display);
                    }
                }
                w.redraw_requested = true;
            }
        }
    }

    fn scale_factor(&self) -> f32 {
        match self {
            Self::Pure(w) => w.scale_factor(),
            #[cfg(feature = "xlib")]
            Self::Xlib(w) => w.scale_factor,
        }
    }

    fn set_scale_factor(&mut self, scale_factor: f32) {
        match self {
            Self::Pure(w) => w.set_scale_factor(scale_factor),
            #[cfg(feature = "xlib")]
            Self::Xlib(w) => {
                w.scale_factor = scale_factor;
                w.redraw_requested = true;
            }
        }
    }

    fn mode(&self) -> WindowMode {
        match self {
            Self::Pure(w) => w.mode(),
            #[cfg(feature = "xlib")]
            Self::Xlib(w) => w.config.mode,
        }
    }

    fn set_mode(&mut self, mode: WindowMode) {
        match self {
            Self::Pure(w) => w.set_mode(mode),
            #[cfg(feature = "xlib")]
            Self::Xlib(w) => {
                let prev = w.config.mode;
                w.config.mode = mode;
                w.redraw_requested = true;
                if mode == WindowMode::Maximized && prev != WindowMode::Maximized {
                    w.event_queue
                        .push_back(PlatformEvent::WindowMaximized(true));
                } else if mode != WindowMode::Maximized && prev == WindowMode::Maximized {
                    w.event_queue
                        .push_back(PlatformEvent::WindowMaximized(false));
                }
            }
        }
    }

    fn is_focused(&self) -> bool {
        match self {
            Self::Pure(w) => w.is_focused(),
            #[cfg(feature = "xlib")]
            Self::Xlib(w) => w.is_focused,
        }
    }

    fn set_focused(&mut self, focused: bool) {
        match self {
            Self::Pure(w) => w.set_focused(focused),
            #[cfg(feature = "xlib")]
            Self::Xlib(w) => w.is_focused = focused,
        }
    }

    fn cursor(&self) -> CursorIcon {
        match self {
            Self::Pure(w) => w.cursor(),
            #[cfg(feature = "xlib")]
            Self::Xlib(w) => w.cursor,
        }
    }

    fn set_cursor(&mut self, cursor: CursorIcon) {
        match self {
            Self::Pure(w) => w.set_cursor(cursor),
            #[cfg(feature = "xlib")]
            Self::Xlib(w) => w.cursor = cursor,
        }
    }

    fn request_redraw(&mut self) {
        match self {
            Self::Pure(w) => w.request_redraw(),
            #[cfg(feature = "xlib")]
            Self::Xlib(w) => w.redraw_requested = true,
        }
    }

    fn is_redraw_requested(&self) -> bool {
        match self {
            Self::Pure(w) => w.is_redraw_requested(),
            #[cfg(feature = "xlib")]
            Self::Xlib(w) => w.redraw_requested,
        }
    }

    fn poll_events(&mut self) -> Vec<PlatformEvent> {
        self.poll_native_events()
    }

    fn push_event(&mut self, event: PlatformEvent) {
        match self {
            Self::Pure(w) => w.push_event(event),
            #[cfg(feature = "xlib")]
            Self::Xlib(w) => w.event_queue.push_back(event),
        }
    }

    fn begin_window_drag(&mut self, x: f32, y: f32) {
        match self {
            Self::Pure(w) => w.begin_window_drag(x, y),
            #[cfg(feature = "xlib")]
            Self::Xlib(_) => {}
        }
    }

    fn begin_window_resize(&mut self, edge: crate::csd::CsdHitZone, x: f32, y: f32) {
        match self {
            Self::Pure(w) => w.begin_window_resize(edge, x, y),
            #[cfg(feature = "xlib")]
            Self::Xlib(_) => {}
        }
    }
}

impl NativeSurfaceWindow for X11Window {
    fn present_surface(
        &mut self,
        pixels: &[u8],
        width: u32,
        height: u32,
    ) -> Result<(), PresentError> {
        self.present_bgra(pixels, width, height)
    }
}

/// Linux X11 Event Loop driving message dispatch and frame pacing.
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

/// Translates Linux evdev hardware keycodes from pure-Rust X11 events into Strata `KeyCode` and characters.
pub fn map_x11_pure_keycode(keycode: u8) -> (KeyCode, Option<char>) {
    let evdev = keycode.saturating_sub(8);
    match evdev {
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
        12 => (KeyCode::Minus, Some('-')),
        13 => (KeyCode::Equals, Some('=')),
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
        26 => (KeyCode::BracketLeft, Some('[')),
        27 => (KeyCode::BracketRight, Some(']')),
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
        39 => (KeyCode::Semicolon, Some(';')),
        40 => (KeyCode::Quote, Some('\'')),
        41 => (KeyCode::Backquote, Some('`')),
        43 => (KeyCode::Backslash, Some('\\')),
        44 => (KeyCode::KeyZ, Some('z')),
        45 => (KeyCode::KeyX, Some('x')),
        46 => (KeyCode::KeyC, Some('c')),
        47 => (KeyCode::KeyV, Some('v')),
        48 => (KeyCode::KeyB, Some('b')),
        49 => (KeyCode::KeyN, Some('n')),
        50 => (KeyCode::KeyM, Some('m')),
        51 => (KeyCode::Comma, Some(',')),
        52 => (KeyCode::Period, Some('.')),
        53 => (KeyCode::Slash, Some('/')),
        57 => (KeyCode::Space, Some(' ')),
        103 => (KeyCode::ArrowUp, None),
        105 => (KeyCode::ArrowLeft, None),
        106 => (KeyCode::ArrowRight, None),
        108 => (KeyCode::ArrowDown, None),
        111 => (KeyCode::Delete, None),
        _ => (KeyCode::Unknown, None),
    }
}

#[cfg(feature = "xlib")]
pub fn map_x11_keysym(keysym: xlib::KeySym) -> KeyCode {
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
        0x002F | 0x003F => KeyCode::Slash,
        0x002C | 0x003C => KeyCode::Comma,
        0x002E | 0x003E => KeyCode::Period,
        0x002D | 0x005F => KeyCode::Minus,
        0x003D | 0x002B => KeyCode::Equals,
        0x005B | 0x007B => KeyCode::BracketLeft,
        0x005D | 0x007D => KeyCode::BracketRight,
        0x003B | 0x003A => KeyCode::Semicolon,
        0x0027 | 0x0022 => KeyCode::Quote,
        0x005C | 0x007C => KeyCode::Backslash,
        0x0060 | 0x007E => KeyCode::Backquote,
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
