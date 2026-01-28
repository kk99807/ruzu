# ruzu Development Guidelines

Auto-generated from all feature plans. Last updated: 2025-12-05

## Active Technologies
- Rust 1.75+ (stable, 2021 edition) + pest (parser, existing), memmap2 (mmap), parking_lot (faster locks), crossbeam (lock-free queues), serde + bincode (serialization), csv (parsing) (002-persistent-storage)
- Custom page-based file format with 4KB pages, WAL, and catalog stored in reserved header pages (002-persistent-storage)
- Rust 1.75+ (stable, 2021 edition) + csv (parsing), memmap2 (mmap), crossbeam (parallel processing), parking_lot (synchronization), rayon (NEEDS CLARIFICATION - may be added for parallel iteration) (003-optimize-csv-import)
- Custom page-based file format with 4KB pages, WAL, buffer pool (003-optimize-csv-import)
- Rust 1.75+ (stable, 2021 edition) + csv (parsing), memmap2 (mmap), crossbeam (parallel processing), parking_lot (synchronization), rayon (parallel iteration) (004-optimize-csv-memory)
- Custom page-based format with 4KB pages, WAL, buffer pool (Phase 1 complete) (005-query-engine)

- Rust 1.75+ (stable, 2021 edition) + pest (parser), criterion (benchmarks), Apache Arrow (deferred for PoC) (001-poc-basic-functionality)

## Project Structure

```text
src/
tests/
```

## Commands

cargo test; cargo clippy

## Code Style

Rust 1.75+ (stable, 2021 edition): Follow standard conventions

## Recent Changes
- 005-query-engine: Added Rust 1.75+ (stable, 2021 edition)
- 004-optimize-csv-memory: Added Rust 1.75+ (stable, 2021 edition) + csv (parsing), memmap2 (mmap), crossbeam (parallel processing), parking_lot (synchronization), rayon (parallel iteration)
- 003-optimize-csv-import: Added Rust 1.75+ (stable, 2021 edition) + csv (parsing), memmap2 (mmap), crossbeam (parallel processing), parking_lot (synchronization), rayon (NEEDS CLARIFICATION - may be added for parallel iteration)


<!-- MANUAL ADDITIONS START -->

## Feature 002-persistent-storage Completion (Phase 1)

**Status**: Complete (2025-12-06)

### Implemented Features

1. **Database Persistence** (User Story 1)
   - File-based storage with 4KB pages
   - Catalog serialization/deserialization
   - Schema and data survive restarts

2. **Crash Recovery with WAL** (User Story 2)
   - Write-ahead logging with checksums
   - Transaction commit/abort semantics
   - Automatic replay on database open
   - Checkpointing support

3. **Relationship/Edge Support** (User Story 3)
   - CREATE REL TABLE syntax
   - CSR-based edge storage (forward + backward)
   - Relationship properties
   - Bidirectional traversal

4. **Bulk CSV Import** (User Story 4)
   - COPY FROM syntax
   - Node and relationship import
   - Progress callbacks
   - Error handling with continue-on-error option

5. **Memory-Constrained Operation** (User Story 5)
   - Buffer pool with LRU eviction
   - Page pinning/unpinning
   - Transparent page reload
   - Concurrent access with parking_lot RwLock

### Test Coverage

- 97 unit tests
- 57 contract tests
- 71 integration tests
- 2 proptest property-based tests

### Performance Benchmarks

Available benchmarks:
- `cargo bench --bench csv_benchmark` - CSV import (target: 50K nodes/sec)
- `cargo bench --bench buffer_benchmark` - Buffer pool operations
- `cargo bench --bench storage_benchmark` - Storage operations
- `cargo bench --bench parse_benchmark` - Query parsing
- `cargo bench --bench e2e_benchmark` - End-to-end operations

### Known Limitations

- No concurrent transactions (single-writer)
- No B-tree indexes (scan-only queries)
- No multi-hop traversals (single hop only)
- No aggregation functions (COUNT, SUM, etc.)
<!-- MANUAL ADDITIONS END -->
