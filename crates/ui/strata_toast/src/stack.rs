//! Stacked Spatial Layout Engine for Sonner-Style Layered Cards.

use crate::toast::{ToastItem, ToastPosition};

/// Computed spatial visual layout properties for an individual toast in a stack.
#[derive(Debug, Clone)]
pub struct StackedToastVisual {
    pub item: ToastItem,
    pub index: usize,
    pub offset_y: f32,
    pub scale: f32,
    pub opacity: f32,
    pub z_index: i32,
}

/// Solves 3D layered stacking physics for active toasts.
pub fn compute_stack_layout(
    toasts: &[ToastItem],
    position: ToastPosition,
    max_visible: usize,
) -> Vec<StackedToastVisual> {
    let visible_count = toasts.len().min(max_visible);
    let mut visuals = Vec::with_capacity(visible_count);

    let is_bottom = matches!(
        position,
        ToastPosition::BottomRight | ToastPosition::BottomLeft | ToastPosition::BottomCenter
    );

    // Front toast is newest (first in slice)
    for (idx, item) in toasts.iter().take(visible_count).enumerate() {
        let depth = idx as f32;
        let scale = (1.0 - (depth * 0.06)).max(0.7);
        let opacity = (1.0 - (depth * 0.18)).max(0.0);
        let z_index = 50 - idx as i32;

        let base_offset = depth * 14.0;
        let offset_y = if is_bottom {
            -base_offset // Expands upwards from bottom
        } else {
            base_offset  // Expands downwards from top
        };

        visuals.push(StackedToastVisual {
            item: item.clone(),
            index: idx,
            offset_y,
            scale,
            opacity,
            z_index,
        });
    }

    visuals
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::toast::ToastKind;

    #[test]
    fn test_compute_stack_layout() {
        let toasts = vec![
            ToastItem::new(1, "First (Front)", ToastKind::Success),
            ToastItem::new(2, "Second (Mid)", ToastKind::Info),
            ToastItem::new(3, "Third (Back)", ToastKind::Warning),
            ToastItem::new(4, "Fourth (Hidden)", ToastKind::Error),
        ];

        let stack = compute_stack_layout(&toasts, ToastPosition::BottomRight, 3);
        assert_eq!(stack.len(), 3);

        // Front card
        assert_eq!(stack[0].item.id, 1);
        assert_eq!(stack[0].offset_y, 0.0);
        assert_eq!(stack[0].scale, 1.0);
        assert_eq!(stack[0].z_index, 50);

        // Second card
        assert_eq!(stack[1].item.id, 2);
        assert_eq!(stack[1].offset_y, -14.0);
        assert_eq!(stack[1].scale, 0.94);
        assert_eq!(stack[1].z_index, 49);

        // Third card
        assert_eq!(stack[2].item.id, 3);
        assert_eq!(stack[2].offset_y, -28.0);
        assert_eq!(stack[2].scale, 0.88);
        assert_eq!(stack[2].z_index, 48);
    }
}
