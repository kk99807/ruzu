# Implementation Plan: Multi-Page Storage

**Branch**: `007-multi-page-storage` | **Date**: 2026-01-30 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/007-multi-page-storage/spec.md`

## Summary

Remove the single-page (4KB) limit for catalog, node table, and relationship table metadata storage. Currently each metadata type is serialized to a fixed page (1, 2, or 3) and must fit within 4092 bytes. This feature introduces dynamic multi-page allocation so metadata sections can span as many contiguous pages as needed, using the existing `PageRange` struct in `DatabaseHeader` which already tracks `(start_page, num_pages)`.

The approach follows KuzuDB's reference implementation pattern: serialize metadata to an in-memory buffer, calculate the number of pages needed, allocate a contiguous page range via `DiskManager`, write across multiple pages, and update the header's `PageRange` fields.

## Technical Context

**Language/Version**: Rust 1.75+ (stable, 2021 edition)
**Primary Dependencies**: serde + bincode (serialization), parking_lot (locks), memmap2 (mmap), uuid
**Storage**: Custom page-based file format with 4KB pages, WAL, buffer pool
**Testing**: `cargo test` (440 existing tests: 140 unit + 116 contract + 108 integration + 76 lib)
**Target Platform**: Linux (x86_64, aarch64), macOS (x86_64, aarch64), Windows (x86_64)
**Project Type**: Single Rust library crate
**Performance Goals**: Save/load within 2x of current single-page operations for equivalent data sizes (SC-006)
**Constraints**: Single-writer model, contiguous page allocation only, no fragmentation support
**Scale/Scope**: Must handle at least 1MB of node and relationship data (SC-001, SC-002)

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

### Principle I: Port-First (Reference Implementation)

- **Status**: PASS
- KuzuDB C++ reference uses `PageRange` (startPageIdx + numPages) in `DatabaseHeader` for multi-page catalog and metadata storage
- KuzuDB uses `InMemFileWriter` pattern: serialize to memory buffer → calculate page count → allocate range → flush to pages
- KuzuDB uses `PageManager.allocatePageRange(numPages)` for contiguous allocation
- Our approach mirrors this pattern using existing `PageRange` struct and `DiskManager.allocate_page()`

### Principle II: TDD with Red-Green-Refactor

- **Status**: WILL COMPLY
- Tests will be written first for each user story
- Contract tests for multi-page serialization format stability
- Integration tests for persistence across restart, crash recovery, backward compatibility
- Unit tests for page allocator and multi-page read/write helpers

### Principle III: Benchmarking & Performance Tracking

- **Status**: WILL COMPLY
- Existing benchmarks (`rel_persist_benchmark`, `storage_benchmark`) will verify no regression
- New benchmark for multi-page save/load with 1MB+ data to validate SC-006

### Principle IV: Rust Best Practices & Idioms

- **Status**: WILL COMPLY
- `cargo clippy` zero warnings maintained
- No new `unsafe` code required
- Uses existing error types with new variants as needed

### Principle V: Safety & Correctness Over Performance

- **Status**: WILL COMPLY
- Simple contiguous allocation (no free-space management or fragmentation)
- Page range validation on load (no overlaps, within file bounds)
- Correctness verified by property-based tests and contract tests

### Post-Design Re-check

- **Status**: PASS — No violations identified. All design choices align with constitution principles.

## Project Structure

### Documentation (this feature)

```text
specs/007-multi-page-storage/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/           # Phase 1 output
│   └── storage-api.md   # Internal API contracts
└── tasks.md             # Phase 2 output (via /speckit.tasks)
```

### Source Code (repository root)

```text
src/
├── lib.rs               # Database open/close, save_all_data, load methods (MODIFY)
├── storage/
│   ├── mod.rs           # DatabaseHeader, PageRange, CURRENT_VERSION (MODIFY)
│   ├── table.rs         # NodeTable, TableData (NO CHANGE)
│   ├── rel_table.rs     # RelTable, RelTableData (NO CHANGE)
│   ├── column.rs        # ColumnStorage (NO CHANGE)
│   ├── buffer_pool/
│   │   └── mod.rs       # BufferPool (MODIFY - add allocate_page_range)
│   ├── page/
│   │   ├── mod.rs       # Page constants (NO CHANGE)
│   │   ├── disk_manager.rs  # DiskManager (MODIFY - add allocate_page_range)
│   │   └── page_id.rs   # PageId (NO CHANGE)
│   ├── wal/             # WAL (NO CHANGE - logical operations unchanged)
│   └── csv/             # CSV import (NO CHANGE)
├── catalog/
│   ├── mod.rs           # (NO CHANGE)
│   └── schema.rs        # Catalog (NO CHANGE)
├── error.rs             # Error types (MODIFY - add new variants)
└── types/               # Value types (NO CHANGE)

tests/
├── contract_tests.rs    # (ADD multi-page format stability tests)
├── integration_tests.rs # (ADD multi-page persistence & recovery tests)
└── unit_tests.rs        # (ADD page allocator & multi-page read/write tests)
```

**Structure Decision**: Single Rust crate, modifications concentrated in `src/lib.rs` (save/load logic) and `src/storage/mod.rs` (header version bump, PageRange helpers). The data model types (`TableData`, `RelTableData`, `Catalog`) remain unchanged — only the I/O layer changes.

## Complexity Tracking

No constitution violations. No complexity justifications needed.
