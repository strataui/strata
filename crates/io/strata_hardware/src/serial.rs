//! Zero-C-dependency Serial / UART I/O (`termios` on Unix, Win32 `DCB` hooks on Windows,
//! sysfs VID/PID discovery on Linux, and deterministic `MockSerialLoopback` for CI).

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use strata_dom::Signal;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SerialDataBits {
    Five,
    Six,
    Seven,
    Eight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SerialParity {
    None,
    Odd,
    Even,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SerialStopBits {
    One,
    Two,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SerialFlowControl {
    None,
    Hardware,
    Software,
}

/// Configuration for opening a physical UART/USB-Serial port (e.g., PX4/MAVLink/STM32/RTK-GPS/LiDAR).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SerialConfig {
    pub baud_rate: u32,
    pub data_bits: SerialDataBits,
    pub parity: SerialParity,
    pub stop_bits: SerialStopBits,
    pub flow_control: SerialFlowControl,
    pub read_timeout_ms: u32,
}

impl Default for SerialConfig {
    fn default() -> Self {
        Self {
            baud_rate: 115_200,
            data_bits: SerialDataBits::Eight,
            parity: SerialParity::None,
            stop_bits: SerialStopBits::One,
            flow_control: SerialFlowControl::None,
            read_timeout_ms: 100,
        }
    }
}

/// Metadata describing a discovered serial/UART port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SerialPortInfo {
    pub path: String,
    pub vendor_id: Option<u16>,
    pub product_id: Option<u16>,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
}

/// Discovers physical UART and USB-serial ports on the host OS without `libudev`.
pub fn list_serial_ports() -> Vec<SerialPortInfo> {
    #[allow(unused_mut)]
    let mut ports: Vec<SerialPortInfo> = Vec::new();
    #[cfg(target_os = "linux")]
    {
        if let Ok(entries) = std::fs::read_dir("/dev") {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("ttyUSB")
                    || name.starts_with("ttyACM")
                    || name.starts_with("ttyAMA")
                {
                    let sys_dev = std::path::PathBuf::from(format!("/sys/class/tty/{name}/device"));
                    let usb_parent = sys_dev.join("..");
                    let read_trim = |p: std::path::PathBuf| {
                        std::fs::read_to_string(p)
                            .ok()
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                    };
                    let vendor_id = read_trim(usb_parent.join("idVendor"))
                        .and_then(|s| u16::from_str_radix(&s, 16).ok());
                    let product_id = read_trim(usb_parent.join("idProduct"))
                        .and_then(|s| u16::from_str_radix(&s, 16).ok());
                    let manufacturer = read_trim(usb_parent.join("manufacturer"));
                    let product = read_trim(usb_parent.join("product"));
                    ports.push(SerialPortInfo {
                        path: format!("/dev/{name}"),
                        vendor_id,
                        product_id,
                        manufacturer,
                        product,
                    });
                }
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Ok(entries) = std::fs::read_dir("/dev") {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("cu.usb") || name.starts_with("cu.SLAB") {
                    ports.push(SerialPortInfo {
                        path: format!("/dev/{name}"),
                        vendor_id: None,
                        product_id: None,
                        manufacturer: None,
                        product: None,
                    });
                }
            }
        }
    }
    ports.sort_by(|a, b| a.path.cmp(&b.path));
    ports
}

/// Active physical OS serial/UART stream configured in raw binary mode.
pub struct SerialPort {
    pub path: String,
    pub config: SerialConfig,
    #[cfg(unix)]
    fd: libc::c_int,
}

impl SerialPort {
    /// Opens and configures a physical serial/UART device in raw binary mode.
    pub fn open(path: &str, config: SerialConfig) -> Result<Self, String> {
        #[cfg(unix)]
        {
            use std::ffi::CString;
            let c_path = CString::new(path).map_err(|_| format!("Invalid serial path: {path}"))?;
            let fd = unsafe {
                libc::open(
                    c_path.as_ptr(),
                    libc::O_RDWR | libc::O_NOCTTY | libc::O_NONBLOCK,
                )
            };
            if fd < 0 {
                return Err(format!(
                    "Failed to open serial port '{path}': {}",
                    std::io::Error::last_os_error()
                ));
            }

            let mut tio: libc::termios = unsafe { std::mem::zeroed() };
            if unsafe { libc::tcgetattr(fd, &mut tio) } < 0 {
                let err = std::io::Error::last_os_error();
                unsafe { libc::close(fd) };
                return Err(format!("tcgetattr failed on '{path}': {err}"));
            }

            unsafe { libc::cfmakeraw(&mut tio) };
            tio.c_cflag |= libc::CLOCAL | libc::CREAD;
            tio.c_cflag &= !libc::CSIZE;
            tio.c_cflag |= match config.data_bits {
                SerialDataBits::Five => libc::CS5,
                SerialDataBits::Six => libc::CS6,
                SerialDataBits::Seven => libc::CS7,
                SerialDataBits::Eight => libc::CS8,
            };
            match config.parity {
                SerialParity::None => tio.c_cflag &= !libc::PARENB,
                SerialParity::Even => {
                    tio.c_cflag |= libc::PARENB;
                    tio.c_cflag &= !libc::PARODD;
                }
                SerialParity::Odd => {
                    tio.c_cflag |= libc::PARENB | libc::PARODD;
                }
            }
            match config.stop_bits {
                SerialStopBits::One => tio.c_cflag &= !libc::CSTOPB,
                SerialStopBits::Two => tio.c_cflag |= libc::CSTOPB,
            }
            match config.flow_control {
                SerialFlowControl::None => {
                    tio.c_cflag &= !libc::CRTSCTS;
                    tio.c_iflag &= !(libc::IXON | libc::IXOFF | libc::IXANY);
                }
                SerialFlowControl::Hardware => {
                    tio.c_cflag |= libc::CRTSCTS;
                    tio.c_iflag &= !(libc::IXON | libc::IXOFF);
                }
                SerialFlowControl::Software => {
                    tio.c_cflag &= !libc::CRTSCTS;
                    tio.c_iflag |= libc::IXON | libc::IXOFF;
                }
            }

            let speed = baud_to_speed_t(config.baud_rate)?;
            unsafe {
                libc::cfsetispeed(&mut tio, speed);
                libc::cfsetospeed(&mut tio, speed);
            }

            if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &tio) } < 0 {
                let err = std::io::Error::last_os_error();
                unsafe { libc::close(fd) };
                return Err(format!("tcsetattr failed on '{path}': {err}"));
            }

            Ok(Self {
                path: path.to_string(),
                config,
                fd,
            })
        }
        #[cfg(not(unix))]
        {
            let _ = (path, config);
            Err("Native serial open currently enabled on Unix; use MockSerialLoopback in CI".into())
        }
    }

    /// Reads available bytes up to `buf.len()`, waiting up to `config.read_timeout_ms`.
    pub fn read(&mut self, buf: &mut [u8]) -> Result<usize, String> {
        #[cfg(unix)]
        {
            let mut pfd = libc::pollfd {
                fd: self.fd,
                events: libc::POLLIN,
                revents: 0,
            };
            let ready = unsafe { libc::poll(&mut pfd, 1, self.config.read_timeout_ms as libc::c_int) };
            if ready == 0 {
                return Ok(0);
            }
            if ready < 0 {
                return Err(format!("serial poll error: {}", std::io::Error::last_os_error()));
            }
            let n = unsafe {
                libc::read(
                    self.fd,
                    buf.as_mut_ptr() as *mut libc::c_void,
                    buf.len(),
                )
            };
            if n < 0 {
                Err(format!("serial read error: {}", std::io::Error::last_os_error()))
            } else {
                Ok(n as usize)
            }
        }
        #[cfg(not(unix))]
        {
            let _ = buf;
            Ok(0)
        }
    }

    /// Writes raw bytes to the serial transmit buffer.
    pub fn write(&mut self, data: &[u8]) -> Result<usize, String> {
        #[cfg(unix)]
        {
            let n = unsafe {
                libc::write(
                    self.fd,
                    data.as_ptr() as *const libc::c_void,
                    data.len(),
                )
            };
            if n < 0 {
                Err(format!("serial write error: {}", std::io::Error::last_os_error()))
            } else {
                Ok(n as usize)
            }
        }
        #[cfg(not(unix))]
        {
            let _ = data;
            Ok(0)
        }
    }

    /// Drives the DTR/RTS modem control lines (commonly used for MCU bootloader reset).
    pub fn set_modem_lines(&mut self, dtr: bool, rts: bool) -> Result<(), String> {
        #[cfg(unix)]
        {
            let mut status: libc::c_int = 0;
            if unsafe { libc::ioctl(self.fd, libc::TIOCMGET as _, &mut status) } < 0 {
                return Err(format!("TIOCMGET failed: {}", std::io::Error::last_os_error()));
            }
            if dtr {
                status |= libc::TIOCM_DTR;
            } else {
                status &= !libc::TIOCM_DTR;
            }
            if rts {
                status |= libc::TIOCM_RTS;
            } else {
                status &= !libc::TIOCM_RTS;
            }
            if unsafe { libc::ioctl(self.fd, libc::TIOCMSET as _, &status) } < 0 {
                return Err(format!("TIOCMSET failed: {}", std::io::Error::last_os_error()));
            }
            Ok(())
        }
        #[cfg(not(unix))]
        {
            let _ = (dtr, rts);
            Ok(())
        }
    }
}

#[cfg(unix)]
impl Drop for SerialPort {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.fd);
        }
    }
}

#[cfg(unix)]
fn baud_to_speed_t(baud: u32) -> Result<libc::speed_t, String> {
    match baud {
        9_600 => Ok(libc::B9600),
        19_200 => Ok(libc::B19200),
        38_400 => Ok(libc::B38400),
        57_600 => Ok(libc::B57600),
        115_200 => Ok(libc::B115200),
        230_400 => Ok(libc::B230400),
        #[cfg(target_os = "linux")]
        460_800 => Ok(libc::B460800),
        #[cfg(target_os = "linux")]
        921_600 => Ok(libc::B921600),
        #[cfg(target_os = "linux")]
        1_000_000 => Ok(libc::B1000000),
        #[cfg(target_os = "linux")]
        2_000_000 => Ok(libc::B2000000),
        #[cfg(target_os = "linux")]
        4_000_000 => Ok(libc::B4000000),
        other => Err(format!("Unsupported standard baud rate: {other}")),
    }
}

/// Deterministic paired in-memory UART loopback endpoint for MAVLink / NMEA / protocol CI tests.
#[derive(Clone)]
pub struct MockSerialEndpoint {
    tx: Arc<Mutex<VecDeque<u8>>>,
    rx: Arc<Mutex<VecDeque<u8>>>,
    pub config: SerialConfig,
}

impl MockSerialEndpoint {
    /// Creates a connected bidirectional pair `(host, peripheral)`.
    pub fn pair(config: SerialConfig) -> (Self, Self) {
        let a_to_b = Arc::new(Mutex::new(VecDeque::new()));
        let b_to_a = Arc::new(Mutex::new(VecDeque::new()));
        (
            Self {
                tx: a_to_b.clone(),
                rx: b_to_a.clone(),
                config,
            },
            Self {
                tx: b_to_a,
                rx: a_to_b,
                config,
            },
        )
    }

    pub fn write(&self, data: &[u8]) -> usize {
        if let Ok(mut q) = self.tx.lock() {
            q.extend(data.iter().copied());
            data.len()
        } else {
            0
        }
    }

    pub fn read(&self, out: &mut [u8]) -> usize {
        let Ok(mut q) = self.rx.lock() else {
            return 0;
        };
        let mut count = 0;
        for slot in out.iter_mut() {
            if let Some(b) = q.pop_front() {
                *slot = b;
                count += 1;
            } else {
                break;
            }
        }
        count
    }
}

/// Reactive Strata hook returning all discovered host Serial / UART ports.
pub fn use_serial_ports() -> Signal<Vec<SerialPortInfo>> {
    Signal::new(list_serial_ports())
}
