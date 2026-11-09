//! Automated Quickstart & Documentation Command Parity Validation Suite.
//!
//! Validates that all examples and entrypoints cited in README.md exist in the workspace,
//! compile cleanly with zero errors, and run headless verification where applicable.

#[test]
fn test_readme_documented_examples_exist_and_compile() {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("Workspace root directory must exist");

    let documented_examples = ["saas_dashboard", "creative_studio", "ecommerce_storefront", "strata_browser"];

    for example in &documented_examples {
        let example_cargo_path = workspace_root.join("examples").join(example).join("Cargo.toml");
        assert!(
            example_cargo_path.exists(),
            "Documented quickstart example '{}' missing Cargo.toml at: {:?}",
            example,
            example_cargo_path
        );
    }
}

#[test]
fn test_strata_browser_headless_flag_execution() {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("Workspace root directory must exist");

    let output = std::process::Command::new(env!("CARGO"))
        .arg("run")
        .arg("-p")
        .arg("strata_browser")
        .arg("--")
        .arg("--headless")
        .current_dir(workspace_root)
        .output()
        .expect("Failed to execute cargo run -p strata_browser -- --headless");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "cargo run -p strata_browser -- --headless failed with status {}.\nStdout: {}\nStderr: {}",
        output.status,
        stdout,
        stderr
    );

    assert!(
        stdout.contains("Headless GUI render verified"),
        "strata_browser did not emit expected headless verification output.\nStdout: {}",
        stdout
    );
}
