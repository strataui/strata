//! The Strata Unified Developer CLI and Packaging Toolchain.

use std::process;
use strata_cli::prelude::*;

fn main() {
    Terminal::restore_cursor();

    let command = match CliArgs::parse() {
        Ok(cmd) => cmd,
        Err(err) => {
            eprintln!("\x1b[1;31merror:\x1b[0m {}", err);
            process::exit(1);
        }
    };

    match command {
        CliCommand::New {
            name,
            template,
            target_dir,
            interactive,
        } => {
            let base_dir = target_dir.unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

            let (project_name, project_template) = if interactive || name.is_none() {
                match ProjectWizard::prompt_interactive(&base_dir) {
                    Ok(cfg) => (cfg.name, cfg.template),
                    Err(e) => {
                        Terminal::restore_cursor();
                        if e.contains("cancelled by user") || e.contains("Cancelled") {
                            println!("\n  \x1b[90mCancelled.\x1b[0m\n");
                            process::exit(130);
                        }
                        eprintln!("\x1b[1;31merror:\x1b[0m Project setup aborted: {}", e);
                        process::exit(1);
                    }
                }
            } else {
                let p_name = name.unwrap_or_default();
                let p_template = template.unwrap_or_default();
                CliArgs::print_banner();
                println!("  \x1b[36m›\x1b[0m Scaffolding project \x1b[1m{}\x1b[0m with template \x1b[37m{}\x1b[0m...", p_name, p_template.as_str());
                (p_name, p_template)
            };

            match ProjectScaffolder::scaffold(&project_name, project_template, &base_dir) {
                Ok(report) => {
                    println!("\n  \x1b[32m√\x1b[0m \x1b[1mSuccess!\x1b[0m Created \x1b[36m{}\x1b[0m at \x1b[90m{}\x1b[0m\n", project_name, report.root_path.display());
                    println!("  Inside that directory, you can run:\n");
                    println!("    \x1b[36mstrata dev\x1b[0m");
                    println!("      Starts the development server with live HMR.\n");
                    println!("    \x1b[36mstrata build\x1b[0m");
                    println!("      Compiles an optimized bundle for release.\n");
                    println!("    \x1b[36mstrata package\x1b[0m");
                    println!("      Generates native installers (.deb, .msi, .AppImage, .dmg).\n");
                    println!("  To get started:\n");
                    println!("    \x1b[36mcd\x1b[0m {}", project_name);
                    println!("    \x1b[36mstrata dev\x1b[0m\n");
                }
                Err(e) => {
                    eprintln!("\x1b[1;31merror:\x1b[0m Failed to scaffold project: {}", e);
                    process::exit(1);
                }
            }
        }
        CliCommand::Dev {
            target,
            port,
            watch,
        } => {
            let mut server = DevServer::new(DevServerConfig {
                target,
                port,
                watch,
                headless: false,
            });

            if let Err(e) = server.run() {
                eprintln!("\x1b[1;31merror:\x1b[0m Dev server encountered fatal error: {}", e);
                process::exit(1);
            }
        }
        CliCommand::Run {
            target,
            release,
            mcp,
            example,
            bin,
        } => {
            CliArgs::print_banner();
            println!(
                "  \x1b[36m›\x1b[0m Running Strata application [mode: \x1b[1m{}\x1b[0m, target: \x1b[1m{}\x1b[0m, mcp: \x1b[37m{}\x1b[0m]...",
                if release { "release" } else { "debug" },
                target.as_str(),
                if mcp { "enabled" } else { "disabled" }
            );

            let mut cmd = std::process::Command::new("cargo");
            cmd.arg("run");
            if release {
                cmd.arg("--release");
            }
            if let Some(ref ex) = example {
                cmd.arg("--example").arg(ex);
            } else if let Some(ref b) = bin {
                cmd.arg("--bin").arg(b);
            }

            if mcp {
                cmd.env("STRATA_MCP_ENABLED", "1");
                let sock_path = strata::mcp_bridge::default_dev_socket_path();
                cmd.env("STRATA_DEV_SOCKET", &sock_path);
                println!("  \x1b[90m• MCP Dev Bridge active at: {:?}\x1b[0m\n", sock_path);
            }

            let status = cmd.status();
            match status {
                Ok(s) if s.success() => {}
                Ok(s) => process::exit(s.code().unwrap_or(1)),
                Err(e) => {
                    eprintln!("\x1b[1;31merror:\x1b[0m Failed to execute application: {}", e);
                    process::exit(1);
                }
            }
        }
        CliCommand::Build {
            target,
            release,
            force,
            out_dir,
            env,
        } => {
            CliArgs::print_banner();

            let base_dir = out_dir.unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
            println!(
                "  \x1b[36m›\x1b[0m Compiling \x1b[1m{}\x1b[0m bundle for target: \x1b[1m{}\x1b[0m (env: \x1b[37m{}\x1b[0m)...",
                if release { "release" } else { "debug" },
                target.as_str(),
                env.as_deref().unwrap_or(if release { "production" } else { "development" })
            );
            match BuildRunner::build_with_options(target, release, force, &base_dir, env.as_deref()) {
                Ok(report) => {
                    println!("  \x1b[32m√\x1b[0m Build complete! Emitted {} artifacts.", report.artifacts.len());
                    for art in report.artifacts {
                        println!("    \x1b[90m•\x1b[0m {:?}", art);
                    }
                    println!();
                }
                Err(e) => {
                    eprintln!("\x1b[1;31merror:\x1b[0m Build failed: {}", e);
                    process::exit(1);
                }
            }
        }
        CliCommand::Package {
            target,
            format,
            out_dir,
            env,
        } => {
            CliArgs::print_banner();
            let base_dir = out_dir.unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
            println!(
                "  \x1b[36m›\x1b[0m Packaging production distribution for target: \x1b[1m{}\x1b[0m (env: \x1b[37m{}\x1b[0m)...",
                target.as_str(),
                env.as_deref().unwrap_or("production")
            );
            match Packager::package_with_env(target, format, &base_dir, env.as_deref()) {
                Ok(report) => {
                    println!(
                        "  \x1b[32m√\x1b[0m Created \x1b[36m{}\x1b[0m installer at \x1b[90m{:?}\x1b[0m",
                        report.format.extension(),
                        report.package_path
                    );
                    println!("    Emitted {} package artifacts and setup manifests.\n", report.generated_files.len());
                }
                Err(e) => {
                    eprintln!("\x1b[1;31merror:\x1b[0m Packaging failed: {}", e);
                    process::exit(1);
                }
            }
        }
        CliCommand::Test {
            parity,
            verbose,
            android,
            ios,
            mock_ios,
        } => {
            CliArgs::print_banner();
            if android || ios || mock_ios {
                println!("  \x1b[36m›\x1b[0m Running Strata Mobile Virtualization Test Suite...");
                let config = strata_mobile_test::StrataMobileTestConfig {
                    is_android: android || (!ios && !mock_ios),
                    is_ios: ios,
                    is_mock_ios: mock_ios,
                    device_serial: None,
                    enable_monkey_fuzz: true,
                    fuzz_event_count: 50,
                    test_foldable: true,
                };
                let mut runner = strata_mobile_test::StrataMobileTestRunner::new(config);
                let report = runner.execute();
                for detail in &report.details {
                    println!("    \x1b[90m•\x1b[0m {}", detail);
                }
                if report.passed {
                    println!(
                        "  \x1b[32m√\x1b[0m Mobile test suite passed: {}/{} tests successful (SSIM: {:.4})\n",
                        report.tests_run.saturating_sub(report.tests_failed),
                        report.tests_run,
                        report.ssim_score
                    );
                } else {
                    eprintln!("\x1b[1;31merror:\x1b[0m Mobile test suite failed: {}/{} failed", report.tests_failed, report.tests_run);
                    process::exit(1);
                }
            } else if parity {
                println!("  \x1b[36m›\x1b[0m Running Chromium-parity visual and layout regression test suite...");
                let report = ParityHarness::run_suite(verbose);
                report.print_scorecard();
            } else {
                println!("  \x1b[36m›\x1b[0m Running Strata tests...");
            }
        }
        CliCommand::Sim { device } => {
            CliArgs::print_banner();
            let preset_name = device.as_deref().unwrap_or("iphone-16-pro");
            let profile = match preset_name {
                "iphone-16-pro" | "iphone" => strata_sim::DeviceProfile::iphone_16_pro_max(),
                "s24-ultra" | "samsung" => strata_sim::DeviceProfile::galaxy_s24_ultra(),
                "pixel-9" | "pixel" => strata_sim::DeviceProfile::pixel_9_pro(),
                "z-fold-6" | "fold" => strata_sim::DeviceProfile::galaxy_z_fold_6(),
                "z-flip-6" | "flip" => strata_sim::DeviceProfile::galaxy_z_flip_6(),
                "duo-2" | "duo" => strata_sim::DeviceProfile::surface_duo_2(),
                _ => {
                    println!("  \x1b[90m• Unknown preset '{}', defaulting to iPhone 16 Pro Max\x1b[0m", preset_name);
                    strata_sim::DeviceProfile::iphone_16_pro_max()
                }
            };
            println!("  \x1b[36m›\x1b[0m Launching Strata Native Mobile Simulator [{}]...", profile.name);
            let workspace = strata_sim::MobileSimulatorWorkspace::new(profile);
            let view = workspace.render_view();
            println!("  \x1b[32m√\x1b[0m Running in pure Rust (Zero Chromium / Zero QEMU).\n");
            strata::launch(move || view);
        }
        CliCommand::Mcp { socket, action } => {
            let sock_path = socket.unwrap_or_else(strata::mcp_bridge::default_dev_socket_path);
            match action.as_deref() {
                Some("info") => {
                    let client = strata_mcp::StrataDevClient::new(&sock_path);
                    match client.session_info() {
                        Ok(info) => {
                            println!("{}", serde_json::to_string_pretty(&info).unwrap());
                        }
                        Err(e) => {
                            eprintln!("\x1b[1;31merror:\x1b[0m Could not connect to running Strata session: {}", e);
                            eprintln!("  Make sure a Strata application is running with 'strata run' or 'strata dev'.");
                            process::exit(1);
                        }
                    }
                }
                Some("screenshot") => {
                    let client = strata_mcp::StrataDevClient::new(&sock_path);
                    match client.take_screenshot() {
                        Ok(bytes) => {
                            let out_path = std::path::PathBuf::from("screenshot.png");
                            if let Err(e) = std::fs::write(&out_path, &bytes) {
                                eprintln!("\x1b[1;31merror:\x1b[0m Failed to save screenshot: {}", e);
                                process::exit(1);
                            }
                            println!("  \x1b[32m√\x1b[0m Screenshot saved to {:?}", out_path);
                        }
                        Err(e) => {
                            eprintln!("\x1b[1;31merror:\x1b[0m Failed to capture screenshot: {}", e);
                            process::exit(1);
                        }
                    }
                }
                _ => {
                    // Default action: Run stdio JSON-RPC MCP server
                    if let Err(e) = strata_mcp::run_stdio_server_with_socket(Some(sock_path)) {
                        eprintln!("\x1b[1;31merror:\x1b[0m MCP server error: {}", e);
                        process::exit(1);
                    }
                }
            }
        }
        CliCommand::Help(topic) => {
            if let Some(cmd) = topic {
                CliArgs::print_help_for(&cmd);
            } else {
                CliArgs::print_help();
            }
        }
        CliCommand::Version => {
            println!("strata 0.1.0 (pure rust, zero-chromium)");
        }
    }
}
