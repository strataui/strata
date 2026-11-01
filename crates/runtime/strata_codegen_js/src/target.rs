//! Unified Web Target Builder: Coordinates Mode A (Web-WASM) and Mode B (Web-JS) pipelines.

use strata_dom::NodeBlueprint;

use crate::css_extractor::CssExtractor;
use crate::runtime::JsRuntimeEmitter;
use crate::transpiler::JsTranspiler;
use crate::wasm_harness::WasmHarnessGenerator;

/// Web compilation target mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WebTargetMode {
    /// Mode B: Lightweight vanilla ESM JS emitting direct DOM calls (zero WASM).
    #[default]
    PureJsDom,
    /// Mode A: Full Strata engine canvas surface backed by WebGPU/WebGL2 and WASM.
    WasmCanvas,
}

/// Configuration options for compiling a web distribution bundle.
#[derive(Debug, Clone)]
pub struct WebBuildConfig {
    pub mode: WebTargetMode,
    pub app_title: String,
    pub root_element_id: String,
    pub canvas_element_id: String,
    pub minify: bool,
    pub inline_runtime: bool,
    pub extract_css: bool,
    pub wasm_binary_name: String,
    pub loader_js_name: String,
    pub runtime_js_name: String,
    pub app_js_name: String,
    pub styles_css_name: String,
}

impl Default for WebBuildConfig {
    fn default() -> Self {
        Self {
            mode: WebTargetMode::PureJsDom,
            app_title: "Strata Web Application".to_string(),
            root_element_id: "app".to_string(),
            canvas_element_id: "strata-surface".to_string(),
            minify: false,
            inline_runtime: false,
            extract_css: true,
            wasm_binary_name: "strata_app.wasm".to_string(),
            loader_js_name: "loader.js".to_string(),
            runtime_js_name: "runtime.js".to_string(),
            app_js_name: "app.js".to_string(),
            styles_css_name: "styles.css".to_string(),
        }
    }
}

/// Output bundle containing all generated web assets ready to be served or packaged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebBuildArtifacts {
    pub mode: WebTargetMode,
    pub html: String,
    pub js: String,
    pub css: Option<String>,
    pub wasm_loader: Option<String>,
    pub runtime_js: Option<String>,
}

/// Unified builder orchestrating web compilation across Mode A and Mode B.
pub struct WebTargetBuilder;

impl WebTargetBuilder {
    /// Builds web distribution artifacts for the provided component blueprint and configuration.
    pub fn build(config: &WebBuildConfig, blueprint: &NodeBlueprint) -> WebBuildArtifacts {
        match config.mode {
            WebTargetMode::PureJsDom => Self::build_pure_js(config, blueprint),
            WebTargetMode::WasmCanvas => Self::build_wasm_canvas(config),
        }
    }

    fn build_pure_js(config: &WebBuildConfig, blueprint: &NodeBlueprint) -> WebBuildArtifacts {
        // 1. CSS Extraction
        let css = if config.extract_css {
            let classes = CssExtractor::collect_classes(blueprint);
            Some(CssExtractor::generate_stylesheet(&classes))
        } else {
            None
        };

        // 2. JavaScript Runtime & App Generation
        let runtime_source = JsRuntimeEmitter::emit_full_runtime(true);

        let app_js = if config.inline_runtime {
            let app_body = JsTranspiler::emit_standalone_app(
                blueprint,
                &config.root_element_id,
                "./runtime.js",
            );
            // Replace import with inlined runtime
            let stripped_app = app_body.lines().skip(2).collect::<Vec<_>>().join("\n");
            format!("{}\n\n{}", runtime_source, stripped_app)
        } else {
            JsTranspiler::emit_standalone_app(
                blueprint,
                &config.root_element_id,
                &format!("./{}", config.runtime_js_name),
            )
        };

        // 3. HTML Shell Generation
        let mut html = String::new();
        html.push_str("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n");
        html.push_str("  <meta charset=\"UTF-8\">\n");
        html.push_str("  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">\n");
        html.push_str(&format!("  <title>{}</title>\n", config.app_title));

        if css.is_some() {
            html.push_str(&format!(
                "  <link rel=\"stylesheet\" href=\"./{}\">\n",
                config.styles_css_name
            ));
        }

        html.push_str("</head>\n<body>\n");
        html.push_str(&format!("  <div id=\"{}\"></div>\n", config.root_element_id));

        if config.inline_runtime {
            html.push_str("  <script type=\"module\">\n");
            html.push_str(&app_js);
            html.push_str("\n  </script>\n");
        } else {
            html.push_str(&format!(
                "  <script type=\"module\" src=\"./{}\"></script>\n",
                config.app_js_name
            ));
        }

        html.push_str("</body>\n</html>\n");

        WebBuildArtifacts {
            mode: WebTargetMode::PureJsDom,
            html,
            js: app_js,
            css,
            wasm_loader: None,
            runtime_js: if config.inline_runtime {
                None
            } else {
                Some(runtime_source)
            },
        }
    }

    fn build_wasm_canvas(config: &WebBuildConfig) -> WebBuildArtifacts {
        let html = WasmHarnessGenerator::generate_index_html(
            &config.app_title,
            &config.canvas_element_id,
            &format!("./{}", config.loader_js_name),
        );

        let wasm_loader = WasmHarnessGenerator::generate_wasm_loader_js(
            &config.canvas_element_id,
            &format!("./{}", config.wasm_binary_name),
        );

        WebBuildArtifacts {
            mode: WebTargetMode::WasmCanvas,
            html,
            js: wasm_loader.clone(),
            css: None,
            wasm_loader: Some(wasm_loader),
            runtime_js: None,
        }
    }
}
