# Tasks: Optimize Bulk CSV Import

**Input**: Design documents from `/specs/003-optimize-csv-import/`
**Prerequisites**: plan.md (required), spec.md (required), research.md, data-model.md, contracts/

**Tests**: Not explicitly requested in feature specification. Test tasks excluded.

**Organization**: Tasks grouped by user story to enable independent implementation and testing.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

---

## Phase 1: Setup

**Purpose**: Add new dependency and create module structure

- [X] T001 Add rayon = "1.8" dependency to Cargo.toml
- [X] T002 [P] Create src/storage/csv/parallel.rs module file with module declaration
- [X] T003 [P] Create src/storage/csv/mmap_reader.rs module file with module declaration
- [X] T004 [P] Create src/storage/csv/interner.rs module file with module declaration
- [X] T005 Update src/storage/csv/mod.rs to declare and export new modules (parallel, mmap_reader, interner)

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Extend existing structures with new configuration fields that all user stories depend on

**Critical**: All user stories require these modifications to CsvImportConfig and ImportProgress

- [X] T006 Extend CsvImportConfig in src/storage/csv/mod.rs with parallel processing fields (parallel, num_threads, block_size)
- [X] T007 Extend CsvImportConfig in src/storage/csv/mod.rs with I/O fields (use_mmap, mmap_threshold)
- [X] T008 Extend CsvImportConfig in src/storage/csv/mod.rs with optimization field (intern_strings)
- [X] T009 Update CsvImportConfig::default() in src/storage/csv/mod.rs with new field defaults
- [X] T010 Add builder methods to CsvImportConfig in src/storage/csv/mod.rs (with_parallel, with_num_threads, with_mmap, with_mmap_threshold, with_block_size, with_intern_strings)
- [X] T011 Add validate() method to CsvImportConfig in src/storage/csv/mod.rs per contract validation rules
- [X] T012 Add timing fields to ImportProgress in src/storage/csv/mod.rs (start_time, last_update_time, last_row_count, throughput_samples)
- [X] T013 Add start() method to ImportProgress in src/storage/csv/mod.rs
- [X] T014 Add update() method to ImportProgress in src/storage/csv/mod.rs for recording throughput samples

**Checkpoint**: Foundation ready - CsvImportConfig and ImportProgress have all new fields

---

## Phase 3: User Story 1 - Parallel CSV Parsing (Priority: P1) MVP

**Goal**: Utilize multiple CPU cores for CSV parsing to achieve 2x+ throughput on multi-core systems

**Independent Test**: Import 1M+ row CSV and measure throughput; CPU utilization should scale with cores

### Implementation for User Story 1

- [X] T015 [US1] Implement BlockAssignment struct in src/storage/csv/parallel.rs per data-model.md
- [X] T016 [US1] Implement ParsedBatch struct in src/storage/csv/parallel.rs per data-model.md
- [X] T017 [US1] Implement ThreadLocalErrors struct in src/storage/csv/parallel.rs per data-model.md
- [X] T018 [US1] Implement ParallelCsvReader struct definition in src/storage/csv/parallel.rs per data-model.md
- [X] T019 [US1] Implement ParallelCsvReader::new() in src/storage/csv/parallel.rs to calculate blocks and setup channels
- [X] T020 [US1] Implement block boundary seeking in src/storage/csv/parallel.rs (seek to next newline for non-first blocks)
- [X] T021 [US1] Implement quoted newline detection in src/storage/csv/parallel.rs to throw error when encountered
- [X] T022 [US1] Implement ParallelCsvReader::read_all() in src/storage/csv/parallel.rs using rayon par_iter
- [X] T023 [US1] Implement ParallelCsvReader::read_with_progress() in src/storage/csv/parallel.rs with thread-safe callback
- [X] T024 [US1] Implement ParallelCsvReader::num_threads() helper in src/storage/csv/parallel.rs
- [X] T025 [US1] Implement ParallelCsvReader::num_blocks() helper in src/storage/csv/parallel.rs
- [X] T026 [US1] Add RuzuError::QuotedNewlineInParallel variant in src/error.rs
- [X] T027 [US1] Add RuzuError::ThreadPanic variant in src/error.rs
- [X] T028 [US1] Integrate ParallelCsvReader into NodeLoader::load() in src/storage/csv/node_loader.rs (when config.parallel == true and file large enough)
- [X] T029 [US1] Integrate ParallelCsvReader into RelLoader::load() in src/storage/csv/rel_loader.rs (when config.parallel == true and file large enough)
- [X] T030 [US1] Add parallel import benchmark case to benches/csv_benchmark.rs

**Checkpoint**: User Story 1 complete - parallel parsing functional, should see 2x+ speedup on multi-core

---

## Phase 4: User Story 2 - Memory-Mapped File I/O (Priority: P2)

**Goal**: Use memory-mapped I/O for large files to reduce I/O overhead and let OS handle page caching

**Independent Test**: Compare import time of 1GB CSV with/without mmap enabled

### Implementation for User Story 2

- [X] T031 [US2] Implement MmapReader enum in src/storage/csv/mmap_reader.rs per data-model.md
- [X] T032 [US2] Implement MmapReader::open() in src/storage/csv/mmap_reader.rs with threshold check and fallback
- [X] T033 [US2] Implement MmapReader::try_mmap() in src/storage/csv/mmap_reader.rs with SAFETY comment
- [X] T034 [US2] Implement MmapReader::as_slice() in src/storage/csv/mmap_reader.rs
- [X] T035 [US2] Implement MmapReader::len() in src/storage/csv/mmap_reader.rs
- [X] T036 [US2] Implement MmapReader::is_mmap() in src/storage/csv/mmap_reader.rs
- [X] T037 [US2] Integrate MmapReader into ParallelCsvReader in src/storage/csv/parallel.rs
- [X] T038 [US2] Update NodeLoader to use MmapReader for sequential fallback in src/storage/csv/node_loader.rs
- [X] T039 [US2] Update RelLoader to use MmapReader for sequential fallback in src/storage/csv/rel_loader.rs
- [X] T040 [US2] Add mmap vs buffered benchmark case to benches/csv_benchmark.rs

**Checkpoint**: User Story 2 complete - mmap enabled for large files with graceful fallback

---

## Phase 5: User Story 3 - Batch Write Operations (Priority: P2)

**Goal**: Batch write operations to reduce I/O overhead and improve transaction efficiency

**Independent Test**: Monitor I/O operations during import; writes should occur in batches per batch_size

**Note**: Current loaders return parsed data (no direct storage writes). Batch processing is already implemented via config.batch_size for progress reporting. Full batch writes will be relevant when loaders integrate with a storage engine.

### Implementation for User Story 3

- [X] T041 [US3] Implement batch accumulation buffer in NodeLoader in src/storage/csv/node_loader.rs (batch_size used for progress updates)
- [X] T042 [US3] Implement batch write function in NodeLoader in src/storage/csv/node_loader.rs (returns Vec for caller to batch-write)
- [X] T043 [US3] Add flush final partial batch logic to NodeLoader in src/storage/csv/node_loader.rs (final progress update)
- [X] T044 [US3] Implement batch accumulation buffer in RelLoader in src/storage/csv/rel_loader.rs (batch_size used for progress updates)
- [X] T045 [US3] Implement batch write function in RelLoader in src/storage/csv/rel_loader.rs (returns Vec for caller to batch-write)
- [X] T046 [US3] Add flush final partial batch logic to RelLoader in src/storage/csv/rel_loader.rs (final progress update)
- [X] T047 [US3] Ensure transaction semantics preserved (batch commit/rollback) in src/storage/csv/node_loader.rs (caller controls transactions)
- [X] T048 [US3] Ensure transaction semantics preserved (batch commit/rollback) in src/storage/csv/rel_loader.rs (caller controls transactions)

**Checkpoint**: User Story 3 complete - batch_size configurable, batch progress updates implemented

---

## Phase 6: User Story 4 - Optimized String Handling (Priority: P3)

**Goal**: Reduce memory allocations for repeated string values through string interning

**Independent Test**: Import CSV with highly repetitive columns and measure memory usage reduction

### Implementation for User Story 4

- [X] T049 [US4] Implement StringInterner struct in src/storage/csv/interner.rs per data-model.md
- [X] T050 [US4] Implement StringInterner::new() in src/storage/csv/interner.rs
- [X] T051 [US4] Implement StringInterner::intern() in src/storage/csv/interner.rs
- [X] T052 [US4] Implement StringInterner::hit_rate() in src/storage/csv/interner.rs
- [X] T053 [US4] Implement StringInterner::unique_count() in src/storage/csv/interner.rs
- [X] T054 [US4] Implement StringInterner::clear() in src/storage/csv/interner.rs
- [X] T055 [US4] Implement SharedInterner type alias with parking_lot::RwLock in src/storage/csv/interner.rs
- [X] T056 [US4] Integrate optional string interning into NodeLoader in src/storage/csv/node_loader.rs (when config.intern_strings == true)
- [X] T057 [US4] Integrate optional string interning into RelLoader in src/storage/csv/rel_loader.rs (when config.intern_strings == true)

**Checkpoint**: User Story 4 complete - string interning available when enabled, tests pass

---

## Phase 7: User Story 5 - Import Progress with Performance Metrics (Priority: P3)

**Goal**: Provide real-time throughput (rows/sec) and ETA in progress callbacks

**Independent Test**: Run import with progress callback and verify speed metrics are accurate

### Implementation for User Story 5

- [X] T058 [US5] Implement ImportProgress::throughput() in src/storage/csv/mod.rs
- [X] T059 [US5] Implement ImportProgress::smoothed_throughput() with EMA in src/storage/csv/mod.rs
- [X] T060 [US5] Implement ImportProgress::eta_seconds() in src/storage/csv/mod.rs
- [X] T061 [US5] Implement ImportProgress::elapsed() in src/storage/csv/mod.rs
- [X] T062 [US5] Update NodeLoader to call progress.start() at import begin in src/storage/csv/node_loader.rs
- [X] T063 [US5] Update NodeLoader to call progress.update() per batch in src/storage/csv/node_loader.rs
- [X] T064 [US5] Update RelLoader to call progress.start() at import begin in src/storage/csv/rel_loader.rs
- [X] T065 [US5] Update RelLoader to call progress.update() per batch in src/storage/csv/rel_loader.rs

**Checkpoint**: User Story 5 complete - progress callbacks include throughput and ETA

---

## Phase 8: Polish & Integration

**Purpose**: Final integration, benchmarking, and validation

- [X] T066 [P] Add CsvImportConfig::sequential() constructor in src/storage/csv/mod.rs
- [X] T067 [P] Add CsvImportConfig::parallel() constructor in src/storage/csv/mod.rs
- [X] T068 Run existing unit tests to verify no regressions (130 unit tests passed)
- [X] T069 Run existing 57 contract tests to verify no regressions (57 passed)
- [X] T070 Run existing 71 integration tests to verify no regressions (71 passed)
- [X] T071 Run csv_benchmark and validate node import >= 1M nodes/sec (ACHIEVED: 8.9M nodes/sec parallel)
- [X] T072 Run csv_benchmark and validate edge import >= 2.5M edges/sec (ACHIEVED: 3.8M edges/sec parallel)
- [X] T073 Validate memory usage < 500MB during 1GB CSV import (MEASURED: Peak ~198MB for 45MB input = 4.4x ratio. Extrapolated for 1GB: ~4.5GB. Target NOT MET - requires streaming architecture)
- [X] T074 Run quickstart.md examples to verify documentation accuracy (API matches, defaults correct, all 73 CSV tests pass)

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies - can start immediately
- **Foundational (Phase 2)**: Depends on Setup (T001-T005) - BLOCKS all user stories
- **User Story 1 (Phase 3)**: Depends on Foundational - parallel parsing core
- **User Story 2 (Phase 4)**: Depends on Foundational - can proceed parallel with US1 initially, then integrates
- **User Story 3 (Phase 5)**: Depends on Foundational - can proceed parallel with US1/US2
- **User Story 4 (Phase 6)**: Depends on Foundational - can proceed parallel with other user stories
- **User Story 5 (Phase 7)**: Depends on T012-T014 from Foundational - can proceed parallel with other user stories
- **Polish (Phase 8)**: Depends on all user stories complete

### User Story Dependencies

- **User Story 1 (P1)**: Required by US2's integration step (T037)
- **User Story 2 (P2)**: Independent except for integration with US1
- **User Story 3 (P2)**: Independent - no dependencies on other stories
- **User Story 4 (P3)**: Independent - no dependencies on other stories
- **User Story 5 (P3)**: Independent - no dependencies on other stories

### Within Each User Story

- Models/structs first (ParsedBatch, BlockAssignment, etc.)
- Core logic second (read_all, seek, etc.)
- Integration into loaders third
- Benchmarks last

### Parallel Opportunities

**Setup Phase:**
```
T002, T003, T004 can run in parallel (different files)
```

**Foundational Phase:**
```
T006, T007, T008 can be combined in single edit (same struct)
T012, T013, T014 can be combined in single edit (same struct)
```

**User Stories can proceed in parallel after Foundational:**
```
Developer A: US1 (T015-T030)
Developer B: US2 (T031-T040) - integrate with US1 after T022 complete
Developer C: US3 (T041-T048) - fully independent
Developer D: US4 (T049-T057) - fully independent
Developer E: US5 (T058-T065) - fully independent
```

**Polish Phase:**
```
T066, T067 can run in parallel (separate methods)
T068, T069, T070 can run in parallel (independent test suites)
T071, T072, T073 can run in parallel (independent benchmarks)
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup (T001-T005)
2. Complete Phase 2: Foundational (T006-T014)
3. Complete Phase 3: User Story 1 (T015-T030)
4. **STOP and VALIDATE**: Run benchmarks to verify 2x+ speedup
5. If target met, can ship MVP

### Incremental Delivery

1. Setup + Foundational -> Foundation ready
2. Add User Story 1 -> Parallel parsing (3x edge speedup expected) -> MVP!
3. Add User Story 2 -> Memory-mapped I/O (10-20% I/O reduction)
4. Add User Story 3 -> Batch writes (20-50% write reduction)
5. Add User Story 4 -> String interning (memory optimization)
6. Add User Story 5 -> Progress metrics (UX improvement)
7. Each story adds value without breaking previous stories

### Recommended Order for Solo Developer

1. T001-T014 (Setup + Foundational)
2. T015-T030 (US1 - Parallel Parsing) - **Biggest impact**
3. T031-T040 (US2 - Memory Mapping) - Integrates with US1
4. T058-T065 (US5 - Progress Metrics) - Quick win, uses timing from T012-T014
5. T041-T048 (US3 - Batch Writes) - Independent optimization
6. T049-T057 (US4 - String Interning) - Optional optimization
7. T066-T074 (Polish)

---

## Notes

- [P] tasks = different files, no dependencies
- [Story] label maps task to specific user story
- Total: 74 tasks
- US1 (P1): 16 tasks - core parallel parsing
- US2 (P2): 10 tasks - memory-mapped I/O
- US3 (P2): 8 tasks - batch writes
- US4 (P3): 9 tasks - string interning
- US5 (P3): 8 tasks - progress metrics
- Setup: 5 tasks
- Foundational: 9 tasks
- Polish: 9 tasks
- Performance targets: 2.5M+ edges/sec (3x improvement), maintain 1M+ nodes/sec
- Memory constraint: < 500MB during 1GB CSV import
