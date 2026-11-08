//! Strata Aria: Headless UI Primitives, Accessible State Machines & Floating Collision Engine.
//!
//! Provides a pure-Rust, zero-runtime-overhead equivalent to Radix UI, React Aria, and Floating UI.
//! Developers retain 100% styling freedom while gaining robust accessibility, focus trapping,
//! roving tabindex keyboard navigation, and viewport collision detection out of the box.

pub mod accordion;
pub mod dialog;
pub mod floating;
pub mod focus_trap;
pub mod live_region;
pub mod roving_tabindex;
pub mod tooltip;

pub use accordion::{use_accordion, AccordionState, AccordionType};
pub use dialog::{use_dialog, DialogConfig, DialogState};
pub use floating::{
    compute_position, Alignment, ComputePositionOptions, ComputePositionResult, FloatRect,
    Placement, Rect, Side,
};
pub use focus_trap::FocusTrap;
pub use live_region::{
    announce_assertive, announce_polite, use_live_region, LiveAnnouncement, LiveRegionAnnouncer,
    LiveRegionConfig, LiveRegionPoliteness, LiveRegionRelevant, LiveRegionState,
};
pub use roving_tabindex::{Orientation, RovingTabIndex};
pub use tooltip::{use_tooltip, TooltipConfig, TooltipState};

pub mod prelude {
    pub use crate::accordion::{use_accordion, AccordionState, AccordionType};
    pub use crate::dialog::{use_dialog, DialogConfig, DialogState};
    pub use crate::floating::{
        compute_position, Alignment, ComputePositionOptions, ComputePositionResult, FloatRect,
        Placement, Side,
    };
    pub use crate::focus_trap::FocusTrap;
    pub use crate::live_region::{
        announce_assertive, announce_polite, use_live_region, LiveAnnouncement,
        LiveRegionAnnouncer, LiveRegionConfig, LiveRegionPoliteness, LiveRegionRelevant,
        LiveRegionState,
    };
    pub use crate::roving_tabindex::{Orientation, RovingTabIndex};
    pub use crate::tooltip::{use_tooltip, TooltipConfig, TooltipState};
}

