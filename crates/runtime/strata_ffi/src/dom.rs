//! C ABI DOM blueprint & View construction builders with live reactive signals and event callbacks.

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_void};
use std::rc::Rc;

use strata_dom::{Attribute, EventType, NodeBlueprint, Tag, View};

use crate::error::{clear_last_error_internal, ffi_guard, ffi_guard_ptr, set_last_error, StrataStatus};
use crate::signal::StrataSignalHandle;

/// Opaque handle representing a mutable DOM [`NodeBlueprint`] under construction from C.
pub struct StrataNodeHandle {
    pub(crate) node: NodeBlueprint,
}

/// Opaque handle representing an immutable [`View`] ready for [`strata::runtime::AppRuntime`].
pub struct StrataViewHandle {
    pub(crate) view: View,
}

/// C-compatible event payload delivered to callbacks registered with `strata_node_on_event`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct StrataDomEvent {
    /// Numeric event kind matching `StrataEventKind` constants (e.g. `1` = click, `11` = input).
    pub event_kind: u32,
    pub client_x: f64,
    pub client_y: f64,
    pub button: i16,
    /// Borrowed null-terminated UTF-8 string valid only for the duration of the callback.
    pub value: *const c_char,
}

/// Callback signature invoked when a registered DOM event fires on a C-built node.
pub type StrataEventCallback = extern "C" fn(event: *const StrataDomEvent, user_data: *mut c_void);

fn parse_tag(name: &str) -> (Tag, Option<String>) {
    match name.to_ascii_lowercase().as_str() {
        "div" => (Tag::Div, None),
        "span" => (Tag::Span, None),
        "button" => (Tag::Button, None),
        "input" => (Tag::Input, None),
        "select" => (Tag::Select, None),
        "option" => (Tag::Option, None),
        "textarea" => (Tag::Textarea, None),
        "label" => (Tag::Label, None),
        "form" => (Tag::Form, None),
        "header" => (Tag::Header, None),
        "nav" => (Tag::Nav, None),
        "main" => (Tag::Main, None),
        "section" => (Tag::Section, None),
        "aside" => (Tag::Aside, None),
        "footer" => (Tag::Footer, None),
        "h1" => (Tag::H1, None),
        "h2" => (Tag::H2, None),
        "h3" => (Tag::H3, None),
        "h4" => (Tag::H4, None),
        "h5" => (Tag::H5, None),
        "h6" => (Tag::H6, None),
        "p" => (Tag::P, None),
        "ul" => (Tag::Ul, None),
        "ol" => (Tag::Ol, None),
        "li" => (Tag::Li, None),
        "a" => (Tag::A, None),
        "img" => (Tag::Img, None),
        "audio" => (Tag::Audio, None),
        "video" => (Tag::Video, None),
        "svg" => (Tag::Svg, None),
        "path" => (Tag::Path, None),
        "canvas" => (Tag::Canvas, None),
        "table" => (Tag::Table, None),
        "thead" => (Tag::Thead, None),
        "tbody" => (Tag::Tbody, None),
        "tr" => (Tag::Tr, None),
        "th" => (Tag::Th, None),
        "td" => (Tag::Td, None),
        "hr" => (Tag::Hr, None),
        "br" => (Tag::Br, None),
        custom => (Tag::Div, Some(custom.to_string())),
    }
}

fn parse_event_type(name: &str) -> Option<(EventType, u32)> {
    match name.to_ascii_lowercase().as_str() {
        "click" => Some((EventType::Click, 1)),
        "dblclick" => Some((EventType::DblClick, 2)),
        "contextmenu" => Some((EventType::ContextMenu, 3)),
        "mousedown" => Some((EventType::MouseDown, 4)),
        "mouseup" => Some((EventType::MouseUp, 5)),
        "mousemove" => Some((EventType::MouseMove, 6)),
        "pointerdown" => Some((EventType::PointerDown, 7)),
        "pointerup" => Some((EventType::PointerUp, 8)),
        "pointermove" => Some((EventType::PointerMove, 9)),
        "keydown" => Some((EventType::KeyDown, 10)),
        "keyup" => Some((EventType::KeyUp, 11)),
        "input" => Some((EventType::Input, 12)),
        "change" => Some((EventType::Change, 13)),
        "submit" => Some((EventType::Submit, 14)),
        "focus" => Some((EventType::Focus, 15)),
        "blur" => Some((EventType::Blur, 16)),
        "wheel" => Some((EventType::Wheel, 17)),
        "scroll" => Some((EventType::Scroll, 18)),
        _ => None,
    }
}

/// Allocates a new HTML element node (e.g. `"div"`, `"button"`, `"span"`, `"input"`).
#[no_mangle]
pub unsafe extern "C" fn strata_node_element(tag: *const c_char) -> *mut StrataNodeHandle {
    ffi_guard_ptr(|| {
        if tag.is_null() {
            set_last_error("strata_node_element: tag pointer is NULL");
            return std::ptr::null_mut();
        }
        let tag_str = match CStr::from_ptr(tag).to_str() {
            Ok(s) => s,
            Err(_) => {
                set_last_error("strata_node_element: tag is invalid UTF-8");
                return std::ptr::null_mut();
            }
        };
        let (parsed_tag, custom_attr) = parse_tag(tag_str);
        let mut attributes = Vec::new();
        if let Some(custom_name) = custom_attr {
            attributes.push(Attribute::property("data-custom-tag", custom_name));
        }
        let node = NodeBlueprint::Element {
            tag: parsed_tag,
            classes: Vec::<String>::new().into(),
            attributes,
            children: Vec::new(),
        };
        clear_last_error_internal();
        Box::into_raw(Box::new(StrataNodeHandle { node }))
    })
}

/// Allocates a new static UTF-8 text leaf node.
#[no_mangle]
pub unsafe extern "C" fn strata_node_text(text: *const c_char) -> *mut StrataNodeHandle {
    ffi_guard_ptr(|| {
        if text.is_null() {
            set_last_error("strata_node_text: text pointer is NULL");
            return std::ptr::null_mut();
        }
        let content = match CStr::from_ptr(text).to_str() {
            Ok(s) => s.to_string(),
            Err(_) => {
                set_last_error("strata_node_text: text is invalid UTF-8");
                return std::ptr::null_mut();
            }
        };
        clear_last_error_internal();
        Box::into_raw(Box::new(StrataNodeHandle {
            node: NodeBlueprint::Text(content),
        }))
    })
}

/// Allocates a live reactive `DynamicText` node whose rendered text tracks `sig` automatically.
#[no_mangle]
pub unsafe extern "C" fn strata_node_dynamic_text_signal(
    sig: *const StrataSignalHandle,
) -> *mut StrataNodeHandle {
    ffi_guard_ptr(|| {
        if sig.is_null() {
            set_last_error("strata_node_dynamic_text_signal: signal pointer is NULL");
            return std::ptr::null_mut();
        }
        let cell = (*sig).clone_cell();
        let node = NodeBlueprint::dynamic_text(move || cell.format_text());
        clear_last_error_internal();
        Box::into_raw(Box::new(StrataNodeHandle { node }))
    })
}

/// Replaces the Tailwind/CSS class list on an element node from a whitespace-separated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn strata_node_set_class(
    node: *mut StrataNodeHandle,
    classes: *const c_char,
) -> StrataStatus {
    ffi_guard(|| {
        if node.is_null() || classes.is_null() {
            set_last_error("strata_node_set_class: node or classes pointer is NULL");
            return StrataStatus::NullPointer;
        }
        let cls_str = match CStr::from_ptr(classes).to_str() {
            Ok(s) => s,
            Err(_) => {
                set_last_error("strata_node_set_class: classes is invalid UTF-8");
                return StrataStatus::InvalidUtf8;
            }
        };
        match &mut (*node).node {
            NodeBlueprint::Element {
                classes: ref mut list,
                ..
            } => {
                let parsed: Vec<String> = cls_str.split_whitespace().map(str::to_string).collect();
                *list = parsed.into();
                clear_last_error_internal();
                StrataStatus::Ok
            }
            _ => {
                set_last_error("strata_node_set_class: target node is not an Element");
                StrataStatus::InvalidHandle
            }
        }
    })
}

/// Sets or updates a string attribute (e.g. `"id"`, `"placeholder"`, `"title"`) on an element node.
#[no_mangle]
pub unsafe extern "C" fn strata_node_set_attr(
    node: *mut StrataNodeHandle,
    key: *const c_char,
    value: *const c_char,
) -> StrataStatus {
    ffi_guard(|| {
        if node.is_null() || key.is_null() || value.is_null() {
            set_last_error("strata_node_set_attr: node, key, or value pointer is NULL");
            return StrataStatus::NullPointer;
        }
        let k = match CStr::from_ptr(key).to_str() {
            Ok(s) => s.to_string(),
            Err(_) => {
                set_last_error("strata_node_set_attr: key is invalid UTF-8");
                return StrataStatus::InvalidUtf8;
            }
        };
        let v = match CStr::from_ptr(value).to_str() {
            Ok(s) => s.to_string(),
            Err(_) => {
                set_last_error("strata_node_set_attr: value is invalid UTF-8");
                return StrataStatus::InvalidUtf8;
            }
        };
        match &mut (*node).node {
            NodeBlueprint::Element { attributes, .. } => {
                if let Some(existing) = attributes.iter_mut().find(|attr| {
                    matches!(attr, Attribute::Property { key: ek, .. } if ek == &k)
                }) {
                    *existing = Attribute::Property { key: k, value: v };
                } else {
                    attributes.push(Attribute::Property { key: k, value: v });
                }
                clear_last_error_internal();
                StrataStatus::Ok
            }
            _ => {
                set_last_error("strata_node_set_attr: target node is not an Element");
                StrataStatus::InvalidHandle
            }
        }
    })
}

/// Appends `child` to `parent`, **taking ownership** of `child`.
///
/// After a successful call, the caller must NOT call `strata_node_destroy(child)`.
#[no_mangle]
pub unsafe extern "C" fn strata_node_append_child(
    parent: *mut StrataNodeHandle,
    child: *mut StrataNodeHandle,
) -> StrataStatus {
    ffi_guard(|| {
        if parent.is_null() || child.is_null() {
            set_last_error("strata_node_append_child: parent or child pointer is NULL");
            return StrataStatus::NullPointer;
        }
        match &mut (*parent).node {
            NodeBlueprint::Element { children, .. } | NodeBlueprint::Fragment(children) => {
                let owned_child = Box::from_raw(child);
                children.push(owned_child.node);
                clear_last_error_internal();
                StrataStatus::Ok
            }
            _ => {
                set_last_error("strata_node_append_child: parent node cannot accept children");
                StrataStatus::InvalidHandle
            }
        }
    })
}

/// Attaches a C event listener (e.g. `"click"`, `"input"`, `"keydown"`) to an element node.
#[no_mangle]
pub unsafe extern "C" fn strata_node_on_event(
    node: *mut StrataNodeHandle,
    event_name: *const c_char,
    callback: Option<StrataEventCallback>,
    user_data: *mut c_void,
) -> StrataStatus {
    ffi_guard(|| {
        if node.is_null() || event_name.is_null() {
            set_last_error("strata_node_on_event: node or event_name pointer is NULL");
            return StrataStatus::NullPointer;
        }
        let cb = match callback {
            Some(c) => c,
            None => {
                set_last_error("strata_node_on_event: callback is NULL");
                return StrataStatus::NullPointer;
            }
        };
        let ev_str = match CStr::from_ptr(event_name).to_str() {
            Ok(s) => s,
            Err(_) => {
                set_last_error("strata_node_on_event: event_name is invalid UTF-8");
                return StrataStatus::InvalidUtf8;
            }
        };
        let Some((ev_type, ev_kind)) = parse_event_type(ev_str) else {
            set_last_error(format!("strata_node_on_event: unrecognized event '{ev_str}'"));
            return StrataStatus::InvalidArgument;
        };

        let user_data_addr = user_data as usize;
        let listener = Attribute::EventListener {
            event: ev_type,
            handler: Rc::new(move |ev: strata_dom::Event| {
                let sanitized = ev.value.replace('\0', "");
                let c_val = CString::new(sanitized).unwrap_or_else(|_| CString::new("").unwrap());
                let payload = StrataDomEvent {
                    event_kind: ev_kind,
                    client_x: ev.client_x,
                    client_y: ev.client_y,
                    button: ev.button,
                    value: c_val.as_ptr(),
                };
                cb(&payload as *const StrataDomEvent, user_data_addr as *mut c_void);
            }),
        };

        match &mut (*node).node {
            NodeBlueprint::Element { attributes, .. } => {
                attributes.push(listener);
                clear_last_error_internal();
                StrataStatus::Ok
            }
            _ => {
                set_last_error("strata_node_on_event: target node is not an Element");
                StrataStatus::InvalidHandle
            }
        }
    })
}

/// Destroys a standalone [`StrataNodeHandle`] that was NOT moved into a parent or View.
#[no_mangle]
pub unsafe extern "C" fn strata_node_destroy(node: *mut StrataNodeHandle) {
    if !node.is_null() {
        let _ = Box::from_raw(node);
    }
}

/// Consumes `root` (taking ownership) and wraps it into a [`StrataViewHandle`].
///
/// After a non-null return, the caller must NOT call `strata_node_destroy(root)`.
#[no_mangle]
pub unsafe extern "C" fn strata_view_from_root(
    root: *mut StrataNodeHandle,
) -> *mut StrataViewHandle {
    ffi_guard_ptr(|| {
        if root.is_null() {
            set_last_error("strata_view_from_root: root node pointer is NULL");
            return std::ptr::null_mut();
        }
        let owned = Box::from_raw(root);
        clear_last_error_internal();
        Box::into_raw(Box::new(StrataViewHandle {
            view: View::new(owned.node),
        }))
    })
}

/// Releases a [`StrataViewHandle`].
#[no_mangle]
pub unsafe extern "C" fn strata_view_destroy(view: *mut StrataViewHandle) {
    if !view.is_null() {
        let _ = Box::from_raw(view);
    }
}
