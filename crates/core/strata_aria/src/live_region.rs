//! W3C ARIA Live Regions and Assistive Speech Announcer for Strata.
//!
//! Conforms to W3C ARIA 1.2 Live Regions Specification (WAI-ARIA 1.2 §5.2.7):
//! - `aria-live`: `off`, `polite`, `assertive`
//! - `aria-atomic`: `true`, `false`
//! - `aria-busy`: `true`, `false`
//! - `aria-relevant`: `additions`, `removals`, `text`, `all`, `additions text`
//!
//! Provides reactive hooks and a thread-safe announcer queue allowing screen readers
//! (Orca, NVDA, VoiceOver) to vocalize dynamic status updates, toast alerts, and layout mutations.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use strata_dom::Signal;

/// Politeness level of a live region governing speech interruption.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum LiveRegionPoliteness {
    /// Screen readers will not announce changes to this region automatically.
    #[default]
    Off,
    /// Screen readers announce changes at the next graceful opportunity (e.g. idle).
    Polite,
    /// Screen readers announce changes immediately, interrupting current vocalization.
    Assertive,
}

impl LiveRegionPoliteness {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Polite => "polite",
            Self::Assertive => "assertive",
        }
    }

    pub fn parse_politeness(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "polite" => Self::Polite,
            "assertive" => Self::Assertive,
            _ => Self::Off,
        }
    }
}

/// Types of changes in a live region considered relevant for speech synthesis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum LiveRegionRelevant {
    Additions,
    Removals,
    Text,
    #[default]
    AdditionsText,
    All,
}

impl LiveRegionRelevant {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Additions => "additions",
            Self::Removals => "removals",
            Self::Text => "text",
            Self::AdditionsText => "additions text",
            Self::All => "all",
        }
    }

    pub fn parse_relevant(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "additions" => Self::Additions,
            "removals" => Self::Removals,
            "text" => Self::Text,
            "all" => Self::All,
            _ => Self::AdditionsText,
        }
    }
}

/// A vocal announcement dispatched to assistive technologies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveAnnouncement {
    pub id: u64,
    pub message: String,
    pub politeness: LiveRegionPoliteness,
    pub atomic: bool,
    pub timestamp_ms: u64,
}

/// Thread-safe queue of live region vocal announcements.
pub struct LiveRegionAnnouncer {
    queue: Mutex<Vec<LiveAnnouncement>>,
    counter: AtomicU64,
}

static GLOBAL_ANNOUNCER: std::sync::OnceLock<LiveRegionAnnouncer> = std::sync::OnceLock::new();

impl Default for LiveRegionAnnouncer {
    fn default() -> Self {
        Self::new()
    }
}

impl LiveRegionAnnouncer {
    pub fn new() -> Self {
        Self {
            queue: Mutex::new(Vec::new()),
            counter: AtomicU64::new(1),
        }
    }

    /// Access the global singleton announcer instance.
    pub fn global() -> &'static Self {
        GLOBAL_ANNOUNCER.get_or_init(Self::new)
    }

    /// Enqueue an announcement with explicit politeness and atomic presentation flags.
    pub fn announce(&self, message: impl Into<String>, politeness: LiveRegionPoliteness, atomic: bool) -> u64 {
        if politeness == LiveRegionPoliteness::Off {
            return 0;
        }

        let msg = message.into();
        if msg.trim().is_empty() {
            return 0;
        }

        let id = self.counter.fetch_add(1, Ordering::Relaxed);
        let announcement = LiveAnnouncement {
            id,
            message: msg,
            politeness,
            atomic,
            timestamp_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
        };

        if let Ok(mut lock) = self.queue.lock() {
            lock.push(announcement);
        }

        id
    }

    /// Enqueue a polite announcement (announced when screen reader is idle).
    pub fn announce_polite(&self, message: impl Into<String>) -> u64 {
        self.announce(message, LiveRegionPoliteness::Polite, true)
    }

    /// Enqueue an assertive announcement (interrupts current speech immediately).
    pub fn announce_assertive(&self, message: impl Into<String>) -> u64 {
        self.announce(message, LiveRegionPoliteness::Assertive, true)
    }

    /// Drains all pending vocal announcements for submission to OS assistive bridges.
    pub fn drain_announcements(&self) -> Vec<LiveAnnouncement> {
        if let Ok(mut lock) = self.queue.lock() {
            std::mem::take(&mut *lock)
        } else {
            Vec::new()
        }
    }

    /// Number of announcements currently awaiting presentation.
    pub fn pending_count(&self) -> usize {
        if let Ok(lock) = self.queue.lock() {
            lock.len()
        } else {
            0
        }
    }

    /// Clears any pending announcements.
    pub fn clear(&self) {
        if let Ok(mut lock) = self.queue.lock() {
            lock.clear();
        }
    }
}

/// Configuration options for creating a live region.
#[derive(Debug, Clone)]
pub struct LiveRegionConfig {
    pub id: String,
    pub role: Option<String>,
    pub politeness: LiveRegionPoliteness,
    pub atomic: bool,
    pub relevant: LiveRegionRelevant,
}

impl Default for LiveRegionConfig {
    fn default() -> Self {
        Self {
            id: "strata-live-region".to_string(),
            role: Some("status".to_string()),
            politeness: LiveRegionPoliteness::Polite,
            atomic: true,
            relevant: LiveRegionRelevant::AdditionsText,
        }
    }
}

/// Headless state manager for an interactive or dynamic live region.
#[derive(Clone)]
pub struct LiveRegionState {
    pub id: String,
    pub role: Option<String>,
    pub politeness: Signal<LiveRegionPoliteness>,
    pub atomic: Signal<bool>,
    pub busy: Signal<bool>,
    pub relevant: Signal<LiveRegionRelevant>,
    pub content: Signal<String>,
}

impl LiveRegionState {
    pub fn new(config: LiveRegionConfig) -> Self {
        Self {
            id: config.id,
            role: config.role,
            politeness: Signal::new(config.politeness),
            atomic: Signal::new(config.atomic),
            busy: Signal::new(false),
            relevant: Signal::new(config.relevant),
            content: Signal::new(String::new()),
        }
    }

    /// Mutates the live region's content and dispatches a vocal announcement if not busy.
    pub fn set_content(&self, text: impl Into<String>) {
        let new_text = text.into();
        self.content.set(new_text.clone());

        if !self.busy.get() && self.politeness.get() != LiveRegionPoliteness::Off {
            LiveRegionAnnouncer::global().announce(new_text, self.politeness.get(), self.atomic.get());
        }
    }

    /// Marks the live region as busy (e.g. while fetching data or performing complex DOM mutations).
    /// Screen readers will suppress announcements until busy is unset.
    pub fn set_busy(&self, is_busy: bool) {
        self.busy.set(is_busy);
        if !is_busy && self.politeness.get() != LiveRegionPoliteness::Off {
            let current = self.content.get();
            if !current.trim().is_empty() {
                LiveRegionAnnouncer::global().announce(current, self.politeness.get(), self.atomic.get());
            }
        }
    }

    /// Returns standard W3C ARIA attribute pairs for binding onto element blueprints.
    pub fn attrs(&self) -> Vec<(&'static str, String)> {
        let mut out = Vec::with_capacity(5);
        if let Some(ref r) = self.role {
            out.push(("role", r.clone()));
        }
        out.push(("aria-live", self.politeness.get().as_str().to_string()));
        out.push(("aria-atomic", self.atomic.get().to_string()));
        out.push(("aria-busy", self.busy.get().to_string()));
        out.push(("aria-relevant", self.relevant.get().as_str().to_string()));
        out
    }
}

/// Convenience hook to initialize a headless ARIA live region.
pub fn use_live_region(config: LiveRegionConfig) -> LiveRegionState {
    LiveRegionState::new(config)
}

/// Dispatches an immediate polite speech announcement to the global queue.
pub fn announce_polite(message: impl Into<String>) -> u64 {
    LiveRegionAnnouncer::global().announce_polite(message)
}

/// Dispatches an immediate assertive speech announcement to the global queue.
pub fn announce_assertive(message: impl Into<String>) -> u64 {
    LiveRegionAnnouncer::global().announce_assertive(message)
}
