//! High-Scale Windowed List Virtualization Engine.

use std::collections::HashMap;

/// Alignment for scroll-to-index operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollAlign {
    Start,
    Center,
    End,
    Auto,
}

/// A slice of a virtual item visible inside the viewport.
#[derive(Debug, Clone, PartialEq)]
pub struct VirtualItem {
    pub index: usize,
    pub start: f32,
    pub size: f32,
    pub end: f32,
}

/// Options configuring virtual scroll slicing.
#[derive(Debug, Clone)]
pub struct VirtualOptions {
    pub count: usize,
    pub estimate_size: f32,
    pub overscan: usize,
    pub viewport_size: f32,
}

impl Default for VirtualOptions {
    fn default() -> Self {
        Self {
            count: 0,
            estimate_size: 40.0,
            overscan: 3,
            viewport_size: 600.0,
        }
    }
}

/// State-machine calculating windowed item slices.
#[derive(Debug, Clone)]
pub struct Virtualizer {
    options: VirtualOptions,
    custom_sizes: HashMap<usize, f32>,
    scroll_offset: f32,
}

impl Virtualizer {
    pub fn new(options: VirtualOptions) -> Self {
        Self {
            options,
            custom_sizes: HashMap::new(),
            scroll_offset: 0.0,
        }
    }

    /// Update current scroll offset in pixels.
    pub fn set_scroll_offset(&mut self, offset: f32) {
        self.scroll_offset = offset.max(0.0);
    }

    /// Update viewport size.
    pub fn set_viewport_size(&mut self, size: f32) {
        self.options.viewport_size = size.max(1.0);
    }

    /// Update item count.
    pub fn set_count(&mut self, count: usize) {
        self.options.count = count;
    }

    /// Record a measured dynamic size for a specific item index.
    pub fn measure_item(&mut self, index: usize, size: f32) {
        self.custom_sizes.insert(index, size);
    }

    /// Size of a given item index.
    pub fn get_item_size(&self, index: usize) -> f32 {
        self.custom_sizes
            .get(&index)
            .copied()
            .unwrap_or(self.options.estimate_size)
    }

    /// Compute total virtual height/width of all items combined.
    pub fn total_size(&self) -> f32 {
        if self.custom_sizes.is_empty() {
            self.options.count as f32 * self.options.estimate_size
        } else {
            let mut total = 0.0;
            for i in 0..self.options.count {
                total += self.get_item_size(i);
            }
            total
        }
    }

    /// Calculate the visible range of items including overscan boundaries.
    pub fn get_virtual_items(&self) -> Vec<VirtualItem> {
        let count = self.options.count;
        if count == 0 {
            return Vec::new();
        }

        let start_bound = self.scroll_offset;
        let end_bound = self.scroll_offset + self.options.viewport_size;

        let mut current_offset = 0.0;
        let mut first_visible = None;
        let mut last_visible = None;

        let mut offsets = Vec::with_capacity(count);

        for i in 0..count {
            let size = self.get_item_size(i);
            let item_start = current_offset;
            let item_end = item_start + size;
            offsets.push((item_start, size));

            if item_end > start_bound && first_visible.is_none() {
                first_visible = Some(i);
            }

            if item_start <= end_bound {
                last_visible = Some(i);
            } else if first_visible.is_some() {
                // Passed viewport
                break;
            }

            current_offset = item_end;
        }

        let first_idx = first_visible.unwrap_or(0);
        let last_idx = last_visible.unwrap_or(first_idx);

        // Apply overscan
        let start_idx = first_idx.saturating_sub(self.options.overscan);
        let end_idx = (last_idx + self.options.overscan + 1).min(count);

        let mut items = Vec::with_capacity(end_idx - start_idx);
        for i in start_idx..end_idx {
            let (start, size) = if i < offsets.len() {
                offsets[i]
            } else {
                let s = self.get_item_size(i);
                (i as f32 * self.options.estimate_size, s)
            };
            items.push(VirtualItem {
                index: i,
                start,
                size,
                end: start + size,
            });
        }

        items
    }

    /// Compute scroll offset needed to bring specified item index into view.
    pub fn scroll_to_index(&self, index: usize, align: ScrollAlign) -> f32 {
        if index >= self.options.count {
            return self.scroll_offset;
        }

        let mut item_start = 0.0;
        for i in 0..index {
            item_start += self.get_item_size(i);
        }
        let item_size = self.get_item_size(index);
        let item_end = item_start + item_size;

        match align {
            ScrollAlign::Start => item_start,
            ScrollAlign::End => (item_end - self.options.viewport_size).max(0.0),
            ScrollAlign::Center => {
                (item_start + item_size / 2.0 - self.options.viewport_size / 2.0).max(0.0)
            }
            ScrollAlign::Auto => {
                if item_start < self.scroll_offset {
                    item_start
                } else if item_end > self.scroll_offset + self.options.viewport_size {
                    (item_end - self.options.viewport_size).max(0.0)
                } else {
                    self.scroll_offset
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_virtualizer_windowing_100k_items() {
        let options = VirtualOptions {
            count: 100_000,
            estimate_size: 50.0,
            overscan: 2,
            viewport_size: 500.0, // exactly 10 visible items (10 * 50 = 500)
        };

        let mut virtualizer = Virtualizer::new(options);
        assert_eq!(virtualizer.total_size(), 5_000_000.0);

        // At scroll_offset = 0: items 0..10 are visible + 2 overscan = items 0..12
        let items = virtualizer.get_virtual_items();
        assert_eq!(items.len(), 13);
        assert_eq!(items[0].index, 0);

        // Scroll to 2500px (item index 50)
        virtualizer.set_scroll_offset(2500.0);
        let items = virtualizer.get_virtual_items();
        // Visible: 50..60, with overscan 2: 48..63
        assert_eq!(items[0].index, 48);
        assert_eq!(items.last().unwrap().index, 62);
    }

    #[test]
    fn test_dynamic_measurements_and_scroll_to() {
        let options = VirtualOptions {
            count: 100,
            estimate_size: 40.0,
            overscan: 1,
            viewport_size: 200.0,
        };

        let mut virtualizer = Virtualizer::new(options);
        // Custom size for item 0
        virtualizer.measure_item(0, 100.0);
        assert_eq!(virtualizer.get_item_size(0), 100.0);
        assert_eq!(virtualizer.total_size(), 100.0 + 99.0 * 40.0);

        // Scroll to item 10 at start
        let offset = virtualizer.scroll_to_index(10, ScrollAlign::Start);
        // item 0 is 100, items 1..10 are 9 * 40 = 360 -> total 460
        assert_eq!(offset, 460.0);
    }
}
