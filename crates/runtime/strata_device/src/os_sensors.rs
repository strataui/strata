//! Cross-platform OS Geolocation/GPS and Hardware Sensor probes.
//!
//! Resolution order for [`probe_os_geolocation`]:
//! 1. [`DeviceMockController`] mock override (`source == GeolocationSource::MockOverride`)
//! 2. `STRATA_GEO_LAT_LON="lat,lon[,alt_m,accuracy_m]"` deterministic environment override
//! 3. Native OS geolocation / GPS backends:
//!    - **Linux**: `gpsd` local JSON daemon (`127.0.0.1:2947`), NMEA-0183 serial GPS
//!      (`STRATA_GPS_DEVICE` or `/dev/ttyACM*` / `/dev/ttyUSB*`), GeoClue2 D-Bus
//!      (`org.freedesktop.GeoClue2`), and XDG Desktop Portal Location (`org.freedesktop.portal.Location`)
//!    - **macOS**: CoreLocation location services probe
//!    - **Windows**: WinRT `Windows.Devices.Geolocation.Geolocator`
//! 4. Optional cached HTTPS GeoIP fallback (`allow_ip_fallback == true`, skipped if
//!    `STRATA_GEO_DISABLE_IP_FALLBACK=1` or `STRATA_EARTH_OFFLINE=1`).
//!
//! Hardware sensor probes (`probe_os_compass`, `probe_os_imu`, `probe_os_barometer`,
//! `probe_os_ambient_light`) read Linux IIO sysfs (`/sys/bus/iio/devices/iio:device*`),
//! macOS IOKit / SMC, and Windows Sensor API with clean mock overrides and directory-injectable
//! parsers for deterministic cross-platform testing.

use std::fs;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
#[cfg(target_os = "linux")]
use std::time::Duration;

use strata_web_api::geolocation::{
    parse_env_lat_lon, parse_ip_geolocation_json, GeoCoordinates, Geolocation as WebGeolocation,
    GeolocationOptions, GeolocationPosition, GeolocationSource as WebGeolocationSource,
};

use crate::mock::DeviceMockController;
use crate::types::{
    AmbientLightReading, BarometerReading, CompassReading, GeolocationInfo, GeolocationSource,
    ImuReading,
};

static IP_GEO_CACHE: OnceLock<Mutex<Option<Option<GeolocationInfo>>>> = OnceLock::new();
static WEB_API_BRIDGE_INSTALLED: OnceLock<()> = OnceLock::new();

fn ip_cache() -> &'static Mutex<Option<Option<GeolocationInfo>>> {
    IP_GEO_CACHE.get_or_init(|| Mutex::new(None))
}

/// Clears the process-wide cached HTTPS GeoIP fallback entry (primarily for tests).
pub fn clear_ip_geolocation_cache() {
    if let Ok(mut guard) = ip_cache().lock() {
        *guard = None;
    }
}

/// Registers `strata_device`'s cross-platform OS geolocation backend as the process-wide
/// provider for [`strata_web_api::Geolocation`].
pub fn register_web_api_geolocation_provider() {
    WEB_API_BRIDGE_INSTALLED.get_or_init(|| {
        WebGeolocation::register_os_provider(web_api_os_geolocation_provider);
    });
}

fn web_api_os_geolocation_provider(options: &GeolocationOptions) -> Option<GeolocationPosition> {
    let info = probe_os_geolocation(options.allow_ip_fallback)?;
    let mut coords = GeoCoordinates::new(info.latitude, info.longitude, info.accuracy_m);
    coords.altitude = info.altitude_m;
    coords.heading = info.heading_deg;
    coords.speed = info.speed_mps;
    let src = match info.source {
        GeolocationSource::MockOverride => WebGeolocationSource::MockOverride,
        GeolocationSource::EnvOverride => WebGeolocationSource::EnvOverride,
        GeolocationSource::HardwareGpsNmea => WebGeolocationSource::NmeaGpsSerial,
        GeolocationSource::IpGeolocationFallback => WebGeolocationSource::HttpsIpFallback,
        GeolocationSource::OsNativeDaemon | GeolocationSource::None => {
            #[cfg(target_os = "linux")]
            {
                WebGeolocationSource::LinuxGeoClue2
            }
            #[cfg(target_os = "macos")]
            {
                WebGeolocationSource::MacOsCoreLocation
            }
            #[cfg(target_os = "windows")]
            {
                WebGeolocationSource::WindowsWinRt
            }
            #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
            {
                WebGeolocationSource::LinuxGeoClue2
            }
        }
    };
    Some(GeolocationPosition::with_source(coords, src))
}

/// Probes host geolocation synchronously following the priority chain:
/// Mock override -> `STRATA_GEO_LAT_LON` -> OS native GPS/GeoClue2/CoreLocation/WinRT ->
/// optional cached HTTPS IP fallback.
pub fn probe_os_geolocation(allow_ip_fallback: bool) -> Option<GeolocationInfo> {
    register_web_api_geolocation_provider();

    // 1. Deterministic mock override
    let mock = DeviceMockController::new().geolocation();
    if mock.is_available && mock.source == GeolocationSource::MockOverride {
        return Some(mock);
    }

    // 2. Environment override: STRATA_GEO_LAT_LON="lat,lon[,alt_m,accuracy_m]"
    if let Ok(raw) = std::env::var("STRATA_GEO_LAT_LON") {
        if let Some(coords) = parse_env_lat_lon(&raw) {
            return Some(
                GeolocationInfo::available(
                    coords.latitude,
                    coords.longitude,
                    coords.accuracy,
                    GeolocationSource::EnvOverride,
                )
                .with_altitude(coords.altitude)
                .with_motion(coords.heading, coords.speed),
            );
        }
    }

    // 3. OS native geolocation / GPS daemon
    if let Some(native) = probe_platform_native_geolocation() {
        return Some(native);
    }

    // 4. Optional cached HTTPS IP fallback (never runs when offline/disabled)
    if allow_ip_fallback && !ip_fallback_env_disabled() {
        return probe_cached_ip_geolocation();
    }

    None
}

/// Spawns a non-blocking background worker thread to probe OS geolocation without stalling UI startup.
pub fn probe_os_geolocation_async<F>(allow_ip_fallback: bool, on_complete: F)
where
    F: FnOnce(Option<GeolocationInfo>) + Send + 'static,
{
    let _ = std::thread::Builder::new()
        .name("strata-geo-probe".into())
        .spawn(move || {
            let result = probe_os_geolocation(allow_ip_fallback);
            on_complete(result);
        });
}

fn ip_fallback_env_disabled() -> bool {
    std::env::var_os("STRATA_GEO_DISABLE_IP_FALLBACK").is_some()
        || std::env::var_os("STRATA_EARTH_OFFLINE").is_some()
}

fn probe_cached_ip_geolocation() -> Option<GeolocationInfo> {
    if let Ok(guard) = ip_cache().lock() {
        if let Some(cached) = guard.as_ref() {
            return cached.clone();
        }
    }

    let fetched = fetch_ip_geolocation_https();
    if let Ok(mut guard) = ip_cache().lock() {
        *guard = Some(fetched.clone());
    }
    fetched
}

fn fetch_ip_geolocation_https() -> Option<GeolocationInfo> {
    const ENDPOINTS: &[&str] = &["https://ipwho.is/", "https://ipapi.co/json/"];
    for url in ENDPOINTS {
        let mut init = strata_web_api::RequestInit::default();
        init.timeout_ms = Some(1800);
        init.headers.append("User-Agent", "strata-device/0.1");
        init.headers.append("Accept", "application/json");
        if let Ok(resp) = strata_web_api::fetch(url, Some(init)) {
            if resp.ok() {
                if let Ok(body) = resp.text() {
                    if let Some(coords) = parse_ip_geolocation_json(&body) {
                        return Some(GeolocationInfo::available(
                            coords.latitude,
                            coords.longitude,
                            coords.accuracy,
                            GeolocationSource::IpGeolocationFallback,
                        ));
                    }
                }
            }
        }
    }
    None
}

fn probe_platform_native_geolocation() -> Option<GeolocationInfo> {
    #[cfg(target_os = "linux")]
    {
        probe_linux_geolocation()
    }
    #[cfg(target_os = "macos")]
    {
        probe_macos_geolocation()
    }
    #[cfg(target_os = "windows")]
    {
        probe_windows_geolocation()
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        None
    }
}

// ---------------------------------------------------------------------------
// Linux Geolocation: gpsd, NMEA-0183 serial GPS, GeoClue2 D-Bus, XDG Portal
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
fn probe_linux_geolocation() -> Option<GeolocationInfo> {
    // a) Local gpsd daemon (non-blocking 80ms connect timeout)
    if let Some(fix) = probe_linux_gpsd() {
        return Some(fix);
    }

    // b) Direct NMEA-0183 serial GPS device (only if explicitly configured via STRATA_GPS_DEVICE)
    if let Ok(dev_path) = std::env::var("STRATA_GPS_DEVICE") {
        if let Some(fix) = probe_nmea_file(Path::new(&dev_path)) {
            return Some(fix);
        }
    }

    // c) GeoClue2 D-Bus service (org.freedesktop.GeoClue2)
    if let Some(fix) = probe_linux_geoclue2_dbus() {
        return Some(fix);
    }

    // d) XDG Desktop Portal Location (org.freedesktop.portal.Location)
    probe_linux_xdg_portal_location()
}

#[cfg(target_os = "linux")]
fn probe_linux_gpsd() -> Option<GeolocationInfo> {
    use std::io::{BufRead, BufReader, Write};
    use std::net::{SocketAddr, TcpStream};

    let addr: SocketAddr = "127.0.0.1:2947".parse().ok()?;
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_millis(80)).ok()?;
    let _ = stream.set_read_timeout(Some(Duration::from_millis(120)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(80)));
    let _ = stream.write_all(b"?WATCH={\"enable\":true,\"json\":true};\n");
    let _ = stream.flush();

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    for _ in 0..8 {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                if let Some(coords) = strata_web_api::parse_gpsd_tpv_json(&line) {
                    return Some(
                        GeolocationInfo::available(
                            coords.latitude,
                            coords.longitude,
                            coords.accuracy,
                            GeolocationSource::HardwareGpsNmea,
                        )
                        .with_altitude(coords.altitude)
                        .with_motion(coords.heading, coords.speed),
                    );
                }
            }
        }
    }
    None
}

/// Reads up to 16 lines from an NMEA-0183 serial device or capture file and returns the first valid fix.
pub fn probe_nmea_file(path: &Path) -> Option<GeolocationInfo> {
    use std::io::{BufRead, BufReader};

    let file = fs::File::open(path).ok()?;
    let mut reader = BufReader::new(file);
    let mut line = String::new();
    for _ in 0..16 {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                if let Some(coords) = strata_web_api::parse_nmea_sentence(&line) {
                    return Some(
                        GeolocationInfo::available(
                            coords.latitude,
                            coords.longitude,
                            coords.accuracy,
                            GeolocationSource::HardwareGpsNmea,
                        )
                        .with_altitude(coords.altitude)
                        .with_motion(coords.heading, coords.speed),
                    );
                }
            }
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn run_command_with_timeout(program: &str, args: &[&str], timeout: Duration) -> Option<String> {
    use std::process::{Command, Stdio};
    use std::time::Instant;

    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return None;
                }
                let out = child.wait_with_output().ok()?;
                return String::from_utf8(out.stdout).ok();
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(Duration::from_millis(12));
            }
            Err(_) => return None,
        }
    }
}

#[cfg(target_os = "linux")]
fn probe_linux_geoclue2_dbus() -> Option<GeolocationInfo> {
    // Acquire a client path from org.freedesktop.GeoClue2.Manager
    let client_raw = run_command_with_timeout(
        "gdbus",
        &[
            "call",
            "--system",
            "--dest",
            "org.freedesktop.GeoClue2",
            "--object-path",
            "/org/freedesktop/GeoClue2/Manager",
            "--method",
            "org.freedesktop.GeoClue2.Manager.GetClient",
        ],
        Duration::from_millis(250),
    )?;
    let client_path = extract_dbus_object_path(&client_raw)?;

    // Identify desktop app (GeoClue2 requires DesktopId before Start)
    let _ = run_command_with_timeout(
        "gdbus",
        &[
            "call",
            "--system",
            "--dest",
            "org.freedesktop.GeoClue2",
            "--object-path",
            &client_path,
            "--method",
            "org.freedesktop.DBus.Properties.Set",
            "org.freedesktop.GeoClue2.Client",
            "DesktopId",
            "<'strata'>",
        ],
        Duration::from_millis(150),
    );
    let _ = run_command_with_timeout(
        "gdbus",
        &[
            "call",
            "--system",
            "--dest",
            "org.freedesktop.GeoClue2",
            "--object-path",
            &client_path,
            "--method",
            "org.freedesktop.GeoClue2.Client.Start",
        ],
        Duration::from_millis(250),
    );

    let loc_prop = run_command_with_timeout(
        "gdbus",
        &[
            "call",
            "--system",
            "--dest",
            "org.freedesktop.GeoClue2",
            "--object-path",
            &client_path,
            "--method",
            "org.freedesktop.DBus.Properties.Get",
            "org.freedesktop.GeoClue2.Client",
            "Location",
        ],
        Duration::from_millis(200),
    );

    let fix = loc_prop
        .as_deref()
        .and_then(extract_dbus_object_path)
        .filter(|p| p != "/")
        .and_then(|loc_path| {
            let props = run_command_with_timeout(
                "gdbus",
                &[
                    "call",
                    "--system",
                    "--dest",
                    "org.freedesktop.GeoClue2",
                    "--object-path",
                    &loc_path,
                    "--method",
                    "org.freedesktop.DBus.Properties.GetAll",
                    "org.freedesktop.GeoClue2.Location",
                ],
                Duration::from_millis(200),
            )?;
            let coords = strata_web_api::parse_geoclue_dbus_output(&props)?;
            Some(
                GeolocationInfo::available(
                    coords.latitude,
                    coords.longitude,
                    coords.accuracy,
                    GeolocationSource::OsNativeDaemon,
                )
                .with_altitude(coords.altitude)
                .with_motion(coords.heading, coords.speed),
            )
        });

    let _ = run_command_with_timeout(
        "gdbus",
        &[
            "call",
            "--system",
            "--dest",
            "org.freedesktop.GeoClue2",
            "--object-path",
            &client_path,
            "--method",
            "org.freedesktop.GeoClue2.Client.Stop",
        ],
        Duration::from_millis(120),
    );

    fix
}

#[cfg(target_os = "linux")]
fn probe_linux_xdg_portal_location() -> Option<GeolocationInfo> {
    // Check if XDG Desktop Portal Location interface exposes a cached/active fix
    let out = run_command_with_timeout(
        "gdbus",
        &[
            "call",
            "--session",
            "--dest",
            "org.freedesktop.portal.Desktop",
            "--object-path",
            "/org/freedesktop/portal/desktop",
            "--method",
            "org.freedesktop.DBus.Properties.GetAll",
            "org.freedesktop.portal.Location",
        ],
        Duration::from_millis(200),
    )?;
    let coords = strata_web_api::parse_geoclue_dbus_output(&out)?;
    Some(
        GeolocationInfo::available(
            coords.latitude,
            coords.longitude,
            coords.accuracy,
            GeolocationSource::OsNativeDaemon,
        )
        .with_altitude(coords.altitude)
        .with_motion(coords.heading, coords.speed),
    )
}

#[cfg(target_os = "linux")]
fn extract_dbus_object_path(raw: &str) -> Option<String> {
    let start = raw.find('/')?;
    let rest = &raw[start..];
    let end = rest
        .find(|c: char| matches!(c, '\'' | '"' | ')' | ',' | '>' | ' ' | '\n' | '\r'))
        .unwrap_or(rest.len());
    let path = &rest[..end];
    if path.starts_with('/') {
        Some(path.to_string())
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// macOS CoreLocation & Windows WinRT Geolocator Probes
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
fn probe_macos_geolocation() -> Option<GeolocationInfo> {
    use std::process::Command;

    // Lightweight CoreLocation JXA/Swift bridge (bounded invocation, fails gracefully if denied)
    let script = concat!(
        "import CoreLocation, Foundation\n",
        "class D: NSObject, CLLocationManagerDelegate {\n",
        "  func locationManager(_ m: CLLocationManager, didUpdateLocations l: [CLLocation]) {\n",
        "    if let p = l.last { print(\"\\(p.coordinate.latitude),\\(p.coordinate.longitude),\\(p.altitude),\\(p.horizontalAccuracy)\"); exit(0) }\n",
        "  }\n",
        "  func locationManager(_ m: CLLocationManager, didFailWithError e: Error) { exit(1) }\n",
        "}\n",
        "let m = CLLocationManager(); let d = D(); m.delegate = d; m.requestLocation()\n",
        "RunLoop.main.run(until: Date(timeIntervalSinceNow: 1.2))\n"
    );
    let out = Command::new("swift").args(["-e", script]).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8(out.stdout).ok()?;
    let coords = parse_env_lat_lon(text.trim())?;
    Some(
        GeolocationInfo::available(
            coords.latitude,
            coords.longitude,
            coords.accuracy,
            GeolocationSource::OsNativeDaemon,
        )
        .with_altitude(coords.altitude),
    )
}

#[cfg(target_os = "windows")]
fn probe_windows_geolocation() -> Option<GeolocationInfo> {
    use std::process::Command;

    let ps = concat!(
        "Add-Type -AssemblyName System.Device; ",
        "$w = New-Object System.Device.Location.GeoCoordinateWatcher; ",
        "$w.TryStart($false, [TimeSpan]::FromMilliseconds(1200)) | Out-Null; ",
        "$c = $w.Position.Location; ",
        "if (-not $c.IsUnknown) { Write-Output \"$($c.Latitude),$($c.Longitude),$($c.Altitude),$($c.HorizontalAccuracy)\" }"
    );
    let out = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", ps])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8(out.stdout).ok()?;
    let coords = parse_env_lat_lon(text.trim())?;
    Some(
        GeolocationInfo::available(
            coords.latitude,
            coords.longitude,
            coords.accuracy,
            GeolocationSource::OsNativeDaemon,
        )
        .with_altitude(coords.altitude),
    )
}

// ---------------------------------------------------------------------------
// Cross-Platform Hardware Sensors: Compass, IMU, Barometer, Ambient Light
// ---------------------------------------------------------------------------

/// Probes host magnetometer / compass hardware (returns mock override first if active).
pub fn probe_os_compass() -> Option<CompassReading> {
    let mock = DeviceMockController::new().compass();
    if mock.available {
        return Some(mock);
    }
    #[cfg(target_os = "linux")]
    {
        probe_linux_iio_compass_in_dir(Path::new("/sys/bus/iio/devices"))
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

/// Probes host 6-DoF IMU (accelerometer + gyroscope) hardware (returns mock override first if active).
pub fn probe_os_imu() -> Option<ImuReading> {
    let mock = DeviceMockController::new().imu();
    if mock.available {
        return Some(mock);
    }
    #[cfg(target_os = "linux")]
    {
        probe_linux_iio_imu_in_dir(Path::new("/sys/bus/iio/devices"))
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

/// Probes host barometric pressure sensor (returns mock override first if active).
pub fn probe_os_barometer() -> Option<BarometerReading> {
    let mock = DeviceMockController::new().barometer();
    if mock.available {
        return Some(mock);
    }
    #[cfg(target_os = "linux")]
    {
        probe_linux_iio_barometer_in_dir(Path::new("/sys/bus/iio/devices"))
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

/// Probes host ambient light sensor (returns mock override first if active).
pub fn probe_os_ambient_light() -> Option<AmbientLightReading> {
    let mock = DeviceMockController::new().ambient_light();
    if mock.available {
        return Some(mock);
    }
    #[cfg(target_os = "linux")]
    {
        probe_linux_iio_ambient_light_in_dir(Path::new("/sys/bus/iio/devices"))
            .or_else(probe_linux_applesmc_light)
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

/// Refreshes geolocation + all hardware sensor reactive signals from live OS probes.
/// Returns `true` if at least one live sensor or geolocation fix was updated.
pub fn refresh_sensors_from_hardware(allow_ip_fallback: bool) -> bool {
    let mock = DeviceMockController::new();
    let mut any = false;
    if let Some(geo) = probe_os_geolocation(allow_ip_fallback) {
        mock.set_geolocation(geo);
        any = true;
    }
    if let Some(c) = probe_os_compass() {
        mock.set_compass(c);
        any = true;
    }
    if let Some(imu) = probe_os_imu() {
        mock.set_imu(imu);
        any = true;
    }
    if let Some(baro) = probe_os_barometer() {
        mock.set_barometer(baro);
        any = true;
    }
    if let Some(als) = probe_os_ambient_light() {
        mock.set_ambient_light(als);
        any = true;
    }
    any
}

// ---------------------------------------------------------------------------
// Linux IIO Sysfs Parsers (Directory-Injectable for 100% Deterministic Testing)
// ---------------------------------------------------------------------------

fn read_f32_file(path: &Path) -> Option<f32> {
    fs::read_to_string(path).ok()?.trim().parse::<f32>().ok().filter(|v| v.is_finite())
}

fn iter_iio_device_dirs(root: &Path) -> Vec<std::path::PathBuf> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut dirs: Vec<std::path::PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_dir()
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("iio:device") || n.starts_with("iio_device"))
        })
        .collect();
    dirs.sort();
    dirs
}

/// Parses a Linux IIO magnetometer/compass from `iio_root` (typically `/sys/bus/iio/devices`).
///
/// Linux IIO magnetometers expose `in_magn_{x,y,z}_raw` with `in_magn_scale` in Gauss
/// (`1 Gauss = 100 uT`) or `in_rot_from_north_magnetic_tilt_comp_raw` + scale in degrees.
pub fn probe_linux_iio_compass_in_dir(iio_root: &Path) -> Option<CompassReading> {
    for dev in iter_iio_device_dirs(iio_root) {
        if let Some(raw_hdg) = read_f32_file(&dev.join("in_rot_from_north_magnetic_tilt_comp_raw"))
        {
            let scale =
                read_f32_file(&dev.join("in_rot_from_north_magnetic_tilt_comp_scale")).unwrap_or(1.0);
            let hdg = (raw_hdg * scale).rem_euclid(360.0);
            return Some(CompassReading {
                available: true,
                magnetic_heading_deg: hdg,
                true_heading_deg: hdg,
                heading_accuracy_deg: 5.0,
                magnetic_field_ut: [0.0; 3],
            });
        }

        let Some(rx) = read_f32_file(&dev.join("in_magn_x_raw")) else {
            continue;
        };
        let Some(ry) = read_f32_file(&dev.join("in_magn_y_raw")) else {
            continue;
        };
        let Some(rz) = read_f32_file(&dev.join("in_magn_z_raw")) else {
            continue;
        };
        // Linux IIO scale converts raw -> Gauss; multiply by 100.0 for microteslas (uT).
        let scale = read_f32_file(&dev.join("in_magn_scale")).unwrap_or(0.001);
        let mag_ut = [rx * scale * 100.0, ry * scale * 100.0, rz * scale * 100.0];
        return Some(CompassReading::from_magnetic_field_ut(mag_ut, 0.0));
    }
    None
}

/// Parses a Linux IIO accelerometer + optional gyroscope from `iio_root` (typically `/sys/bus/iio/devices`).
pub fn probe_linux_iio_imu_in_dir(iio_root: &Path) -> Option<ImuReading> {
    let mut accel: Option<[f32; 3]> = None;
    let mut gyro: Option<[f32; 3]> = None;

    for dev in iter_iio_device_dirs(iio_root) {
        if accel.is_none() {
            if let (Some(ax), Some(ay), Some(az)) = (
                read_f32_file(&dev.join("in_accel_x_raw")),
                read_f32_file(&dev.join("in_accel_y_raw")),
                read_f32_file(&dev.join("in_accel_z_raw")),
            ) {
                let scale = read_f32_file(&dev.join("in_accel_scale")).unwrap_or(1.0);
                accel = Some([ax * scale, ay * scale, az * scale]);
            }
        }
        if gyro.is_none() {
            if let (Some(gx), Some(gy), Some(gz)) = (
                read_f32_file(&dev.join("in_anglvel_x_raw")),
                read_f32_file(&dev.join("in_anglvel_y_raw")),
                read_f32_file(&dev.join("in_anglvel_z_raw")),
            ) {
                let scale = read_f32_file(&dev.join("in_anglvel_scale")).unwrap_or(1.0);
                gyro = Some([gx * scale, gy * scale, gz * scale]);
            }
        }
    }

    match (accel, gyro) {
        (Some(a), Some(g)) => Some(ImuReading::from_accel_gyro(a, g)),
        (Some(a), None) => Some(ImuReading::from_accel_gyro(a, [0.0; 3])),
        (None, Some(g)) => Some(ImuReading::from_accel_gyro([0.0, 0.0, 9.80665], g)),
        (None, None) => None,
    }
}

/// Parses a Linux IIO barometric pressure sensor from `iio_root` (`in_pressure_input` or
/// `in_pressure_raw * in_pressure_scale` in kPa -> hPa via `* 10.0`).
pub fn probe_linux_iio_barometer_in_dir(iio_root: &Path) -> Option<BarometerReading> {
    for dev in iter_iio_device_dirs(iio_root) {
        let kpa = read_f32_file(&dev.join("in_pressure_input")).or_else(|| {
            let raw = read_f32_file(&dev.join("in_pressure_raw"))?;
            let scale = read_f32_file(&dev.join("in_pressure_scale")).unwrap_or(1.0);
            Some(raw * scale)
        });
        if let Some(p_kpa) = kpa {
            // IIO pressure units are kPa; 1 kPa = 10 hPa. Guard against drivers already reporting Pa/hPa.
            let hpa = if p_kpa > 2000.0 {
                p_kpa / 100.0
            } else if p_kpa > 200.0 {
                p_kpa
            } else {
                p_kpa * 10.0
            };
            let temp_c = read_f32_file(&dev.join("in_temp_input"))
                .map(|milli_c| if milli_c.abs() > 200.0 { milli_c / 1000.0 } else { milli_c })
                .or_else(|| {
                    let raw = read_f32_file(&dev.join("in_temp_raw"))?;
                    let scale = read_f32_file(&dev.join("in_temp_scale")).unwrap_or(1.0);
                    let offset = read_f32_file(&dev.join("in_temp_offset")).unwrap_or(0.0);
                    let v = (raw + offset) * scale;
                    Some(if v.abs() > 200.0 { v / 1000.0 } else { v })
                });
            return Some(BarometerReading::from_hpa(hpa, temp_c));
        }
    }
    None
}

/// Parses a Linux IIO ambient light sensor (`in_illuminance_input` or `in_illuminance_raw * scale`).
pub fn probe_linux_iio_ambient_light_in_dir(iio_root: &Path) -> Option<AmbientLightReading> {
    for dev in iter_iio_device_dirs(iio_root) {
        if let Some(lux) = read_f32_file(&dev.join("in_illuminance_input")) {
            return Some(AmbientLightReading::from_lux(lux));
        }
        if let Some(raw) = read_f32_file(&dev.join("in_illuminance_raw")) {
            let scale = read_f32_file(&dev.join("in_illuminance_scale")).unwrap_or(1.0);
            return Some(AmbientLightReading::from_lux(raw * scale));
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn probe_linux_applesmc_light() -> Option<AmbientLightReading> {
    let raw = fs::read_to_string("/sys/devices/platform/applesmc.768/light").ok()?;
    // Format: "(left,right)" e.g. "(120,118)"
    let trimmed = raw.trim().trim_start_matches('(').trim_end_matches(')');
    let first = trimmed.split(',').next()?.trim().parse::<f32>().ok()?;
    Some(AmbientLightReading::from_lux(first))
}

// ---------------------------------------------------------------------------
// Phase 8 Expanded Hardware Sensor Probes & Cycle-Free Provider Bridges
// ---------------------------------------------------------------------------

use crate::types::{
    AmbientSplReading, HeartRateReading, HingeAngleReading, PedometerReading, ProximityReading,
    RadioSignalReading,
};

type AmbientSplProviderFn = fn() -> Option<f32>;
type HeartRateProviderFn = fn() -> Option<u16>;

static AMBIENT_SPL_PROVIDER: OnceLock<Mutex<Option<AmbientSplProviderFn>>> = OnceLock::new();
static HEART_RATE_PROVIDER: OnceLock<Mutex<Option<HeartRateProviderFn>>> = OnceLock::new();

/// Registers a cycle-free process-wide ambient SPL (`dBA`) provider (e.g., `strata_audio::latest_ambient_spl_dba`).
pub fn register_ambient_spl_provider(provider: Option<AmbientSplProviderFn>) {
    let slot = AMBIENT_SPL_PROVIDER.get_or_init(|| Mutex::new(None));
    if let Ok(mut guard) = slot.lock() {
        *guard = provider;
    }
}

/// Registers a cycle-free process-wide heart-rate BPM provider (e.g., `strata_hardware::latest_ble_heart_rate_bpm`).
pub fn register_heart_rate_provider(provider: Option<HeartRateProviderFn>) {
    let slot = HEART_RATE_PROVIDER.get_or_init(|| Mutex::new(None));
    if let Ok(mut guard) = slot.lock() {
        *guard = provider;
    }
}

/// Probes ambient SPL (`dBA`) via mock override or registered audio provider.
pub fn probe_os_ambient_spl() -> Option<AmbientSplReading> {
    let mock = DeviceMockController::new().ambient_spl();
    if mock.available {
        return Some(mock);
    }
    let provider = AMBIENT_SPL_PROVIDER
        .get_or_init(|| Mutex::new(None))
        .lock()
        .ok()
        .and_then(|g| *g);
    let dba = provider?()?;
    Some(AmbientSplReading::from_dba(dba))
}

/// Probes heart-rate BPM via mock override or registered BLE/wearable provider.
pub fn probe_os_heart_rate() -> Option<HeartRateReading> {
    let mock = DeviceMockController::new().heart_rate();
    if mock.available {
        return Some(mock);
    }
    let provider = HEART_RATE_PROVIDER
        .get_or_init(|| Mutex::new(None))
        .lock()
        .ok()
        .and_then(|g| *g);
    let bpm = provider?()?;
    Some(HeartRateReading {
        available: true,
        bpm,
        sensor_contact: true,
    })
}

/// Parses a Linux IIO proximity sensor (`in_proximity_raw` / `in_proximity_input`) in `iio_root`.
pub fn probe_linux_iio_proximity_in_dir(iio_root: &Path) -> Option<ProximityReading> {
    for dev in iter_iio_device_dirs(iio_root) {
        if let Some(raw) = read_f32_file(&dev.join("in_proximity_raw"))
            .or_else(|| read_f32_file(&dev.join("in_proximity_input")))
        {
            let scale = read_f32_file(&dev.join("in_proximity_scale")).unwrap_or(1.0);
            let val = (raw * scale).max(0.0);
            // IIO proximity values vary by chip; treat small cm distances (< 5 cm) or high raw counts as near.
            let is_near = val <= 5.0;
            return Some(ProximityReading {
                available: true,
                is_near,
                distance_cm: val,
            });
        }
    }
    None
}

/// Parses a Linux IIO hinge/lid angle sensor (`in_angl_raw` * `in_angl_scale`) in `iio_root`.
pub fn probe_linux_iio_hinge_angle_in_dir(iio_root: &Path) -> Option<HingeAngleReading> {
    for dev in iter_iio_device_dirs(iio_root) {
        if let Some(raw) = read_f32_file(&dev.join("in_angl_raw"))
            .or_else(|| read_f32_file(&dev.join("in_angl0_raw")))
        {
            let scale = read_f32_file(&dev.join("in_angl_scale")).unwrap_or(1.0);
            let rad_or_deg = raw * scale;
            // IIO reports angles in radians when scale < 0.05; convert radians to degrees cleanly.
            let deg = if scale < 0.05 && rad_or_deg.abs() <= std::f32::consts::TAU + 0.1 {
                rad_or_deg.to_degrees()
            } else {
                rad_or_deg
            };
            return Some(HingeAngleReading::from_degrees(deg));
        }
    }
    None
}

/// Parses a Linux IIO step counter / pedometer (`in_steps_input`) in `iio_root`.
pub fn probe_linux_iio_pedometer_in_dir(iio_root: &Path) -> Option<PedometerReading> {
    for dev in iter_iio_device_dirs(iio_root) {
        if let Some(steps_f) = read_f32_file(&dev.join("in_steps_input")) {
            return Some(PedometerReading {
                available: true,
                steps: steps_f.max(0.0) as u64,
                cadence_spm: 0.0,
            });
        }
    }
    None
}

/// Parses Linux `/proc/net/wireless` content into a [`RadioSignalReading`].
pub fn parse_proc_net_wireless(content: &str) -> Option<RadioSignalReading> {
    for line in content.lines().skip(2) {
        let trimmed = line.trim();
        let Some((iface, rest)) = trimmed.split_once(':') else {
            continue;
        };
        let cols: Vec<&str> = rest.split_whitespace().collect();
        if cols.len() >= 3 {
            // Columns: status, link quality (0..70), signal level (dBm, often negative with trailing '.')
            let link_raw: f32 = cols[1].trim_end_matches('.').parse().ok()?;
            let level_raw: f32 = cols[2].trim_end_matches('.').parse().ok()?;
            let rssi_dbm = if level_raw > 0.0 && level_raw <= 255.0 {
                (level_raw as i32) - 256
            } else {
                level_raw.round() as i32
            };
            let quality_pct = ((link_raw / 70.0) * 100.0).clamp(0.0, 100.0);
            return Some(RadioSignalReading {
                available: true,
                wifi_interface: Some(iface.trim().to_string()),
                wifi_rssi_dbm: Some(rssi_dbm),
                wifi_link_quality_pct: Some(quality_pct),
                cellular_rssi_dbm: None,
            });
        }
    }
    None
}

/// Probes host Wi-Fi RSSI & radio signal strength (mock override -> `/proc/net/wireless` on Linux).
pub fn probe_os_radio_signal() -> Option<RadioSignalReading> {
    let mock = DeviceMockController::new().radio_signal();
    if mock.available {
        return Some(mock);
    }
    #[cfg(target_os = "linux")]
    if let Ok(text) = fs::read_to_string("/proc/net/wireless") {
        if let Some(r) = parse_proc_net_wireless(&text) {
            return Some(r);
        }
    }
    None
}

/// Probes host proximity sensor via mock override or Linux IIO sysfs.
pub fn probe_os_proximity() -> Option<ProximityReading> {
    let mock = DeviceMockController::new().proximity();
    if mock.available {
        return Some(mock);
    }
    #[cfg(target_os = "linux")]
    if let Some(r) = probe_linux_iio_proximity_in_dir(Path::new("/sys/bus/iio/devices")) {
        return Some(r);
    }
    None
}

/// Probes host foldable / laptop lid hinge angle via mock override, Linux IIO, or ACPI lid state.
pub fn probe_os_hinge_angle() -> Option<HingeAngleReading> {
    let mock = DeviceMockController::new().hinge_angle();
    if mock.available {
        return Some(mock);
    }
    #[cfg(target_os = "linux")]
    {
        if let Some(r) = probe_linux_iio_hinge_angle_in_dir(Path::new("/sys/bus/iio/devices")) {
            return Some(r);
        }
        if let Ok(entries) = fs::read_dir("/proc/acpi/button/lid") {
            for entry in entries.flatten() {
                if let Ok(txt) = fs::read_to_string(entry.path().join("state")) {
                    if txt.contains("closed") {
                        return Some(HingeAngleReading::from_degrees(0.0));
                    } else if txt.contains("open") {
                        return Some(HingeAngleReading::from_degrees(115.0));
                    }
                }
            }
        }
    }
    None
}

/// Probes host hardware pedometer via mock override or Linux IIO sysfs.
pub fn probe_os_pedometer() -> Option<PedometerReading> {
    let mock = DeviceMockController::new().pedometer();
    if mock.available {
        return Some(mock);
    }
    #[cfg(target_os = "linux")]
    if let Some(r) = probe_linux_iio_pedometer_in_dir(Path::new("/sys/bus/iio/devices")) {
        return Some(r);
    }
    None
}
