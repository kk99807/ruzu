//! Project operator for RETURN clauses.

use crate::error::Result;
use crate::executor::PhysicalOperator;
use crate::types::Row;

/// Project operator for column projection.
pub struct ProjectOperator {
    child: Box<dyn PhysicalOperator>,
    /// Projections as (var, property) tuples
    projections: Vec<(String, String)>,
}

impl ProjectOperator {
    /// Creates a new project operator with the given child and projections.
    #[must_use]
    pub fn new(child: Box<dyn PhysicalOperator>, projections: Vec<(String, String)>) -> Self {
        ProjectOperator { child, projections }
    }

    /// Returns the column names that will be in the output.
    #[must_use]
    pub fn output_columns(&self) -> Vec<String> {
        self.projections
            .iter()
            .map(|(var, prop)| format!("{var}.{prop}"))
            .collect()
    }
}

impl PhysicalOperator for ProjectOperator {
    fn next(&mut self) -> Result<Option<Row>> {
        if let Some(input_row) = self.child.next()? {
            let mut output_row = Row::new();

            for (var, prop) in &self.projections {
                let column_name = format!("{var}.{prop}");
                if let Some(value) = input_row.get(&column_name) {
                    output_row.set(column_name, value.clone());
                }
            }

            Ok(Some(output_row))
        } else {
            Ok(None)
        }
    }
}
