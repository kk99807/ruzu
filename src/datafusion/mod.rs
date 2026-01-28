//! DataFusion integration module.
//!
//! This module provides integration with Apache DataFusion for query execution.
//! It implements:
//! - TableProvider trait for NodeTable and RelTable
//! - Custom graph operators (ExtendExec, PathExpandExec)
//! - Expression conversion from Cypher to DataFusion

pub mod cypher_to_df;
pub mod table_provider;

pub use cypher_to_df::CypherToDf;
pub use table_provider::{NodeTableProvider, RelTableProvider};
