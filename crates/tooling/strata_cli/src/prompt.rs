//! Reusable, zero-dependency TUI prompt primitives for the Strata CLI.
//!
//! Provides interactive terminal prompts with:
//! - Terminal raw mode and ANSI escape sequence control.
//! - Arrow key (Up/Down) & Vim key (j/k) navigation.
//! - Dynamic in-place line clearing and prompt collapsing (Next.js / Clack style).
//! - Non-TTY fallback for deterministic streaming, pipes, and automated unit testing.

use std::io::{BufRead, Read, Write};

#[cfg(unix)]
mod sys {
    extern "C" {
        pub fn isatty(fd: i32) -> i32;
        pub fn tcgetattr(fd: i32, termios_p: *mut Termios) -> i32;
        pub fn tcsetattr(fd: i32, optional_actions: i32, termios_p: *const Termios) -> i32;
        pub fn write(fd: i32, buf: *const u8, count: usize) -> isize;
        pub fn _exit(status: i32) -> !;
        pub fn signal(signum: i32, handler: extern "C" fn(i32)) -> usize;
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct Termios {
        pub c_iflag: u32,
        pub c_oflag: u32,
        pub c_cflag: u32,
        pub c_lflag: u32,
        pub c_line: u8,
        pub c_cc: [u8; 32],
        pub c_ispeed: u32,
        pub c_ospeed: u32,
    }

    pub const TCSANOW: i32 = 0;
    pub const ICANON: u32 = 0o000002;
    pub const ECHO: u32 = 0o000010;
    pub const ISIG: u32 = 0o000001;
    pub const SIGINT: i32 = 2;
    pub const SIGTERM: i32 = 15;
}

#[cfg(unix)]
static mut SAVED_TERMIOS: Option<sys::Termios> = None;
#[cfg(unix)]
static RAW_MODE_ACTIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[cfg(unix)]
extern "C" fn sigint_handler(_sig: i32) {
    unsafe {
        let msg = b"\x1b[?25h\r\n";
        let _ = sys::write(1, msg.as_ptr(), msg.len());

        if RAW_MODE_ACTIVE.load(std::sync::atomic::Ordering::SeqCst) {
            if let Some(ref orig) = SAVED_TERMIOS {
                let _ = sys::tcsetattr(0, sys::TCSANOW, orig);
            }
        }

        sys::_exit(130);
    }
}

/// RAII Guard that puts standard input in raw mode and restores canonical mode on drop.
pub struct RawModeGuard {
    #[cfg(unix)]
    orig_termios: sys::Termios,
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        #[cfg(unix)]
        unsafe {
            RAW_MODE_ACTIVE.store(false, std::sync::atomic::Ordering::SeqCst);
            sys::tcsetattr(0, sys::TCSANOW, &self.orig_termios);
        }
        // Always restore cursor visibility on drop
        Terminal::restore_cursor();
    }
}

/// Terminal helper methods for TUI prompt interactions.
pub struct Terminal;

impl Terminal {
    /// Restores terminal cursor visibility and flushes stdout.
    pub fn restore_cursor() {
        let _ = std::io::stdout().write_all(b"\x1b[?25h");
        let _ = std::io::stdout().flush();
    }

    /// Returns true if both standard input and standard output are connected to a TTY.
    pub fn is_interactive() -> bool {
        #[cfg(unix)]
        unsafe {
            sys::isatty(0) != 0 && sys::isatty(1) != 0
        }
        #[cfg(not(unix))]
        {
            false
        }
    }

    /// Attempts to enable terminal raw mode. Returns a guard that restores canonical mode on drop.
    pub fn enable_raw_mode() -> Option<RawModeGuard> {
        #[cfg(unix)]
        unsafe {
            if sys::isatty(0) == 0 {
                return None;
            }
            let mut orig: sys::Termios = std::mem::zeroed();
            if sys::tcgetattr(0, &mut orig) != 0 {
                return None;
            }
            SAVED_TERMIOS = Some(orig);
            RAW_MODE_ACTIVE.store(true, std::sync::atomic::Ordering::SeqCst);

            sys::signal(sys::SIGINT, sigint_handler);
            sys::signal(sys::SIGTERM, sigint_handler);

            let mut raw = orig;
            // Disable canonical mode, local echo, and ISIG so Ctrl-C is intercepted
            raw.c_lflag &= !(sys::ICANON | sys::ECHO | sys::ISIG);
            // Minimum number of characters for noncanonical read
            raw.c_cc[6] = 1; // VMIN
            raw.c_cc[5] = 0; // VTIME
            if sys::tcsetattr(0, sys::TCSANOW, &raw) != 0 {
                return None;
            }
            Some(RawModeGuard { orig_termios: orig })
        }
        #[cfg(not(unix))]
        {
            None
        }
    }
}

/// Represents a parsed keyboard event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Up,
    Down,
    Left,
    Right,
    Enter,
    Backspace,
    Tab,
    Esc,
    CtrlC,
    CtrlD,
    Unknown,
}

/// Reads a single key event from a reader.
pub fn read_key<R: Read>(reader: &mut R) -> std::io::Result<Option<Key>> {
    let mut buf = [0u8; 1];
    let n = match reader.read(&mut buf) {
        Ok(0) => return Ok(None),
        Ok(n) => n,
        Err(e) => return Err(e),
    };
    if n == 0 {
        return Ok(None);
    }
    match buf[0] {
        b'\x03' => Ok(Some(Key::CtrlC)),
        b'\x04' => Ok(Some(Key::CtrlD)),
        b'\r' | b'\n' => Ok(Some(Key::Enter)),
        b'\x7f' | b'\x08' => Ok(Some(Key::Backspace)),
        b'\t' => Ok(Some(Key::Tab)),
        b'\x1b' => {
            let mut seq = [0u8; 2];
            match reader.read(&mut seq[0..1]) {
                Ok(1) if seq[0] == b'[' => match reader.read(&mut seq[1..2]) {
                    Ok(1) => match seq[1] {
                        b'A' => Ok(Some(Key::Up)),
                        b'B' => Ok(Some(Key::Down)),
                        b'C' => Ok(Some(Key::Right)),
                        b'D' => Ok(Some(Key::Left)),
                        b'H' => Ok(Some(Key::Up)),
                        b'F' => Ok(Some(Key::Down)),
                        b'3' => {
                            let mut tilde = [0u8; 1];
                            let _ = reader.read(&mut tilde);
                            Ok(Some(Key::Backspace))
                        }
                        _ => Ok(Some(Key::Unknown)),
                    },
                    _ => Ok(Some(Key::Esc)),
                },
                _ => Ok(Some(Key::Esc)),
            }
        }
        c if c.is_ascii() => Ok(Some(Key::Char(c as char))),
        _ => Ok(Some(Key::Unknown)),
    }
}

/// Visual theme and design constants for Strata CLI prompts.
pub struct PromptTheme;

impl PromptTheme {
    pub const SYMBOL_QUESTION: &'static str = "?";
    pub const SYMBOL_SUCCESS: &'static str = "√";
    pub const SYMBOL_POINTER: &'static str = "›";
    pub const SYMBOL_BULLET: &'static str = "•";
    pub const SYMBOL_DELIMITER: &'static str = "›";
    pub const SYMBOL_BANNER: &'static str = "▲";
    pub const SYMBOL_BAR: &'static str = "─";

    pub fn question_prefix() -> &'static str {
        "\x1b[1;36m?\x1b[0m"
    }

    pub fn success_prefix() -> &'static str {
        "\x1b[32m√\x1b[0m"
    }

    pub fn pointer() -> &'static str {
        "\x1b[36m›\x1b[0m"
    }

    pub fn delimiter() -> &'static str {
        "\x1b[90m›\x1b[0m"
    }
}

pub type TextValidator = Box<dyn Fn(&str) -> Result<(), String>>;

/// An interactive single-line text input prompt with dynamic collapsing.
pub struct TextPrompt {
    pub message: String,
    pub default: Option<String>,
    pub validator: Option<TextValidator>,
}

impl TextPrompt {
    /// Creates a new text prompt with a question message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            default: None,
            validator: None,
        }
    }

    /// Sets the optional default value used when Enter is pressed on empty input.
    pub fn with_default(mut self, default: impl Into<String>) -> Self {
        self.default = Some(default.into());
        self
    }

    /// Sets a validation function for input values.
    pub fn with_validator<F>(mut self, validator: F) -> Self
    where
        F: Fn(&str) -> Result<(), String> + 'static,
    {
        self.validator = Some(Box::new(validator));
        self
    }

    /// Executes the text prompt.
    pub fn run<R: BufRead, W: Write>(
        &self,
        reader: &mut R,
        writer: &mut W,
        is_interactive: bool,
    ) -> Result<String, String> {
        if is_interactive {
            self.run_interactive(reader, writer)
        } else {
            self.run_stream(reader, writer)
        }
    }

    fn run_stream<R: BufRead, W: Write>(
        &self,
        reader: &mut R,
        writer: &mut W,
    ) -> Result<String, String> {
        let def_str = self.default.as_deref().unwrap_or("");
        write!(
            writer,
            "{} \x1b[1m{}\x1b[0m {} \x1b[90m({})\x1b[0m ",
            PromptTheme::question_prefix(),
            self.message,
            PromptTheme::delimiter(),
            def_str
        )
        .map_err(|e| e.to_string())?;
        writer.flush().map_err(|e| e.to_string())?;

        let mut line = String::new();
        reader.read_line(&mut line).map_err(|e| e.to_string())?;
        let trimmed = line.trim();
        let value = if trimmed.is_empty() {
            def_str.to_string()
        } else {
            trimmed.to_string()
        };

        if let Some(validator) = &self.validator {
            validator(&value)?;
        }

        writeln!(
            writer,
            "{} \x1b[1m{}\x1b[0m {} \x1b[36m{}\x1b[0m",
            PromptTheme::success_prefix(),
            self.message,
            PromptTheme::delimiter(),
            value
        )
        .map_err(|e| e.to_string())?;
        writer.flush().map_err(|e| e.to_string())?;

        Ok(value)
    }

    fn run_interactive<R: Read, W: Write>(
        &self,
        reader: &mut R,
        writer: &mut W,
    ) -> Result<String, String> {
        let def_str = self.default.as_deref().unwrap_or("");
        let mut buffer = String::new();

        loop {
            // Render active line
            write!(
                writer,
                "\r\x1b[2K{} \x1b[1m{}\x1b[0m {} ",
                PromptTheme::question_prefix(),
                self.message,
                PromptTheme::delimiter()
            )
            .map_err(|e| e.to_string())?;

            if buffer.is_empty() {
                write!(writer, "\x1b[90m({})\x1b[0m ", def_str).map_err(|e| e.to_string())?;
            } else {
                write!(writer, "\x1b[36m{}\x1b[0m", buffer).map_err(|e| e.to_string())?;
            }
            writer.flush().map_err(|e| e.to_string())?;

            match read_key(reader) {
                Ok(Some(Key::Enter)) => {
                    let final_val = if buffer.trim().is_empty() {
                        def_str.to_string()
                    } else {
                        buffer.trim().to_string()
                    };

                    if let Some(validator) = &self.validator {
                        if let Err(err) = validator(&final_val) {
                            writeln!(writer, "\n\x1b[31m[!] {}\x1b[0m", err)
                                .map_err(|e| e.to_string())?;
                            buffer.clear();
                            continue;
                        }
                    }

                    // Dynamic in-place collapse
                    write!(
                        writer,
                        "\r\x1b[2K{} \x1b[1m{}\x1b[0m {} \x1b[36m{}\x1b[0m\n",
                        PromptTheme::success_prefix(),
                        self.message,
                        PromptTheme::delimiter(),
                        final_val
                    )
                    .map_err(|e| e.to_string())?;
                    writer.flush().map_err(|e| e.to_string())?;
                    return Ok(final_val);
                }
                Ok(Some(Key::Backspace)) => {
                    buffer.pop();
                }
                Ok(Some(Key::Char(c))) => {
                    if !c.is_control() {
                        buffer.push(c);
                    }
                }
                Ok(Some(Key::Esc)) | Ok(Some(Key::CtrlC)) | Ok(Some(Key::CtrlD)) => {
                    Terminal::restore_cursor();
                    return Err("Input cancelled by user".to_string());
                }
                Ok(None) => {
                    Terminal::restore_cursor();
                    return Err("Unexpected EOF while reading input".to_string());
                }
                _ => {}
            }
        }
    }
}

/// An individual selectable item within a `SelectPrompt`.
#[derive(Debug, Clone)]
pub struct SelectItem<T: Clone> {
    pub value: T,
    pub label: String,
    pub description: Option<String>,
}

impl<T: Clone> SelectItem<T> {
    /// Creates a new select item with a label.
    pub fn new(value: T, label: impl Into<String>) -> Self {
        Self {
            value,
            label: label.into(),
            description: None,
        }
    }

    /// Creates a new select item with a label and an explanatory description.
    pub fn with_description(
        value: T,
        label: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            value,
            label: label.into(),
            description: Some(description.into()),
        }
    }
}

/// An interactive selection modal/prompt component with arrow key navigation,
/// number jump shortcuts, and dynamic in-place question collapsing.
pub struct SelectPrompt<T: Clone> {
    pub message: String,
    pub items: Vec<SelectItem<T>>,
    pub default_index: usize,
}

impl<T: Clone> SelectPrompt<T> {
    /// Creates a new select prompt with a question message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            items: Vec::new(),
            default_index: 0,
        }
    }

    /// Appends an item to the prompt choice list.
    pub fn item(
        mut self,
        value: T,
        label: impl Into<String>,
        description: Option<impl Into<String>>,
    ) -> Self {
        self.items.push(SelectItem {
            value,
            label: label.into(),
            description: description.map(|d| d.into()),
        });
        self
    }

    /// Sets the list of items.
    pub fn items(mut self, items: Vec<SelectItem<T>>) -> Self {
        self.items = items;
        self
    }

    /// Sets the initial default item index.
    pub fn with_default_index(mut self, index: usize) -> Self {
        self.default_index = index;
        self
    }

    /// Executes the select prompt.
    pub fn run<R: BufRead, W: Write>(
        &self,
        reader: &mut R,
        writer: &mut W,
        is_interactive: bool,
    ) -> Result<T, String> {
        if self.items.is_empty() {
            return Err("SelectPrompt requires at least one item".to_string());
        }
        if is_interactive {
            self.run_interactive(reader, writer)
        } else {
            self.run_stream(reader, writer)
        }
    }

    fn run_stream<R: BufRead, W: Write>(
        &self,
        reader: &mut R,
        writer: &mut W,
    ) -> Result<T, String> {
        let max_label_len = self.items.iter().map(|it| it.label.len()).max().unwrap_or(0);

        writeln!(
            writer,
            "\n{} \x1b[1m{}\x1b[0m",
            PromptTheme::question_prefix(),
            self.message
        )
        .map_err(|e| e.to_string())?;

        for (idx, item) in self.items.iter().enumerate() {
            let num = idx + 1;
            let desc_str = match &item.description {
                Some(d) => format!(" \x1b[90m({})\x1b[0m", d),
                None => String::new(),
            };
            writeln!(
                writer,
                "  \x1b[36m{})\x1b[0m {:width$}{}",
                num,
                item.label,
                desc_str,
                width = max_label_len + 3
            )
            .map_err(|e| e.to_string())?;
        }

        write!(
            writer,
            "  \x1b[90m› Enter choice [1-{}] (default: {}):\x1b[0m ",
            self.items.len(),
            self.default_index + 1
        )
        .map_err(|e| e.to_string())?;
        writer.flush().map_err(|e| e.to_string())?;

        let mut line = String::new();
        reader.read_line(&mut line).map_err(|e| e.to_string())?;
        let trimmed = line.trim();

        let chosen_idx = if trimmed.is_empty() {
            self.default_index
        } else if let Ok(num) = trimmed.parse::<usize>() {
            if num >= 1 && num <= self.items.len() {
                num - 1
            } else {
                self.default_index
            }
        } else {
            // Try matching substring or lowercase name
            let lower = trimmed.to_lowercase();
            self.items
                .iter()
                .position(|it| it.label.to_lowercase().contains(&lower))
                .unwrap_or(self.default_index)
        };

        let selected = &self.items[chosen_idx];
        writeln!(
            writer,
            "{} \x1b[1m{}\x1b[0m {} \x1b[36m{}\x1b[0m",
            PromptTheme::success_prefix(),
            self.message,
            PromptTheme::delimiter(),
            selected.label
        )
        .map_err(|e| e.to_string())?;
        writer.flush().map_err(|e| e.to_string())?;

        Ok(selected.value.clone())
    }

    fn run_interactive<R: Read, W: Write>(
        &self,
        reader: &mut R,
        writer: &mut W,
    ) -> Result<T, String> {
        let n = self.items.len();
        let mut active = if self.default_index < n {
            self.default_index
        } else {
            0
        };
        let max_label_len = self.items.iter().map(|it| it.label.len()).max().unwrap_or(0);

        // Hide cursor during interactive option selection
        write!(writer, "\x1b[?25l").map_err(|e| e.to_string())?;

        // Print header question
        writeln!(
            writer,
            "\n{} \x1b[1m{}\x1b[0m",
            PromptTheme::question_prefix(),
            self.message
        )
        .map_err(|e| e.to_string())?;

        // Print initial choices
        for (i, item) in self.items.iter().enumerate() {
            let num = i + 1;
            let is_selected = i == active;
            let ptr = if is_selected {
                PromptTheme::pointer()
            } else {
                " "
            };
            let num_str = if is_selected {
                format!("\x1b[36m{})\x1b[0m", num)
            } else {
                format!("\x1b[90m{})\x1b[0m", num)
            };
            let label_str = if is_selected {
                format!(
                    "\x1b[1;36m{:width$}\x1b[0m",
                    item.label,
                    width = max_label_len + 3
                )
            } else {
                format!(
                    "\x1b[37m{:width$}\x1b[0m",
                    item.label,
                    width = max_label_len + 3
                )
            };
            let desc_str = match &item.description {
                Some(d) => format!(" \x1b[90m({})\x1b[0m", d),
                None => String::new(),
            };
            let nl = if i + 1 < n { "\r\n" } else { "" };
            write!(
                writer,
                "\r\x1b[2K  {} {} {}{}{}",
                ptr, num_str, label_str, desc_str, nl
            )
            .map_err(|e| e.to_string())?;
        }
        writer.flush().map_err(|e| e.to_string())?;

        loop {
            match read_key(reader) {
                Ok(Some(Key::Up)) | Ok(Some(Key::Char('k'))) => {
                    active = if active == 0 { n - 1 } else { active - 1 };
                }
                Ok(Some(Key::Down)) | Ok(Some(Key::Char('j'))) => {
                    active = (active + 1) % n;
                }
                Ok(Some(Key::Char(c @ '1'..='9'))) => {
                    let idx = (c as usize) - ('1' as usize);
                    if idx < n {
                        active = idx;
                    }
                }
                Ok(Some(Key::Enter)) => {
                    break;
                }
                Ok(Some(Key::Esc)) | Ok(Some(Key::CtrlC)) | Ok(Some(Key::CtrlD)) => {
                    Terminal::restore_cursor();
                    return Err("Selection cancelled by user".to_string());
                }
                Ok(None) => {
                    Terminal::restore_cursor();
                    return Err("Unexpected EOF while selecting item".to_string());
                }
                _ => continue,
            }

            // Move cursor up (n - 1) lines to start of line 1
            if n > 1 {
                write!(writer, "\x1b[{}A\r", n - 1).map_err(|e| e.to_string())?;
            } else {
                write!(writer, "\r").map_err(|e| e.to_string())?;
            }

            for (i, item) in self.items.iter().enumerate() {
                let num = i + 1;
                let is_selected = i == active;
                let ptr = if is_selected {
                    PromptTheme::pointer()
                } else {
                    " "
                };
                let num_str = if is_selected {
                    format!("\x1b[36m{})\x1b[0m", num)
                } else {
                    format!("\x1b[90m{})\x1b[0m", num)
                };
                let label_str = if is_selected {
                    format!(
                        "\x1b[1;36m{:width$}\x1b[0m",
                        item.label,
                        width = max_label_len + 3
                    )
                } else {
                    format!(
                        "\x1b[37m{:width$}\x1b[0m",
                        item.label,
                        width = max_label_len + 3
                    )
                };
                let desc_str = match &item.description {
                    Some(d) => format!(" \x1b[90m({})\x1b[0m", d),
                    None => String::new(),
                };
                let nl = if i + 1 < n { "\r\n" } else { "" };
                write!(
                    writer,
                    "\r\x1b[2K  {} {} {}{}{}",
                    ptr, num_str, label_str, desc_str, nl
                )
                .map_err(|e| e.to_string())?;
            }
            writer.flush().map_err(|e| e.to_string())?;
        }

        // On Enter: Erase all option lines + prompt header line in-place
        // Cursor is at line n. Moving up n lines reaches the question line.
        write!(writer, "\x1b[{}A\r\x1b[J", n).map_err(|e| e.to_string())?;

        // Print collapsed single line
        let selected = &self.items[active];
        writeln!(
            writer,
            "{} \x1b[1m{}\x1b[0m {} \x1b[36m{}\x1b[0m",
            PromptTheme::success_prefix(),
            self.message,
            PromptTheme::delimiter(),
            selected.label
        )
        .map_err(|e| e.to_string())?;

        // Restore cursor
        Terminal::restore_cursor();
        writer.flush().map_err(|e| e.to_string())?;

        Ok(selected.value.clone())
    }
}

/// An interactive yes/no confirmation prompt with in-place collapsing.
pub struct ConfirmPrompt {
    pub message: String,
    pub default: bool,
}

impl ConfirmPrompt {
    /// Creates a new confirmation prompt with a question message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            default: true,
        }
    }

    /// Sets the default boolean choice.
    pub fn with_default(mut self, default: bool) -> Self {
        self.default = default;
        self
    }

    /// Executes the confirmation prompt.
    pub fn run<R: BufRead, W: Write>(
        &self,
        reader: &mut R,
        writer: &mut W,
        is_interactive: bool,
    ) -> Result<bool, String> {
        if is_interactive {
            self.run_interactive(reader, writer)
        } else {
            self.run_stream(reader, writer)
        }
    }

    fn run_stream<R: BufRead, W: Write>(
        &self,
        reader: &mut R,
        writer: &mut W,
    ) -> Result<bool, String> {
        let hint = if self.default { "(Y/n)" } else { "(y/N)" };
        write!(
            writer,
            "\n{} \x1b[1m{}\x1b[0m \x1b[90m› {}\x1b[0m ",
            PromptTheme::question_prefix(),
            self.message,
            hint
        )
        .map_err(|e| e.to_string())?;
        writer.flush().map_err(|e| e.to_string())?;

        let mut line = String::new();
        reader.read_line(&mut line).map_err(|e| e.to_string())?;
        let trimmed = line.trim();

        let val = if trimmed.is_empty() {
            self.default
        } else { !(trimmed.starts_with('n') || trimmed.starts_with('N')) };

        let val_str = if val {
            "\x1b[32mYes\x1b[0m"
        } else {
            "\x1b[90mNo\x1b[0m"
        };
        writeln!(
            writer,
            "{} \x1b[1m{}\x1b[0m {} {}",
            PromptTheme::success_prefix(),
            self.message,
            PromptTheme::delimiter(),
            val_str
        )
        .map_err(|e| e.to_string())?;
        writer.flush().map_err(|e| e.to_string())?;

        Ok(val)
    }

    fn run_interactive<R: Read, W: Write>(
        &self,
        reader: &mut R,
        writer: &mut W,
    ) -> Result<bool, String> {
        let hint = if self.default { "(Y/n)" } else { "(y/N)" };
        write!(
            writer,
            "\n{} \x1b[1m{}\x1b[0m \x1b[90m› {}\x1b[0m ",
            PromptTheme::question_prefix(),
            self.message,
            hint
        )
        .map_err(|e| e.to_string())?;
        writer.flush().map_err(|e| e.to_string())?;

        let chosen = loop {
            match read_key(reader) {
                Ok(Some(Key::Enter)) => break self.default,
                Ok(Some(Key::Char('y' | 'Y'))) => break true,
                Ok(Some(Key::Char('n' | 'N'))) => break false,
                Ok(Some(Key::Esc)) | Ok(Some(Key::CtrlC)) | Ok(Some(Key::CtrlD)) => {
                    Terminal::restore_cursor();
                    return Err("Confirmation cancelled by user".to_string());
                }
                Ok(None) => {
                    Terminal::restore_cursor();
                    return Err("Unexpected EOF while reading confirmation".to_string());
                }
                _ => continue,
            }
        };

        let val_str = if chosen {
            "\x1b[32mYes\x1b[0m"
        } else {
            "\x1b[90mNo\x1b[0m"
        };
        // Collapse in-place
        write!(
            writer,
            "\r\x1b[2K{} \x1b[1m{}\x1b[0m {} {}\n",
            PromptTheme::success_prefix(),
            self.message,
            PromptTheme::delimiter(),
            val_str
        )
        .map_err(|e| e.to_string())?;
        writer.flush().map_err(|e| e.to_string())?;

        Ok(chosen)
    }
}

/// An individual selectable item within a `MultiSelectPrompt`.
#[derive(Debug, Clone)]
pub struct MultiSelectItem<T: Clone> {
    pub value: T,
    pub label: String,
    pub description: Option<String>,
    pub selected: bool,
}

impl<T: Clone> MultiSelectItem<T> {
    /// Creates a new multi-select item with initial selection state.
    pub fn new(value: T, label: impl Into<String>, selected: bool) -> Self {
        Self {
            value,
            label: label.into(),
            description: None,
            selected,
        }
    }

    /// Creates a new multi-select item with initial selection state and description.
    pub fn with_description(
        value: T,
        label: impl Into<String>,
        description: impl Into<String>,
        selected: bool,
    ) -> Self {
        Self {
            value,
            label: label.into(),
            description: Some(description.into()),
            selected,
        }
    }
}

/// An interactive multi-selection prompt modal with [x] / [ ] checkboxes,
/// spacebar toggling, arrow navigation, and dynamic in-place question collapsing.
pub struct MultiSelectPrompt<T: Clone> {
    pub message: String,
    pub items: Vec<MultiSelectItem<T>>,
    pub required: bool,
}

impl<T: Clone> MultiSelectPrompt<T> {
    /// Creates a new multi-select prompt with a question message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            items: Vec::new(),
            required: true,
        }
    }

    /// Appends an item to the multi-select choice list.
    pub fn item(
        mut self,
        value: T,
        label: impl Into<String>,
        description: Option<impl Into<String>>,
        selected: bool,
    ) -> Self {
        self.items.push(MultiSelectItem {
            value,
            label: label.into(),
            description: description.map(|d| d.into()),
            selected,
        });
        self
    }

    /// Sets the list of items.
    pub fn items(mut self, items: Vec<MultiSelectItem<T>>) -> Self {
        self.items = items;
        self
    }

    /// Sets whether at least one selection is required.
    pub fn required(mut self, required: bool) -> Self {
        self.required = required;
        self
    }

    /// Executes the multi-select prompt.
    pub fn run<R: BufRead, W: Write>(
        &self,
        reader: &mut R,
        writer: &mut W,
        is_interactive: bool,
    ) -> Result<Vec<T>, String> {
        if self.items.is_empty() {
            return Err("MultiSelectPrompt requires at least one item".to_string());
        }
        if is_interactive {
            self.run_interactive(reader, writer)
        } else {
            self.run_stream(reader, writer)
        }
    }

    fn sync_universal(items: &mut [MultiSelectItem<T>]) {
        if let Some(univ_idx) = items
            .iter()
            .position(|it| it.label.eq_ignore_ascii_case("universal") || it.label.eq_ignore_ascii_case("all"))
        {
            let non_univ: Vec<_> = items
                .iter()
                .enumerate()
                .filter(|(idx, _)| *idx != univ_idx)
                .collect();
            if !non_univ.is_empty() {
                let all_non_univ_selected = non_univ.iter().all(|(_, it)| it.selected);
                items[univ_idx].selected = all_non_univ_selected;
            }
        }
    }

    fn run_stream<R: BufRead, W: Write>(
        &self,
        reader: &mut R,
        writer: &mut W,
    ) -> Result<Vec<T>, String> {
        let max_label_len = self.items.iter().map(|it| it.label.len()).max().unwrap_or(0);

        writeln!(
            writer,
            "\n{} \x1b[1m{}\x1b[0m",
            PromptTheme::question_prefix(),
            self.message
        )
        .map_err(|e| e.to_string())?;

        for item in &self.items {
            let check = if item.selected {
                "\x1b[32m[x]\x1b[0m"
            } else {
                "\x1b[90m[ ]\x1b[0m"
            };
            let desc_str = match &item.description {
                Some(d) => format!(" \x1b[90m({})\x1b[0m", d),
                None => String::new(),
            };
            writeln!(
                writer,
                "  {} {:width$}{}",
                check,
                item.label,
                desc_str,
                width = max_label_len + 3
            )
            .map_err(|e| e.to_string())?;
        }

        let default_nums: Vec<String> = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, it)| it.selected)
            .map(|(i, _)| (i + 1).to_string())
            .collect();
        let default_hint = if default_nums.is_empty() {
            "1".to_string()
        } else {
            default_nums.join(",")
        };

        write!(
            writer,
            "  \x1b[90m› Enter choices (comma-separated, default: {}):\x1b[0m ",
            default_hint
        )
        .map_err(|e| e.to_string())?;
        writer.flush().map_err(|e| e.to_string())?;

        let mut line = String::new();
        reader.read_line(&mut line).map_err(|e| e.to_string())?;
        let trimmed = line.trim();

        let mut items = self.items.clone();

        if trimmed.is_empty() {
            if !items.iter().any(|it| it.selected) && !items.is_empty() {
                items[0].selected = true;
            }
        } else if trimmed.eq_ignore_ascii_case("all") || trimmed.eq_ignore_ascii_case("universal") {
            for it in &mut items {
                it.selected = true;
            }
        } else {
            for it in &mut items {
                it.selected = false;
            }
            let mut universal_triggered = false;
            for part in trimmed.split([',', ' ']) {
                let part = part.trim();
                if part.is_empty() {
                    continue;
                }
                if let Ok(num) = part.parse::<usize>() {
                    if num >= 1 && num <= items.len() {
                        let idx = num - 1;
                        items[idx].selected = true;
                        if items[idx].label.eq_ignore_ascii_case("universal")
                            || items[idx].label.eq_ignore_ascii_case("all")
                        {
                            universal_triggered = true;
                        }
                    }
                } else {
                    let lower = part.to_lowercase();
                    for it in &mut items {
                        if it.label.to_lowercase().contains(&lower) {
                            it.selected = true;
                            if it.label.eq_ignore_ascii_case("universal")
                                || it.label.eq_ignore_ascii_case("all")
                            {
                                universal_triggered = true;
                            }
                        }
                    }
                }
            }
            if universal_triggered {
                for it in &mut items {
                    it.selected = true;
                }
            } else {
                Self::sync_universal(&mut items);
            }
        }

        let selected_items: Vec<_> = items.iter().filter(|it| it.selected).collect();
        let has_universal = selected_items.iter().any(|it| it.label.eq_ignore_ascii_case("universal"));
        let non_universal_count = items.iter().filter(|it| !it.label.eq_ignore_ascii_case("universal")).count();
        let selected_non_universal = selected_items.iter().filter(|it| !it.label.eq_ignore_ascii_case("universal")).count();
        let is_all = has_universal || (non_universal_count > 0 && selected_non_universal == non_universal_count);

        let summary_str = if is_all {
            "Universal".to_string()
        } else if selected_items.is_empty() {
            "None".to_string()
        } else {
            selected_items
                .iter()
                .filter(|it| !it.label.eq_ignore_ascii_case("universal"))
                .map(|it| it.label.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        };

        writeln!(
            writer,
            "{} \x1b[1m{}\x1b[0m {} \x1b[36m{}\x1b[0m",
            PromptTheme::success_prefix(),
            self.message,
            PromptTheme::delimiter(),
            summary_str
        )
        .map_err(|e| e.to_string())?;
        writer.flush().map_err(|e| e.to_string())?;

        let returned_items: Vec<_> = if is_all {
            items
                .iter()
                .filter(|it| !it.label.eq_ignore_ascii_case("universal"))
                .map(|it| it.value.clone())
                .collect()
        } else {
            selected_items
                .into_iter()
                .filter(|it| !it.label.eq_ignore_ascii_case("universal"))
                .map(|it| it.value.clone())
                .collect()
        };

        Ok(returned_items)
    }

    fn run_interactive<R: Read, W: Write>(
        &self,
        reader: &mut R,
        writer: &mut W,
    ) -> Result<Vec<T>, String> {
        let n = self.items.len();
        let mut cursor = 0;
        let mut items = self.items.clone();
        let max_label_len = items.iter().map(|it| it.label.len()).max().unwrap_or(0);

        // Hide cursor during interactive option selection
        write!(writer, "\x1b[?25l").map_err(|e| e.to_string())?;

        // Print header question (clean and short so it never line-wraps on window resize)
        writeln!(
            writer,
            "\n{} \x1b[1m{}\x1b[0m",
            PromptTheme::question_prefix(),
            self.message
        )
        .map_err(|e| e.to_string())?;

        let render_row = |active: bool, item: &MultiSelectItem<T>| -> String {
            let ptr = if active {
                PromptTheme::pointer()
            } else {
                " "
            };
            let check_box = if item.selected {
                "\x1b[32m[x]\x1b[0m"
            } else {
                "\x1b[90m[ ]\x1b[0m"
            };
            let label_str = if item.selected {
                format!(
                    "\x1b[1;36m{:width$}\x1b[0m",
                    item.label,
                    width = max_label_len + 3
                )
            } else {
                format!(
                    "\x1b[37m{:width$}\x1b[0m",
                    item.label,
                    width = max_label_len + 3
                )
            };
            let desc_str = match &item.description {
                Some(d) => format!(" \x1b[90m({})\x1b[0m", d),
                None => String::new(),
            };
            format!("  {} {} {}{}", ptr, check_box, label_str, desc_str)
        };

        for (i, item) in items.iter().enumerate() {
            let row = render_row(i == cursor, item);
            let nl = if i + 1 < n { "\r\n" } else { "" };
            write!(writer, "\r\x1b[2K{}{}", row, nl).map_err(|e| e.to_string())?;
        }
        writer.flush().map_err(|e| e.to_string())?;

        loop {
            match read_key(reader) {
                Ok(Some(Key::Up)) | Ok(Some(Key::Char('k'))) => {
                    cursor = if cursor == 0 { n - 1 } else { cursor - 1 };
                }
                Ok(Some(Key::Down)) | Ok(Some(Key::Char('j'))) => {
                    cursor = (cursor + 1) % n;
                }
                Ok(Some(Key::Char(' '))) => {
                    items[cursor].selected = !items[cursor].selected;
                    if items[cursor].label.eq_ignore_ascii_case("universal")
                        || items[cursor].label.eq_ignore_ascii_case("all")
                    {
                        let state = items[cursor].selected;
                        for it in &mut items {
                            it.selected = state;
                        }
                    } else {
                        Self::sync_universal(&mut items);
                    }
                }
                Ok(Some(Key::Char(c @ '1'..='9'))) => {
                    let idx = (c as usize) - ('1' as usize);
                    if idx < n {
                        items[idx].selected = !items[idx].selected;
                        cursor = idx;
                        if items[idx].label.eq_ignore_ascii_case("universal")
                            || items[idx].label.eq_ignore_ascii_case("all")
                        {
                            let state = items[idx].selected;
                            for it in &mut items {
                                it.selected = state;
                            }
                        } else {
                            Self::sync_universal(&mut items);
                        }
                    }
                }
                Ok(Some(Key::Char('a' | 'A'))) => {
                    let all_selected = items.iter().all(|it| it.selected);
                    let new_state = !all_selected;
                    for it in &mut items {
                        it.selected = new_state;
                    }
                }
                Ok(Some(Key::Enter)) => {
                    let count_selected = items.iter().filter(|it| it.selected).count();
                    if count_selected == 0 && self.required {
                        items[cursor].selected = true;
                        Self::sync_universal(&mut items);
                    }
                    break;
                }
                Ok(Some(Key::Esc)) | Ok(Some(Key::CtrlC)) | Ok(Some(Key::CtrlD)) => {
                    Terminal::restore_cursor();
                    return Err("Multi-selection cancelled by user".to_string());
                }
                Ok(None) => {
                    Terminal::restore_cursor();
                    return Err("Unexpected EOF while selecting items".to_string());
                }
                _ => continue,
            }

            if n > 1 {
                write!(writer, "\x1b[{}A\r", n - 1).map_err(|e| e.to_string())?;
            } else {
                write!(writer, "\r").map_err(|e| e.to_string())?;
            }

            for (i, item) in items.iter().enumerate() {
                let row = render_row(i == cursor, item);
                let nl = if i + 1 < n { "\r\n" } else { "" };
                write!(writer, "\r\x1b[2K{}{}", row, nl).map_err(|e| e.to_string())?;
            }
            writer.flush().map_err(|e| e.to_string())?;
        }

        // On Enter: Erase all option lines + prompt header line in-place
        write!(writer, "\x1b[{}A\r\x1b[J", n).map_err(|e| e.to_string())?;

        let selected_items: Vec<_> = items.iter().filter(|it| it.selected).collect();
        let has_universal = selected_items.iter().any(|it| it.label.eq_ignore_ascii_case("universal"));
        let non_universal_count = items.iter().filter(|it| !it.label.eq_ignore_ascii_case("universal")).count();
        let selected_non_universal = selected_items.iter().filter(|it| !it.label.eq_ignore_ascii_case("universal")).count();
        let is_all = has_universal || (non_universal_count > 0 && selected_non_universal == non_universal_count);

        let summary_str = if is_all {
            "Universal".to_string()
        } else if selected_items.is_empty() {
            "None".to_string()
        } else {
            selected_items
                .iter()
                .filter(|it| !it.label.eq_ignore_ascii_case("universal"))
                .map(|it| it.label.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        };

        // Print collapsed single line
        writeln!(
            writer,
            "{} \x1b[1m{}\x1b[0m {} \x1b[36m{}\x1b[0m",
            PromptTheme::success_prefix(),
            self.message,
            PromptTheme::delimiter(),
            summary_str
        )
        .map_err(|e| e.to_string())?;

        // Restore cursor
        Terminal::restore_cursor();
        writer.flush().map_err(|e| e.to_string())?;

        let returned_items: Vec<_> = if is_all {
            items
                .iter()
                .filter(|it| !it.label.eq_ignore_ascii_case("universal"))
                .map(|it| it.value.clone())
                .collect()
        } else {
            selected_items
                .into_iter()
                .filter(|it| !it.label.eq_ignore_ascii_case("universal"))
                .map(|it| it.value.clone())
                .collect()
        };

        Ok(returned_items)
    }
}

/// Type alias for single-selection option modal.
pub type OptionModal<T> = SelectPrompt<T>;

/// Type alias for multi-selection option modal.
pub type MultiSelectModal<T> = MultiSelectPrompt<T>;

