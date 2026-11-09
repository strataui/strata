//! W3C Cross-Origin Resource Sharing (CORS) Integration and Security Tests.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

use strata_web_api::fetch::{
    compute_origin, fetch, is_cors_simple_request, FetchError, RequestCredentials,
    RequestInit, RequestMode,
};

#[test]
fn test_compute_origin_normalization() {
    assert_eq!(
        compute_origin("http://example.com/api/v1").unwrap(),
        "http://example.com"
    );
    assert_eq!(
        compute_origin("http://example.com:80/path?query=1#frag").unwrap(),
        "http://example.com"
    );
    assert_eq!(
        compute_origin("https://secure.example.com:443/").unwrap(),
        "https://secure.example.com"
    );
    assert_eq!(
        compute_origin("https://api.example.com:8443/endpoint").unwrap(),
        "https://api.example.com:8443"
    );
    assert_eq!(
        compute_origin("http://localhost:3000/").unwrap(),
        "http://localhost:3000"
    );
}

#[test]
fn test_is_cors_simple_request() {
    let get_req = RequestInit::get();
    let (is_simple, headers) = is_cors_simple_request(&get_req);
    assert!(is_simple);
    assert!(headers.is_empty());

    let post_form = RequestInit::post().header(
        "Content-Type",
        "application/x-www-form-urlencoded; charset=utf-8",
    );
    let (is_simple, _) = is_cors_simple_request(&post_form);
    assert!(is_simple);

    let put_req = RequestInit::put();
    let (is_simple, _) = is_cors_simple_request(&put_req);
    assert!(!is_simple, "PUT must not be a simple request");

    let json_req = RequestInit::post().header("Content-Type", "application/json");
    let (is_simple, custom_headers) = is_cors_simple_request(&json_req);
    assert!(!is_simple, "application/json requires CORS preflight");
    assert_eq!(custom_headers, vec!["content-type"]);

    let custom_hdr_req = RequestInit::get().header("X-API-Key", "secret-token-12345");
    let (is_simple, custom_headers) = is_cors_simple_request(&custom_hdr_req);
    assert!(!is_simple, "Custom headers require CORS preflight");
    assert_eq!(custom_headers, vec!["x-api-key"]);
}

#[test]
fn test_cors_same_origin_policy_blocks_cross_origin() {
    let req = RequestInit::get()
        .origin("http://malicious.origin.test")
        .mode(RequestMode::SameOrigin);

    let result = fetch("http://127.0.0.1:8080/data", Some(req));
    match result {
        Err(FetchError::CorsError(msg)) => {
            assert!(msg.contains("SameOrigin policy"));
            assert!(msg.contains("malicious.origin.test"));
        }
        other => panic!("Expected CorsError from SameOrigin violation, got: {other:?}"),
    }
}

#[test]
fn test_cors_missing_allow_origin_rejected() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind ephemeral loopback");
    let port = listener.local_addr().unwrap().port();

    let server_handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = [0u8; 1024];
        let _ = stream.read(&mut buf).unwrap();

        let response = "HTTP/1.1 200 OK\r\n\
                        Content-Length: 13\r\n\
                        Content-Type: text/plain\r\n\
                        Connection: close\r\n\r\n\
                        Hello, World!";
        stream.write_all(response.as_bytes()).unwrap();
        stream.flush().unwrap();
    });

    let target_url = format!("http://127.0.0.1:{port}/secret-intranet");
    let req = RequestInit::get().origin("http://client.example.com");

    let result = fetch(&target_url, Some(req));
    server_handle.join().unwrap();

    match result {
        Err(FetchError::CorsError(msg)) => {
            assert!(msg.contains("missing 'Access-Control-Allow-Origin' header"));
        }
        other => panic!("Expected CorsError for missing header, got: {other:?}"),
    }
}

#[test]
fn test_cors_wildcard_origin_allowed_uncredentialed() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind ephemeral loopback");
    let port = listener.local_addr().unwrap().port();

    let server_handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = [0u8; 1024];
        let _ = stream.read(&mut buf).unwrap();

        let response = "HTTP/1.1 200 OK\r\n\
                        Access-Control-Allow-Origin: *\r\n\
                        Content-Length: 12\r\n\
                        Content-Type: text/plain\r\n\
                        Connection: close\r\n\r\n\
                        Public Data!";
        stream.write_all(response.as_bytes()).unwrap();
        stream.flush().unwrap();
    });

    let target_url = format!("http://127.0.0.1:{port}/public");
    let req = RequestInit::get()
        .origin("http://client.example.com")
        .credentials(RequestCredentials::SameOrigin);

    let resp = fetch(&target_url, Some(req)).expect("Wildcard CORS should succeed");
    server_handle.join().unwrap();

    assert_eq!(resp.status, 200);
    assert_eq!(resp.text().unwrap(), "Public Data!");
}

#[test]
fn test_cors_wildcard_rejected_with_include_credentials() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind ephemeral loopback");
    let port = listener.local_addr().unwrap().port();

    let server_handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = [0u8; 1024];
        let _ = stream.read(&mut buf).unwrap();

        let response = "HTTP/1.1 200 OK\r\n\
                        Access-Control-Allow-Origin: *\r\n\
                        Content-Length: 14\r\n\
                        Content-Type: text/plain\r\n\
                        Connection: close\r\n\r\n\
                        Sensitive Data";
        stream.write_all(response.as_bytes()).unwrap();
        stream.flush().unwrap();
    });

    let target_url = format!("http://127.0.0.1:{port}/account");
    let req = RequestInit::get()
        .origin("http://client.example.com")
        .credentials(RequestCredentials::Include);

    let result = fetch(&target_url, Some(req));
    server_handle.join().unwrap();

    match result {
        Err(FetchError::CorsError(msg)) => {
            assert!(msg.contains("cannot be wildcard '*'"));
        }
        other => panic!("Expected CorsError for wildcard origin with credentials, got: {other:?}"),
    }
}

#[test]
fn test_cors_exact_origin_match_succeeds() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind ephemeral loopback");
    let port = listener.local_addr().unwrap().port();

    let server_handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = [0u8; 1024];
        let _ = stream.read(&mut buf).unwrap();

        let response = "HTTP/1.1 200 OK\r\n\
                        Access-Control-Allow-Origin: http://authorized-app.example.com\r\n\
                        Access-Control-Allow-Credentials: true\r\n\
                        Content-Length: 17\r\n\
                        Content-Type: text/plain\r\n\
                        Connection: close\r\n\r\n\
                        Authenticated API";
        stream.write_all(response.as_bytes()).unwrap();
        stream.flush().unwrap();
    });

    let target_url = format!("http://127.0.0.1:{port}/api");
    let req = RequestInit::get()
        .origin("http://authorized-app.example.com")
        .credentials(RequestCredentials::Include);

    let resp = fetch(&target_url, Some(req)).expect("Exact origin match must succeed");
    server_handle.join().unwrap();

    assert_eq!(resp.status, 200);
    assert_eq!(resp.text().unwrap(), "Authenticated API");
}

#[test]
fn test_cors_preflight_options_handshake_and_actual_request() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind ephemeral loopback");
    let port = listener.local_addr().unwrap().port();

    let preflight_received = Arc::new(AtomicBool::new(false));
    let put_received = Arc::new(AtomicBool::new(false));

    let p_flag = preflight_received.clone();
    let put_flag = put_received.clone();

    let server_handle = thread::spawn(move || {
        // 1. Receive Preflight OPTIONS request
        let (mut stream1, _) = listener.accept().unwrap();
        let mut buf1 = [0u8; 2048];
        let n1 = stream1.read(&mut buf1).unwrap();
        let req1_str = String::from_utf8_lossy(&buf1[..n1]);

        assert!(req1_str.starts_with("OPTIONS "), "First request must be OPTIONS preflight");
        assert!(req1_str.contains("access-control-request-method: PUT") || req1_str.contains("Access-Control-Request-Method: PUT"));
        assert!(req1_str.to_ascii_lowercase().contains("x-custom-token"));
        p_flag.store(true, Ordering::SeqCst);

        let options_resp = "HTTP/1.1 204 No Content\r\n\
                            Access-Control-Allow-Origin: http://portal.company.test\r\n\
                            Access-Control-Allow-Methods: GET, POST, PUT, DELETE\r\n\
                            Access-Control-Allow-Headers: x-custom-token, content-type\r\n\
                            Connection: close\r\n\r\n";
        stream1.write_all(options_resp.as_bytes()).unwrap();
        stream1.flush().unwrap();

        // 2. Receive actual PUT request
        let (mut stream2, _) = listener.accept().unwrap();
        let mut buf2 = [0u8; 2048];
        let n2 = stream2.read(&mut buf2).unwrap();
        let req2_str = String::from_utf8_lossy(&buf2[..n2]);

        assert!(req2_str.starts_with("PUT "), "Second request must be actual PUT");
        assert!(req2_str.to_ascii_lowercase().contains("origin: http://portal.company.test"));
        put_flag.store(true, Ordering::SeqCst);

        let put_resp = "HTTP/1.1 200 OK\r\n\
                        Access-Control-Allow-Origin: http://portal.company.test\r\n\
                        Content-Length: 14\r\n\
                        Content-Type: text/plain\r\n\
                        Connection: close\r\n\r\n\
                        Resource Saved";
        stream2.write_all(put_resp.as_bytes()).unwrap();
        stream2.flush().unwrap();
    });

    let target_url = format!("http://127.0.0.1:{port}/resource/42");
    let req = RequestInit::put()
        .origin("http://portal.company.test")
        .header("X-Custom-Token", "super-auth-value")
        .body(b"data".to_vec());

    let resp = fetch(&target_url, Some(req)).expect("Preflight and actual PUT should both succeed");
    server_handle.join().unwrap();

    assert!(preflight_received.load(Ordering::SeqCst));
    assert!(put_received.load(Ordering::SeqCst));
    assert_eq!(resp.status, 200);
    assert_eq!(resp.text().unwrap(), "Resource Saved");
}

#[test]
fn test_cors_preflight_method_disallowed_fails() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind ephemeral loopback");
    let port = listener.local_addr().unwrap().port();

    let server_handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = [0u8; 1024];
        let _ = stream.read(&mut buf).unwrap();

        let options_resp = "HTTP/1.1 204 No Content\r\n\
                            Access-Control-Allow-Origin: http://portal.company.test\r\n\
                            Access-Control-Allow-Methods: GET, POST\r\n\
                            Connection: close\r\n\r\n";
        stream.write_all(options_resp.as_bytes()).unwrap();
        stream.flush().unwrap();
    });

    let target_url = format!("http://127.0.0.1:{port}/data");
    let req = RequestInit::delete().origin("http://portal.company.test");

    let result = fetch(&target_url, Some(req));
    server_handle.join().unwrap();

    match result {
        Err(FetchError::CorsError(msg)) => {
            assert!(msg.contains("method 'DELETE' not permitted"));
        }
        other => panic!("Expected CorsError for disallowed DELETE method, got: {other:?}"),
    }
}
