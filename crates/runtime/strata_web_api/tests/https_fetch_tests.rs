use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;
use strata_web_api::fetch::{fetch, FetchError, Headers, ParsedUrl, RequestInit};
use strata_web_api::tls::get_or_init_tls_config;

#[test]
fn test_url_scheme_and_default_ports() {
    let http = ParsedUrl::parse("http://example.com/api/v1/users").expect("Valid HTTP URL");
    assert_eq!(http.scheme, "http");
    assert_eq!(http.host, "example.com");
    assert_eq!(http.port, 80);
    assert_eq!(http.path_and_query, "/api/v1/users");

    let https = ParsedUrl::parse("https://secure.example.com/dashboard?view=grid").expect("Valid HTTPS URL");
    assert_eq!(https.scheme, "https");
    assert_eq!(https.host, "secure.example.com");
    assert_eq!(https.port, 443);
    assert_eq!(https.path_and_query, "/dashboard?view=grid");

    let custom_port = ParsedUrl::parse("https://localhost:8443/ping").expect("Valid custom port URL");
    assert_eq!(custom_port.scheme, "https");
    assert_eq!(custom_port.host, "localhost");
    assert_eq!(custom_port.port, 8443);
    assert_eq!(custom_port.path_and_query, "/ping");

    let invalid_scheme = ParsedUrl::parse("ftp://ftp.example.com/file.txt");
    assert!(matches!(invalid_scheme, Err(FetchError::InvalidUrl(_))));
}

#[test]
fn test_tls_config_singleton_and_mozilla_roots() {
    let config = get_or_init_tls_config();
    // Verify ALPN protocol negotiation preference (HTTP/2 prioritized, falling back to HTTP/1.1)
    assert_eq!(config.alpn_protocols, vec![b"h2".to_vec(), b"http/1.1".to_vec()]);

    // Verify config is idempotent and shared across calls
    let config2 = get_or_init_tls_config();
    assert!(std::sync::Arc::ptr_eq(&config, &config2));
}

#[test]
fn test_live_rust_lang_org_fetch() {
    let resp = strata_web_api::fetch("https://www.rust-lang.org", None)
        .expect("Live HTTP/2 fetch to rust-lang.org should succeed");
    assert_eq!(resp.status, 200);
    assert_eq!(resp.status_text, "OK");
    assert!(resp.headers.get("content-type").unwrap_or("").contains("text/html"));
    assert!(!resp.bytes().is_empty());
}

#[test]
fn test_invalid_dns_host_error_reporting() {
    let init = RequestInit {
        timeout_ms: Some(500),
        ..Default::default()
    };

    let result = fetch("https://non-existent-subdomain-strata-unresolvable.invalid/test", Some(init));
    assert!(result.is_err(), "Expected DNS resolution failure");
    let err = result.unwrap_err();
    assert!(
        matches!(err, FetchError::NetworkError(_)),
        "Expected NetworkError on non-existent DNS host, got: {err:?}"
    );
}

#[test]
fn test_https_fetch_connection_timeout_handling() {
    let init = RequestInit {
        timeout_ms: Some(150),
        ..Default::default()
    };

    // 192.0.2.1 is TEST-NET-1 (RFC 5737), guaranteed unroutable
    let result = fetch("https://192.0.2.1:443/timeout", Some(init));
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, FetchError::NetworkError(_) | FetchError::Timeout(_) | FetchError::IoError(_)),
        "Expected timeout/network error, got: {err:?}"
    );
}

#[test]
fn test_mock_loopback_plain_http_request() {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").expect("Bind ephemeral loopback");
    let port = listener.local_addr().unwrap().port();

    let server_handle = std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0u8; 1024];
            let n = stream.read(&mut buf).unwrap_or(0);
            let req_str = String::from_utf8_lossy(&buf[..n]);
            assert!(req_str.contains("GET /hello HTTP/1.1"));
            assert!(req_str.to_ascii_lowercase().contains("user-agent: strata"));

            let response = "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 13\r\nConnection: close\r\n\r\nHello, Strata";
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });

    let mut headers = Headers::new();
    headers.set("User-Agent", "Strata");
    let init = RequestInit {
        headers,
        timeout_ms: Some(3000),
        ..Default::default()
    };

    let url = format!("http://127.0.0.1:{port}/hello");
    let resp = fetch(&url, Some(init)).expect("Successful loopback HTTP fetch");

    assert_eq!(resp.status, 200);
    assert_eq!(resp.status_text, "OK");
    assert_eq!(resp.text().expect("Valid UTF-8 body"), "Hello, Strata");

    server_handle.join().expect("Server thread finished");
}

#[test]
fn test_mock_loopback_chunked_gzip_response() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use flate2::write::GzEncoder;
    use flate2::Compression;

    let listener = TcpListener::bind("127.0.0.1:0").expect("Bind ephemeral loopback");
    let port = listener.local_addr().unwrap().port();

    let server_handle = std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);

            // Compress payload with Gzip
            let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(b"{\"status\":\"success\",\"engine\":\"strata_pure_rust\"}").unwrap();
            let gzipped_payload = encoder.finish().unwrap();

            // Format as HTTP/1.1 Chunked Transfer
            let mut response_bytes = Vec::new();
            response_bytes.extend_from_slice(b"HTTP/1.1 200 OK\r\n");
            response_bytes.extend_from_slice(b"Content-Type: application/json\r\n");
            response_bytes.extend_from_slice(b"Content-Encoding: gzip\r\n");
            response_bytes.extend_from_slice(b"Transfer-Encoding: chunked\r\n");
            response_bytes.extend_from_slice(b"Connection: close\r\n\r\n");

            // Chunk 1
            let hex_len = format!("{:x}\r\n", gzipped_payload.len());
            response_bytes.extend_from_slice(hex_len.as_bytes());
            response_bytes.extend_from_slice(&gzipped_payload);
            response_bytes.extend_from_slice(b"\r\n");

            // Final chunk
            response_bytes.extend_from_slice(b"0\r\n\r\n");

            let _ = stream.write_all(&response_bytes);
            let _ = stream.flush();
        }
    });

    let url = format!("http://127.0.0.1:{port}/chunked-json");
    let resp = fetch(&url, None).expect("Successful chunked fetch");

    assert_eq!(resp.status, 200);
    assert_eq!(
        resp.text().expect("Valid UTF-8 decoded JSON"),
        "{\"status\":\"success\",\"engine\":\"strata_pure_rust\"}"
    );

    server_handle.join().expect("Server thread finished");
}

#[test]
fn test_http11_keep_alive_without_connection_close_does_not_hang() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("Bind test server");
    let port = listener.local_addr().unwrap().port();

    let server_handle = thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);

            // Server responds with Content-Length and keep-alive, keeping the socket open
            let response = b"HTTP/1.1 200 OK\r\nContent-Length: 13\r\nConnection: keep-alive\r\n\r\nHello, World!";
            stream.write_all(response).unwrap();
            stream.flush().unwrap();

            // Do NOT close the socket — keep it open for 3 seconds
            thread::sleep(std::time::Duration::from_millis(3000));
        }
    });

    let start = std::time::Instant::now();
    let url = format!("http://127.0.0.1:{port}/keepalive");
    let resp = fetch(&url, None).expect("Keep-alive fetch should succeed");
    let elapsed = start.elapsed();

    assert_eq!(resp.status, 200);
    assert_eq!(resp.text().unwrap(), "Hello, World!");
    // Must complete in well under 1 second without waiting for the server socket to close (which takes 3s)
    assert!(
        elapsed < std::time::Duration::from_millis(500),
        "Fetch should terminate on Content-Length without waiting for keep-alive socket close, took {elapsed:?}"
    );

    server_handle.join().expect("Server thread finished");
}

