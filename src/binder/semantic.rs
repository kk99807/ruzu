//! Semantic analysis and binding.

use crate::catalog::Catalog;
use crate::error::{Result, RuzuError};
use crate::types::DataType;

use super::expression::BoundExpression;
use super::query_graph::{BoundNode, BoundRelationship, Direction, QueryGraph};
use super::scope::{BinderScope, BoundVariable};

/// Errors that can occur during binding.
#[derive(Debug, Clone)]
pub enum BindError {
    /// Referenced an undefined variable.
    UndefinedVariable(String),
    /// Referenced an undefined table.
    UndefinedTable(String),
    /// Referenced an undefined column.
    UndefinedColumn(String, String),
    /// Type mismatch in expression.
    TypeMismatch {
        expected: DataType,
        actual: DataType,
    },
    /// Aggregate function in WHERE clause.
    AggregateInWhereClause,
    /// Duplicate variable name.
    DuplicateVariable(String),
    /// Invalid path bounds.
    InvalidPathBounds { min: usize, max: usize },
    /// Invalid property access on non-node/rel variable.
    InvalidPropertyAccess(String, String),
}

impl std::fmt::Display for BindError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BindError::UndefinedVariable(name) => write!(f, "Undefined variable: {name}"),
            BindError::UndefinedTable(name) => write!(f, "Undefined table: {name}"),
            BindError::UndefinedColumn(table, col) => {
                write!(f, "Undefined column '{col}' in table '{table}'")
            }
            BindError::TypeMismatch { expected, actual } => {
                write!(f, "Type mismatch: expected {expected:?}, got {actual:?}")
            }
            BindError::AggregateInWhereClause => {
                write!(f, "Aggregate functions not allowed in WHERE clause")
            }
            BindError::DuplicateVariable(name) => write!(f, "Duplicate variable: {name}"),
            BindError::InvalidPathBounds { min, max } => {
                write!(f, "Invalid path bounds: min {min} > max {max}")
            }
            BindError::InvalidPropertyAccess(var, prop) => {
                write!(f, "Invalid property access: {var}.{prop}")
            }
        }
    }
}

impl std::error::Error for BindError {}

impl From<BindError> for RuzuError {
    fn from(err: BindError) -> Self {
        RuzuError::BindError(err.to_string())
    }
}

/// Bound statement after semantic analysis.
#[derive(Debug)]
pub enum BoundStatement {
    /// Bound query.
    Query(BoundQuery),
}

/// Bound query with all components.
#[derive(Debug)]
pub struct BoundQuery {
    /// Bound MATCH pattern.
    pub query_graph: QueryGraph,
    /// Bound WHERE clause.
    pub where_clause: Option<BoundExpression>,
    /// Bound RETURN clause.
    pub return_clause: BoundReturn,
    /// Bound ORDER BY.
    pub order_by: Option<Vec<SortExpr>>,
    /// SKIP amount.
    pub skip: Option<usize>,
    /// LIMIT amount.
    pub limit: Option<usize>,
}

impl BoundQuery {
    /// Creates a new bound query.
    #[must_use]
    pub fn new(query_graph: QueryGraph, return_clause: BoundReturn) -> Self {
        BoundQuery {
            query_graph,
            where_clause: None,
            return_clause,
            order_by: None,
            skip: None,
            limit: None,
        }
    }

    /// Sets the WHERE clause.
    #[must_use]
    pub fn with_where(mut self, where_clause: BoundExpression) -> Self {
        self.where_clause = Some(where_clause);
        self
    }

    /// Sets the ORDER BY clause.
    #[must_use]
    pub fn with_order_by(mut self, order_by: Vec<SortExpr>) -> Self {
        self.order_by = Some(order_by);
        self
    }

    /// Sets the SKIP amount.
    #[must_use]
    pub fn with_skip(mut self, skip: usize) -> Self {
        self.skip = Some(skip);
        self
    }

    /// Sets the LIMIT amount.
    #[must_use]
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }
}

/// Sort expression for ORDER BY.
#[derive(Debug, Clone)]
pub struct SortExpr {
    /// The expression to sort by.
    pub expr: BoundExpression,
    /// Sort ascending (true) or descending (false).
    pub ascending: bool,
    /// NULLs first (true) or last (false).
    pub nulls_first: bool,
}

impl SortExpr {
    /// Creates a new ascending sort expression.
    #[must_use]
    pub fn asc(expr: BoundExpression) -> Self {
        SortExpr {
            expr,
            ascending: true,
            nulls_first: false,
        }
    }

    /// Creates a new descending sort expression.
    #[must_use]
    pub fn desc(expr: BoundExpression) -> Self {
        SortExpr {
            expr,
            ascending: false,
            nulls_first: false,
        }
    }
}

/// Bound RETURN clause.
#[derive(Debug)]
pub struct BoundReturn {
    /// (alias, expression) pairs.
    pub projections: Vec<(String, BoundExpression)>,
    /// Is RETURN DISTINCT?
    pub distinct: bool,
    /// GROUP BY columns (extracted from aggregates).
    pub group_by: Vec<BoundExpression>,
}

impl BoundReturn {
    /// Creates a new bound return clause.
    #[must_use]
    pub fn new(projections: Vec<(String, BoundExpression)>) -> Self {
        BoundReturn {
            projections,
            distinct: false,
            group_by: Vec::new(),
        }
    }

    /// Sets the DISTINCT flag.
    #[must_use]
    pub fn with_distinct(mut self) -> Self {
        self.distinct = true;
        self
    }

    /// Sets the GROUP BY columns.
    #[must_use]
    pub fn with_group_by(mut self, group_by: Vec<BoundExpression>) -> Self {
        self.group_by = group_by;
        self
    }
}

/// Main binder for semantic analysis.
pub struct Binder<'a> {
    /// Reference to database catalog.
    catalog: &'a Catalog,
    /// Current variable scope.
    scope: BinderScope,
}

impl<'a> Binder<'a> {
    /// Creates a new binder with the given catalog.
    #[must_use]
    pub fn new(catalog: &'a Catalog) -> Self {
        Binder {
            catalog,
            scope: BinderScope::new(),
        }
    }

    /// Returns a reference to the current scope.
    #[must_use]
    pub fn scope(&self) -> &BinderScope {
        &self.scope
    }

    /// Returns a mutable reference to the current scope.
    pub fn scope_mut(&mut self) -> &mut BinderScope {
        &mut self.scope
    }

    /// Binds a node pattern, returning a bound node.
    ///
    /// # Errors
    ///
    /// Returns an error if the variable is already defined in scope or the
    /// table label does not exist in the catalog.
    pub fn bind_node(&mut self, variable: &str, label: &str) -> Result<BoundNode> {
        // Check for duplicate variable
        if self.scope.contains(variable) {
            return Err(BindError::DuplicateVariable(variable.to_string()).into());
        }

        // Look up table schema
        let schema = self.catalog.get_table(label).ok_or_else(|| {
            RuzuError::from(BindError::UndefinedTable(label.to_string()))
        })?;

        // Add variable to scope
        let bound_var = BoundVariable::node(variable.to_string(), schema.clone());
        self.scope.add_variable(bound_var);

        Ok(BoundNode::new(variable.to_string(), schema))
    }

    /// Binds a relationship pattern, returning a bound relationship.
    ///
    /// # Errors
    ///
    /// Returns an error if the variable is already defined, the source or
    /// destination variable is undefined, or the relationship type does not
    /// exist in the catalog.
    pub fn bind_relationship(
        &mut self,
        variable: Option<&str>,
        rel_type: &str,
        src_variable: &str,
        dst_variable: &str,
        direction: Direction,
    ) -> Result<BoundRelationship> {
        // Check for duplicate variable if present
        if let Some(var) = variable {
            if self.scope.contains(var) {
                return Err(BindError::DuplicateVariable(var.to_string()).into());
            }
        }

        // Validate source and destination variables exist
        if !self.scope.contains(src_variable) {
            return Err(BindError::UndefinedVariable(src_variable.to_string()).into());
        }
        if !self.scope.contains(dst_variable) {
            return Err(BindError::UndefinedVariable(dst_variable.to_string()).into());
        }

        // Look up relationship schema
        let rel_schema = self.catalog.get_rel_table(rel_type).ok_or_else(|| {
            RuzuError::from(BindError::UndefinedTable(rel_type.to_string()))
        })?;

        // Add variable to scope if present
        if let Some(var) = variable {
            let bound_var = BoundVariable::relationship(var.to_string(), DataType::Int64);
            self.scope.add_variable(bound_var);
        }

        Ok(BoundRelationship::new(
            variable.map(String::from),
            rel_schema,
            src_variable.to_string(),
            dst_variable.to_string(),
            direction,
        ))
    }

    /// Validates that a variable exists in scope.
    ///
    /// # Errors
    ///
    /// Returns an error if the variable is not defined in the current scope.
    pub fn validate_variable(&self, name: &str) -> Result<&BoundVariable> {
        self.scope
            .lookup(name)
            .ok_or_else(|| BindError::UndefinedVariable(name.to_string()).into())
    }

    /// Validates that a property exists on a variable's table schema.
    ///
    /// # Errors
    ///
    /// Returns an error if the variable is undefined or the property does not
    /// exist on the variable's table schema.
    pub fn validate_property(&self, variable: &str, property: &str) -> Result<DataType> {
        let var = self.validate_variable(variable)?;

        // Get schema to look up property type
        if let Some(schema) = &var.schema {
            for col in &schema.columns {
                if col.name == property {
                    return Ok(col.data_type);
                }
            }
            return Err(BindError::UndefinedColumn(variable.to_string(), property.to_string()).into());
        }

        Err(BindError::InvalidPropertyAccess(variable.to_string(), property.to_string()).into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bind_error_display() {
        let err = BindError::UndefinedVariable("foo".to_string());
        assert_eq!(err.to_string(), "Undefined variable: foo");

        let err = BindError::UndefinedTable("Person".to_string());
        assert_eq!(err.to_string(), "Undefined table: Person");

        let err = BindError::TypeMismatch {
            expected: DataType::Int64,
            actual: DataType::String,
        };
        assert!(err.to_string().contains("Type mismatch"));
    }
}
