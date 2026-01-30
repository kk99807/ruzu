# Tasks: Fix Relationship Table Persistence

**Input**: Design documents from `/specs/001-fix-rel-persistence/`
**Prerequisites**: plan.md (required), spec.md (required for user stories), research.md, data-model.md, contracts/

**Tests**: This feature follows TDD principles - tests are included and MUST be written FIRST to ensure they FAIL before implementation.

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

## Path Conventions

Single project structure at repository root:
- `src/` - Source code
- `tests/` - Test files (contract, integration, unit)

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Verify environment and prepare for implementation

- [X] T001 Verify all design documents are complete in specs/001-fix-rel-persistence/
- [X] T002 [P] Run cargo test to establish baseline (all existing tests must pass)
- [X] T003 [P] Run cargo clippy to verify no existing warnings

**Checkpoint**: Environment verified - ready for foundational work

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core infrastructure changes that MUST be complete before ANY user story can be implemented

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

- [X] T004 Add RelTableLoadError and RelTableCorrupted error variants to src/error.rs
- [X] T005 [P] Add rel_metadata_range: PageRange field to DatabaseHeader in src/storage/mod.rs
- [X] T006 [P] Bump DatabaseHeader version from 1 to 2 in src/storage/mod.rs
- [X] T007 [P] Implement DatabaseHeader::from_v1() migration function in src/storage/mod.rs
- [X] T008 Create load_rel_table_data() function signature in src/lib.rs (stub implementation returning empty HashMap)
- [X] T009 Modify Database::open() in src/lib.rs to initialize rel_metadata_range to PageRange::new(3, 1) for new databases
- [X] T010 Add rel_tables parameter to replay_wal() function signature in src/lib.rs

**Checkpoint**: Foundation ready - user story implementation can now begin in parallel

---

## Phase 3: User Story 1 - Relationship Data Survives Database Restart (Priority: P1) 🎯 MVP

**Goal**: Fix the core bug where relationship table data is lost after database close/reopen operations. Delivers a working persistent relationship storage system.

**Independent Test**: Create database with nodes and relationships, close it, reopen it, query relationships - all data should be present.

### Tests for User Story 1 (TDD - Write These FIRST)

> **NOTE: Write these tests FIRST, ensure they FAIL before implementation**

- [X] T011 [P] [US1] Write contract test for empty rel_table save/load in tests/contract/test_rel_persistence_format.rs
- [X] T012 [P] [US1] Write contract test for single rel_table serialization format in tests/contract/test_rel_persistence_format.rs
- [X] T013 [P] [US1] Write contract test for multiple rel_tables save/load in tests/contract/test_rel_persistence_format.rs
- [X] T014 [P] [US1] Write contract test for CSR invariant preservation in tests/contract/test_rel_persistence_format.rs
- [X] T015 [P] [US1] Write integration test for basic relationship persistence in tests/integration/test_database_restart.rs
- [X] T016 [P] [US1] Write integration test for empty relationship tables in tests/integration/test_database_restart.rs
- [X] T017 [P] [US1] Write integration test for multiple relationship tables persistence in tests/integration/test_database_restart.rs

### Implementation for User Story 1

- [X] T018 [US1] Implement load_rel_table_data() function in src/lib.rs (handle empty database case)
- [X] T019 [US1] Implement load_rel_table_data() length validation and error handling in src/lib.rs
- [X] T020 [US1] Implement load_rel_table_data() deserialization logic in src/lib.rs
- [X] T021 [US1] Implement load_rel_table_data() schema consistency validation in src/lib.rs
- [X] T022 [US1] Implement load_rel_table_data() RelTable instance creation in src/lib.rs
- [X] T023 [US1] Modify Database::open() to call load_rel_table_data() and populate rel_tables in src/lib.rs
- [X] T024 [US1] Modify save_all_data() to serialize rel_tables HashMap in src/lib.rs
- [X] T025 [US1] Modify save_all_data() to write rel_table data to page 3 in src/lib.rs
- [X] T026 [US1] Add size validation to save_all_data() for rel_table metadata in src/lib.rs
- [X] T027 [US1] Run all contract tests and verify they PASS
- [X] T028 [US1] Run all integration tests and verify they PASS
- [X] T029 [US1] Verify cargo clippy reports zero warnings

**Checkpoint**: At this point, User Story 1 should be fully functional - relationships persist across database restarts

---

## Phase 4: User Story 2 - CSV-Imported Relationships Persist After Restart (Priority: P2)

**Goal**: Ensure bulk-imported relationship data survives database restart, making bulk operations practical for production use.

**Independent Test**: Import relationships via CSV using COPY FROM, close database, reopen it, verify all imported relationships are present.

### Tests for User Story 2 (TDD - Write These FIRST)

- [X] T030 [P] [US2] Write integration test for CSV import with 1000 relationships in tests/integration/test_database_restart.rs
- [X] T031 [P] [US2] Write integration test for multiple CSV imports across different rel_tables in tests/integration/test_database_restart.rs

### Implementation for User Story 2

- [X] T032 [US2] Verify CSV import uses existing COPY FROM implementation (no changes needed)
- [X] T033 [US2] Test CSV import of 1000 relationships followed by close/reopen cycle
- [X] T034 [US2] Test CSV import of multiple relationship tables followed by close/reopen cycle
- [X] T035 [US2] Run all US2 integration tests and verify they PASS
- [X] T036 [US2] Run benchmark: cargo bench --bench csv_benchmark to verify no regression

**Checkpoint**: At this point, User Stories 1 AND 2 should both work - CSV imports persist correctly

---

## Phase 5: User Story 3 - Recovery After Uncommitted Relationship Changes (Priority: P3)

**Goal**: Ensure WAL recovery mechanism handles relationships correctly, delivering crash-safe relationship storage.

**Independent Test**: Create committed relationships, make uncommitted changes, simulate crash, reopen database, verify only committed relationships are present.

### Tests for User Story 3 (TDD - Write These FIRST)

- [X] T037 [P] [US3] Write integration test for uncommitted relationship changes in tests/integration/test_wal_recovery.rs
- [X] T038 [P] [US3] Write integration test for committed relationships after crash in tests/integration/test_wal_recovery.rs
- [X] T039 [P] [US3] Write integration test for WAL replay with CreateRel operations in tests/integration/test_wal_recovery.rs
- [X] T040 [P] [US3] Write integration test for WAL replay with InsertRel operations in tests/integration/test_wal_recovery.rs

### Implementation for User Story 3

- [X] T041 [US3] Modify replay_wal() to initialize empty RelTable on CreateRel in src/lib.rs
- [X] T042 [US3] Modify replay_wal() to handle InsertRel operations in src/lib.rs
- [X] T043 [US3] Add debug assertions for CSR invariants in RelTable::from_data() in src/storage/rel_table.rs
- [X] T044 [US3] Test crash simulation: create relationships, commit, kill process, reopen, verify recovery
- [X] T045 [US3] Test crash simulation: create relationships, don't commit, kill process, reopen, verify rollback
- [X] T046 [US3] Run all US3 integration tests and verify they PASS
- [X] T047 [US3] Run benchmark: cargo bench --bench storage_benchmark to verify no regression

**Checkpoint**: All user stories should now be independently functional - relationships are crash-safe

---

## Phase 6: Version Migration & Backward Compatibility

**Goal**: Ensure version 1 databases can be upgraded to version 2 without data loss

### Tests for Version Migration (TDD - Write These FIRST)

- [X] T048 [P] Write contract test for version 1 to version 2 header migration in tests/contract/test_rel_persistence_format.rs
- [X] T049 [P] Write integration test for opening version 1 database with version 2 code in tests/integration/test_database_restart.rs

### Implementation for Version Migration

- [X] T050 Implement version detection logic in DatabaseHeader::deserialize_with_migration_flag() in src/storage/mod.rs
- [X] T051 Test opening a version 1 database (create manually or use fixture)
- [X] T052 Verify version 1 database opens with empty rel_tables and allocates page 3
- [X] T053 Add relationships to upgraded database, save, verify version 2 format
- [X] T054 Run all version migration tests and verify they PASS

**Checkpoint**: Version migration works - version 1 databases upgrade seamlessly

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: Improvements that affect multiple user stories

- [X] T055 [P] Add benchmark for database open time with varying relationship counts in benches/rel_persist_benchmark.rs
- [X] T056 [P] Add benchmark for relationship query performance before/after restart in benches/rel_persist_benchmark.rs
- [X] T057 [P] Add comprehensive doc comments to load_rel_table_data() in src/lib.rs
- [X] T058 [P] Add comprehensive doc comments to DatabaseHeader fields in src/storage/mod.rs
- [X] T059 Run full test suite: cargo test --all-features
- [X] T060 Run all benchmarks: cargo bench and verify <5% regression
- [X] T061 Run cargo clippy and ensure zero warnings
- [X] T062 Validate quickstart.md scenarios manually
- [X] T063 Update CLAUDE.md with feature completion status
- [X] T064 Code review: verify all invariants documented in data-model.md are enforced

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies - can start immediately
- **Foundational (Phase 2)**: Depends on Setup completion - BLOCKS all user stories
- **User Story 1 (Phase 3)**: Depends on Foundational (Phase 2) completion - MVP
- **User Story 2 (Phase 4)**: Depends on User Story 1 completion (uses same persistence layer)
- **User Story 3 (Phase 5)**: Depends on User Story 1 completion (extends WAL replay)
- **Version Migration (Phase 6)**: Depends on User Story 1 completion (tests migration path)
- **Polish (Phase 7)**: Depends on all user stories being complete

### User Story Dependencies

- **User Story 1 (P1)**: Can start after Foundational (Phase 2) - No dependencies on other stories
- **User Story 2 (P2)**: Can start after US1 complete - Builds on basic persistence
- **User Story 3 (P3)**: Can start after US1 complete - Extends with WAL recovery
- **Note**: US2 and US3 could potentially run in parallel if team capacity allows, as they modify different code paths

### Within Each User Story

- Tests MUST be written and FAIL before implementation (TDD)
- Contract tests before integration tests
- Load function implementation before save function modification
- Core implementation before edge case handling
- All tests for a story must PASS before moving to next story

### Parallel Opportunities

- All Setup tasks marked [P] can run in parallel
- All Foundational tasks marked [P] can run in parallel (within Phase 2)
- All tests for a user story marked [P] can be written in parallel
- All benchmarks in Polish phase marked [P] can run in parallel
- Within US1: Contract tests can all be written in parallel (T011-T014)
- Within US1: Integration tests can all be written in parallel (T015-T017)

---

## Parallel Example: User Story 1

```bash
# Launch all contract tests for User Story 1 together:
Task: "Write contract test for empty rel_table save/load in tests/contract/test_rel_persistence_format.rs"
Task: "Write contract test for single rel_table serialization format in tests/contract/test_rel_persistence_format.rs"
Task: "Write contract test for multiple rel_tables save/load in tests/contract/test_rel_persistence_format.rs"
Task: "Write contract test for CSR invariant preservation in tests/contract/test_rel_persistence_format.rs"

# Launch all integration tests for User Story 1 together:
Task: "Write integration test for basic relationship persistence in tests/integration/test_database_restart.rs"
Task: "Write integration test for empty relationship tables in tests/integration/test_database_restart.rs"
Task: "Write integration test for multiple relationship tables persistence in tests/integration/test_database_restart.rs"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup
2. Complete Phase 2: Foundational (CRITICAL - blocks all stories)
3. Complete Phase 3: User Story 1
   - Write all tests FIRST (T011-T017)
   - Verify tests FAIL
   - Implement functionality (T018-T026)
   - Verify tests PASS (T027-T028)
4. **STOP and VALIDATE**: Test User Story 1 independently using quickstart.md
5. Deploy/demo if ready

### Incremental Delivery

1. Complete Setup + Foundational → Foundation ready
2. Add User Story 1 → Test independently → Deploy/Demo (MVP - core bug fixed!)
3. Add User Story 2 → Test independently → Deploy/Demo (bulk import works)
4. Add User Story 3 → Test independently → Deploy/Demo (crash recovery works)
5. Add Version Migration → Test independently → Deploy/Demo (backward compatibility)
6. Polish phase → Final validation → Production ready
7. Each story adds value without breaking previous stories

### Sequential Implementation (Recommended for Solo Developer)

Due to dependencies between stories, sequential implementation is recommended:

1. Team completes Setup + Foundational together
2. Complete User Story 1 (P1 - highest priority, MVP)
3. Complete User Story 2 (P2 - builds on US1)
4. Complete User Story 3 (P3 - extends US1)
5. Complete Version Migration (ensures backward compatibility)
6. Complete Polish phase (benchmarks, documentation)

---

## Notes

- [P] tasks = different files, no dependencies
- [Story] label maps task to specific user story for traceability
- Each user story should be independently completable and testable
- **TDD CRITICAL**: Verify tests fail before implementing functionality
- Commit after each task or logical group
- Stop at any checkpoint to validate story independently
- Follow constitution: Port-first, TDD, benchmarking, Rust best practices, safety over performance
- Estimated implementation effort: ~320 LOC across ~100 LOC in src/lib.rs, ~10 LOC in src/storage/mod.rs, ~10 LOC in src/error.rs, ~200 LOC in tests
- No new dependencies required - uses existing serde, bincode, parking_lot

---

## Success Criteria Checklist

From spec.md success criteria:

- [ ] SC-001: Database with relationships can be closed and reopened 100 times without losing data
- [ ] SC-002: Queries for relationships return identical results before and after database restart
- [ ] SC-003: CSV import of 10,000 relationships survives restart
- [ ] SC-004: Zero silent data loss - failures produce explicit error messages
- [ ] SC-005: Relationship table schemas and data both present after restart
- [ ] SC-006: WAL recovery correctly restores committed relationships after crash
- [ ] SC-007: Database open time increases linearly with number of relationships
- [ ] SC-008: Memory usage during open remains constant (on-demand loading)
