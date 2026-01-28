# API Contract: Streaming CSV Import

**Feature**: 004-optimize-csv-memory
**Version**: 1.0.0
**Date**: 2025-12-07

## Overview

This document defines the API contracts for memory-bounded streaming CSV import. All APIs are internal Rust interfaces (no external API changes).

---

## Contract 1: StreamingConfig

### Type Definition

```rust
/// Configuration for memory-bounded streaming imports
#[derive(Debug, Clone)]
pub struct StreamingConfig {
    /// Number of rows per batch before flush to storage
    /// Range: 1..=10_000_000
    /// Default: 100_000
    pub batch_size: usize,

    /// Pre-allocate buffer capacity (rows)
    /// Default: batch_size
    pub buffer_capacity: usize,

    /// Enable streaming mode
    /// Default: auto (true if file > streaming_threshold)
    pub streaming_enabled: bool,

    /// File size threshold for auto-enable (bytes)
    /// Default: 100 * 1024 * 1024 (100MB)
    pub streaming_threshold: u64,
}

impl Default for StreamingConfig {
    fn default() -> Self {
        Self {
            batch_size: 100_000,
            buffer_capacity: 100_000,
            streaming_enabled: true,
            streaming_threshold: 100 * 1024 * 1024,
        }
    }
}
```

### Validation Rules

| Field | Constraint | Error |
|-------|------------|-------|
| `batch_size` | 1 ≤ x ≤ 10,000,000 | `InvalidConfig("batch_size out of range")` |
| `buffer_capacity` | ≥ 1 | `InvalidConfig("buffer_capacity must be positive")` |
| `streaming_threshold` | ≥ 1 | `InvalidConfig("streaming_threshold must be positive")` |

---

## Contract 2: RowBuffer

### Type Definition

```rust
/// Reusable buffer for memory-efficient row parsing
pub struct RowBuffer {
    rows: Vec<Vec<Value>>,
    len: usize,
    column_capacity: usize,
}

#[derive(Debug)]
pub struct BufferFull;

impl RowBuffer {
    /// Create buffer with pre-allocated capacity
    ///
    /// # Arguments
    /// * `row_capacity` - Maximum rows before full
    /// * `column_capacity` - Expected columns per row
    pub fn new(row_capacity: usize, column_capacity: usize) -> Self;

    /// Add row to buffer
    ///
    /// # Returns
    /// * `Ok(())` - Row added
    /// * `Err(BufferFull)` - Buffer at capacity
    pub fn push(&mut self, row: Vec<Value>) -> Result<(), BufferFull>;

    /// Take all rows, reset buffer for reuse
    ///
    /// Buffer retains allocated capacity for recycling.
    pub fn take(&mut self) -> Vec<Vec<Value>>;

    /// Clear buffer without deallocating
    pub fn clear(&mut self);

    /// Current row count
    pub fn len(&self) -> usize;

    /// True if buffer at capacity
    pub fn is_full(&self) -> bool;
}
```

### Invariants

1. `len() <= row_capacity` (always)
2. After `take()`: `len() == 0`, capacity preserved
3. After `clear()`: `len() == 0`, capacity preserved
4. `is_full()` ≡ `len() == row_capacity`

### Memory Contract

- `clear()` MUST NOT deallocate
- `take()` MUST NOT deallocate buffer internals
- Memory footprint bounded by `row_capacity * column_capacity * sizeof(Value)`

---

## Contract 3: NodeLoader::load_streaming

### Signature

```rust
impl NodeLoader {
    /// Load CSV with streaming writes
    ///
    /// Parses CSV and writes batches incrementally via callback.
    /// Memory usage bounded by `config.batch_size * avg_row_size`.
    ///
    /// # Arguments
    /// * `path` - Path to CSV file
    /// * `config` - Streaming configuration
    /// * `write_batch` - Callback invoked when batch is full
    /// * `progress_callback` - Optional progress reporting
    ///
    /// # Returns
    /// * `Ok(ImportResult)` - Import statistics
    /// * `Err(RuzuError)` - Parse or write error
    pub fn load_streaming<W>(
        &self,
        path: impl AsRef<Path>,
        config: &StreamingConfig,
        mut write_batch: W,
        progress_callback: Option<Box<dyn Fn(ImportProgress)>>,
    ) -> Result<ImportResult, RuzuError>
    where
        W: FnMut(Vec<Vec<Value>>) -> Result<(), RuzuError>;
}
```

### Behavior Contract

| Condition | Behavior |
|-----------|----------|
| Batch reaches `config.batch_size` | Invoke `write_batch` with rows |
| End of file with partial batch | Invoke `write_batch` with remaining rows |
| `write_batch` returns `Err` | Stop processing, return error |
| Progress callback provided | Invoke at batch boundaries |

### Memory Contract

- Peak memory ≤ `2 * batch_size * avg_row_size` (double buffer for overlap)
- Memory independent of total file size

---

## Contract 4: NodeTable::insert_batch

### Signature

```rust
impl NodeTable {
    /// Insert multiple rows in a single batch
    ///
    /// More efficient than repeated single inserts:
    /// - Single validation pass for column structure
    /// - Batch primary key uniqueness check
    /// - Pre-allocated column growth
    ///
    /// # Arguments
    /// * `rows` - Row data as Vec of column values
    /// * `columns` - Column names in order matching row values
    ///
    /// # Returns
    /// * `Ok(usize)` - Number of rows inserted
    /// * `Err(RuzuError)` - Validation or insert error
    pub fn insert_batch(
        &mut self,
        rows: Vec<Vec<Value>>,
        columns: &[String],
    ) -> Result<usize, RuzuError>;
}
```

### Behavior Contract

| Condition | Behavior |
|-----------|----------|
| Empty rows | Return `Ok(0)` |
| Column mismatch | Return `Err(SchemaMismatch)` |
| Type mismatch | Return `Err(TypeMismatch)` on first bad row |
| Duplicate PK | Return `Err(DuplicateKey)` |
| All valid | Insert all, return `Ok(rows.len())` |

### Atomicity

- Batch insert is atomic: all rows inserted or none
- On error, table state unchanged

---

## Contract 5: RelTable::insert_batch

### Signature

```rust
impl RelTable {
    /// Insert multiple relationships in a single batch
    ///
    /// # Arguments
    /// * `relationships` - (from_offset, to_offset, properties)
    ///
    /// # Returns
    /// * `Ok(usize)` - Number of relationships inserted
    /// * `Err(RuzuError)` - Invalid offset or property error
    pub fn insert_batch(
        &mut self,
        relationships: Vec<(u64, u64, Vec<Value>)>,
    ) -> Result<usize, RuzuError>;
}
```

### Behavior Contract

| Condition | Behavior |
|-----------|----------|
| Empty relationships | Return `Ok(0)` |
| Invalid from_offset | Return `Err(InvalidNodeOffset)` |
| Invalid to_offset | Return `Err(InvalidNodeOffset)` |
| Property count mismatch | Return `Err(SchemaMismatch)` |
| All valid | Insert all, return `Ok(relationships.len())` |

---

## Memory Contracts (Testable)

| ID | Condition | Bound | Test Method |
|----|-----------|-------|-------------|
| MC-001 | Import 1GB CSV (nodes) | Peak < 500MB | DHAT profiler |
| MC-002 | Import 1GB CSV (edges) | Peak < 500MB | DHAT profiler |
| MC-003 | Import 5GB CSV | Peak < 500MB | DHAT profiler |
| MC-004 | Variance 100MB-5GB | < 100MB diff | DHAT comparison |

## Throughput Contracts (Testable)

| ID | Condition | Bound | Test Method |
|----|-----------|-------|-------------|
| TC-001 | Node import | ≥ 7M nodes/sec | criterion bench |
| TC-002 | Edge import | ≥ 3M edges/sec | criterion bench |

---

## Error Types

```rust
#[derive(Debug)]
pub enum StreamingError {
    /// Buffer reached capacity
    BufferFull,

    /// Write callback failed
    WriteFailed(RuzuError),

    /// Parse error at row
    ParseError { row: usize, error: String },

    /// Invalid configuration
    InvalidConfig(String),
}
```

---

## Backward Compatibility

- Existing `NodeLoader::load()` unchanged
- Existing `Database::import_nodes()` uses streaming by default for large files
- Old code continues to work without modification
