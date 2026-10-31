//! Reactive Virtualizer Hooks (use_virtualizer).

use std::cell::RefCell;
use std::rc::Rc;
use strata_dom::reactive::Signal;

use crate::virtualizer::{ScrollAlign, VirtualItem, VirtualOptions, Virtualizer};

/// Reactive handle returned by `use_virtualizer`.
#[derive(Clone)]
pub struct VirtualizerHandle {
    pub virtual_items: Signal<Vec<VirtualItem>>,
    pub total_size: Signal<f32>,
    pub scroll_offset: Signal<f32>,
    virtualizer: Rc<RefCell<Virtualizer>>,
}

impl VirtualizerHandle {
    /// Update current scroll position and recalculate windowed items.
    pub fn on_scroll(&self, offset: f32) {
        self.scroll_offset.set(offset);
        let mut v = self.virtualizer.borrow_mut();
        v.set_scroll_offset(offset);
        let items = v.get_virtual_items();
        self.virtual_items.set(items);
    }

    /// Report a dynamic measured item height/width.
    pub fn measure_item(&self, index: usize, size: f32) {
        let mut v = self.virtualizer.borrow_mut();
        v.measure_item(index, size);
        let total = v.total_size();
        let items = v.get_virtual_items();
        self.total_size.set(total);
        self.virtual_items.set(items);
    }

    /// Compute scroll target to bring item into view.
    pub fn scroll_to_index(&self, index: usize, align: ScrollAlign) -> f32 {
        let v = self.virtualizer.borrow();
        v.scroll_to_index(index, align)
    }

    /// Update total item count dynamically.
    pub fn set_count(&self, count: usize) {
        let mut v = self.virtualizer.borrow_mut();
        v.set_count(count);
        let total = v.total_size();
        let items = v.get_virtual_items();
        self.total_size.set(total);
        self.virtual_items.set(items);
    }
}

/// Hook to initialize a reactive list virtualizer.
pub fn use_virtualizer(options: VirtualOptions) -> VirtualizerHandle {
    let virtualizer = Virtualizer::new(options);
    let initial_items = virtualizer.get_virtual_items();
    let initial_total = virtualizer.total_size();

    VirtualizerHandle {
        virtual_items: Signal::new(initial_items),
        total_size: Signal::new(initial_total),
        scroll_offset: Signal::new(0.0),
        virtualizer: Rc::new(RefCell::new(virtualizer)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_use_virtualizer_hook() {
        let handle = use_virtualizer(VirtualOptions {
            count: 1_000,
            estimate_size: 50.0,
            overscan: 2,
            viewport_size: 500.0,
        });

        assert_eq!(handle.total_size.get(), 50_000.0);
        assert!(!handle.virtual_items.get().is_empty());

        handle.on_scroll(500.0);
        assert_eq!(handle.scroll_offset.get(), 500.0);
        assert!(handle.virtual_items.get()[0].index >= 8);
    }
}
