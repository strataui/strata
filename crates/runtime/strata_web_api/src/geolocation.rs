//! W3C Geolocation API with Coordinate Math, OS Provider Bridge, NMEA-0183 / GeoClue2 / GeoIP
//! payload decoders, and Great-Circle Distance Solving.

use std::fmt;
use std::sync::{Arc, OnceLock, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

/// Origin source that produced a [`GeolocationPosition`] fix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GeolocationSource {
    #[default]
    MockOverride,
    EnvOverride,
    LinuxGeoClue2,
    LinuxPortal,
    NmeaGpsSerial,
    GpsdDaemon,
    MacOsCoreLocation,
    WindowsWinRt,
    HttpsIpFallback,
}

/// W3C Geographic coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct GeoCoordinates {
    pub latitude: f64,
    pub longitude: f64,
    pub altitude: Option<f64>,
    pub accuracy: f64,
    pub altitude_accuracy: Option<f64>,
    pub heading: Option<f64>,
    pub speed: Option<f64>,
}

impl GeoCoordinates {
    pub fn new(latitude: f64, longitude: f64, accuracy: f64) -> Self {
        Self {
            latitude,
            longitude,
            altitude: None,
            accuracy,
            altitude_accuracy: None,
            heading: None,
            speed: None,
        }
    }
}

/// A complete W3C Geolocation position snapshot.
#[derive(Debug, Clone, PartialEq)]
pub struct GeolocationPosition {
    pub coords: GeoCoordinates,
    pub timestamp: u64,
    pub source: GeolocationSource,
}

impl GeolocationPosition {
    pub fn new(coords: GeoCoordinates) -> Self {
        Self::with_source(coords, GeolocationSource::MockOverride)
    }

    pub fn with_source(coords: GeoCoordinates, source: GeolocationSource) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        Self {
            coords,
            timestamp,
            source,
        }
    }
}

/// Errors occurring during geolocation lookups.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GeolocationError {
    PermissionDenied(String),
    PositionUnavailable(String),
    Timeout(String),
}

impl fmt::Display for GeolocationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PermissionDenied(s) => write!(f, "Geolocation permission denied: {s}"),
            Self::PositionUnavailable(s) => write!(f, "Position unavailable: {s}"),
            Self::Timeout(s) => write!(f, "Geolocation lookup timed out: {s}"),
        }
    }
}

impl std::error::Error for GeolocationError {}

/// Options for acquiring geolocation.
#[derive(Debug, Clone)]
pub struct GeolocationOptions {
    pub enable_high_accuracy: bool,
    pub timeout_ms: u64,
    pub maximum_age_ms: u64,
    /// Permit coarse HTTPS GeoIP fallback on desktop workstations lacking GPS/OS daemons.
    pub allow_ip_fallback: bool,
}

impl Default for GeolocationOptions {
    fn default() -> Self {
        Self {
            enable_high_accuracy: false,
            timeout_ms: 10000,
            maximum_age_ms: 0,
            allow_ip_fallback: true,
        }
    }
}

/// Callback signature for process-wide OS geolocation provider hooks (e.g. `strata_device`).
pub type OsGeolocationProvider = fn(&GeolocationOptions) -> Option<GeolocationPosition>;

static OS_GEO_PROVIDER: OnceLock<RwLock<Option<OsGeolocationProvider>>> = OnceLock::new();

fn os_provider_slot() -> &'static RwLock<Option<OsGeolocationProvider>> {
    OS_GEO_PROVIDER.get_or_init(|| RwLock::new(None))
}

/// Thread-safe Geolocation controller.
#[derive(Clone)]
pub struct Geolocation {
    current_position: Arc<RwLock<Option<GeolocationPosition>>>,
}

impl Default for Geolocation {
    fn default() -> Self {
        Self::new()
    }
}

impl Geolocation {
    pub fn new() -> Self {
        Self {
            current_position: Arc::new(RwLock::new(None)),
        }
    }

    /// Registers a process-wide OS geolocation provider consulted whenever no mock position
    /// has been injected on a [`Geolocation`] instance.
    pub fn register_os_provider(provider: OsGeolocationProvider) {
        if let Ok(mut slot) = os_provider_slot().write() {
            *slot = Some(provider);
        }
    }

    /// Clears any registered OS geolocation provider (primarily for test isolation).
    pub fn clear_os_provider() {
        if let Ok(mut slot) = os_provider_slot().write() {
            *slot = None;
        }
    }

    /// Configures or injects a mock coordinate (e.g. for testing or headless simulation).
    pub fn set_mock_position(&self, coords: GeoCoordinates) {
        if let Ok(mut pos) = self.current_position.write() {
            *pos = Some(GeolocationPosition::with_source(
                coords,
                GeolocationSource::MockOverride,
            ));
        }
    }

    /// Clears any mock coordinate previously injected on this controller.
    pub fn clear_mock_position(&self) {
        if let Ok(mut pos) = self.current_position.write() {
            *pos = None;
        }
    }

    /// Retrieves the current device position snapshot using default [`GeolocationOptions`].
    pub fn get_current_position(&self) -> Result<GeolocationPosition, GeolocationError> {
        self.get_current_position_with_options(&GeolocationOptions::default())
    }

    /// Retrieves the current device position snapshot respecting `options`.
    /// Checks instance mock position first, then the registered OS geolocation provider.
    pub fn get_current_position_with_options(
        &self,
        options: &GeolocationOptions,
    ) -> Result<GeolocationPosition, GeolocationError> {
        let mock_pos = self
            .current_position
            .read()
            .map_err(|e| GeolocationError::PositionUnavailable(e.to_string()))?
            .clone();

        if let Some(pos) = mock_pos {
            return Ok(pos);
        }

        let provider = os_provider_slot().read().ok().and_then(|g| *g);
        if let Some(provider_fn) = provider {
            if let Some(pos) = provider_fn(options) {
                return Ok(pos);
            }
        }

        Err(GeolocationError::PositionUnavailable(
            "No position available from device provider".into(),
        ))
    }
}

// ---------------------------------------------------------------------------
// Deterministic NMEA-0183, gpsd, GeoClue2, Env, and GeoIP Payload Decoders
// ---------------------------------------------------------------------------

/// Parses `"lat,lon"`, `"lat,lon,alt_m"`, or `"lat,lon,alt_m,accuracy_m"` (used for
/// `STRATA_GEO_LAT_LON` deterministic overrides).
pub fn parse_env_lat_lon(raw: &str) -> Option<GeoCoordinates> {
    let parts: Vec<&str> = raw.split(',').map(str::trim).collect();
    if parts.len() < 2 {
        return None;
    }
    let lat: f64 = parts[0].parse().ok()?;
    let lon: f64 = parts[1].parse().ok()?;
    if !is_valid_lat_lon(lat, lon) {
        return None;
    }
    let alt = parts.get(2).and_then(|s| s.parse::<f64>().ok());
    let acc = parts
        .get(3)
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|a| a.is_finite() && *a >= 0.0)
        .unwrap_or(15.0);

    let mut coords = GeoCoordinates::new(lat, lon, acc);
    coords.altitude = alt.filter(|v| v.is_finite());
    Some(coords)
}

/// Validates and parses an NMEA-0183 `$GPGGA` / `$GNGGA` or `$GPRMC` / `$GNRMC` sentence
/// into WGS84 [`GeoCoordinates`].
pub fn parse_nmea_sentence(sentence: &str) -> Option<GeoCoordinates> {
    let trimmed = sentence.trim();
    if !trimmed.starts_with('$') {
        return None;
    }

    let body = if let Some((payload, checksum_hex)) = trimmed[1..].split_once('*') {
        let expected = u8::from_str_radix(checksum_hex.trim().get(..2)?, 16).ok()?;
        let computed = payload.bytes().fold(0u8, |acc, b| acc ^ b);
        if computed != expected {
            return None;
        }
        payload
    } else {
        &trimmed[1..]
    };

    let fields: Vec<&str> = body.split(',').collect();
    let talker_msg = *fields.first()?;
    if talker_msg.len() < 5 {
        return None;
    }
    let msg_type = &talker_msg[talker_msg.len() - 3..];

    match msg_type {
        "GGA" => {
            // $GPGGA,hhmmss.ss,ddmm.mmmm,N,dddmm.mmmm,E,fix,sats,hdop,alt,M,...
            if fields.len() < 10 {
                return None;
            }
            let fix_quality: u8 = fields[6].parse().ok()?;
            if fix_quality == 0 {
                return None;
            }
            let lat = parse_nmea_deg_min(fields[2], fields[3], false)?;
            let lon = parse_nmea_deg_min(fields[4], fields[5], true)?;
            let hdop = fields[8].parse::<f64>().ok().filter(|v| *v > 0.0).unwrap_or(1.5);
            let accuracy = (hdop * 5.0).clamp(1.0, 250.0);
            let alt = fields[9].parse::<f64>().ok().filter(|v| v.is_finite());

            let mut coords = GeoCoordinates::new(lat, lon, accuracy);
            coords.altitude = alt;
            Some(coords)
        }
        "RMC" => {
            // $GPRMC,hhmmss.ss,A,ddmm.mmmm,N,dddmm.mmmm,E,knots,track_deg,ddmmyy,...
            if fields.len() < 9 || fields[2] != "A" {
                return None;
            }
            let lat = parse_nmea_deg_min(fields[3], fields[4], false)?;
            let lon = parse_nmea_deg_min(fields[5], fields[6], true)?;
            let speed_mps = fields[7]
                .parse::<f64>()
                .ok()
                .filter(|v| v.is_finite() && *v >= 0.0)
                .map(|knots| knots * 0.514_444);
            let heading = fields[8]
                .parse::<f64>()
                .ok()
                .filter(|v| v.is_finite())
                .map(|deg| deg.rem_euclid(360.0));

            let mut coords = GeoCoordinates::new(lat, lon, 10.0);
            coords.speed = speed_mps;
            coords.heading = heading;
            Some(coords)
        }
        _ => None,
    }
}

fn parse_nmea_deg_min(value: &str, hemi: &str, is_lon: bool) -> Option<f64> {
    let raw: f64 = value.trim().parse().ok()?;
    if !raw.is_finite() || raw < 0.0 {
        return None;
    }
    let degrees = (raw / 100.0).floor();
    let minutes = raw - degrees * 100.0;
    if minutes >= 60.0 {
        return None;
    }
    let mut decimal = degrees + minutes / 60.0;
    match hemi.trim() {
        "N" if !is_lon => {}
        "S" if !is_lon => decimal = -decimal,
        "E" if is_lon => {}
        "W" if is_lon => decimal = -decimal,
        _ => return None,
    }
    if (!is_lon && decimal.abs() <= 90.0) || (is_lon && decimal.abs() <= 180.0) {
        Some(decimal)
    } else {
        None
    }
}

/// Parses a `gpsd` JSON `TPV` report line into [`GeoCoordinates`].
pub fn parse_gpsd_tpv_json(line: &str) -> Option<GeoCoordinates> {
    let val: serde_json::Value = serde_json::from_str(line.trim()).ok()?;
    if val.get("class").and_then(|v| v.as_str()) != Some("TPV") {
        return None;
    }
    let mode = val.get("mode").and_then(|v| v.as_i64()).unwrap_or(0);
    if mode < 2 {
        return None;
    }
    let lat = val.get("lat").and_then(|v| v.as_f64())?;
    let lon = val.get("lon").and_then(|v| v.as_f64())?;
    if !is_valid_lat_lon(lat, lon) {
        return None;
    }
    let epx = val.get("epx").and_then(|v| v.as_f64()).unwrap_or(10.0);
    let epy = val.get("epy").and_then(|v| v.as_f64()).unwrap_or(10.0);
    let accuracy = epx.hypot(epy).clamp(1.0, 50_000.0);

    let mut coords = GeoCoordinates::new(lat, lon, accuracy);
    coords.altitude = val
        .get("altMSL")
        .or_else(|| val.get("alt"))
        .and_then(|v| v.as_f64())
        .filter(|v| v.is_finite());
    coords.speed = val
        .get("speed")
        .and_then(|v| v.as_f64())
        .filter(|v| v.is_finite() && *v >= 0.0);
    coords.heading = val
        .get("track")
        .and_then(|v| v.as_f64())
        .filter(|v| v.is_finite())
        .map(|d| d.rem_euclid(360.0));
    Some(coords)
}

/// Parses `gdbus call --method org.freedesktop.DBus.Properties.GetAll` or `key=value`
/// GeoClue2 / XDG Portal `Location` dictionary payloads into [`GeoCoordinates`].
pub fn parse_geoclue_dbus_output(text: &str) -> Option<GeoCoordinates> {
    let lat = extract_named_f64(text, "Latitude")?;
    let lon = extract_named_f64(text, "Longitude")?;
    if !is_valid_lat_lon(lat, lon) {
        return None;
    }
    let accuracy = extract_named_f64(text, "Accuracy")
        .filter(|v| v.is_finite() && *v >= 0.0)
        .unwrap_or(50.0);
    let altitude = extract_named_f64(text, "Altitude")
        .filter(|v| v.is_finite() && *v > -1.0e6);
    let speed = extract_named_f64(text, "Speed")
        .filter(|v| v.is_finite() && *v >= 0.0);
    let heading = extract_named_f64(text, "Heading")
        .filter(|v| v.is_finite() && *v >= 0.0)
        .map(|d| d.rem_euclid(360.0));

    let mut coords = GeoCoordinates::new(lat, lon, accuracy);
    coords.altitude = altitude;
    coords.speed = speed;
    coords.heading = heading;
    Some(coords)
}

fn extract_named_f64(text: &str, key: &str) -> Option<f64> {
    // Matches formats like `'Latitude': <37.7749>`, `"Latitude": 37.7749`, `Latitude = 37.7749`
    let idx = text.find(key)?;
    let rest = &text[idx + key.len()..];
    let bytes = rest.as_bytes();
    let mut pos = 0usize;
    while pos < bytes.len() {
        let b = bytes[pos];
        if b.is_ascii_digit() || b == b'-' || b == b'+' {
            // Skip numeric-looking characters inside keys/quotes only after separator
            let end = rest[pos..]
                .find(|c: char| !(c.is_ascii_digit() || matches!(c, '.' | '-' | '+' | 'e' | 'E')))
                .map(|rel| pos + rel)
                .unwrap_or(rest.len());
            if let Ok(val) = rest[pos..end].parse::<f64>() {
                return Some(val);
            }
            pos = end;
        } else if b == b'\n' || b == b',' || b == b'}' {
            break;
        } else {
            pos += 1;
        }
    }
    None
}

/// Parses common HTTPS GeoIP JSON payloads (`ipwho.is`, `ipapi.co`, `ipinfo.io`) into coarse
/// desktop [`GeoCoordinates`].
pub fn parse_ip_geolocation_json(json_str: &str) -> Option<GeoCoordinates> {
    let val: serde_json::Value = serde_json::from_str(json_str.trim()).ok()?;
    if val.get("success").and_then(|v| v.as_bool()) == Some(false) {
        return None;
    }
    if val.get("error").and_then(|v| v.as_bool()) == Some(true) {
        return None;
    }

    let (lat, lon) = if let (Some(la), Some(lo)) = (
        val.get("latitude").and_then(json_to_f64),
        val.get("longitude").and_then(json_to_f64),
    ) {
        (la, lo)
    } else if let (Some(la), Some(lo)) = (
        val.get("lat").and_then(json_to_f64),
        val.get("lon").and_then(json_to_f64),
    ) {
        (la, lo)
    } else if let Some(loc) = val.get("loc").and_then(|v| v.as_str()) {
        let (a, b) = loc.split_once(',')?;
        (a.trim().parse().ok()?, b.trim().parse().ok()?)
    } else {
        return None;
    };

    if !is_valid_lat_lon(lat, lon) || (lat.abs() < 1e-6 && lon.abs() < 1e-6) {
        return None;
    }
    let accuracy = val
        .get("accuracy")
        .and_then(json_to_f64)
        .filter(|v| v.is_finite() && *v > 0.0)
        .unwrap_or(5_000.0);
    Some(GeoCoordinates::new(lat, lon, accuracy))
}

fn json_to_f64(v: &serde_json::Value) -> Option<f64> {
    v.as_f64()
        .or_else(|| v.as_str().and_then(|s| s.trim().parse::<f64>().ok()))
}

#[inline]
fn is_valid_lat_lon(lat: f64, lon: f64) -> bool {
    lat.is_finite() && lon.is_finite() && (-90.0..=90.0).contains(&lat) && (-180.0..=180.0).contains(&lon)
}

/// Calculates great-circle distance between two WGS84 coordinates using the Haversine formula.
/// Returns distance in meters.
pub fn haversine_distance_meters(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    const EARTH_RADIUS_METERS: f64 = 6_371_000.0;

    let d_lat = (lat2 - lat1).to_radians();
    let d_lon = (lon2 - lon1).to_radians();

    let rad_lat1 = lat1.to_radians();
    let rad_lat2 = lat2.to_radians();

    let a =
        (d_lat / 2.0).sin().powi(2) + rad_lat1.cos() * rad_lat2.cos() * (d_lon / 2.0).sin().powi(2);

    let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());

    EARTH_RADIUS_METERS * c
}

/// Calculates the initial bearing (forward azimuth) from coordinate 1 to coordinate 2 in degrees [0, 360).
pub fn initial_bearing_degrees(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let d_lon = (lon2 - lon1).to_radians();
    let rad_lat1 = lat1.to_radians();
    let rad_lat2 = lat2.to_radians();

    let y = d_lon.sin() * rad_lat2.cos();
    let x = rad_lat1.cos() * rad_lat2.sin() - rad_lat1.sin() * rad_lat2.cos() * d_lon.cos();

    let bearing_rad = y.atan2(x);
    let bearing_deg = bearing_rad.to_degrees();

    (bearing_deg + 360.0) % 360.0
}
