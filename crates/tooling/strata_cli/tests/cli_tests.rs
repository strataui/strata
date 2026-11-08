use std::fs;
use strata_cli::prelude::*;

#[test]
fn test_cli_args_parsing_commands() {
    // 1. New command
    let cmd = CliArgs::parse_from(["strata", "new", "my-app", "--template", "saas"]).unwrap();
    assert_eq!(
        cmd,
        CliCommand::New {
            name: Some("my-app".to_string()),
            template: Some(TemplateKind::Saas),
            interactive: false,
            target_dir: None,
        }
    );

    // 2. Dev command
    let cmd = CliArgs::parse_from(["strata", "dev", "--target", "web-wasm", "--port", "8080"]).unwrap();
    assert_eq!(
        cmd,
        CliCommand::Dev {
            target: BuildTarget::WebWasm,
            port: 8080,
            watch: true,
        }
    );

    // 3. Build command
    let cmd = CliArgs::parse_from(["strata", "build", "--target", "web-js", "--release"]).unwrap();
    assert_eq!(
        cmd,
        CliCommand::Build {
            target: BuildTarget::WebJs,
            release: true,
            force: false,
            out_dir: None,
            env: None,
        }
    );

    let cmd = CliArgs::parse_from(["strata", "build", "--target", "desktop", "-e", "staging"]).unwrap();
    assert_eq!(
        cmd,
        CliCommand::Build {
            target: BuildTarget::Desktop,
            release: false,
            force: false,
            out_dir: None,
            env: Some("staging".to_string()),
        }
    );

    let cmd = CliArgs::parse_from(["strata", "build", "--target", "desktop", "--force"]).unwrap();
    assert_eq!(
        cmd,
        CliCommand::Build {
            target: BuildTarget::Desktop,
            release: false,
            force: true,
            out_dir: None,
            env: None,
        }
    );

    // 4. Run command
    let cmd = CliArgs::parse_from(["strata", "run", "--release", "--example", "saas_dashboard"]).unwrap();
    assert_eq!(
        cmd,
        CliCommand::Run {
            target: BuildTarget::Desktop,
            release: true,
            mcp: true,
            example: Some("saas_dashboard".to_string()),
            bin: None,
        }
    );

    // 5. MCP command
    let cmd = CliArgs::parse_from(["strata", "mcp", "--socket", "/tmp/test.sock", "info"]).unwrap();
    assert_eq!(
        cmd,
        CliCommand::Mcp {
            socket: Some(std::path::PathBuf::from("/tmp/test.sock")),
            action: Some("info".to_string()),
        }
    );

    // 4. Package command
    let cmd = CliArgs::parse_from(["strata", "package", "--target", "android", "--format", "apk"]).unwrap();
    assert_eq!(
        cmd,
        CliCommand::Package {
            target: BuildTarget::Android,
            format: Some(PackageFormat::Apk),
            out_dir: None,
            env: None,
        }
    );

    // 5. Test command
    let cmd = CliArgs::parse_from(["strata", "test", "--parity"]).unwrap();
    assert_eq!(
        cmd,
        CliCommand::Test {
            parity: true,
            verbose: false,
            android: false,
            ios: false,
            mock_ios: false,
        }
    );

    let cmd_android = CliArgs::parse_from(["strata", "test", "--android"]).unwrap();
    assert_eq!(
        cmd_android,
        CliCommand::Test {
            parity: false,
            verbose: false,
            android: true,
            ios: false,
            mock_ios: false,
        }
    );

    // 6. Sim command
    let cmd_sim = CliArgs::parse_from(["strata", "sim", "--device", "z-fold-6"]).unwrap();
    assert_eq!(
        cmd_sim,
        CliCommand::Sim {
            device: Some("z-fold-6".to_string()),
        }
    );

    // 7. Help & Version
    assert_eq!(CliArgs::parse_from(["strata", "help"]).unwrap(), CliCommand::Help(None));
    assert_eq!(CliArgs::parse_from(["strata", "--version"]).unwrap(), CliCommand::Version);
}

#[test]
fn test_project_scaffolding_templates() {
    let temp_dir = std::env::temp_dir().join(format!("strata_test_scaffold_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let report = ProjectScaffolder::scaffold("test_saas_app", TemplateKind::Saas, &temp_dir).unwrap();
    assert_eq!(report.name, "test_saas_app");
    assert_eq!(report.template, TemplateKind::Saas);
    assert!(report.root_path.exists());
    assert!(report.root_path.join("Cargo.toml").exists());
    assert!(report.root_path.join("strata.toml").exists());
    assert!(report.root_path.join("src/main.rs").exists());
    assert!(report.root_path.join("README.md").exists());
    assert!(report.root_path.join(".gitignore").exists());

    let cargo_toml = fs::read_to_string(report.root_path.join("Cargo.toml")).unwrap();
    assert!(cargo_toml.contains("name = \"test_saas_app\""));

    let main_rs = fs::read_to_string(report.root_path.join("src/main.rs")).unwrap();
    assert!(main_rs.contains("MetricCard"));

    let report_wb = ProjectScaffolder::scaffold("test_workbench_app", TemplateKind::Workbench, &temp_dir).unwrap();
    assert_eq!(report_wb.name, "test_workbench_app");
    assert_eq!(report_wb.template, TemplateKind::Workbench);
    let wb_main_rs = fs::read_to_string(report_wb.root_path.join("src/main.rs")).unwrap();
    assert!(wb_main_rs.contains("Workbench"));
    assert!(wb_main_rs.contains("Ctrl+K"));

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_hmr_session_state_preservation() {
    let mut session = HmrSession::new();
    assert_eq!(session.reload_count, 0);
    assert!(session.restore_state().is_none());

    let state_payload = vec![42u8, 100, 200, 255];
    session.snapshot_state(state_payload.clone());
    assert_eq!(session.restore_state(), Some(state_payload.as_slice()));

    let count = session.trigger_reload();
    assert_eq!(count, 1);
    assert_eq!(session.reload_count, 1);
    // State is preserved across reloads
    assert_eq!(session.restore_state(), Some(state_payload.as_slice()));
}

#[test]
fn test_builder_web_js_and_web_wasm_emission() {
    let temp_dir = std::env::temp_dir().join(format!("strata_test_build_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    // 1. Web-JS build
    let js_report = BuildRunner::build(BuildTarget::WebJs, true, &temp_dir).unwrap();
    assert_eq!(js_report.target, BuildTarget::WebJs);
    assert!(temp_dir.join("dist/web-js/index.html").exists());
    assert!(temp_dir.join("dist/web-js/app.js").exists());
    assert!(temp_dir.join("dist/web-js/runtime.js").exists());

    // 2. Web-WASM build
    let wasm_report = BuildRunner::build(BuildTarget::WebWasm, true, &temp_dir).unwrap();
    assert_eq!(wasm_report.target, BuildTarget::WebWasm);
    assert!(temp_dir.join("dist/web-wasm/index.html").exists());
    assert!(temp_dir.join("dist/web-wasm/loader.js").exists());

    // 3. Desktop Native build (Release)
    let desktop_report = BuildRunner::build(BuildTarget::Desktop, true, &temp_dir).unwrap();
    assert_eq!(desktop_report.target, BuildTarget::Desktop);
    assert!(desktop_report.release);
    assert!(temp_dir.join("dist/desktop/strata_build.json").exists());
    let release_manifest = fs::read_to_string(temp_dir.join("dist/desktop/strata_build.json")).unwrap();
    assert!(release_manifest.contains(r#""release": true"#));

    // 4. Desktop Native build (Debug without --force)
    let debug_report = BuildRunner::build(BuildTarget::Desktop, false, &temp_dir).unwrap();
    assert_eq!(debug_report.target, BuildTarget::Desktop);
    assert!(!debug_report.release);
    assert!(temp_dir.join("dist/desktop/strata_build.json").exists());
    let debug_manifest = fs::read_to_string(temp_dir.join("dist/desktop/strata_build.json")).unwrap();
    assert!(debug_manifest.contains(r#""release": false"#));

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_packager_formats_and_validation() {
    let temp_dir = std::env::temp_dir().join(format!("strata_test_package_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    // Valid packaging
    let report = Packager::package(BuildTarget::Android, Some(PackageFormat::Apk), &temp_dir).unwrap();
    assert_eq!(report.format, PackageFormat::Apk);
    assert!(report.package_path.exists());

    let report = Packager::package(BuildTarget::MacOs, None, &temp_dir).unwrap();
    assert_eq!(report.format, PackageFormat::Dmg);
    assert!(report.package_path.exists());

    // Incompatible packaging format
    let invalid = Packager::package(BuildTarget::Android, Some(PackageFormat::Dmg), &temp_dir);
    assert!(invalid.is_err());

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_chromium_parity_harness_execution() {
    let report = ParityHarness::run_suite(false);
    assert!(report.total_cases >= 5);
    assert_eq!(report.passed_cases, report.total_cases);
    assert_eq!(report.average_pixel_delta, 0.0);
    assert!(report.is_full_parity);
}

#[test]
fn test_scaffold_rejects_existing_non_empty_directory() {
    let temp_dir = std::env::temp_dir().join(format!("strata_non_empty_test_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    // Create a dummy file inside
    let dummy_file = temp_dir.join("existing_file.txt");
    fs::write(&dummy_file, b"content").unwrap();

    let result = ProjectScaffolder::scaffold(
        temp_dir.file_name().unwrap().to_str().unwrap(),
        TemplateKind::Saas,
        temp_dir.parent().unwrap(),
    );

    assert!(result.is_err());
    let err_msg = result.unwrap_err();
    assert!(
        err_msg.contains("already exists and is not empty"),
        "Unexpected error message: {}",
        err_msg
    );

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_scaffold_allows_existing_empty_directory() {
    let temp_dir = std::env::temp_dir().join(format!("strata_empty_test_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let result = ProjectScaffolder::scaffold(
        temp_dir.file_name().unwrap().to_str().unwrap(),
        TemplateKind::Blank,
        temp_dir.parent().unwrap(),
    );

    assert!(result.is_ok());
    let _ = fs::remove_dir_all(&temp_dir);
}
