//! Tests for StrataConfig parser and serializer supporting strata.toml and strata.yaml.

use std::fs;
use strata_cli::config::StrataConfig;

#[test]
fn test_parse_strata_toml() {
    let toml_content = r#"
[app]
name = "my-custom-app"
version = "1.2.3"
template = "workbench"
description = "High-performance developer workbench"

[window]
title = "My Custom App Window"
width = 1600
height = 900
resizable = true
transparent = false

[web]
target = "web-wasm"

[[plugins]]
name = "name-injector"
path = "plugins/dynamic_name.rs"
methods = ["transform_config", "before_package"]
environments = ["production", "staging"]

[environments.production]
app.name = "my-custom-app-prod"
app.version = "2.0.0"
actions = ["strip", "audit"]

[environments.development]
app.name = "my-custom-app-dev"
"#;

    let config = StrataConfig::from_toml(toml_content).unwrap();
    assert_eq!(config.app.name, "my-custom-app");
    assert_eq!(config.app.version, "1.2.3");
    assert_eq!(config.app.template, "workbench");
    assert_eq!(config.window.title, "My Custom App Window");
    assert_eq!(config.window.width, 1600);
    assert_eq!(config.window.height, 900);
    assert!(config.window.resizable);
    assert_eq!(config.web.target, "web-wasm");

    assert_eq!(config.plugins.len(), 1);
    let p = &config.plugins[0];
    assert_eq!(p.name, "name-injector");
    assert_eq!(p.methods, vec!["transform_config", "before_package"]);
    assert_eq!(p.environments, vec!["production", "staging"]);

    // Test environment override
    let mut prod_cfg = config.clone();
    prod_cfg.apply_environment("production");
    assert_eq!(prod_cfg.app.name, "my-custom-app-prod");
    assert_eq!(prod_cfg.app.version, "2.0.0");
    assert_eq!(prod_cfg.active_actions, vec!["strip", "audit"]);

    let mut dev_cfg = config.clone();
    dev_cfg.apply_environment("development");
    assert_eq!(dev_cfg.app.name, "my-custom-app-dev");
    assert_eq!(dev_cfg.app.version, "1.2.3"); // Not overridden in dev
}

#[test]
fn test_parse_strata_yaml() {
    let yaml_content = r#"
app:
  name: "yaml-app"
  version: "0.5.0"
  template: "saas"
  description: "Cloud telemetry application"

window:
  title: "Cloud Telemetry"
  width: 1440
  height: 900
  resizable: true

web:
  target: "web-js"

plugins:
  - name: "custom-plugin"
    path: "plugins/custom.rs"
    methods:
      - "transform_config"
    environments:
      - "production"

environments:
  production:
    name: "yaml-app-enterprise"
    version: "1.0.0"
    actions:
      - "bundle_certs"
"#;

    let config = StrataConfig::from_yaml(yaml_content).unwrap();
    assert_eq!(config.app.name, "yaml-app");
    assert_eq!(config.app.version, "0.5.0");
    assert_eq!(config.app.template, "saas");
    assert_eq!(config.window.title, "Cloud Telemetry");
    assert_eq!(config.window.width, 1440);
    assert_eq!(config.plugins.len(), 1);
    assert_eq!(config.plugins[0].name, "custom-plugin");

    let mut prod_cfg = config.clone();
    prod_cfg.apply_environment("production");
    assert_eq!(prod_cfg.app.name, "yaml-app-enterprise");
    assert_eq!(prod_cfg.app.version, "1.0.0");
    assert_eq!(prod_cfg.active_actions, vec!["bundle_certs"]);
}

#[test]
fn test_load_from_dir_precedence() {
    let temp_dir = std::env::temp_dir().join(format!("strata_cfg_test_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    // 1. Write strata.toml
    let toml_file = temp_dir.join("strata.toml");
    fs::write(&toml_file, "[app]\nname = \"from-toml\"\n").unwrap();

    let cfg = StrataConfig::load_from_dir(&temp_dir).unwrap();
    assert_eq!(cfg.app.name, "from-toml");

    // 2. Write strata.yaml which takes precedence over strata.toml
    let yaml_file = temp_dir.join("strata.yaml");
    fs::write(&yaml_file, "app:\n  name: \"from-yaml\"\n").unwrap();

    let cfg = StrataConfig::load_from_dir(&temp_dir).unwrap();
    assert_eq!(cfg.app.name, "from-yaml");

    let _ = fs::remove_dir_all(&temp_dir);
}
