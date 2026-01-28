//! Extend operator for relationship traversal.
//!
//! The Extend operator performs single-hop graph traversal by looking up
//! edges in the CSR (Compressed Sparse Row) index.

use std::sync::Arc;

use crate::catalog::{Direction, RelTableSchema};
use crate::error::{Result, RuzuError};
use crate::storage::RelTable;
use crate::types::{Row, Value};

use super::PhysicalOperator;

/// Extend operator for graph traversal.
pub struct ExtendOperator {
    /// Input operator producing source rows.
    input: Box<dyn PhysicalOperator>,
    /// Relationship table to traverse.
    rel_table: Arc<RelTable>,
    /// Schema for the relationship table.
    rel_schema: Arc<RelTableSchema>,
    /// Variable name for the source node.
    src_variable: String,
    /// Variable name for the destination node.
    dst_variable: String,
    /// Optional variable name for the relationship.
    rel_variable: Option<String>,
    /// Direction of traversal.
    direction: Direction,
    /// Current input row being processed.
    current_input_row: Option<Row>,
    /// Current edges for the current input row.
    current_edges: Vec<(u64, u64)>,
    /// Index into current_edges.
    edge_index: usize,
}

impl ExtendOperator {
    /// Creates a new extend operator.
    #[must_use]
    pub fn new(
        input: Box<dyn PhysicalOperator>,
        rel_table: Arc<RelTable>,
        rel_schema: Arc<RelTableSchema>,
        src_variable: String,
        dst_variable: String,
        rel_variable: Option<String>,
        direction: Direction,
    ) -> Self {
        Self {
            input,
            rel_table,
            rel_schema,
            src_variable,
            dst_variable,
            rel_variable,
            direction,
            current_input_row: None,
            current_edges: Vec::new(),
            edge_index: 0,
        }
    }

    fn get_src_node_id(&self, row: &Row) -> Result<u64> {
        let id_col = format!("{}._id", self.src_variable);
        if let Some(Value::Int64(id)) = row.get(&id_col) {
            return Ok(*id as u64);
        }

        let id_col = format!("{}.id", self.src_variable);
        if let Some(Value::Int64(id)) = row.get(&id_col) {
            return Ok(*id as u64);
        }

        for (key, value) in row.iter() {
            if key.starts_with(&self.src_variable) && key.ends_with(".id") {
                if let Value::Int64(id) = value {
                    return Ok(*id as u64);
                }
            }
        }

        Err(RuzuError::ExecutionError(format!(
            "Could not find node ID for variable {} in row",
            self.src_variable
        )))
    }

    fn fetch_edges(&self, node_id: u64) -> Vec<(u64, u64)> {
        match self.direction {
            Direction::Forward => self.rel_table.get_forward_edges(node_id),
            Direction::Backward => self.rel_table.get_backward_edges(node_id),
            Direction::Both => {
                let mut edges = self.rel_table.get_forward_edges(node_id);
                edges.extend(self.rel_table.get_backward_edges(node_id));
                edges
            }
        }
    }

    fn create_output_row(&self, input_row: &Row, dst_node_id: u64, rel_id: u64) -> Row {
        let mut output = input_row.clone();
        let dst_id_col = format!("{}._id", self.dst_variable);
        output.insert(dst_id_col, Value::Int64(dst_node_id as i64));

        if let Some(ref rel_var) = self.rel_variable {
            let rel_id_col = format!("{}._id", rel_var);
            output.insert(rel_id_col, Value::Int64(rel_id as i64));

            if let Some(props) = self.rel_table.get_properties(rel_id) {
                for (idx, col) in self.rel_schema.columns.iter().enumerate() {
                    if idx < props.len() {
                        let prop_col = format!("{}.{}", rel_var, col.name);
                        output.insert(prop_col, props[idx].clone());
                    }
                }
            }
        }
        output
    }
}

impl PhysicalOperator for ExtendOperator {
    fn next(&mut self) -> Result<Option<Row>> {
        loop {
            if self.edge_index < self.current_edges.len() {
                let (dst_node_id, rel_id) = self.current_edges[self.edge_index];
                self.edge_index += 1;

                if let Some(ref input_row) = self.current_input_row {
                    return Ok(Some(self.create_output_row(input_row, dst_node_id, rel_id)));
                }
            }

            match self.input.next()? {
                Some(input_row) => {
                    let src_node_id = self.get_src_node_id(&input_row)?;
                    self.current_edges = self.fetch_edges(src_node_id);
                    self.edge_index = 0;
                    self.current_input_row = Some(input_row);
                }
                None => {
                    return Ok(None);
                }
            }
        }
    }
}
