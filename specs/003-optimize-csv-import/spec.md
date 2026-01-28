# Feature Specification: Optimize Bulk CSV Import

**Feature Branch**: `003-optimize-csv-import`
**Created**: 2025-12-06
**Status**: Complete
**Input**: User description: "Optimize bulk CSV import (COPY) for speed."

---

## Implementation Summary

### Performance Results

| Metric | Target | Achieved | Status |
|--------|--------|----------|--------|
| Node import throughput | ≥1M nodes/sec | **8.9M nodes/sec** | ✅ Exceeded |
| Edge import throughput | ≥2.5M edges/sec | **3.8M edges/sec** | ✅ Exceeded |
| Parallel speedup | ≥2x | **~4.8x** | ✅ Exceeded |
| Memory usage (1GB import) | <500MB | ~4.5GB (extrapolated) | ❌ Not met |

### Memory Profiling Results

**Target**: <500MB peak memory during 1GB CSV import

**Measured**: Using DHAT heap profiler with 45MB input (500K nodes + 1M relationships):
- Peak memory: **198MB** at max allocation point
- Memory amplification ratio: **4.4x** (input size → peak memory)
- Extrapolated for 1GB input: **~4.5GB peak memory**

**Root Cause**: Current architecture loads all parsed rows into memory before returning to caller.

### Strategies to Achieve Memory Target (Future Work)

1. **Streaming Writes**: Modify loaders to write batches to storage as they complete instead of collecting all rows in memory. This requires integrating with the storage engine during parsing.

2. **Row Buffer Recycling**: Reuse allocated `Vec<Value>` buffers across batches. After a batch is written to storage, recycle the row vectors for the next batch rather than allocating new ones.

3. **Direct-to-Page Parsing**: Parse CSV fields directly into page-format storage without intermediate `Value` allocations. This eliminates the memory amplification from parsed representation.

---

## User Scenarios & Testing

### User Story 1 - Parallel CSV Parsing and Processing (Priority: P1)

A data engineer needs to import large datasets (millions of rows) from CSV files into the graph database. The current sequential parsing approach becomes a bottleneck, especially on multi-core systems where only one CPU core is utilized during import.

**Why this priority**: Parallel processing provides the largest performance gain with the least user-facing changes. Most modern systems have multiple CPU cores that go unused during sequential imports.

**Independent Test**: Can be fully tested by importing a 1M+ row CSV file and measuring throughput improvement. Delivers immediate value by reducing import times proportionally to available CPU cores.

**Acceptance Scenarios**:

1. **Given** a CSV file with 100,000 or more rows, **When** the user runs `COPY FROM`, **Then** the system utilizes multiple CPU cores for parsing and achieves at least 2x throughput compared to single-threaded import on a multi-core system.

2. **Given** parallel parsing is enabled (default), **When** an error occurs in one parsing thread, **Then** the error is properly reported with the correct row number and the import either stops (default) or continues based on configuration.

3. **Given** a system with N CPU cores, **When** importing a large CSV, **Then** CPU utilization scales appropriately without excessive memory consumption (memory usage stays within configured limits).

---

### User Story 2 - Memory-Mapped File I/O (Priority: P2)

A data analyst imports multi-gigabyte CSV files but experiences slow initial loading due to traditional file I/O buffering. Memory-mapped file access can improve I/O performance for large files by letting the operating system handle page caching optimally.

**Why this priority**: Memory mapping provides significant I/O improvements for large files with minimal code complexity. The `memmap2` crate is already a project dependency.

**Independent Test**: Can be tested by comparing import time of a 1GB CSV file with and without memory mapping enabled. Delivers value through reduced I/O wait time.

**Acceptance Scenarios**:

1. **Given** a CSV file larger than 100MB, **When** the user runs `COPY FROM`, **Then** the system uses memory-mapped I/O by default and completes faster than traditional buffered I/O.

2. **Given** memory mapping is enabled, **When** the file is located on a slow storage medium (network drive, HDD), **Then** the system falls back gracefully to buffered I/O if memory mapping fails or performs poorly.

3. **Given** a CSV file that exceeds available physical memory, **When** importing with memory mapping, **Then** the import completes successfully without running out of memory (OS handles paging).

---

### User Story 3 - Batch Write Operations (Priority: P2)

A database administrator imports relationship data and notices that the system writes each row individually to storage, causing excessive I/O operations. Batching write operations would reduce disk I/O overhead.

**Why this priority**: Batched writes significantly reduce I/O overhead and work synergistically with parallel parsing. The batch_size configuration already exists but isn't fully utilized for write batching.

**Independent Test**: Can be tested by monitoring I/O operations during import and verifying that writes occur in batches rather than per-row. Delivers value through reduced storage I/O.

**Acceptance Scenarios**:

1. **Given** the default batch size configuration (2048 rows), **When** importing nodes or relationships, **Then** the system batches write operations to storage, reducing the number of individual write calls.

2. **Given** a custom batch size configured by the user, **When** importing data, **Then** the system respects the configured batch size for both parsing and write operations.

3. **Given** an import operation with batch writes enabled, **When** a failure occurs mid-batch, **Then** the transaction semantics are preserved (either the whole batch commits or rolls back).

---

### User Story 4 - Optimized String Handling (Priority: P3)

A user imports CSV files with many repeated string values (e.g., category fields, status codes). Each occurrence creates a new string allocation, wasting memory and time.

**Why this priority**: String interning reduces memory allocations for repetitive data, which is common in real-world datasets. This optimization complements other improvements.

**Independent Test**: Can be tested by importing a CSV with highly repetitive string columns and measuring memory usage reduction.

**Acceptance Scenarios**:

1. **Given** a CSV column with many repeated string values, **When** importing data, **Then** the system reuses string allocations for duplicate values, reducing memory usage.

2. **Given** string interning is applied, **When** querying the imported data, **Then** string comparisons remain correct and query performance is not degraded.

---

### User Story 5 - Import Progress with Performance Metrics (Priority: P3)

A user importing a large dataset wants visibility into not just progress percentage but also current import speed (rows/sec) and estimated time remaining.

**Why this priority**: Enhanced progress feedback improves user experience during long imports. This builds on existing progress callback infrastructure.

**Independent Test**: Can be tested by running an import with progress callbacks and verifying speed metrics are accurate and ETA updates appropriately.

**Acceptance Scenarios**:

1. **Given** a progress callback is configured, **When** importing data, **Then** the progress updates include current throughput (rows/sec) and estimated time remaining.

2. **Given** import speed varies during processing, **When** progress is reported, **Then** the throughput metric reflects a reasonable moving average, not instantaneous spikes.

---

### Edge Cases

- What happens when parsing threads encounter different encodings or malformed UTF-8 in parallel?
- How does the system handle CSV files with inconsistent row lengths when using memory mapping?
- What happens if memory mapping fails (e.g., file permissions, unsupported filesystem)?
- How are very wide rows (many columns or large string values) handled with batching?
- What happens when disk space runs out mid-batch during a write operation?

## Requirements

### Functional Requirements

- **FR-001**: System MUST support parallel CSV parsing utilizing multiple CPU cores.
- **FR-002**: System MUST support memory-mapped file I/O for CSV files.
- **FR-003**: System MUST batch write operations to storage to reduce I/O overhead.
- **FR-004**: System MUST provide fallback to sequential/buffered processing when parallel/mmap approaches are not available or fail.
- **FR-005**: System MUST maintain correct row numbering for error reporting when using parallel parsing.
- **FR-006**: System MUST preserve transaction semantics (commit/rollback) when using batch writes.
- **FR-007**: System SHOULD support string interning for repeated string values during import.
- **FR-008**: System MUST report import throughput (rows/sec) and estimated time remaining in progress callbacks.
- **FR-009**: System MUST allow configuration of parallelism level (number of threads).
- **FR-010**: System MUST remain backward compatible with existing `COPY FROM` syntax and behavior.

### Key Entities

- **CsvImportConfig**: Configuration for CSV import including parallelism settings, batch sizes, and feature toggles.
- **ImportProgress**: Extended to include throughput metrics and ETA calculations.
- **ParsedBatch**: A batch of parsed rows ready for writing to storage.
- **StringInterner**: (Optional) Shared data structure for deduplicating string values during import.

## Success Criteria

### Measurable Outcomes

Based on current benchmarks (ruzu: 1.07M nodes/sec, 820K edges/sec) and KuzuDB reference (769K nodes/sec, 5.3M edges/sec):

- **SC-001**: Node import throughput maintains or improves current ~1M+ nodes/sec performance (already exceeds KuzuDB).
- **SC-002**: Edge/relationship import throughput reaches at least 2.5M edges/sec (3x improvement, closing gap with KuzuDB's 5.3M edges/sec).
- **SC-003**: Edge import achieves at least 50% of KuzuDB's throughput (target: 2.65M edges/sec) as a stretch goal.
- **SC-004**: Memory usage during import of a 1GB CSV file stays below 500MB (excluding OS file cache).
- **SC-005**: Import of 2.4M edges completes in under 1 second (matching KuzuDB study dataset).
- **SC-006**: Users can see accurate progress updates including rows/sec and ETA during imports.
- **SC-007**: Existing tests continue to pass with no breaking changes to the `COPY FROM` syntax.

## Assumptions

- Modern systems have at least 4 CPU cores available for parallel processing.
- The `memmap2` and `crossbeam` crates (already project dependencies) are suitable for this optimization work.
- CSV files are stored on local or reasonably fast storage (SSD, fast HDD, or cached network drive).
- Most real-world CSV imports have file sizes ranging from megabytes to low gigabytes.
- The current CSV parsing using the `csv` crate is single-threaded and represents a bottleneck.
- WAL and transaction mechanisms can accommodate batch writes without modification to their core design.

## Out of Scope

- Distributed/cluster-based CSV import across multiple machines.
- Streaming import from remote URLs or cloud storage (S3, GCS).
- Support for compressed CSV files (gzip, zstd) during import.
- Changes to the COPY FROM SQL syntax (syntax remains unchanged).
- Index building optimization during import (no indexes currently supported).
