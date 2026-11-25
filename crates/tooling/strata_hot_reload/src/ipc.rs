//! Cross-platform JSON-Lines IPC transport connecting `strata dev` (CLI) to a live Strata app.
//!
//! - On Unix (`Linux`, `macOS`), binds a `0o600` Unix domain socket.
//! - On non-Unix (`Windows`), binds an ephemeral loopback TCP listener (`127.0.0.1:0`)
//!   and writes its `host:port` endpoint into the descriptor file path.

use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

/// Environment variable set by `strata dev` pointing the launched app at the HMR control socket.
pub const STRATA_HMR_SOCKET_ENV: &str = "STRATA_HMR_SOCKET";

/// Computes a deterministic per-project HMR socket path in the system temp directory.
pub fn default_hmr_socket_path(project_dir: &Path) -> PathBuf {
    let canonical = project_dir
        .canonicalize()
        .unwrap_or_else(|_| project_dir.to_path_buf());
    let mut hash: u64 = 0xcbf29ce484222325;
    for &b in canonical.to_string_lossy().as_bytes() {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    std::env::temp_dir().join(format!("strata-hmr-{hash:016x}.sock"))
}

/// Request sent from `strata dev` CLI to the running application's HMR runtime bridge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum HmrRequest {
    Ping,
    ApplyPatch {
        dylib_path: String,
        epoch: u64,
        compile_ms: u64,
    },
    Rollback,
    Shutdown,
}

/// Response returned from the running application back to `strata dev`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HmrResponse {
    Pong {
        epoch: u64,
    },
    PatchAck {
        ok: bool,
        epoch: u64,
        swap_us: u64,
        signals_restored: usize,
        error: Option<String>,
    },
    ShutdownAck,
}

/// Incoming HMR request paired with a one-shot response channel back to the IPC worker thread.
pub struct PendingHmrCommand {
    pub request: HmrRequest,
    pub responder: Sender<HmrResponse>,
}

/// Background IPC server embedded inside a running Strata application.
pub struct HmrIpcServer {
    socket_path: PathBuf,
    rx: Receiver<PendingHmrCommand>,
}

impl HmrIpcServer {
    /// Binds an HMR IPC endpoint and spawns a daemon listener thread.
    ///
    /// Optional `on_notify` is invoked immediately whenever a request arrives so native
    /// GUI event loops (GTK/Wayland/X11/macOS/Win32) wake up without waiting for a timer.
    pub fn bind<F>(socket_path: impl Into<PathBuf>, on_notify: F) -> Result<Self, std::io::Error>
    where
        F: Fn() + Send + Sync + 'static,
    {
        let path = socket_path.into();
        if path.exists() {
            let _ = std::fs::remove_file(&path);
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let (tx, rx) = mpsc::channel::<PendingHmrCommand>();
        let notify = std::sync::Arc::new(on_notify);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            use std::os::unix::net::UnixListener;
            let listener = UnixListener::bind(&path)?;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
            let tx_bg = tx.clone();
            let notify_bg = notify.clone();
            std::thread::Builder::new()
                .name("strata-hmr-ipc-server".to_string())
                .spawn(move || {
                    for stream in listener.incoming().flatten() {
                        let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
                        let _ = stream.set_write_timeout(Some(Duration::from_secs(10)));
                        if let Ok(reader_stream) = stream.try_clone() {
                            serve_connection(
                                BufReader::new(reader_stream),
                                stream,
                                &tx_bg,
                                notify_bg.as_ref(),
                            );
                        }
                    }
                })?;
        }

        #[cfg(not(unix))]
        {
            use std::net::TcpListener;
            let listener = TcpListener::bind("127.0.0.1:0")?;
            let local_addr = listener.local_addr()?.to_string();
            std::fs::write(&path, &local_addr)?;
            let tx_bg = tx.clone();
            let notify_bg = notify.clone();
            std::thread::Builder::new()
                .name("strata-hmr-ipc-server".to_string())
                .spawn(move || {
                    for stream in listener.incoming().flatten() {
                        let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
                        let _ = stream.set_write_timeout(Some(Duration::from_secs(10)));
                        if let Ok(reader_stream) = stream.try_clone() {
                            serve_connection(
                                BufReader::new(reader_stream),
                                stream,
                                &tx_bg,
                                notify_bg.as_ref(),
                            );
                        }
                    }
                })?;
        }

        Ok(Self {
            socket_path: path,
            rx,
        })
    }

    /// Returns the bound IPC path.
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    /// Non-blockingly drains all pending HMR commands queued since the previous poll.
    pub fn drain_pending(&self) -> Vec<PendingHmrCommand> {
        let mut out = Vec::new();
        while let Ok(cmd) = self.rx.try_recv() {
            out.push(cmd);
        }
        out
    }
}

impl Drop for HmrIpcServer {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

fn serve_connection<R: Read, W: Write>(
    mut reader: BufReader<R>,
    mut writer: W,
    tx: &Sender<PendingHmrCommand>,
    on_notify: &(dyn Fn() + Send + Sync),
) {
    let mut line = String::new();
    while let Ok(n) = reader.read_line(&mut line) {
        if n == 0 {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            line.clear();
            continue;
        }
        let resp = match serde_json::from_str::<HmrRequest>(trimmed) {
            Ok(request) => {
                let (resp_tx, resp_rx) = mpsc::channel();
                if tx.send(PendingHmrCommand { request, responder: resp_tx }).is_ok() {
                    on_notify();
                    resp_rx
                        .recv_timeout(Duration::from_secs(10))
                        .unwrap_or(HmrResponse::PatchAck {
                            ok: false,
                            epoch: 0,
                            swap_us: 0,
                            signals_restored: 0,
                            error: Some("Timed out waiting for Strata UI thread to apply HMR patch".to_string()),
                        })
                } else {
                    HmrResponse::PatchAck {
                        ok: false,
                        epoch: 0,
                        swap_us: 0,
                        signals_restored: 0,
                        error: Some("HMR UI receiver closed".to_string()),
                    }
                }
            }
            Err(err) => HmrResponse::PatchAck {
                ok: false,
                epoch: 0,
                swap_us: 0,
                signals_restored: 0,
                error: Some(format!("Malformed HMR request JSON: {err}")),
            },
        };
        if let Ok(mut json) = serde_json::to_string(&resp) {
            json.push('\n');
            if writer.write_all(json.as_bytes()).is_err() || writer.flush().is_err() {
                break;
            }
        }
        line.clear();
    }
}

/// Client used by `strata dev` to push compiled dynamic libraries to a running Strata app.
#[derive(Debug, Clone)]
pub struct HmrIpcClient {
    socket_path: PathBuf,
}

impl HmrIpcClient {
    /// Creates an IPC client targeting `socket_path`.
    pub fn new(socket_path: impl Into<PathBuf>) -> Self {
        Self {
            socket_path: socket_path.into(),
        }
    }

    /// Sends a single [`HmrRequest`] and awaits the application's [`HmrResponse`].
    pub fn send(&self, request: &HmrRequest) -> Result<HmrResponse, String> {
        let mut payload = serde_json::to_string(request).map_err(|e| e.to_string())?;
        payload.push('\n');

        let response_line = self.send_raw(&payload)?;
        serde_json::from_str::<HmrResponse>(response_line.trim())
            .map_err(|e| format!("Invalid HMR response JSON '{}': {e}", response_line.trim()))
    }

    /// Convenience helper sending [`HmrRequest::Ping`].
    pub fn ping(&self) -> Result<u64, String> {
        match self.send(&HmrRequest::Ping)? {
            HmrResponse::Pong { epoch } => Ok(epoch),
            other => Err(format!("Unexpected ping response: {:?}", other)),
        }
    }

    /// Convenience helper pushing an updated shared library artifact.
    pub fn apply_patch(
        &self,
        dylib_path: &Path,
        epoch: u64,
        compile_ms: u64,
    ) -> Result<HmrResponse, String> {
        self.send(&HmrRequest::ApplyPatch {
            dylib_path: dylib_path.to_string_lossy().to_string(),
            epoch,
            compile_ms,
        })
    }

    #[cfg(unix)]
    fn send_raw(&self, payload: &str) -> Result<String, String> {
        use std::os::unix::net::UnixStream;
        let mut stream = UnixStream::connect(&self.socket_path).map_err(|e| {
            format!(
                "Failed to connect to Strata HMR socket '{}': {e}",
                self.socket_path.display()
            )
        })?;
        let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
        let _ = stream.set_write_timeout(Some(Duration::from_secs(10)));
        stream
            .write_all(payload.as_bytes())
            .and_then(|_| stream.flush())
            .map_err(|e| format!("Failed writing HMR payload: {e}"))?;
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader
            .read_line(&mut line)
            .map_err(|e| format!("Failed reading HMR response: {e}"))?;
        Ok(line)
    }

    #[cfg(not(unix))]
    fn send_raw(&self, payload: &str) -> Result<String, String> {
        use std::net::TcpStream;
        let addr = std::fs::read_to_string(&self.socket_path)
            .map_err(|e| {
                format!(
                    "Failed to read HMR endpoint descriptor '{}': {e}",
                    self.socket_path.display()
                )
            })?
            .trim()
            .to_string();
        let mut stream = TcpStream::connect(&addr)
            .map_err(|e| format!("Failed to connect to Strata HMR TCP endpoint '{addr}': {e}"))?;
        let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
        let _ = stream.set_write_timeout(Some(Duration::from_secs(10)));
        stream
            .write_all(payload.as_bytes())
            .and_then(|_| stream.flush())
            .map_err(|e| format!("Failed writing HMR payload: {e}"))?;
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader
            .read_line(&mut line)
            .map_err(|e| format!("Failed reading HMR response: {e}"))?;
        Ok(line)
    }
}
