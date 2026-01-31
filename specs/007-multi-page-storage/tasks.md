# Tasks: Multi-Page Storage

**Input**: Design documents from `/specs/007-multi-page-storage/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/storage-api.md

**Tests**: Included per the constitution (TDD with Red-Green-Refactor required by Principle II).

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

---

## Phase 1: Setup

**Purpose**: No new project setup needed — this feature modifies an existing Rust crate. Phase 1 covers error type additions only.

- [X] T001 Add `PageRangeOverlap`, `PageRangeOutOfBounds`, and `MultiPageDataCorrupted` error variants in src/error.rs

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core infrastructure that MUST be complete before ANY user story can be implemented. These are the multi-page read/write primitives and PageRange helpers.

**CRITICAL**: No user story work can begin until this phase is complete.

### PageRange Helpers

- [X] T002 [P] Add `byte_capacity()` method to `PageRange` in src/storage/mod.rs
- [X] T003 [P] Add `overlaps()` method to `PageRange` in src/storage/mod.rs (per Contract 4)
- [X] T004 [P] Add `contains_page()` method to `PageRange` in src/storage/mod.rs

### Page Allocator

- [X] T005 Implement `DiskManager::allocate_page_range(num_pages: u32) -> Result<PageRange>` in src/storage/page/disk_manager.rs (per Contract 1)

### Multi-Page Read/Write Helpers

- [X] T006 Implement `calculate_pages_needed(data_len: usize) -> u32` utility function in src/lib.rs (per Contract 8)
- [X] T007 Implement `write_multi_page()` helper in src/lib.rs (per Contract 2)
- [X] T008 Implement `read_multi_page()` helper in src/lib.rs (per Contract 3)

### Header Validation

- [X] T009 Implement `DatabaseHeader::validate_ranges()` in src/storage/mod.rs (per Contract 5)

### Foundational Tests

- [X] T010 [P] Unit tests for `PageRange::byte_capacity()`, `overlaps()`, `contains_page()` in tests/unit_tests.rs
- [X] T011 [P] Unit tests for `DiskManager::allocate_page_range()` in tests/unit_tests.rs
- [X] T012 [P] Unit tests for `calculate_pages_needed()` in tests/unit_tests.rs
- [X] T013 [P] Unit tests for `write_multi_page()` and `read_multi_page()` round-trip in tests/unit_tests.rs
- [X] T014 [P] Unit tests for `DatabaseHeader::validate_ranges()` in tests/unit_tests.rs
- [X] T015 [P] Contract test for multi-page data format stability (length prefix + data across pages) in tests/contract_tests.rs

**Checkpoint**: Foundation ready — multi-page primitives are tested and available. User story implementation can now begin.

---

## Phase 3: User Story 1 — Store Node Data Beyond Single Page Limit (Priority: P1) MVP

**Goal**: Node table data can span multiple contiguous pages, removing the 4KB storage ceiling for node data.

**Independent Test**: Create a node table, insert enough rows to exceed 4KB of serialized data, close the database, reopen it, and verify all data is intact.

### Tests for User Story 1

- [X] T016 [P] [US1] Integration test: node data exceeding 4KB persists across close/reopen in tests/integration_tests.rs
- [X] T017 [P] [US1] Integration test: multiple node tables with combined data > 4KB in tests/integration_tests.rs
- [X] T018 [P] [US1] Integration test: node data grows from < 4KB to > 4KB after additional inserts in tests/integration_tests.rs
- [X] T019 [P] [US1] Contract test: node data multi-page serialization format stability in tests/contract_tests.rs

### Implementation for User Story 1

- [X] T020 [US1] Modify `save_all_data()` in src/lib.rs to use `write_multi_page()` for node table data with dynamic page allocation (per Contract 6)
- [X] T021 [US1] Modify `load_table_data()` in src/lib.rs to use `read_multi_page()` for node table data
- [X] T022 [US1] Update header `metadata_range` in `save_all_data()` to reflect dynamically allocated node data page range in src/lib.rs
- [X] T023 [US1] Verify all existing node-related tests still pass (cargo test)

**Checkpoint**: Node data multi-page storage works. Database can persist and reload > 4KB of node data.

---

## Phase 4: User Story 2 — Store Relationship Data Beyond Single Page Limit (Priority: P2)

**Goal**: Relationship table data can span multiple contiguous pages, removing the 4KB storage ceiling for relationship/edge data.

**Independent Test**: Create node and relationship tables, insert enough relationships to exceed 4KB of serialized data, close the database, reopen it, and verify all relationship data (edges, properties, bidirectional indices) is intact.

### Tests for User Story 2

- [X] T024 [P] [US2] Integration test: relationship data exceeding 4KB persists across close/reopen in tests/integration_tests.rs
- [X] T025 [P] [US2] Integration test: multiple rel tables with combined data > 4KB in tests/integration_tests.rs
- [X] T026 [P] [US2] Integration test: rel data grows beyond one page after CSV import in tests/integration_tests.rs
- [X] T027 [P] [US2] Contract test: rel data multi-page serialization format stability in tests/contract_tests.rs

### Implementation for User Story 2

- [X] T028 [US2] Modify `save_all_data()` in src/lib.rs to use `write_multi_page()` for rel table data with dynamic page allocation
- [X] T029 [US2] Modify `load_rel_table_data()` in src/lib.rs to use `read_multi_page()` for rel table data
- [X] T030 [US2] Update header `rel_metadata_range` in `save_all_data()` to reflect dynamically allocated rel data page range in src/lib.rs
- [X] T031 [US2] Remove the existing rel data size validation check (`rel_data_len > PAGE_SIZE - 4`) in src/lib.rs
- [X] T032 [US2] Verify all existing relationship-related tests still pass (cargo test)

**Checkpoint**: Relationship data multi-page storage works. Database can persist and reload > 4KB of rel data.

---

## Phase 5: User Story 3 — Store Catalog Data Beyond Single Page Limit (Priority: P3)

**Goal**: Catalog (schema definitions) can span multiple pages, removing the schema storage limit.

**Independent Test**: Create enough tables with sufficient columns and property definitions to exceed 4KB of catalog data, close the database, reopen it, and verify all schemas are intact.

### Tests for User Story 3

- [X] T033 [P] [US3] Integration test: catalog data exceeding 4KB persists across close/reopen in tests/integration_tests.rs
- [X] T034 [P] [US3] Integration test: catalog grows beyond 4KB after new table creation in tests/integration_tests.rs
- [X] T035 [P] [US3] Contract test: catalog multi-page serialization format stability in tests/contract_tests.rs

### Implementation for User Story 3

- [X] T036 [US3] Modify `save_all_data()` in src/lib.rs to use `write_multi_page()` for catalog data with dynamic page allocation
- [X] T037 [US3] Modify catalog loading in src/lib.rs to use `read_multi_page()` for catalog data
- [X] T038 [US3] Update header `catalog_range` in `save_all_data()` to reflect dynamically allocated catalog page range in src/lib.rs
- [X] T039 [US3] Verify all existing catalog-related tests still pass (cargo test)

**Checkpoint**: All three metadata types (node, rel, catalog) now support multi-page storage.

---

## Phase 6: User Story 4 — Backward Compatibility with Existing Databases (Priority: P1)

**Goal**: Existing v2 databases open and operate correctly after the upgrade. No manual migration required.

**Independent Test**: Open a database created with the current format (version 2), verify all data loads correctly, then save and reopen to confirm the database now uses the new multi-page format (version 3).

### Tests for User Story 4

- [X] T040 [P] [US4] Integration test: v2 database with node and rel data opens correctly in updated system in tests/integration_tests.rs
- [X] T041 [P] [US4] Integration test: v2 database is re-saved as v3 format after first checkpoint in tests/integration_tests.rs
- [X] T042 [P] [US4] Integration test: v2 database with WAL replays correctly in updated system in tests/integration_tests.rs
- [X] T043 [P] [US4] Contract test: v2 header binary format is still correctly parseable in tests/contract_tests.rs

### Implementation for User Story 4

- [X] T044 [US4] Bump `CURRENT_VERSION` from 2 to 3 in src/storage/mod.rs
- [X] T045 [US4] Add v2-to-v3 migration path in `DatabaseHeader::deserialize_with_migration_flag()` in src/storage/mod.rs
- [X] T046 [US4] Ensure `Database::open()` sets dirty flag when v2 database is loaded so it re-saves as v3 on close in src/lib.rs (per Contract 7)
- [X] T047 [US4] Verify all existing tests still pass with version 3 header (cargo test)

**Checkpoint**: v2 databases seamlessly migrate to v3. No data loss, no manual steps.

---

## Phase 7: User Story 5 — Crash Recovery with Multi-Page Data (Priority: P2)

**Goal**: WAL replay works correctly when underlying data spans multiple pages. Committed data is recoverable; uncommitted data is rolled back.

**Independent Test**: Insert multi-page quantities of data, simulate a crash before checkpoint, reopen the database, and verify WAL replay restores committed data.

### Tests for User Story 5

- [X] T048 [P] [US5] Integration test: WAL replay restores committed multi-page node data after crash in tests/integration_tests.rs
- [X] T049 [P] [US5] Integration test: WAL replay does NOT restore uncommitted multi-page data in tests/integration_tests.rs
- [X] T050 [P] [US5] Integration test: WAL replay restores committed multi-page rel data after crash in tests/integration_tests.rs

### Implementation for User Story 5

- [X] T051 [US5] Verify `save_all_data()` multi-page writes occur before header update so crash safety is maintained in src/lib.rs
- [X] T052 [US5] Verify WAL checkpoint record is written after header flush to ensure atomicity in src/lib.rs
- [X] T053 [US5] Add page range bounds validation on load (`end_page <= file_page_count`) in src/lib.rs

**Checkpoint**: Crash recovery works with multi-page data. Committed transactions are always recoverable.

---

## Phase 8: Polish & Cross-Cutting Concerns

**Purpose**: Improvements that affect multiple user stories, edge cases, and final validation.

- [X] T054 [P] Handle page-boundary edge cases: data exactly at 4KB, 8KB, etc. in tests/unit_tests.rs
- [X] T055 [P] Handle empty data case (0 bytes serialized) in write_multi_page/read_multi_page in src/lib.rs
- [X] T056 Run full test suite: `cargo test` — all 440+ existing tests must pass
- [X] T057 Run `cargo clippy` — zero warnings
- [X] T058 Run existing benchmarks (`cargo bench --bench storage_benchmark`, `cargo bench --bench rel_persist_benchmark`) — verify no > 2x regression
- [X] T059 Run quickstart.md verification checklist

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — can start immediately
- **Foundational (Phase 2)**: Depends on Phase 1 (error types) — BLOCKS all user stories
- **User Story 1 (Phase 3)**: Depends on Phase 2 completion
- **User Story 2 (Phase 4)**: Depends on Phase 2 completion (independent of US1)
- **User Story 3 (Phase 5)**: Depends on Phase 2 completion (independent of US1, US2)
- **User Story 4 (Phase 6)**: Depends on Phase 3 (node multi-page must work before version bump makes sense)
- **User Story 5 (Phase 7)**: Depends on Phase 3 and Phase 4 (crash recovery tests need multi-page data to exist)
- **Polish (Phase 8)**: Depends on all user stories being complete

### User Story Dependencies

- **US1 (P1)**: After Foundational — no dependencies on other stories
- **US2 (P2)**: After Foundational — no dependencies on other stories (can parallel with US1)
- **US3 (P3)**: After Foundational — no dependencies on other stories (can parallel with US1, US2)
- **US4 (P1)**: After US1+US2+US3 — version bump should happen after all save_all_data changes are in place
- **US5 (P2)**: After US1+US2 — crash recovery tests need multi-page node and rel data

### Within Each User Story

- Tests MUST be written and FAIL before implementation
- Implementation follows the save → load → header update sequence
- Story complete → run `cargo test` to verify no regressions

### Parallel Opportunities

- T002, T003, T004: All PageRange helper methods (different methods, same file but independent)
- T010–T015: All foundational tests (different test functions)
- T016–T019: All US1 tests (different test functions)
- T024–T027: All US2 tests
- T033–T035: All US3 tests
- T040–T043: All US4 tests
- T048–T050: All US5 tests
- US1, US2, US3 can proceed in parallel after Phase 2 (different metadata types)

---

## Parallel Example: User Story 1

```bash
# Launch all tests for US1 together (write first, expect failures):
Task: "Integration test: node data > 4KB persists" in tests/integration_tests.rs
Task: "Contract test: node data multi-page format" in tests/contract_tests.rs

# Then implement sequentially:
Task: "Modify save_all_data() for node data" in src/lib.rs
Task: "Modify load_table_data() for node data" in src/lib.rs
Task: "Update header metadata_range" in src/lib.rs
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup (error variants)
2. Complete Phase 2: Foundational (PageRange helpers, allocator, read/write, validation)
3. Complete Phase 3: User Story 1 (node multi-page)
4. **STOP and VALIDATE**: Test US1 independently — insert > 4KB node data, close, reopen, verify
5. All existing tests still pass

### Incremental Delivery

1. Setup + Foundational -> Foundation ready
2. Add US1 (node multi-page) -> Test independently -> MVP!
3. Add US2 (rel multi-page) -> Test independently
4. Add US3 (catalog multi-page) -> Test independently -> All metadata types done
5. Add US4 (v2->v3 migration) -> Test with old databases
6. Add US5 (crash recovery) -> Test with simulated crashes
7. Polish -> Full regression, clippy, benchmarks

---

## Notes

- [P] tasks = different files or independent functions, no dependencies
- [Story] label maps task to specific user story for traceability
- Each user story is independently completable and testable
- `save_all_data()` changes from `&self` to `&mut self` (updates header ranges)
- The sequential allocator always allocates catalog first, then node data, then rel data (deterministic order)
- Old pages are not reclaimed — file grows monotonically (free-space management is out of scope)
