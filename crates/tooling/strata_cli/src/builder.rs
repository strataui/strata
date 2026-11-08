//! Cross-platform build runner driving native and web targets.

use std::fs;
use std::path::{Path, PathBuf};
use strata_codegen_js::prelude::*;
use strata_dom::{NodeBlueprint, Tag};

use crate::args::BuildTarget;
use crate::config::StrataConfig;
use crate::plugin::{PluginContext, PluginEngine};

/// Summary report of a completed build operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildReport {
    pub target: BuildTarget,
    pub release: bool,
    pub artifacts: Vec<PathBuf>,
}

/// Build runner engine.
pub struct BuildRunner;

impl BuildRunner {
    /// Builds application artifacts for the specified target.
    pub fn build(
        target: BuildTarget,
        release: bool,
        out_dir: &Path,
    ) -> Result<BuildReport, String> {
        Self::build_with_options(target, release, false, out_dir, None)
    }

    /// Builds application artifacts with an optional environment override.
    pub fn build_with_env(
        target: BuildTarget,
        release: bool,
        out_dir: &Path,
        env: Option<&str>,
    ) -> Result<BuildReport, String> {
        Self::build_with_options(target, release, false, out_dir, env)
    }

    /// Builds application artifacts with force override and environment options.
    pub fn build_with_options(
        target: BuildTarget,
        release: bool,
        force: bool,
        out_dir: &Path,
        env: Option<&str>,
    ) -> Result<BuildReport, String> {
        let _ = force;

        let dist_target = out_dir.join("dist").join(target.as_str());
        fs::create_dir_all(&dist_target)
            .map_err(|e| format!("Failed to create build output directory: {}", e))?;

        // 1. Load project configuration and run plugin transformations
        let mut strata_cfg = StrataConfig::load_from_dir(out_dir).unwrap_or_default();
        let mut ctx = PluginContext::new(target, release, out_dir, &dist_target);
        if let Some(e) = env {
            ctx.environment = e.to_string();
        }
        let _ = PluginEngine::run_config_transformations(&mut strata_cfg, &ctx);

        let mut artifacts = Vec::new();

        match target {
            BuildTarget::WebJs => {
                let config = WebBuildConfig {
                    mode: WebTargetMode::PureJsDom,
                    app_title: strata_cfg.window.title.clone(),
                    app_js_name: strata_cfg.web.app_js_name.clone(),
                    styles_css_name: strata_cfg.web.styles_css_name.clone(),
                    ..Default::default()
                };

                // Root component blueprint for standalone bundle
                let blueprint = NodeBlueprint::Element {
                    tag: Tag::Div,
                    classes: vec!["flex flex-col items-center justify-center h-screen".to_string()].into(),
                    attributes: vec![],
                    children: vec![NodeBlueprint::text(strata_cfg.app.name.clone())],
                };

                let bundle = WebTargetBuilder::build(&config, &blueprint);

                // 1. index.html
                let html_path = dist_target.join("index.html");
                fs::write(&html_path, bundle.html)
                    .map_err(|e| format!("Failed to write index.html: {}", e))?;
                artifacts.push(html_path);

                // 2. app.js
                let app_js_path = dist_target.join(&config.app_js_name);
                fs::write(&app_js_path, bundle.js)
                    .map_err(|e| format!("Failed to write app.js: {}", e))?;
                artifacts.push(app_js_path);

                // 3. runtime.js
                if let Some(runtime_js) = bundle.runtime_js {
                    let runtime_path = dist_target.join(&config.runtime_js_name);
                    fs::write(&runtime_path, runtime_js)
                        .map_err(|e| format!("Failed to write runtime.js: {}", e))?;
                    artifacts.push(runtime_path);
                }

                // 4. styles.css
                if let Some(css) = bundle.css {
                    let css_path = dist_target.join(&config.styles_css_name);
                    fs::write(&css_path, css)
                        .map_err(|e| format!("Failed to write styles.css: {}", e))?;
                    artifacts.push(css_path);
                }
            }
            BuildTarget::WebWasm => {
                let config = WebBuildConfig {
                    mode: WebTargetMode::WasmCanvas,
                    app_title: strata_cfg.window.title.clone(),
                    ..Default::default()
                };

                let blueprint = NodeBlueprint::empty();
                let bundle = WebTargetBuilder::build(&config, &blueprint);

                let html_path = dist_target.join("index.html");
                fs::write(&html_path, bundle.html)
                    .map_err(|e| format!("Failed to write index.html: {}", e))?;
                artifacts.push(html_path);

                if let Some(loader_js) = bundle.wasm_loader {
                    let loader_path = dist_target.join(&config.loader_js_name);
                    fs::write(&loader_path, loader_js)
                        .map_err(|e| format!("Failed to write loader.js: {}", e))?;
                    artifacts.push(loader_path);
                }
            }
            BuildTarget::Desktop
            | BuildTarget::MacOs
            | BuildTarget::Windows
            | BuildTarget::Linux
            | BuildTarget::Android
            | BuildTarget::Ios => {
                let binary_name = if target == BuildTarget::Windows {
                    format!("{}.exe", strata_cfg.app.name)
                } else {
                    strata_cfg.app.name.clone()
                };
                let bin_path = dist_target.join(binary_name);
                fs::write(&bin_path, b"STRATA_BINARY_STUB\n")
                    .map_err(|e| format!("Failed to write binary stub: {}", e))?;
                artifacts.push(bin_path);

                let manifest_path = dist_target.join("strata_build.json");
                let manifest = format!(
                    r#"{{"app_name": "{}", "version": "{}", "target": "{}", "release": {}, "mode": "native_zero_chromium"}}"#,
                    strata_cfg.app.name,
                    strata_cfg.app.version,
                    target.as_str(),
                    release
                );
                fs::write(&manifest_path, manifest)
                    .map_err(|e| format!("Failed to write build manifest: {}", e))?;
                artifacts.push(manifest_path);
            }
        }

        Ok(BuildReport {
            target,
            release,
            artifacts,
        })
    }
}
