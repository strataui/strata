//! Integration tests for authentic asynchronous HTTP client in `strata_js`.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use strata_js::engine::JsEngine;
use strata_js::JsValue;

/// Helper function that starts a lightweight mock HTTP server on localhost.
/// Returns the bound port and an Arc<AtomicBool> stop flag.
fn start_test_http_server() -> (u16, Arc<AtomicBool>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let port = listener.local_addr().expect("local addr").port();
    let stop_signal = Arc::new(AtomicBool::new(false));
    let stop_clone = Arc::clone(&stop_signal);

    listener.set_nonblocking(true).expect("set nonblocking");

    thread::spawn(move || {
        while !stop_clone.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let mut buffer = [0u8; 4096];
                    let mut total_read = 0;
                    // Read request
                    stream
                        .set_read_timeout(Some(Duration::from_millis(500)))
                        .ok();
                    if let Ok(n) = stream.read(&mut buffer) {
                        total_read = n;
                    }

                    let req_str = String::from_utf8_lossy(&buffer[..total_read]);
                    let first_line = req_str.lines().next().unwrap_or("");

                    let response = if first_line.starts_with("GET /api/hello ") {
                        let body = "{\"greeting\":\"Hello from live HTTP!\",\"code\":100}";
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nX-Strata-Test: Passed\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        )
                    } else if first_line.starts_with("POST /api/echo ") {
                        // Extract request body after double CRLF
                        let body_start = req_str
                            .find("\r\n\r\n")
                            .map(|idx| idx + 4)
                            .unwrap_or(total_read);
                        let req_body = &req_str[body_start..];
                        let resp_body =
                            format!("{{\"echo\":{},\"status\":\"received\"}}", req_body.trim());
                        format!(
                            "HTTP/1.1 201 Created\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            resp_body.len(),
                            resp_body
                        )
                    } else if first_line.starts_with("DELETE /api/item/99 ") {
                        let resp_body = "{\"deleted\":true,\"id\":99}";
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            resp_body.len(),
                            resp_body
                        )
                    } else {
                        let resp_body = "{\"error\":\"not_found\"}";
                        format!(
                            "HTTP/1.1 404 Not Found\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            resp_body.len(),
                            resp_body
                        )
                    };

                    let _ = stream.write_all(response.as_bytes());
                    let _ = stream.flush();
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(_) => {
                    break;
                }
            }
        }
    });

    (port, stop_signal)
}

#[test]
fn test_live_http_fetch_get_and_json_unpack() {
    let (port, stop_signal) = start_test_http_server();

    let mut engine = JsEngine::new();
    let script = format!(
        r#"
        async function fetchHello() {{
            var res = await fetch("http://127.0.0.1:{}/api/hello");
            var status = res.status;
            var ok = res.ok;
            var headerVal = res.headers.get("x-strata-test");
            var data = await res.json();
            return {{
                status: status,
                ok: ok,
                headerVal: headerVal,
                greeting: data.greeting,
                code: data.code
            }};
        }}

        var result = await fetchHello();
        var status = result.status;
        var ok = result.ok;
        var headerVal = result.headerVal;
        var greeting = result.greeting;
        var code = result.code;
        "#,
        port
    );

    let res = engine.eval(&script);
    stop_signal.store(true, Ordering::SeqCst);

    assert!(
        res.is_ok(),
        "Evaluation of live HTTP GET fetch must succeed: {:?}",
        res
    );
    assert_eq!(engine.globals.get("status"), Some(&JsValue::Number(200.0)));
    assert_eq!(engine.globals.get("ok"), Some(&JsValue::Bool(true)));
    assert_eq!(
        engine.globals.get("headerVal"),
        Some(&JsValue::String("Passed".to_string()))
    );
    assert_eq!(
        engine.globals.get("greeting"),
        Some(&JsValue::String("Hello from live HTTP!".to_string()))
    );
    assert_eq!(engine.globals.get("code"), Some(&JsValue::Number(100.0)));
}

#[test]
fn test_live_http_fetch_post_json_payload() {
    let (port, stop_signal) = start_test_http_server();

    let mut engine = JsEngine::new();
    let script = format!(
        r#"
        async function sendPost() {{
            var res = await fetch("http://127.0.0.1:{}/api/echo", {{
                method: "POST",
                headers: {{
                    "Content-Type": "application/json"
                }},
                body: '{{"msg":"test-payload"}}'
            }});
            var status = res.status;
            var ok = res.ok;
            var data = await res.json();
            return {{
                status: status,
                ok: ok,
                statusText: res.statusText,
                data: data
            }};
        }}

        var result = await sendPost();
        var status = result.status;
        var ok = result.ok;
        var echoMsg = result.data.echo.msg;
        var echoStatus = result.data.status;
        "#,
        port
    );

    let res = engine.eval(&script);
    stop_signal.store(true, Ordering::SeqCst);

    assert!(
        res.is_ok(),
        "Evaluation of live HTTP POST fetch must succeed: {:?}",
        res
    );
    assert_eq!(engine.globals.get("status"), Some(&JsValue::Number(201.0)));
    assert_eq!(engine.globals.get("ok"), Some(&JsValue::Bool(true)));
    assert_eq!(
        engine.globals.get("echoMsg"),
        Some(&JsValue::String("test-payload".to_string()))
    );
    assert_eq!(
        engine.globals.get("echoStatus"),
        Some(&JsValue::String("received".to_string()))
    );
}

#[test]
fn test_live_http_fetch_delete_method() {
    let (port, stop_signal) = start_test_http_server();

    let mut engine = JsEngine::new();
    let script = format!(
        r#"
        async function sendDelete() {{
            var res = await fetch("http://127.0.0.1:{}/api/item/99", {{
                method: "DELETE"
            }});
            var data = await res.json();
            return {{
                status: res.status,
                deleted: data.deleted,
                id: data.id
            }};
        }}

        var result = await sendDelete();
        var status = result.status;
        var deleted = result.deleted;
        var id = result.id;
        "#,
        port
    );

    let res = engine.eval(&script);
    stop_signal.store(true, Ordering::SeqCst);

    assert!(
        res.is_ok(),
        "Evaluation of live HTTP DELETE fetch must succeed: {:?}",
        res
    );
    assert_eq!(engine.globals.get("status"), Some(&JsValue::Number(200.0)));
    assert_eq!(engine.globals.get("deleted"), Some(&JsValue::Bool(true)));
    assert_eq!(engine.globals.get("id"), Some(&JsValue::Number(99.0)));
}

#[test]
fn test_rust_async_web_fetch_api() {
    let (port, stop_signal) = start_test_http_server();
    let url = format!("http://127.0.0.1:{}/api/hello", port);

    // Call strata_js::web::fetch directly
    let future = strata_js::web::fetch(&url, None);
    // Poll the future synchronously
    let resp = futures_executor_poll(future);
    stop_signal.store(true, Ordering::SeqCst);

    assert!(
        resp.is_ok(),
        "strata_js::web::fetch directly must succeed: {:?}",
        resp
    );
    let response = resp.unwrap();
    assert_eq!(response.status, 200);
    assert!(response.ok);
    assert_eq!(
        response.headers.get("x-strata-test"),
        Some("Passed".to_string())
    );
}

fn futures_executor_poll<F: std::future::Future>(mut f: F) -> F::Output {
    use std::task::{Context, Poll, RawWaker, Waker};
    let raw_waker = RawWaker::new(std::ptr::null(), &VTABLE);
    let waker = unsafe { Waker::from_raw(raw_waker) };
    let mut cx = Context::from_waker(&waker);
    let mut pin = unsafe { std::pin::Pin::new_unchecked(&mut f) };
    match pin.as_mut().poll(&mut cx) {
        Poll::Ready(res) => res,
        Poll::Pending => panic!("Future expected to resolve immediately"),
    }
}

static VTABLE: std::task::RawWakerVTable = std::task::RawWakerVTable::new(
    |_| std::task::RawWaker::new(std::ptr::null(), &VTABLE),
    |_| {},
    |_| {},
    |_| {},
);
