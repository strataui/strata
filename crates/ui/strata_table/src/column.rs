//! Column Definitions and Configuration for Headless Data Grid.

use std::rc::Rc;

/// Sort order direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDirection {
    Asc,
    Desc,
    None,
}

impl SortDirection {
    pub fn next(&self) -> Self {
        match self {
            SortDirection::None => SortDirection::Asc,
            SortDirection::Asc => SortDirection::Desc,
            SortDirection::Desc => SortDirection::None,
        }
    }
}

/// Column horizontal pinning alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnPin {
    Left,
    Right,
    None,
}

/// Definition of a single table column with data extraction accessor.
#[derive(Clone)]
pub struct ColumnDef<T> {
    pub id: String,
    pub header: String,
    pub accessor: Rc<dyn Fn(&T) -> String>,
    pub sortable: bool,
    pub filterable: bool,
    pub pin: ColumnPin,
}

impl<T> ColumnDef<T> {
    pub fn new(id: impl Into<String>, header: impl Into<String>, accessor: impl Fn(&T) -> String + 'static) -> Self {
        Self {
            id: id.into(),
            header: header.into(),
            accessor: Rc::new(accessor),
            sortable: true,
            filterable: true,
            pin: ColumnPin::None,
        }
    }

    pub fn with_pin(mut self, pin: ColumnPin) -> Self {
        self.pin = pin;
        self
    }

    pub fn sortable(mut self, sortable: bool) -> Self {
        self.sortable = sortable;
        self
    }

    pub fn filterable(mut self, filterable: bool) -> Self {
        self.filterable = filterable;
        self
    }
}
