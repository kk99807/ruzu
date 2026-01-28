# Implementation Plan: Optimize Bulk CSV Import

**Branch**: `003-optimize-csv-import` | **Date**: 2025-12-06 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/003-optimize-csv-import/spec.md`

## Summary

Optimize the existing CSV import system to achieve at least 2.5M edges/sec throughput (3x improvement over current ~820K edges/sec) and maintain node import performance at 1M+ nodes/sec. The optimization focuses on parallel CSV parsing using crossbeam channels, memory-mapped I/O using the existing memmap2 dependency, batch write operations, and enhanced progress reporting with throughput metrics.

## Technical Context

**Language/Version**: Rust 1.75+ (stable, 2021 edition)
**Primary Dependencies**: csv (parsing), memmap2 (mmap), crossbeam (parallel processing), parking_lot (synchronization), rayon (to be added for parallel batch processing)
**Storage**: Custom page-based file format with 4KB pages, WAL, buffer pool
**Testing**: cargo test, criterion (benchmarks), proptest (property-based)
**Target Platform**: Linux (x86_64, aarch64), macOS (x86_64, aarch64), Windows (x86_64)
**Project Type**: Single Rust library/CLI project
**Performance Goals**:
- Node import: Maintain 1M+ nodes/sec (currently ~1.07M nodes/sec)
- Edge import: Achieve 2.5M+ edges/sec (currently ~820K edges/sec, target is 3x improvement)
- Edge import stretch goal: 2.65M edges/sec (50% of KuzuDB's 5.3M edges/sec)

**Constraints**:
- Memory usage <500MB during 1GB CSV import (excluding OS file cache)
- 2.4M edges import must complete in <1 second
- Backward compatible with existing COPY FROM syntax

**Scale/Scope**: Optimizing ~4 files in src/storage/csv/, adding ~2-3 new modules for parallel processing infrastructure

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

### Principle I: Port-First (Reference Implementation)
- ✅ **PASS**: This is an optimization feature, not new architecture. KuzuDB's C++ implementation uses similar parallel CSV parsing and memory-mapped I/O.
- **Note**: Will reference C:\dev\kuzu for parallel loading patterns if needed.

### Principle II: TDD with Red-Green-Refactor
- ✅ **PASS**: Will follow TDD workflow:
  1. Write performance tests/benchmarks first (Red - failing to meet targets)
  2. Implement optimizations (Green - meet targets)
  3. Refactor for clarity
- **Existing tests**: 97 unit, 57 contract, 71 integration tests must continue passing

### Principle III: Benchmarking & Performance Tracking
- ✅ **PASS**: Existing benchmarks at `benches/csv_benchmark.rs` provide baseline:
  - Current: ~1.07M nodes/sec, ~820K edges/sec
  - Target: Maintain nodes/sec, achieve 2.5M+ edges/sec
- **Regression threshold**: >20% slower = block merge

### Principle IV: Rust Best Practices & Idioms
- ✅ **PASS**: Will use:
  - crossbeam (already a dependency) for lock-free channels
  - memmap2 (already a dependency) for memory-mapped I/O
  - parking_lot (already a dependency) for synchronization
  - Safe Rust where possible; any unsafe for mmap will have SAFETY comments

### Principle V: Safety & Correctness Over Performance (Initially)
- ✅ **PASS**: Current correct implementation exists. Optimizations will:
  1. Be implemented incrementally with tests at each step
  2. Use safe Rust except for necessary mmap unsafe blocks
  3. Maintain transaction semantics (batch commit/rollback)
  4. Preserve correct error handling and row numbering

**Gate Status**: ✅ PASS - All principles satisfied, proceed to Phase 0

## Project Structure

### Documentation (this feature)

```text
specs/003-optimize-csv-import/
├── plan.md              # This file
├── research.md          # Phase 0 output: parallel parsing research
├── data-model.md        # Phase 1 output: new/modified data structures
├── quickstart.md        # Phase 1 output: developer guide
├── contracts/           # Phase 1 output: API contracts (if any)
└── tasks.md             # Phase 2 output (/speckit.tasks)
```

### Source Code (repository root)

```text
src/
├── storage/
│   ├── csv/
│   │   ├── mod.rs           # CsvImportConfig, ImportProgress (modify for throughput)
│   │   ├── parser.rs        # CsvParser (modify for mmap support)
│   │   ├── node_loader.rs   # NodeLoader (modify for parallel parsing)
│   │   ├── rel_loader.rs    # RelLoader (modify for parallel parsing)
│   │   ├── parallel.rs      # NEW: Parallel parsing infrastructure
│   │   └── mmap_reader.rs   # NEW: Memory-mapped file reader
│   └── ...
├── ...
└── lib.rs

tests/
├── contract/
├── integration/
│   └── csv_parallel_tests.rs  # NEW: Parallel import tests
└── unit/

benches/
└── csv_benchmark.rs     # Existing benchmark (add parallel benchmarks)
```

**Structure Decision**: Single project structure. New modules (`parallel.rs`, `mmap_reader.rs`) added under existing `src/storage/csv/` directory. No new top-level directories needed.

## Complexity Tracking

> **No violations identified** - All implementations follow constitution principles.

| Aspect | Approach | Constitution Alignment |
|--------|----------|----------------------|
| Parallel parsing | crossbeam channels (existing dep) | Principle IV: Uses existing ecosystem |
| Memory mapping | memmap2 (existing dep) | Principle IV: Uses existing ecosystem |
| String interning | Optional, can use standard HashMap | Principle V: Simple first |
| Batch writes | Extend existing batch_size config | Minimal change |
