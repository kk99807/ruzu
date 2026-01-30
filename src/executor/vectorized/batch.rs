//! Vectorized batch wrapper around Arrow RecordBatch.

use arrow::array::{Array, ArrayRef, UInt32Array};
use arrow::datatypes::SchemaRef;
use arrow::record_batch::RecordBatch;

/// Default batch size for vectorized execution (rows per batch).
pub const DEFAULT_BATCH_SIZE: usize = 2048;

/// Wrapper around Arrow RecordBatch with optional selection vector.
#[derive(Debug, Clone)]
pub struct VectorizedBatch {
    /// The underlying Arrow RecordBatch.
    batch: RecordBatch,
    /// Optional selection vector for filtered rows.
    selection: Option<SelectionVector>,
}

impl VectorizedBatch {
    /// Creates a new vectorized batch from a RecordBatch.
    pub fn new(batch: RecordBatch) -> Self {
        VectorizedBatch {
            batch,
            selection: None,
        }
    }

    /// Creates a new vectorized batch with a selection vector.
    pub fn with_selection(batch: RecordBatch, selection: SelectionVector) -> Self {
        VectorizedBatch {
            batch,
            selection: Some(selection),
        }
    }

    /// Returns the underlying RecordBatch.
    #[must_use]
    pub fn batch(&self) -> &RecordBatch {
        &self.batch
    }

    /// Returns the selection vector, if any.
    #[must_use]
    pub fn selection(&self) -> Option<&SelectionVector> {
        self.selection.as_ref()
    }

    /// Returns the schema of this batch.
    #[must_use]
    pub fn schema(&self) -> SchemaRef {
        self.batch.schema()
    }

    /// Returns the number of rows in this batch.
    ///
    /// If there's a selection vector, returns the number of selected rows.
    #[must_use]
    pub fn num_rows(&self) -> usize {
        self.selection
            .as_ref()
            .map_or_else(|| self.batch.num_rows(), |s| s.len())
    }

    /// Returns the number of columns in this batch.
    #[must_use]
    pub fn num_columns(&self) -> usize {
        self.batch.num_columns()
    }

    /// Returns a column by index.
    #[must_use]
    pub fn column(&self, index: usize) -> &ArrayRef {
        self.batch.column(index)
    }

    /// Returns a column by name.
    pub fn column_by_name(&self, name: &str) -> Option<&ArrayRef> {
        let schema = self.batch.schema();
        schema.index_of(name).ok().map(|i| self.batch.column(i))
    }

    /// Applies the selection vector to produce a new batch with only selected rows.
    ///
    /// If there's no selection vector, returns the batch unchanged.
    pub fn materialize(&self) -> arrow::error::Result<RecordBatch> {
        if let Some(ref selection) = self.selection {
            let indices = UInt32Array::from(selection.indices.clone());
            let columns: Vec<ArrayRef> = self
                .batch
                .columns()
                .iter()
                .map(|col| arrow::compute::take(col, &indices, None))
                .collect::<arrow::error::Result<Vec<_>>>()?;
            RecordBatch::try_new(self.batch.schema(), columns)
        } else {
            Ok(self.batch.clone())
        }
    }

    /// Applies a filter predicate to this batch, returning a new batch with selection.
    pub fn filter(&self, predicate: &dyn Array) -> arrow::error::Result<Self> {
        let bool_array = predicate
            .as_any()
            .downcast_ref::<arrow::array::BooleanArray>()
            .ok_or_else(|| {
                arrow::error::ArrowError::ComputeError("Predicate must be boolean array".into())
            })?;

        let mut indices = Vec::new();
        for i in 0..bool_array.len() {
            if bool_array.value(i) {
                indices.push(i as u32);
            }
        }

        let selection = SelectionVector::new(indices);
        Ok(VectorizedBatch::with_selection(self.batch.clone(), selection))
    }
}

/// Selection vector for filtered batches.
///
/// Instead of materializing filtered results immediately,
/// we keep track of which rows are selected for lazy evaluation.
#[derive(Debug, Clone)]
pub struct SelectionVector {
    /// Indices of selected rows.
    pub indices: Vec<u32>,
}

impl SelectionVector {
    /// Creates a new selection vector with the given indices.
    pub fn new(indices: Vec<u32>) -> Self {
        SelectionVector { indices }
    }

    /// Creates a selection vector selecting all rows up to count.
    pub fn all(count: usize) -> Self {
        SelectionVector {
            indices: (0..count as u32).collect(),
        }
    }

    /// Returns the number of selected rows.
    #[must_use]
    pub fn len(&self) -> usize {
        self.indices.len()
    }

    /// Returns true if no rows are selected.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    /// Returns the index at the given position.
    #[must_use]
    pub fn get(&self, pos: usize) -> Option<u32> {
        self.indices.get(pos).copied()
    }

    /// Intersects this selection with another, keeping only rows in both.
    #[must_use]
    pub fn intersect(&self, other: &SelectionVector) -> Self {
        let other_set: std::collections::HashSet<_> = other.indices.iter().collect();
        let indices: Vec<_> = self
            .indices
            .iter()
            .filter(|i| other_set.contains(i))
            .copied()
            .collect();
        SelectionVector::new(indices)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use arrow::array::Int64Array;
    use arrow::datatypes::{DataType, Field, Schema};

    #[test]
    fn test_vectorized_batch_basic() {
        let schema = Arc::new(Schema::new(vec![
            Field::new("a", DataType::Int64, false),
            Field::new("b", DataType::Int64, false),
        ]));

        let a = Arc::new(Int64Array::from(vec![1, 2, 3, 4, 5])) as ArrayRef;
        let b = Arc::new(Int64Array::from(vec![10, 20, 30, 40, 50])) as ArrayRef;

        let batch = RecordBatch::try_new(schema.clone(), vec![a, b]).unwrap();
        let vbatch = VectorizedBatch::new(batch);

        assert_eq!(vbatch.num_rows(), 5);
        assert_eq!(vbatch.num_columns(), 2);
        assert!(vbatch.selection().is_none());
    }

    #[test]
    fn test_selection_vector() {
        let selection = SelectionVector::new(vec![0, 2, 4]);
        assert_eq!(selection.len(), 3);
        assert_eq!(selection.get(0), Some(0));
        assert_eq!(selection.get(1), Some(2));
        assert_eq!(selection.get(2), Some(4));
    }

    #[test]
    fn test_selection_all() {
        let selection = SelectionVector::all(5);
        assert_eq!(selection.len(), 5);
        assert_eq!(selection.indices, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn test_selection_intersect() {
        let s1 = SelectionVector::new(vec![0, 1, 2, 3]);
        let s2 = SelectionVector::new(vec![1, 3, 5]);
        let result = s1.intersect(&s2);
        assert_eq!(result.indices, vec![1, 3]);
    }
}
