//! RFC 6265 HTTP State Management & Cookie Jar Integration Tests.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread;
use strata_web_api::cookie::{
    can_set_cookie_domain, default_path_for_request, domain_matches, parse_http_date, path_matches,
    Cookie, CookieJar, SameSite,
};
use strata_web_api::fetch::{fetch, RequestInit};

#[test]
fn test_parse_set_cookie_attributes() {
    let header = "sessionId=38afes7a8; Domain=example.com; Path=/docs; Secure; HttpOnly; SameSite=Strict";
    let url = "https://example.com/docs/index.html";

    let cookie = Cookie::parse(header, url).expect("Cookie parsing must succeed");
    assert_eq!(cookie.name, "sessionId");
    assert_eq!(cookie.value, "38afes7a8");
    assert_eq!(cookie.domain, "example.com");
    assert_eq!(cookie.path, "/docs");
    assert!(cookie.secure);
    assert!(cookie.http_only);
    assert_eq!(cookie.same_site, SameSite::Strict);
    assert!(!cookie.host_only);
}

#[test]
fn test_host_only_cookie_default() {
    let header = "user=alice; Path=/";
    let url = "http://my-service.internal/dashboard";

    let cookie = Cookie::parse(header, url).expect("Cookie parsing must succeed");
    assert_eq!(cookie.name, "user");
    assert_eq!(cookie.value, "alice");
    assert_eq!(cookie.domain, "my-service.internal");
    assert!(cookie.host_only, "Cookie without Domain attribute must be host-only");
    assert_eq!(cookie.path, "/");
    assert!(!cookie.secure);
    assert!(!cookie.http_only);
}

#[test]
fn test_default_path_computation_rfc6265() {
    assert_eq!(default_path_for_request("/"), "/");
    assert_eq!(default_path_for_request("/index.html"), "/");
    assert_eq!(default_path_for_request("/api/v1/users"), "/api/v1");
    assert_eq!(default_path_for_request("/docs/spec/"), "/docs/spec");
    assert_eq!(default_path_for_request(""), "/");
}

#[test]
fn test_http_date_parsing_rfc1123_rfc850_asctime() {
    // 09 Jun 2021 10:18:14 GMT
    let expected_secs = 1623233894;

    let rfc1123 = "Wed, 09 Jun 2021 10:18:14 GMT";
    let parsed1123 = parse_http_date(rfc1123).expect("RFC 1123 parse");
    assert_eq!(parsed1123, expected_secs);

    let rfc850 = "Wednesday, 09-Jun-21 10:18:14 GMT";
    let parsed850 = parse_http_date(rfc850).expect("RFC 850 parse");
    assert_eq!(parsed850, expected_secs);

    let asctime = "Wed Jun  9 10:18:14 2021";
    let parsed_asctime = parse_http_date(asctime).expect("asctime parse");
    assert_eq!(parsed_asctime, expected_secs);
}

#[test]
fn test_max_age_precedence_over_expires() {
    let header = "token=secret; Expires=Wed, 09 Jun 2021 10:18:14 GMT; Max-Age=7200";
    let url = "https://api.example.com/v1";

    let cookie = Cookie::parse(header, url).expect("Cookie parsing must succeed");
    assert_eq!(cookie.max_age, Some(7200));
    // When Max-Age is specified, expires timestamp must reflect now + 7200, NOT the 2021 date
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let exp = cookie.expires.expect("Expires must be present");
    assert!(exp >= now + 7190 && exp <= now + 7210);
    assert!(!cookie.is_expired(now));
}

#[test]
fn test_domain_matching_and_security_boundaries() {
    // Exact match
    assert!(domain_matches("example.com", "example.com", false));
    assert!(domain_matches("example.com", "example.com", true));

    // Subdomain match when not host-only
    assert!(domain_matches("sub.example.com", "example.com", false));
    assert!(domain_matches("deep.sub.example.com", "example.com", false));

    // Subdomain rejected when host-only
    assert!(!domain_matches("sub.example.com", "example.com", true));

    // Different domain
    assert!(!domain_matches("attacker.com", "example.com", false));
    assert!(!domain_matches("notexample.com", "example.com", false));

    // Security: can an origin set a cookie for a domain?
    assert!(can_set_cookie_domain("sub.example.com", "example.com"));
    assert!(can_set_cookie_domain("sub.example.com", "sub.example.com"));
    // Origin cannot set cookie for unrelated domain or top-level public suffix
    assert!(!can_set_cookie_domain("sub.example.com", "other.com"));
    assert!(!can_set_cookie_domain("sub.example.com", "com"));
}

#[test]
fn test_path_matching_rfc6265() {
    // Exact path
    assert!(path_matches("/api", "/api"));
    // Sub-path with trailing slash
    assert!(path_matches("/api/v1", "/api/"));
    // Sub-path boundary
    assert!(path_matches("/api/v1/users", "/api"));
    // Prefix that is not a directory boundary must NOT match
    assert!(!path_matches("/apiv1", "/api"));
    assert!(!path_matches("/other", "/api"));
}

#[test]
fn test_cookie_jar_sorting_and_header_generation() {
    let mut jar = CookieJar::new();

    // Add cookie with path /
    let c1 = Cookie {
        name: "general".to_string(),
        value: "site".to_string(),
        domain: "example.com".to_string(),
        path: "/".to_string(),
        expires: None,
        max_age: None,
        secure: false,
        http_only: false,
        same_site: SameSite::Lax,
        host_only: false,
        created_at: 100,
        last_accessed: 100,
    };
    jar.store(c1);

    // Add cookie with more specific path /api
    let c2 = Cookie {
        name: "auth".to_string(),
        value: "token123".to_string(),
        domain: "example.com".to_string(),
        path: "/api".to_string(),
        expires: None,
        max_age: None,
        secure: false,
        http_only: false,
        same_site: SameSite::Lax,
        host_only: false,
        created_at: 200,
        last_accessed: 200,
    };
    jar.store(c2);

    // Request to /api/users must include both, with /api sorted BEFORE /
    let header_val = jar
        .get_cookie_header_value("http://example.com/api/users")
        .expect("Header value must exist");
    assert_eq!(header_val, "auth=token123; general=site");

    // Request to /blog must only include /
    let blog_header = jar
        .get_cookie_header_value("http://example.com/blog")
        .expect("Header value must exist");
    assert_eq!(blog_header, "general=site");
}

#[test]
fn test_cookie_jar_expiration_and_removal() {
    let mut jar = CookieJar::new();

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Store active cookie
    let active = Cookie {
        name: "alive".to_string(),
        value: "yes".to_string(),
        domain: "example.com".to_string(),
        path: "/".to_string(),
        expires: Some(now + 3600),
        max_age: Some(3600),
        secure: false,
        http_only: false,
        same_site: SameSite::Lax,
        host_only: false,
        created_at: now,
        last_accessed: now,
    };
    jar.store(active);

    // Store already expired cookie
    let expired = Cookie {
        name: "dead".to_string(),
        value: "no".to_string(),
        domain: "example.com".to_string(),
        path: "/".to_string(),
        expires: Some(now - 10),
        max_age: Some(-10),
        secure: false,
        http_only: false,
        same_site: SameSite::Lax,
        host_only: false,
        created_at: now - 100,
        last_accessed: now - 100,
    };
    jar.store(expired);

    // Expired cookie must not be emitted
    let header = jar
        .get_cookie_header_value("http://example.com/")
        .expect("Header must exist");
    assert_eq!(header, "alive=yes");
    assert_eq!(jar.len(), 1);
}

#[test]
fn test_cookie_jar_json_serialization_roundtrip() {
    let mut jar = CookieJar::new();
    jar.process_set_cookie("pref=dark; Path=/; Domain=test.local", "http://test.local/");
    jar.process_set_cookie(
        "id=98765; Path=/app; Domain=test.local; Secure; HttpOnly",
        "https://test.local/app",
    );

    let json = jar.to_json().expect("Serialization to JSON must succeed");
    let loaded = CookieJar::from_json(&json).expect("Deserialization from JSON must succeed");

    assert_eq!(loaded.len(), 2);
    let pref = loaded.get("pref", "test.local", "/").expect("pref cookie");
    assert_eq!(pref.value, "dark");
    let id_c = loaded.get("id", "test.local", "/app").expect("id cookie");
    assert_eq!(id_c.value, "98765");
    assert!(id_c.secure);
    assert!(id_c.http_only);
}

#[test]
fn test_fetch_cookie_jar_lifecycle_and_injection() {
    // Spin up local HTTP server to verify Set-Cookie extraction and Cookie injection
    let listener = TcpListener::bind("127.0.0.1:0").expect("Bind test server");
    let addr = listener.local_addr().expect("local addr");
    let port = addr.port();

    let server_thread = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("Accept request 1");
        let mut buf = [0u8; 2048];
        let n = stream.read(&mut buf).expect("Read request 1");
        let req1_str = String::from_utf8_lossy(&buf[..n]);
        assert!(req1_str.starts_with("GET /login HTTP/1.1"));

        // Server sends 2 Set-Cookie headers
        let resp1 = "HTTP/1.1 200 OK\r\n\
                     Content-Length: 13\r\n\
                     Set-Cookie: session=xyz987; Path=/; HttpOnly\r\n\
                     Set-Cookie: user=bob; Path=/api\r\n\
                     Connection: close\r\n\r\n\
                     Logged In OK!";
        stream.write_all(resp1.as_bytes()).expect("write resp 1");
        stream.flush().unwrap();

        // Second request to /api/data
        let (mut stream2, _) = listener.accept().expect("Accept request 2");
        let mut buf2 = [0u8; 2048];
        let n2 = stream2.read(&mut buf2).expect("Read request 2");
        let req2_str = String::from_utf8_lossy(&buf2[..n2]);

        assert!(req2_str.starts_with("GET /api/data HTTP/1.1"));
        assert!(
            req2_str.contains("Cookie: user=bob; session=xyz987")
                || req2_str.contains("Cookie: session=xyz987; user=bob")
                || req2_str.contains("cookie: user=bob; session=xyz987")
                || req2_str.contains("cookie: session=xyz987; user=bob"),
            "Request 2 must contain injected Cookie header! Got:\n{req2_str}"
        );

        let resp2 = "HTTP/1.1 200 OK\r\n\
                     Content-Length: 7\r\n\
                     Connection: close\r\n\r\n\
                     Data OK";
        stream2.write_all(resp2.as_bytes()).expect("write resp 2");
        stream2.flush().unwrap();
    });

    let jar = Arc::new(Mutex::new(CookieJar::new()));

    // 1. Initial request to /login
    let login_url = format!("http://127.0.0.1:{port}/login");
    let init1 = RequestInit::get().cookie_jar(jar.clone());
    let resp1 = fetch(&login_url, Some(init1)).expect("fetch login");
    assert_eq!(resp1.status, 200);

    // Verify cookies stored in jar
    {
        let locked = jar.lock().unwrap();
        assert_eq!(locked.len(), 2);
        assert!(locked.get("session", "127.0.0.1", "/").is_some());
        assert!(locked.get("user", "127.0.0.1", "/api").is_some());
    }

    // 2. Subsequent request to /api/data
    let data_url = format!("http://127.0.0.1:{port}/api/data");
    let init2 = RequestInit::get().cookie_jar(jar.clone());
    let resp2 = fetch(&data_url, Some(init2)).expect("fetch data");
    assert_eq!(resp2.status, 200);

    server_thread.join().expect("Server thread must finish cleanly");
}
