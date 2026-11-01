//! Strata Web API: W3C Web Platform APIs in Pure Idiomatic Rust.

pub mod clipboard;
pub mod fetch;
pub mod geolocation;
pub mod websocket;

pub use clipboard::{Clipboard, ClipboardError};
pub use fetch::{
    fetch, parse_http_response, AbortController, AbortSignal, FetchError, FormData, Headers,
    ParsedUrl, RequestInit, Response,
};
pub use geolocation::{
    haversine_distance_meters, initial_bearing_degrees, GeoCoordinates, Geolocation,
    GeolocationError, GeolocationOptions, GeolocationPosition,
};
pub use websocket::{
    decode_frame, encode_frame, WebSocket, WsError, WsFrame, WsMessage, WsOpcode, WsState,
};

pub mod prelude {
    pub use super::clipboard::{Clipboard, ClipboardError};
    pub use super::fetch::{
        fetch, AbortController, AbortSignal, FetchError, FormData, Headers, RequestInit, Response,
    };
    pub use super::geolocation::{
        haversine_distance_meters, GeoCoordinates, Geolocation, GeolocationError,
        GeolocationPosition,
    };
    pub use super::websocket::{WebSocket, WsError, WsMessage, WsOpcode, WsState};
}
