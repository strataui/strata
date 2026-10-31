use std::io::Cursor;
use std::path::Path;
use strata_cli::prelude::*;

#[test]
fn test_wizard_default_answers() {
    // All empty inputs simulate user pressing Enter on all 3 prompts
    let input_bytes = b"\n\n\n";
    let mut reader = Cursor::new(&input_bytes[..]);
    let mut writer = Vec::new();
    let base_dir = Path::new("/tmp/strata_test_workspace");

    let config = ProjectWizard::prompt_with_io(&mut reader, &mut writer, base_dir).unwrap();
    assert_eq!(config.name, "strata-app");
    assert_eq!(config.template, TemplateKind::Blank);
    assert_eq!(config.target, BuildTarget::Desktop);
    assert_eq!(config.targets, vec![BuildTarget::Desktop]);
    assert!(config.enable_tailwind);
    assert!(config.enable_canvas_3d);
    assert!(config.enable_bitcask_storage);
    assert_eq!(config.target_dir, base_dir.join("strata-app"));

    let output_str = String::from_utf8(writer).unwrap();
    assert!(output_str.contains("Strata"));
    assert!(output_str.contains("What is your project named?"));
    assert!(output_str.contains("Configuration confirmed"));
    // Ensure no emojis in wizard output
    assert!(!output_str.contains('\u{26A1}'));
    assert!(!output_str.contains('\u{1F680}'));
    assert!(!output_str.contains('\u{2714}'));
}

#[test]
fn test_wizard_custom_answers() {
    // Custom name: enterprise-hub
    // Template: 2 (SaaS Dashboard)
    // Target: 2 (Web)
    let input_bytes = b"enterprise-hub\n2\n2\n";
    let mut reader = Cursor::new(&input_bytes[..]);
    let mut writer = Vec::new();
    let base_dir = Path::new("/tmp/strata_test_workspace");

    let config = ProjectWizard::prompt_with_io(&mut reader, &mut writer, base_dir).unwrap();
    assert_eq!(config.name, "enterprise-hub");
    assert_eq!(config.template, TemplateKind::Saas);
    assert_eq!(config.target, BuildTarget::WebWasm);
    assert!(config.enable_tailwind);
    assert!(config.enable_canvas_3d);
    assert!(config.enable_bitcask_storage);
    assert_eq!(config.target_dir, base_dir.join("enterprise-hub"));
}

#[test]
fn test_wizard_workbench_and_multi_target() {
    // Custom name: developer_desk
    // Template: 3 (Developer Workbench)
    // Target: 1,2 (Desktop + Web)
    let input_bytes = b"developer_desk\n3\n1,2\n";
    let mut reader = Cursor::new(&input_bytes[..]);
    let mut writer = Vec::new();
    let base_dir = Path::new("/tmp/strata_test_workspace");

    let config = ProjectWizard::prompt_with_io(&mut reader, &mut writer, base_dir).unwrap();
    assert_eq!(config.name, "developer_desk");
    assert_eq!(config.template, TemplateKind::Workbench);
    assert_eq!(config.targets, vec![BuildTarget::Desktop, BuildTarget::WebWasm]);
    assert_eq!(config.target, BuildTarget::Desktop);
}

#[test]
fn test_wizard_studio_and_mobile_answers() {
    // Custom name: creative_canvas
    // Template: 4 (Studio)
    // Target: 3 (Mobile)
    let input_bytes = b"creative_canvas\n4\n3\n";
    let mut reader = Cursor::new(&input_bytes[..]);
    let mut writer = Vec::new();
    let base_dir = Path::new("/tmp/strata_test_workspace");

    let config = ProjectWizard::prompt_with_io(&mut reader, &mut writer, base_dir).unwrap();
    assert_eq!(config.name, "creative_canvas");
    assert_eq!(config.template, TemplateKind::Studio);
    assert_eq!(config.target, BuildTarget::Android);
    assert!(config.enable_tailwind);
    assert!(config.enable_canvas_3d);
    assert!(config.enable_bitcask_storage);
}

#[test]
fn test_wizard_invalid_name_rejection() {
    // Spaces in name
    let input_bytes = b"my project with spaces\n";
    let mut reader = Cursor::new(&input_bytes[..]);
    let mut writer = Vec::new();
    let base_dir = Path::new("/tmp/strata_test_workspace");

    let res = ProjectWizard::prompt_with_io(&mut reader, &mut writer, base_dir);
    assert!(res.is_err());
    let err_msg = res.unwrap_err();
    assert!(err_msg.contains("Invalid project name"));

    // Special characters in name
    let input_bytes = b"app#name@bad!\n";
    let mut reader = Cursor::new(&input_bytes[..]);
    let mut writer = Vec::new();

    let res = ProjectWizard::prompt_with_io(&mut reader, &mut writer, base_dir);
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("Invalid project name"));
}

#[test]
fn test_wizard_rejects_existing_non_empty_dir() {
    let temp_dir = std::env::temp_dir().join(format!("strata_wiz_non_empty_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let existing_project = temp_dir.join("already_exists");
    std::fs::create_dir_all(&existing_project).unwrap();
    std::fs::write(existing_project.join("data.txt"), b"some data").unwrap();

    let input_bytes = b"already_exists\n";
    let mut reader = Cursor::new(&input_bytes[..]);
    let mut writer = Vec::new();

    let res = ProjectWizard::prompt_with_io(&mut reader, &mut writer, &temp_dir);
    assert!(res.is_err());
    let err_msg = res.unwrap_err();
    assert!(
        err_msg.contains("already exists and is not empty"),
        "Unexpected error: {}",
        err_msg
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}
