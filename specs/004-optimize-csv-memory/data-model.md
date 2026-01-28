# Data Model: Optimize Peak Memory During CSV Import

**Feature**: 004-optimize-csv-memory
**Date**: 2025-12-07

## Overview

This feature does not introduce new persistent data structures. All changes are to in-memory processing during CSV import. This document describes the internal types used for streaming and buffer management.

---

## Internal Types (Non-Persistent)

### StreamingConfig

Configuration for memory-bounded streaming imports.

```rust
pub struct StreamingConfig {
    /// Number of rows per batch before flush to storage
    /// Default: 100_000 (balances ~200MB memory with minimal throughput loss)
    pub batch_size: usize,

    /// Pre-allocate buffer capacity for row storage
    /// Default: same as batch_size
    pub buffer_capacity: usize,

    /// Enable streaming mode
    /// Default: true for files > 100MB, false otherwise
    pub streaming_enabled: bool,

    /// Threshold file size to auto-enable streaming (bytes)
    /// Default: 100 * 1024 * 1024 (100MB)
    pub streaming_threshold: u64,
}
```

**Rationale**: 100K rows keeps memory under 200MB worst-case while maintaining ~95% of baseline throughput.

---

### RowBuffer

Reusable buffer for parsed CSV rows with memory recycling.

```rust
pub struct RowBuffer {
    /// Pre-allocated row storage
    rows: Vec<Vec<Value>>,

    /// Current row count (may be < rows.len() after recycling)
    len: usize,

    /// Pre-allocated capacity for column values per row
    column_capacity: usize,
}
```

**Memory Behavior**:
- `new(row_capacity, column_capacity)` pre-allocates storage
- `push(row)` reuses existing inner Vecs when available
- `clear()` resets counts without deallocating
- `take()` returns rows and resets for reuse

**Diagram**:
```
RowBuffer (batch_size=100K, cols=5)
┌─────────────────────────────────────┐
│ rows: Vec<Vec<Value>>               │
│   [0]: [v,v,v,v,v]  ← pre-allocated │
│   [1]: [v,v,v,v,v]                  │
│   ...                               │
│   [99999]: [v,v,v,v,v]              │
│                                     │
│ len: 50000  ← actual rows in use    │
│ column_capacity: 5                  │
└─────────────────────────────────────┘
         │
         ▼ clear()
┌─────────────────────────────────────┐
│ rows: still allocated (capacity)    │
│ len: 0  ← reset, ready for reuse    │
└─────────────────────────────────────┘
```

---

### RelationshipBuffer

Specialized buffer for relationship imports.

```rust
pub struct RelationshipBuffer {
    /// Pre-allocated relationship storage
    relationships: Vec<ParsedRelationship>,

    /// Current count
    len: usize,
}

pub struct ParsedRelationship {
    pub from_key: Value,
    pub to_key: Value,
    pub properties: Vec<Value>,
}
```

---

### BatchWriteResult

Result of a streaming batch write operation.

```rust
pub struct BatchWriteResult {
    /// Number of rows successfully written
    pub rows_written: usize,

    /// Number of rows that failed (if continue-on-error)
    pub rows_failed: usize,

    /// Errors encountered (if any)
    pub errors: Vec<ImportError>,
}
```

---

## State Transitions

### Import State Machine

```
┌──────────┐
│  Start   │
└────┬─────┘
     │
     ▼
┌──────────────────┐
│  Initialize      │
│  - Create buffer │
│  - Open CSV      │
└────────┬─────────┘
         │
         ▼
┌──────────────────┐
│  Parse Rows      │◄──────┐
│  - Fill buffer   │       │
│  - Track progress│       │
└────────┬─────────┘       │
         │                 │
         ▼                 │
    ┌────────────┐         │
    │ Buffer     │ No      │
    │ Full?      ├─────────┘
    └────┬───────┘
         │ Yes
         ▼
┌──────────────────┐
│  Flush Batch     │
│  - Write to      │
│    storage       │
│  - Recycle       │
│    buffer        │
└────────┬─────────┘
         │
         ▼
    ┌────────────┐
    │ More       │ Yes
    │ Rows?      ├────────────┐
    └────┬───────┘            │
         │ No                 │
         ▼                    │
┌──────────────────┐          │
│  Flush Final     │          │
│  - Write         │          │
│    remaining     │          │
└────────┬─────────┘          │
         │                    │
         ▼                    │
┌──────────────────┐          │
│  Complete        │◄─────────┘
│  - Return result │
└──────────────────┘
```

---

## Memory Budget Analysis

### Worst-Case Memory Calculation

For 100K row batch with 10 columns:

| Component | Size | Count | Total |
|-----------|------|-------|-------|
| Value enum | 24 bytes | 100K × 10 | 24 MB |
| String heap (avg 50 bytes) | 50 bytes | 100K × 5 strings | 25 MB |
| Vec metadata (outer) | 24 bytes | 100K | 2.4 MB |
| Vec metadata (inner) | 24 bytes | 100K × 10 | 24 MB |
| Buffer overhead | - | - | ~5 MB |
| **Total per batch** | | | **~80 MB** |

With 2 buffers (parse + write overlap): **~160 MB**

Safety margin for peaks: **~200 MB**

**Conclusion**: 100K batch size comfortably fits in 500MB target.

---

## Validation Rules

### StreamingConfig Validation

1. `batch_size` must be > 0 and ≤ 10,000,000
2. `buffer_capacity` defaults to `batch_size` if unset
3. `streaming_threshold` must be > 0

### RowBuffer Validation

1. `row_capacity` must be > 0
2. `column_capacity` must be > 0
3. Buffer must not exceed configured capacity (returns `BufferFull` error)

---

## Relationship to Existing Types

### Value (unchanged)
```rust
pub enum Value {
    Int64(i64),
    Float64(f64),
    Bool(bool),
    String(String),
    Date(i32),
    Null,
}
```

### CsvImportConfig (extended)
```rust
pub struct CsvImportConfig {
    // Existing fields...
    pub batch_size: usize,
    pub parallel: bool,
    pub block_size: usize,
    // ...

    // NEW: Streaming configuration
    pub streaming: StreamingConfig,
}
```

### ImportResult (unchanged)
```rust
pub struct ImportResult {
    pub rows_imported: usize,
    pub rows_skipped: usize,
    pub errors: Vec<ImportError>,
}
```
