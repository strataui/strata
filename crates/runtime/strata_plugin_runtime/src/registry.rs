//! Dynamic extension points for custom RSX components, Tailwind utilities, and event hooks.

use std::collections::HashMap;

pub type ComponentRenderer = Box<dyn Fn(&HashMap<String, String>, Option<&str>) -> String + Send + Sync>;
pub type EventInterceptor = Box<dyn Fn(&str, &str) -> bool + Send + Sync>;

/// Central runtime registry of dynamic extensions provided by loaded plugins.
#[derive(Default)]
pub struct DynamicExtensionRegistry {
    components: HashMap<String, ComponentRenderer>,
    utilities: HashMap<String, String>,
    interceptors: Vec<EventInterceptor>,
}

impl DynamicExtensionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a dynamic RSX component tag renderer.
    pub fn register_component<F>(&mut self, tag: impl Into<String>, renderer: F)
    where
        F: Fn(&HashMap<String, String>, Option<&str>) -> String + Send + Sync + 'static,
    {
        self.components.insert(tag.into().to_ascii_lowercase(), Box::new(renderer));
    }

    pub fn has_component(&self, tag: &str) -> bool {
        self.components.contains_key(&tag.to_ascii_lowercase())
    }

    pub fn render_component(&self, tag: &str, props: &HashMap<String, String>, children_text: Option<&str>) -> Option<String> {
        let renderer = self.components.get(&tag.to_ascii_lowercase())?;
        Some(renderer(props, children_text))
    }

    /// Registers a custom Tailwind/CSS utility class mapping (e.g. "glow-cyan" -> "box-shadow: 0 0 15px #06b6d4;").
    pub fn register_utility(&mut self, class_name: impl Into<String>, css_declaration: impl Into<String>) {
        self.utilities.insert(class_name.into(), css_declaration.into());
    }

    pub fn resolve_utility(&self, class_name: &str) -> Option<&str> {
        self.utilities.get(class_name).map(|s| s.as_str())
    }

    /// Registers a synthetic event interceptor. If any interceptor returns `true`, the event is consumed/canceled.
    pub fn register_event_interceptor<F>(&mut self, interceptor: F)
    where
        F: Fn(&str, &str) -> bool + Send + Sync + 'static,
    {
        self.interceptors.push(Box::new(interceptor));
    }

    pub fn intercept_event(&self, event_type: &str, target_id: &str) -> bool {
        for interceptor in &self.interceptors {
            if interceptor(event_type, target_id) {
                return true; // Consumed
            }
        }
        false
    }
}
