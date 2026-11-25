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

/// Serializes a slice of arguments into length-prefixed binary wire format.
///
/// Wire format:
/// - 4-byte little-endian u32: argument count `N`
/// - For each argument:
///   - 4-byte little-endian u32: byte length `L`
///   - `L` UTF-8 payload bytes
pub fn encode_args(args: &[String]) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(&(args.len() as u32).to_le_bytes());
    for arg in args {
        let bytes = arg.as_bytes();
        buf.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        buf.extend_from_slice(bytes);
    }
    buf
}

/// Decodes arguments from the length-prefixed binary wire format.
///
/// Provides graceful backwards-compatible fallback for null-separated and newline-separated
/// payloads if length-prefixed framing is absent.
pub fn decode_args(payload: &[u8]) -> Vec<String> {
    if payload.len() >= 4 {
        let count = u32::from_le_bytes(payload[0..4].try_into().unwrap()) as usize;
        let mut offset = 4;
        let mut args = Vec::with_capacity(count.min(1024));
        let mut ok = true;
        for _ in 0..count {
            if offset + 4 > payload.len() {
                ok = false;
                break;
            }
            let len = u32::from_le_bytes(payload[offset..offset + 4].try_into().unwrap()) as usize;
            offset += 4;
            if offset + len > payload.len() {
                ok = false;
                break;
            }
            if let Ok(s) = std::str::from_utf8(&payload[offset..offset + len]) {
                args.push(s.to_string());
            } else {
                args.push(String::from_utf8_lossy(&payload[offset..offset + len]).into_owned());
            }
            offset += len;
        }
        if ok && offset == payload.len() {
            return args;
        }
    }

    // Backwards-compatible fallback: null-byte or newline delimited payload
    if let Ok(raw) = std::str::from_utf8(payload) {
        if raw.contains('\0') {
            return raw.split('\0').map(|s| s.to_string()).collect();
        }
        return raw
            .lines()
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }

    Vec::new()
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
    listener_thread: Option<std::thread::JoinHandle<()>>,
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
            let runtime_dir = match std::env::var_os("XDG_RUNTIME_DIR") {
                Some(p) => PathBuf::from(p),
                None => {
                    extern "C" {
                        fn getuid() -> u32;
                    }
                    let uid = unsafe { getuid() };
                    let dir = PathBuf::from(format!("/tmp/strata-runtime-{}", uid));
                    use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};

                    if let Ok(meta) = std::fs::symlink_metadata(&dir) {
                        if meta.file_type().is_symlink() {
                            return Err(std::io::Error::new(
                                std::io::ErrorKind::PermissionDenied,
                                format!("Security violation: runtime directory {:?} is an insecure symlink (CWE-59)", dir),
                            ));
                        }
                        if meta.uid() != uid {
                            return Err(std::io::Error::new(
                                std::io::ErrorKind::PermissionDenied,
                                format!("Security violation: runtime directory {:?} is not owned by current user (uid {})", dir, uid),
                            ));
                        }
                        if meta.permissions().mode() & 0o777 != 0o700 {
                            let _ = std::fs::set_permissions(
                                &dir,
                                std::fs::Permissions::from_mode(0o700),
                            );
                        }
                    } else {
                        let mut builder = std::fs::DirBuilder::new();
                        builder.mode(0o700);
                        if let Err(e) = builder.create(&dir) {
                            if e.kind() != std::io::ErrorKind::AlreadyExists {
                                return Err(e);
                            }
                        }
                        let meta = std::fs::symlink_metadata(&dir)?;
                        if meta.file_type().is_symlink() || meta.uid() != uid {
                            return Err(std::io::Error::new(
                                std::io::ErrorKind::PermissionDenied,
                                format!("Security violation: created runtime directory {:?} failed verification", dir),
                            ));
                        }
                        let _ =
                            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
                    }
                    dir
                }
            };
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
            listener_thread: None,
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

        // Security check: remove any untrusted symlink before socket binding (CWE-59)
        if let Ok(meta) = std::fs::symlink_metadata(&socket_path) {
            if meta.file_type().is_symlink() {
                let _ = std::fs::remove_file(&socket_path);
            }
        }

        // Try binding the socket as the primary instance.
        match UnixListener::bind(&socket_path) {
            Ok(listener) => {
                use std::os::unix::fs::PermissionsExt;
                let _ =
                    std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o600));
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
                        let payload = encode_args(args);
                        let _ = stream.write_all(&payload);
                        let _ = stream.flush();
                        let _ = stream.shutdown(std::net::Shutdown::Write);
                        Ok(SingleInstanceStatus::SecondaryExited)
                    }
                    Err(_) => {
                        // Stale socket left by a crashed primary. Remove and rebind.
                        let _ = std::fs::remove_file(&socket_path);
                        let listener = UnixListener::bind(&socket_path)?;
                        use std::os::unix::fs::PermissionsExt;
                        let _ = std::fs::set_permissions(
                            &socket_path,
                            std::fs::Permissions::from_mode(0o600),
                        );
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
        let thread = Self::spawn_unix_listener(listener, Arc::clone(&running), on_args)?;
        Ok(Self {
            app_id: app_id.to_string(),
            running,
            socket_path,
            listener_thread: Some(thread),
        })
    }

    #[cfg(unix)]
    fn spawn_unix_listener<F>(
        listener: std::os::unix::net::UnixListener,
        running: Arc<AtomicBool>,
        on_args: Arc<F>,
    ) -> Result<std::thread::JoinHandle<()>, std::io::Error>
    where
        F: Fn(Vec<String>) + Send + Sync + 'static,
    {
        use std::io::Read;

        listener.set_nonblocking(true)?;

        let handle = std::thread::Builder::new()
            .name("strata-single-instance-listener".to_string())
            .spawn(move || {
                while running.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            let _ = stream.set_nonblocking(false);
                            let mut payload = Vec::new();
                            if stream.read_to_end(&mut payload).is_ok() {
                                let parsed_args = decode_args(&payload);
                                on_args(parsed_args);
                            }
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(std::time::Duration::from_millis(50));
                        }
                        Err(_) => break,
                    }
                }
            })?;
        Ok(handle)
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
        let thread = match windows_ipc::spawn_server_listener(handle, Arc::clone(&running), on_args)
        {
            Ok(t) => t,
            Err(error) => {
                windows_ipc::close(handle);
                return Err(error);
            }
        };

        Ok(SingleInstanceStatus::Primary(Self {
            app_id: app_id.to_string(),
            running,
            pipe_handle: handle,
            listener_thread: Some(thread),
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
            windows_ipc::unblock_server(&self.app_id);
            windows_ipc::cancel_io(self.pipe_handle);
        }
        if let Some(thread) = self.listener_thread.take() {
            let _ = thread.join();
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
    const PIPE_REJECT_REMOTE_CLIENTS: u32 = 0x0000_0008;
    const PIPE_UNLIMITED_INSTANCES: u32 = 255;
    const GENERIC_WRITE: u32 = 0x4000_0000;
    const OPEN_EXISTING: u32 = 3;
    const ERROR_PIPE_BUSY: u32 = 231;
    const ERROR_NO_DATA: u32 = 232;
    const ERROR_PIPE_CONNECTED: u32 = 535;
    const TOKEN_QUERY: u32 = 0x0008;
    const TOKEN_USER_INFO_CLASS: u32 = 1;
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
        fn FlushFileBuffers(file: Handle) -> i32;
        fn CreateFileW(
            name: *const u16,
            desired_access: u32,
            share_mode: u32,
            security_attributes: *mut c_void,
            creation_disposition: u32,
            flags_and_attributes: u32,
            template_file: Handle,
        ) -> Handle;
        fn CancelIoEx(file: Handle, overlapped: *mut c_void) -> i32;
        fn GetCurrentProcess() -> Handle;
        fn CloseHandle(object: Handle) -> i32;
        fn GetLastError() -> u32;
        fn LocalFree(h_mem: *mut c_void) -> *mut c_void;
    }

    #[link(name = "advapi32")]
    extern "system" {
        fn OpenProcessToken(
            process_handle: Handle,
            desired_access: u32,
            token_handle: *mut Handle,
        ) -> i32;
        fn GetTokenInformation(
            token_handle: Handle,
            token_information_class: u32,
            token_information: *mut c_void,
            token_information_length: u32,
            return_length: *mut u32,
        ) -> i32;
        fn ConvertSidToStringSidW(sid: *mut c_void, string_sid: *mut *mut u16) -> i32;
        fn ConvertStringSecurityDescriptorToSecurityDescriptorW(
            string_security_descriptor: *const u16,
            string_sd_revision: u32,
            security_descriptor: *mut *mut c_void,
            security_descriptor_size: *mut u32,
        ) -> i32;
    }

    #[repr(C)]
    struct TokenUserHeader {
        sid: *mut c_void,
        attributes: u32,
    }

    /// Queries the current process user SID string (e.g. `S-1-5-21-...`) for strict per-user pipe DACL.
    fn current_user_sid_string() -> Option<String> {
        let mut token: Handle = std::ptr::null_mut();
        let ok = unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) };
        if ok == 0 || token.is_null() {
            return None;
        }
        let mut needed: u32 = 0;
        unsafe {
            GetTokenInformation(
                token,
                TOKEN_USER_INFO_CLASS,
                std::ptr::null_mut(),
                0,
                &mut needed,
            );
        }
        if needed == 0 {
            unsafe {
                CloseHandle(token);
            }
            return None;
        }
        let mut buf = vec![0u8; needed as usize];
        let info_ok = unsafe {
            GetTokenInformation(
                token,
                TOKEN_USER_INFO_CLASS,
                buf.as_mut_ptr() as *mut c_void,
                needed,
                &mut needed,
            )
        };
        unsafe {
            CloseHandle(token);
        }
        if info_ok == 0 || buf.len() < std::mem::size_of::<TokenUserHeader>() {
            return None;
        }
        let header = unsafe { &*(buf.as_ptr() as *const TokenUserHeader) };
        if header.sid.is_null() {
            return None;
        }
        let mut sid_wide: *mut u16 = std::ptr::null_mut();
        let conv_ok = unsafe { ConvertSidToStringSidW(header.sid, &mut sid_wide) };
        if conv_ok == 0 || sid_wide.is_null() {
            return None;
        }
        let mut len = 0usize;
        unsafe {
            while *sid_wide.add(len) != 0 {
                len += 1;
            }
        }
        let slice = unsafe { std::slice::from_raw_parts(sid_wide, len) };
        let result = String::from_utf16_lossy(slice);
        unsafe {
            LocalFree(sid_wide as *mut c_void);
        }
        Some(result)
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

    #[repr(C)]
    struct SecurityAttributes {
        n_length: u32,
        lp_security_descriptor: *mut c_void,
        b_inherit_handle: i32,
    }

    pub(super) fn forward_to_primary(app_id: &str, args: &[String]) -> bool {
        let name = pipe_name(app_id);
        let bytes = super::encode_args(args);

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
                if ok != 0 {
                    unsafe {
                        FlushFileBuffers(handle);
                    }
                }
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

        // Enforce explicit Win32 DACL granting Generic All strictly to the current user SID.
        let mut p_sd: *mut c_void = std::ptr::null_mut();
        if let Some(user_sid) = current_user_sid_string() {
            let sddl = format!("D:(A;;GA;;;{})\0", user_sid)
                .encode_utf16()
                .collect::<Vec<u16>>();
            let mut sd_size = 0u32;
            let has_sd = unsafe {
                ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    sddl.as_ptr(),
                    1, // SDDL_REVISION_1
                    &mut p_sd,
                    &mut sd_size,
                )
            };
            if has_sd == 0 {
                p_sd = std::ptr::null_mut();
            }
        }

        let mut sa = SecurityAttributes {
            n_length: std::mem::size_of::<SecurityAttributes>() as u32,
            lp_security_descriptor: p_sd,
            b_inherit_handle: 0,
        };
        let sa_ptr = if p_sd.is_null() {
            std::ptr::null_mut()
        } else {
            &mut sa as *mut _ as *mut c_void
        };

        let handle = unsafe {
            CreateNamedPipeW(
                name.as_ptr(),
                PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                PIPE_UNLIMITED_INSTANCES,
                BUFFER_SIZE,
                BUFFER_SIZE,
                0,
                sa_ptr,
            )
        };

        if !p_sd.is_null() {
            unsafe {
                LocalFree(p_sd);
            }
        }

        if handle == INVALID_HANDLE_VALUE {
            None
        } else {
            Some(handle as usize)
        }
    }

    pub(super) fn cancel_io(handle: usize) {
        if handle != 0 {
            unsafe {
                CancelIoEx(to_handle(handle), std::ptr::null_mut());
            }
        }
    }

    pub(super) fn unblock_server(app_id: &str) {
        let name = pipe_name(app_id);
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
            unsafe {
                CloseHandle(handle);
            }
        }
    }

    pub(super) fn spawn_server_listener<F>(
        handle: usize,
        running: Arc<AtomicBool>,
        on_args: Arc<F>,
    ) -> Result<std::thread::JoinHandle<()>, std::io::Error>
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
                        if error != ERROR_PIPE_CONNECTED && error != ERROR_NO_DATA {
                            break;
                        }
                    }

                    if !running.load(Ordering::Relaxed) {
                        unsafe {
                            DisconnectNamedPipe(pipe);
                        }
                        break;
                    }

                    let mut payload = Vec::new();
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
                        payload.extend_from_slice(&buffer[..bytes_read as usize]);
                        if (bytes_read as usize) < buffer.len() {
                            break;
                        }
                    }

                    if !payload.is_empty() {
                        let parsed = super::decode_args(&payload);
                        on_args(parsed);
                    }

                    unsafe {
                        DisconnectNamedPipe(pipe);
                    }
                }
            })
    }

    pub(super) fn close(handle: usize) {
        if handle != 0 {
            unsafe {
                CloseHandle(to_handle(handle));
            }
        }
    }
}
