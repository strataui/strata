//! Code generator lowering RSX AST into pure Rust View and NodeBlueprint expressions.

use crate::ast::*;

pub fn generate_rsx(nodes: &[RsxNode]) -> String {
    if nodes.is_empty() {
        return "strata_dom::View::new(strata_dom::NodeBlueprint::Empty)".to_string();
    }

    if nodes.len() == 1 {
        let node_code = lower_node(&nodes[0]);
        format!("strata_dom::View::new({})", node_code)
    } else {
        let mut children_code = Vec::new();
        for node in nodes {
            children_code.push(lower_node(node));
        }
        format!(
            "strata_dom::View::new(strata_dom::NodeBlueprint::fragment(vec![{}]))",
            children_code.join(", ")
        )
    }
}

fn lower_node(node: &RsxNode) -> String {
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
                children_code.push(lower_node(child));
            }
            format!(
                "strata_dom::NodeBlueprint::fragment(vec![{}])",
                children_code.join(", ")
            )
        }
        RsxNode::Element(el) => lower_element(el),
        RsxNode::Component(comp) => lower_component(comp),
    }
}

fn lower_element(el: &RsxElement) -> String {
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

    // Extract classes
    let mut classes_code = "Vec::new()".to_string();
    let mut other_attrs = Vec::new();

    for attr in &el.attributes {
        if attr.name == "class" || attr.name == "className" {
            match &attr.value {
                RsxAttrValue::Literal(s) => {
                    let parts: Vec<String> = s
                        .split_whitespace()
                        .map(|c| format!("{:?}.to_string()", c))
                        .collect();
                    classes_code = format!("vec![{}]", parts.join(", "));
                }
                RsxAttrValue::Expression(stream) => {
                    classes_code = format!(
                        "({{ let _s = ({}).to_string(); _s.split_whitespace().map(|c| c.to_string()).collect::<Vec<String>>() }})",
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
            let event_type = match attr.name.to_ascii_lowercase().as_str() {
                "onclick" => "strata_dom::EventType::Click".to_string(),
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
                _ => format!("strata_dom::EventType::Custom({:?})", attr.name),
            };

            match &attr.value {
                RsxAttrValue::Expression(stream) => {
                    let method = if attr.name == "oninput" || attr.name == "onchange" {
                        "value_listener"
                    } else {
                        "event_listener"
                    };
                    attrs_code.push(format!(
                        "strata_dom::Attribute::{}({}, {})",
                        method, event_type, stream
                    ));
                }
                _ => {}
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
        children_code.push(lower_node(child));
    }

    format!(
        "strata_dom::NodeBlueprint::element({}, {}, vec![{}], vec![{}])",
        tag_code,
        classes_code,
        attrs_code.join(", "),
        children_code.join(", ")
    )
}

fn lower_component(comp: &RsxComponent) -> String {
    let full_path = comp.path.join("::");

    // Case A: Namespaced path (e.g. `Icons::Plus`)
    if comp.path.len() > 1 {
        if comp.props.is_empty() {
            return format!(
                "strata_dom::IntoNodeBlueprint::into_node_blueprint({}(None))",
                full_path
            );
        } else {
            let mut arg_exprs = Vec::new();
            for prop in &comp.props {
                match &prop.value {
                    RsxAttrValue::Literal(s) => arg_exprs.push(format!("{:?}", s)),
                    RsxAttrValue::Expression(stream) => arg_exprs.push(format!("{}", stream)),
                    RsxAttrValue::Flag => arg_exprs.push("true".to_string()),
                }
            }
            return format!(
                "strata_dom::IntoNodeBlueprint::into_node_blueprint({}({}))",
                full_path,
                arg_exprs.join(", ")
            );
        }
    }

    // Case B: Component with 0 props and 0 children: zero-overhead direct function call
    if comp.props.is_empty() && comp.children.is_empty() {
        return format!(
            "strata_dom::IntoNodeBlueprint::into_node_blueprint({}())",
            full_path
        );
    }

    // Case C: Component with props and/or children: uses companion builder
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
            children_code.push(lower_node(child));
        }
        builder_calls.push(format!(".children(vec![{}])", children_code.join(", ")));
    }

    format!(
        "strata_dom::IntoNodeBlueprint::into_node_blueprint({}::builder(){}.build())",
        full_path,
        builder_calls.join("")
    )
}
