//! Interactive project creation wizard for `strata new`.

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use crate::args::{BuildTarget, TemplateKind};

/// Configuration options selected via the interactive setup wizard.
#[derive(Debug, Clone)]
pub struct WizardConfig {
    pub name: String,
    pub template: TemplateKind,
    pub target: BuildTarget,
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
        let stdin = std::io::stdin();
        let mut reader = stdin.lock();
        let stdout = std::io::stdout();
        let mut writer = stdout.lock();
        Self::prompt_with_io(&mut reader, &mut writer, base_dir)
    }

    /// Parameterized interactive prompt taking generic reader/writer for determinism in unit tests.
    pub fn prompt_with_io<R: BufRead, W: Write>(
        reader: &mut R,
        writer: &mut W,
        base_dir: &Path,
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
        write!(
            writer,
            "\x1b[1;36m?\x1b[0m \x1b[1mWhat is your project named?\x1b[0m \x1b[90m›\x1b[0m \x1b[90m(strata-app)\x1b[0m "
        )
        .map_err(|e| e.to_string())?;
        writer.flush().map_err(|e| e.to_string())?;
        let mut name_input = String::new();
        reader.read_line(&mut name_input).map_err(|e| e.to_string())?;
        let mut name = name_input.trim().to_string();
        if name.is_empty() {
            name = "strata-app".to_string();
        }

        // Validate project name syntax (only alphanumeric, hyphens, underscores)
        if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            return Err(format!(
                "Invalid project name '{}'. Use only letters, numbers, hyphens, and underscores.",
                name
            ));
        }

        // 2. Select Template
        writeln!(
            writer,
            "\n\x1b[1;36m?\x1b[0m \x1b[1mWhich template would you like to start with?\x1b[0m"
        )
        .map_err(|e| e.to_string())?;
        writeln!(
            writer,
            "  \x1b[36m1)\x1b[0m Blank Canvas       \x1b[90m(Minimal zero-dependency reactive canvas)\x1b[0m"
        )
        .map_err(|e| e.to_string())?;
        writeln!(
            writer,
            "  \x1b[36m2)\x1b[0m SaaS Dashboard     \x1b[90m(AI Cloud & telemetry infrastructure portal)\x1b[0m"
        )
        .map_err(|e| e.to_string())?;
        writeln!(
            writer,
            "  \x1b[36m3)\x1b[0m E-Commerce         \x1b[90m(Storefront catalog, cart drawer & checkout)\x1b[0m"
        )
        .map_err(|e| e.to_string())?;
        writeln!(
            writer,
            "  \x1b[36m4)\x1b[0m Creative Studio    \x1b[90m(Vector canvas & node properties editor)\x1b[0m"
        )
        .map_err(|e| e.to_string())?;
        write!(writer, "  \x1b[90m› Enter choice [1-4] (default: 1):\x1b[0m ")
            .map_err(|e| e.to_string())?;
        writer.flush().map_err(|e| e.to_string())?;

        let mut tpl_input = String::new();
        reader.read_line(&mut tpl_input).map_err(|e| e.to_string())?;
        let template = match tpl_input.trim() {
            "2" | "saas" => TemplateKind::Saas,
            "3" | "ecommerce" | "commerce" => TemplateKind::Ecommerce,
            "4" | "studio" | "creative" => TemplateKind::Studio,
            _ => TemplateKind::Blank,
        };

        // 3. Select Target Platform
        writeln!(
            writer,
            "\n\x1b[1;36m?\x1b[0m \x1b[1mWhich target platform do you want to target?\x1b[0m"
        )
        .map_err(|e| e.to_string())?;
        writeln!(
            writer,
            "  \x1b[36m1)\x1b[0m Desktop            \x1b[90m(Native Vulkan / Metal / DirectX hardware swapchain)\x1b[0m"
        )
        .map_err(|e| e.to_string())?;
        writeln!(
            writer,
            "  \x1b[36m2)\x1b[0m Web                \x1b[90m(Dual WebAssembly canvas + pure JS ESM runtime)\x1b[0m"
        )
        .map_err(|e| e.to_string())?;
        writeln!(
            writer,
            "  \x1b[36m3)\x1b[0m Mobile             \x1b[90m(Android Vulkan + iOS Metal NDK runtime)\x1b[0m"
        )
        .map_err(|e| e.to_string())?;
        writeln!(
            writer,
            "  \x1b[36m4)\x1b[0m Universal          \x1b[90m(All platform targets)\x1b[0m"
        )
        .map_err(|e| e.to_string())?;
        write!(writer, "  \x1b[90m› Enter choice [1-4] (default: 1):\x1b[0m ")
            .map_err(|e| e.to_string())?;
        writer.flush().map_err(|e| e.to_string())?;

        let mut target_input = String::new();
        reader.read_line(&mut target_input).map_err(|e| e.to_string())?;
        let target = match target_input.trim() {
            "2" | "web" => BuildTarget::WebWasm,
            "3" | "mobile" => BuildTarget::Android,
            "4" | "universal" => BuildTarget::Desktop,
            _ => BuildTarget::Desktop,
        };

        // 4. Optional Add-ons
        write!(
            writer,
            "\n\x1b[1;36m?\x1b[0m \x1b[1mWould you like to use Tailwind CSS?\x1b[0m \x1b[90m› (Y/n)\x1b[0m "
        )
        .map_err(|e| e.to_string())?;
        writer.flush().map_err(|e| e.to_string())?;
        let mut tw_input = String::new();
        reader.read_line(&mut tw_input).map_err(|e| e.to_string())?;
        let enable_tailwind = !tw_input.trim().eq_ignore_ascii_case("n");

        write!(
            writer,
            "\n\x1b[1;36m?\x1b[0m \x1b[1mInclude 3D Viewport & PBR shaders?\x1b[0m \x1b[90m› (Y/n)\x1b[0m "
        )
        .map_err(|e| e.to_string())?;
        writer.flush().map_err(|e| e.to_string())?;
        let mut c3d_input = String::new();
        reader.read_line(&mut c3d_input).map_err(|e| e.to_string())?;
        let enable_canvas_3d = !c3d_input.trim().eq_ignore_ascii_case("n");

        write!(
            writer,
            "\n\x1b[1;36m?\x1b[0m \x1b[1mInclude persistent Bitcask storage?\x1b[0m \x1b[90m› (Y/n)\x1b[0m "
        )
        .map_err(|e| e.to_string())?;
        writer.flush().map_err(|e| e.to_string())?;
        let mut store_input = String::new();
        reader.read_line(&mut store_input).map_err(|e| e.to_string())?;
        let enable_bitcask_storage = !store_input.trim().eq_ignore_ascii_case("n");

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
        writeln!(
            writer,
            "  \x1b[90m•\x1b[0m Target:      \x1b[37m{}\x1b[0m",
            target.as_str()
        )
        .map_err(|e| e.to_string())?;
        writeln!(
            writer,
            "  \x1b[90m•\x1b[0m Tailwind:    {}",
            if enable_tailwind {
                "\x1b[32mYes\x1b[0m"
            } else {
                "\x1b[90mNo\x1b[0m"
            }
        )
        .map_err(|e| e.to_string())?;
        writeln!(
            writer,
            "  \x1b[90m•\x1b[0m 3D Engine:   {}",
            if enable_canvas_3d {
                "\x1b[32mYes\x1b[0m"
            } else {
                "\x1b[90mNo\x1b[0m"
            }
        )
        .map_err(|e| e.to_string())?;
        writeln!(
            writer,
            "  \x1b[90m•\x1b[0m Bitcask:     {}",
            if enable_bitcask_storage {
                "\x1b[32mYes\x1b[0m"
            } else {
                "\x1b[90mNo\x1b[0m"
            }
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
            target,
            enable_tailwind,
            enable_canvas_3d,
            enable_bitcask_storage,
            target_dir,
        })
    }
}
