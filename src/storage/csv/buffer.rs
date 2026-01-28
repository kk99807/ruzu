//! Reusable row buffer for memory-efficient CSV parsing.
//!
//! This module provides `RowBuffer`, a buffer that can be recycled
//! to minimize memory allocations during streaming CSV imports.

use crate::types::Value;

/// Error returned when the buffer is full.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BufferFull;

impl std::fmt::Display for BufferFull {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Row buffer is full")
    }
}

impl std::error::Error for BufferFull {}

/// Reusable buffer for parsed CSV rows with memory recycling.
///
/// `RowBuffer` pre-allocates storage for a fixed number of rows
/// and reuses that storage across batches to minimize allocations.
///
/// # Memory Recycling
///
/// The buffer supports two recycling strategies:
/// - `clear()`: Drops inner Vecs but keeps outer capacity (simple, less memory retention)
/// - `recycle()`: Clears inner Vecs but keeps their allocations (optimal for repeated use)
///
/// # Example
///
/// ```ignore
/// let mut buffer = RowBuffer::new(1000, 5);
///
/// // Fill the buffer
/// for _ in 0..1000 {
///     buffer.push(vec![Value::Int64(42), Value::String("test".into())])?;
/// }
///
/// // Process and recycle
/// let rows = buffer.take();
/// // buffer is now empty but retains its allocated capacity
/// ```
#[derive(Debug)]
pub struct RowBuffer {
    /// Pre-allocated row storage.
    rows: Vec<Vec<Value>>,
    /// Maximum number of rows this buffer can hold.
    capacity: usize,
    /// Pre-allocated capacity for column values per row.
    column_capacity: usize,
    /// Pool of recycled inner Vecs for reuse.
    recycled_rows: Vec<Vec<Value>>,
}

impl RowBuffer {
    /// Creates a new row buffer with the given capacities.
    ///
    /// # Arguments
    ///
    /// * `row_capacity` - Maximum number of rows the buffer can hold
    /// * `column_capacity` - Number of columns per row (for pre-allocation)
    #[must_use]
    pub fn new(row_capacity: usize, column_capacity: usize) -> Self {
        Self {
            rows: Vec::with_capacity(row_capacity),
            capacity: row_capacity,
            column_capacity,
            recycled_rows: Vec::new(),
        }
    }

    /// Pushes a row into the buffer.
    ///
    /// # Errors
    ///
    /// Returns `BufferFull` if the buffer is at capacity.
    pub fn push(&mut self, row: Vec<Value>) -> Result<(), BufferFull> {
        if self.rows.len() >= self.capacity {
            return Err(BufferFull);
        }
        self.rows.push(row);
        Ok(())
    }

    /// Pushes a row using a recycled Vec if available.
    ///
    /// This is more memory-efficient for repeated batch operations
    /// as it reuses previously allocated Vecs.
    ///
    /// # Errors
    ///
    /// Returns `BufferFull` if the buffer is at capacity.
    pub fn push_with_recycling(
        &mut self,
        values: impl IntoIterator<Item = Value>,
    ) -> Result<(), BufferFull> {
        if self.rows.len() >= self.capacity {
            return Err(BufferFull);
        }

        // Try to reuse a recycled row Vec
        let mut row = self
            .recycled_rows
            .pop()
            .unwrap_or_else(|| Vec::with_capacity(self.column_capacity));
        row.extend(values);
        self.rows.push(row);
        Ok(())
    }

    /// Returns the number of rows currently in the buffer.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Returns true if the buffer is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Returns true if the buffer is at capacity.
    #[must_use]
    pub fn is_full(&self) -> bool {
        self.rows.len() >= self.capacity
    }

    /// Returns the maximum capacity of the buffer.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Clears the buffer without deallocating.
    ///
    /// This preserves the allocated capacity for the outer Vec
    /// but drops all inner Vecs. For full recycling that preserves
    /// inner Vec allocations, use `recycle()` instead.
    pub fn clear(&mut self) {
        self.rows.clear();
    }

    /// Recycles the buffer for reuse, preserving inner Vec allocations.
    ///
    /// This clears all values from inner Vecs but keeps their allocated
    /// capacity for reuse in subsequent batches. This is optimal for
    /// streaming imports where batches have similar structure.
    pub fn recycle(&mut self) {
        // Move rows to recycled pool, clearing their contents
        for mut row in self.rows.drain(..) {
            row.clear(); // Clear values but keep allocation
            self.recycled_rows.push(row);
        }
        // Limit recycled pool size to avoid unbounded growth
        let max_recycled = self.capacity * 2;
        if self.recycled_rows.len() > max_recycled {
            self.recycled_rows.truncate(max_recycled);
        }
    }

    /// Takes all rows from the buffer, resetting it for reuse.
    ///
    /// Returns the rows and leaves the buffer empty but with
    /// its original allocated capacity preserved.
    pub fn take(&mut self) -> Vec<Vec<Value>> {
        std::mem::take(&mut self.rows)
    }

    /// Takes all rows and recycles them after external processing.
    ///
    /// Call this when you've processed the returned rows and want
    /// to recycle their Vec allocations. Pass back the rows after
    /// processing to enable allocation reuse.
    pub fn take_and_prepare_recycle(&mut self) -> Vec<Vec<Value>> {
        // Restore capacity for the rows vec
        std::mem::replace(&mut self.rows, Vec::with_capacity(self.capacity))
    }

    /// Returns rows to the recycled pool after external processing.
    ///
    /// Call this after processing rows from `take_and_prepare_recycle()`
    /// to return the Vec allocations for reuse.
    pub fn return_for_recycling(&mut self, mut rows: Vec<Vec<Value>>) {
        for mut row in rows.drain(..) {
            row.clear();
            self.recycled_rows.push(row);
        }
        // Limit recycled pool size
        let max_recycled = self.capacity * 2;
        if self.recycled_rows.len() > max_recycled {
            self.recycled_rows.truncate(max_recycled);
        }
    }

    /// Returns the column capacity used for pre-allocation hints.
    #[must_use]
    pub fn column_capacity(&self) -> usize {
        self.column_capacity
    }

    /// Returns the number of recycled Vecs available for reuse.
    #[must_use]
    pub fn recycled_count(&self) -> usize {
        self.recycled_rows.len()
    }

    /// Returns an iterator over the rows.
    pub fn iter(&self) -> impl Iterator<Item = &Vec<Value>> {
        self.rows.iter()
    }

    /// Returns a mutable iterator over the rows.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Vec<Value>> {
        self.rows.iter_mut()
    }
}

impl IntoIterator for RowBuffer {
    type Item = Vec<Value>;
    type IntoIter = std::vec::IntoIter<Vec<Value>>;

    fn into_iter(self) -> Self::IntoIter {
        self.rows.into_iter()
    }
}
