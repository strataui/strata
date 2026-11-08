//! Chrome DevTools Protocol (CDP) WebSocket & HTTP Discovery Server.
//!
//! Listens on `ws://127.0.0.1:9222` (or configurable port), serving `/json/version`,
//! `/json/list`, and RFC 6455 WebSocket connections for Chrome DevTools inspection.

use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::cdp::{compute_sec_websocket_accept, CdpDispatcher, WebSocketCodec};

/// Chrome DevTools Protocol HTTP and WebSocket server.
pub struct CdpServer {
    port: u16,
    running: Arc<AtomicBool>,
    thread_handle: Option<JoinHandle<()>>,
}

impl CdpServer {
    /// Binds and starts the CDP server on `127.0.0.1:port` in a background thread.
    /// If `port == 0`, the OS assigns an available ephemeral port.
    pub fn start(dispatcher: CdpDispatcher, port: u16) -> io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", port))?;
        listener.set_nonblocking(true)?;
        let bound_port = listener.local_addr()?.port();

        let running = Arc::new(AtomicBool::new(true));
        let running_clone = Arc::clone(&running);

        let thread_handle = thread::Builder::new()
            .name(format!("cdp-server-{}", bound_port))
            .spawn(move || {
                run_server_loop(listener, dispatcher, bound_port, running_clone);
            })?;

        Ok(Self {
            port: bound_port,
            running,
            thread_handle: Some(thread_handle),
        })
    }

    /// Port the server is bound to.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Stops the server and waits for the background listener thread to exit.
    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for CdpServer {
    fn drop(&mut self) {
        self.stop();
    }
}

fn run_server_loop(
    listener: TcpListener,
    dispatcher: CdpDispatcher,
    port: u16,
    running: Arc<AtomicBool>,
) {
    while running.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => {
                let disp = dispatcher.clone();
                let run = Arc::clone(&running);
                thread::spawn(move || {
                    let _ = handle_client(stream, disp, port, run);
                });
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(10));
            }
            Err(_) => break,
        }
    }
}

fn handle_client(
    mut stream: TcpStream,
    dispatcher: CdpDispatcher,
    port: u16,
    running: Arc<AtomicBool>,
) -> io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_millis(500)))?;
    let mut header_buf = [0u8; 4096];
    let mut read_bytes = 0;

    // Read initial HTTP request
    loop {
        match stream.read(&mut header_buf[read_bytes..]) {
            Ok(0) => return Ok(()),
            Ok(n) => {
                read_bytes += n;
                let text = String::from_utf8_lossy(&header_buf[..read_bytes]);
                if text.contains("\r\n\r\n") || text.contains("\n\n") {
                    break;
                }
                if read_bytes == header_buf.len() {
                    break;
                }
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::TimedOut => {
                if read_bytes > 0 {
                    break;
                }
                return Ok(());
            }
            Err(e) => return Err(e),
        }
    }

    let req_text = String::from_utf8_lossy(&header_buf[..read_bytes]);
    let first_line = req_text.lines().next().unwrap_or_default();

    // 1. Chrome DevTools Version Endpoint
    if first_line.starts_with("GET /json/version") {
        let body = format!(
            r#"{{"Browser":"Strata/0.1.0","Protocol-Version":"1.3","User-Agent":"Mozilla/5.0 (Strata; Linux x86_64)","V8-Version":"0.0.0","WebKit-Version":"537.36 (@strata)","webSocketDebuggerUrl":"ws://127.0.0.1:{}/devtools/page/1"}}"#,
            port
        );
        let resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json; charset=UTF-8\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream.write_all(resp.as_bytes())?;
        stream.flush()?;
        return Ok(());
    }

    // 2. Chrome DevTools Target List Endpoint
    if first_line.starts_with("GET /json") {
        let body = format!(
            r#"[{{"description":"Strata Universal Frontend Application","devtoolsFrontendUrl":"/devtools/inspector.html?ws=127.0.0.1:{port}/devtools/page/1","id":"1","title":"Strata Application","type":"page","url":"strata://app","webSocketDebuggerUrl":"ws://127.0.0.1:{port}/devtools/page/1"}}]"#,
            port = port
        );
        let resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json; charset=UTF-8\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream.write_all(resp.as_bytes())?;
        stream.flush()?;
        return Ok(());
    }

    // 3. WebSocket Upgrade
    let is_upgrade = req_text
        .lines()
        .any(|l| l.to_ascii_lowercase().starts_with("upgrade:") && l.to_ascii_lowercase().contains("websocket"));

    if is_upgrade {
        let mut ws_key = None;
        for line in req_text.lines() {
            if line.to_ascii_lowercase().starts_with("sec-websocket-key:") {
                let parts: Vec<&str> = line.split(':').collect();
                if parts.len() >= 2 {
                    ws_key = Some(parts[1].trim().to_string());
                }
            }
        }

        let key = ws_key.unwrap_or_else(|| "dGhlIHNhbXBsZSBub25jZQ==".to_string());
        let accept = compute_sec_websocket_accept(&key);

        let handshake = format!(
            "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n\r\n",
            accept
        );
        stream.write_all(handshake.as_bytes())?;
        stream.flush()?;

        // Switch to blocking read with small timeout for persistent WebSocket session
        stream.set_read_timeout(Some(Duration::from_millis(200)))?;
        let mut ws_buffer = Vec::with_capacity(8192);
        let mut temp = [0u8; 4096];

        while running.load(Ordering::Relaxed) {
            match stream.read(&mut temp) {
                Ok(0) => break, // Client disconnected
                Ok(n) => {
                    ws_buffer.extend_from_slice(&temp[..n]);
                    while !ws_buffer.is_empty() {
                        match WebSocketCodec::decode_frame(&ws_buffer) {
                            Ok((frame, consumed)) => {
                                ws_buffer.drain(..consumed);
                                if let Some(resp_frame) = dispatcher.handle_ws_frame(&frame) {
                                    let encoded = WebSocketCodec::encode_frame(resp_frame.opcode, &resp_frame.payload);
                                    if stream.write_all(&encoded).is_err() {
                                        return Ok(());
                                    }
                                    let _ = stream.flush();
                                }
                            }
                            Err(_) => break, // Incomplete frame, wait for more data
                        }
                    }
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::TimedOut => {
                    continue;
                }
                Err(_) => break,
            }
        }
        return Ok(());
    }

    // Default 404 response
    let not_found = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
    stream.write_all(not_found.as_bytes())?;
    stream.flush()?;
    Ok(())
}
