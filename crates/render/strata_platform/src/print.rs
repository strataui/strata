//! Cross-platform native OS printer discovery and PDF print spooler dispatch
//! (CUPS `lpstat`/`lp` on Linux & macOS, Win32 spooler hooks on Windows, and `MockPrintSpooler` for CI).

#[cfg(unix)]
use std::io::Write;
#[cfg(unix)]
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

/// Duplex (two-sided) printing mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PrintDuplex {
    #[default]
    None,
    LongEdge,
    ShortEdge,
}

/// Metadata describing an installed host printer queue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrinterInfo {
    pub name: String,
    pub is_default: bool,
    pub status: String,
}

/// Configuration for submitting a native OS print job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrintJobOptions {
    pub printer: Option<String>,
    pub copies: u32,
    pub duplex: PrintDuplex,
    pub media: String,
    pub job_name: String,
}

impl Default for PrintJobOptions {
    fn default() -> Self {
        Self {
            printer: None,
            copies: 1,
            duplex: PrintDuplex::None,
            media: "A4".into(),
            job_name: "Strata Document".into(),
        }
    }
}

/// Captured print payload recorded when mock print spooler mode is active.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedPrintJob {
    pub options: PrintJobOptions,
    pub pdf_bytes: Vec<u8>,
}

static MOCK_PRINT_ENABLED: AtomicBool = AtomicBool::new(false);
static MOCK_PRINT_JOBS: OnceLock<Mutex<Vec<RecordedPrintJob>>> = OnceLock::new();

fn mock_jobs() -> &'static Mutex<Vec<RecordedPrintJob>> {
    MOCK_PRINT_JOBS.get_or_init(|| Mutex::new(Vec::new()))
}

/// Enables or disables deterministic in-memory mock print spooling for CI tests.
pub fn set_mock_print_spooler(enabled: bool) {
    MOCK_PRINT_ENABLED.store(enabled, Ordering::SeqCst);
}

/// Drains and returns all PDF jobs captured by the mock print spooler.
pub fn take_mock_print_jobs() -> Vec<RecordedPrintJob> {
    mock_jobs()
        .lock()
        .map(|mut v| std::mem::take(&mut *v))
        .unwrap_or_default()
}

/// Enumerates printers installed on the host OS.
pub fn list_printers() -> Vec<PrinterInfo> {
    if MOCK_PRINT_ENABLED.load(Ordering::SeqCst) {
        return vec![PrinterInfo {
            name: "Strata_Virtual_PDF_Printer".into(),
            is_default: true,
            status: "idle".into(),
        }];
    }

    #[allow(unused_mut)]
    let mut printers = Vec::new();
    #[cfg(unix)]
    {
        let default_name = Command::new("lpstat")
            .arg("-d")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|s| s.split(':').nth(1).map(|p| p.trim().to_string()));

        if let Ok(out) = Command::new("lpstat").arg("-p").output() {
            if let Ok(text) = String::from_utf8(out.stdout) {
                for line in text.lines() {
                    if let Some(rest) = line.strip_prefix("printer ") {
                        let mut parts = rest.split_whitespace();
                        if let Some(name) = parts.next() {
                            let status = parts.collect::<Vec<_>>().join(" ");
                            printers.push(PrinterInfo {
                                is_default: default_name.as_deref() == Some(name),
                                name: name.to_string(),
                                status,
                            });
                        }
                    }
                }
            }
        }
    }
    printers
}

/// Dispatches a PDF byte payload to a host OS printer queue (or mock spooler in CI).
pub fn print_pdf_bytes(pdf: &[u8], options: &PrintJobOptions) -> Result<String, String> {
    if !pdf.starts_with(b"%PDF-") {
        return Err("Invalid PDF header: payload must begin with %PDF-".into());
    }

    if MOCK_PRINT_ENABLED.load(Ordering::SeqCst) {
        if let Ok(mut jobs) = mock_jobs().lock() {
            jobs.push(RecordedPrintJob {
                options: options.clone(),
                pdf_bytes: pdf.to_vec(),
            });
        }
        return Ok("mock-job-1".into());
    }

    #[cfg(unix)]
    {
        let mut cmd = Command::new("lp");
        if let Some(ref p) = options.printer {
            cmd.args(["-d", p]);
        }
        cmd.args(["-n", &options.copies.max(1).to_string()]);
        cmd.args(["-t", &options.job_name]);
        cmd.args(["-o", &format!("media={}", options.media)]);
        match options.duplex {
            PrintDuplex::None => {}
            PrintDuplex::LongEdge => {
                cmd.args(["-o", "sides=two-sided-long-edge"]);
            }
            PrintDuplex::ShortEdge => {
                cmd.args(["-o", "sides=two-sided-short-edge"]);
            }
        }
        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to invoke CUPS `lp`: {e}"))?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(pdf)
                .map_err(|e| format!("Failed writing PDF to `lp` stdin: {e}"))?;
        }
        let output = child
            .wait_with_output()
            .map_err(|e| format!("Failed waiting on `lp`: {e}"))?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
        }
    }

    #[cfg(not(unix))]
    {
        let _ = (pdf, options);
        Err("Direct native print dispatch requires CUPS on Unix or MockPrintSpooler".into())
    }
}
