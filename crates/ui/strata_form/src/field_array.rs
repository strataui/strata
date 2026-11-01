//! Dynamic Repeating Field Arrays (use_field_array).

use strata_dom::reactive::Signal;

/// Reactive dynamic array field controller.
#[derive(Clone)]
pub struct FieldArray<T: Clone + 'static> {
    items: Signal<Vec<T>>,
}

impl<T: Clone + 'static> FieldArray<T> {
    pub fn new(initial: Vec<T>) -> Self {
        Self {
            items: Signal::new(initial),
        }
    }

    /// Reactive list of items.
    pub fn items(&self) -> Signal<Vec<T>> {
        self.items.clone()
    }

    /// Append item to the end of the array.
    pub fn append(&self, item: T) {
        self.items.update(|list| list.push(item));
    }

    /// Prepend item to the start of the array.
    pub fn prepend(&self, item: T) {
        self.items.update(|list| list.insert(0, item));
    }

    /// Remove item at specified index.
    pub fn remove(&self, index: usize) -> Option<T> {
        self.items.update(|list| {
            if index < list.len() {
                Some(list.remove(index))
            } else {
                None
            }
        })
    }

    /// Swap two items by indices.
    pub fn swap(&self, a: usize, b: usize) {
        self.items.update(|list| {
            if a < list.len() && b < list.len() {
                list.swap(a, b);
            }
        });
    }

    /// Insert item at specified index.
    pub fn insert(&self, index: usize, item: T) {
        self.items.update(|list| {
            if index <= list.len() {
                list.insert(index, item);
            } else {
                list.push(item);
            }
        });
    }

    /// Clear all items.
    pub fn clear(&self) {
        self.items.set(Vec::new());
    }

    /// Item count.
    pub fn len(&self) -> usize {
        self.items.get().len()
    }

    /// Whether the array is empty.
    pub fn is_empty(&self) -> bool {
        self.items.get().is_empty()
    }
}

/// Hook to initialize a dynamic reactive FieldArray.
pub fn use_field_array<T: Clone + 'static>(initial: Vec<T>) -> FieldArray<T> {
    FieldArray::new(initial)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_field_array_mutations() {
        let tags = use_field_array(vec!["rust".to_string(), "compiler".to_string()]);
        assert_eq!(tags.len(), 2);

        tags.append("wasm".to_string());
        assert_eq!(tags.len(), 3);
        assert_eq!(tags.items().get()[2], "wasm");

        tags.swap(0, 2);
        assert_eq!(tags.items().get()[0], "wasm");
        assert_eq!(tags.items().get()[2], "rust");

        let removed = tags.remove(1);
        assert_eq!(removed.unwrap(), "compiler");
        assert_eq!(tags.len(), 2);
    }
}
