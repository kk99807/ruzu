# Research: Optimize Bulk CSV Import

**Feature**: 003-optimize-csv-import
**Date**: 2025-12-06
**Status**: Complete

This document resolves all NEEDS CLARIFICATION items from the Technical Context and provides implementation decisions.

## Research Summary

| Topic | Decision | Rationale |
|-------|----------|-----------|
| Parallelism Library | crossbeam + rayon | crossbeam for channels, rayon for par_iter on batches |
| Parallel Strategy | Block-based file splitting | Matches KuzuDB's PARALLEL_BLOCK_SIZE approach |
| Memory Mapping | memmap2 (existing dep) | Already a project dependency, proven approach |
| String Interning | Standard HashMap + Rc<str> | Simple first (Constitution Principle V) |
| CSV Library | Keep csv crate | Highly optimized, safe, handles edge cases |

---

## 1. Parallel CSV Parsing Strategy

### Decision: Block-Based Parallel Reading

After researching how KuzuDB implements parallel CSV reading (see [parallel_csv_reader.cpp](C:\dev\kuzu\src\processor\operator\persistent\reader\csv\parallel_csv_reader.cpp)), the approach is:

1. **Split file into fixed-size blocks** (KuzuDB uses `PARALLEL_BLOCK_SIZE`)
2. **Each thread processes one block** starting at block boundary
3. **Non-block-0 threads seek to next newline** before parsing
4. **Thread finishes when it crosses into next block** (at a row boundary)

### Key Implementation Details from KuzuDB

```
Block boundaries handled by:
1. seekToBlockStart() - seeks to block offset, then scans for newline
2. finishedBlock() - checks if getFileOffset() > (currentBlockIdx + 1) * BLOCK_SIZE
3. handleQuotedNewline() - DISALLOWS quoted newlines in parallel mode
```

### Limitation Accepted

**Quoted newlines are NOT supported in parallel mode**. KuzuDB explicitly throws an error:
> "Quoted newlines are not supported in parallel CSV reader. Please specify PARALLEL=FALSE in the options."

This is acceptable for ruzu because:
- Real-world CSV data rarely has quoted newlines
- Users can fall back to sequential mode for such files
- Matches KuzuDB behavior (our reference implementation)

### Alternative Rejected: Sequential Parse + Parallel Processing

**Why rejected**: This approach only parallelizes the processing after parsing, but CSV parsing is already fast. The bottleneck is I/O and the csv crate is highly optimized. Block-based parallel reading provides better I/O parallelism.

---

## 2. Parallelism Library Choice

### Decision: crossbeam + rayon

| Library | Use Case | Why |
|---------|----------|-----|
| **crossbeam** | Thread-safe channels, scoped threads | Already a dependency, lock-free primitives |
| **rayon** | Parallel iteration on batches | Work-stealing for balanced load |

### Rayon vs Crossbeam Comparison

| Aspect | Rayon | Crossbeam |
|--------|-------|-----------|
| **Best For** | Data-parallel workloads | Fine-grained concurrency control |
| **Architecture** | Work-stealing thread pool | Lower-level primitives |
| **Overhead** | Negligible for 100K+ elements | Lower per-operation |
| **Our Use** | Process parsed batches | Block assignment channels |

### Implementation Pattern

```rust
// Pseudocode for block-based parallel CSV reading
use crossbeam::channel;
use rayon::prelude::*;

// Shared state manages block assignments
let (block_sender, block_receiver) = channel::bounded(num_threads);

// Pre-populate with block indices
for block_idx in 0..num_blocks {
    block_sender.send(block_idx).unwrap();
}
drop(block_sender);

// Each thread claims blocks and processes them
let results: Vec<_> = (0..num_threads)
    .into_par_iter()
    .flat_map(|_| {
        let mut local_results = Vec::new();
        while let Ok(block_idx) = block_receiver.try_recv() {
            let rows = process_block(mmap, block_idx, block_size);
            local_results.extend(rows);
        }
        local_results
    })
    .collect();
```

### Dependencies to Add

```toml
# In Cargo.toml - crossbeam already exists, add rayon
rayon = "1.8"
```

---

## 3. Memory-Mapped I/O

### Decision: Use memmap2 for Files > 100MB

The `memmap2` crate is already a project dependency. Memory mapping provides:
- Reduced file I/O overhead (OS page cache management)
- Zero-copy potential with `csv::ByteRecord`
- Large file support via OS virtual memory

### Implementation Pattern

```rust
use memmap2::Mmap;
use std::fs::File;

pub fn mmap_csv_file(path: &Path) -> Result<Mmap> {
    let file = File::open(path)?;
    let metadata = file.metadata()?;

    // Only mmap files > 100MB (threshold from spec)
    if metadata.len() < 100 * 1024 * 1024 {
        return Err(/* fallback to buffered I/O */);
    }

    // SAFETY: File is opened read-only, no concurrent writes
    unsafe { Mmap::map(&file) }
        .map_err(|e| RuzuError::StorageError(format!("Failed to mmap: {}", e)))
}
```

### Fallback Strategy

Mmap can fail on:
- Network drives
- Files larger than available address space (32-bit)
- Certain filesystems (NFS edge cases)

When mmap fails, fall back to buffered `BufReader`. This aligns with **FR-004** from the spec.

---

## 4. String Interning for Repeated Values

### Decision: Optional HashMap-based Interning

For columns with high cardinality (categories, enums), string interning reduces allocations.

### Simple Implementation

```rust
use std::collections::HashMap;
use std::rc::Rc;

pub struct StringInterner {
    strings: HashMap<Box<str>, Rc<str>>,
}

impl StringInterner {
    pub fn intern(&mut self, s: &str) -> Rc<str> {
        if let Some(existing) = self.strings.get(s) {
            return Rc::clone(existing);
        }
        let rc: Rc<str> = s.into();
        self.strings.insert(s.into(), Rc::clone(&rc));
        rc
    }
}
```

### When to Use

| Scenario | Interning | Rationale |
|----------|-----------|-----------|
| Primary keys (unique) | No | No benefit, extra overhead |
| Category columns | Yes | High repetition |
| Free-text columns | No | Low repetition |

**Configuration**: Add `intern_strings: bool` to `CsvImportConfig` (default: false).

---

## 5. Batch Write Operations

### Decision: Extend Existing batch_size

The existing `CsvImportConfig.batch_size` (default: 2048) controls progress reporting. Extend to also control:
- WAL write batching
- Buffer pool flush frequency

### Batch Write Strategy

```rust
// Accumulate rows until batch_size reached
let mut pending_rows: Vec<Vec<Value>> = Vec::with_capacity(config.batch_size);

for row in parsed_rows {
    pending_rows.push(row);
    if pending_rows.len() >= config.batch_size {
        write_batch_to_storage(&pending_rows)?;
        pending_rows.clear();
    }
}

// Final partial batch
if !pending_rows.is_empty() {
    write_batch_to_storage(&pending_rows)?;
}
```

### Transaction Semantics

From **FR-006**: "System MUST preserve transaction semantics (commit/rollback) when using batch writes."

Implementation:
- Each batch is a single WAL transaction
- On failure, rollback only the current batch
- Previously committed batches remain

---

## 6. Progress Reporting with Throughput

### Decision: Extend ImportProgress with Timing

Current `ImportProgress` lacks timing for throughput calculation. Add:

```rust
pub struct ImportProgress {
    // Existing fields...
    pub rows_processed: u64,
    pub rows_total: Option<u64>,
    pub bytes_read: u64,

    // NEW: Timing for throughput
    pub start_time: Option<Instant>,
    pub last_update_time: Option<Instant>,
    pub last_row_count: u64,
}

impl ImportProgress {
    /// Calculate current throughput (rows/sec)
    pub fn throughput(&self) -> Option<f64> {
        let start = self.start_time?;
        let elapsed = start.elapsed().as_secs_f64();
        if elapsed > 0.0 {
            Some(self.rows_processed as f64 / elapsed)
        } else {
            None
        }
    }

    /// Calculate ETA based on current throughput
    pub fn eta_seconds(&self) -> Option<f64> {
        let remaining = self.rows_total? - self.rows_processed;
        let throughput = self.throughput()?;
        if throughput > 0.0 {
            Some(remaining as f64 / throughput)
        } else {
            None
        }
    }

    /// Calculate smoothed throughput (moving average)
    pub fn smoothed_throughput(&self) -> Option<f64> {
        // Use exponential moving average to smooth spikes
        // Implementation details...
    }
}
```

---

## 7. KuzuDB Reference Implementation Analysis

### Key Files Reviewed

| File | Purpose |
|------|---------|
| `parallel_csv_reader.h` | ParallelCSVReader class definition |
| `parallel_csv_reader.cpp` | Block-based parallel implementation |
| `base_csv_reader.h` | Base CSV reader with common functionality |

### Block Size

KuzuDB uses `CopyConstants::PARALLEL_BLOCK_SIZE` (likely 256KB-1MB based on typical values).

For ruzu, recommended block size: **256KB** (262,144 bytes)
- Large enough to amortize thread coordination overhead
- Small enough for good parallelism on 100K row files

### Error Handling Pattern

KuzuDB uses `LocalFileErrorHandler` per thread, aggregating errors via shared state:

```cpp
// Each thread has local error handler
localState->errorHandler = std::make_unique<LocalFileErrorHandler>(
    &sharedState->errorHandlers[fileIdx],
    sharedState->csvOption.ignoreErrors,
    sharedState->context,
    true /* parallel mode */
);
```

For ruzu, implement similar pattern with thread-local error collection.

---

## 8. Performance Targets Analysis

### Current Baseline (from benchmarks)

| Metric | Current | Target |
|--------|---------|--------|
| Node import | ~1.07M nodes/sec | Maintain (already exceeds KuzuDB's 769K) |
| Edge import | ~820K edges/sec | 2.5M edges/sec (3x improvement) |

### Expected Improvements

| Optimization | Expected Impact |
|--------------|-----------------|
| Parallel parsing (4 threads) | 2-3x for I/O bound files |
| Memory mapping | 10-20% I/O reduction |
| ByteRecord (zero-copy) | ~30% parsing speedup |
| Batch writes | 20-50% write reduction |

### Realistic Target

With all optimizations, expect:
- **Edge import**: 2-3M edges/sec (achievable with 4+ cores)
- **Node import**: Maintain 1M+ nodes/sec

---

## 9. Risk Analysis

### High Risk

| Risk | Mitigation |
|------|------------|
| Parallel parsing introduces row numbering bugs | Extensive tests with known error positions |
| Memory mapping fails on some platforms | Fallback to buffered I/O (FR-004) |

### Medium Risk

| Risk | Mitigation |
|------|------------|
| Performance regression for small files | Auto-detect file size, skip parallel for <10MB |
| String interning memory overhead | Make optional, off by default |

### Low Risk

| Risk | Mitigation |
|------|------------|
| Rayon dependency adds complexity | Widely used, stable crate |
| Batch writes break transaction semantics | Test with crash injection |

---

## 10. Implementation Order

### Phase 1: Foundation (User Stories 2, 3)
1. Memory-mapped file reader (`mmap_reader.rs`)
2. Batch write infrastructure
3. Tests for mmap fallback

### Phase 2: Parallelism (User Story 1)
1. Block-based file splitting
2. Parallel parsing infrastructure (`parallel.rs`)
3. Thread-local error aggregation
4. Row number tracking across blocks

### Phase 3: Enhancements (User Stories 4, 5)
1. String interning (optional)
2. Progress throughput metrics
3. ETA calculation

### Phase 4: Integration & Benchmarks
1. Update `NodeLoader` and `RelLoader`
2. Add parallel benchmarks
3. Performance validation against targets

---

## Sources

- KuzuDB parallel_csv_reader.cpp: C:\dev\kuzu\src\processor\operator\persistent\reader\csv\parallel_csv_reader.cpp
- rust-csv documentation: https://docs.rs/csv/
- memmap2 documentation: https://docs.rs/memmap2/
- Rayon documentation: https://docs.rs/rayon/
- Crossbeam documentation: https://docs.rs/crossbeam/
- KuzuDB COPY documentation: https://docs.kuzudb.com/import/csv/
- DuckDB parallel CSV: https://duckdb.org/docs/stable/data/overview
