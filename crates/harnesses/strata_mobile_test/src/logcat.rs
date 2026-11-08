//! Logcat Stream Parser, Thread-Time Demultiplexer, and Rust Panic Trapper.

/// Severity level of a log entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Verbose,
    Debug,
    Info,
    Warn,
    Error,
    Fatal,
}

impl LogLevel {
    pub fn from_char(c: char) -> Self {
        match c {
            'V' => Self::Verbose,
            'D' => Self::Debug,
            'I' => Self::Info,
            'W' => Self::Warn,
            'E' => Self::Error,
            'F' => Self::Fatal,
            _ => Self::Info,
        }
    }
}

/// A parsed entry from the Android logcat threadtime stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogcatEntry {
    pub date: String,
    pub time: String,
    pub pid: u32,
    pub tid: u32,
    pub level: LogLevel,
    pub tag: String,
    pub message: String,
}

/// Parser for standard threadtime logcat lines without regex overhead.
pub struct LogcatParser;

impl LogcatParser {
    /// Parses a single line of `logcat -v threadtime`.
    /// Example: `10-31 14:22:01.123  1420  1420 I StrataNative: Frame rendered in 4.2ms`
    pub fn parse_line(line: &str) -> Option<LogcatEntry> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return None;
        }

        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() < 6 {
            return None;
        }

        let date = parts[0].to_string();
        let time = parts[1].to_string();
        let pid: u32 = parts[2].parse().ok()?;
        let tid: u32 = parts[3].parse().ok()?;
        let level = LogLevel::from_char(parts[4].chars().next().unwrap_or('I'));

        // Tag may end with ':' or space
        let raw_tag = parts[5].trim_end_matches(':');
        let tag = raw_tag.to_string();

        // Message is everything after the tag
        let tag_idx = line.find(parts[5])?;
        let msg_start = tag_idx + parts[5].len();
        let message = line[msg_start..].trim_start_matches(':').trim().to_string();

        Some(LogcatEntry {
            date,
            time,
            pid,
            tid,
            level,
            tag,
            message,
        })
    }
}

/// Information extracted from an intercepted crash or unhandled panic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanicReport {
    pub is_panic: bool,
    pub is_segfault: bool,
    pub message: String,
    pub location: Option<String>,
}

/// Real-time panic and native crash watchdog.
#[derive(Debug, Default)]
pub struct PanicTrap {
    captured_panics: Vec<PanicReport>,
}

impl PanicTrap {
    pub fn new() -> Self {
        Self::default()
    }

    /// Inspects an incoming logcat entry and traps Rust panics or segmentation faults.
    pub fn inspect_entry(&mut self, entry: &LogcatEntry) -> Option<&PanicReport> {
        let msg = &entry.message;

        if msg.contains("panicked at") || msg.contains("fatal runtime error") {
            let location = msg.find("at ").map(|idx| msg[idx + 3..].trim().to_string());

            let report = PanicReport {
                is_panic: true,
                is_segfault: false,
                message: msg.clone(),
                location,
            };
            self.captured_panics.push(report);
            return self.captured_panics.last();
        }

        if msg.contains("signal 11 (SIGSEGV)") || msg.contains("SEGV_MAPERR") {
            let report = PanicReport {
                is_panic: false,
                is_segfault: true,
                message: msg.clone(),
                location: None,
            };
            self.captured_panics.push(report);
            return self.captured_panics.last();
        }

        None
    }

    pub fn has_crashed(&self) -> bool {
        !self.captured_panics.is_empty()
    }

    pub fn captured_panics(&self) -> &[PanicReport] {
        &self.captured_panics
    }
}
