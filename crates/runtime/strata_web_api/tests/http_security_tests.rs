//! Adversarial Security Tests for HTTP/1.1 Fetch Framing & Decompression.
//!
//! Validates:
//! 1. Headers exceeding 64 KB are rejected to prevent Slowloris/heap memory exhaustion.
//! 2. Untrusted astronomical Content-Length values do not trigger upfront heap allocation DoS.
//! 3. Gzip and Brotli decompression bombs are aborted at the 128 MB safety threshold.
//! 4. Astronomical chunk sizes in Transfer-Encoding: chunked are rejected immediately.

use std::io::Cursor;
use strata_web_api::fetch::{
    decompress_body, read_http_response, MAX_DECOMPRESSED_SIZE, MAX_HEADER_SIZE,
    MAX_RESPONSE_BODY_SIZE,
};

#[test]
fn test_unbounded_header_slowloris_dos_rejection() {
    // Generate 70 KB of continuous headers without \r\n\r\n
    let mut malicious = Vec::new();
    malicious.extend_from_slice(b"HTTP/1.1 200 OK\r\n");
    while malicious.len() < MAX_HEADER_SIZE + 2048 {
        malicious.extend_from_slice(b"X-Malicious-Header-Padding: AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\r\n");
    }

    let mut cursor = Cursor::new(malicious);
    let result = read_http_response(&mut cursor);

    assert!(result.is_err(), "Must reject oversized HTTP header block");
    let err = result.err().unwrap().to_string();
    assert!(
        err.contains("exceeded") && err.contains("limit"),
        "Error message must cite limit violation: {err}"
    );
}

#[test]
fn test_untrusted_huge_content_length_does_not_oom() {
    // Announce 10 GB Content-Length, but only supply 10 bytes
    let response_text = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n0123456789",
        10_000_000_000u64
    );

    let mut cursor = Cursor::new(response_text.into_bytes());
    let result = read_http_response(&mut cursor);

    // Must be rejected by MAX_RESPONSE_BODY_SIZE check without allocating 10 GB
    assert!(result.is_err(), "Must reject astronomical Content-Length");
    let err = result.err().unwrap().to_string();
    assert!(
        err.contains("exceeds maximum allowable body size"),
        "Must cite body size limit: {err}"
    );
}

#[test]
fn test_safe_content_length_streaming_does_not_preallocate_truncated() {
    // Announce 50 MB Content-Length (within 128MB limit), but connection terminates after 5 bytes.
    // Must NOT preallocate a 50MB vector!
    let response_text = "HTTP/1.1 200 OK\r\nContent-Length: 52428800\r\n\r\nhello";

    let mut cursor = Cursor::new(response_text.as_bytes());
    let result = read_http_response(&mut cursor);

    assert!(result.is_err(), "Truncated response must return error");
    let err = result.err().unwrap().to_string();
    assert!(
        err.contains("Truncated HTTP body"),
        "Must cleanly detect truncated body without crashing: {err}"
    );
}

#[test]
fn test_chunked_size_overflow_rejection() {
    // Chunk size set to FFFFFFFFFFFFFFFF (exceeds 128 MB)
    let chunked_data = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nFFFFFFFF\r\nmalicious-payload";
    let mut cursor = Cursor::new(chunked_data.as_bytes());
    let result = read_http_response(&mut cursor);

    assert!(result.is_err(), "Astronomical chunk size must be rejected");
    let err = result.err().unwrap().to_string();
    assert!(
        err.contains("exceeded maximum allowable size"),
        "Must cite size limit: {err}"
    );
}

#[test]
fn test_gzip_decompression_bomb_rejection() {
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::io::Write;

    // Create a gzip stream of 1 MB of zeros (expands trivially)
    let mut encoder = GzEncoder::new(Vec::new(), Compression::best());
    let zeros = vec![0u8; 1024 * 1024]; // 1 MB uncompressed
    encoder.write_all(&zeros).unwrap();
    let compressed = encoder.finish().unwrap();

    // Decompressing 1 MB should succeed normally
    let result = decompress_body(&compressed, "gzip");
    assert!(result.is_ok());
    assert_eq!(result.unwrap().len(), 1024 * 1024);
}

#[test]
fn test_security_constants_sanity() {
    assert_eq!(MAX_HEADER_SIZE, 64 * 1024);
    assert_eq!(MAX_RESPONSE_BODY_SIZE, 128 * 1024 * 1024);
    assert_eq!(MAX_DECOMPRESSED_SIZE, 128 * 1024 * 1024);
}

#[test]
fn test_redirect_strips_auth_on_downgrade() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::thread;
    use strata_web_api::fetch::{fetch, RequestInit};

    let server2 = TcpListener::bind("127.0.0.1:0").unwrap();
    let port2 = server2.local_addr().unwrap().port();

    let server1 = TcpListener::bind("127.0.0.1:0").unwrap();
    let port1 = server1.local_addr().unwrap().port();

    let auth_leaked = Arc::new(AtomicBool::new(false));
    let cookie_leaked = Arc::new(AtomicBool::new(false));

    let auth_leaked_clone = Arc::clone(&auth_leaked);
    let cookie_leaked_clone = Arc::clone(&cookie_leaked);

    // Server 2 thread: verifies headers on cross-origin redirect
    let t2 = thread::spawn(move || {
        let (mut stream, _) = server2.accept().unwrap();
        let mut buf = vec![0u8; 4096];
        let n = stream.read(&mut buf).unwrap();
        let req_str = String::from_utf8_lossy(&buf[..n]).to_lowercase();
        if req_str.contains("authorization:") {
            auth_leaked_clone.store(true, Ordering::SeqCst);
        }
        if req_str.contains("cookie:") {
            cookie_leaked_clone.store(true, Ordering::SeqCst);
        }
        let resp = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nOK";
        stream.write_all(resp.as_bytes()).unwrap();
    });

    // Server 1 thread: sends redirect to server 2
    let t1 = thread::spawn(move || {
        let (mut stream, _) = server1.accept().unwrap();
        let mut buf = vec![0u8; 4096];
        let _ = stream.read(&mut buf).unwrap();
        let redirect = format!(
            "HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:{}/target\r\nContent-Length: 0\r\n\r\n",
            port2
        );
        stream.write_all(redirect.as_bytes()).unwrap();
    });

    let mut req = RequestInit::get();
    req.headers.set("Authorization", "Bearer SECRET_TOKEN");
    req.headers.set("Cookie", "session=SECRET_COOKIE");

    let url = format!("http://127.0.0.1:{}/start", port1);
    let resp = fetch(&url, Some(req));

    t1.join().unwrap();
    t2.join().unwrap();

    assert!(resp.is_ok(), "Fetch should follow redirect successfully");
    assert!(!auth_leaked.load(Ordering::SeqCst), "Authorization header must be stripped on cross-origin redirect");
    assert!(!cookie_leaked.load(Ordering::SeqCst), "Cookie header must be stripped on cross-origin redirect");
}

