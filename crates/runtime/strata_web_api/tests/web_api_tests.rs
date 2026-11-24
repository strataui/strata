use strata_web_api::prelude::*;
use strata_web_api::{decode_frame, encode_frame, parse_http_response, ParsedUrl, WsOpcode};

#[test]
fn test_fetch_response_parsing() {
    let raw = b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nX-Custom-Header: StrataTest\r\n\r\n{\"status\":\"ok\"}";
    let res = parse_http_response(raw).expect("Failed to parse HTTP response");

    assert_eq!(res.status, 200);
    assert!(res.ok());
    assert_eq!(res.headers.get("content-type"), Some("application/json"));
    assert_eq!(res.headers.get("x-custom-header"), Some("StrataTest"));
    assert_eq!(res.text().unwrap(), "{\"status\":\"ok\"}");
}

#[test]
fn test_abort_controller_and_url_parsing() {
    let controller = AbortController::new();
    let signal = controller.signal();
    assert!(!signal.is_aborted());

    controller.abort();
    assert!(signal.is_aborted());

    let parsed = ParsedUrl::parse("https://api.strata.dev:8443/v1/metrics?view=compact").unwrap();
    assert_eq!(parsed.scheme, "https");
    assert_eq!(parsed.host, "api.strata.dev");
    assert_eq!(parsed.port, 8443);
    assert_eq!(parsed.path_and_query, "/v1/metrics?view=compact");
}

#[test]
fn test_form_data_payload_generation() {
    let mut form = FormData::new();
    form.append("username", "alice");
    form.append("role", "admin");

    assert_eq!(form.get("username"), Some("alice"));
    assert_eq!(form.get("role"), Some("admin"));

    let payload = form.to_multipart_payload("boundary123");
    let payload_str = String::from_utf8(payload).unwrap();
    assert!(payload_str.contains(
        "--boundary123\r\nContent-Disposition: form-data; name=\"username\"\r\n\r\nalice\r\n"
    ));
    assert!(payload_str.contains("--boundary123--\r\n"));
}

#[test]
fn test_websocket_frame_roundtrip() {
    let original_text = "Hello Strata WebSocket Engine!";
    let mask_key = [0xAA, 0xBB, 0xCC, 0xDD];
    let encoded = encode_frame(
        true,
        WsOpcode::Text,
        original_text.as_bytes(),
        true,
        Some(mask_key),
    );

    let (frame, consumed) = decode_frame(&encoded)
        .expect("Decode error")
        .expect("Incomplete frame");

    assert_eq!(consumed, encoded.len());
    assert!(frame.fin);
    assert_eq!(frame.opcode, WsOpcode::Text);
    assert_eq!(String::from_utf8(frame.payload).unwrap(), original_text);
}

#[test]
fn test_websocket_extended_payload_frame() {
    // 300 bytes triggers 16-bit length encoding (indicator = 126)
    let large_payload = vec![0x42u8; 300];
    let encoded = encode_frame(true, WsOpcode::Binary, &large_payload, false, None);

    assert_eq!(encoded[1], 126); // length indicator
    let (frame, consumed) = decode_frame(&encoded).unwrap().unwrap();
    assert_eq!(consumed, encoded.len());
    assert_eq!(frame.payload.len(), 300);
    assert_eq!(frame.payload, large_payload);
}

#[test]
fn test_websocket_client_lifecycle() {
    let ws = WebSocket::new("wss://echo.strata.dev");
    assert_eq!(ws.ready_state(), WsState::Connecting);

    ws.set_open();
    assert_eq!(ws.ready_state(), WsState::Open);

    ws.send_text("ping message").unwrap();
    let frames = ws.drain_outgoing_frames();
    assert_eq!(frames.len(), 1);

    // Ingest simulated server response
    let server_msg = encode_frame(true, WsOpcode::Text, b"pong message", false, None);
    ws.ingest_incoming_bytes(&server_msg).unwrap();

    let msg = ws.poll_message().expect("Should have received message");
    match msg {
        WsMessage::Text(s) => assert_eq!(s, "pong message"),
        other => panic!("Expected text message, got {:?}", other),
    }

    ws.close(1000, "Normal closure").unwrap();
    assert_eq!(ws.ready_state(), WsState::Closed);
}

#[test]
fn test_clipboard_operations() {
    let clip = Clipboard::new();
    clip.write_text("Copied to Strata Clipboard").unwrap();
    assert_eq!(clip.read_text().unwrap(), "Copied to Strata Clipboard");

    clip.write_data("application/json", b"{\"key\":\"val\"}")
        .unwrap();
    assert_eq!(
        clip.read_data("application/json").unwrap(),
        Some(b"{\"key\":\"val\"}".to_vec())
    );

    clip.clear().unwrap();
    assert_eq!(clip.read_text().unwrap(), "");
    assert_eq!(clip.read_data("application/json").unwrap(), None);
}

#[test]
fn test_geolocation_and_haversine() {
    let geo = Geolocation::new();
    let coords = GeoCoordinates::new(37.7749, -122.4194, 5.0); // San Francisco
    geo.set_mock_position(coords.clone());

    let pos = geo.get_current_position().unwrap();
    assert_eq!(pos.coords.latitude, 37.7749);
    assert_eq!(pos.coords.longitude, -122.4194);
    assert_eq!(pos.coords.accuracy, 5.0);

    // Distance between London (51.5074, -0.1278) and Paris (48.8566, 2.3522)
    // Approximately 343 km = 343,000 meters
    let distance = strata_web_api::haversine_distance_meters(51.5074, -0.1278, 48.8566, 2.3522);
    assert!((distance - 343_550.0).abs() < 5000.0); // within 5km accuracy
}

#[test]
fn test_chunked_transfer_decoding() {
    let raw = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4\r\nWiki\r\n6\r\npedia \r\n8\r\nArticles\r\n0\r\n\r\n";
    let res = parse_http_response(raw).expect("Failed to parse chunked response");
    assert_eq!(res.text().unwrap(), "Wikipedia Articles");
}

#[test]
fn test_gzip_deflate_brotli_decompression() {
    use std::io::Write;
    let original = "Strata high-performance compressed stream engine 2026";

    // 1. Gzip
    let mut gz_encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gz_encoder.write_all(original.as_bytes()).unwrap();
    let gz_bytes = gz_encoder.finish().unwrap();

    let decomp_gz = strata_web_api::decompress_body(&gz_bytes, "gzip").unwrap();
    assert_eq!(String::from_utf8(decomp_gz).unwrap(), original);

    // 2. Deflate
    let mut def_encoder =
        flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    def_encoder.write_all(original.as_bytes()).unwrap();
    let def_bytes = def_encoder.finish().unwrap();

    let decomp_def = strata_web_api::decompress_body(&def_bytes, "deflate").unwrap();
    assert_eq!(String::from_utf8(decomp_def).unwrap(), original);

    // 3. Brotli
    let mut br_bytes = Vec::new();
    let params = brotli::enc::BrotliEncoderParams::default();
    brotli::BrotliCompress(&mut original.as_bytes(), &mut br_bytes, &params).unwrap();

    let decomp_br = strata_web_api::decompress_body(&br_bytes, "br").unwrap();
    assert_eq!(String::from_utf8(decomp_br).unwrap(), original);
}

#[test]
fn test_w3c_cache_storage_and_cache_control() {
    let storage = CacheStorage::new();
    let cache = storage.open("v1-assets");

    let mut headers = Headers::new();
    headers.set("Cache-Control", "public, max-age=3600");
    headers.set("ETag", "\"123456\"");
    let response = Response::new(200, "OK", headers, b"Cached stylesheet content".to_vec());

    let stored = cache
        .put("https://cdn.strata.dev/style.css", response)
        .unwrap();
    assert!(stored);

    let matched = cache
        .match_url("https://cdn.strata.dev/style.css")
        .expect("Cache hit expected");
    assert_eq!(matched.text().unwrap(), "Cached stylesheet content");

    // no-store must NOT be stored in cache
    let mut no_store_headers = Headers::new();
    no_store_headers.set("Cache-Control", "no-store");
    let no_store_res = Response::new(200, "OK", no_store_headers, b"Private user data".to_vec());
    let stored_no_store = cache
        .put("https://api.strata.dev/user/private", no_store_res)
        .unwrap();
    assert!(!stored_no_store);
    assert!(cache
        .match_url("https://api.strata.dev/user/private")
        .is_none());
}

#[test]
fn test_nmea_gpsd_geoclue_geoip_and_env_decoders() {
    use strata_web_api::{
        parse_env_lat_lon, parse_geoclue_dbus_output, parse_gpsd_tpv_json,
        parse_ip_geolocation_json, parse_nmea_sentence,
    };

    // 1. NMEA-0183 $GPGGA valid checksum (San Francisco: 37°46.4940'N, 122°25.1640'W, alt 52.4m)
    let gga_body = "GPGGA,123519,3746.4940,N,12225.1640,W,1,08,0.9,52.4,M,46.9,M,,";
    let cksum = gga_body.bytes().fold(0u8, |a, b| a ^ b);
    let gga = format!("${gga_body}*{cksum:02X}");
    let c_gga = parse_nmea_sentence(&gga).expect("Valid $GPGGA must parse");
    assert!((c_gga.latitude - 37.7749).abs() < 1e-4);
    assert!((c_gga.longitude - (-122.4194)).abs() < 1e-4);
    assert!((c_gga.altitude.unwrap() - 52.4).abs() < 1e-3);
    assert!((c_gga.accuracy - 4.5).abs() < 1e-3);

    // Corrupted checksum must be rejected
    let bad_gga = format!("${gga_body}*{:02X}", cksum ^ 0xFF);
    assert!(parse_nmea_sentence(&bad_gga).is_none());

    // 2. NMEA-0183 $GNRMC active fix with speed (knots -> m/s) and heading
    let rmc_body = "GNRMC,123519,A,4100.4920,N,02858.7040,E,10.0,135.5,230326,003.1,W";
    let rmc_ck = rmc_body.bytes().fold(0u8, |a, b| a ^ b);
    let rmc = format!("${rmc_body}*{rmc_ck:02X}");
    let c_rmc = parse_nmea_sentence(&rmc).expect("Valid $GNRMC must parse");
    assert!((c_rmc.latitude - 41.0082).abs() < 1e-4);
    assert!((c_rmc.longitude - 28.9784).abs() < 1e-4);
    assert!((c_rmc.speed.unwrap() - 5.14444).abs() < 1e-3);
    assert!((c_rmc.heading.unwrap() - 135.5).abs() < 1e-3);

    // Void RMC fix ('V') rejected
    assert!(parse_nmea_sentence("$GNRMC,123519,V,4100.4920,N,02858.7040,E,0,0,230326,,").is_none());

    // 3. gpsd TPV JSON
    let tpv = r#"{"class":"TPV","mode":3,"lat":46.5775,"lon":7.9850,"altMSL":1240.5,"epx":3.0,"epy":4.0,"speed":12.2,"track":275.0}"#;
    let c_tpv = parse_gpsd_tpv_json(tpv).expect("gpsd mode=3 TPV must parse");
    assert!((c_tpv.latitude - 46.5775).abs() < 1e-6);
    assert!((c_tpv.longitude - 7.9850).abs() < 1e-6);
    assert!((c_tpv.altitude.unwrap() - 1240.5).abs() < 1e-3);
    assert!((c_tpv.accuracy - 5.0).abs() < 1e-3);
    assert!(parse_gpsd_tpv_json(r#"{"class":"TPV","mode":1}"#).is_none());

    // 4. GeoClue2 D-Bus GetAll output
    let dbus_out = "({'Latitude': <48.8566>, 'Longitude': <2.3522>, 'Accuracy': <25.0>, 'Altitude': <35.0>, 'Speed': <1.5>, 'Heading': <90.0>},)";
    let c_gc = parse_geoclue_dbus_output(dbus_out).expect("GeoClue2 D-Bus payload must parse");
    assert!((c_gc.latitude - 48.8566).abs() < 1e-6);
    assert!((c_gc.longitude - 2.3522).abs() < 1e-6);
    assert!((c_gc.accuracy - 25.0).abs() < 1e-3);
    assert!((c_gc.altitude.unwrap() - 35.0).abs() < 1e-3);

    // 5. GeoIP JSON schemas
    let ipwho = r#"{"success":true,"latitude":35.6762,"longitude":139.6503}"#;
    let c_ip1 = parse_ip_geolocation_json(ipwho).expect("ipwho.is JSON must parse");
    assert!((c_ip1.latitude - 35.6762).abs() < 1e-4);
    assert!((c_ip1.longitude - 139.6503).abs() < 1e-4);

    let ipinfo = r#"{"loc":"51.5074,-0.1278"}"#;
    let c_ip2 = parse_ip_geolocation_json(ipinfo).expect("ipinfo loc JSON must parse");
    assert!((c_ip2.latitude - 51.5074).abs() < 1e-4);
    assert!((c_ip2.longitude - (-0.1278)).abs() < 1e-4);

    assert!(parse_ip_geolocation_json(r#"{"success":false,"latitude":10.0,"longitude":20.0}"#).is_none());
    assert!(parse_ip_geolocation_json(r#"{"latitude":0.0,"longitude":0.0}"#).is_none());

    // 6. Env lat/lon parser
    let env_c = parse_env_lat_lon("37.7749, -122.4194, 42.0, 8.5").expect("Env lat/lon must parse");
    assert!((env_c.latitude - 37.7749).abs() < 1e-6);
    assert!((env_c.longitude - (-122.4194)).abs() < 1e-6);
    assert_eq!(env_c.altitude, Some(42.0));
    assert!((env_c.accuracy - 8.5).abs() < 1e-6);
    assert!(parse_env_lat_lon("95.0, 10.0").is_none());
}

#[test]
fn test_geolocation_os_provider_bridge_and_mock_precedence() {
    use strata_web_api::{Geolocation, GeolocationOptions, GeolocationPosition, GeolocationSource};

    fn fake_os_provider(opts: &GeolocationOptions) -> Option<GeolocationPosition> {
        let src = if opts.allow_ip_fallback {
            GeolocationSource::HttpsIpFallback
        } else {
            GeolocationSource::LinuxGeoClue2
        };
        Some(GeolocationPosition::with_source(
            GeoCoordinates::new(39.9334, 32.8597, 30.0),
            src,
        ))
    }

    Geolocation::register_os_provider(fake_os_provider);
    let geo = Geolocation::new();

    // Without mock override, resolves through registered OS provider
    let os_pos = geo.get_current_position().expect("OS provider fix");
    assert!((os_pos.coords.latitude - 39.9334).abs() < 1e-6);
    assert_eq!(os_pos.source, GeolocationSource::HttpsIpFallback);

    let opts = GeolocationOptions {
        allow_ip_fallback: false,
        ..Default::default()
    };
    let native_pos = geo
        .get_current_position_with_options(&opts)
        .expect("OS provider fix with ip fallback disabled");
    assert_eq!(native_pos.source, GeolocationSource::LinuxGeoClue2);

    // Instance mock takes top precedence over OS provider
    geo.set_mock_position(GeoCoordinates::new(40.7128, -74.0060, 5.0));
    let mock_pos = geo.get_current_position().expect("Mock fix");
    assert!((mock_pos.coords.latitude - 40.7128).abs() < 1e-6);
    assert_eq!(mock_pos.source, GeolocationSource::MockOverride);

    Geolocation::clear_os_provider();
}

