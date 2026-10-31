//! Interactive project creation wizard for `strata new`.

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use crate::args::{BuildTarget, TemplateKind};
use crate::prompt::{MultiSelectPrompt, SelectPrompt, Terminal, TextPrompt};

/// Configuration options selected via the interactive setup wizard.
#[derive(Debug, Clone)]
pub struct WizardConfig {
    pub name: String,
    pub template: TemplateKind,
    pub target: BuildTarget,
    pub targets: Vec<BuildTarget>,
    pub enable_tailwind: bool,
    pub enable_canvas_3d: bool,
    pub enable_bitcask_storage: bool,
    pub target_dir: PathBuf,
}

/// Interactive terminal wizard for `strata new`.
pub struct ProjectWizard;

impl ProjectWizard {
    /// Runs the interactive project setup wizard using standard input/output.
    pub fn prompt_interactive(base_dir: &Path) -> Result<WizardConfig, String> {
        let is_interactive = Terminal::is_interactive();
        let _raw_guard = if is_interactive {
            Terminal::enable_raw_mode()
        } else {
            None
        };
        let stdin = std::io::stdin();
        let mut reader = stdin.lock();
        let stdout = std::io::stdout();
        let mut writer = stdout.lock();
        Self::prompt_impl(&mut reader, &mut writer, base_dir, is_interactive)
    }

    /// Parameterized interactive prompt taking generic reader/writer for determinism in unit tests.
    pub fn prompt_with_io<R: BufRead, W: Write>(
        reader: &mut R,
        writer: &mut W,
        base_dir: &Path,
    ) -> Result<WizardConfig, String> {
        Self::prompt_impl(reader, writer, base_dir, false)
    }

    /// Internal parameterized prompt implementation supporting both interactive TTY and stream execution.
    pub fn prompt_impl<R: BufRead, W: Write>(
        reader: &mut R,
        writer: &mut W,
        base_dir: &Path,
        is_interactive: bool,
    ) -> Result<WizardConfig, String> {
        writeln!(
            writer,
            "\x1b[1;36m▲ Strata\x1b[0m \x1b[90mv0.1.0 (Zero Chromium Pure Rust Engine)\x1b[0m"
        )
        .map_err(|e| e.to_string())?;
        writeln!(
            writer,
            "\x1b[90m─────────────────────────────────────────────────────────────\x1b[0m\n"
        )
        .map_err(|e| e.to_string())?;

        // 1. Project Name
        let name = TextPrompt::new("What is your project named?")
            .with_default("strata-app")
            .with_validator(|n| {
                if !n.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
                    Err(format!(
                        "Invalid project name '{}'. Use only letters, numbers, hyphens, and underscores.",
                        n
                    ))
                } else {
                    Ok(())
                }
            })
            .run(reader, writer, is_interactive)?;

        // 2. Select Template
        let template = SelectPrompt::new("Which template would you like to start with?")
            .item(
                TemplateKind::Blank,
                "Blank Canvas",
                Some("Minimal zero-dependency reactive canvas"),
            )
            .item(
                TemplateKind::Saas,
                "SaaS Dashboard",
                Some("AI Cloud & telemetry infrastructure portal"),
            )
            .item(
                TemplateKind::Workbench,
                "Developer Workbench",
                Some("Multi-pane desktop workspace with command palette & tabs"),
            )
            .item(
                TemplateKind::Studio,
                "Creative Studio",
                Some("Vector canvas & node properties editor with 3D PBR"),
            )
            .with_default_index(0)
            .run(reader, writer, is_interactive)?;

        // 3. Select Target Platform(s)
        let targets = MultiSelectPrompt::new("Which target platform do you want to target?")
            .item(
                BuildTarget::Desktop,
                "Desktop",
                Some("Native Vulkan / Metal / DirectX hardware swapchain"),
                true,
            )
            .item(
                BuildTarget::WebWasm,
                "Web",
                Some("Dual WebAssembly canvas + pure JS ESM runtime"),
                false,
            )
            .item(
                BuildTarget::Android,
                "Mobile",
                Some("Android Vulkan + iOS Metal NDK runtime"),
                false,
            )
            .item(
                BuildTarget::Desktop,
                "Universal",
                Some("All platform targets"),
                false,
            )
            .run(reader, writer, is_interactive)?;

        let primary_target = targets.first().copied().unwrap_or(BuildTarget::Desktop);

        // Standard built-in engine capabilities (enabled by default)
        let enable_tailwind = true;
        let enable_canvas_3d = true;
        let enable_bitcask_storage = true;

        writeln!(
            writer,
            "\n\x1b[90m─────────────────────────────────────────────────────────────\x1b[0m"
        )
        .map_err(|e| e.to_string())?;
        writeln!(
            writer,
            "\x1b[32m√\x1b[0m \x1b[1mConfiguration confirmed:\x1b[0m"
        )
        .map_err(|e| e.to_string())?;
        writeln!(writer, "  \x1b[90m•\x1b[0m Project:     \x1b[36m{}\x1b[0m", name)
            .map_err(|e| e.to_string())?;
        writeln!(
            writer,
            "  \x1b[90m•\x1b[0m Template:    \x1b[37m{}\x1b[0m",
            template.as_str()
        )
        .map_err(|e| e.to_string())?;

        let targets_display = if targets.len() >= 3 {
            "Universal".to_string()
        } else if targets.is_empty() {
            "Desktop".to_string()
        } else {
            let names: Vec<&str> = targets
                .iter()
                .map(|t| match t {
                    BuildTarget::Desktop => "Desktop",
                    BuildTarget::WebWasm => "Web",
                    BuildTarget::Android => "Mobile",
                    _ => t.as_str(),
                })
                .collect();
            names.join(", ")
        };
        writeln!(
            writer,
            "  \x1b[90m•\x1b[0m Target:      \x1b[37m{}\x1b[0m",
            targets_display
        )
        .map_err(|e| e.to_string())?;
        writeln!(
            writer,
            "\x1b[90m─────────────────────────────────────────────────────────────\x1b[0m\n"
        )
        .map_err(|e| e.to_string())?;

        let target_dir = base_dir.join(&name);

        Ok(WizardConfig {
            name,
            template,
            target: primary_target,
            targets,
            enable_tailwind,
            enable_canvas_3d,
            enable_bitcask_storage,
            target_dir,
        })
    }
}
