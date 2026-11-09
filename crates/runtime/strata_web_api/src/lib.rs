//! Strata Web API: W3C Web Platform APIs in Pure Idiomatic Rust.
//!
//! # Examples
//!
//! ```rust
//! use strata_web_api::cookie::CookieJar;
//!
//! let mut jar = CookieJar::new();
//! jar.process_set_cookie("session=xyz; Path=/", "https://example.com/api");
//! assert_eq!(jar.len(), 1);
//! ```

pub mod clipboard;
pub mod cookie;
pub mod fetch;
pub mod geolocation;
pub mod websocket;
pub mod cache;
pub mod tls;
pub mod huffman_table;
pub mod http2;

pub use cache::{Cache, CacheEntry, CacheStorage};
pub use clipboard::{Clipboard, ClipboardError};
pub use cookie::{
    can_set_cookie_domain, default_path_for_request, domain_matches, parse_http_date, path_matches,
    Cookie, CookieJar, SameSite,
};
pub use tls::{create_tls_stream, get_or_init_tls_config, HttpTransportStream};
pub use http2::{
    decode_hpack_headers, encode_hpack_headers, execute_http2_request, Http2Frame,
    FRAME_CONTINUATION, FRAME_DATA, FRAME_GOAWAY, FRAME_HEADERS, FRAME_PING, FRAME_PRIORITY,
    FRAME_PUSH_PROMISE, FRAME_RST_STREAM, FRAME_SETTINGS, FRAME_WINDOW_UPDATE, FLAG_ACK,
    FLAG_END_HEADERS, FLAG_END_STREAM,
};
pub use fetch::{
    compute_origin, decode_chunked_body, decompress_body, fetch, fetch_single,
    is_cors_simple_request, parse_http_response, resolve_redirect_url, AbortController,
    AbortSignal, FetchError, FormData, Headers, ParsedUrl, RequestCredentials, RequestInit,
    RequestMode, Response,
};
pub use geolocation::{
    haversine_distance_meters, initial_bearing_degrees, GeoCoordinates, Geolocation,
    GeolocationError, GeolocationOptions, GeolocationPosition,
};
pub use websocket::{
    base64_encode, compute_sec_websocket_accept, decode_frame, encode_frame,
    generate_sec_websocket_key, sha1, WebSocket, WsError, WsFrame, WsMessage, WsOpcode, WsState,
};

pub mod prelude {
    pub use super::cache::{Cache, CacheEntry, CacheStorage};
    pub use super::clipboard::{Clipboard, ClipboardError};
    pub use super::cookie::{Cookie, CookieJar, SameSite};
    pub use super::fetch::{
        fetch, AbortController, AbortSignal, FetchError, FormData, Headers, RequestCredentials,
        RequestInit, RequestMode, Response,
    };
    pub use super::geolocation::{
        haversine_distance_meters, GeoCoordinates, Geolocation, GeolocationError,
        GeolocationPosition,
    };
    pub use super::websocket::{WebSocket, WsError, WsMessage, WsOpcode, WsState};
}
