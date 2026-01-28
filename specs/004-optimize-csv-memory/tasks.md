# Tasks: Optimize Peak Memory During CSV Import

**Input**: Design documents from `/specs/004-optimize-csv-memory/`
**Prerequisites**: plan.md (required), spec.md (required), research.md, data-model.md, contracts/

**Tests**: Included per constitution requirement (TDD with Red-Green-Refactor)

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3, US4)
- Include exact file paths in descriptions

## Path Conventions

- **Single project**: `src/`, `tests/` at repository root (Rust crate structure)
- Paths shown below match the existing ruzu codebase layout

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Project initialization and basic structure for streaming imports

- [X] T001 Add streaming module declaration in src/storage/csv/mod.rs
- [X] T002 [P] Create StreamingConfig struct with defaults in src/storage/csv/mod.rs
- [X] T003 [P] Create StreamingError enum in src/storage/csv/mod.rs
- [X] T004 [P] Add dhat-heap feature flag in Cargo.toml for memory profiling

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core infrastructure that MUST be complete before ANY user story can be implemented

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

### Tests for Foundational Phase

> **NOTE: Write these tests FIRST, ensure they FAIL before implementation**

- [X] T005 [P] Unit test for RowBuffer::new() and capacity in tests/unit/buffer_tests.rs
- [X] T006 [P] Unit test for RowBuffer::push() and is_full() in tests/unit/buffer_tests.rs
- [X] T007 [P] Unit test for RowBuffer::clear() preserves capacity in tests/unit/buffer_tests.rs
- [X] T008 [P] Unit test for RowBuffer::take() and recycling in tests/unit/buffer_tests.rs

### Implementation for Foundational Phase

- [X] T009 Create RowBuffer struct in src/storage/csv/buffer.rs
- [X] T010 Implement RowBuffer::new() with pre-allocation in src/storage/csv/buffer.rs
- [X] T011 Implement RowBuffer::push() with BufferFull error in src/storage/csv/buffer.rs
- [X] T012 Implement RowBuffer::clear() without deallocation in src/storage/csv/buffer.rs
- [X] T013 Implement RowBuffer::take() returning rows and resetting in src/storage/csv/buffer.rs
- [X] T014 Implement RowBuffer::len() and is_full() in src/storage/csv/buffer.rs
- [X] T015 Export buffer module from src/storage/csv/mod.rs

**Checkpoint**: RowBuffer infrastructure ready - user story implementation can now begin

---

## Phase 3: User Story 1 - Import Large CSV Within Memory Budget (Priority: P1) 🎯 MVP

**Goal**: Enable CSV imports of 1GB+ files with peak memory <500MB

**Independent Test**: Import a 1GB CSV file and verify peak memory stays under 500MB using DHAT profiler

### Tests for User Story 1

> **NOTE: Write these tests FIRST, ensure they FAIL before implementation**

- [X] T016 [P] [US1] Unit test for NodeTable::insert_batch() empty input in tests/unit/table_batch_tests.rs
- [X] T017 [P] [US1] Unit test for NodeTable::insert_batch() valid rows in tests/unit/table_batch_tests.rs
- [X] T018 [P] [US1] Unit test for NodeTable::insert_batch() schema mismatch in tests/unit/table_batch_tests.rs
- [X] T019 [P] [US1] Unit test for RelTable::insert_batch() valid relationships in tests/unit/rel_table_batch_tests.rs
- [X] T020 [P] [US1] Integration test for NodeLoader::load_streaming() in tests/integration/streaming_import_tests.rs
- [X] T021 [P] [US1] Integration test for RelLoader::load_streaming() in tests/integration/streaming_import_tests.rs

### Implementation for User Story 1

- [X] T022 [US1] Implement NodeTable::insert_batch() with single validation pass in src/storage/table.rs
- [X] T023 [US1] Implement RelTable::insert_batch() for relationship batches in src/storage/rel_table.rs
- [X] T024 [US1] Implement NodeLoader::load_streaming() sequential mode in src/storage/csv/node_loader.rs
- [X] T025 [US1] Implement RelLoader::load_streaming() sequential mode in src/storage/csv/rel_loader.rs
- [X] T026 [US1] Wire streaming import into Database::import_nodes() in src/lib.rs
- [X] T027 [US1] Wire streaming import into Database::import_relationships() in src/lib.rs
- [X] T028 [US1] Auto-enable streaming for files > 100MB based on StreamingConfig threshold in src/lib.rs

**Checkpoint**: User Story 1 complete - streaming imports work with bounded memory

---

## Phase 4: User Story 2 - Maintain Import Throughput (Priority: P1)

**Goal**: Ensure streaming writes don't degrade throughput below 80% of baseline (≥7M nodes/sec, ≥3M edges/sec)

**Independent Test**: Run csv_benchmark and verify throughput meets targets

### Tests for User Story 2

> **NOTE: Write these tests FIRST, ensure they FAIL before implementation**

- [X] T029 [P] [US2] Benchmark test for streaming node import throughput in benches/csv_benchmark.rs
- [X] T030 [P] [US2] Benchmark test for streaming edge import throughput in benches/csv_benchmark.rs

### Implementation for User Story 2

- [X] T031 [US2] Pre-allocate column storage in NodeTable::insert_batch() for throughput in src/storage/table.rs
- [X] T032 [US2] Pre-allocate CSR storage in RelTable::insert_batch() for throughput in src/storage/rel_table.rs
- [X] T033 [US2] Optimize RowBuffer recycling to reuse inner Vec allocations in src/storage/csv/buffer.rs
- [X] T034 [US2] Add batch_size configuration to CsvImportConfig in src/storage/csv/mod.rs
- [X] T035 [US2] Tune default batch_size (100K) for optimal memory/throughput balance in src/storage/csv/mod.rs

**Checkpoint**: User Story 2 complete - throughput meets ≥7M nodes/sec, ≥3M edges/sec targets

---

## Phase 5: User Story 3 - Predictable Memory Regardless of File Size (Priority: P2)

**Goal**: Memory usage variance <100MB across file sizes from 100MB to 5GB

**Independent Test**: Import files of varying sizes and verify peak memory stays within 100MB band

### Tests for User Story 3

> **NOTE: Write these tests FIRST, ensure they FAIL before implementation**

- [X] T036 [P] [US3] Memory contract test MC-001 (1GB nodes <500MB) in tests/contract_tests.rs
- [X] T037 [P] [US3] Memory contract test MC-002 (1GB edges <500MB) in tests/contract_tests.rs
- [X] T038 [P] [US3] Memory contract test MC-003 (5GB <500MB) in tests/contract_tests.rs
- [X] T039 [P] [US3] Memory variance test MC-004 (<100MB diff) in tests/contract_tests.rs

### Implementation for User Story 3

- [X] T040 [US3] Add memory profiling benchmark with DHAT in benches/memory_benchmark.rs
- [X] T041 [US3] Ensure parallel block processing releases memory per-block in src/storage/csv/parallel.rs
- [X] T042 [US3] Add streaming mode to parallel CSV processing in src/storage/csv/parallel.rs
- [X] T043 [US3] Verify buffer recycling prevents memory growth across batches in src/storage/csv/buffer.rs

**Checkpoint**: User Story 3 complete - memory usage predictable regardless of file size

---

## Phase 6: User Story 4 - Progress Visibility During Streaming (Priority: P3)

**Goal**: Progress callbacks at least every 100,000 rows during streaming import

**Independent Test**: Start large import and verify progress updates displayed at regular intervals

### Tests for User Story 4

> **NOTE: Write these tests FIRST, ensure they FAIL before implementation**

- [X] T044 [P] [US4] Unit test for progress callback frequency in tests/unit_tests.rs
- [X] T045 [P] [US4] Integration test for progress during streaming import in tests/unit_tests.rs

### Implementation for User Story 4

- [X] T046 [US4] Add progress callback invocation at batch boundaries in NodeLoader::load_streaming() in src/storage/csv/node_loader.rs
- [X] T047 [US4] Add progress callback invocation at batch boundaries in RelLoader::load_streaming() in src/storage/csv/rel_loader.rs
- [X] T048 [US4] Ensure progress row counts are monotonically increasing in src/storage/csv/node_loader.rs
- [X] T049 [US4] Update ImportProgress to include batch_count field in src/storage/csv/mod.rs

**Checkpoint**: User Story 4 complete - progress visibility works during streaming imports

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: Improvements that affect multiple user stories

- [X] T050 [P] Run cargo clippy and fix all warnings
- [X] T051 [P] Run cargo fmt to ensure consistent formatting
- [X] T052 [P] Update quickstart.md with streaming usage examples in specs/004-optimize-csv-memory/quickstart.md
- [X] T053 Verify all existing CSV import tests pass (backward compatibility SC-006)
- [X] T054 Add doc comments to all new public APIs in src/storage/csv/
- [X] T055 Run full test suite: cargo test --all-features
- [X] T056 Run benchmarks and document results: cargo bench --bench csv_benchmark

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies - can start immediately
- **Foundational (Phase 2)**: Depends on Setup completion - BLOCKS all user stories
- **User Story 1 (Phase 3)**: Depends on Foundational - core streaming implementation
- **User Story 2 (Phase 4)**: Can start after Foundational, optimizes US1 implementation
- **User Story 3 (Phase 5)**: Can start after Foundational, validates memory contracts
- **User Story 4 (Phase 6)**: Can start after Foundational, adds progress tracking
- **Polish (Phase 7)**: Depends on all desired user stories being complete

### User Story Dependencies

| Story | Depends On | Can Parallel With |
|-------|------------|-------------------|
| US1 (P1) | Foundational only | US3, US4 |
| US2 (P1) | Foundational only | US3, US4 |
| US3 (P2) | Foundational only | US1, US2, US4 |
| US4 (P3) | Foundational only | US1, US2, US3 |

**Note**: US1 and US2 are both P1 and closely related - recommend implementing sequentially as US2 optimizes US1.

### Within Each User Story

1. Tests MUST be written and FAIL before implementation (TDD requirement)
2. Batch insert APIs before streaming loaders
3. Core implementation before integration
4. Verify tests pass (GREEN) before moving to next story

### Parallel Opportunities

- All Setup tasks marked [P] can run in parallel
- All Foundational tests marked [P] can run in parallel
- Tests within a user story marked [P] can run in parallel
- User Stories US3 and US4 can run in parallel with US1/US2 if staffed

---

## Parallel Example: User Story 1

```bash
# Launch all tests for User Story 1 together:
Task: "T016 Unit test for NodeTable::insert_batch() empty input"
Task: "T017 Unit test for NodeTable::insert_batch() valid rows"
Task: "T018 Unit test for NodeTable::insert_batch() schema mismatch"
Task: "T019 Unit test for RelTable::insert_batch() valid relationships"
Task: "T020 Integration test for NodeLoader::load_streaming()"
Task: "T021 Integration test for RelLoader::load_streaming()"

# After tests written (RED), implement in sequence:
Task: "T022 Implement NodeTable::insert_batch()"
Task: "T023 Implement RelTable::insert_batch()"
Task: "T024 Implement NodeLoader::load_streaming()"
# ... etc
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup (T001-T004)
2. Complete Phase 2: Foundational (T005-T015)
3. Complete Phase 3: User Story 1 (T016-T028)
4. **STOP and VALIDATE**: Run memory profiler, verify <500MB for 1GB import
5. Deploy/demo if ready - streaming imports functional

### Incremental Delivery

1. Setup + Foundational → RowBuffer infrastructure ready
2. User Story 1 → Memory-bounded streaming ✓ (MVP!)
3. User Story 2 → Throughput optimized ✓
4. User Story 3 → Memory contracts validated ✓
5. User Story 4 → Progress visibility ✓
6. Polish → Production ready ✓

### TDD Workflow (Per Task)

```bash
# RED: Write test, verify FAILS
cargo test test_row_buffer_new -- --nocapture
# Expected: FAIL (RowBuffer not implemented)

# GREEN: Implement minimal code
# Edit src/storage/csv/buffer.rs

cargo test test_row_buffer_new -- --nocapture
# Expected: PASS

# REFACTOR: Clean up
cargo clippy
cargo fmt
cargo test test_row_buffer_new -- --nocapture
# Expected: PASS (still)
```

---

## Task Summary

| Phase | Tasks | Parallel Tasks |
|-------|-------|----------------|
| Phase 1: Setup | 4 | 3 |
| Phase 2: Foundational | 11 | 4 |
| Phase 3: US1 (P1) | 13 | 6 |
| Phase 4: US2 (P1) | 7 | 2 |
| Phase 5: US3 (P2) | 8 | 4 |
| Phase 6: US4 (P3) | 6 | 2 |
| Phase 7: Polish | 7 | 3 |
| **Total** | **56** | **24** |

---

## Notes

- [P] tasks = different files, no dependencies
- [Story] label maps task to specific user story for traceability
- Each user story should be independently completable and testable
- TDD is NON-NEGOTIABLE per constitution - tests first, verify fail, then implement
- Commit after each task or logical group
- Stop at any checkpoint to validate story independently
- Success criteria from spec.md:
  - SC-001: 1GB import <500MB memory
  - SC-002: 5GB import <500MB memory
  - SC-003: ≥7M nodes/sec throughput
  - SC-004: ≥3M edges/sec throughput
  - SC-005: <100MB memory variance across file sizes
  - SC-006: All existing tests pass (backward compatibility)
