//! AccessKit Native Screen Reader Engine Integration & Platform Accessibility Adapter.
//!
//! Provides genuine AccessKit integration with a platform accessibility adapter that maps Strata
//! DOM and layout nodes to AccessKit `Node` structures, generates platform accessibility trees
//! (Windows UI Automation, macOS NSAccessibility, Linux AT-SPI, Web ARIA), handles accessibility
//! actions (focus, click, scroll, text selection), and sends real-time tree updates on DOM mutations.

use std::sync::{Arc, Mutex};

use crate::node::{A11yAction, A11yLivePoliteness, A11yNode, A11yNodeId, A11yTree};
use crate::role::A11yRole;
use crate::update::A11yTreeUpdate;

/// Converts a Strata `A11yRole` into its canonical platform `accesskit::Role`.
pub fn a11y_role_to_accesskit(role: A11yRole) -> accesskit::Role {
    match role {
        A11yRole::Window => accesskit::Role::Window,
        A11yRole::RootWebArea => accesskit::Role::RootWebArea,
        A11yRole::Button => accesskit::Role::Button,
        A11yRole::Checkbox => accesskit::Role::CheckBox,
        A11yRole::Radio | A11yRole::RadioButton => accesskit::Role::RadioButton,
        A11yRole::Slider => accesskit::Role::Slider,
        A11yRole::Heading => accesskit::Role::Heading,
        A11yRole::Dialog => accesskit::Role::Dialog,
        A11yRole::Alert => accesskit::Role::Alert,
        A11yRole::AlertDialog => accesskit::Role::AlertDialog,
        A11yRole::TextInput => accesskit::Role::TextInput,
        A11yRole::List => accesskit::Role::List,
        A11yRole::ListItem => accesskit::Role::ListItem,
        A11yRole::Grid => accesskit::Role::Grid,
        A11yRole::Cell => accesskit::Role::Cell,
        A11yRole::Row => accesskit::Role::Row,
        A11yRole::Image => accesskit::Role::Image,
        A11yRole::Link => accesskit::Role::Link,
        A11yRole::Navigation => accesskit::Role::Navigation,
        A11yRole::Banner => accesskit::Role::Banner,
        A11yRole::Main => accesskit::Role::Main,
        A11yRole::Complementary => accesskit::Role::Complementary,
        A11yRole::ContentInfo => accesskit::Role::ContentInfo,
        A11yRole::Form => accesskit::Role::Form,
        A11yRole::Toolbar => accesskit::Role::Toolbar,
        A11yRole::Tab => accesskit::Role::Tab,
        A11yRole::TabList => accesskit::Role::TabList,
        A11yRole::TabPanel => accesskit::Role::TabPanel,
        A11yRole::Switch => accesskit::Role::Switch,
        A11yRole::Status => accesskit::Role::Status,
        A11yRole::ProgressBar => accesskit::Role::ProgressIndicator,
        A11yRole::Menu => accesskit::Role::Menu,
        A11yRole::MenuItem => accesskit::Role::MenuItem,
        A11yRole::Tree => accesskit::Role::Tree,
        A11yRole::TreeItem => accesskit::Role::TreeItem,
        A11yRole::Unknown => accesskit::Role::GenericContainer,
    }
}

/// Converts a Strata `A11yAction` into its canonical `accesskit::Action`.
pub fn a11y_action_to_accesskit(action: &A11yAction) -> accesskit::Action {
    match action {
        A11yAction::Click => accesskit::Action::Click,
        A11yAction::Focus => accesskit::Action::Focus,
        A11yAction::Blur => accesskit::Action::Blur,
        A11yAction::SetValue(_) => accesskit::Action::SetValue,
        A11yAction::Increment => accesskit::Action::Increment,
        A11yAction::Decrement => accesskit::Action::Decrement,
        A11yAction::ScrollUp => accesskit::Action::ScrollUp,
        A11yAction::ScrollDown => accesskit::Action::ScrollDown,
    }
}

/// Translates an incoming `accesskit::ActionRequest` from platform assistive technologies into a Strata `A11yAction`.
pub fn accesskit_action_to_a11y(
    action: accesskit::Action,
    data: Option<&accesskit::ActionData>,
) -> Option<A11yAction> {
    match action {
        accesskit::Action::Click => Some(A11yAction::Click),
        accesskit::Action::Focus => Some(A11yAction::Focus),
        accesskit::Action::Blur => Some(A11yAction::Blur),
        accesskit::Action::SetValue => {
            let val = data
                .and_then(|d| match d {
                    accesskit::ActionData::Value(v) => Some(v.to_string()),
                    accesskit::ActionData::NumericValue(n) => Some(n.to_string()),
                    _ => None,
                })
                .unwrap_or_default();
            Some(A11yAction::SetValue(val))
        }
        accesskit::Action::Increment => Some(A11yAction::Increment),
        accesskit::Action::Decrement => Some(A11yAction::Decrement),
        accesskit::Action::ScrollUp => Some(A11yAction::ScrollUp),
        accesskit::Action::ScrollDown => Some(A11yAction::ScrollDown),
        _ => None,
    }
}

/// Maps a Strata `A11yNode` into a real `accesskit::Node`.
pub fn a11y_node_to_accesskit(node: &A11yNode) -> accesskit::Node {
    let ak_role = a11y_role_to_accesskit(node.role);
    let mut ak_node = accesskit::Node::new(ak_role);

    if let Some(b) = node.bounds {
        ak_node.set_bounds(accesskit::Rect {
            x0: b.x as f64,
            y0: b.y as f64,
            x1: (b.x + b.width) as f64,
            y1: (b.y + b.height) as f64,
        });
    }

    if !node.children.is_empty() {
        let child_ids: Vec<accesskit::NodeId> =
            node.children.iter().map(|c| accesskit::NodeId(c.0)).collect();
        ak_node.set_children(child_ids);
    }

    if let Some(ref name) = node.name {
        ak_node.set_label(name.clone());
    }

    if let Some(ref desc) = node.description {
        ak_node.set_description(desc.clone());
    }

    if let Some(ref val) = node.value {
        ak_node.set_value(val.clone());
    }

    if let Some(num) = node.numeric_value {
        ak_node.set_numeric_value(num);
    }

    if let Some(min) = node.min_numeric_value {
        ak_node.set_min_numeric_value(min);
    }

    if let Some(max) = node.max_numeric_value {
        ak_node.set_max_numeric_value(max);
    }

    if let Some(step) = node.numeric_step {
        ak_node.set_numeric_value_step(step);
    }

    if let Some(live) = node.live {
        match live {
            A11yLivePoliteness::Off => ak_node.set_live(accesskit::Live::Off),
            A11yLivePoliteness::Polite => ak_node.set_live(accesskit::Live::Polite),
            A11yLivePoliteness::Assertive => ak_node.set_live(accesskit::Live::Assertive),
        }
    }

    if node.disabled {
        ak_node.set_disabled();
    }

    if node.selected {
        ak_node.set_selected(true);
    }

    if let Some(chk) = node.checked {
        ak_node.set_toggled(if chk {
            accesskit::Toggled::True
        } else {
            accesskit::Toggled::False
        });
    }

    if let Some(exp) = node.expanded {
        ak_node.set_expanded(exp);
    }

    for action in &node.supported_actions {
        ak_node.add_action(a11y_action_to_accesskit(action));
    }

    ak_node
}

/// Action handler bridge that receives OS accessibility action requests from AccessKit
/// and dispatches them into the Strata event pipeline.
#[derive(Clone)]
pub struct StrataAccessKitActionHandler {
    bridge: Arc<Mutex<crate::bridge::A11yBridge>>,
}

impl StrataAccessKitActionHandler {
    pub fn new(bridge: Arc<Mutex<crate::bridge::A11yBridge>>) -> Self {
        Self { bridge }
    }
}

impl accesskit::ActionHandler for StrataAccessKitActionHandler {
    fn do_action(&mut self, request: accesskit::ActionRequest) {
        if let Some(action) = accesskit_action_to_a11y(request.action, request.data.as_ref()) {
            let target_id = A11yNodeId(request.target_node.0);
            if let Ok(bridge) = self.bridge.lock() {
                bridge.dispatch_action(target_id, action);
            }
        }
    }
}

/// Platform accessibility adapter managing AccessKit native updates, tree lifecycle, and action bridging.
pub struct AccessKitAdapter {
    app_name: String,
    tree_id: accesskit::TreeId,
    has_sent_initial_tree: bool,
}

impl AccessKitAdapter {
    pub fn new(app_name: impl Into<String>) -> Self {
        Self {
            app_name: app_name.into(),
            tree_id: accesskit::TreeId(uuid::Uuid::nil()),
            has_sent_initial_tree: false,
        }
    }

    /// Accesses the assigned AccessKit `TreeId`.
    pub fn tree_id(&self) -> accesskit::TreeId {
        self.tree_id
    }

    /// Generates a genuine `accesskit::TreeUpdate` from an incremental `A11yTreeUpdate`.
    pub fn to_accesskit_update(
        &mut self,
        update: &A11yTreeUpdate,
        root_id: Option<A11yNodeId>,
    ) -> accesskit::TreeUpdate {
        let send_meta = !self.has_sent_initial_tree || root_id.is_some();
        let tree_info = if send_meta {
            self.has_sent_initial_tree = true;
            root_id.map(|rid| {
                let mut info = accesskit::TreeInfo::new(accesskit::NodeId(rid.0));
                info.toolkit_name = Some("Strata".into());
                info.toolkit_version = Some(env!("CARGO_PKG_VERSION").into());
                info
            })
        } else {
            None
        };

        let nodes: Vec<(accesskit::NodeId, accesskit::Node)> = update
            .nodes
            .iter()
            .map(|(id, n)| (accesskit::NodeId(id.0), a11y_node_to_accesskit(n)))
            .collect();

        accesskit::TreeUpdate {
            nodes,
            tree: tree_info,
            tree_id: self.tree_id,
            focus: accesskit::NodeId(update.focus.map_or(0, |f| f.0)),
        }
    }

    /// Generates a complete initial `accesskit::TreeUpdate` snapshot from an `A11yTree`.
    pub fn to_accesskit_tree(&mut self, tree: &A11yTree) -> accesskit::TreeUpdate {
        self.has_sent_initial_tree = true;
        let diff = A11yTreeUpdate::diff(None, tree);
        self.to_accesskit_update(&diff, Some(tree.root_id))
    }

    /// Creates an `ActionHandler` that connects incoming platform screen reader actions to `A11yBridge`.
    pub fn create_action_handler(
        &self,
        bridge: Arc<Mutex<crate::bridge::A11yBridge>>,
    ) -> StrataAccessKitActionHandler {
        StrataAccessKitActionHandler::new(bridge)
    }

    /// Backwards-compatible legacy serializer method for existing tests.
    pub fn process_update(
        &mut self,
        update: &A11yTreeUpdate,
        root_id: Option<A11yNodeId>,
    ) -> AccessKitTreeUpdate {
        let send_meta = !self.has_sent_initial_tree || root_id.is_some();
        let meta_root = if send_meta {
            self.has_sent_initial_tree = true;
            root_id
        } else {
            None
        };

        AccessKitTreeUpdate::from_a11y_tree_update(update, meta_root, &self.app_name)
    }

    /// Backwards-compatible legacy serializer snapshot for existing tests.
    pub fn process_tree(&mut self, tree: &A11yTree) -> AccessKitTreeUpdate {
        self.has_sent_initial_tree = true;
        let update = A11yTreeUpdate::diff(None, tree);
        AccessKitTreeUpdate::from_a11y_tree_update(&update, Some(tree.root_id), &self.app_name)
    }

    /// Backwards-compatible action request dispatcher.
    pub fn handle_action_request(
        &self,
        request: &AccessKitActionRequest,
        bridge: &crate::bridge::A11yBridge,
    ) {
        bridge.dispatch_action(request.target, request.action.clone());
    }
}

/// Canonical serialized node structure for legacy test validation and IPC transmission.
#[derive(Debug, Clone, PartialEq)]
pub struct AccessKitNode {
    pub id: u64,
    pub role: &'static str,
    pub bounds: Option<[f32; 4]>,
    pub children: Vec<u64>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub value: Option<String>,
    pub numeric_value: Option<f64>,
    pub min_numeric_value: Option<f64>,
    pub max_numeric_value: Option<f64>,
    pub numeric_step: Option<f64>,
    pub live: Option<&'static str>,
    pub disabled: bool,
    pub selected: bool,
    pub checked: Option<bool>,
    pub expanded: Option<bool>,
    pub focusable: bool,
    pub focused: bool,
    pub actions: Vec<&'static str>,
}

impl AccessKitNode {
    pub fn from_a11y_node(node: &A11yNode) -> Self {
        let role_str = match node.role {
            A11yRole::Window => "window",
            A11yRole::RootWebArea => "rootWebArea",
            A11yRole::Button => "button",
            A11yRole::Checkbox => "checkBox",
            A11yRole::Radio | A11yRole::RadioButton => "radioButton",
            A11yRole::Slider => "slider",
            A11yRole::Heading => "heading",
            A11yRole::Dialog => "dialog",
            A11yRole::Alert => "alert",
            A11yRole::AlertDialog => "alertDialog",
            A11yRole::TextInput => "textInput",
            A11yRole::List => "list",
            A11yRole::ListItem => "listItem",
            A11yRole::Grid => "grid",
            A11yRole::Cell => "cell",
            A11yRole::Row => "row",
            A11yRole::Image => "image",
            A11yRole::Link => "link",
            A11yRole::Navigation => "navigation",
            A11yRole::Banner => "banner",
            A11yRole::Main => "main",
            A11yRole::Complementary => "complementary",
            A11yRole::ContentInfo => "contentInfo",
            A11yRole::Form => "form",
            A11yRole::Toolbar => "toolbar",
            A11yRole::Tab => "tab",
            A11yRole::TabList => "tabList",
            A11yRole::TabPanel => "tabPanel",
            A11yRole::Switch => "switch",
            A11yRole::Status => "status",
            A11yRole::ProgressBar => "progressIndicator",
            A11yRole::Menu => "menu",
            A11yRole::MenuItem => "menuItem",
            A11yRole::Tree => "tree",
            A11yRole::TreeItem => "treeItem",
            A11yRole::Unknown => "genericContainer",
        };

        let actions_str: Vec<&'static str> = node
            .supported_actions
            .iter()
            .map(|a| match a {
                A11yAction::Click => "click",
                A11yAction::Focus => "focus",
                A11yAction::Blur => "blur",
                A11yAction::SetValue(_) => "setValue",
                A11yAction::Increment => "increment",
                A11yAction::Decrement => "decrement",
                A11yAction::ScrollUp => "scrollUp",
                A11yAction::ScrollDown => "scrollDown",
            })
            .collect();

        Self {
            id: node.id.0,
            role: role_str,
            bounds: node.bounds.map(|b| [b.x, b.y, b.width, b.height]),
            children: node.children.iter().map(|c| c.0).collect(),
            name: node.name.clone(),
            description: node.description.clone(),
            value: node.value.clone(),
            numeric_value: node.numeric_value,
            min_numeric_value: node.min_numeric_value,
            max_numeric_value: node.max_numeric_value,
            numeric_step: node.numeric_step,
            live: node.live.map(|l| l.as_str()),
            disabled: node.disabled,
            selected: node.selected,
            checked: node.checked,
            expanded: node.expanded,
            focusable: node.focusable,
            focused: node.focused,
            actions: actions_str,
        }
    }
}

/// Metadata header describing the active application tree for AccessKit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessKitTreeMeta {
    pub root: u64,
    pub app_name: String,
    pub toolkit_name: String,
    pub toolkit_version: String,
}

/// Legacy incremental update payload serialized for platform assistive backends.
#[derive(Debug, Clone, PartialEq)]
pub struct AccessKitTreeUpdate {
    pub tree: Option<AccessKitTreeMeta>,
    pub nodes: Vec<AccessKitNode>,
    pub focus: Option<u64>,
}

impl AccessKitTreeUpdate {
    pub fn from_a11y_tree_update(
        update: &A11yTreeUpdate,
        root_id: Option<A11yNodeId>,
        app_name: &str,
    ) -> Self {
        let tree_meta = root_id.map(|rid| AccessKitTreeMeta {
            root: rid.0,
            app_name: app_name.to_string(),
            toolkit_name: "Strata".to_string(),
            toolkit_version: env!("CARGO_PKG_VERSION").to_string(),
        });

        let nodes: Vec<AccessKitNode> = update
            .nodes
            .iter()
            .map(|(_, n)| AccessKitNode::from_a11y_node(n))
            .collect();

        Self {
            tree: tree_meta,
            nodes,
            focus: update.focus.map(|f| f.0),
        }
    }

    pub fn to_json(&self) -> String {
        let mut out = String::with_capacity(1024);
        out.push_str("{\"type\":\"accesskit_tree_update\",");

        if let Some(ref t) = self.tree {
            out.push_str(&format!(
                "\"tree\":{{\"root\":{},\"app_name\":\"{}\",\"toolkit_name\":\"{}\",\"toolkit_version\":\"{}\"}},",
                t.root, t.app_name, t.toolkit_name, t.toolkit_version
            ));
        }

        if let Some(f) = self.focus {
            out.push_str(&format!("\"focus\":{f},"));
        } else {
            out.push_str("\"focus\":null,");
        }

        out.push_str("\"nodes\":[");
        for (i, node) in self.nodes.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(&format!("{{\"id\":{},\"role\":\"{}\"", node.id, node.role));

            if let Some(b) = node.bounds {
                out.push_str(&format!(
                    ",\"bounds\":[{},{},{},{}]",
                    b[0], b[1], b[2], b[3]
                ));
            }

            if let Some(ref name) = node.name {
                let escaped = name.replace('\"', "\\\"");
                out.push_str(&format!(",\"name\":\"{escaped}\""));
            }

            if let Some(ref val) = node.value {
                let escaped = val.replace('\"', "\\\"");
                out.push_str(&format!(",\"value\":\"{escaped}\""));
            }

            if let Some(num) = node.numeric_value {
                out.push_str(&format!(",\"numeric_value\":{num}"));
            }

            if let Some(live) = node.live {
                out.push_str(&format!(",\"live\":\"{live}\""));
            }

            if node.disabled {
                out.push_str(",\"disabled\":true");
            }
            if node.selected {
                out.push_str(",\"selected\":true");
            }
            if let Some(chk) = node.checked {
                out.push_str(&format!(",\"checked\":{chk}"));
            }
            if let Some(exp) = node.expanded {
                out.push_str(&format!(",\"expanded\":{exp}"));
            }
            if node.focused {
                out.push_str(",\"focused\":true");
            }

            out.push_str(",\"children\":[");
            for (j, cid) in node.children.iter().enumerate() {
                if j > 0 {
                    out.push(',');
                }
                out.push_str(&cid.to_string());
            }
            out.push(']');

            out.push('}');
        }
        out.push_str("]}");
        out
    }
}

/// Request sent by an external assistive technology to execute an action on a node.
#[derive(Debug, Clone, PartialEq)]
pub struct AccessKitActionRequest {
    pub target: A11yNodeId,
    pub action: A11yAction,
    pub data: Option<String>,
}
