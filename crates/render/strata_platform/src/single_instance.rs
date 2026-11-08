//! Single-Instance Mutex and Inter-Process Communication (IPC) Window Focusing.
//!
//! Prevents duplicate application processes from opening when an executable is launched
//! repeatedly, automatically forwarding CLI arguments to the primary instance and requesting
//! window focusing / un-minimization.
//!
//! Unix hosts use a domain socket in `$XDG_RUNTIME_DIR` (falling back to `/tmp`). Windows
//! hosts use a named pipe at `\\.\pipe\<sanitized-app-id>`. Both transports forward
//! newline-separated CLI arguments to the primary instance.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[cfg(unix)]
use std::path::PathBuf;

/// Status result when attempting to acquire a single-instance application lock.
#[derive(Debug)]
pub enum SingleInstanceStatus {
    /// This process is the primary instance and acquired the exclusive mutex.
    Primary(SingleInstanceLock),
    /// A primary instance is already running; CLI arguments were forwarded and this process should exit.
    SecondaryExited,
}

/// Mutex handle holding the single-instance lock and IPC listener thread.
#[derive(Debug)]
pub struct SingleInstanceLock {
    app_id: String,
    running: Arc<AtomicBool>,
    #[cfg(unix)]
    socket_path: PathBuf,
    #[cfg(windows)]
    pipe_handle: usize,
}

impl SingleInstanceLock {
    /// Attempts to acquire an exclusive single-instance application lock for the given application ID.
    ///
    /// If another instance is already running, forwards `args` to it and returns
    /// `SingleInstanceStatus::SecondaryExited`.
    ///
    /// If this is the first instance, binds the IPC channel and invokes `on_secondary_args` whenever
    /// subsequent processes launch.
    pub fn acquire<F>(
        app_id: &str,
        args: &[String],
        on_secondary_args: F,
    ) -> Result<SingleInstanceStatus, std::io::Error>
    where
        F: Fn(Vec<String>) + Send + Sync + 'static,
    {
        #[cfg(unix)]
        {
            let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    extern "C" {
                        fn getuid() -> u32;
                    }
                    let uid = unsafe { getuid() };
                    let dir = PathBuf::from(format!("/tmp/strata-runtime-{}", uid));
                    let _ = std::fs::create_dir_all(&dir);
                    use std::os::unix::fs::PermissionsExt;
                    let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
                    dir
                });
            Self::acquire_in_dir(app_id, args, &runtime_dir, on_secondary_args)
        }
        #[cfg(windows)]
        {
            Self::acquire_windows(app_id, args, on_secondary_args)
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = (args, on_secondary_args);
            Ok(SingleInstanceStatus::Primary(Self::handle_only(app_id)))
        }
    }

    /// Acquires the lock inside an explicit runtime directory.
    ///
    /// This is the testable form of [`SingleInstanceLock::acquire`]: callers can
    /// point it at a private directory instead of mutating process environment
    /// variables. On Windows the runtime directory is ignored because the named
    /// pipe namespace is global.
    pub fn acquire_in_dir<F>(
        app_id: &str,
        args: &[String],
        runtime_dir: &std::path::Path,
        on_secondary_args: F,
    ) -> Result<SingleInstanceStatus, std::io::Error>
    where
        F: Fn(Vec<String>) + Send + Sync + 'static,
    {
        #[cfg(unix)]
        {
            Self::acquire_unix(app_id, args, runtime_dir, on_secondary_args)
        }
        #[cfg(windows)]
        {
            let _ = runtime_dir;
            Self::acquire_windows(app_id, args, on_secondary_args)
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = (runtime_dir, args, on_secondary_args);
            Ok(SingleInstanceStatus::Primary(Self::handle_only(app_id)))
        }
    }

    #[cfg(not(any(unix, windows)))]
    fn handle_only(app_id: &str) -> Self {
        Self {
            app_id: app_id.to_string(),
            running: Arc::new(AtomicBool::new(true)),
        }
    }

    #[cfg(unix)]
    fn acquire_unix<F>(
        app_id: &str,
        args: &[String],
        runtime_dir: &std::path::Path,
        on_secondary_args: F,
    ) -> Result<SingleInstanceStatus, std::io::Error>
    where
        F: Fn(Vec<String>) + Send + Sync + 'static,
    {
        use std::io::Write;
        use std::os::unix::net::{UnixListener, UnixStream};

        let socket_name = format!("{}.sock", app_id.replace('/', "_"));
        let socket_path = runtime_dir.join(socket_name);

        // Try binding the socket as the primary instance.
        match UnixListener::bind(&socket_path) {
            Ok(listener) => {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o600));
                Ok(SingleInstanceStatus::Primary(Self::establish_unix(
                    app_id,
                    listener,
                    socket_path,
                    on_secondary_args,
                )?))
            }
            Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
                // The path exists. Connect to determine whether the owner is live.
                match UnixStream::connect(&socket_path) {
                    Ok(mut stream) => {
                        // Primary is running: forward CLI arguments and exit cleanly.
                        let payload = args.join("\n");
                        let _ = stream.write_all(payload.as_bytes());
                        let _ = stream.flush();
                        let _ = stream.shutdown(std::net::Shutdown::Write);
                        Ok(SingleInstanceStatus::SecondaryExited)
                    }
                    Err(_) => {
                        // Stale socket left by a crashed primary. Remove and rebind.
                        let _ = std::fs::remove_file(&socket_path);
                        let listener = UnixListener::bind(&socket_path)?;
                        use std::os::unix::fs::PermissionsExt;
                        let _ = std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o600));
                        Ok(SingleInstanceStatus::Primary(Self::establish_unix(
                            app_id,
                            listener,
                            socket_path,
                            on_secondary_args,
                        )?))
                    }
                }
            }
            Err(e) => Err(e),
        }
    }

    #[cfg(unix)]
    fn establish_unix<F>(
        app_id: &str,
        listener: std::os::unix::net::UnixListener,
        socket_path: PathBuf,
        on_secondary_args: F,
    ) -> Result<Self, std::io::Error>
    where
        F: Fn(Vec<String>) + Send + Sync + 'static,
    {
        let running = Arc::new(AtomicBool::new(true));
        let on_args = Arc::new(on_secondary_args);
        Self::spawn_unix_listener(listener, Arc::clone(&running), on_args)?;
        Ok(Self {
            app_id: app_id.to_string(),
            running,
            socket_path,
        })
    }

    #[cfg(unix)]
    fn spawn_unix_listener<F>(
        listener: std::os::unix::net::UnixListener,
        running: Arc<AtomicBool>,
        on_args: Arc<F>,
    ) -> Result<(), std::io::Error>
    where
        F: Fn(Vec<String>) + Send + Sync + 'static,
    {
        use std::io::Read;

        std::thread::Builder::new()
            .name("strata-single-instance-listener".to_string())
            .spawn(move || {
                while running.load(Ordering::Relaxed) {
                    if let Ok((mut stream, _)) = listener.accept() {
                        let mut payload = Vec::new();
                        if stream.read_to_end(&mut payload).is_ok() {
                            if let Ok(raw) = String::from_utf8(payload) {
                                let parsed_args: Vec<String> = raw
                                    .lines()
                                    .map(|line| line.to_string())
                                    .filter(|line| !line.is_empty())
                                    .collect();
                                on_args(parsed_args);
                            }
                        }
                    }
                }
            })?;
        Ok(())
    }

    #[cfg(windows)]
    fn acquire_windows<F>(
        app_id: &str,
        args: &[String],
        on_secondary_args: F,
    ) -> Result<SingleInstanceStatus, std::io::Error>
    where
        F: Fn(Vec<String>) + Send + Sync + 'static,
    {
        // If a primary already owns the pipe, forward arguments and exit.
        if windows_ipc::forward_to_primary(app_id, args) {
            return Ok(SingleInstanceStatus::SecondaryExited);
        }

        let handle = match windows_ipc::create_server(app_id) {
            Some(handle) => handle,
            None => {
                // Another process won the first-instance race after our initial
                // connect attempt; forward to it instead of failing.
                let create_error = std::io::Error::last_os_error();
                if windows_ipc::forward_to_primary(app_id, args) {
                    return Ok(SingleInstanceStatus::SecondaryExited);
                }
                return Err(create_error);
            }
        };

        let running = Arc::new(AtomicBool::new(true));
        let on_args = Arc::new(on_secondary_args);
        if let Err(error) =
            windows_ipc::spawn_server_listener(handle, Arc::clone(&running), on_args)
        {
            windows_ipc::close(handle);
            return Err(error);
        }

        Ok(SingleInstanceStatus::Primary(Self {
            app_id: app_id.to_string(),
            running,
            pipe_handle: handle,
        }))
    }

    /// Returns the unique application identifier for this lock.
    pub fn app_id(&self) -> &str {
        &self.app_id
    }
}

impl Drop for SingleInstanceLock {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        #[cfg(unix)]
        {
            let _ = std::fs::remove_file(&self.socket_path);
        }
        #[cfg(windows)]
        {
            windows_ipc::close(self.pipe_handle);
        }
    }
}

/// Raw `kernel32` named-pipe transport used by the Windows single-instance path.
///
/// The pipe name is `\\.\pipe\<sanitized-app-id>`. The primary creates a duplex
/// byte-mode server instance and listens for newline-separated CLI arguments;
/// a secondary opens the pipe, writes its arguments, and exits. This module is
/// compile-gated to Windows and is not exercised on other hosts.
#[cfg(windows)]
mod windows_ipc {
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    type Handle = *mut c_void;

    const INVALID_HANDLE_VALUE: Handle = -1isize as Handle;
    const PIPE_ACCESS_DUPLEX: u32 = 0x0000_0003;
    const FILE_FLAG_FIRST_PIPE_INSTANCE: u32 = 0x0008_0000;
    const PIPE_TYPE_BYTE: u32 = 0x0000_0000;
    const PIPE_READMODE_BYTE: u32 = 0x0000_0000;
    const PIPE_WAIT: u32 = 0x0000_0000;
    const PIPE_UNLIMITED_INSTANCES: u32 = 255;
    const GENERIC_WRITE: u32 = 0x4000_0000;
    const OPEN_EXISTING: u32 = 3;
    const ERROR_PIPE_BUSY: u32 = 231;
    const ERROR_PIPE_CONNECTED: u32 = 535;
    const BUFFER_SIZE: u32 = 8192;

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateNamedPipeW(
            name: *const u16,
            open_mode: u32,
            pipe_mode: u32,
            max_instances: u32,
            out_buffer_size: u32,
            in_buffer_size: u32,
            default_timeout: u32,
            security_attributes: *mut c_void,
        ) -> Handle;
        fn ConnectNamedPipe(pipe: Handle, overlapped: *mut c_void) -> i32;
        fn DisconnectNamedPipe(pipe: Handle) -> i32;
        fn ReadFile(
            file: Handle,
            buffer: *mut u8,
            bytes_to_read: u32,
            bytes_read: *mut u32,
            overlapped: *mut c_void,
        ) -> i32;
        fn WriteFile(
            file: Handle,
            buffer: *const u8,
            bytes_to_write: u32,
            bytes_written: *mut u32,
            overlapped: *mut c_void,
        ) -> i32;
        fn CreateFileW(
            name: *const u16,
            desired_access: u32,
            share_mode: u32,
            security_attributes: *mut c_void,
            creation_disposition: u32,
            flags_and_attributes: u32,
            template_file: Handle,
        ) -> Handle;
        fn CloseHandle(object: Handle) -> i32;
        fn GetLastError() -> u32;
    }

    fn pipe_name(app_id: &str) -> Vec<u16> {
        let sanitized: String = app_id
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() || ch == '.' || ch == '-' || ch == '_' {
                    ch
                } else {
                    '-'
                }
            })
            .collect();
        format!("\\\\.\\pipe\\{}", sanitized)
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect()
    }

    fn to_handle(value: usize) -> Handle {
        value as Handle
    }

    pub(super) fn forward_to_primary(app_id: &str, args: &[String]) -> bool {
        let name = pipe_name(app_id);
        let mut payload = args.join("\n");
        if payload.is_empty() {
            payload.push('\n');
        }
        let bytes = payload.as_bytes();

        for _ in 0..40 {
            let handle = unsafe {
                CreateFileW(
                    name.as_ptr(),
                    GENERIC_WRITE,
                    0,
                    std::ptr::null_mut(),
                    OPEN_EXISTING,
                    0,
                    std::ptr::null_mut(),
                )
            };
            if handle != INVALID_HANDLE_VALUE {
                let mut written = 0u32;
                let ok = unsafe {
                    WriteFile(
                        handle,
                        bytes.as_ptr(),
                        bytes.len() as u32,
                        &mut written,
                        std::ptr::null_mut(),
                    )
                };
                unsafe {
                    CloseHandle(handle);
                }
                return ok != 0;
            }
            let error = unsafe { GetLastError() };
            if error != ERROR_PIPE_BUSY {
                return false;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        false
    }

    pub(super) fn create_server(app_id: &str) -> Option<usize> {
        let name = pipe_name(app_id);
        let handle = unsafe {
            CreateNamedPipeW(
                name.as_ptr(),
                PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
                PIPE_UNLIMITED_INSTANCES,
                BUFFER_SIZE,
                BUFFER_SIZE,
                0,
                std::ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            None
        } else {
            Some(handle as usize)
        }
    }

    pub(super) fn spawn_server_listener<F>(
        handle: usize,
        running: Arc<AtomicBool>,
        on_args: Arc<F>,
    ) -> Result<(), std::io::Error>
    where
        F: Fn(Vec<String>) + Send + Sync + 'static,
    {
        std::thread::Builder::new()
            .name("strata-single-instance-pipe".to_string())
            .spawn(move || {
                let pipe = to_handle(handle);
                let mut buffer = vec![0u8; BUFFER_SIZE as usize];

                while running.load(Ordering::Relaxed) {
                    let connected = unsafe { ConnectNamedPipe(pipe, std::ptr::null_mut()) };
                    if connected == 0 {
                        let error = unsafe { GetLastError() };
                        if error != ERROR_PIPE_CONNECTED {
                            break;
                        }
                    }

                    loop {
                        let mut bytes_read = 0u32;
                        let ok = unsafe {
                            ReadFile(
                                pipe,
                                buffer.as_mut_ptr(),
                                buffer.len() as u32,
                                &mut bytes_read,
                                std::ptr::null_mut(),
                            )
                        };
                        if ok == 0 || bytes_read == 0 {
                            break;
                        }
                        let raw = String::from_utf8_lossy(&buffer[..bytes_read as usize]);
                        let parsed: Vec<String> = raw
                            .lines()
                            .map(|line| line.to_string())
                            .filter(|line| !line.is_empty())
                            .collect();
                        on_args(parsed);
                        if (bytes_read as usize) < buffer.len() {
                            break;
                        }
                    }

                    unsafe {
                        DisconnectNamedPipe(pipe);
                    }
                }
            })?;
        Ok(())
    }

    pub(super) fn close(handle: usize) {
        if handle != 0 {
            unsafe {
                CloseHandle(to_handle(handle));
            }
        }
    }
}
