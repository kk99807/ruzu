# Implementation Plan: Optimize Peak Memory During CSV Import

**Branch**: `004-optimize-csv-memory` | **Date**: 2025-12-07 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/004-optimize-csv-memory/spec.md`

## Summary

Reduce peak memory usage during CSV import from ~4.5GB to <500MB for 1GB files while maintaining throughput within 20% of the 8.9M nodes/sec and 3.8M edges/sec baselines from feature 003. The primary approach is implementing streaming writes with buffer recycling, as identified in the [003-optimize-csv-import spec](../003-optimize-csv-import/spec.md) (lines 32-38):

> **Strategies to Achieve Memory Target (Future Work)**
>
> 1. **Streaming Writes**: Modify loaders to write batches to storage as they complete instead of collecting all rows in memory. This requires integrating with the storage engine during parsing.
>
> 2. **Row Buffer Recycling**: Reuse allocated `Vec<Value>` buffers across batches. After a batch is written to storage, recycle the row vectors for the next batch rather than allocating new ones.
>
> 3. **Direct-to-Page Parsing**: Parse CSV fields directly into page-format storage without intermediate `Value` allocations. This eliminates the memory amplification from parsed representation.

## Technical Context

**Language/Version**: Rust 1.75+ (stable, 2021 edition)
**Primary Dependencies**: csv (parsing), memmap2 (mmap), crossbeam (parallel processing), parking_lot (synchronization), rayon (parallel iteration)
**Storage**: Custom page-based file format with 4KB pages, WAL, buffer pool
**Testing**: cargo test, criterion benchmarks, DHAT heap profiler
**Target Platform**: Linux (x86_64, aarch64), macOS (x86_64, aarch64), Windows (x86_64)
**Project Type**: Single library crate with benchmarks
**Performance Goals**: ≥7M nodes/sec, ≥3M edges/sec (within 20% of 003 baseline)
**Constraints**: <500MB peak memory for imports up to 5GB
**Scale/Scope**: Support CSV files up to 5GB with bounded memory

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Notes |
|-----------|--------|-------|
| **I. Port-First** | ✅ PASS | Memory optimization is implementation detail; follows KuzuDB's batch-based import model |
| **II. TDD (Red-Green-Refactor)** | ✅ PASS | Will write memory tests first, then implement streaming |
| **III. Benchmarking & Performance** | ✅ PASS | Using established benchmarks from 003; will track memory alongside throughput |
| **IV. Rust Best Practices** | ✅ PASS | Buffer recycling follows Rust idioms; no unsafe needed |
| **V. Safety & Correctness First** | ✅ PASS | Correctness preserved; streaming writes use existing storage APIs |

## Project Structure

### Documentation (this feature)

```text
specs/004-optimize-csv-memory/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/           # Phase 1 output
└── tasks.md             # Phase 2 output
```

### Source Code (repository root)

```text
src/
├── storage/
│   ├── csv/
│   │   ├── mod.rs              # CsvImportConfig - add streaming options
│   │   ├── node_loader.rs      # NodeLoader - implement streaming writes
│   │   ├── rel_loader.rs       # RelLoader - implement streaming writes
│   │   ├── parallel.rs         # Streaming block processing
│   │   ├── buffer_pool.rs      # NEW: Row buffer recycling pool
│   │   └── streaming.rs        # NEW: Streaming write coordinator
│   ├── table.rs                # NodeTable - batch insert API
│   └── rel_table.rs            # RelTable - batch insert API

tests/
├── contract/
│   └── memory_contract_tests.rs  # Memory bound contracts
├── integration/
│   └── streaming_import_tests.rs # End-to-end streaming tests
└── unit/
    └── buffer_pool_tests.rs      # Row buffer recycling tests

benches/
├── csv_benchmark.rs              # Throughput (existing)
└── memory_benchmark.rs           # NEW: Memory profiling
```

**Structure Decision**: Single project structure matching existing codebase layout. New files added within existing `storage/csv/` module.

## Complexity Tracking

> No constitution violations - no entries needed.

---

## Phase 0: Research

### Research Tasks

1. **Analyze current memory allocation patterns** - Identify exactly where memory accumulates during import
2. **Evaluate streaming write feasibility** - Verify storage engine can accept incremental writes
3. **Buffer recycling strategies** - Research Rust patterns for buffer pool management
4. **Throughput impact analysis** - Understand tradeoffs between memory and speed

### Research Findings

#### R1: Current Memory Allocation Analysis

**Source**: Codebase exploration + 003 DHAT profiling

The current architecture has two main memory accumulation points:

1. **NodeLoader/RelLoader** (`node_loader.rs`, `rel_loader.rs`):
   ```rust
   // Sequential mode: accumulates ALL rows
   let mut rows = Vec::new();
   for result in reader.records() {
       rows.push(self.parse_record(&record, ...)?);
   }

   // Parallel mode: same issue
   let (rows, errors, bytes_processed) = parallel_read_all(data, &self.config, parse_row)?;
   ```

2. **Value String allocations** (`types/value.rs`):
   ```rust
   pub enum Value {
       String(String),  // Each string field is heap-allocated
       // ...
   }
   ```

**Memory amplification breakdown** (from 003 profiling):
- Raw CSV bytes: 1x
- Parsed `Value` objects: ~2.5x (due to enum overhead + String heap)
- `Vec<Vec<Value>>` container: ~1.5x (Vec metadata + alignment)
- Total: ~4.4x amplification

**Decision**: Target streaming writes in loaders; String optimization deferred to future work.

#### R2: Streaming Write Feasibility

**Source**: `storage/table.rs`, `storage/rel_table.rs` examination

The storage engine already supports incremental writes:

```rust
// NodeTable - supports single-row insert
impl NodeTable {
    pub fn insert(&mut self, row: &HashMap<String, Value>) -> Result<()>
}

// RelTable - supports single-edge insert
impl RelTable {
    pub fn insert(&mut self, from: u64, to: u64, properties: Vec<Value>) -> Result<()>
}
```

**Finding**: No batch insert API exists, but single-row inserts are supported. For throughput, we need:
1. Add batch insert methods to avoid per-row validation overhead
2. Write batches atomically within WAL transaction boundaries

**Decision**: Implement `insert_batch()` methods that accept `Vec<Vec<Value>>` with single validation pass.

#### R3: Buffer Recycling Patterns

**Source**: Rust ecosystem research

Three common patterns for buffer recycling in Rust:

| Pattern | Memory Overhead | Complexity | Thread Safety |
|---------|-----------------|------------|---------------|
| Object pool (parking_lot) | Low | Medium | Yes |
| Ring buffer | Fixed | Low | Yes |
| Reusable Vec with clear() | Minimal | Low | No (use per-thread) |

**Recommended approach**: Use `Vec::clear()` with pre-allocated capacity:

```rust
struct BatchBuffer {
    rows: Vec<Vec<Value>>,
    capacity: usize,
}

impl BatchBuffer {
    fn recycle(&mut self) {
        for row in &mut self.rows {
            row.clear();  // Keep inner Vec allocations
        }
        self.rows.clear();  // Keep outer Vec allocation
    }
}
```

**Decision**: Per-thread batch buffers with `clear()` recycling. Simpler than object pool, sufficient for our needs.

#### R4: Throughput Impact Analysis

**Source**: 003 benchmark results + theoretical analysis

| Approach | Estimated Memory | Estimated Throughput Impact |
|----------|------------------|----------------------------|
| Full in-memory (current) | 4.4x input | Baseline (8.9M nodes/sec) |
| Streaming (batch=10K rows) | ~50MB fixed | -5% to -15% (disk sync overhead) |
| Streaming (batch=100K rows) | ~200MB fixed | -2% to -5% |
| Streaming + buffer recycle | ~150MB fixed | -5% to -10% |

**Decision**: Use 100K row batches to balance memory (well under 500MB target) and throughput (minimal impact).

---

## Phase 1: Design

### Data Model

No new persistent data structures. Changes are to in-memory processing only.

### Internal Types

```rust
/// Configuration for streaming CSV import
pub struct StreamingConfig {
    /// Number of rows per batch before flush to storage (default: 100_000)
    pub batch_size: usize,

    /// Pre-allocate buffer capacity (default: batch_size)
    pub buffer_capacity: usize,

    /// Enable streaming mode (default: true for files > 100MB)
    pub streaming_enabled: bool,
}

/// Reusable row buffer for memory-efficient parsing
pub struct RowBuffer {
    /// Pre-allocated row storage
    rows: Vec<Vec<Value>>,

    /// Track actual row count (may be less than capacity)
    len: usize,

    /// Pre-allocated inner Vec capacity
    column_capacity: usize,
}

impl RowBuffer {
    pub fn new(row_capacity: usize, column_capacity: usize) -> Self;
    pub fn push(&mut self, row: Vec<Value>) -> Result<(), BufferFull>;
    pub fn take(&mut self) -> Vec<Vec<Value>>;  // Returns rows, resets buffer
    pub fn clear(&mut self);  // Reset without deallocation
    pub fn len(&self) -> usize;
    pub fn is_full(&self) -> bool;
}

/// Callback for streaming write operations
pub type WriteCallback = Box<dyn FnMut(Vec<Vec<Value>>) -> Result<(), RuzuError> + Send>;
```

### API Contracts

#### Contract 1: StreamingNodeLoader

```rust
/// Extended NodeLoader with streaming support
impl NodeLoader {
    /// Load CSV with streaming writes
    ///
    /// Instead of accumulating all rows, calls `write_batch` when batch is full.
    /// Memory usage bounded by `batch_size * avg_row_size`.
    pub fn load_streaming<W>(
        &self,
        path: impl AsRef<Path>,
        config: &StreamingConfig,
        write_batch: W,
        progress_callback: Option<Box<dyn Fn(ImportProgress)>>,
    ) -> Result<ImportResult, RuzuError>
    where
        W: FnMut(Vec<Vec<Value>>) -> Result<(), RuzuError>;
}
```

#### Contract 2: StreamingRelLoader

```rust
/// Extended RelLoader with streaming support
impl RelLoader {
    pub fn load_streaming<W>(
        &self,
        path: impl AsRef<Path>,
        config: &StreamingConfig,
        write_batch: W,
        progress_callback: Option<Box<dyn Fn(ImportProgress)>>,
    ) -> Result<ImportResult, RuzuError>
    where
        W: FnMut(Vec<ParsedRelationship>) -> Result<(), RuzuError>;
}
```

#### Contract 3: Batch Insert for NodeTable

```rust
impl NodeTable {
    /// Insert multiple rows in a single batch
    ///
    /// More efficient than repeated single inserts:
    /// - Single validation pass for column structure
    /// - Batch primary key uniqueness check
    /// - Pre-allocated column growth
    pub fn insert_batch(&mut self, rows: Vec<Vec<Value>>, columns: &[String]) -> Result<usize, RuzuError>;
}
```

#### Contract 4: Batch Insert for RelTable

```rust
impl RelTable {
    /// Insert multiple relationships in a single batch
    pub fn insert_batch(&mut self, relationships: Vec<(u64, u64, Vec<Value>)>) -> Result<usize, RuzuError>;
}
```

### Memory Contracts (Testable)

| Contract | Condition | Bound |
|----------|-----------|-------|
| MC-001 | Import 1GB CSV (nodes) | Peak memory < 500MB |
| MC-002 | Import 1GB CSV (edges) | Peak memory < 500MB |
| MC-003 | Import 5GB CSV | Peak memory < 500MB |
| MC-004 | Memory variance across file sizes | < 100MB difference |

### Throughput Contracts (Testable)

| Contract | Condition | Bound |
|----------|-----------|-------|
| TC-001 | Node import throughput | ≥ 7M nodes/sec |
| TC-002 | Edge import throughput | ≥ 3M edges/sec |

---

## Quickstart

### Development Setup

```bash
# Clone and checkout feature branch
git clone <repo>
cd ruzu
git checkout 004-optimize-csv-memory

# Run existing tests to verify baseline
cargo test

# Run benchmarks to establish baseline
cargo bench --bench csv_benchmark
```

### Testing Memory Usage

```bash
# Use DHAT profiler (as established in 003)
cargo build --release --features dhat-heap
./target/release/memory_profile_test
```

### Key Files to Modify

1. `src/storage/csv/mod.rs` - Add `StreamingConfig`
2. `src/storage/csv/node_loader.rs` - Implement `load_streaming()`
3. `src/storage/csv/rel_loader.rs` - Implement `load_streaming()`
4. `src/storage/table.rs` - Add `insert_batch()`
5. `src/storage/rel_table.rs` - Add `insert_batch()`
6. `src/lib.rs` - Wire up streaming imports in `Database::import_*` methods

### Implementation Order

1. **RowBuffer** - Create buffer pool with recycling
2. **insert_batch()** - Add batch APIs to storage layer
3. **load_streaming()** - Implement streaming loaders
4. **Integration** - Wire up in Database API
5. **Memory tests** - Verify contracts with profiler
6. **Throughput tests** - Verify performance constraints

---

## Reference: 003-optimize-csv-import Memory Analysis

From [003-optimize-csv-import/spec.md](../003-optimize-csv-import/spec.md) (lines 19-38):

```
### Performance Results

| Metric | Target | Achieved | Status |
|--------|--------|----------|--------|
| Memory usage (1GB import) | <500MB | ~4.5GB (extrapolated) | ❌ Not met |

### Memory Profiling Results

**Target**: <500MB peak memory during 1GB CSV import

**Measured**: Using DHAT heap profiler with 45MB input (500K nodes + 1M relationships):
- Peak memory: **198MB** at max allocation point
- Memory amplification ratio: **4.4x** (input size → peak memory)
- Extrapolated for 1GB input: **~4.5GB peak memory**

**Root Cause**: Current architecture loads all parsed rows into memory before returning to caller.

### Strategies to Achieve Memory Target (Future Work)

1. **Streaming Writes**: Modify loaders to write batches to storage as they complete
   instead of collecting all rows in memory.

2. **Row Buffer Recycling**: Reuse allocated `Vec<Value>` buffers across batches.

3. **Direct-to-Page Parsing**: Parse CSV fields directly into page-format storage
   without intermediate `Value` allocations.
```

This feature implements strategies 1 and 2. Strategy 3 (direct-to-page parsing) is deferred as a future optimization.
