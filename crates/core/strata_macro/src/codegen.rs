//! Code generator lowering RSX AST into pure Rust View and NodeBlueprint expressions.

use crate::ast::*;

/// Lower an RSX AST into Rust tokens, collecting every invalid attribute as a
/// diagnostic instead of silently dropping it.
pub fn generate_rsx(nodes: &[RsxNode]) -> Result<String, Vec<String>> {
    let mut errors = Vec::new();
    let code = generate_rsx_inner(nodes, &mut errors);
    if errors.is_empty() {
        Ok(code)
    } else {
        Err(errors)
    }
}

fn generate_rsx_inner(nodes: &[RsxNode], errors: &mut Vec<String>) -> String {
    if nodes.is_empty() {
        return "strata_dom::View::new(strata_dom::NodeBlueprint::Empty)".to_string();
    }

    if nodes.len() == 1 {
        let node_code = lower_node(&nodes[0], errors);
        format!("strata_dom::View::new({})", node_code)
    } else {
        let mut children_code = Vec::new();
        for node in nodes {
            children_code.push(lower_node(node, errors));
        }
        format!(
            "strata_dom::View::new(strata_dom::NodeBlueprint::fragment(vec![{}]))",
            children_code.join(", ")
        )
    }
}

fn lower_node(node: &RsxNode, errors: &mut Vec<String>) -> String {
    match node {
        RsxNode::Text(txt) => {
            format!("strata_dom::NodeBlueprint::text({:?})", txt)
        }
        RsxNode::Expression(stream) => {
            format!(
                "strata_dom::IntoNodeBlueprint::into_node_blueprint({{ {} }})",
                stream
            )
        }
        RsxNode::Fragment(children) => {
            let mut children_code = Vec::new();
            for child in children {
                children_code.push(lower_node(child, errors));
            }
            format!(
                "strata_dom::NodeBlueprint::fragment(vec![{}])",
                children_code.join(", ")
            )
        }
        RsxNode::Element(el) => lower_element(el, errors),
        RsxNode::Component(comp) => lower_component(comp, errors),
    }
}

/// Normalize an event attribute so `onclick`, `onClick`, and `on_click` all
/// resolve to the same `EventType`.
fn normalize_event_name(name: &str) -> String {
    name.to_ascii_lowercase().replace('_', "")
}

fn lower_element(el: &RsxElement, errors: &mut Vec<String>) -> String {
    let tag_code = match el.tag.to_ascii_lowercase().as_str() {
        "div" => "strata_dom::Tag::Div".to_string(),
        "span" => "strata_dom::Tag::Span".to_string(),
        "button" => "strata_dom::Tag::Button".to_string(),
        "input" => "strata_dom::Tag::Input".to_string(),
        "select" => "strata_dom::Tag::Select".to_string(),
        "option" => "strata_dom::Tag::Option".to_string(),
        "textarea" => "strata_dom::Tag::Textarea".to_string(),
        "label" => "strata_dom::Tag::Label".to_string(),
        "form" => "strata_dom::Tag::Form".to_string(),
        "header" => "strata_dom::Tag::Header".to_string(),
        "nav" => "strata_dom::Tag::Nav".to_string(),
        "main" => "strata_dom::Tag::Main".to_string(),
        "section" => "strata_dom::Tag::Section".to_string(),
        "aside" => "strata_dom::Tag::Aside".to_string(),
        "footer" => "strata_dom::Tag::Footer".to_string(),
        "h1" => "strata_dom::Tag::H1".to_string(),
        "h2" => "strata_dom::Tag::H2".to_string(),
        "h3" => "strata_dom::Tag::H3".to_string(),
        "h4" => "strata_dom::Tag::H4".to_string(),
        "h5" => "strata_dom::Tag::H5".to_string(),
        "h6" => "strata_dom::Tag::H6".to_string(),
        "p" => "strata_dom::Tag::P".to_string(),
        "ul" => "strata_dom::Tag::Ul".to_string(),
        "ol" => "strata_dom::Tag::Ol".to_string(),
        "li" => "strata_dom::Tag::Li".to_string(),
        "a" => "strata_dom::Tag::A".to_string(),
        "img" => "strata_dom::Tag::Img".to_string(),
        "svg" => "strata_dom::Tag::Svg".to_string(),
        "path" => "strata_dom::Tag::Path".to_string(),
        "canvas" => "strata_dom::Tag::Canvas".to_string(),
        "table" => "strata_dom::Tag::Table".to_string(),
        "thead" => "strata_dom::Tag::Thead".to_string(),
        "tbody" => "strata_dom::Tag::Tbody".to_string(),
        "tr" => "strata_dom::Tag::Tr".to_string(),
        "th" => "strata_dom::Tag::Th".to_string(),
        "td" => "strata_dom::Tag::Td".to_string(),
        "hr" => "strata_dom::Tag::Hr".to_string(),
        "br" => "strata_dom::Tag::Br".to_string(),
        _ => format!("strata_dom::Tag::Custom({:?})", el.tag),
    };

    // Extract classes with compile-time deduplication and 256-bit SIMD bitmask resolution
    let mut classes_code = "strata_dom::ClassList::Empty".to_string();
    let mut other_attrs = Vec::new();

    for attr in &el.attributes {
        if attr.name == "class" || attr.name == "className" {
            match &attr.value {
                RsxAttrValue::Literal(s) => {
                    classes_code = crate::classes::emit_static_class_list(s);
                }
                RsxAttrValue::Expression(stream) => {
                    classes_code = format!(
                        "strata_dom::ClassList::Dynamic({{ let _s = ({}).to_string(); _s.split_whitespace().map(|c| c.to_string()).collect::<Vec<String>>() }})",
                        stream
                    );
                }
                RsxAttrValue::Flag => {}
            }
        } else {
            other_attrs.push(attr);
        }
    }

    // Lower attributes and event listeners
    let mut attrs_code = Vec::new();
    for attr in other_attrs {
        if attr.name.starts_with("on") {
            let event_name = normalize_event_name(&attr.name);
            let event_type = match event_name.as_str() {
                "onclick" => "strata_dom::EventType::Click".to_string(),
                "ondblclick" => "strata_dom::EventType::DblClick".to_string(),
                "oncontextmenu" => "strata_dom::EventType::ContextMenu".to_string(),
                "onauxclick" => "strata_dom::EventType::AuxClick".to_string(),
                "onmousedown" => "strata_dom::EventType::MouseDown".to_string(),
                "onmouseup" => "strata_dom::EventType::MouseUp".to_string(),
                "onmousemove" => "strata_dom::EventType::MouseMove".to_string(),
                "onmouseenter" => "strata_dom::EventType::MouseEnter".to_string(),
                "onmouseleave" => "strata_dom::EventType::MouseLeave".to_string(),
                "onmouseover" => "strata_dom::EventType::MouseOver".to_string(),
                "onmouseout" => "strata_dom::EventType::MouseOut".to_string(),
                "oninput" => "strata_dom::EventType::Input".to_string(),
                "onchange" => "strata_dom::EventType::Change".to_string(),
                "onsubmit" => "strata_dom::EventType::Submit".to_string(),
                "onkeydown" => "strata_dom::EventType::KeyDown".to_string(),
                "onkeyup" => "strata_dom::EventType::KeyUp".to_string(),
                "onpointerdown" => "strata_dom::EventType::PointerDown".to_string(),
                "onpointerup" => "strata_dom::EventType::PointerUp".to_string(),
                "onpointermove" => "strata_dom::EventType::PointerMove".to_string(),
                "onfocus" => "strata_dom::EventType::Focus".to_string(),
                "onblur" => "strata_dom::EventType::Blur".to_string(),
                "onwheel" => "strata_dom::EventType::Wheel".to_string(),
                "ontransitionrun" => "strata_dom::EventType::TransitionRun".to_string(),
                "ontransitionstart" => "strata_dom::EventType::TransitionStart".to_string(),
                "ontransitionend" => "strata_dom::EventType::TransitionEnd".to_string(),
                "ontransitioncancel" => "strata_dom::EventType::TransitionCancel".to_string(),
                "onanimationstart" => "strata_dom::EventType::AnimationStart".to_string(),
                "onanimationiteration" => "strata_dom::EventType::AnimationIteration".to_string(),
                "onanimationend" => "strata_dom::EventType::AnimationEnd".to_string(),
                "onanimationcancel" => "strata_dom::EventType::AnimationCancel".to_string(),
                _ => {
                    errors.push(format!(
                        "unsupported event attribute `{}`; expected a known DOM event such as `onclick` or `on_click`",
                        attr.name
                    ));
                    continue;
                }
            };

            match &attr.value {
                RsxAttrValue::Expression(stream) => {
                    let method = if event_name == "oninput"
                        || event_name == "onchange"
                        || event_name == "onsubmit"
                    {
                        "value_listener"
                    } else {
                        "event_listener"
                    };
                    attrs_code.push(format!(
                        "strata_dom::Attribute::{}({}, {})",
                        method, event_type, stream
                    ));
                }
                RsxAttrValue::Literal(_) | RsxAttrValue::Flag => {
                    errors.push(format!(
                        "event attribute `{}` requires a callable expression, e.g. `{0}={{|_event| {{}}}}`; a literal string or flag is not a handler",
                        attr.name
                    ));
                }
            }
        } else {
            match &attr.value {
                RsxAttrValue::Literal(s) => {
                    attrs_code.push(format!(
                        "strata_dom::Attribute::property({:?}, {:?})",
                        attr.name, s
                    ));
                }
                RsxAttrValue::Expression(stream) => {
                    attrs_code.push(format!(
                        "strata_dom::Attribute::property({:?}, ({}).to_string())",
                        attr.name, stream
                    ));
                }
                RsxAttrValue::Flag => {
                    attrs_code.push(format!(
                        "strata_dom::Attribute::property({:?}, \"true\")",
                        attr.name
                    ));
                }
            }
        }
    }

    // Lower children
    let mut children_code = Vec::new();
    for child in &el.children {
        children_code.push(lower_node(child, errors));
    }

    format!(
        "strata_dom::NodeBlueprint::element({}, {}, vec![{}], vec![{}])",
        tag_code,
        classes_code,
        attrs_code.join(", "),
        children_code.join(", ")
    )
}

fn lower_component(comp: &RsxComponent, errors: &mut Vec<String>) -> String {
    let full_path = comp.full_path();
    let last_is_upper = comp
        .path
        .last()
        .is_some_and(|seg| segment_starts_uppercase(seg));
    let penultimate_is_upper =
        comp.path.len() >= 2 && segment_starts_uppercase(&comp.path[comp.path.len() - 2]);

    // Case A1: Associated function on a PascalCase type namespace (e.g. `Icons::Plus` or
    // `strata_icons::Icons::Plus`). In Rust, structs/enums cannot contain nested companion
    // builder structs (`Icons::Plus::builder()` is impossible), so type-associated tags invoke
    // the associated function directly.
    if penultimate_is_upper {
        if comp.props.is_empty() && comp.children.is_empty() {
            return format!(
                "strata_dom::IntoNodeBlueprint::into_node_blueprint({}(None))",
                full_path
            );
        }
        return lower_positional_call(&full_path, comp, errors);
    }

    // Case A2: Function-component call (final path segment starts with a lowercase character,
    // e.g. `<my_mod::render_badge label="ok" />` or `<::helpers::divider />`).
    if !last_is_upper {
        return lower_positional_call(&full_path, comp, errors);
    }

    // Case B: PascalCase component (bare `<Portal>` or path-qualified `<strata_dom::Portal>`,
    // `<::strata_dom::Portal>`, `<crate::ui::Modal>`, `<self::Card>`, `<super::Dialog>`)
    // with 0 props and 0 children: zero-overhead direct call.
    if comp.props.is_empty() && comp.children.is_empty() {
        return format!(
            "strata_dom::IntoNodeBlueprint::into_node_blueprint({}())",
            full_path
        );
    }

    // Case C: PascalCase component with props and/or children: uses companion builder
    // (`#full_path::builder()...build()`).
    let mut builder_calls = Vec::new();

    for prop in &comp.props {
        // Convert hyphenated props e.g. foo-bar to snake_case foo_bar
        let prop_name = prop.name.replace('-', "_");
        match &prop.value {
            RsxAttrValue::Literal(s) => {
                builder_calls.push(format!(".{}({:?})", prop_name, s));
            }
            RsxAttrValue::Expression(stream) => {
                builder_calls.push(format!(".{}({})", prop_name, stream));
            }
            RsxAttrValue::Flag => {
                builder_calls.push(format!(".{}(true)", prop_name));
            }
        }
    }

    if !comp.children.is_empty() {
        let mut children_code = Vec::new();
        for child in &comp.children {
            children_code.push(lower_node(child, errors));
        }
        builder_calls.push(format!(".children(vec![{}])", children_code.join(", ")));
    }

    format!(
        "strata_dom::IntoNodeBlueprint::into_node_blueprint({}::builder(){}.build())",
        full_path,
        builder_calls.join("")
    )
}

fn lower_positional_call(
    full_path: &str,
    comp: &RsxComponent,
    errors: &mut Vec<String>,
) -> String {
    let mut arg_exprs = Vec::new();
    for prop in &comp.props {
        match &prop.value {
            RsxAttrValue::Literal(s) => arg_exprs.push(format!("{:?}", s)),
            RsxAttrValue::Expression(stream) => arg_exprs.push(format!("{}", stream)),
            RsxAttrValue::Flag => arg_exprs.push("true".to_string()),
        }
    }
    if !comp.children.is_empty() {
        let mut children_code = Vec::new();
        for child in &comp.children {
            children_code.push(lower_node(child, errors));
        }
        arg_exprs.push(format!("vec![{}]", children_code.join(", ")));
    }
    format!(
        "strata_dom::IntoNodeBlueprint::into_node_blueprint({}({}))",
        full_path,
        arg_exprs.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn button_with(name: &str, value: RsxAttrValue) -> Vec<RsxNode> {
        vec![RsxNode::Element(RsxElement {
            tag: "button".to_string(),
            attributes: vec![RsxAttribute {
                name: name.to_string(),
                value,
            }],
            children: vec![RsxNode::Text("ok".to_string())],
            self_closing: false,
        })]
    }

    #[test]
    fn unknown_event_attribute_is_rejected() {
        let nodes = button_with("on_clicky", RsxAttrValue::Literal("f".to_string()));
        let errors = generate_rsx(&nodes).expect_err("unknown event must be rejected");
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("unsupported event attribute `on_clicky`"));
    }

    #[test]
    fn literal_event_handler_is_rejected() {
        let nodes = button_with("on_click", RsxAttrValue::Literal("f".to_string()));
        let errors = generate_rsx(&nodes).expect_err("literal handler must be rejected");
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("requires a callable expression"));
    }

    #[test]
    fn flag_event_handler_is_rejected() {
        let nodes = button_with("onclick", RsxAttrValue::Flag);
        let errors = generate_rsx(&nodes).expect_err("flag handler must be rejected");
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("requires a callable expression"));
    }

    #[test]
    fn path_qualified_pascal_case_component_uses_builder() {
        let nodes = vec![RsxNode::Component(RsxComponent {
            leading_colon: false,
            path: vec!["strata_dom".to_string(), "Portal".to_string()],
            props: vec![RsxAttribute {
                name: "anchor".to_string(),
                value: RsxAttrValue::Literal("trigger-btn".to_string()),
            }],
            children: vec![RsxNode::Text("Menu".to_string())],
            self_closing: false,
        })];
        let code = generate_rsx(&nodes).expect("codegen must succeed");
        assert!(
            code.contains("strata_dom::Portal::builder().anchor(\"trigger-btn\").children(vec!["),
            "unexpected codegen: {code}"
        );
    }

    #[test]
    fn path_qualified_pascal_case_component_with_only_children_uses_builder() {
        let nodes = vec![RsxNode::Component(RsxComponent {
            leading_colon: false,
            path: vec!["strata_dom".to_string(), "Portal".to_string()],
            props: vec![],
            children: vec![RsxNode::Text("Top layer".to_string())],
            self_closing: false,
        })];
        let code = generate_rsx(&nodes).expect("codegen must succeed");
        assert!(
            code.contains("strata_dom::Portal::builder().children(vec!["),
            "unexpected codegen: {code}"
        );
    }

    #[test]
    fn leading_colon_and_empty_path_qualified_component_codegen() {
        let empty_nodes = vec![RsxNode::Component(RsxComponent {
            leading_colon: false,
            path: vec!["strata_dom".to_string(), "Portal".to_string()],
            props: vec![],
            children: vec![],
            self_closing: true,
        })];
        let empty_code = generate_rsx(&empty_nodes).expect("empty component codegen must succeed");
        assert!(
            empty_code.contains(
                "strata_dom::IntoNodeBlueprint::into_node_blueprint(strata_dom::Portal())"
            ),
            "unexpected empty codegen: {empty_code}"
        );

        let root_nodes = vec![RsxNode::Component(RsxComponent {
            leading_colon: true,
            path: vec!["strata_dom".to_string(), "Portal".to_string()],
            props: vec![],
            children: vec![RsxNode::Text("Global".to_string())],
            self_closing: false,
        })];
        let root_code = generate_rsx(&root_nodes).expect("leading-colon codegen must succeed");
        assert!(
            root_code.contains("::strata_dom::Portal::builder().children(vec!["),
            "unexpected leading-colon codegen: {root_code}"
        );
    }

    #[test]
    fn type_associated_and_lowercase_function_paths_use_call_syntax() {
        let icon_nodes = vec![RsxNode::Component(RsxComponent {
            leading_colon: false,
            path: vec![
                "strata_icons".to_string(),
                "Icons".to_string(),
                "Plus".to_string(),
            ],
            props: vec![],
            children: vec![],
            self_closing: true,
        })];
        let icon_code = generate_rsx(&icon_nodes).expect("icon codegen must succeed");
        assert!(
            icon_code.contains("strata_icons::Icons::Plus(None)"),
            "unexpected icon codegen: {icon_code}"
        );

        let fn_nodes = vec![RsxNode::Component(RsxComponent {
            leading_colon: false,
            path: vec!["helpers".to_string(), "render_badge".to_string()],
            props: vec![RsxAttribute {
                name: "label".to_string(),
                value: RsxAttrValue::Literal("v1".to_string()),
            }],
            children: vec![],
            self_closing: true,
        })];
        let fn_code = generate_rsx(&fn_nodes).expect("fn codegen must succeed");
        assert!(
            fn_code.contains("helpers::render_badge(\"v1\")"),
            "unexpected fn codegen: {fn_code}"
        );
    }
}
