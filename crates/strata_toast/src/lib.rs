//! Strata Toast: Sonner-Tier Stacked Notification Engine.
//!
//! Provides a reactive and imperative toast notification pipeline with 3D stacked card physics,
//! action buttons, swipe-to-dismiss semantics, and multi-channel categorization.

pub mod manager;
pub mod stack;
pub mod toast;

pub use manager::{
    toast_dismiss, toast_error, toast_info, toast_success, toast_warning, ToastManager,
};
pub use stack::{compute_stack_layout, StackedToastVisual};
pub use toast::{ToastAction, ToastItem, ToastKind, ToastPosition};

pub mod prelude {
    pub use crate::manager::{
        toast_dismiss, toast_error, toast_info, toast_success, toast_warning, ToastManager,
    };
    pub use crate::stack::{compute_stack_layout, StackedToastVisual};
    pub use crate::toast::{ToastAction, ToastItem, ToastKind, ToastPosition};
}
