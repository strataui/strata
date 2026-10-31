//! Strata Table: Headless Data Grid & Table Engine.
//!
//! Provides a zero-DOM, pure-Rust equivalent to TanStack Table (React Table).
//! Features declarative column accessors, multi-column filtering, multi-directional sorting,
//! pagination windowing, and row selection.

pub mod column;
pub mod table;

pub use column::{ColumnDef, ColumnPin, SortDirection};
pub use table::{use_table, Table};

pub mod prelude {
    pub use crate::column::{ColumnDef, ColumnPin, SortDirection};
    pub use crate::table::{use_table, Table};
}
