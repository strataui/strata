//! Tests for PluginEngine and dynamic build/package lifecycle mutations.

use std::fs;
use strata_cli::prelude::*;

#[test]
fn test_plugin_dynamic_app_name_mutation() {
    let temp_dir = std::env::temp_dir().join(format!("strata_plugin_test_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let plugins_dir = temp_dir.join("plugins");
    fs::create_dir_all(&plugins_dir).unwrap();

    // 1. Create a Rust plugin file that mutates config.app.name
    let plugin_file = plugins_dir.join("dynamic_namer.rs");
    let plugin_src = r#"
// Strata plugin script
config.app.name = "quantum-strata-app";
config.app.version = "1.5.0";
"#;
    fs::write(&plugin_file, plugin_src).unwrap();

    // 2. Create strata.yaml pointing to the plugin
    let yaml_file = temp_dir.join("strata.yaml");
    let yaml_content = r#"
app:
  name: "original-app"
  version: "0.1.0"

plugins:
  - name: "dynamic-namer"
    path: "plugins/dynamic_namer.rs"
"#;
    fs::write(&yaml_file, yaml_content).unwrap();

    // 3. Run Packager: it should execute the plugin and use "quantum-strata-app"
    let report = Packager::package(BuildTarget::Linux, Some(PackageFormat::Deb), &temp_dir).unwrap();
    assert_eq!(report.format, PackageFormat::Deb);

    let pkg_name = report.package_path.file_name().unwrap().to_str().unwrap();
    assert!(
        pkg_name.contains("quantum-strata-app"),
        "Package file name was '{}', expected to contain 'quantum-strata-app'",
        pkg_name
    );

    // Verify control file content
    let deb_control = temp_dir.join("dist/packages/quantum-strata-app-deb-pkg/DEBIAN/control");
    assert!(deb_control.exists());
    let control_text = fs::read_to_string(&deb_control).unwrap();
    assert!(control_text.contains("Package: quantum-strata-app"));
    assert!(control_text.contains("Version: 1.5.0"));

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_plugin_declarative_action_methods() {
    let temp_dir = std::env::temp_dir().join(format!("strata_action_test_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let toml_file = temp_dir.join("strata.toml");
    let toml_content = r#"
[app]
name = "core-engine"
version = "0.1.0"

[[plugins]]
name = "prefix-adder"
action = "prefix:enterprise-"

[[plugins]]
name = "version-bumper"
action = "bump_patch"
"#;
    fs::write(&toml_file, toml_content).unwrap();

    let report = Packager::package(BuildTarget::Linux, Some(PackageFormat::AppImage), &temp_dir).unwrap();
    let file_name = report.package_path.file_name().unwrap().to_str().unwrap();
    assert!(
        file_name.contains("enterprise-core-engine"),
        "Expected 'enterprise-core-engine', got '{}'",
        file_name
    );

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_plugin_environment_conditional_execution() {
    let temp_dir = std::env::temp_dir().join(format!("strata_env_plugin_test_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let yaml_file = temp_dir.join("strata.yaml");
    let yaml_content = r#"
app:
  name: "my-app"
  version: "0.1.0"

plugins:
  - name: "prod-only"
    action: "suffix:-prod"
    environments:
      - "production"

  - name: "dev-only"
    action: "suffix:-dev"
    environments:
      - "development"
"#;
    fs::write(&yaml_file, yaml_content).unwrap();

    // 1. In production environment: should only run prod-only
    let prod_report = Packager::package_with_env(BuildTarget::Desktop, None, &temp_dir, Some("production")).unwrap();
    let prod_name = prod_report.package_path.file_name().unwrap().to_str().unwrap();
    assert!(prod_name.contains("my-app-prod"), "Expected 'my-app-prod', got '{}'", prod_name);
    assert!(!prod_name.contains("dev"));

    // 2. In development environment: should only run dev-only
    let dev_report = Packager::package_with_env(BuildTarget::Desktop, None, &temp_dir, Some("development")).unwrap();
    let dev_name = dev_report.package_path.file_name().unwrap().to_str().unwrap();
    assert!(dev_name.contains("my-app-dev"), "Expected 'my-app-dev', got '{}'", dev_name);
    assert!(!dev_name.contains("prod"));

    let _ = fs::remove_dir_all(&temp_dir);
}

struct CustomInMemPlugin;
impl StrataBuildPlugin for CustomInMemPlugin {
    fn name(&self) -> &str {
        "custom-in-memory-plugin"
    }

    fn transform_config(&self, config: &mut StrataConfig, _ctx: &PluginContext) -> Result<(), String> {
        config.app.name = format!("{}-memory-injected", config.app.name);
        Ok(())
    }
}

#[test]
fn test_in_memory_plugin_registration() {
    let temp_dir = std::env::temp_dir().join(format!("strata_in_mem_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    PluginEngine::clear_in_memory_plugins();
    PluginEngine::register_in_memory_plugin(Box::new(CustomInMemPlugin));

    let report = Packager::package(BuildTarget::Desktop, None, &temp_dir).unwrap();
    let file_name = report.package_path.file_name().unwrap().to_str().unwrap();
    assert!(file_name.contains("memory-injected"));

    PluginEngine::clear_in_memory_plugins();
    let _ = fs::remove_dir_all(&temp_dir);
}
