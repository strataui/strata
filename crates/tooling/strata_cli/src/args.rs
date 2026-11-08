//! Zero-dependency CLI argument parser, command models, and command-specific help printer.

use std::path::PathBuf;

/// Compilation and runtime platform target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BuildTarget {
    #[default]
    Desktop,
    Android,
    Ios,
    MacOs,
    Windows,
    Linux,
    WebWasm,
    WebJs,
}

impl BuildTarget {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s.to_ascii_lowercase().as_str() {
            "desktop" => Ok(Self::Desktop),
            "android" => Ok(Self::Android),
            "ios" => Ok(Self::Ios),
            "macos" => Ok(Self::MacOs),
            "windows" => Ok(Self::Windows),
            "linux" => Ok(Self::Linux),
            "web-wasm" | "wasm" => Ok(Self::WebWasm),
            "web-js" | "js" => Ok(Self::WebJs),
            unknown => Err(format!(
                "Invalid target '{}'. Valid options: desktop, android, ios, macos, windows, linux, web-wasm, web-js",
                unknown
            )),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Desktop => "desktop",
            Self::Android => "android",
            Self::Ios => "ios",
            Self::MacOs => "macos",
            Self::Windows => "windows",
            Self::Linux => "linux",
            Self::WebWasm => "web-wasm",
            Self::WebJs => "web-js",
        }
    }
}

/// Project template options for `strata new`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TemplateKind {
    #[default]
    Blank,
    Saas,
    Workbench,
    Studio,
    Ecommerce,
}

impl TemplateKind {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s.to_ascii_lowercase().as_str() {
            "blank" | "minimal" => Ok(Self::Blank),
            "saas" | "dashboard" => Ok(Self::Saas),
            "workbench" | "tool" | "editor" | "desktop" => Ok(Self::Workbench),
            "studio" | "creative" | "canvas" => Ok(Self::Studio),
            "ecommerce" | "commerce" => Ok(Self::Ecommerce),
            unknown => Err(format!(
                "Invalid template '{}'. Valid options: blank, saas, workbench, studio",
                unknown
            )),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Blank => "blank",
            Self::Saas => "saas",
            Self::Workbench => "workbench",
            Self::Studio => "studio",
            Self::Ecommerce => "ecommerce",
        }
    }
}

/// Native installer and store packaging format options across OSes and Linux distros.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageFormat {
    Apk,
    Aab,
    Ipa,
    Dmg,
    App,
    Pkg,
    Msi,
    Exe,
    AppImage,
    Deb,
    Rpm,
    PkgBuild,
    TarGz,
    Flatpak,
    Snap,
}

impl PackageFormat {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s.to_ascii_lowercase().as_str() {
            "apk" => Ok(Self::Apk),
            "aab" => Ok(Self::Aab),
            "ipa" => Ok(Self::Ipa),
            "dmg" => Ok(Self::Dmg),
            "app" => Ok(Self::App),
            "pkg" => Ok(Self::Pkg),
            "msi" => Ok(Self::Msi),
            "exe" => Ok(Self::Exe),
            "appimage" => Ok(Self::AppImage),
            "deb" => Ok(Self::Deb),
            "rpm" => Ok(Self::Rpm),
            "pkgbuild" | "arch" => Ok(Self::PkgBuild),
            "tar.gz" | "targz" | "tar" => Ok(Self::TarGz),
            "flatpak" => Ok(Self::Flatpak),
            "snap" => Ok(Self::Snap),
            unknown => Err(format!(
                "Invalid packaging format '{}'. Valid options: apk, aab, ipa, dmg, app, pkg, msi, exe, appimage, deb, rpm, pkgbuild, tar.gz, flatpak, snap",
                unknown
            )),
        }
    }

    pub fn default_for_target(target: BuildTarget) -> Self {
        match target {
            BuildTarget::Android => Self::Aab,
            BuildTarget::Ios => Self::Ipa,
            BuildTarget::MacOs => Self::Dmg,
            BuildTarget::Windows => Self::Msi,
            BuildTarget::Linux => Self::AppImage,
            BuildTarget::Desktop => {
                #[cfg(target_os = "macos")]
                return Self::Dmg;
                #[cfg(target_os = "windows")]
                return Self::Msi;
                #[cfg(not(any(target_os = "macos", target_os = "windows")))]
                return Self::AppImage;
            }
            BuildTarget::WebWasm | BuildTarget::WebJs => Self::TarGz,
        }
    }

    pub fn extension(&self) -> &'static str {
        match self {
            Self::Apk => "apk",
            Self::Aab => "aab",
            Self::Ipa => "ipa",
            Self::Dmg => "dmg",
            Self::App => "app",
            Self::Pkg => "pkg",
            Self::Msi => "msi",
            Self::Exe => "exe",
            Self::AppImage => "AppImage",
            Self::Deb => "deb",
            Self::Rpm => "rpm",
            Self::PkgBuild => "PKGBUILD",
            Self::TarGz => "tar.gz",
            Self::Flatpak => "flatpak",
            Self::Snap => "snap",
        }
    }
}

/// Parsed CLI command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliCommand {
    New {
        name: Option<String>,
        template: Option<TemplateKind>,
        target_dir: Option<PathBuf>,
        interactive: bool,
    },
    Dev {
        target: BuildTarget,
        port: u16,
        watch: bool,
    },
    Run {
        target: BuildTarget,
        release: bool,
        mcp: bool,
        example: Option<String>,
        bin: Option<String>,
    },
    Build {
        target: BuildTarget,
        release: bool,
        force: bool,
        out_dir: Option<PathBuf>,
        env: Option<String>,
    },
    Package {
        target: BuildTarget,
        format: Option<PackageFormat>,
        out_dir: Option<PathBuf>,
        env: Option<String>,
    },
    Test {
        parity: bool,
        verbose: bool,
        android: bool,
        ios: bool,
        mock_ios: bool,
    },
    Sim {
        device: Option<String>,
    },
    Mcp {
        socket: Option<PathBuf>,
        action: Option<String>,
    },
    Help(Option<String>),
    Version,
}

/// CLI Argument Parser.
pub struct CliArgs;

impl CliArgs {
    pub fn print_banner() {
        println!("\x1b[1;36m▲ Strata Universal Frontend Engine & Toolchain\x1b[0m \x1b[90mv0.1.0\x1b[0m");
        println!("\x1b[90m  Zero Chromium. Pure Rust. Pixel-Perfect CSS & Flexbox/Grid.\x1b[0m\n");
    }

    pub fn help_text(subcmd: Option<&str>) -> String {
        match subcmd {
            Some("new") => {
                "Usage: strata new [NAME] [OPTIONS]\n\n\
                 Scaffolds a new Strata application. If NAME is omitted, an interactive setup wizard will guide you through configuration.\n\n\
                 Arguments:\n\
                   [NAME]                          Project name (e.g. my-app)\n\n\
                 Options:\n\
                   -t, --template <TEMPLATE>       Project template: blank, saas, workbench, studio\n\
                   -d, --dir <PATH>                Target destination directory\n\
                   -h, --help                      Print this help message\n\n\
                 Examples:\n\
                   strata new                      Launch interactive project setup wizard\n\
                   strata new my-app               Create a new app with default template\n\
                   strata new shop --template saas Create a SaaS dashboard application\n".to_string()
            }
            Some("dev") => {
                "Usage: strata dev [OPTIONS]\n\n\
                 Starts the local development server with live signal graph HMR, file watching, and swapchain presentation.\n\n\
                 Options:\n\
                   -t, --target <TARGET>           Target platform: desktop, android, ios, macos, windows, linux, web-wasm, web-js (default: desktop)\n\
                   -p, --port <PORT>               Local HTTP/HMR port (default: 3000)\n\
                       --no-watch                  Disable background filesystem watcher\n\
                   -h, --help                      Print this help message\n\n\
                 Interactive Controls (while running):\n\
                   [r]                             Trigger manual HMR state reload\n\
                   [c]                             Clear terminal screen\n\
                   [h]                             Display keyboard shortcuts\n\
                   [q]                             Quit dev server\n".to_string()
            }
            Some("run") => {
                "Usage: strata run [OPTIONS]\n\n\
                 Runs the Strata application directly in debug or release mode.\n\n\
                 Options:\n\
                   -t, --target <TARGET>           Target platform: desktop (default), web-wasm, web-js\n\
                   -r, --release                   Run with release profile optimizations (default: debug)\n\
                       --debug                     Run in debug mode (default)\n\
                       --mcp                       Enable Model Context Protocol (MCP) bridge for AI agents (default: enabled)\n\
                       --no-mcp                    Disable MCP bridge\n\
                       --example <NAME>            Run a specific example target\n\
                       --bin <NAME>                Run a specific binary target\n\
                   -h, --help                      Print this help message\n\n\
                 Examples:\n\
                   strata run                      Run local application in debug mode\n\
                   strata run --release            Run with full release optimizations\n\
                   strata run --example eda_app    Run the EDA showcase example\n".to_string()
            }
            Some("build") => {
                "Usage: strata build [OPTIONS]\n\n\
                 Compiles the application for development, testing, or distribution.\n\n\
                 Options:\n\
                   -t, --target <TARGET>           Target platform: desktop, android, ios, macos, windows, linux, web-wasm, web-js (default: desktop)\n\
                   -r, --release                   Compile with release optimizations (default: debug profile)\n\
                       --debug                     Target debug profile\n\
                   -f, --force, --force-build      Force rebuild of all artifacts\n\
                   -e, --env <ENV>                 Environment profile (development, production, staging, custom)\n\
                   -o, --out-dir <DIR>             Output directory for artifacts\n\
                   -h, --help                      Print this help message\n".to_string()
            }
            Some("mcp") => {
                "Usage: strata mcp [OPTIONS] [COMMAND]\n\n\
                 Launches or interacts with the Model Context Protocol (MCP) server for external developer tools and language models.\n\n\
                 Commands:\n\
                   stdio                           Run JSON-RPC 2.0 MCP server over stdio (default)\n\
                   info                            Query active Strata development session info\n\
                   screenshot [OUT_PATH]           Take PNG screenshot of running session\n\n\
                 Options:\n\
                   -s, --socket <PATH>             Custom Unix domain socket path (default: /tmp/strata-dev.sock)\n\
                   -h, --help                      Print this help message\n".to_string()
            }
            Some("package") => {
                "Usage: strata package [OPTIONS]\n\n\
                 Packages application into native OS installers, packages, or standalone portable bundles.\n\n\
                 Options:\n\
                   -t, --target <TARGET>           Target platform: desktop, android, ios, macos, windows, linux, web-wasm, web-js (default: desktop)\n\
                   -f, --format <FORMAT>           Package format:\n\
                                                     Windows: msi, exe\n\
                                                     Linux:   deb, rpm, appimage, pkgbuild, tar.gz\n\
                                                     macOS:   dmg, pkg, app\n\
                                                     Mobile:  apk, aab, ipa\n\
                   -e, --env <ENV>                 Environment profile (development, production, staging, custom)\n\
                   -o, --out-dir <DIR>             Output directory for generated packages\n\
                   -h, --help                      Print this help message\n".to_string()
            }
            Some("test") => {
                "Usage: strata test [OPTIONS]\n\n\
                 Runs automated regression, layout parity, and mobile emulator testing.\n\n\
                 Options:\n\
                       --parity [ENGINE]           Run W3C WPT layout parity tests against Chromium (default: chromium)\n\
                   -v, --verbose                   Print detailed per-fixture scorecards\n\
                       --android                   Run automated headless Android emulator test suite\n\
                       --ios                       Run automated iOS simulator test suite\n\
                       --mock-ios                  Run Strata high-fidelity iOS mock runner\n\
                   -h, --help                      Print this help message\n".to_string()
            }
            Some("sim") | Some("simulator") => {
                "Usage: strata sim [OPTIONS]\n\n\
                 Launches the dogfooded Strata Native Mobile Device Simulator and virtual phone OS.\n\n\
                 Options:\n\
                   -d, --device <PRESET>           Device profile preset: iphone-16-pro, s24-ultra, pixel-9, z-fold-6, z-flip-6, duo-2\n\
                   -h, --help                      Print this help message\n".to_string()
            }
            _ => {
                "▲ Strata Universal Frontend Engine & Toolchain v0.1.0\n\
                 Zero Chromium. Pure Rust. Pixel-Perfect CSS & Flexbox/Grid.\n\n\
                 Usage: strata <COMMAND> [OPTIONS]\n\n\
                 Commands:\n\
                   new [name]                      Scaffold a new project (interactive wizard if name omitted)\n\
                   dev [OPTIONS]                   Start local dev server with state-preserving HMR\n\
                   run [OPTIONS]                   Run application in debug or release mode\n\
                   build [OPTIONS]                 Compile optimized binary for target platform\n\
                   package [OPTIONS]               Package application into native OS installer or bundle\n\
                   test [OPTIONS]                  Run visual parity and mobile integration tests\n\
                   sim [OPTIONS]                   Launch native dogfooded mobile device simulator\n\
                   mcp [OPTIONS]                   Connect AI assistants to live running Strata dev session\n\
                   help [command]                  Display general help or help for a specific command\n\
                   version                         Display version information\n\n\
                 Run 'strata <COMMAND> --help' for detailed usage of any command.\n\n\
                 Supported Targets:\n\
                   desktop (default), android, ios, macos, windows, linux, web-wasm, web-js\n".to_string()
            }
        }
    }

    pub fn print_help() {
        print!("{}", Self::help_text(None));
    }

    pub fn print_help_for(subcmd: &str) {
        print!("{}", Self::help_text(Some(subcmd)));
    }

    pub fn parse() -> Result<CliCommand, String> {
        Self::parse_from(std::env::args())
    }

    pub fn parse_from<I, T>(iter: I) -> Result<CliCommand, String>
    where
        I: IntoIterator<Item = T>,
        T: AsRef<str>,
    {
        let raw_args: Vec<String> = iter.into_iter().map(|s| s.as_ref().to_string()).collect();
        if raw_args.len() < 2 {
            return Ok(CliCommand::Help(None));
        }

        let subcommand = &raw_args[1];
        let sub_args = &raw_args[2..];

        match subcommand.as_str() {
            "new" => Self::parse_new(sub_args),
            "dev" => Self::parse_dev(sub_args),
            "run" => Self::parse_run(sub_args),
            "build" => Self::parse_build(sub_args),
            "package" => Self::parse_package(sub_args),
            "test" => Self::parse_test(sub_args),
            "sim" | "simulator" => Self::parse_sim(sub_args),
            "mcp" => Self::parse_mcp(sub_args),
            "help" => {
                let topic = sub_args.first().cloned();
                Ok(CliCommand::Help(topic))
            }
            "--help" | "-h" => Ok(CliCommand::Help(None)),
            "version" | "--version" | "-v" => Ok(CliCommand::Version),
            unknown => Err(format!(
                "Unknown command: '{}'. Run 'strata --help' for available commands.",
                unknown
            )),
        }
    }

    fn parse_new(args: &[String]) -> Result<CliCommand, String> {
        let mut name = None;
        let mut template = None;
        let mut target_dir = None;

        let mut i = 0;
        while i < args.len() {
            let arg = &args[i];
            if arg == "--help" || arg == "-h" {
                return Ok(CliCommand::Help(Some("new".to_string())));
            } else if arg == "--template" || arg == "-t" {
                i += 1;
                if i >= args.len() {
                    return Err("Expected template name after '--template'. Run 'strata new --help' for usage.".to_string());
                }
                template = Some(TemplateKind::parse(&args[i])?);
            } else if arg == "--dir" || arg == "-d" {
                i += 1;
                if i >= args.len() {
                    return Err("Expected path after '--dir'. Run 'strata new --help' for usage.".to_string());
                }
                target_dir = Some(PathBuf::from(&args[i]));
            } else if arg.starts_with('-') {
                return Err(format!(
                    "Unrecognized option '{}' for command 'new'. Run 'strata new --help' for usage.",
                    arg
                ));
            } else if name.is_none() {
                name = Some(arg.clone());
            } else {
                return Err(format!(
                    "Unexpected argument '{}'. Run 'strata new --help' for usage.",
                    arg
                ));
            }
            i += 1;
        }

        let interactive = name.is_none();

        Ok(CliCommand::New {
            name,
            template,
            target_dir,
            interactive,
        })
    }

    fn parse_dev(args: &[String]) -> Result<CliCommand, String> {
        let mut target = BuildTarget::Desktop;
        let mut port = 3000;
        let mut watch = true;

        let mut i = 0;
        while i < args.len() {
            let arg = &args[i];
            if arg == "--help" || arg == "-h" {
                return Ok(CliCommand::Help(Some("dev".to_string())));
            } else if arg == "--target" || arg == "-t" {
                i += 1;
                if i >= args.len() {
                    return Err("Expected target after '--target'. Run 'strata dev --help' for usage.".to_string());
                }
                target = BuildTarget::parse(&args[i])?;
            } else if arg == "--port" || arg == "-p" {
                i += 1;
                if i >= args.len() {
                    return Err("Expected port after '--port'. Run 'strata dev --help' for usage.".to_string());
                }
                port = args[i].parse::<u16>().map_err(|_| "Invalid port number. Must be between 1 and 65535.")?;
            } else if arg == "--no-watch" {
                watch = false;
            } else if arg.starts_with('-') {
                return Err(format!(
                    "Unrecognized option '{}' for command 'dev'. Run 'strata dev --help' for usage.",
                    arg
                ));
            } else {
                return Err(format!(
                    "Unexpected argument '{}'. Run 'strata dev --help' for usage.",
                    arg
                ));
            }
            i += 1;
        }

        Ok(CliCommand::Dev {
            target,
            port,
            watch,
        })
    }

    fn parse_run(args: &[String]) -> Result<CliCommand, String> {
        let mut target = BuildTarget::Desktop;
        let mut release = false;
        let mut mcp = true;
        let mut example = None;
        let mut bin = None;

        let mut i = 0;
        while i < args.len() {
            let arg = &args[i];
            if arg == "--help" || arg == "-h" {
                return Ok(CliCommand::Help(Some("run".to_string())));
            } else if arg == "--target" || arg == "-t" {
                i += 1;
                if i >= args.len() {
                    return Err("Expected target after '--target'. Run 'strata run --help' for usage.".to_string());
                }
                target = BuildTarget::parse(&args[i])?;
            } else if arg == "--release" || arg == "-r" {
                release = true;
            } else if arg == "--debug" {
                release = false;
            } else if arg == "--mcp" {
                mcp = true;
            } else if arg == "--no-mcp" {
                mcp = false;
            } else if arg == "--example" {
                i += 1;
                if i >= args.len() {
                    return Err("Expected example name after '--example'.".to_string());
                }
                example = Some(args[i].clone());
            } else if arg == "--bin" {
                i += 1;
                if i >= args.len() {
                    return Err("Expected binary name after '--bin'.".to_string());
                }
                bin = Some(args[i].clone());
            } else if arg.starts_with('-') {
                return Err(format!(
                    "Unrecognized option '{}' for command 'run'. Run 'strata run --help' for usage.",
                    arg
                ));
            } else if example.is_none() && bin.is_none() {
                bin = Some(arg.clone());
            } else {
                return Err(format!(
                    "Unexpected argument '{}'. Run 'strata run --help' for usage.",
                    arg
                ));
            }
            i += 1;
        }

        Ok(CliCommand::Run {
            target,
            release,
            mcp,
            example,
            bin,
        })
    }

    fn parse_build(args: &[String]) -> Result<CliCommand, String> {
        let mut target = BuildTarget::Desktop;
        let mut release = false;
        let mut force = false;
        let mut out_dir = None;
        let mut env = None;

        let mut i = 0;
        while i < args.len() {
            let arg = &args[i];
            if arg == "--help" || arg == "-h" {
                return Ok(CliCommand::Help(Some("build".to_string())));
            } else if arg == "--target" || arg == "-t" {
                i += 1;
                if i >= args.len() {
                    return Err("Expected target after '--target'. Run 'strata build --help' for usage.".to_string());
                }
                target = BuildTarget::parse(&args[i])?;
            } else if arg == "--release" || arg == "-r" {
                release = true;
            } else if arg == "--debug" {
                release = false;
            } else if arg == "--force" || arg == "-f" || arg == "--force-build" {
                force = true;
            } else if arg == "--env" || arg == "-e" {
                i += 1;
                if i >= args.len() {
                    return Err("Expected environment after '--env'. Run 'strata build --help' for usage.".to_string());
                }
                env = Some(args[i].clone());
            } else if arg == "--out-dir" || arg == "--out" || arg == "-o" {
                i += 1;
                if i >= args.len() {
                    return Err("Expected directory after '--out-dir'. Run 'strata build --help' for usage.".to_string());
                }
                out_dir = Some(PathBuf::from(&args[i]));
            } else if arg.starts_with('-') {
                return Err(format!(
                    "Unrecognized option '{}' for command 'build'. Run 'strata build --help' for usage.",
                    arg
                ));
            } else {
                return Err(format!(
                    "Unexpected argument '{}'. Run 'strata build --help' for usage.",
                    arg
                ));
            }
            i += 1;
        }

        Ok(CliCommand::Build {
            target,
            release,
            force,
            out_dir,
            env,
        })
    }

    fn parse_mcp(args: &[String]) -> Result<CliCommand, String> {
        let mut socket = None;
        let mut action = None;

        let mut i = 0;
        while i < args.len() {
            let arg = &args[i];
            if arg == "--help" || arg == "-h" {
                return Ok(CliCommand::Help(Some("mcp".to_string())));
            } else if arg == "--socket" || arg == "-s" {
                i += 1;
                if i >= args.len() {
                    return Err("Expected path after '--socket'.".to_string());
                }
                socket = Some(PathBuf::from(&args[i]));
            } else if !arg.starts_with('-') && action.is_none() {
                action = Some(arg.clone());
            } else if arg.starts_with('-') {
                return Err(format!(
                    "Unrecognized option '{}' for command 'mcp'. Run 'strata mcp --help' for usage.",
                    arg
                ));
            }
            i += 1;
        }

        Ok(CliCommand::Mcp { socket, action })
    }

    fn parse_package(args: &[String]) -> Result<CliCommand, String> {
        let mut target = BuildTarget::Desktop;
        let mut format = None;
        let mut out_dir = None;
        let mut env = None;

        let mut i = 0;
        while i < args.len() {
            let arg = &args[i];
            if arg == "--help" || arg == "-h" {
                return Ok(CliCommand::Help(Some("package".to_string())));
            } else if arg == "--target" || arg == "-t" {
                i += 1;
                if i >= args.len() {
                    return Err("Expected target after '--target'. Run 'strata package --help' for usage.".to_string());
                }
                target = BuildTarget::parse(&args[i])?;
            } else if arg == "--format" || arg == "-f" {
                i += 1;
                if i >= args.len() {
                    return Err("Expected format after '--format'. Run 'strata package --help' for usage.".to_string());
                }
                format = Some(PackageFormat::parse(&args[i])?);
            } else if arg == "--env" || arg == "-e" {
                i += 1;
                if i >= args.len() {
                    return Err("Expected environment after '--env'. Run 'strata package --help' for usage.".to_string());
                }
                env = Some(args[i].clone());
            } else if arg == "--out-dir" || arg == "--out" || arg == "-o" {
                i += 1;
                if i >= args.len() {
                    return Err("Expected directory after '--out-dir'. Run 'strata package --help' for usage.".to_string());
                }
                out_dir = Some(PathBuf::from(&args[i]));
            } else if arg.starts_with('-') {
                return Err(format!(
                    "Unrecognized option '{}' for command 'package'. Run 'strata package --help' for usage.",
                    arg
                ));
            } else {
                return Err(format!(
                    "Unexpected argument '{}'. Run 'strata package --help' for usage.",
                    arg
                ));
            }
            i += 1;
        }

        Ok(CliCommand::Package {
            target,
            format,
            out_dir,
            env,
        })
    }

    fn parse_test(args: &[String]) -> Result<CliCommand, String> {
        let mut parity = false;
        let mut verbose = false;
        let mut android = false;
        let mut ios = false;
        let mut mock_ios = false;

        let mut i = 0;
        while i < args.len() {
            let arg = &args[i];
            if arg == "--help" || arg == "-h" {
                return Ok(CliCommand::Help(Some("test".to_string())));
            } else if arg == "--parity" {
                parity = true;
                if i + 1 < args.len() && !args[i + 1].starts_with('-') {
                    i += 1; // optional value e.g. "chromium"
                }
            } else if arg == "--verbose" || arg == "-v" {
                verbose = true;
            } else if arg == "--android" {
                android = true;
            } else if arg == "--ios" {
                ios = true;
            } else if arg == "--mock-ios" {
                mock_ios = true;
            } else if arg.starts_with('-') {
                return Err(format!(
                    "Unrecognized option '{}' for command 'test'. Run 'strata test --help' for usage.",
                    arg
                ));
            } else {
                return Err(format!(
                    "Unexpected argument '{}'. Run 'strata test --help' for usage.",
                    arg
                ));
            }
            i += 1;
        }

        Ok(CliCommand::Test {
            parity,
            verbose,
            android,
            ios,
            mock_ios,
        })
    }

    fn parse_sim(args: &[String]) -> Result<CliCommand, String> {
        let mut device = None;
        let mut i = 0;
        while i < args.len() {
            let arg = &args[i];
            if arg == "--help" || arg == "-h" {
                return Ok(CliCommand::Help(Some("sim".to_string())));
            } else if arg == "--device" || arg == "-d" {
                i += 1;
                if i >= args.len() {
                    return Err("Expected device preset name after '--device'. Run 'strata sim --help' for usage.".to_string());
                }
                device = Some(args[i].clone());
            } else if arg.starts_with('-') {
                return Err(format!(
                    "Unrecognized option '{}' for command 'sim'. Run 'strata sim --help' for usage.",
                    arg
                ));
            } else {
                return Err(format!(
                    "Unexpected argument '{}'. Run 'strata sim --help' for usage.",
                    arg
                ));
            }
            i += 1;
        }

        Ok(CliCommand::Sim { device })
    }
}
