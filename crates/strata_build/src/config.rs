//! Multi-format configuration definitions and parsing for `strata.toml` and `strata.yaml`.

use std::collections::HashMap;
use crate::yaml_parser::parse_yaml;

/// Application section in Strata configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct AppConfig {
    pub name: String,
    pub version: String,
    pub template: String,
    pub description: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            name: "strata-app".to_string(),
            version: "0.1.0".to_string(),
            template: "saas".to_string(),
            description: String::new(),
        }
    }
}

/// Window configuration section.
#[derive(Debug, Clone, PartialEq)]
pub struct WindowConfig {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub resizable: bool,
    pub fullscreen: bool,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            title: "strata-app".to_string(),
            width: 1280,
            height: 800,
            resizable: true,
            fullscreen: false,
        }
    }
}

/// Web compilation configuration section.
#[derive(Debug, Clone, PartialEq)]
pub struct WebConfig {
    pub target: String, // "web-js" or "web-wasm"
    pub output_dir: String,
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            target: "web-js".to_string(),
            output_dir: "dist".to_string(),
        }
    }
}

/// Build pipeline configuration section.
#[derive(Debug, Clone, PartialEq)]
pub struct BuildConfig {
    pub target_cpu: String,
    pub opt_level: String,
    pub plugins: Vec<String>,
}

impl Default for BuildConfig {
    fn default() -> Self {
        Self {
            target_cpu: "native".to_string(),
            opt_level: "3".to_string(),
            plugins: Vec::new(),
        }
    }
}

/// Master project configuration representation.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ProjectConfig {
    pub app: AppConfig,
    pub window: WindowConfig,
    pub web: WebConfig,
    pub build: BuildConfig,
    pub env_overrides: HashMap<String, HashMap<String, String>>,
}

impl ProjectConfig {
    /// Parses configuration from either TOML or YAML content based on format hint or extension.
    pub fn parse_auto(content: &str, is_yaml: bool) -> Result<Self, String> {
        if is_yaml {
            Self::parse_yaml(content)
        } else {
            Self::parse_toml(content)
        }
    }

    /// Parses from YAML content.
    pub fn parse_yaml(content: &str) -> Result<Self, String> {
        let val = parse_yaml(content)?;
        let mut config = ProjectConfig::default();

        if let Some(app_val) = val.get("app") {
            if let Some(name) = app_val.get("name").and_then(|v| v.as_str()) {
                config.app.name = name.to_string();
            }
            if let Some(ver) = app_val.get("version").and_then(|v| v.as_str()) {
                config.app.version = ver.to_string();
            }
            if let Some(tpl) = app_val.get("template").and_then(|v| v.as_str()) {
                config.app.template = tpl.to_string();
            }
            if let Some(desc) = app_val.get("description").and_then(|v| v.as_str()) {
                config.app.description = desc.to_string();
            }
        }

        if let Some(win_val) = val.get("window") {
            if let Some(title) = win_val.get("title").and_then(|v| v.as_str()) {
                config.window.title = title.to_string();
            }
            if let Some(w) = win_val.get("width").and_then(|v| v.as_i64()) {
                config.window.width = w as u32;
            }
            if let Some(h) = win_val.get("height").and_then(|v| v.as_i64()) {
                config.window.height = h as u32;
            }
            if let Some(res) = win_val.get("resizable").and_then(|v| v.as_bool()) {
                config.window.resizable = res;
            }
            if let Some(fs) = win_val.get("fullscreen").and_then(|v| v.as_bool()) {
                config.window.fullscreen = fs;
            }
        }

        if let Some(web_val) = val.get("web") {
            if let Some(target) = web_val.get("target").and_then(|v| v.as_str()) {
                config.web.target = target.to_string();
            }
            if let Some(out) = web_val.get("output_dir").and_then(|v| v.as_str()) {
                config.web.output_dir = out.to_string();
            }
        }

        if let Some(bld_val) = val.get("build") {
            if let Some(plugins_arr) = bld_val.get("plugins").and_then(|v| v.as_array()) {
                config.build.plugins = plugins_arr
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect();
            }
        }

        if let Some(env_map) = val.get("env").and_then(|v| v.as_map()) {
            for (env_name, env_props) in env_map {
                if let Some(props_map) = env_props.as_map() {
                    let mut overrides = HashMap::new();
                    for (k, v) in props_map {
                        if let Some(s) = v.as_str() {
                            overrides.insert(k.clone(), s.to_string());
                        }
                    }
                    config.env_overrides.insert(env_name.clone(), overrides);
                }
            }
        }

        Ok(config)
    }

    /// Parses from TOML content.
    pub fn parse_toml(content: &str) -> Result<Self, String> {
        let mut config = ProjectConfig::default();
        let mut current_section = "";

        for raw_line in content.lines() {
            let line = raw_line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }

            if line.starts_with('[') && line.ends_with(']') {
                current_section = &line[1..line.len() - 1];
                continue;
            }

            if let Some(eq_idx) = line.find('=') {
                let key = line[..eq_idx].trim();
                let val_str = line[eq_idx + 1..].trim();

                let clean_val = val_str.trim_matches('"').trim_matches('\'');

                match current_section {
                    "app" => match key {
                        "name" => config.app.name = clean_val.to_string(),
                        "version" => config.app.version = clean_val.to_string(),
                        "template" => config.app.template = clean_val.to_string(),
                        "description" => config.app.description = clean_val.to_string(),
                        _ => {}
                    },
                    "window" => match key {
                        "title" => config.window.title = clean_val.to_string(),
                        "width" => {
                            if let Ok(w) = clean_val.parse::<u32>() {
                                config.window.width = w;
                            }
                        }
                        "height" => {
                            if let Ok(h) = clean_val.parse::<u32>() {
                                config.window.height = h;
                            }
                        }
                        "resizable" => config.window.resizable = clean_val.parse().unwrap_or(true),
                        "fullscreen" => config.window.fullscreen = clean_val.parse().unwrap_or(false),
                        _ => {}
                    },
                    "web" => match key {
                        "target" => config.web.target = clean_val.to_string(),
                        "output_dir" => config.web.output_dir = clean_val.to_string(),
                        _ => {}
                    },
                    "build" => match key {
                        "target_cpu" => config.build.target_cpu = clean_val.to_string(),
                        "opt_level" => config.build.opt_level = clean_val.to_string(),
                        "plugins" => {
                            let trimmed = val_str.trim_start_matches('[').trim_end_matches(']');
                            config.build.plugins = trimmed
                                .split(',')
                                .map(|s| s.trim().trim_matches('"').to_string())
                                .filter(|s| !s.is_empty())
                                .collect();
                        }
                        _ => {}
                    },
                    sec if sec.starts_with("env.") => {
                        let env_name = &sec[4..];
                        config
                            .env_overrides
                            .entry(env_name.to_string())
                            .or_default()
                            .insert(key.to_string(), clean_val.to_string());
                    }
                    _ => {}
                }
            }
        }

        Ok(config)
    }

    /// Resolves environment-specific cascading overrides.
    pub fn resolve_environment(&self, env: &str) -> ProjectConfig {
        let mut resolved = self.clone();
        if let Some(overrides) = self.env_overrides.get(env) {
            for (key, val) in overrides {
                match key.as_str() {
                    "app.name" => resolved.app.name = val.clone(),
                    "app.version" => resolved.app.version = val.clone(),
                    "window.title" => resolved.window.title = val.clone(),
                    "window.width" => {
                        if let Ok(w) = val.parse::<u32>() {
                            resolved.window.width = w;
                        }
                    }
                    "window.height" => {
                        if let Ok(h) = val.parse::<u32>() {
                            resolved.window.height = h;
                        }
                    }
                    "web.target" => resolved.web.target = val.clone(),
                    _ => {}
                }
            }
        }
        resolved
    }
}
