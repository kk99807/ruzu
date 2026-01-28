# Research: Optimize Peak Memory During CSV Import

**Feature**: 004-optimize-csv-memory
**Date**: 2025-12-07
**Status**: Complete

## Research Questions

1. Where does memory accumulate during CSV import?
2. Can the storage engine accept incremental batch writes?
3. What buffer recycling patterns are suitable for Rust?
4. What is the throughput impact of streaming writes?

---

## R1: Memory Allocation Analysis

### Source
- Codebase exploration of `src/storage/csv/`
- 003-optimize-csv-import DHAT profiling results

### Findings

The current CSV import has two primary memory accumulation points:

**1. Row Accumulation in Loaders**

```rust
// node_loader.rs - Sequential mode
let mut rows = Vec::new();  // Grows unbounded
for result in reader.records() {
    rows.push(self.parse_record(&record, ...)?);
}
// All rows held until function returns

// parallel.rs - Parallel mode
let (rows, errors, bytes_processed) = parallel_read_all(data, &self.config, parse_row)?;
// Still returns ALL rows in memory
```

**2. Value String Allocations**

```rust
// types/value.rs
pub enum Value {
    String(String),  // Each string is heap-allocated
    Int64(i64),
    Float64(f64),
    Bool(bool),
    Date(i32),
    Null,
}
```

### Memory Amplification Breakdown

| Component | Amplification Factor |
|-----------|---------------------|
| Raw CSV bytes | 1.0x |
| Parsed `Value` objects | ~2.5x |
| `Vec<Vec<Value>>` container | ~1.5x |
| **Total** | **~4.4x** |

### Decision

Target streaming writes in loaders (high impact). Defer String optimization (lower impact, higher complexity).

---

## R2: Streaming Write Feasibility

### Source
- `src/storage/table.rs` - NodeTable implementation
- `src/storage/rel_table.rs` - RelTable implementation

### Findings

The storage engine already supports incremental writes:

```rust
// NodeTable
impl NodeTable {
    pub fn insert(&mut self, row: &HashMap<String, Value>) -> Result<()>
}

// RelTable
impl RelTable {
    pub fn insert(&mut self, from: u64, to: u64, properties: Vec<Value>) -> Result<()>
}
```

**Limitation**: No batch insert API exists. Per-row inserts have overhead:
- Column validation on each insert
- Primary key lookup on each insert
- HashMap conversion for row data

### Decision

Implement `insert_batch()` methods that:
1. Validate columns once per batch
2. Pre-grow column storage
3. Batch primary key checks

---

## R3: Buffer Recycling Patterns

### Source
- Rust ecosystem research
- Standard library Vec behavior

### Options Evaluated

| Pattern | Memory Overhead | Complexity | Thread Safety |
|---------|-----------------|------------|---------------|
| Object pool (crossbeam/parking_lot) | Low | Medium | Yes |
| Ring buffer (fixed allocation) | Fixed | Low | Yes |
| `Vec::clear()` with capacity | Minimal | Low | No |

### Recommendation

Use `Vec::clear()` approach for simplicity:

```rust
struct RowBuffer {
    rows: Vec<Vec<Value>>,
    column_capacity: usize,
}

impl RowBuffer {
    fn recycle(&mut self) {
        for row in &mut self.rows {
            row.clear();  // Keep inner allocations
        }
        self.rows.clear();  // Keep outer allocation
    }
}
```

**Rationale**:
- No external dependencies
- Natural Rust idiom
- Per-thread buffers avoid synchronization

### Decision

Per-thread `RowBuffer` with `clear()` recycling.

---

## R4: Throughput Impact Analysis

### Source
- 003-optimize-csv-import benchmark results
- Theoretical analysis of disk I/O overhead

### Baseline

From 003 feature:
- Node import: 8.9M nodes/sec
- Edge import: 3.8M edges/sec

### Projected Impact

| Approach | Est. Memory | Est. Throughput | Notes |
|----------|-------------|-----------------|-------|
| Current (in-memory) | 4.4x input | 100% | Baseline |
| Streaming (10K batch) | ~50MB fixed | 85-95% | High sync overhead |
| Streaming (100K batch) | ~200MB fixed | 95-98% | Optimal balance |
| Streaming + recycle | ~150MB fixed | 90-95% | Adds recycling overhead |

### Decision

Use 100,000 row batch size:
- Well under 500MB target (~200MB worst case)
- Minimal throughput impact (~5% expected)
- Configurable for tuning

---

## Summary of Decisions

| Question | Decision | Rationale |
|----------|----------|-----------|
| Memory target | Streaming writes | Addresses primary accumulation point |
| Buffer management | `Vec::clear()` recycling | Simple, no dependencies |
| Batch size | 100,000 rows | Balance of memory (~200MB) and throughput (~95%) |
| String optimization | Deferred | Lower impact, higher complexity |
| Batch insert API | Required | Avoid per-row validation overhead |

---

## References

- [003-optimize-csv-import/spec.md](../003-optimize-csv-import/spec.md) lines 19-38 - Memory profiling results
- [src/storage/csv/node_loader.rs](../../src/storage/csv/node_loader.rs) - Current accumulation logic
- [src/storage/table.rs](../../src/storage/table.rs) - Storage insert API
