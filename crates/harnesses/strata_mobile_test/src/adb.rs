//! Pure-Rust Zero-Dependency ADB Client Protocol, Packet Framer, Port Scanner, and Mock Bridge.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

/// Standard emulator ADB discovery ports across Windows, macOS, and Linux.
pub const STANDARD_EMULATOR_PORTS: &[u16] = &[5555, 5557, 7555, 16384, 62001, 62025];

/// Represents an active Android device or emulator recognized by the ADB daemon.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdbDevice {
    pub serial: String,
    pub state: String,
    pub product: String,
    pub model: String,
    pub device: String,
}

impl AdbDevice {
    /// Parses an ADB `host:devices-l` line output into an `AdbDevice`.
    /// Example: `emulator-5554   device product:sdk_gphone64_x86_64 model:Pixel_Fold device:generic_x86_64 transport_id:1`
    pub fn parse_device_line(line: &str) -> Option<Self> {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            return None;
        }

        let serial = parts[0].to_string();
        let state = parts[1].to_string();

        let mut product = String::new();
        let mut model = String::new();
        let mut device = String::new();

        for part in &parts[2..] {
            if let Some(val) = part.strip_prefix("product:") {
                product = val.to_string();
            } else if let Some(val) = part.strip_prefix("model:") {
                model = val.to_string();
            } else if let Some(val) = part.strip_prefix("device:") {
                device = val.to_string();
            }
        }

        Some(Self {
            serial,
            state,
            product,
            model,
            device,
        })
    }
}

/// Packet encoding and framing for the ADB 4-byte hexadecimal length protocol.
pub struct AdbPacketCodec;

impl AdbPacketCodec {
    /// Formats an outgoing ADB command string with a 4-byte uppercase hexadecimal length prefix.
    /// Example: `"host:version"` -> `"000Chost:version"`
    pub fn encode_command(command: &str) -> Vec<u8> {
        let len = command.len();
        let header = format!("{:04X}", len);
        let mut packet = Vec::with_capacity(4 + len);
        packet.extend_from_slice(header.as_bytes());
        packet.extend_from_slice(command.as_bytes());
        packet
    }

    /// Encodes a successful ADB status response `"OKAY"` with optional payload.
    pub fn encode_response_okay(payload: &[u8]) -> Vec<u8> {
        let len = payload.len();
        let header = format!("{:04X}", len);
        let mut packet = Vec::with_capacity(4 + 4 + len);
        packet.extend_from_slice(b"OKAY");
        packet.extend_from_slice(header.as_bytes());
        packet.extend_from_slice(payload);
        packet
    }

    /// Encodes a failed ADB status response `"FAIL"` with an error message payload.
    pub fn encode_response_fail(error_message: &str) -> Vec<u8> {
        let len = error_message.len();
        let header = format!("{:04X}", len);
        let mut packet = Vec::with_capacity(4 + 4 + len);
        packet.extend_from_slice(b"FAIL");
        packet.extend_from_slice(header.as_bytes());
        packet.extend_from_slice(error_message.as_bytes());
        packet
    }

    /// Parses a 4-byte hexadecimal length prefix.
    pub fn parse_hex_length(header: &[u8]) -> Result<usize, &'static str> {
        if header.len() < 4 {
            return Err("Header buffer too short, expected 4 bytes");
        }
        let hex_str = std::str::from_utf8(&header[0..4]).map_err(|_| "Invalid UTF-8 header")?;
        usize::from_str_radix(hex_str, 16).map_err(|_| "Invalid hexadecimal integer")
    }
}

/// Port scanner identifying third-party and official Android emulator instances.
pub struct AdbPortScanner;

impl AdbPortScanner {
    /// Matches an open port to its known emulator runtime ecosystem.
    pub fn identify_runtime(port: u16) -> &'static str {
        match port {
            5555 => "Standard AVD / Redroid / BlueStacks / LDPlayer (Inst 0)",
            5557 => "LDPlayer (Instance 1)",
            7555 => "MuMu Player Classic",
            16384 => "MuMu Player 12",
            62001 => "NoxPlayer (Instance 0)",
            62025 => "NoxPlayer (Instance 1)",
            _ => "Unknown ADB Endpoint",
        }
    }
}

/// Pure-Rust real TCP client for the Android Debug Bridge (ADB) daemon running on port 5037.
pub struct AdbClient {
    stream: TcpStream,
}

impl AdbClient {
    pub const DEFAULT_HOST: &'static str = "127.0.0.1";
    pub const DEFAULT_PORT: u16 = 5037;

    /// Connects to the local ADB daemon at 127.0.0.1:5037.
    pub fn connect_default() -> std::io::Result<Self> {
        Self::connect("127.0.0.1:5037")
    }

    /// Connects to an ADB daemon at the specified socket address.
    pub fn connect<A: ToSocketAddrs>(addr: A) -> std::io::Result<Self> {
        let socket_addr = addr
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "No valid address"))?;
        let stream = TcpStream::connect_timeout(&socket_addr, Duration::from_millis(1500))?;
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        stream.set_write_timeout(Some(Duration::from_secs(5)))?;
        Ok(Self { stream })
    }

    /// Helper to send an ADB framed command.
    fn send_command(&mut self, cmd: &str) -> Result<(), String> {
        let packet = AdbPacketCodec::encode_command(cmd);
        self.stream.write_all(&packet).map_err(|e| e.to_string())
    }

    /// Reads 4-byte status from ADB ("OKAY" or "FAIL").
    fn read_status_okay(&mut self) -> Result<(), String> {
        let mut status = [0u8; 4];
        self.stream.read_exact(&mut status).map_err(|e| e.to_string())?;
        if &status == b"OKAY" {
            Ok(())
        } else if &status == b"FAIL" {
            let mut len_buf = [0u8; 4];
            self.stream.read_exact(&mut len_buf).map_err(|e| e.to_string())?;
            let len = AdbPacketCodec::parse_hex_length(&len_buf)?;
            let mut err_buf = vec![0u8; len];
            self.stream.read_exact(&mut err_buf).map_err(|e| e.to_string())?;
            Err(String::from_utf8_lossy(&err_buf).to_string())
        } else {
            Err(format!("Unexpected ADB response status: {:?}", status))
        }
    }

    /// Reads OKAY status followed by length-prefixed payload.
    fn read_length_prefixed_response(&mut self) -> Result<Vec<u8>, String> {
        self.read_status_okay()?;
        let mut len_buf = [0u8; 4];
        self.stream.read_exact(&mut len_buf).map_err(|e| e.to_string())?;
        let len = AdbPacketCodec::parse_hex_length(&len_buf)?;
        let mut data = vec![0u8; len];
        self.stream.read_exact(&mut data).map_err(|e| e.to_string())?;
        Ok(data)
    }

    /// Queries the running ADB server protocol version (e.g. 41).
    pub fn get_version(&mut self) -> Result<u32, String> {
        self.send_command("host:version")?;
        let resp = self.read_length_prefixed_response()?;
        let text = std::str::from_utf8(&resp).map_err(|e| e.to_string())?;
        u32::from_str_radix(text.trim(), 16).map_err(|e| e.to_string())
    }

    /// Discovers connected devices and emulators.
    pub fn list_devices(&mut self) -> Result<Vec<AdbDevice>, String> {
        self.send_command("host:devices-l")?;
        let resp = self.read_length_prefixed_response()?;
        let text = std::str::from_utf8(&resp).map_err(|e| e.to_string())?;
        let mut devices = Vec::new();
        for line in text.lines() {
            if let Some(dev) = AdbDevice::parse_device_line(line) {
                devices.push(dev);
            }
        }
        Ok(devices)
    }

    /// Sets up a port forward: host port -> device port.
    pub fn forward_port(
        &mut self,
        serial: &str,
        host_port: u16,
        device_port: u16,
    ) -> Result<(), String> {
        let cmd = format!("host-serial:{serial}:forward:tcp:{host_port};tcp:{device_port}");
        self.send_command(&cmd)?;
        self.read_status_okay()
    }

    /// Sets up a reverse port forward: device port -> host port.
    pub fn reverse_port(
        &mut self,
        serial: &str,
        device_port: u16,
        host_port: u16,
    ) -> Result<(), String> {
        let cmd = format!("host-serial:{serial}:reverse:forward:tcp:{device_port};tcp:{host_port}");
        self.send_command(&cmd)?;
        self.read_status_okay()
    }

    /// Executes a shell command on the specified device and returns its stdout.
    pub fn shell(&mut self, serial: &str, command: &str) -> Result<String, String> {
        let _ = self.send_command(&format!("host:transport:{serial}"));
        let _ = self.read_status_okay();

        self.send_command(&format!("shell:{command}"))?;
        let mut status = [0u8; 4];
        self.stream.read_exact(&mut status).map_err(|e| e.to_string())?;
        if &status == b"OKAY" {
            let mut len_buf = [0u8; 4];
            if self.stream.read_exact(&mut len_buf).is_ok() {
                if let Ok(len) = AdbPacketCodec::parse_hex_length(&len_buf) {
                    let mut data = vec![0u8; len];
                    self.stream.read_exact(&mut data).map_err(|e| e.to_string())?;
                    return String::from_utf8(data).map_err(|e| e.to_string());
                }
            }
            let mut remaining = Vec::new();
            let _ = self.stream.read_to_end(&mut remaining);
            String::from_utf8(remaining).map_err(|e| e.to_string())
        } else {
            Err("ADB shell command returned non-OKAY status".to_string())
        }
    }

    /// Captures a screenshot from the target device using `exec:screencap -p`.
    pub fn screencap(&mut self, serial: &str) -> Result<Vec<u8>, String> {
        let _ = self.send_command(&format!("host:transport:{serial}"));
        let _ = self.read_status_okay();

        self.send_command("exec:screencap -p")?;
        let mut status = [0u8; 4];
        self.stream.read_exact(&mut status).map_err(|e| e.to_string())?;
        if &status == b"OKAY" {
            let mut len_buf = [0u8; 4];
            if self.stream.read_exact(&mut len_buf).is_ok() {
                if let Ok(len) = AdbPacketCodec::parse_hex_length(&len_buf) {
                    let mut data = vec![0u8; len];
                    self.stream.read_exact(&mut data).map_err(|e| e.to_string())?;
                    return Ok(data);
                }
            }
            let mut remaining = Vec::new();
            let _ = self.stream.read_to_end(&mut remaining);
            Ok(remaining)
        } else {
            Err("ADB screencap command returned non-OKAY status".to_string())
        }
    }
}

/// Pure-Rust client talking to Android Emulator Telnet Console (default port 5554)
/// for programmatic hardware posture, hinge angle, and sensor manipulation.
pub struct EmulatorConsoleClient {
    stream: TcpStream,
}

impl EmulatorConsoleClient {
    /// Connects to emulator console port (e.g. 5554).
    pub fn connect(port: u16) -> std::io::Result<Self> {
        let stream =
            TcpStream::connect_timeout(&format!("127.0.0.1:{port}").parse().unwrap(), Duration::from_millis(1500))?;
        stream.set_read_timeout(Some(Duration::from_secs(3)))?;
        stream.set_write_timeout(Some(Duration::from_secs(3)))?;
        Ok(Self { stream })
    }

    /// Authenticates with the emulator console using auth token.
    /// If token is None, attempts to read `~/.emulator_console_auth_token`.
    pub fn authenticate(&mut self, explicit_token: Option<&str>) -> Result<(), String> {
        // Drain initial console greeting
        let mut banner = [0u8; 256];
        let _ = self.stream.read(&mut banner);

        let token = if let Some(t) = explicit_token {
            t.to_string()
        } else if let Ok(home) = std::env::var("HOME") {
            let path = std::path::Path::new(&home).join(".emulator_console_auth_token");
            std::fs::read_to_string(&path)
                .map(|s| s.trim().to_string())
                .unwrap_or_default()
        } else {
            String::new()
        };

        if !token.is_empty() {
            let cmd = format!("auth {token}\r\n");
            self.stream.write_all(cmd.as_bytes()).map_err(|e| e.to_string())?;
            let mut reader = BufReader::new(self.stream.try_clone().map_err(|e| e.to_string())?);
            let mut resp = String::new();
            reader.read_line(&mut resp).map_err(|e| e.to_string())?;
            if !resp.contains("OK") {
                return Err(format!("Emulator console authentication failed: {resp}"));
            }
        }
        Ok(())
    }

    /// Injects programmatic foldable/flip posture (e.g. "closed", "half-opened", "flat", "flipped").
    pub fn set_posture(&mut self, posture_str: &str) -> Result<(), String> {
        let cmd = format!("posture set {posture_str}\r\n");
        self.stream.write_all(cmd.as_bytes()).map_err(|e| e.to_string())?;
        let mut reader = BufReader::new(self.stream.try_clone().map_err(|e| e.to_string())?);
        let mut resp = String::new();
        reader.read_line(&mut resp).map_err(|e| e.to_string())?;
        if resp.contains("OK") {
            Ok(())
        } else {
            Err(format!("Posture injection failed: {resp}"))
        }
    }

    /// Injects specific hinge sensor angle in degrees [0, 360].
    pub fn set_hinge_angle(&mut self, angle: u32) -> Result<(), String> {
        let cmd = format!("sensor set hinge_angle {angle}\r\n");
        self.stream.write_all(cmd.as_bytes()).map_err(|e| e.to_string())?;
        let mut reader = BufReader::new(self.stream.try_clone().map_err(|e| e.to_string())?);
        let mut resp = String::new();
        reader.read_line(&mut resp).map_err(|e| e.to_string())?;
        if resp.contains("OK") {
            Ok(())
        } else {
            Err(format!("Hinge angle injection failed: {resp}"))
        }
    }

    /// Terminates the emulator console session.
    pub fn quit(&mut self) -> Result<(), String> {
        let _ = self.stream.write_all(b"quit\r\n");
        Ok(())
    }
}

/// In-memory simulated ADB daemon server for deterministic, offline testing and CI workflows.
#[derive(Debug, Clone)]
pub struct MockAdbServer {
    devices: Vec<AdbDevice>,
    forwarded_ports: HashMap<u16, u16>,
    reversed_ports: HashMap<u16, u16>,
    shell_responses: HashMap<String, String>,
}

impl Default for MockAdbServer {
    fn default() -> Self {
        let mut server = Self {
            devices: Vec::new(),
            forwarded_ports: HashMap::new(),
            reversed_ports: HashMap::new(),
            shell_responses: HashMap::new(),
        };

        // Add standard simulated Pixel Fold device
        server.add_device(AdbDevice {
            serial: "emulator-5554".to_string(),
            state: "device".to_string(),
            product: "sdk_gphone64_x86_64".to_string(),
            model: "Pixel_Fold".to_string(),
            device: "generic_x86_64".to_string(),
        });

        // Add standard boot completed property response
        server.set_shell_response("getprop sys.boot_completed", "1\n");
        server
    }
}

impl MockAdbServer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_device(&mut self, device: AdbDevice) {
        self.devices.push(device);
    }

    pub fn set_shell_response(&mut self, cmd: impl Into<String>, response: impl Into<String>) {
        self.shell_responses.insert(cmd.into(), response.into());
    }

    /// Starts an in-memory MockAdbServer listening on a live TCP port.
    pub fn start_tcp(
        port: u16,
    ) -> std::io::Result<(u16, Arc<AtomicBool>, thread::JoinHandle<()>)> {
        let listener = TcpListener::bind(("127.0.0.1", port))?;
        let bound_port = listener.local_addr()?.port();
        listener.set_nonblocking(true)?;

        let running = Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();

        let handle = thread::spawn(move || {
            let mut server = MockAdbServer::new();
            while running_clone.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
                        let _ = stream.set_write_timeout(Some(Duration::from_millis(500)));

                        let mut len_buf = [0u8; 4];
                        while stream.read_exact(&mut len_buf).is_ok() {
                            if let Ok(cmd_len) = AdbPacketCodec::parse_hex_length(&len_buf) {
                                let mut cmd_buf = vec![0u8; cmd_len];
                                if stream.read_exact(&mut cmd_buf).is_ok() {
                                    if let Ok(cmd_str) = std::str::from_utf8(&cmd_buf) {
                                        let resp = server.handle_command(cmd_str);
                                        let _ = stream.write_all(&resp);
                                    }
                                }
                            }
                        }
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            }
        });

        Ok((bound_port, running, handle))
    }

    /// Handles a raw command string and returns the encoded response packet.
    pub fn handle_command(&mut self, command: &str) -> Vec<u8> {
        if command == "host:version" {
            return AdbPacketCodec::encode_response_okay(b"0029"); // Protocol v41
        }

        if command == "host:devices-l" {
            let mut list_str = String::new();
            for d in &self.devices {
                list_str.push_str(&format!(
                    "{}   {} product:{} model:{} device:{} transport_id:1\n",
                    d.serial, d.state, d.product, d.model, d.device
                ));
            }
            return AdbPacketCodec::encode_response_okay(list_str.as_bytes());
        }

        if command.starts_with("host:transport:") {
            return b"OKAY".to_vec();
        }

        if let Some(fwd) = command.strip_prefix("host-serial:") {
            if let Some(idx) = fwd.find(":reverse:forward:tcp:") {
                let rest = &fwd[idx + ":reverse:forward:tcp:".len()..];
                if let Some(semi) = rest.find(";tcp:") {
                    let dev_port: u16 = rest[..semi].parse().unwrap_or(0);
                    let host_port: u16 = rest[semi + ";tcp:".len()..].parse().unwrap_or(0);
                    self.reversed_ports.insert(dev_port, host_port);
                    return b"OKAY".to_vec();
                }
            } else if let Some(idx) = fwd.find(":forward:tcp:") {
                let rest = &fwd[idx + ":forward:tcp:".len()..];
                if let Some(semi) = rest.find(";tcp:") {
                    let host_port: u16 = rest[..semi].parse().unwrap_or(0);
                    let dev_port: u16 = rest[semi + ";tcp:".len()..].parse().unwrap_or(0);
                    self.forwarded_ports.insert(host_port, dev_port);
                    return b"OKAY".to_vec();
                }
            }
        }

        if let Some(cmd) = command.strip_prefix("shell:") {
            let trimmed = cmd.trim();
            if let Some(resp) = self.shell_responses.get(trimmed) {
                return AdbPacketCodec::encode_response_okay(resp.as_bytes());
            }
            return AdbPacketCodec::encode_response_okay(b"");
        }

        if command == "exec:screencap -p" {
            // Emits mock 4-byte PNG signature header followed by dummy RGBA stream
            let mut png_mock = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
            png_mock.extend_from_slice(&[0u8; 64]);
            return AdbPacketCodec::encode_response_okay(&png_mock);
        }

        AdbPacketCodec::encode_response_fail("Unknown ADB command")
    }

    pub fn forwarded_ports(&self) -> &HashMap<u16, u16> {
        &self.forwarded_ports
    }

    pub fn reversed_ports(&self) -> &HashMap<u16, u16> {
        &self.reversed_ports
    }
}

/// Mock Emulator Telnet Console server on TCP port for testing posture and hinge injection.
pub struct MockEmulatorConsoleServer;

impl MockEmulatorConsoleServer {
    pub fn start_tcp(
        port: u16,
        auth_token: &'static str,
    ) -> std::io::Result<(u16, Arc<AtomicBool>, thread::JoinHandle<()>)> {
        let listener = TcpListener::bind(("127.0.0.1", port))?;
        let bound_port = listener.local_addr()?.port();
        listener.set_nonblocking(true)?;

        let running = Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();

        let handle = thread::spawn(move || {
            while running_clone.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
                        let _ = stream.set_write_timeout(Some(Duration::from_millis(500)));

                        let _ = stream.write_all(b"Android Console: Authentication required\r\nOK\r\n");

                        let reader_stream = match stream.try_clone() {
                            Ok(s) => s,
                            Err(_) => continue,
                        };
                        let mut reader = BufReader::new(reader_stream);
                        let mut line = String::new();
                        while let Ok(n) = reader.read_line(&mut line) {
                            if n == 0 {
                                break;
                            }
                            let trimmed = line.trim();
                            if trimmed.starts_with("auth ") {
                                let token = trimmed.strip_prefix("auth ").unwrap_or("");
                                if token == auth_token {
                                    let _ = stream.write_all(b"OK\r\n");
                                } else {
                                    let _ = stream.write_all(b"KO: bad auth token\r\n");
                                }
                            } else if trimmed.starts_with("posture set ")
                                || trimmed.starts_with("sensor set hinge_angle ")
                            {
                                let _ = stream.write_all(b"OK\r\n");
                            } else if trimmed == "quit" {
                                let _ = stream.write_all(b"OK: bye\r\n");
                                break;
                            } else {
                                let _ = stream.write_all(b"OK\r\n");
                            }
                            line.clear();
                        }
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            }
        });

        Ok((bound_port, running, handle))
    }
}
