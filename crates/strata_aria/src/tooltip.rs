//! Accessible Tooltip Primitive with Delay and ARIA DescribedBy Semantics.

use strata_dom::Signal;

/// Configuration for an accessible tooltip.
#[derive(Debug, Clone)]
pub struct TooltipConfig {
    pub id: String,
    pub delay_ms: u32,
}

impl Default for TooltipConfig {
    fn default() -> Self {
        Self {
            id: "strata-tooltip".to_string(),
            delay_ms: 200,
        }
    }
}

/// Headless tooltip controller.
#[derive(Clone)]
pub struct TooltipState {
    pub is_open: Signal<bool>,
    pub id: String,
    pub delay_ms: u32,
}

impl TooltipState {
    /// Creates a new tooltip state.
    pub fn new(config: TooltipConfig) -> Self {
        Self {
            is_open: Signal::new(false),
            id: config.id,
            delay_ms: config.delay_ms,
        }
    }

    /// Open tooltip.
    pub fn show(&self) {
        self.is_open.set(true);
    }

    /// Close tooltip.
    pub fn hide(&self) {
        self.is_open.set(false);
    }

    /// Attributes for the interactive trigger element.
    pub fn trigger_attrs(&self) -> [(&'static str, String); 2] {
        [
            ("aria-describedby", self.id.clone()),
            ("data-state", if self.is_open.get() { "open".to_string() } else { "closed".to_string() }),
        ]
    }

    /// Attributes for the floating tooltip container.
    pub fn tooltip_attrs(&self) -> [(&'static str, String); 3] {
        [
            ("role", "tooltip".to_string()),
            ("id", self.id.clone()),
            ("aria-hidden", (!self.is_open.get()).to_string()),
        ]
    }
}

/// Convenience hook to create a tooltip state controller.
pub fn use_tooltip(config: TooltipConfig) -> TooltipState {
    TooltipState::new(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tooltip_state() {
        let tip = use_tooltip(TooltipConfig {
            id: "copy-tooltip".to_string(),
            delay_ms: 150,
        });

        assert!(!tip.is_open.get());
        tip.show();
        assert!(tip.is_open.get());
        let attrs = tip.trigger_attrs();
        assert_eq!(attrs[0].1, "copy-tooltip");
        assert_eq!(attrs[1].1, "open");

        tip.hide();
        assert!(!tip.is_open.get());
    }
}
