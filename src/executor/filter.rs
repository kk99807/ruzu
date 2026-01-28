//! Filter operator for WHERE clauses.

use crate::error::Result;
use crate::executor::{evaluate_expression, PhysicalOperator};
use crate::parser::ast::Expression;
use crate::types::Row;

/// Filter operator for WHERE clause evaluation.
pub struct FilterOperator {
    child: Box<dyn PhysicalOperator>,
    predicate: Expression,
}

impl FilterOperator {
    /// Creates a new filter operator with the given child and predicate.
    #[must_use]
    pub fn new(child: Box<dyn PhysicalOperator>, predicate: Expression) -> Self {
        FilterOperator { child, predicate }
    }
}

impl PhysicalOperator for FilterOperator {
    fn next(&mut self) -> Result<Option<Row>> {
        while let Some(row) = self.child.next()? {
            if evaluate_expression(&self.predicate, &row)? {
                return Ok(Some(row));
            }
        }
        Ok(None)
    }
}
