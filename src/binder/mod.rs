//! Binder module for semantic analysis.
//!
//! The binder performs semantic analysis on parsed AST, resolving:
//! - Table and column names against the catalog
//! - Variable scoping and type inference
//! - Expression type checking
//!
//! The output is a bound query graph ready for planning.

mod expression;
mod query_graph;
mod scope;
mod semantic;

pub use expression::{AggregateFunction, ArithmeticOp, BoundExpression, ComparisonOp, LogicalOp};
pub use query_graph::{BoundNode, BoundRelationship, Direction, QueryGraph};
pub use scope::{BinderScope, BoundVariable, VariableType};
pub use semantic::{BindError, Binder, BoundQuery, BoundReturn, BoundStatement};
