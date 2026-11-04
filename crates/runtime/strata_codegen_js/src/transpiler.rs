//! Pure JavaScript ESM Transpiler for Strata RSX & NodeBlueprint trees.

use strata_dom::{Attribute, EventType, NodeBlueprint, Tag};

/// Transpiles Strata NodeBlueprint hierarchies into vanilla JavaScript DOM creation calls.
pub struct JsTranspiler;

impl JsTranspiler {
    /// Transpiles a NodeBlueprint into an ESM component function.
    pub fn emit_esm_component(
        component_name: &str,
        blueprint: &NodeBlueprint,
        runtime_module: &str,
    ) -> String {
        let mut js = String::new();
        js.push_str(&format!(
            "import {{ el, txt, frag, on, bindText, bindAttr, bindClass, mount }} from '{}';\n\n",
            runtime_module
        ));
        js.push_str(&format!("export function {}() {{\n  return ", component_name));
        Self::render_blueprint(blueprint, &mut js, 1);
        js.push_str(";\n}\n");
        js
    }

    /// Transpiles a NodeBlueprint into a complete standalone ESM application module.
    pub fn emit_standalone_app(
        blueprint: &NodeBlueprint,
        root_element_id: &str,
        runtime_module: &str,
    ) -> String {
        let mut js = String::new();
        js.push_str(&format!(
            "import {{ el, txt, frag, on, bindText, bindAttr, bindClass, mount }} from '{}';\n\n",
            runtime_module
        ));
        js.push_str("export function App() {\n  return ");
        Self::render_blueprint(blueprint, &mut js, 1);
        js.push_str(";\n}\n\n");
        js.push_str(&format!(
            r#"if (typeof document !== 'undefined') {{
  const root = document.getElementById('{}');
  if (root) {{
    mount(root, App);
  }}
}}
"#,
            root_element_id
        ));
        js
    }

    /// Recursively formats a NodeBlueprint into `el(...)`, `txt(...)`, or `frag(...)` JS calls.
    pub fn render_blueprint(blueprint: &NodeBlueprint, out: &mut String, indent: usize) {
        match blueprint {
            NodeBlueprint::Element {
                tag,
                classes,
                attributes,
                children,
            } => {
                let tag_name = Self::tag_name(*tag);
                let class_str = classes.join(" ");

                out.push_str(&format!("el('{}', '{}', {{", tag_name, class_str));

                // Attributes
                let mut first_attr = true;
                for attr in attributes {
                    match attr {
                        Attribute::Property { key, value } => {
                            if !first_attr {
                                out.push_str(", ");
                            }
                            out.push_str(&format!("'{}': '{}'", Self::escape_str(key), Self::escape_str(value)));
                            first_attr = false;
                        }
                        Attribute::EventListener { event, .. } => {
                            if !first_attr {
                                out.push_str(", ");
                            }
                            let evt_name = Self::event_name(*event);
                            out.push_str(&format!("'on{}': (e) => {{ /* event handler */ }}", evt_name));
                            first_attr = false;
                        }
                        Attribute::DirectStyle(_) => {}
                    }
                }

                out.push_str("}, [");

                if !children.is_empty() {
                    out.push('\n');
                    for (i, child) in children.iter().enumerate() {
                        Self::write_indent(out, indent + 1);
                        Self::render_blueprint(child, out, indent + 1);
                        if i + 1 < children.len() {
                            out.push(',');
                        }
                        out.push('\n');
                    }
                    Self::write_indent(out, indent);
                }

                out.push_str("])");
            }
            NodeBlueprint::Text(text) => {
                out.push_str(&format!("txt('{}')", Self::escape_str(text)));
            }
            NodeBlueprint::DynamicText(_) => {
                out.push_str("(() => { const t = txt(''); bindText(t, () => ''); return t; })()");
            }
            NodeBlueprint::Fragment(children) => {
                out.push_str("frag([");
                if !children.is_empty() {
                    out.push('\n');
                    for (i, child) in children.iter().enumerate() {
                        Self::write_indent(out, indent + 1);
                        Self::render_blueprint(child, out, indent + 1);
                        if i + 1 < children.len() {
                            out.push(',');
                        }
                        out.push('\n');
                    }
                    Self::write_indent(out, indent);
                }
                out.push_str("])");
            }
            NodeBlueprint::Empty => {
                out.push_str("null");
            }
        }
    }

    fn write_indent(out: &mut String, indent: usize) {
        for _ in 0..indent {
            out.push_str("  ");
        }
    }

    fn escape_str(s: &str) -> String {
        s.replace('\\', "\\\\")
            .replace('\'', "\\'")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
    }

    pub fn tag_name(tag: Tag) -> &'static str {
        match tag {
            Tag::Div => "div",
            Tag::Span => "span",
            Tag::Button => "button",
            Tag::Input => "input",
            Tag::Select => "select",
            Tag::Option => "option",
            Tag::Textarea => "textarea",
            Tag::Label => "label",
            Tag::Form => "form",
            Tag::Header => "header",
            Tag::Nav => "nav",
            Tag::Main => "main",
            Tag::Section => "section",
            Tag::Aside => "aside",
            Tag::Footer => "footer",
            Tag::H1 => "h1",
            Tag::H2 => "h2",
            Tag::H3 => "h3",
            Tag::H4 => "h4",
            Tag::H5 => "h5",
            Tag::H6 => "h6",
            Tag::P => "p",
            Tag::Ul => "ul",
            Tag::Ol => "ol",
            Tag::Li => "li",
            Tag::A => "a",
            Tag::Img => "img",
            Tag::Svg => "svg",
            Tag::Path => "path",
            Tag::Canvas => "canvas",
            Tag::Table => "table",
            Tag::Thead => "thead",
            Tag::Tbody => "tbody",
            Tag::Tr => "tr",
            Tag::Th => "th",
            Tag::Td => "td",
            Tag::Hr => "hr",
            Tag::Br => "br",
            Tag::Custom(custom) => custom,
        }
    }

    pub fn event_name(event: EventType) -> &'static str {
        match event {
            EventType::Click => "click",
            EventType::DblClick => "dblclick",
            EventType::ContextMenu => "contextmenu",
            EventType::AuxClick => "auxclick",
            EventType::MouseDown => "mousedown",
            EventType::MouseUp => "mouseup",
            EventType::MouseMove => "mousemove",
            EventType::MouseEnter => "mouseenter",
            EventType::MouseLeave => "mouseleave",
            EventType::MouseOver => "mouseover",
            EventType::MouseOut => "mouseout",
            EventType::Input => "input",
            EventType::Change => "change",
            EventType::Submit => "submit",
            EventType::KeyDown => "keydown",
            EventType::KeyUp => "keyup",
            EventType::PointerDown => "pointerdown",
            EventType::PointerUp => "pointerup",
            EventType::PointerMove => "pointermove",
            EventType::Focus => "focus",
            EventType::Blur => "blur",
            EventType::Wheel => "wheel",
            EventType::TransitionRun => "transitionrun",
            EventType::TransitionStart => "transitionstart",
            EventType::TransitionEnd => "transitionend",
            EventType::TransitionCancel => "transitioncancel",
            EventType::AnimationStart => "animationstart",
            EventType::AnimationIteration => "animationiteration",
            EventType::AnimationEnd => "animationend",
            EventType::AnimationCancel => "animationcancel",
            EventType::Custom(_) => "custom",
        }
    }
}
