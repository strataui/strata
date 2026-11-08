//! Unified configuration engine for Strata projects supporting `strata.toml` and `strata.yaml`.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Application metadata configuration section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppConfig {
    pub name: String,
    pub version: String,
    pub description: String,
    pub template: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            name: "strata-app".to_string(),
            version: "0.1.0".to_string(),
            description: "Strata Universal Frontend Application (Zero Chromium Pure Rust)".to_string(),
            template: "saas".to_string(),
        }
    }
}

/// Window presentation configuration section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowConfigSection {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub resizable: bool,
    pub transparent: bool,
}

impl Default for WindowConfigSection {
    fn default() -> Self {
        Self {
            title: "Strata App".to_string(),
            width: 1280,
            height: 800,
            resizable: true,
            transparent: false,
        }
    }
}

/// Web target output configuration section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebConfigSection {
    pub target: String,
    pub app_js_name: String,
    pub styles_css_name: String,
}

impl Default for WebConfigSection {
    fn default() -> Self {
        Self {
            target: "web-js".to_string(),
            app_js_name: "app.js".to_string(),
            styles_css_name: "styles.css".to_string(),
        }
    }
}

/// Specification for a build/packaging plugin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginSpec {
    pub name: String,
    pub path: Option<PathBuf>,
    pub crate_name: Option<String>,
    pub action: Option<String>,
    pub methods: Vec<String>,
    pub environments: Vec<String>,
}

impl PluginSpec {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            path: None,
            crate_name: None,
            action: None,
            methods: Vec::new(),
            environments: Vec::new(),
        }
    }

    pub fn with_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.path = Some(path.into());
        self
    }

    pub fn with_methods(mut self, methods: &[&str]) -> Self {
        self.methods = methods.iter().map(|s| s.to_string()).collect();
        self
    }

    pub fn with_environments(mut self, envs: &[&str]) -> Self {
        self.environments = envs.iter().map(|s| s.to_string()).collect();
        self
    }
}

/// Environment-specific overrides and lifecycle actions.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EnvironmentConfig {
    pub app_name: Option<String>,
    pub app_version: Option<String>,
    pub window_title: Option<String>,
    pub actions: Vec<String>,
}

/// Root configuration holding application, window, web, plugins, and environment definitions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrataConfig {
    pub app: AppConfig,
    pub window: WindowConfigSection,
    pub web: WebConfigSection,
    pub plugins: Vec<PluginSpec>,
    pub environments: HashMap<String, EnvironmentConfig>,
    pub active_environment: String,
    pub active_actions: Vec<String>,
}

impl Default for StrataConfig {
    fn default() -> Self {
        Self {
            app: AppConfig::default(),
            window: WindowConfigSection::default(),
            web: WebConfigSection::default(),
            plugins: Vec::new(),
            environments: HashMap::new(),
            active_environment: "development".to_string(),
            active_actions: Vec::new(),
        }
    }
}

impl StrataConfig {
    /// Discovers and loads configuration from `dir`, prioritizing `strata.yaml` / `strata.yml`, then `strata.toml`.
    pub fn load_from_dir(dir: &Path) -> Result<Self, String> {
        let yaml_path = dir.join("strata.yaml");
        let yml_path = dir.join("strata.yml");
        let toml_path = dir.join("strata.toml");

        if yaml_path.exists() {
            let content = fs::read_to_string(&yaml_path)
                .map_err(|e| format!("Failed to read 'strata.yaml': {}", e))?;
            Self::from_yaml(&content)
        } else if yml_path.exists() {
            let content = fs::read_to_string(&yml_path)
                .map_err(|e| format!("Failed to read 'strata.yml': {}", e))?;
            Self::from_yaml(&content)
        } else if toml_path.exists() {
            let content = fs::read_to_string(&toml_path)
                .map_err(|e| format!("Failed to read 'strata.toml': {}", e))?;
            Self::from_toml(&content)
        } else {
            Ok(Self::default())
        }
    }

    /// Applies environment-specific overrides (e.g. "production", "development", "staging", "ci").
    pub fn apply_environment(&mut self, env: &str) {
        self.active_environment = env.to_string();
        if let Some(env_cfg) = self.environments.get(env) {
            if let Some(ref name) = env_cfg.app_name {
                self.app.name = name.clone();
            }
            if let Some(ref version) = env_cfg.app_version {
                self.app.version = version.clone();
            }
            if let Some(ref title) = env_cfg.window_title {
                self.window.title = title.clone();
            }
            for action in &env_cfg.actions {
                if !self.active_actions.contains(action) {
                    self.active_actions.push(action.clone());
                }
            }
        }
    }

    /// Parses configuration from TOML format.
    pub fn from_toml(content: &str) -> Result<Self, String> {
        let mut config = Self::default();
        let mut current_section = String::new();
        let mut in_plugin_array = false;
        let mut current_plugin: Option<PluginSpec> = None;

        for raw_line in content.lines() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if line.starts_with("[[") && line.ends_with("]]") {
                let table_name = line[2..line.len() - 2].trim();
                if let Some(p) = current_plugin.take() {
                    config.plugins.push(p);
                }
                if table_name == "plugins" {
                    in_plugin_array = true;
                    current_plugin = Some(PluginSpec::new(""));
                } else {
                    in_plugin_array = false;
                }
                current_section.clear();
                continue;
            }

            if line.starts_with('[') && line.ends_with(']') {
                if let Some(p) = current_plugin.take() {
                    config.plugins.push(p);
                }
                in_plugin_array = false;
                current_section = line[1..line.len() - 1].trim().to_string();
                continue;
            }

            if let Some((raw_k, raw_v)) = line.split_once('=') {
                let key = raw_k.trim();
                let val_str = raw_v.trim();

                if in_plugin_array {
                    if let Some(ref mut plugin) = current_plugin {
                        match key {
                            "name" => plugin.name = parse_string_val(val_str),
                            "path" => plugin.path = Some(PathBuf::from(parse_string_val(val_str))),
                            "crate" => plugin.crate_name = Some(parse_string_val(val_str)),
                            "action" => plugin.action = Some(parse_string_val(val_str)),
                            "methods" => plugin.methods = parse_string_array(val_str),
                            "environments" | "environment" => {
                                plugin.environments = parse_string_array(val_str);
                            }
                            _ => {}
                        }
                    }
                    continue;
                }

                if current_section.starts_with("environments.") || current_section.starts_with("env.") {
                    let env_name = current_section
                        .strip_prefix("environments.")
                        .or_else(|| current_section.strip_prefix("env."))
                        .unwrap_or("");
                    let entry = config.environments.entry(env_name.to_string()).or_default();
                    match key {
                        "app.name" | "name" => entry.app_name = Some(parse_string_val(val_str)),
                        "app.version" | "version" => entry.app_version = Some(parse_string_val(val_str)),
                        "window.title" | "title" => entry.window_title = Some(parse_string_val(val_str)),
                        "actions" => entry.actions = parse_string_array(val_str),
                        _ => {}
                    }
                    continue;
                }

                match current_section.as_str() {
                    "app" => match key {
                        "name" => config.app.name = parse_string_val(val_str),
                        "version" => config.app.version = parse_string_val(val_str),
                        "description" => config.app.description = parse_string_val(val_str),
                        "template" => config.app.template = parse_string_val(val_str),
                        _ => {}
                    },
                    "window" => match key {
                        "title" => config.window.title = parse_string_val(val_str),
                        "width" => {
                            if let Ok(w) = val_str.parse::<u32>() {
                                config.window.width = w;
                            }
                        }
                        "height" => {
                            if let Ok(h) = val_str.parse::<u32>() {
                                config.window.height = h;
                            }
                        }
                        "resizable" => config.window.resizable = val_str == "true",
                        "transparent" => config.window.transparent = val_str == "true",
                        _ => {}
                    },
                    "web" => match key {
                        "target" => config.web.target = parse_string_val(val_str),
                        "app_js" | "app_js_name" => config.web.app_js_name = parse_string_val(val_str),
                        "styles_css" | "styles_css_name" => {
                            config.web.styles_css_name = parse_string_val(val_str);
                        }
                        _ => {}
                    },
                    _ => {}
                }
            }
        }

        if let Some(p) = current_plugin.take() {
            config.plugins.push(p);
        }

        Ok(config)
    }

    /// Parses configuration from YAML format.
    pub fn from_yaml(content: &str) -> Result<Self, String> {
        let mut config = Self::default();
        let mut current_section = String::new();
        let mut in_plugins = false;
        let mut in_environments = false;
        let mut current_env_name = String::new();
        let mut current_plugin: Option<PluginSpec> = None;
        let mut current_list_key = String::new();

        for raw_line in content.lines() {
            let line_trimmed = raw_line.trim();
            if line_trimmed.is_empty() || line_trimmed.starts_with('#') {
                continue;
            }

            let indent = raw_line.chars().take_while(|c| c.is_whitespace()).count();

            // Root section key (e.g. "app:", "window:", "web:", "plugins:", "environments:")
            if indent == 0 && line_trimmed.ends_with(':') {
                if let Some(p) = current_plugin.take() {
                    config.plugins.push(p);
                }
                let section = line_trimmed.trim_end_matches(':').trim();
                current_section = section.to_string();
                in_plugins = section == "plugins";
                in_environments = section == "environments" || section == "env";
                current_env_name.clear();
                current_list_key.clear();
                continue;
            }

            if in_plugins {
                let is_sub_list_item = !current_list_key.is_empty()
                    && line_trimmed.starts_with("- ")
                    && !line_trimmed.strip_prefix("- ").unwrap_or("").contains(':');

                if is_sub_list_item {
                    if let Some(ref mut plugin) = current_plugin {
                        let item = parse_string_val(line_trimmed.strip_prefix("- ").unwrap_or("").trim());
                        if current_list_key == "methods" {
                            plugin.methods.push(item);
                        } else if current_list_key == "environments" || current_list_key == "environment" {
                            plugin.environments.push(item);
                        }
                    }
                    continue;
                }

                if line_trimmed.starts_with("- ") {
                    current_list_key.clear();
                    if let Some(p) = current_plugin.take() {
                        config.plugins.push(p);
                    }
                    let item_content = line_trimmed.strip_prefix("- ").unwrap_or("").trim();
                    let mut plugin = PluginSpec::new("");
                    if let Some((k, v)) = item_content.split_once(':') {
                        Self::set_plugin_field(&mut plugin, k.trim(), v.trim());
                    } else if !item_content.is_empty() {
                        plugin.name = parse_string_val(item_content);
                    }
                    current_plugin = Some(plugin);
                    continue;
                }

                if let Some(ref mut plugin) = current_plugin {
                    if let Some((k, v)) = line_trimmed.split_once(':') {
                        let key = k.trim();
                        let val = v.trim();
                        if val.is_empty() {
                            current_list_key = key.to_string();
                        } else {
                            current_list_key.clear();
                            Self::set_plugin_field(plugin, key, val);
                        }
                    }
                }
                continue;
            }

            if in_environments {
                if indent == 2 && line_trimmed.ends_with(':') {
                    current_env_name = line_trimmed.trim_end_matches(':').trim().to_string();
                    config.environments.entry(current_env_name.clone()).or_default();
                    current_list_key.clear();
                    continue;
                }

                if !current_env_name.is_empty() {
                    let entry = config.environments.entry(current_env_name.clone()).or_default();
                    if line_trimmed.starts_with("- ") && current_list_key == "actions" {
                        let act = parse_string_val(line_trimmed.strip_prefix("- ").unwrap_or("").trim());
                        entry.actions.push(act);
                        continue;
                    }

                    if let Some((k, v)) = line_trimmed.split_once(':') {
                        let key = k.trim();
                        let val = v.trim();
                        if val.is_empty() {
                            current_list_key = key.to_string();
                        } else {
                            current_list_key.clear();
                            match key {
                                "name" | "app.name" => entry.app_name = Some(parse_string_val(val)),
                                "version" | "app.version" => entry.app_version = Some(parse_string_val(val)),
                                "title" | "window.title" => entry.window_title = Some(parse_string_val(val)),
                                "actions" => entry.actions = parse_string_array(val),
                                _ => {}
                            }
                        }
                    }
                }
                continue;
            }

            // Normal key-value under app, window, web
            if let Some((k, v)) = line_trimmed.split_once(':') {
                let key = k.trim();
                let val_str = v.trim();

                match current_section.as_str() {
                    "app" => match key {
                        "name" => config.app.name = parse_string_val(val_str),
                        "version" => config.app.version = parse_string_val(val_str),
                        "description" => config.app.description = parse_string_val(val_str),
                        "template" => config.app.template = parse_string_val(val_str),
                        _ => {}
                    },
                    "window" => match key {
                        "title" => config.window.title = parse_string_val(val_str),
                        "width" => {
                            if let Ok(w) = val_str.parse::<u32>() {
                                config.window.width = w;
                            }
                        }
                        "height" => {
                            if let Ok(h) = val_str.parse::<u32>() {
                                config.window.height = h;
                            }
                        }
                        "resizable" => config.window.resizable = val_str == "true",
                        "transparent" => config.window.transparent = val_str == "true",
                        _ => {}
                    },
                    "web" => match key {
                        "target" => config.web.target = parse_string_val(val_str),
                        "app_js" | "app_js_name" => config.web.app_js_name = parse_string_val(val_str),
                        "styles_css" | "styles_css_name" => {
                            config.web.styles_css_name = parse_string_val(val_str);
                        }
                        _ => {}
                    },
                    _ => {}
                }
            }
        }

        if let Some(p) = current_plugin.take() {
            config.plugins.push(p);
        }

        Ok(config)
    }

    fn set_plugin_field(plugin: &mut PluginSpec, key: &str, val: &str) {
        match key {
            "name" => plugin.name = parse_string_val(val),
            "path" => plugin.path = Some(PathBuf::from(parse_string_val(val))),
            "crate" => plugin.crate_name = Some(parse_string_val(val)),
            "action" => plugin.action = Some(parse_string_val(val)),
            "methods" => plugin.methods = parse_string_array(val),
            "environments" | "environment" => {
                plugin.environments = parse_string_array(val);
            }
            _ => {}
        }
    }

    /// Serializes current configuration to TOML string.
    pub fn to_toml(&self) -> String {
        let mut out = String::new();
        out.push_str("[app]\n");
        out.push_str(&format!("name = \"{}\"\n", self.app.name));
        out.push_str(&format!("version = \"{}\"\n", self.app.version));
        out.push_str(&format!("template = \"{}\"\n", self.app.template));
        out.push_str(&format!("description = \"{}\"\n\n", self.app.description));

        out.push_str("[window]\n");
        out.push_str(&format!("title = \"{}\"\n", self.window.title));
        out.push_str(&format!("width = {}\n", self.window.width));
        out.push_str(&format!("height = {}\n", self.window.height));
        out.push_str(&format!("resizable = {}\n", self.window.resizable));
        out.push_str(&format!("transparent = {}\n\n", self.window.transparent));

        out.push_str("[web]\n");
        out.push_str(&format!("target = \"{}\"\n\n", self.web.target));

        for plugin in &self.plugins {
            out.push_str("[[plugins]]\n");
            out.push_str(&format!("name = \"{}\"\n", plugin.name));
            if let Some(ref path) = plugin.path {
                out.push_str(&format!("path = \"{}\"\n", path.display()));
            }
            if let Some(ref crt) = plugin.crate_name {
                out.push_str(&format!("crate = \"{}\"\n", crt));
            }
            if let Some(ref act) = plugin.action {
                out.push_str(&format!("action = \"{}\"\n", act));
            }
            if !plugin.methods.is_empty() {
                let m_str = plugin.methods.iter().map(|m| format!("\"{}\"", m)).collect::<Vec<_>>().join(", ");
                out.push_str(&format!("methods = [{}]\n", m_str));
            }
            if !plugin.environments.is_empty() {
                let e_str = plugin.environments.iter().map(|e| format!("\"{}\"", e)).collect::<Vec<_>>().join(", ");
                out.push_str(&format!("environments = [{}]\n", e_str));
            }
            out.push('\n');
        }

        for (env_name, env_cfg) in &self.environments {
            out.push_str(&format!("[environments.{}]\n", env_name));
            if let Some(ref name) = env_cfg.app_name {
                out.push_str(&format!("app.name = \"{}\"\n", name));
            }
            if let Some(ref ver) = env_cfg.app_version {
                out.push_str(&format!("app.version = \"{}\"\n", ver));
            }
            if let Some(ref title) = env_cfg.window_title {
                out.push_str(&format!("window.title = \"{}\"\n", title));
            }
            if !env_cfg.actions.is_empty() {
                let a_str = env_cfg.actions.iter().map(|a| format!("\"{}\"", a)).collect::<Vec<_>>().join(", ");
                out.push_str(&format!("actions = [{}]\n", a_str));
            }
            out.push('\n');
        }

        out
    }

    /// Serializes current configuration to YAML string.
    pub fn to_yaml(&self) -> String {
        let mut out = String::new();
        out.push_str("app:\n");
        out.push_str(&format!("  name: \"{}\"\n", self.app.name));
        out.push_str(&format!("  version: \"{}\"\n", self.app.version));
        out.push_str(&format!("  template: \"{}\"\n", self.app.template));
        out.push_str(&format!("  description: \"{}\"\n\n", self.app.description));

        out.push_str("window:\n");
        out.push_str(&format!("  title: \"{}\"\n", self.window.title));
        out.push_str(&format!("  width: {}\n", self.window.width));
        out.push_str(&format!("  height: {}\n", self.window.height));
        out.push_str(&format!("  resizable: {}\n", self.window.resizable));
        out.push_str(&format!("  transparent: {}\n\n", self.window.transparent));

        out.push_str("web:\n");
        out.push_str(&format!("  target: \"{}\"\n\n", self.web.target));

        if !self.plugins.is_empty() {
            out.push_str("plugins:\n");
            for plugin in &self.plugins {
                out.push_str(&format!("  - name: \"{}\"\n", plugin.name));
                if let Some(ref path) = plugin.path {
                    out.push_str(&format!("    path: \"{}\"\n", path.display()));
                }
                if let Some(ref crt) = plugin.crate_name {
                    out.push_str(&format!("    crate: \"{}\"\n", crt));
                }
                if let Some(ref act) = plugin.action {
                    out.push_str(&format!("    action: \"{}\"\n", act));
                }
                if !plugin.methods.is_empty() {
                    out.push_str("    methods:\n");
                    for m in &plugin.methods {
                        out.push_str(&format!("      - \"{}\"\n", m));
                    }
                }
                if !plugin.environments.is_empty() {
                    out.push_str("    environments:\n");
                    for e in &plugin.environments {
                        out.push_str(&format!("      - \"{}\"\n", e));
                    }
                }
            }
            out.push('\n');
        }

        if !self.environments.is_empty() {
            out.push_str("environments:\n");
            for (env_name, env_cfg) in &self.environments {
                out.push_str(&format!("  {}:\n", env_name));
                if let Some(ref name) = env_cfg.app_name {
                    out.push_str(&format!("    name: \"{}\"\n", name));
                }
                if let Some(ref ver) = env_cfg.app_version {
                    out.push_str(&format!("    version: \"{}\"\n", ver));
                }
                if let Some(ref title) = env_cfg.window_title {
                    out.push_str(&format!("    title: \"{}\"\n", title));
                }
                if !env_cfg.actions.is_empty() {
                    out.push_str("    actions:\n");
                    for a in &env_cfg.actions {
                        out.push_str(&format!("      - \"{}\"\n", a));
                    }
                }
            }
            out.push('\n');
        }

        out
    }
}

fn parse_string_val(s: &str) -> String {
    let trimmed = s.trim();
    if ((trimmed.starts_with('"') && trimmed.ends_with('"'))
        || (trimmed.starts_with('\'') && trimmed.ends_with('\'')))
        && trimmed.len() >= 2 {
            return trimmed[1..trimmed.len() - 1].to_string();
        }
    trimmed.to_string()
}

fn parse_string_array(s: &str) -> Vec<String> {
    let trimmed = s.trim();
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        let inside = &trimmed[1..trimmed.len() - 1];
        inside
            .split(',')
            .map(|item| parse_string_val(item.trim()))
            .filter(|item| !item.is_empty())
            .collect()
    } else if !trimmed.is_empty() {
        vec![parse_string_val(trimmed)]
    } else {
        Vec::new()
    }
}
