//! Zero-C-dependency USB / Raw HID I/O (Linux `/sys/class/hidraw` + `/dev/hidraw*` ioctls
//! with zero `libudev-dev`, plus deterministic `MockHidDevice` for automated testing).

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

/// Metadata describing a discovered USB or Bluetooth Raw HID device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HidDeviceInfo {
    pub path: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub name: String,
    pub serial_number: Option<String>,
}

/// Discovers Raw HID endpoints on the host OS without `libudev`.
pub fn list_hid_devices() -> Vec<HidDeviceInfo> {
    let mut out = Vec::new();
    #[cfg(target_os = "linux")]
    {
        if let Ok(entries) = std::fs::read_dir("/sys/class/hidraw") {
            for entry in entries.flatten() {
                let node = entry.file_name().to_string_lossy().to_string();
                let uevent_path = entry.path().join("device/uevent");
                let text = std::fs::read_to_string(uevent_path).unwrap_or_default();
                let mut vid = 0u16;
                let mut pid = 0u16;
                let mut name = node.clone();
                let mut serial = None;

                for line in text.lines() {
                    if let Some(val) = line.strip_prefix("HID_ID=") {
                        // Format: BBBB:VVVVVVVV:PPPPPPPP
                        let parts: Vec<&str> = val.split(':').collect();
                        if parts.len() == 3 {
                            vid = u32::from_str_radix(parts[1], 16).unwrap_or(0) as u16;
                            pid = u32::from_str_radix(parts[2], 16).unwrap_or(0) as u16;
                        }
                    } else if let Some(val) = line.strip_prefix("HID_NAME=") {
                        name = val.trim().to_string();
                    } else if let Some(val) = line.strip_prefix("HID_UNIQ=") {
                        let s = val.trim();
                        if !s.is_empty() {
                            serial = Some(s.to_string());
                        }
                    }
                }

                out.push(HidDeviceInfo {
                    path: format!("/dev/{node}"),
                    vendor_id: vid,
                    product_id: pid,
                    name,
                    serial_number: serial,
                });
            }
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// Active physical Raw HID device handle (Linux `/dev/hidraw*`).
pub struct HidDevice {
    pub path: String,
    #[cfg(target_os = "linux")]
    fd: libc::c_int,
}

#[cfg(target_os = "linux")]
mod hidraw {
    pub const fn hidiocsfeature(len: usize) -> libc::c_ulong {
        0xC000_4806 | ((len as libc::c_ulong) << 16)
    }
    pub const fn hidiocgfeature(len: usize) -> libc::c_ulong {
        0xC000_4807 | ((len as libc::c_ulong) << 16)
    }
}

impl HidDevice {
    pub fn open(path: &str) -> Result<Self, String> {
        #[cfg(target_os = "linux")]
        {
            use std::ffi::CString;
            let c_path = CString::new(path).map_err(|_| format!("Invalid HID path: {path}"))?;
            let fd = unsafe { libc::open(c_path.as_ptr(), libc::O_RDWR | libc::O_NONBLOCK) };
            if fd < 0 {
                return Err(format!(
                    "Failed to open HID device '{path}': {}",
                    std::io::Error::last_os_error()
                ));
            }
            Ok(Self {
                path: path.to_string(),
                fd,
            })
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = path;
            Err("Direct hidraw open is Linux-native; use MockHidDevice in CI".into())
        }
    }

    pub fn read_report(&mut self, buf: &mut [u8]) -> Result<usize, String> {
        #[cfg(target_os = "linux")]
        {
            let n = unsafe {
                libc::read(self.fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len())
            };
            if n < 0 {
                Err(format!("HID read error: {}", std::io::Error::last_os_error()))
            } else {
                Ok(n as usize)
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = buf;
            Ok(0)
        }
    }

    pub fn write_report(&mut self, report: &[u8]) -> Result<usize, String> {
        #[cfg(target_os = "linux")]
        {
            let n = unsafe {
                libc::write(
                    self.fd,
                    report.as_ptr() as *const libc::c_void,
                    report.len(),
                )
            };
            if n < 0 {
                Err(format!("HID write error: {}", std::io::Error::last_os_error()))
            } else {
                Ok(n as usize)
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = report;
            Ok(0)
        }
    }

    pub fn send_feature_report(&mut self, report: &[u8]) -> Result<usize, String> {
        #[cfg(target_os = "linux")]
        {
            let rc = unsafe {
                libc::ioctl(
                    self.fd,
                    hidraw::hidiocsfeature(report.len()) as _,
                    report.as_ptr(),
                )
            };
            if rc < 0 {
                Err(format!("HIDIOCSFEATURE failed: {}", std::io::Error::last_os_error()))
            } else {
                Ok(rc as usize)
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = report;
            Ok(0)
        }
    }

    pub fn get_feature_report(&mut self, buf: &mut [u8]) -> Result<usize, String> {
        #[cfg(target_os = "linux")]
        {
            let rc = unsafe {
                libc::ioctl(
                    self.fd,
                    hidraw::hidiocgfeature(buf.len()) as _,
                    buf.as_mut_ptr(),
                )
            };
            if rc < 0 {
                Err(format!("HIDIOCGFEATURE failed: {}", std::io::Error::last_os_error()))
            } else {
                Ok(rc as usize)
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = buf;
            Ok(0)
        }
    }
}

#[cfg(target_os = "linux")]
impl Drop for HidDevice {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.fd);
        }
    }
}

/// Deterministic mock USB/Raw HID device supporting input, output, and feature report round-trips.
#[derive(Clone)]
pub struct MockHidDevice {
    pub info: HidDeviceInfo,
    in_reports: Arc<Mutex<VecDeque<Vec<u8>>>>,
    out_reports: Arc<Mutex<Vec<Vec<u8>>>>,
    feature_reports: Arc<Mutex<HashMap<u8, Vec<u8>>>>,
}

impl MockHidDevice {
    pub fn new(info: HidDeviceInfo) -> Self {
        Self {
            info,
            in_reports: Arc::new(Mutex::new(VecDeque::new())),
            out_reports: Arc::new(Mutex::new(Vec::new())),
            feature_reports: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn push_input_report(&self, report: Vec<u8>) {
        if let Ok(mut q) = self.in_reports.lock() {
            q.push_back(report);
        }
    }

    pub fn read_report(&self) -> Option<Vec<u8>> {
        self.in_reports.lock().ok()?.pop_front()
    }

    pub fn write_report(&self, report: Vec<u8>) {
        if let Ok(mut log) = self.out_reports.lock() {
            log.push(report);
        }
    }

    pub fn written_reports(&self) -> Vec<Vec<u8>> {
        self.out_reports.lock().map(|v| v.clone()).unwrap_or_default()
    }

    pub fn send_feature_report(&self, report_id: u8, payload: Vec<u8>) {
        if let Ok(mut map) = self.feature_reports.lock() {
            map.insert(report_id, payload);
        }
    }

    pub fn get_feature_report(&self, report_id: u8) -> Option<Vec<u8>> {
        self.feature_reports.lock().ok()?.get(&report_id).cloned()
    }
}
