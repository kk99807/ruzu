//! Columnar storage using `Vec<Value>`.

use serde::{Deserialize, Serialize};

use crate::types::Value;

/// Simple columnar storage using `Vec<Value>`.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ColumnStorage {
    data: Vec<Value>,
}

impl ColumnStorage {
    /// Creates a new empty column.
    #[must_use]
    pub fn new() -> Self {
        ColumnStorage { data: Vec::new() }
    }

    /// Appends a value to the column.
    pub fn push(&mut self, value: Value) {
        self.data.push(value);
    }

    /// Gets a value by row index.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&Value> {
        self.data.get(index)
    }

    /// Returns the number of values in the column.
    #[must_use]
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Returns true if the column is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Reserves capacity for at least `additional` more elements.
    pub fn reserve(&mut self, additional: usize) {
        self.data.reserve(additional);
    }
}
