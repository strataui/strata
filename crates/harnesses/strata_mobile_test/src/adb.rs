//! Pure-Rust Zero-Dependency ADB Client Protocol, Packet Framer, Port Scanner, and Mock Bridge.

use std::collections::HashMap;

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
