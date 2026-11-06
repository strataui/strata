//! Geodesy, camera control, and tile quadtree for the Earth scene plugin.
//!
//! Coordinates are WGS84-ish ECEF metres in f64. Nothing here narrows to f32;
//! the render hand-off is [`crate::EarthScene::build`].

use strata_render::camera64::{DVec3, EARTH_RADIUS_M};

/// WGS84 ellipsoid constants.
pub mod wgs84 {
    pub const A: f64 = 6_378_137.0;
    pub const F: f64 = 1.0 / 298.257_223_563;
    pub const B: f64 = A * (1.0 - F);
    pub const E2: f64 = F * (2.0 - F - F * F);
}

/// WGS84 geodetic position.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Geodetic {
    /// Latitude in degrees, positive north.
    pub lat_deg: f64,
    /// Longitude in degrees, positive east.
    pub lon_deg: f64,
    /// Height above the ellipsoid in metres.
    pub height_m: f64,
}

impl Geodetic {
    pub const fn new(lat_deg: f64, lon_deg: f64, height_m: f64) -> Self {
        Self {
            lat_deg,
            lon_deg,
            height_m,
        }
    }
}

/// Converts a geodetic position to ECEF metres.
pub fn geodetic_to_ecef(position: Geodetic) -> DVec3 {
    let lat = position.lat_deg.to_radians();
    let lon = position.lon_deg.to_radians();
    let (sin_lat, cos_lat) = lat.sin_cos();
    let (sin_lon, cos_lon) = lon.sin_cos();

    // Radius of curvature in the prime vertical.
    let n = wgs84::A / (1.0 - wgs84::E2 * sin_lat * sin_lat).sqrt();
    let x = (n + position.height_m) * cos_lat * cos_lon;
    let y = (n + position.height_m) * cos_lat * sin_lon;
    let z = (n * (1.0 - wgs84::E2) + position.height_m) * sin_lat;
    DVec3::new(x, y, z)
}

/// The local east/north/up basis at a geodetic position, in ECEF.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LocalFrame {
    pub east: DVec3,
    pub north: DVec3,
    pub up: DVec3,
}

impl LocalFrame {
    /// Computes the ENU basis at the given latitude and longitude.
    pub fn at(lat_deg: f64, lon_deg: f64) -> Self {
        let lat = lat_deg.to_radians();
        let lon = lon_deg.to_radians();
        let (sin_lat, cos_lat) = lat.sin_cos();
        let (sin_lon, cos_lon) = lon.sin_cos();
        Self {
            east: DVec3::new(-sin_lon, cos_lon, 0.0),
            north: DVec3::new(-sin_lat * cos_lon, -sin_lat * sin_lon, cos_lat),
            up: DVec3::new(cos_lat * cos_lon, cos_lat * sin_lon, sin_lat),
        }
    }
}

/// Camera control for the globe, expressed as a ground focus point plus a
/// geodesic eye offset in the local frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EarthCamera {
    /// Point on the globe the camera looks at.
    pub focus: Geodetic,
    /// Aim point offset, metres east, north, and up from `focus`.
    pub aim_offset_m: [f64; 3],
    /// Eye offset from `focus`, metres east, north, and up.
    ///
    /// The default points north along the tangent, so the camera hovers
    /// `altitude` above the ground with the local up as its screen up.
    pub eye_offset_m: [f64; 3],
    /// Vertical field of view in degrees.
    pub fov_y_deg: f64,
    /// Near plane in metres.
    pub near_m: f64,
}

impl Default for EarthCamera {
    fn default() -> Self {
        Self {
            focus: Geodetic::new(0.0, 0.0, 0.0),
            aim_offset_m: [0.0, 0.0, 0.0],
            eye_offset_m: [0.0, 0.0, 3.0 * EARTH_RADIUS_M],
            fov_y_deg: 60.0,
            near_m: 1.0,
        }
    }
}

/// A resolved eye/aim pair in ECEF with the far plane set beyond the globe.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraPlacement {
    pub eye: DVec3,
    pub aim: DVec3,
    pub up: DVec3,
    pub fov_y_deg: f64,
    pub near_m: f64,
    pub far_m: f64,
}

impl EarthCamera {
    /// Distance from the focus point to the eye in metres.
    pub fn eye_distance_m(&self) -> f64 {
        let [e, n, u] = self.eye_offset_m;
        (e * e + n * n + u * u).sqrt()
    }

    /// Altitude of the eye above the ellipsoid in metres.
    ///
    /// A mostly-vertical offset uses its up component; a horizontal component
    /// additionally lifts the eye above the tangent plane through the focus.
    pub fn altitude_m(&self) -> f64 {
        let [e, n, u] = self.eye_offset_m;
        let horizontal = (e * e + n * n).sqrt();
        let chord = (horizontal * horizontal + u * u).sqrt();
        // A horizontal offset is a chord, so it lifts the eye by
        // chord^2 / (2R) above the focus tangent plane.
        let lift = if chord > f64::EPSILON {
            chord * chord / (2.0 * EARTH_RADIUS_M)
        } else {
            0.0
        };
        (u + lift).max(0.0) + self.focus.height_m.max(0.0)
    }

    /// Ground width covered by the horizontal field of view, in metres.
    ///
    /// The texture rasterizer keeps only the frame around each triangle, so
    /// terrain patches are sized from this so they stay partly on screen.
    pub fn ground_span_m(&self) -> f64 {
        let altitude = self.altitude_m().max(1e-3);
        2.0 * altitude * (self.fov_y_deg.to_radians() * 0.5).tan()
    }

    /// Resolves the camera to ECEF positions.
    pub fn placement(&self) -> CameraPlacement {
        let anchor = geodetic_to_ecef(self.focus);
        let frame = LocalFrame::at(self.focus.lat_deg, self.focus.lon_deg);
        let offset = |[e, n, u]: [f64; 3]| {
            frame
                .east
                .scale(e)
                .add(frame.north.scale(n))
                .add(frame.up.scale(u))
        };
        let eye = anchor.add(offset(self.eye_offset_m));
        let aim = anchor.add(offset(self.aim_offset_m));
        // The camera's up vector is the local up, so the globe's north stays
        // up in the frame.
        let up = frame.up;
        let far = self.eye_distance_m() + 8.0 * EARTH_RADIUS_M;
        CameraPlacement {
            eye,
            aim,
            up,
            fov_y_deg: self.fov_y_deg,
            near_m: self.near_m,
            far_m: far,
        }
    }
}

/// A node in the tile quadtree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TileKey {
    /// Zoom level, 0 = whole world.
    pub level: u8,
    pub x: u32,
    pub y: u32,
}

impl TileKey {
    pub const fn new(level: u8, x: u32, y: u32) -> Self {
        Self { level, x, y }
    }

    /// Number of tiles per axis at this level.
    pub fn tiles_per_axis(&self) -> u32 {
        1u32 << self.level
    }

    /// The parent tile, or `None` at level 0.
    pub fn parent(&self) -> Option<Self> {
        if self.level == 0 {
            None
        } else {
            Some(Self::new(self.level - 1, self.x / 2, self.y / 2))
        }
    }

    /// The child quadrant containing this tile's `(u, v)` position.
    pub fn child(&self, quadrant: u8) -> Option<Self> {
        if self.level >= 30 {
            return None;
        }
        let (dx, dy) = (
            ((quadrant & 1) != 0) as u32,
            (((quadrant >> 1) & 1) != 0) as u32,
        );
        let base = Self::new(self.level + 1, self.x * 2, self.y * 2);
        let per_axis = base.tiles_per_axis();
        let x = base.x + dx;
        let y = base.y + dy;
        if x < per_axis && y < per_axis {
            Some(Self::new(base.level, x, y))
        } else {
            None
        }
    }

    /// Maps a normalized `(u, v)` inside this tile into the parent's space.
    pub fn uv_in_parent(&self, u: f64, v: f64) -> (f64, f64) {
        let x_parity = self.x & 1;
        let y_parity = self.y & 1;
        ((u + x_parity as f64) * 0.5, (v + y_parity as f64) * 0.5)
    }
}

/// Resolves the sub-rectangle of a loaded ancestor image that a tile shows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TileTextureResolution {
    /// Tile whose image should be sampled.
    pub source: TileKey,
    /// Sub-rectangle of the source image in `0..1` UV space:
    /// `(u_min, v_min, u_max, v_max)`.
    pub uv_rect: (f64, f64, f64, f64),
    /// True when the requested tile itself was available.
    pub exact: bool,
}

/// Resolves which loaded tile supplies the imagery for `target`.
///
/// Walks ancestors nearest-first and accumulates the UV sub-rectangle in the
/// ancestor's space. `is_loaded` decides availability; the function is pure so
/// it can be tested without a cache or a renderer.
pub fn resolve_tile_imagery(
    target: TileKey,
    mut is_loaded: impl FnMut(TileKey) -> bool,
) -> Option<TileTextureResolution> {
    if is_loaded(target) {
        return Some(TileTextureResolution {
            source: target,
            uv_rect: (0.0, 0.0, 1.0, 1.0),
            exact: true,
        });
    }

    let (mut u_min, mut v_min, mut u_max, mut v_max) = (0.0f64, 0.0f64, 1.0f64, 1.0f64);
    let mut current = target;
    while let Some(parent) = current.parent() {
        let (pu_min, pv_min) = current.uv_in_parent(u_min, v_min);
        let (pu_max, pv_max) = current.uv_in_parent(u_max, v_max);
        u_min = pu_min;
        v_min = pv_min;
        u_max = pu_max;
        v_max = pv_max;
        current = parent;
        if is_loaded(current) {
            return Some(TileTextureResolution {
                source: current,
                uv_rect: (u_min, v_min, u_max, v_max),
                exact: false,
            });
        }
    }
    None
}

/// Picks the tile level for a camera altitude: one level per factor-of-two
/// reduction in eye height above the ground, clamped to `0..=max_level`.
pub fn level_for_altitude(altitude_m: f64, max_level: u8) -> u8 {
    if altitude_m <= 0.0 {
        return max_level;
    }
    // A whole-earth view sits around 20 000 km up; halving from there adds a
    // level each time.
    let reference = 20_000_000.0;
    let ratio = (reference / altitude_m.max(1.0)).log2();
    (ratio.max(0.0).round() as u8).min(max_level)
}

/// Wraps a longitude into `-180..180`.
pub fn wrap_longitude(lon_deg: f64) -> f64 {
    let wrapped = (lon_deg + 180.0).rem_euclid(360.0) - 180.0;
    wrapped
}

/// Clamps a latitude to the polar limit.
pub fn clamp_latitude(lat_deg: f64) -> f64 {
    lat_deg.clamp(-90.0, 90.0)
}
