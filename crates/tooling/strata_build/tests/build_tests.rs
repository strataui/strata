use strata_build::{
    ActionContext, ActionRegistry, BuildContext, BuildPlugin, DynamicBrandingPlugin,
    PackageManifest, PluginRegistry, ProjectConfig,
};

#[test]
fn test_parse_strata_toml() {
    let toml_src = r#"
[app]
name = "my-workbench-app"
version = "2.1.0"
template = "workbench"
description = "Enterprise desktop developer workbench"

[window]
title = "Workbench Studio"
width = 1920
height = 1080
resizable = true
fullscreen = false

[web]
target = "web-wasm"
output_dir = "public"

[build]
target_cpu = "native"
opt_level = "3"
plugins = ["branding", "analytics"]

[env.production]
app.name = "my-workbench-app-prod"
window.title = "Workbench Studio (Production)"
"#;

    let config = ProjectConfig::parse_toml(toml_src).expect("Failed to parse TOML");
    assert_eq!(config.app.name, "my-workbench-app");
    assert_eq!(config.app.version, "2.1.0");
    assert_eq!(config.app.template, "workbench");
    assert_eq!(config.window.width, 1920);
    assert_eq!(config.window.height, 1080);
    assert_eq!(config.web.target, "web-wasm");
    assert_eq!(config.build.plugins.len(), 2);

    // Test environment cascade
    let prod_config = config.resolve_environment("production");
    assert_eq!(prod_config.app.name, "my-workbench-app-prod");
    assert_eq!(prod_config.window.title, "Workbench Studio (Production)");
}

#[test]
fn test_parse_strata_yaml() {
    let yaml_src = r#"
app:
  name: "eda-circuit-studio"
  version: "1.0.4"
  template: "studio"
  description: "SPICE mixed-signal CAD studio"

window:
  title: "Strata EDA Studio"
  width: 1440
  height: 900
  resizable: true

web:
  target: "web-js"

build:
  plugins: ["rename-app"]

env:
  staging:
    app.name: "eda-studio-stage"
    window.title: "Strata EDA (STAGING)"
"#;

    let config = ProjectConfig::parse_yaml(yaml_src).expect("Failed to parse YAML");
    assert_eq!(config.app.name, "eda-circuit-studio");
    assert_eq!(config.app.version, "1.0.4");
    assert_eq!(config.window.width, 1440);
    assert_eq!(config.window.height, 900);
    assert_eq!(config.build.plugins, vec!["rename-app".to_string()]);

    let stage_config = config.resolve_environment("staging");
    assert_eq!(stage_config.app.name, "eda-studio-stage");
    assert_eq!(stage_config.window.title, "Strata EDA (STAGING)");
}

#[test]
fn test_dynamic_branding_plugin_hook() {
    let mut config = ProjectConfig::default();
    config.app.name = "flight-controller".to_string();

    let mut registry = PluginRegistry::new();
    let plugin = DynamicBrandingPlugin::new("enterprise-", "-v2");
    registry.register(Box::new(plugin));

    assert_eq!(registry.plugin_count(), 1);

    // Execute on_load_config
    registry
        .execute_on_load_config(&mut config, "production")
        .expect("Plugin execution failed");

    assert_eq!(config.app.name, "enterprise-flight-controller-v2");
    assert_eq!(config.window.title, "enterprise-flight-controller-v2 (PROD)");

    // Modify packaging manifest
    let mut manifest = PackageManifest {
        app_name: "flight-controller".to_string(),
        version: "1.0.0".to_string(),
        identifier: "com.strata.flight".to_string(),
        executable_name: "flight".to_string(),
    };

    registry
        .execute_modify_manifest(&mut manifest)
        .expect("Manifest modification failed");

    assert_eq!(manifest.app_name, "enterprise-flight-controller-v2");
}

#[test]
fn test_custom_build_plugin_execution() {
    struct CustomOptimizationPlugin;
    impl BuildPlugin for CustomOptimizationPlugin {
        fn name(&self) -> &str {
            "custom-opt"
        }
        fn before_build(&self, context: &BuildContext) -> Result<(), String> {
            if context.release && context.target == "desktop" {
                Ok(())
            } else {
                Err("Plugin requires release desktop target".to_string())
            }
        }
    }

    let mut registry = PluginRegistry::new();
    registry.register(Box::new(CustomOptimizationPlugin));

    let valid_ctx = BuildContext {
        target: "desktop".to_string(),
        environment: "production".to_string(),
        out_dir: std::path::PathBuf::from("/tmp"),
        release: true,
    };
    assert!(registry.execute_before_build(&valid_ctx).is_ok());

    let invalid_ctx = BuildContext {
        target: "web".to_string(),
        environment: "dev".to_string(),
        out_dir: std::path::PathBuf::from("/tmp"),
        release: false,
    };
    assert!(registry.execute_before_build(&invalid_ctx).is_err());
}

#[test]
fn test_action_registry_dispatch() {
    let mut actions = ActionRegistry::new();

    // Built-in info action
    let ctx = ActionContext {
        name: "info".to_string(),
        environment: "production".to_string(),
        arguments: vec![],
        config: ProjectConfig::default(),
    };
    let info_res = actions.dispatch(&ctx).expect("info action failed");
    assert!(info_res.contains("Strata App 'strata-app'"));
    assert!(info_res.contains("production"));

    // Register custom action
    actions.register("deploy-dry-run", |ctx| {
        Ok(format!("Ready to deploy {} to {}", ctx.config.app.name, ctx.environment))
    });

    let deploy_ctx = ActionContext {
        name: "deploy-dry-run".to_string(),
        environment: "us-west-2".to_string(),
        arguments: vec!["--force".to_string()],
        config: ProjectConfig::default(),
    };
    let deploy_res = actions.dispatch(&deploy_ctx).expect("custom deploy action failed");
    assert_eq!(deploy_res, "Ready to deploy strata-app to us-west-2");
}
