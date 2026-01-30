# ruzu Development Guidelines

Auto-generated from all feature plans. Last updated: 2025-12-05

## Active Technologies
- Rust 1.75+ (stable, 2021 edition) + pest (parser, existing), memmap2 (mmap), parking_lot (faster locks), crossbeam (lock-free queues), serde + bincode (serialization), csv (parsing) (002-persistent-storage)
- Custom page-based file format with 4KB pages, WAL, and catalog stored in reserved header pages (002-persistent-storage)
- Rust 1.75+ (stable, 2021 edition) + csv (parsing), memmap2 (mmap), crossbeam (parallel processing), parking_lot (synchronization), rayon (NEEDS CLARIFICATION - may be added for parallel iteration) (003-optimize-csv-import)
- Custom page-based file format with 4KB pages, WAL, buffer pool (003-optimize-csv-import)
- Rust 1.75+ (stable, 2021 edition) + csv (parsing), memmap2 (mmap), crossbeam (parallel processing), parking_lot (synchronization), rayon (parallel iteration) (004-optimize-csv-memory)
- Custom page-based format with 4KB pages, WAL, buffer pool (Phase 1 complete) (005-query-engine)
- Rust 1.75+ (stable, 2021 edition) + serde + bincode (serialization, already in use), parking_lot (synchronization, already in use) (001-fix-rel-persistence)
- Custom page-based file format with 4KB pages, WAL, buffer pool (already implemented) (001-fix-rel-persistence)
- Rust 1.75+ (stable, 2021 edition) + pest (parser), serde + bincode (serialization), parking_lot (locks), csv (parsing), memmap2 (mmap) (006-add-datatypes)

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
- 006-add-datatypes: Added Rust 1.75+ (stable, 2021 edition) + pest (parser), serde + bincode (serialization), parking_lot (locks), csv (parsing), memmap2 (mmap)
- 001-fix-rel-persistence: Added Rust 1.75+ (stable, 2021 edition) + serde + bincode (serialization, already in use), parking_lot (synchronization, already in use)
- 005-query-engine: Added Rust 1.75+ (stable, 2021 edition)


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
## Feature 001-fix-rel-persistence Completion

**Status**: Complete (2026-01-30)

### Bug Fix Summary

Fixed critical data loss bug where relationship table data was not persisted to or loaded from
disk during database open/close operations. All relationship data was silently lost after restart.

### Changes

1. **Relationship Table Persistence** (User Story 1 - P1)
   - Added `load_rel_table_data()` to deserialize rel tables from page 3
   - Modified `save_all_data()` to serialize rel tables to page 3
   - Modified `Database::open()` to load rel tables on startup
   - Length-prefixed bincode format matching node table pattern

2. **CSV-Imported Relationships Persist** (User Story 2 - P2)
   - Verified COPY FROM relationships survive restart
   - Tested with bulk imports across multiple rel tables

3. **WAL Recovery for Relationships** (User Story 3 - P3)
   - Extended `replay_wal()` to handle CreateRel and InsertRel operations
   - Committed relationships restored after crash; uncommitted rolled back

4. **Version Migration** (Phase 6)
   - DatabaseHeader version 1 → 2 migration with `rel_metadata_range` field
   - Version 1 databases open seamlessly with empty rel tables

5. **Error Handling**
   - `RelTableLoadError` and `RelTableCorrupted` error variants
   - Fail-fast on corruption with explicit error messages

### Files Modified

- `src/lib.rs` - load/save rel tables, WAL replay
- `src/storage/mod.rs` - DatabaseHeader v2, migration
- `src/error.rs` - New error variants
- `src/storage/rel_table.rs` - Debug assertions for CSR invariants
- `tests/contract_tests.rs` - Format stability tests
- `tests/integration_tests.rs` - Persistence and recovery tests

### Test Coverage

- 440 total tests passing (140 unit + 116 contract + 108 integration + 76 lib)
- 11 persistence-specific tests
- 15 crash recovery tests (including WAL replay for relationships)

### Performance Benchmarks

- `cargo bench --bench rel_persist_benchmark` - Open time and query after restart
- No regression on existing benchmarks (<5% threshold met)

### Known Limitations

- Relationship metadata must fit within single 4KB page (~4092 bytes usable)
- Sufficient for ~8 relationship tables in typical databases
<!-- MANUAL ADDITIONS END -->
