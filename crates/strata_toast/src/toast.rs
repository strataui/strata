//! Toast Data Model and Notification Kinds.

use std::rc::Rc;

/// Classification kind of the toast notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Default,
    Success,
    Error,
    Info,
    Warning,
    Loading,
}

/// Screen anchor position for stacked toast presentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastPosition {
    BottomRight,
    BottomLeft,
    BottomCenter,
    TopRight,
    TopLeft,
    TopCenter,
}

/// Interactive button action embedded within a toast.
#[derive(Clone)]
pub struct ToastAction {
    pub label: String,
    pub on_click: Option<Rc<dyn Fn()>>,
}

impl std::fmt::Debug for ToastAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToastAction").field("label", &self.label).finish()
    }
}

/// An individual toast notification item.
#[derive(Debug, Clone)]
pub struct ToastItem {
    pub id: u64,
    pub title: String,
    pub description: Option<String>,
    pub kind: ToastKind,
    pub duration_ms: u64,
    pub action: Option<ToastAction>,
    pub dismissible: bool,
}

impl ToastItem {
    pub fn new(id: u64, title: impl Into<String>, kind: ToastKind) -> Self {
        Self {
            id,
            title: title.into(),
            description: None,
            kind,
            duration_ms: 4000,
            action: None,
            dismissible: true,
        }
    }

    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    pub fn with_duration(mut self, duration_ms: u64) -> Self {
        self.duration_ms = duration_ms;
        self
    }

    pub fn with_action(mut self, label: impl Into<String>, on_click: impl Fn() + 'static) -> Self {
        self.action = Some(ToastAction {
            label: label.into(),
            on_click: Some(Rc::new(on_click)),
        });
        self
    }
}
