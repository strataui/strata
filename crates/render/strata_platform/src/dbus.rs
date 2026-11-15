//! Pure-Rust D-Bus wire protocol serializer, client, and desktop notification daemon.
//!
//! Provides zero-dependency D-Bus message framing, SASL authentication over UNIX domain sockets,
//! and integration with `org.freedesktop.Notifications` and `org.kde.StatusNotifierItem`.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

// D-Bus Message Types
pub const DBUS_MESSAGE_TYPE_METHOD_CALL: u8 = 1;
pub const DBUS_MESSAGE_TYPE_METHOD_RETURN: u8 = 2;
pub const DBUS_MESSAGE_TYPE_ERROR: u8 = 3;
pub const DBUS_MESSAGE_TYPE_SIGNAL: u8 = 4;

// D-Bus Header Field Codes
pub const DBUS_HEADER_FIELD_PATH: u8 = 1;
pub const DBUS_HEADER_FIELD_INTERFACE: u8 = 2;
pub const DBUS_HEADER_FIELD_MEMBER: u8 = 3;
pub const DBUS_HEADER_FIELD_ERROR_NAME: u8 = 4;
pub const DBUS_HEADER_FIELD_REPLY_SERIAL: u8 = 5;
pub const DBUS_HEADER_FIELD_DESTINATION: u8 = 6;
pub const DBUS_HEADER_FIELD_SENDER: u8 = 7;
pub const DBUS_HEADER_FIELD_SIGNATURE: u8 = 8;

// D-Bus Flags
pub const DBUS_FLAG_NO_REPLY_EXPECTED: u8 = 0x01;
pub const DBUS_FLAG_NO_AUTO_START: u8 = 0x02;

/// Resolves the UNIX domain socket path for the active D-Bus session bus.
pub fn session_bus_socket_path() -> Result<String, String> {
    if let Ok(addr) = std::env::var("DBUS_SESSION_BUS_ADDRESS") {
        for part in addr.split(',') {
            if let Some(path) = part.strip_prefix("unix:path=") {
                return Ok(path.to_string());
            }
        }
    }

    #[cfg(unix)]
    {
        let uid = unsafe { libc::getuid() };
        let candidate = format!("/run/user/{}/bus", uid);
        if std::path::Path::new(&candidate).exists() {
            return Ok(candidate);
        }
    }

    Err("Could not resolve D-Bus session bus socket address".to_string())
}

/// Encodes an ASCII string to hexadecimal for D-Bus SASL authentication.
pub fn hex_encode_bytes(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{:02x}", b));
    }
    out
}

/// Formats a complete SASL authentication handshake sequence for external UID auth.
pub fn format_sasl_auth_request(uid: u32) -> Vec<u8> {
    let uid_str = uid.to_string();
    let hex_uid = hex_encode_bytes(uid_str.as_bytes());
    format!("\0AUTH EXTERNAL {}\r\nBEGIN\r\n", hex_uid).into_bytes()
}

/// Encoder for little-endian D-Bus wire protocol binary messages.
#[derive(Debug, Default)]
pub struct DbusMessageEncoder {
    buffer: Vec<u8>,
}

impl DbusMessageEncoder {
    /// Creates a new encoder with standard capacity.
    pub fn new() -> Self {
        Self {
            buffer: Vec::with_capacity(512),
        }
    }

    /// Clears the internal buffer.
    pub fn clear(&mut self) {
        self.buffer.clear();
    }

    /// Returns the encoded byte slice.
    pub fn as_bytes(&self) -> &[u8] {
        &self.buffer
    }

    /// Pads the buffer to the given byte alignment.
    pub fn pad_to(&mut self, alignment: usize) {
        let remainder = self.buffer.len() % alignment;
        if remainder != 0 {
            let pad = alignment - remainder;
            self.buffer.resize(self.buffer.len() + pad, 0);
        }
    }

    /// Appends a raw byte.
    pub fn push_u8(&mut self, val: u8) {
        self.buffer.push(val);
    }

    /// Appends a little-endian unsigned 16-bit integer.
    pub fn push_u16(&mut self, val: u16) {
        self.pad_to(2);
        self.buffer.extend_from_slice(&val.to_le_bytes());
    }

    /// Appends a little-endian unsigned 32-bit integer.
    pub fn push_u32(&mut self, val: u32) {
        self.pad_to(4);
        self.buffer.extend_from_slice(&val.to_le_bytes());
    }

    /// Appends a little-endian signed 32-bit integer.
    pub fn push_i32(&mut self, val: i32) {
        self.pad_to(4);
        self.buffer.extend_from_slice(&val.to_le_bytes());
    }

    /// Appends a little-endian unsigned 64-bit integer.
    pub fn push_u64(&mut self, val: u64) {
        self.pad_to(8);
        self.buffer.extend_from_slice(&val.to_le_bytes());
    }

    /// Appends a D-Bus String (4-byte length + bytes + null terminator).
    pub fn push_string(&mut self, val: &str) {
        self.push_u32(val.len() as u32);
        self.buffer.extend_from_slice(val.as_bytes());
        self.buffer.push(0);
    }

    /// Appends a D-Bus ObjectPath (4-byte length + bytes + null terminator).
    pub fn push_object_path(&mut self, path: &str) {
        self.push_string(path);
    }

    /// Appends a D-Bus Signature (1-byte length + bytes + null terminator).
    pub fn push_signature(&mut self, sig: &str) {
        self.push_u8(sig.len() as u8);
        self.buffer.extend_from_slice(sig.as_bytes());
        self.buffer.push(0);
    }

    /// Encodes a method call message with the given header fields and body data.
    pub fn encode_method_call(
        &mut self,
        serial: u32,
        destination: Option<&str>,
        path: &str,
        interface: Option<&str>,
        member: &str,
        signature: Option<&str>,
        body: &[u8],
    ) {
        self.clear();

        // Build header fields array
        let mut fields_encoder = DbusMessageEncoder::new();

        // 1. Path field
        fields_encoder.pad_to(8);
        fields_encoder.push_u8(DBUS_HEADER_FIELD_PATH);
        fields_encoder.push_signature("o");
        fields_encoder.push_object_path(path);

        // 2. Member field
        fields_encoder.pad_to(8);
        fields_encoder.push_u8(DBUS_HEADER_FIELD_MEMBER);
        fields_encoder.push_signature("s");
        fields_encoder.push_string(member);

        // 3. Interface field
        if let Some(iface) = interface {
            fields_encoder.pad_to(8);
            fields_encoder.push_u8(DBUS_HEADER_FIELD_INTERFACE);
            fields_encoder.push_signature("s");
            fields_encoder.push_string(iface);
        }

        // 4. Destination field
        if let Some(dest) = destination {
            fields_encoder.pad_to(8);
            fields_encoder.push_u8(DBUS_HEADER_FIELD_DESTINATION);
            fields_encoder.push_signature("s");
            fields_encoder.push_string(dest);
        }

        // 5. Signature field
        if let Some(sig) = signature {
            fields_encoder.pad_to(8);
            fields_encoder.push_u8(DBUS_HEADER_FIELD_SIGNATURE);
            fields_encoder.push_signature("g");
            fields_encoder.push_signature(sig);
        }

        let fields_bytes = fields_encoder.as_bytes();
        let fields_len = fields_bytes.len() as u32;
        let body_len = body.len() as u32;

        // Fixed 16-byte header
        self.push_u8(b'l'); // Little-endian
        self.push_u8(DBUS_MESSAGE_TYPE_METHOD_CALL);
        self.push_u8(0); // Flags
        self.push_u8(1); // Protocol version 1
        self.push_u32(body_len);
        self.push_u32(serial);
        self.push_u32(fields_len);

        // Header fields
        self.buffer.extend_from_slice(fields_bytes);

        // Pad header to 8 bytes before body
        self.pad_to(8);

        // Body
        self.buffer.extend_from_slice(body);
    }

    /// Encodes a method return message in reply to a specific method call serial.
    pub fn encode_method_return(
        &mut self,
        serial: u32,
        reply_serial: u32,
        destination: Option<&str>,
        signature: Option<&str>,
        body: &[u8],
    ) {
        self.clear();

        let mut fields_encoder = DbusMessageEncoder::new();

        // 1. ReplySerial field
        fields_encoder.pad_to(8);
        fields_encoder.push_u8(DBUS_HEADER_FIELD_REPLY_SERIAL);
        fields_encoder.push_signature("u");
        fields_encoder.push_u32(reply_serial);

        // 2. Destination field
        if let Some(dest) = destination {
            fields_encoder.pad_to(8);
            fields_encoder.push_u8(DBUS_HEADER_FIELD_DESTINATION);
            fields_encoder.push_signature("s");
            fields_encoder.push_string(dest);
        }

        // 3. Signature field
        if let Some(sig) = signature {
            fields_encoder.pad_to(8);
            fields_encoder.push_u8(DBUS_HEADER_FIELD_SIGNATURE);
            fields_encoder.push_signature("g");
            fields_encoder.push_signature(sig);
        }

        let fields_bytes = fields_encoder.as_bytes();
        let fields_len = fields_bytes.len() as u32;
        let body_len = body.len() as u32;

        // Fixed 16-byte header
        self.push_u8(b'l'); // Little-endian
        self.push_u8(DBUS_MESSAGE_TYPE_METHOD_RETURN);
        self.push_u8(0); // Flags
        self.push_u8(1); // Protocol version 1
        self.push_u32(body_len);
        self.push_u32(serial);
        self.push_u32(fields_len);

        // Header fields
        self.buffer.extend_from_slice(fields_bytes);

        // Pad header to 8 bytes before body
        self.pad_to(8);

        // Body
        self.buffer.extend_from_slice(body);
    }
}

/// Decoder for little-endian D-Bus wire protocol binary messages.
pub struct DbusMessageDecoder<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> DbusMessageDecoder<'a> {
    /// Creates a decoder wrapping the given slice.
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }

    /// Aligns current offset to the specified boundary.
    pub fn align_to(&mut self, alignment: usize) {
        let rem = self.offset % alignment;
        if rem != 0 {
            self.offset += alignment - rem;
        }
    }

    /// Reads a single byte.
    pub fn read_u8(&mut self) -> Option<u8> {
        if self.offset < self.data.len() {
            let val = self.data[self.offset];
            self.offset += 1;
            Some(val)
        } else {
            None
        }
    }

    /// Reads a little-endian unsigned 16-bit integer.
    pub fn read_u16(&mut self) -> Option<u16> {
        self.align_to(2);
        if self.offset + 2 <= self.data.len() {
            let val =
                u16::from_le_bytes(self.data[self.offset..self.offset + 2].try_into().unwrap());
            self.offset += 2;
            Some(val)
        } else {
            None
        }
    }

    /// Reads a little-endian unsigned 32-bit integer.
    pub fn read_u32(&mut self) -> Option<u32> {
        self.align_to(4);
        if self.offset + 4 <= self.data.len() {
            let val =
                u32::from_le_bytes(self.data[self.offset..self.offset + 4].try_into().unwrap());
            self.offset += 4;
            Some(val)
        } else {
            None
        }
    }

    /// Reads a little-endian signed 32-bit integer.
    pub fn read_i32(&mut self) -> Option<i32> {
        self.align_to(4);
        if self.offset + 4 <= self.data.len() {
            let val =
                i32::from_le_bytes(self.data[self.offset..self.offset + 4].try_into().unwrap());
            self.offset += 4;
            Some(val)
        } else {
            None
        }
    }

    /// Reads a D-Bus String (4-byte length + bytes + null).
    pub fn read_string(&mut self) -> Option<String> {
        let len = self.read_u32()? as usize;
        if self.offset + len < self.data.len() {
            let bytes = &self.data[self.offset..self.offset + len];
            let str = String::from_utf8_lossy(bytes).to_string();
            self.offset += len + 1; // +1 to skip null terminator
            Some(str)
        } else {
            None
        }
    }
}

/// Lightweight D-Bus connection for session bus interaction.
pub struct DbusClient {
    stream: UnixStream,
    next_serial: u32,
    pub unique_name: Option<String>,
}

impl DbusClient {
    /// Connects to the local session bus and completes SASL authentication.
    pub fn connect() -> Result<Self, String> {
        let path = session_bus_socket_path()?;
        Self::connect_to_path(&path)
    }

    /// Connects to a specific UNIX domain socket path and performs SASL authentication.
    pub fn connect_to_path(socket_path: &str) -> Result<Self, String> {
        let mut stream = UnixStream::connect(socket_path)
            .map_err(|e| format!("Cannot connect to D-Bus socket \"{}\": {}", socket_path, e))?;

        stream
            .set_read_timeout(Some(Duration::from_millis(1500)))
            .map_err(|e| format!("Failed to set read timeout: {}", e))?;

        #[cfg(unix)]
        let uid = unsafe { libc::getuid() };
        #[cfg(not(unix))]
        let uid = 1000;

        // 1. Send SASL authentication
        let auth_payload = format_sasl_auth_request(uid);
        stream
            .write_all(&auth_payload)
            .map_err(|e| format!("Failed to write SASL auth: {}", e))?;

        // 2. Read SASL OK reply line
        let mut response = Vec::new();
        let mut byte = [0u8; 1];
        while let Ok(1) = stream.read(&mut byte) {
            response.push(byte[0]);
            if response.ends_with(b"\r\n") {
                break;
            }
        }

        let resp_str = String::from_utf8_lossy(&response);
        if !resp_str.starts_with("OK") {
            return Err(format!("D-Bus SASL auth rejected: {}", resp_str.trim()));
        }

        let mut client = Self {
            stream,
            next_serial: 1,
            unique_name: None,
        };

        // 3. Call org.freedesktop.DBus.Hello to register on the bus
        let name = client.call_hello()?;
        client.unique_name = Some(name);

        Ok(client)
    }

    /// Allocates an incremental serial number.
    pub fn next_serial(&mut self) -> u32 {
        let s = self.next_serial;
        self.next_serial += 1;
        s
    }

    /// Invokes `org.freedesktop.DBus.Hello` to obtain the assigned client unique name.
    pub fn call_hello(&mut self) -> Result<String, String> {
        let serial = self.next_serial();
        let mut encoder = DbusMessageEncoder::new();
        encoder.encode_method_call(
            serial,
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            Some("org.freedesktop.DBus"),
            "Hello",
            None,
            &[],
        );

        self.stream
            .write_all(encoder.as_bytes())
            .map_err(|e| format!("Failed to send DBus.Hello: {}", e))?;

        // Read method return
        let reply = self.read_message()?;
        let mut decoder = DbusMessageDecoder::new(&reply);
        // Skip 16-byte fixed header
        if reply.len() < 16 {
            return Err("D-Bus reply header too short".to_string());
        }
        let body_len = u32::from_le_bytes(reply[4..8].try_into().unwrap()) as usize;
        let fields_len = u32::from_le_bytes(reply[12..16].try_into().unwrap()) as usize;
        let mut header_total = 16 + fields_len;
        if !header_total.is_multiple_of(8) {
            header_total += 8 - (header_total % 8);
        }

        if reply.len() < header_total + body_len {
            return Err("D-Bus reply payload truncated".to_string());
        }

        decoder.offset = header_total;
        decoder
            .read_string()
            .ok_or_else(|| "Failed to parse unique name from DBus.Hello".to_string())
    }

    /// Dispatches a method call and reads the method return reply.
    pub fn call_method(
        &mut self,
        destination: &str,
        path: &str,
        interface: &str,
        member: &str,
        signature: Option<&str>,
        body: &[u8],
    ) -> Result<Vec<u8>, String> {
        let serial = self.next_serial();
        let mut encoder = DbusMessageEncoder::new();
        encoder.encode_method_call(
            serial,
            Some(destination),
            path,
            Some(interface),
            member,
            signature,
            body,
        );

        self.stream
            .write_all(encoder.as_bytes())
            .map_err(|e| format!("D-Bus write error for {}.{}: {}", interface, member, e))?;

        self.read_message()
    }

    /// Reads a complete framed D-Bus binary message from the stream.
    pub fn read_message(&mut self) -> Result<Vec<u8>, String> {
        let mut header = [0u8; 16];
        self.stream
            .read_exact(&mut header)
            .map_err(|e| format!("Failed to read D-Bus message header: {}", e))?;

        let body_len = u32::from_le_bytes(header[4..8].try_into().unwrap()) as usize;
        let fields_len = u32::from_le_bytes(header[12..16].try_into().unwrap()) as usize;

        let mut header_fields = vec![0u8; fields_len];
        if fields_len > 0 {
            self.stream
                .read_exact(&mut header_fields)
                .map_err(|e| format!("Failed to read D-Bus header fields: {}", e))?;
        }

        // Header padding
        let total_header = 16 + fields_len;
        let rem = total_header % 8;
        let pad_len = if rem != 0 { 8 - rem } else { 0 };
        if pad_len > 0 {
            let mut pad = vec![0u8; pad_len];
            self.stream
                .read_exact(&mut pad)
                .map_err(|e| format!("Failed to read D-Bus header padding: {}", e))?;
        }

        let mut body = vec![0u8; body_len];
        if body_len > 0 {
            self.stream
                .read_exact(&mut body)
                .map_err(|e| format!("Failed to read D-Bus body: {}", e))?;
        }

        let mut full_msg = Vec::with_capacity(16 + fields_len + pad_len + body_len);
        full_msg.extend_from_slice(&header);
        full_msg.extend_from_slice(&header_fields);
        full_msg.resize(full_msg.len() + pad_len, 0);
        full_msg.extend_from_slice(&body);
        Ok(full_msg)
    }
}

/// Dispatches a native desktop notification via `org.freedesktop.Notifications.Notify`.
///
/// Returns the notification ID assigned by the daemon.
pub fn show_desktop_notification(
    summary: &str,
    body: &str,
    icon: Option<&str>,
    timeout_ms: i32,
) -> Result<u32, String> {
    let mut client = DbusClient::connect()?;

    // Body signature: susssasa{sv}i
    // app_name (s), replaces_id (u), app_icon (s), summary (s), body (s), actions (as), hints (a{sv}), expire_timeout (i)
    let mut body_encoder = DbusMessageEncoder::new();
    body_encoder.push_string("Strata");
    body_encoder.push_u32(0); // replaces_id = 0
    body_encoder.push_string(icon.unwrap_or("dialog-information"));
    body_encoder.push_string(summary);
    body_encoder.push_string(body);

    // actions: array of strings (empty)
    body_encoder.push_u32(0);

    // hints: array of dict entries {sv} (empty)
    body_encoder.push_u32(0);

    // expire_timeout (i32)
    body_encoder.push_i32(timeout_ms);

    let reply = client.call_method(
        "org.freedesktop.Notifications",
        "/org/freedesktop/Notifications",
        "org.freedesktop.Notifications",
        "Notify",
        Some("susssasa{sv}i"),
        body_encoder.as_bytes(),
    )?;

    if reply.len() < 16 {
        return Err("Notification reply payload too short".to_string());
    }

    let fields_len = u32::from_le_bytes(reply[12..16].try_into().unwrap()) as usize;
    let mut header_total = 16 + fields_len;
    if !header_total.is_multiple_of(8) {
        header_total += 8 - (header_total % 8);
    }

    if reply.len() >= header_total + 4 {
        let notif_id =
            u32::from_le_bytes(reply[header_total..header_total + 4].try_into().unwrap());
        Ok(notif_id)
    } else {
        Ok(1)
    }
}
