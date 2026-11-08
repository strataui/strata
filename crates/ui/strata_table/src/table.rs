//! Headless Reactive Table State Machine.

use strata_dom::Signal;
use crate::column::{ColumnDef, SortDirection};

/// Headless table controller orchestrating sorting, filtering, pagination, and row selection.
#[derive(Clone)]
pub struct Table<T: Clone + 'static> {
    pub data: Signal<Vec<T>>,
    pub columns: Vec<ColumnDef<T>>,
    pub sorting: Signal<Option<(String, SortDirection)>>,
    pub filters: Signal<Vec<(String, String)>>,
    pub page_index: Signal<usize>,
    pub page_size: Signal<usize>,
    pub selected_indices: Signal<Vec<usize>>,
}

impl<T: Clone + 'static> Table<T> {
    /// Creates a new table instance with initial data and column definitions.
    pub fn new(data: Vec<T>, columns: Vec<ColumnDef<T>>) -> Self {
        Self {
            data: Signal::new(data),
            columns,
            sorting: Signal::new(None),
            filters: Signal::new(Vec::new()),
            page_index: Signal::new(0),
            page_size: Signal::new(10),
            selected_indices: Signal::new(Vec::new()),
        }
    }

    /// Toggles the sort state of a column through Asc -> Desc -> None.
    pub fn toggle_sort(&self, column_id: &str) {
        let current = self.sorting.get();
        let next_direction = match current {
            Some((ref id, dir)) if id == column_id => dir.next(),
            _ => SortDirection::Asc,
        };

        if next_direction == SortDirection::None {
            self.sorting.set(None);
        } else {
            self.sorting.set(Some((column_id.to_string(), next_direction)));
        }
    }

    /// Sets a filter query on a specific column.
    pub fn set_filter(&self, column_id: &str, query: impl Into<String>) {
        let q = query.into();
        let mut filters = self.filters.get();
        filters.retain(|(id, _)| id != column_id);
        if !q.is_empty() {
            filters.push((column_id.to_string(), q));
        }
        self.filters.set(filters);
        self.page_index.set(0); // Reset to first page
    }

    /// Toggle selection for a row index.
    pub fn toggle_row(&self, index: usize) {
        let mut sel = self.selected_indices.get();
        if let Some(pos) = sel.iter().position(|&i| i == index) {
            sel.remove(pos);
        } else {
            sel.push(index);
        }
        self.selected_indices.set(sel);
    }

    /// Check if a row index is selected.
    pub fn is_selected(&self, index: usize) -> bool {
        self.selected_indices.get().contains(&index)
    }

    /// Select all rows.
    pub fn select_all(&self) {
        let count = self.data.get().len();
        self.selected_indices.set((0..count).collect());
    }

    /// Deselect all rows.
    pub fn deselect_all(&self) {
        self.selected_indices.set(Vec::new());
    }

    /// Advance to next page.
    pub fn next_page(&self) {
        let max = self.total_pages();
        let cur = self.page_index.get();
        if cur + 1 < max {
            self.page_index.set(cur + 1);
        }
    }

    /// Step back to previous page.
    pub fn prev_page(&self) {
        let cur = self.page_index.get();
        if cur > 0 {
            self.page_index.set(cur - 1);
        }
    }

    /// Calculates total number of pages according to filtered row count.
    pub fn total_pages(&self) -> usize {
        let count = self.filtered_rows().len();
        let size = self.page_size.get().max(1);
        count.div_ceil(size)
    }

    /// Computes filtered rows without pagination.
    fn filtered_rows(&self) -> Vec<(usize, T)> {
        let all_rows = self.data.get();
        let filters = self.filters.get();

        all_rows
            .into_iter()
            .enumerate()
            .filter(|(_, row)| {
                for (col_id, query) in &filters {
                    if let Some(col) = self.columns.iter().find(|c| &c.id == col_id) {
                        let val = (col.accessor)(row);
                        if !val.to_lowercase().contains(&query.to_lowercase()) {
                            return false;
                        }
                    }
                }
                true
            })
            .collect()
    }

    /// Returns the currently visible rows after filtering, sorting, and pagination.
    pub fn rows(&self) -> Vec<(usize, T)> {
        let mut rows = self.filtered_rows();

        // Apply sorting
        if let Some((ref col_id, dir)) = self.sorting.get() {
            if let Some(col) = self.columns.iter().find(|c| &c.id == col_id) {
                let accessor = col.accessor.clone();
                rows.sort_by(|(_, a), (_, b)| {
                    let va = (accessor)(a);
                    let vb = (accessor)(b);
                    match dir {
                        SortDirection::Asc => va.cmp(&vb),
                        SortDirection::Desc => vb.cmp(&va),
                        SortDirection::None => std::cmp::Ordering::Equal,
                    }
                });
            }
        }

        // Apply pagination
        let page = self.page_index.get();
        let size = self.page_size.get();
        let start = page * size;
        let end = (start + size).min(rows.len());

        if start >= rows.len() {
            Vec::new()
        } else {
            rows[start..end].to_vec()
        }
    }
}

/// Convenience hook to create a headless table controller.
pub fn use_table<T: Clone + 'static>(data: Vec<T>, columns: Vec<ColumnDef<T>>) -> Table<T> {
    Table::new(data, columns)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    struct User {
        id: usize,
        name: String,
        role: String,
    }

    #[test]
    fn test_table_sorting_filtering_pagination() {
        let data = vec![
            User { id: 1, name: "Charlie".to_string(), role: "Admin".to_string() },
            User { id: 2, name: "Alice".to_string(), role: "Engineer".to_string() },
            User { id: 3, name: "Bob".to_string(), role: "Engineer".to_string() },
            User { id: 4, name: "David".to_string(), role: "Designer".to_string() },
        ];

        let columns = vec![
            ColumnDef::new("name", "Name", |u: &User| u.name.clone()),
            ColumnDef::new("role", "Role", |u: &User| u.role.clone()),
        ];

        let table = use_table(data, columns);
        table.page_size.set(2);

        // Initial 2 rows
        let rows = table.rows();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].1.name, "Charlie");

        // Sort by Name Ascending
        table.toggle_sort("name");
        let sorted = table.rows();
        assert_eq!(sorted[0].1.name, "Alice");
        assert_eq!(sorted[1].1.name, "Bob");

        // Filter by role "Engineer"
        table.set_filter("role", "Engineer");
        let filtered = table.rows();
        assert_eq!(filtered.len(), 2); // Alice & Bob
        assert_eq!(filtered[0].1.name, "Alice");
        assert_eq!(filtered[1].1.name, "Bob");

        // Selection
        table.toggle_row(filtered[0].0);
        assert!(table.is_selected(filtered[0].0));
    }
}
