//! Table scan operator.

use std::sync::Arc;

use crate::error::Result;
use crate::executor::PhysicalOperator;
use crate::storage::NodeTable;
use crate::types::Row;

/// Scan operator for full table scans.
pub struct ScanOperator {
    table: Arc<NodeTable>,
    variable: String,
    cursor: usize,
}

impl ScanOperator {
    /// Creates a new scan operator for the given table.
    #[must_use]
    pub fn new(table: Arc<NodeTable>, variable: String) -> Self {
        ScanOperator {
            table,
            variable,
            cursor: 0,
        }
    }
}

impl PhysicalOperator for ScanOperator {
    fn next(&mut self) -> Result<Option<Row>> {
        if self.cursor >= self.table.row_count() {
            return Ok(None);
        }

        let mut row = Row::new();
        let schema = self.table.schema();

        // Build the row with fully qualified column names (var.column)
        for (col_idx, col_def) in schema.columns.iter().enumerate() {
            if let Some(column) = self.table.get_column(col_idx) {
                if let Some(value) = column.get(self.cursor) {
                    let full_name = format!("{}.{}", self.variable, col_def.name);
                    row.set(full_name, value.clone());
                }
            }
        }

        self.cursor += 1;
        Ok(Some(row))
    }
}
