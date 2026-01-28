# Implementation Plan: Persistent Storage with Edge Support

**Branch**: `002-persistent-storage` | **Date**: 2025-12-06 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/002-persistent-storage/spec.md`

## Summary

This phase implements disk-based persistence for ruzu, transforming it from a PoC demo into a real database. The implementation includes buffer pool management with LRU eviction, Write-Ahead Logging (WAL) for crash recovery, catalog persistence, relationship/edge storage using CSR format, and bulk CSV ingestion. Following the Port-First principle, this design closely mirrors the KuzuDB C++ implementation while using idiomatic Rust patterns.

## Technical Context

**Language/Version**: Rust 1.75+ (stable, 2021 edition)
**Primary Dependencies**: pest (parser, existing), memmap2 (mmap), parking_lot (faster locks), crossbeam (lock-free queues), serde + bincode (serialization), csv (parsing)
**Storage**: Custom page-based file format with 4KB pages, WAL, and catalog stored in reserved header pages
**Testing**: cargo test (unit + integration), criterion (benchmarks), existing Phase 0 test suite
**Target Platform**: Windows x86_64 (primary dev), Linux/macOS x86_64 and aarch64 (CI)
**Project Type**: Single Rust library crate with binary examples
**Performance Goals**: 50K nodes/sec CSV import, 100K relationships/sec CSV import, <30 sec crash recovery for 10GB DB
**Constraints**: Buffer pool configurable (default 256MB or 80% RAM), single-writer transactions, little-endian platforms only
**Scale/Scope**: Support databases up to 10GB with 4x overcommit (40GB data in 256MB buffer pool)

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

### I. Port-First (Reference Implementation) ✅

| Requirement | Status | Evidence |
|-------------|--------|----------|
| Reference C++ implementation as authoritative design | ✅ PASS | KuzuDB c:/dev/kuzu used for buffer manager, WAL, CSR designs |
| Preserve core algorithms and data structures | ✅ PASS | Using KuzuDB's page state machine, second-chance eviction, CSR format |
| Document deviations with rationale | ✅ PASS | See Complexity Tracking for Rust-idiomatic changes |
| No alternative research when C++ provides solution | ✅ PASS | Storage design follows KuzuDB exactly |

### II. TDD with Red-Green-Refactor ✅

| Requirement | Status | Evidence |
|-------------|--------|----------|
| Write tests FIRST | ✅ PLANNED | tasks.md will specify test-first workflow |
| Red-Green-Refactor workflow | ✅ PLANNED | Each component starts with failing tests |
| Contract → Integration → Unit priority | ✅ PASS | File format contracts tested first |

### III. Benchmarking & Performance Tracking ✅

| Requirement | Status | Evidence |
|-------------|--------|----------|
| Baseline benchmarks established | ✅ EXISTS | Phase 0 has e2e_benchmark, parse_benchmark, storage_benchmark |
| criterion for micro-benchmarks | ✅ EXISTS | Already in dev-dependencies |
| Performance within 5x of C++ (Phase 1 target) | ⚠️ TBD | Will validate during implementation |
| CI/CD integration | ✅ PLANNED | Benchmarks run in PR checks |

### IV. Rust Best Practices & Idioms ✅

| Requirement | Status | Evidence |
|-------------|--------|----------|
| cargo clippy with zero warnings | ✅ EXISTS | Phase 0 enforces `clippy::pedantic` |
| rustfmt standard formatting | ✅ EXISTS | Already configured |
| Safe Rust preferred | ✅ PLANNED | mmap requires unsafe, but isolated to buffer_manager module |
| Result<T,E> error handling | ✅ EXISTS | RuzuError enum already defined |
| Public APIs documented | ✅ PLANNED | All new public APIs will have `///` docs |

### V. Safety & Correctness Over Performance ✅

| Requirement | Status | Evidence |
|-------------|--------|----------|
| Correctness prioritized | ✅ PASS | Simple LRU before clock algorithm |
| Property-based testing for invariants | ✅ PLANNED | proptest for buffer pool, WAL replay |
| Unsafe code justified | ✅ PLANNED | SAFETY comments required for mmap code |

**Constitution Check Result**: ✅ PASS - All principles satisfied or planned with clear implementation path.

## Project Structure

### Documentation (this feature)

```text
specs/002-persistent-storage/
├── plan.md              # This file (/speckit.plan command output)
├── research.md          # Phase 0 output - KuzuDB internals analysis
├── data-model.md        # Phase 1 output - entity definitions
├── quickstart.md        # Phase 1 output - developer guide
├── contracts/           # Phase 1 output - API contracts
│   └── storage-format.md  # Binary format specification
└── tasks.md             # Phase 2 output (/speckit.tasks command)
```

### Source Code (repository root)

```text
src/
├── lib.rs               # (existing) Database entry point
├── error.rs             # (existing) Error types - extend for storage errors
├── types/               # (existing) Value types
├── parser/              # (existing) Cypher parser
├── catalog/             # (existing) Schema management - add persistence
│   ├── mod.rs
│   └── schema.rs
├── binder/              # (existing) Semantic analysis
├── executor/            # (existing) Query execution - extend for relationships
├── storage/             # (extend) Core storage layer
│   ├── mod.rs           # (existing) Module exports
│   ├── column.rs        # (existing) In-memory columns
│   ├── table.rs         # (existing) In-memory node tables
│   ├── buffer_pool/     # (NEW) Buffer pool management
│   │   ├── mod.rs       # BufferPool, PageHandle exports
│   │   ├── page_state.rs # 4-state machine (EVICTED/LOCKED/MARKED/UNLOCKED)
│   │   ├── buffer_frame.rs # Frame metadata
│   │   ├── eviction.rs  # LRU eviction queue
│   │   └── vm_region.rs # mmap'd memory region
│   ├── page/            # (NEW) Page-level I/O
│   │   ├── mod.rs
│   │   ├── page_id.rs   # Page addressing
│   │   └── disk_manager.rs # File I/O abstraction
│   ├── wal/             # (NEW) Write-ahead logging
│   │   ├── mod.rs
│   │   ├── record.rs    # WAL record types
│   │   ├── writer.rs    # WAL append-only writer
│   │   ├── reader.rs    # WAL replay reader
│   │   └── checkpointer.rs # Checkpoint coordination
│   ├── node_table.rs    # (NEW) Persistent node storage
│   ├── rel_table.rs     # (NEW) Persistent relationship storage (CSR)
│   └── csv/             # (NEW) Bulk CSV import
│       ├── mod.rs
│       ├── parser.rs    # CSV parsing with options
│       ├── node_loader.rs # Node bulk insert
│       └── rel_loader.rs  # Relationship bulk insert

tests/
├── contract/            # (NEW) File format compatibility tests
│   ├── test_page_format.rs
│   ├── test_wal_format.rs
│   └── test_catalog_format.rs
├── integration/         # (NEW) Multi-component tests
│   ├── test_persistence.rs  # Create, close, reopen, query
│   ├── test_crash_recovery.rs # WAL replay after crash
│   ├── test_buffer_pool.rs   # Eviction under memory pressure
│   └── test_csv_import.rs    # Bulk loading workflows
└── unit/                # Module-level tests (in src/ via #[cfg(test)])
```

**Structure Decision**: Extending the existing single-project structure from Phase 0. New storage components are organized into submodules (buffer_pool/, page/, wal/, csv/) for clear separation of concerns while maintaining a flat crate structure.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| `unsafe` code in buffer_pool | mmap requires unsafe for memory-mapped I/O | No safe alternative for mmap; isolated to vm_region.rs with SAFETY comments |
| Deviation: Rust-native file format | C++ format uses raw pointers, C++ struct layouts | Rust serde/bincode is safer and more maintainable; migration tool can be added later |
| Deviation: LRU instead of clock | Simpler correctness-first approach | Will upgrade to second-chance in Phase 4 if benchmarks show need |
