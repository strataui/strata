//! Cross-platform rich native OS notifications (`org.freedesktop.Notifications` over pure-Rust
//! D-Bus on Linux, `UNUserNotificationCenter`/osascript on macOS, WinRT Toast on Windows)
//! plus an in-memory deterministic recorder for headless CI testing.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};

/// Semantic priority level of an OS notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NotificationUrgency {
    Low = 0,
    #[default]
    Normal = 1,
    Critical = 2,
}

/// Interactive action button attached to a desktop notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationAction {
    pub id: String,
    pub label: String,
}

/// Full immutable record of a dispatched notification (also captured in recorder/CI mode).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationRecord {
    pub id: u32,
    pub app_name: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub body: String,
    pub icon: Option<String>,
    pub urgency: NotificationUrgency,
    pub category: Option<String>,
    pub timeout_ms: i32,
    pub replaces_id: u32,
    pub actions: Vec<NotificationAction>,
}

static RECORDER_ENABLED: AtomicBool = AtomicBool::new(false);
static NEXT_MOCK_ID: AtomicU32 = AtomicU32::new(1);
static RECORDED: OnceLock<Mutex<Vec<NotificationRecord>>> = OnceLock::new();
static CLOSED_IDS: OnceLock<Mutex<Vec<u32>>> = OnceLock::new();

fn recorded_store() -> &'static Mutex<Vec<NotificationRecord>> {
    RECORDED.get_or_init(|| Mutex::new(Vec::new()))
}

fn closed_store() -> &'static Mutex<Vec<u32>> {
    CLOSED_IDS.get_or_init(|| Mutex::new(Vec::new()))
}

/// Enables or disables deterministic in-memory notification recording (skips live OS popups in CI).
pub fn enable_notification_recorder(enabled: bool) {
    RECORDER_ENABLED.store(enabled, Ordering::SeqCst);
}

/// Drains and returns all notifications captured while the recorder was enabled.
pub fn take_recorded_notifications() -> Vec<NotificationRecord> {
    recorded_store()
        .lock()
        .map(|mut v| std::mem::take(&mut *v))
        .unwrap_or_default()
}

/// Drains and returns all notification IDs dismissed while the recorder was enabled.
pub fn take_closed_notification_ids() -> Vec<u32> {
    closed_store()
        .lock()
        .map(|mut v| std::mem::take(&mut *v))
        .unwrap_or_default()
}

/// Fluent builder for rich cross-platform native OS notifications.
#[derive(Debug, Clone)]
pub struct NotificationBuilder {
    app_name: String,
    title: String,
    subtitle: Option<String>,
    body: String,
    icon: Option<String>,
    urgency: NotificationUrgency,
    category: Option<String>,
    timeout_ms: i32,
    replaces_id: u32,
    actions: Vec<NotificationAction>,
}

impl NotificationBuilder {
    pub fn new(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            app_name: "Strata".into(),
            title: title.into(),
            subtitle: None,
            body: body.into(),
            icon: None,
            urgency: NotificationUrgency::Normal,
            category: None,
            timeout_ms: 5000,
            replaces_id: 0,
            actions: Vec::new(),
        }
    }

    pub fn app_name(mut self, name: impl Into<String>) -> Self {
        self.app_name = name.into();
        self
    }

    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    pub fn icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn urgency(mut self, urgency: NotificationUrgency) -> Self {
        self.urgency = urgency;
        self
    }

    pub fn category(mut self, category: impl Into<String>) -> Self {
        self.category = Some(category.into());
        self
    }

    pub fn timeout_ms(mut self, ms: i32) -> Self {
        self.timeout_ms = ms;
        self
    }

    pub fn replaces_id(mut self, id: u32) -> Self {
        self.replaces_id = id;
        self
    }

    pub fn action(mut self, id: impl Into<String>, label: impl Into<String>) -> Self {
        self.actions.push(NotificationAction {
            id: id.into(),
            label: label.into(),
        });
        self
    }

    /// Dispatches the notification to the host OS notification daemon and returns its assigned ID.
    pub fn send(self) -> Result<u32, String> {
        if RECORDER_ENABLED.load(Ordering::SeqCst) {
            let id = if self.replaces_id > 0 {
                self.replaces_id
            } else {
                NEXT_MOCK_ID.fetch_add(1, Ordering::SeqCst)
            };
            if let Ok(mut store) = recorded_store().lock() {
                store.push(NotificationRecord {
                    id,
                    app_name: self.app_name,
                    title: self.title,
                    subtitle: self.subtitle,
                    body: self.body,
                    icon: self.icon,
                    urgency: self.urgency,
                    category: self.category,
                    timeout_ms: self.timeout_ms,
                    replaces_id: self.replaces_id,
                    actions: self.actions,
                });
            }
            return Ok(id);
        }

        #[cfg(target_os = "linux")]
        {
            let action_pairs: Vec<(&str, &str)> = self
                .actions
                .iter()
                .map(|a| (a.id.as_str(), a.label.as_str()))
                .collect();
            let full_body = match &self.subtitle {
                Some(sub) => format!("{sub}\n{}", self.body),
                None => self.body.clone(),
            };
            crate::dbus::show_desktop_notification_rich(
                &self.app_name,
                self.replaces_id,
                self.icon.as_deref(),
                &self.title,
                &full_body,
                &action_pairs,
                self.urgency as u8,
                self.category.as_deref(),
                self.timeout_ms,
            )
        }

        #[cfg(target_os = "macos")]
        {
            let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
            let mut script = format!(
                "display notification \"{}\" with title \"{}\"",
                esc(&self.body),
                esc(&self.title)
            );
            if let Some(ref sub) = self.subtitle {
                script.push_str(&format!(" subtitle \"{}\"", esc(sub)));
            }
            let status = std::process::Command::new("osascript")
                .args(["-e", &script])
                .status()
                .map_err(|e| format!("macOS notification failed: {e}"))?;
            if status.success() {
                Ok(NEXT_MOCK_ID.fetch_add(1, Ordering::SeqCst))
            } else {
                Err("osascript notification exited non-zero".into())
            }
        }

        #[cfg(windows)]
        {
            let esc = |s: &str| s.replace('\'', "''").replace('<', "&lt;").replace('>', "&gt;");
            let ps = format!(
                "[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] > $null; \
                 $xml = New-Object Windows.Data.Xml.Dom.XmlDocument; \
                 $xml.LoadXml('<toast><visual><binding template=\"ToastGeneric\"><text>{}</text><text>{}</text></binding></visual></toast>'); \
                 $toast = New-Object Windows.UI.Notifications.ToastNotification $xml; \
                 [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('{}').Show($toast);",
                esc(&self.title),
                esc(&self.body),
                esc(&self.app_name)
            );
            let status = std::process::Command::new("powershell")
                .args(["-NoProfile", "-Command", &ps])
                .status()
                .map_err(|e| format!("Windows Toast failed: {e}"))?;
            if status.success() {
                Ok(NEXT_MOCK_ID.fetch_add(1, Ordering::SeqCst))
            } else {
                Err("Windows Toast exited non-zero".into())
            }
        }

        #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
        {
            Ok(NEXT_MOCK_ID.fetch_add(1, Ordering::SeqCst))
        }
    }
}

/// Dismisses a previously dispatched desktop notification by ID.
pub fn close_notification(id: u32) -> Result<(), String> {
    if RECORDER_ENABLED.load(Ordering::SeqCst) {
        if let Ok(mut closed) = closed_store().lock() {
            closed.push(id);
        }
        return Ok(());
    }

    #[cfg(target_os = "linux")]
    {
        crate::dbus::close_desktop_notification(id)
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = id;
        Ok(())
    }
}
