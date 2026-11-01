//! W3C Geolocation API with Coordinate Math and Great-Circle Distance Solving.

use std::fmt;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

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
}

impl GeolocationPosition {
    pub fn new(coords: GeoCoordinates) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        Self { coords, timestamp }
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
}

impl Default for GeolocationOptions {
    fn default() -> Self {
        Self {
            enable_high_accuracy: false,
            timeout_ms: 10000,
            maximum_age_ms: 0,
        }
    }
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

    /// Configures or injects a mock coordinate (e.g. for testing or headless simulation).
    pub fn set_mock_position(&self, coords: GeoCoordinates) {
        if let Ok(mut pos) = self.current_position.write() {
            *pos = Some(GeolocationPosition::new(coords));
        }
    }

    /// Retrieves the current device position snapshot.
    pub fn get_current_position(&self) -> Result<GeolocationPosition, GeolocationError> {
        let pos = self
            .current_position
            .read()
            .map_err(|e| GeolocationError::PositionUnavailable(e.to_string()))?;

        pos.clone().ok_or_else(|| {
            GeolocationError::PositionUnavailable("No position available from device provider".into())
        })
    }
}

/// Calculates great-circle distance between two WGS84 coordinates using the Haversine formula.
/// Returns distance in meters.
pub fn haversine_distance_meters(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    const EARTH_RADIUS_METERS: f64 = 6_371_000.0;

    let d_lat = (lat2 - lat1).to_radians();
    let d_lon = (lon2 - lon1).to_radians();

    let rad_lat1 = lat1.to_radians();
    let rad_lat2 = lat2.to_radians();

    let a = (d_lat / 2.0).sin().powi(2)
        + rad_lat1.cos() * rad_lat2.cos() * (d_lon / 2.0).sin().powi(2);

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
