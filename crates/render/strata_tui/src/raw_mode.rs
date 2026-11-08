//! Terminal Raw Mode Management, Alternate Screen Buffers, and Panic Safety.

use std::io::{self, stdout, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

static PANIC_HOOK_SET: AtomicBool = AtomicBool::new(false);

#[cfg(unix)]
static SAVED_TERMIOS: Mutex<Option<libc::termios>> = Mutex::new(None);

#[cfg(windows)]
static SAVED_CONSOLE_MODE: Mutex<Option<(u32, u32)>> = Mutex::new(None);

/// RAII Guard that manages terminal raw mode, alternate screen buffer, and cursor state.
/// When dropped, it automatically restores the original terminal state and cursor.
pub struct RawModeGuard {
    active: bool,
    #[cfg(unix)]
    orig_termios: Option<libc::termios>,
    #[cfg(windows)]
    orig_mode: Option<(u32, u32)>,
}

impl RawModeGuard {
    /// Disarms the guard so it does not restore the terminal on drop.
    pub fn disarm(&mut self) {
        self.active = false;
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        if self.active {
            let _ = restore_terminal(
                #[cfg(unix)]
                self.orig_termios,
                #[cfg(windows)]
                self.orig_mode,
            );
        }
    }
}

/// Enables raw terminal mode, switches to the alternate screen buffer, hides cursor,
/// and enables mouse and bracketed paste tracking.
pub fn enable_raw_mode() -> io::Result<RawModeGuard> {
    setup_panic_hook();

    #[cfg(unix)]
    {
        let is_tty = unsafe { libc::isatty(libc::STDIN_FILENO) == 1 };
        if !is_tty {
            // Not a TTY (e.g. CI pipeline or redirected pipe) - return inert guard
            return Ok(RawModeGuard {
                active: true,
                orig_termios: None,
            });
        }

        let mut orig = std::mem::MaybeUninit::<libc::termios>::uninit();
        let ret = unsafe { libc::tcgetattr(libc::STDIN_FILENO, orig.as_mut_ptr()) };
        if ret != 0 {
            return Err(io::Error::last_os_error());
        }
        let orig = unsafe { orig.assume_init() };

        // Save globally for panic recovery
        if let Ok(mut lock) = SAVED_TERMIOS.lock() {
            *lock = Some(orig);
        }

        let mut raw = orig;
        // Input flags: disable break condition, CR-to-NL, parity check, strip 8th bit, software flow control
        raw.c_iflag &= !(libc::BRKINT | libc::ICRNL | libc::INPCK | libc::ISTRIP | libc::IXON);
        // Output flags: disable post-processing
        raw.c_oflag &= !libc::OPOST;
        // Control flags: 8-bit characters
        raw.c_cflag |= libc::CS8;
        // Local flags: disable echo, canonical mode, extended input processing, signal generation (Ctrl+C, Ctrl+Z)
        raw.c_lflag &= !(libc::ECHO | libc::ICANON | libc::IEXTEN | libc::ISIG);

        // Control characters: non-blocking read with 100ms timeout
        raw.c_cc[libc::VMIN] = 0;
        raw.c_cc[libc::VTIME] = 1;

        let ret = unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSAFLUSH, &raw) };
        if ret != 0 {
            return Err(io::Error::last_os_error());
        }

        enter_tui_terminal_mode()?;

        Ok(RawModeGuard {
            active: true,
            orig_termios: Some(orig),
        })
    }

    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Console::{
            GetConsoleMode, GetStdHandle, SetConsoleMode, ENABLE_ECHO_INPUT,
            ENABLE_LINE_INPUT, ENABLE_MOUSE_INPUT, ENABLE_PROCESSED_INPUT,
            ENABLE_VIRTUAL_TERMINAL_INPUT, ENABLE_VIRTUAL_TERMINAL_PROCESSING,
            ENABLE_WINDOW_INPUT, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
        };

        unsafe {
            let in_handle = GetStdHandle(STD_INPUT_HANDLE);
            let out_handle = GetStdHandle(STD_OUTPUT_HANDLE);

            let mut orig_in_mode = 0u32;
            let mut orig_out_mode = 0u32;

            if GetConsoleMode(in_handle, &mut orig_in_mode) == 0 {
                return Ok(RawModeGuard {
                    active: true,
                    orig_mode: None,
                });
            }
            GetConsoleMode(out_handle, &mut orig_out_mode);

            if let Ok(mut lock) = SAVED_CONSOLE_MODE.lock() {
                *lock = Some((orig_in_mode, orig_out_mode));
            }

            let raw_in = (orig_in_mode | ENABLE_VIRTUAL_TERMINAL_INPUT | ENABLE_WINDOW_INPUT | ENABLE_MOUSE_INPUT)
                & !(ENABLE_LINE_INPUT | ENABLE_ECHO_INPUT | ENABLE_PROCESSED_INPUT);
            let raw_out = orig_out_mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING;

            SetConsoleMode(in_handle, raw_in);
            SetConsoleMode(out_handle, raw_out);

            enter_tui_terminal_mode()?;

            Ok(RawModeGuard {
                active: true,
                orig_mode: Some((orig_in_mode, orig_out_mode)),
            })
        }
    }

    #[cfg(not(any(unix, windows)))]
    {
        enter_tui_terminal_mode()?;
        Ok(RawModeGuard { active: true })
    }
}

/// Sends ANSI escape sequences to switch to alternate screen, hide cursor, and enable mouse & paste.
fn enter_tui_terminal_mode() -> io::Result<()> {
    let mut out = stdout();
    // \x1b[?1049h : Alternate Screen Buffer
    // \x1b[?25l   : Hide Cursor
    // \x1b[?1000h : Normal Mouse Tracking
    // \x1b[?1002h : Button-Event Mouse Tracking
    // \x1b[?1006h : SGR Extended Mouse Reporting
    // \x1b[?2004h : Bracketed Paste Mode
    out.write_all(b"\x1b[?1049h\x1b[?25l\x1b[?1000h\x1b[?1002h\x1b[?1006h\x1b[?2004h")?;
    out.flush()
}

/// Restores the cooked terminal mode, cursor, normal screen buffer, and disables mouse reporting.
pub fn restore_terminal(
    #[cfg(unix)] orig_termios: Option<libc::termios>,
    #[cfg(windows)] orig_mode: Option<(u32, u32)>,
) -> io::Result<()> {
    let mut out = stdout();
    // \x1b[?2004l : Disable Bracketed Paste Mode
    // \x1b[?1006l : Disable SGR Mouse
    // \x1b[?1002l : Disable Button Mouse
    // \x1b[?1000l : Disable Normal Mouse
    // \x1b[?25h   : Show Cursor
    // \x1b[?1049l : Normal Screen Buffer
    // \x1b[0m     : Reset SGR Attributes
    let _ = out.write_all(b"\x1b[?2004l\x1b[?1006l\x1b[?1002l\x1b[?1000l\x1b[?25h\x1b[?1049l\x1b[0m");
    let _ = out.flush();

    #[cfg(unix)]
    if let Some(orig) = orig_termios {
        unsafe {
            libc::tcsetattr(libc::STDIN_FILENO, libc::TCSAFLUSH, &orig);
        }
    }

    #[cfg(windows)]
    if let Some((in_mode, out_mode)) = orig_mode {
        use windows_sys::Win32::System::Console::{GetStdHandle, SetConsoleMode, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE};
        unsafe {
            SetConsoleMode(GetStdHandle(STD_INPUT_HANDLE), in_mode);
            SetConsoleMode(GetStdHandle(STD_OUTPUT_HANDLE), out_mode);
        }
    }

    Ok(())
}

/// Sets up a global panic hook that guarantees the terminal is restored to cooked mode
/// if the process encounters an unexpected panic.
fn setup_panic_hook() {
    if !PANIC_HOOK_SET.swap(true, Ordering::SeqCst) {
        let prev_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            #[cfg(unix)]
            let termios = SAVED_TERMIOS.lock().ok().and_then(|g| *g);
            #[cfg(windows)]
            let mode = SAVED_CONSOLE_MODE.lock().ok().and_then(|g| *g);

            let _ = restore_terminal(
                #[cfg(unix)]
                termios,
                #[cfg(windows)]
                mode,
            );
            prev_hook(info);
        }));
    }
}

/// Queries the current terminal screen dimensions (cols, rows).
pub fn get_terminal_size() -> (u16, u16) {
    #[cfg(unix)]
    {
        let mut ws = std::mem::MaybeUninit::<libc::winsize>::uninit();
        let ret = unsafe { libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, ws.as_mut_ptr()) };
        if ret == 0 {
            let ws = unsafe { ws.assume_init() };
            if ws.ws_col > 0 && ws.ws_row > 0 {
                return (ws.ws_col, ws.ws_row);
            }
        }
    }

    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Console::{
            GetConsoleScreenBufferInfo, GetStdHandle, CONSOLE_SCREEN_BUFFER_INFO, STD_OUTPUT_HANDLE,
        };
        unsafe {
            let out_handle = GetStdHandle(STD_OUTPUT_HANDLE);
            let mut info = std::mem::zeroed::<CONSOLE_SCREEN_BUFFER_INFO>();
            if GetConsoleScreenBufferInfo(out_handle, &mut info) != 0 {
                let cols = (info.srWindow.Right - info.srWindow.Left + 1) as u16;
                let rows = (info.srWindow.Bottom - info.srWindow.Top + 1) as u16;
                if cols > 0 && rows > 0 {
                    return (cols, rows);
                }
            }
        }
    }

    // Environment variable fallback
    if let (Ok(cols_str), Ok(rows_str)) = (std::env::var("COLUMNS"), std::env::var("LINES")) {
        if let (Ok(c), Ok(r)) = (cols_str.parse::<u16>(), rows_str.parse::<u16>()) {
            if c > 0 && r > 0 {
                return (c, r);
            }
        }
    }

    // Canonical default terminal size
    (80, 24)
}
