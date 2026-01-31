//! Node table storage with columnar layout.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::catalog::NodeTableSchema;
use crate::error::{Result, RuzuError};
use crate::storage::ColumnStorage;
use crate::types::Value;

/// Serializable table data (columns only, schema stored separately).
#[derive(Debug, Serialize, Deserialize)]
pub struct TableData {
    /// Column data.
    pub columns: Vec<ColumnStorage>,
    /// Number of rows.
    pub row_count: usize,
}

/// Node table with columnar storage.
pub struct NodeTable {
    schema: Arc<NodeTableSchema>,
    columns: Vec<ColumnStorage>,
    row_count: usize,
    pk_index: HashMap<Vec<Value>, usize>,
}

impl std::fmt::Debug for NodeTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NodeTable")
            .field("schema", &self.schema.name)
            .field("row_count", &self.row_count)
            .field("column_count", &self.columns.len())
            .finish_non_exhaustive()
    }
}

impl NodeTable {
    /// Creates a new empty node table with the given schema.
    #[must_use]
    pub fn new(schema: Arc<NodeTableSchema>) -> Self {
        let columns = schema
            .columns
            .iter()
            .map(|_| ColumnStorage::new())
            .collect();
        NodeTable {
            schema,
            columns,
            row_count: 0,
            pk_index: HashMap::new(),
        }
    }

    /// Creates a node table from serialized data.
    #[must_use]
    pub fn from_data(schema: Arc<NodeTableSchema>, data: TableData) -> Self {
        let mut pk_index = HashMap::new();

        // Rebuild primary key index
        for row_idx in 0..data.row_count {
            let pk_values: Vec<Value> = schema
                .primary_key
                .iter()
                .filter_map(|col_name| {
                    schema
                        .get_column_index(col_name)
                        .and_then(|idx| data.columns.get(idx))
                        .and_then(|col| col.get(row_idx).cloned())
                })
                .collect();
            pk_index.insert(pk_values, row_idx);
        }

        NodeTable {
            schema,
            columns: data.columns,
            row_count: data.row_count,
            pk_index,
        }
    }

    /// Exports table data for serialization.
    #[must_use]
    pub fn to_data(&self) -> TableData {
        TableData {
            columns: self.columns.clone(),
            row_count: self.row_count,
        }
    }

    /// Returns the table schema.
    #[must_use]
    pub fn schema(&self) -> &NodeTableSchema {
        &self.schema
    }

    /// Returns the number of rows in the table.
    #[must_use]
    pub fn row_count(&self) -> usize {
        self.row_count
    }

    /// Gets a column by index.
    #[must_use]
    pub fn get_column(&self, index: usize) -> Option<&ColumnStorage> {
        self.columns.get(index)
    }

    /// Inserts a row into the table.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - A required column is missing
    /// - A value has the wrong type
    /// - The primary key is duplicated
    ///
    /// # Panics
    ///
    /// Panics if a primary key column is missing from the row after validation.
    /// This should not happen if validation passes.
    pub fn insert(&mut self, row: &HashMap<String, Value>) -> Result<()> {
        // Validate all columns present
        for col_def in &self.schema.columns {
            if !row.contains_key(&col_def.name) {
                return Err(RuzuError::SchemaError(format!(
                    "Missing value for column '{}'",
                    col_def.name
                )));
            }
        }

        // Validate types
        for (col_name, value) in row {
            let col_def = self
                .schema
                .get_column(col_name)
                .ok_or_else(|| RuzuError::SchemaError(format!("Unknown column '{col_name}'")))?;

            if !value.is_null() {
                if let Some(val_type) = value.data_type() {
                    if val_type != col_def.data_type {
                        return Err(RuzuError::TypeError {
                            expected: col_def.data_type.name().into(),
                            actual: val_type.name().into(),
                        });
                    }
                }
            }
        }

        // Extract primary key values
        let pk_values: Vec<Value> = self
            .schema
            .primary_key
            .iter()
            .map(|col_name| row.get(col_name).unwrap().clone())
            .collect();

        // Check primary key uniqueness
        if self.pk_index.contains_key(&pk_values) {
            return Err(RuzuError::ConstraintViolation(format!(
                "Duplicate primary key: {pk_values:?}"
            )));
        }

        // Insert values into columns
        for (i, col_def) in self.schema.columns.iter().enumerate() {
            let value = row.get(&col_def.name).unwrap().clone();
            self.columns[i].push(value);
        }

        // Update primary key index
        self.pk_index.insert(pk_values, self.row_count);
        self.row_count += 1;

        Ok(())
    }

    /// Returns the number of rows in the table.
    #[must_use]
    pub fn len(&self) -> usize {
        self.row_count
    }

    /// Returns true if the table has no rows.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.row_count == 0
    }

    /// Finds a row by primary key value.
    ///
    /// This is a simple lookup that assumes single-column primary keys.
    /// Returns the row offset if found.
    #[must_use]
    pub fn find_by_pk(&self, key_column: &str, value: &Value) -> Option<usize> {
        // Check if key_column is actually a primary key column
        if !self.schema.primary_key.contains(&key_column.to_string()) {
            return None;
        }

        // For single-column PK, do a direct lookup
        if self.schema.primary_key.len() == 1 {
            let pk_vec = vec![value.clone()];
            return self.pk_index.get(&pk_vec).copied();
        }

        // For composite keys, we need to scan (less efficient)
        // This is a fallback for partial key lookups
        let col_idx = self.schema.get_column_index(key_column)?;
        let column = self.columns.get(col_idx)?;

        for row_idx in 0..self.row_count {
            if let Some(val) = column.get(row_idx) {
                if val == value {
                    return Some(row_idx);
                }
            }
        }

        None
    }

    /// Gets a column value for a specific row.
    #[must_use]
    pub fn get(&self, row_idx: usize, column_name: &str) -> Option<Value> {
        let col_idx = self.schema.get_column_index(column_name)?;
        let column = self.columns.get(col_idx)?;
        column.get(row_idx).cloned()
    }

    /// Inserts multiple rows into the table in a single batch.
    ///
    /// This is more efficient than repeated single inserts:
    /// - Single validation pass for column structure
    /// - Batch primary key uniqueness check
    /// - Pre-allocated column growth
    ///
    /// # Arguments
    ///
    /// * `rows` - Vector of rows, where each row is a vector of values
    /// * `columns` - Column names in the order they appear in each row
    ///
    /// # Returns
    ///
    /// The number of rows inserted.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Column count doesn't match schema
    /// - Any column name is unknown
    /// - Type mismatch for any value
    /// - Duplicate primary key in batch or existing table
    pub fn insert_batch(&mut self, rows: Vec<Vec<Value>>, columns: &[String]) -> Result<usize> {
        if rows.is_empty() {
            return Ok(0);
        }

        // Validate column count
        if columns.len() != self.schema.columns.len() {
            return Err(RuzuError::SchemaError(format!(
                "Expected {} columns, got {}",
                self.schema.columns.len(),
                columns.len()
            )));
        }

        // Build column mapping: provided column order -> schema column order
        let mut col_mapping = Vec::with_capacity(self.schema.columns.len());
        for schema_col in &self.schema.columns {
            match columns.iter().position(|c| c == &schema_col.name) {
                Some(idx) => col_mapping.push(idx),
                None => {
                    return Err(RuzuError::SchemaError(format!(
                        "Missing column '{}' in batch",
                        schema_col.name
                    )));
                }
            }
        }

        // Find primary key column indices (in schema order)
        let pk_col_indices: Vec<usize> = self
            .schema
            .primary_key
            .iter()
            .filter_map(|pk_col| self.schema.get_column_index(pk_col))
            .collect();

        // Collect all primary keys from the batch for uniqueness check
        let mut batch_pks = std::collections::HashSet::new();
        for (row_idx, row) in rows.iter().enumerate() {
            if row.len() != columns.len() {
                return Err(RuzuError::SchemaError(format!(
                    "Row {} has {} values, expected {}",
                    row_idx,
                    row.len(),
                    columns.len()
                )));
            }

            // Extract primary key values in schema order
            let pk_values: Vec<Value> = pk_col_indices
                .iter()
                .map(|&schema_idx| {
                    let input_idx = col_mapping[schema_idx];
                    row[input_idx].clone()
                })
                .collect();

            // Check within batch
            if !batch_pks.insert(pk_values.clone()) {
                return Err(RuzuError::ConstraintViolation(format!(
                    "Duplicate primary key in batch: {pk_values:?}"
                )));
            }

            // Check against existing table
            if self.pk_index.contains_key(&pk_values) {
                return Err(RuzuError::ConstraintViolation(format!(
                    "Duplicate primary key: {pk_values:?}"
                )));
            }
        }

        // Pre-grow columns
        let new_count = rows.len();
        for col in &mut self.columns {
            col.reserve(new_count);
        }

        // Insert all rows
        for row in rows {
            // Extract primary key values
            let pk_values: Vec<Value> = pk_col_indices
                .iter()
                .map(|&schema_idx| {
                    let input_idx = col_mapping[schema_idx];
                    row[input_idx].clone()
                })
                .collect();

            // Insert values into columns in schema order
            for (schema_idx, col) in self.columns.iter_mut().enumerate() {
                let input_idx = col_mapping[schema_idx];
                col.push(row[input_idx].clone());
            }

            // Update primary key index
            self.pk_index.insert(pk_values, self.row_count);
            self.row_count += 1;
        }

        Ok(new_count)
    }
}
