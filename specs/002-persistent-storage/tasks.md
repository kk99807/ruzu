# Tasks: Persistent Storage with Edge Support

**Input**: Design documents from `/specs/002-persistent-storage/`
**Prerequisites**: plan.md (required), spec.md (required), research.md, data-model.md, contracts/storage-format.md

**Tests**: TDD approach per constitution - tests FIRST, ensure they FAIL before implementation.

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

## Path Conventions

Based on plan.md project structure:
- Source: `src/` at repository root
- Tests: `tests/` at repository root
- New modules: `src/storage/buffer_pool/`, `src/storage/page/`, `src/storage/wal/`, `src/storage/csv/`

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Add new dependencies and create module structure for persistent storage

- [X] T001 Add Phase 1 dependencies to Cargo.toml: memmap2, parking_lot, serde + bincode, csv, crc32fast, uuid, proptest, tempfile
- [X] T002 [P] Create buffer_pool module structure with mod.rs in src/storage/buffer_pool/mod.rs
- [X] T003 [P] Create page module structure with mod.rs in src/storage/page/mod.rs
- [X] T004 [P] Create wal module structure with mod.rs in src/storage/wal/mod.rs
- [X] T005 [P] Create csv module structure with mod.rs in src/storage/csv/mod.rs
- [X] T006 [P] Create tests/contract/ directory for format compatibility tests
- [X] T007 [P] Create tests/integration/ directory for multi-component tests
- [X] T008 Update src/storage/mod.rs to export new submodules
- [X] T009 Extend src/error.rs with storage-specific error variants (StorageError enum per contracts/storage-format.md)

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core infrastructure that MUST be complete before ANY user story can be implemented

**CRITICAL**: No user story work can begin until this phase is complete

### 2.1 Page Infrastructure

- [X] T010 [P] Define PAGE_SIZE constant (4096) and PageId struct in src/storage/page/page_id.rs
- [X] T011 [P] Implement Page struct with data array and checksum in src/storage/page/mod.rs
- [X] T012 Implement DiskManager for file I/O abstraction in src/storage/page/disk_manager.rs

### 2.2 Buffer Pool Core

- [X] T013 [P] Define PageState enum (EVICTED/LOCKED/MARKED/UNLOCKED) with atomic operations in src/storage/buffer_pool/page_state.rs
- [X] T014 [P] Implement BufferFrame struct with pin_count, dirty flag, last_access in src/storage/buffer_pool/buffer_frame.rs
- [X] T015 [P] Implement LRU eviction queue in src/storage/buffer_pool/eviction.rs
- [X] T016 Implement VmRegion for mmap'd memory in src/storage/buffer_pool/vm_region.rs (requires unsafe, add SAFETY comments)
- [X] T017 Implement BufferPool struct with pin/unpin operations in src/storage/buffer_pool/mod.rs
- [X] T018 Add PageHandle RAII guard that unpins on drop in src/storage/buffer_pool/mod.rs

### 2.3 Database Header & Catalog Persistence

- [X] T019 [P] Define DatabaseHeader struct matching contracts/storage-format.md in src/storage/mod.rs
- [X] T020 [P] Add serde derives to catalog types in src/catalog/schema.rs (NodeTableSchema, ColumnDef)
- [X] T021 [P] Define SerializedCatalog wrapper for bincode serialization in src/catalog/mod.rs
- [X] T022 Implement catalog serialization/deserialization with bincode in src/catalog/mod.rs
- [X] T023 Implement DatabaseHeader read/write with magic bytes and checksum validation in src/storage/mod.rs

### 2.4 Foundational Tests

- [X] T024 [P] Contract test for database header format in tests/contract/test_header_format.rs
- [X] T025 [P] Contract test for catalog serialization format in tests/contract/test_catalog_format.rs
- [X] T026 [P] Unit tests for PageState transitions in src/storage/buffer_pool/page_state.rs
- [X] T027 [P] Unit tests for LRU eviction ordering in src/storage/buffer_pool/eviction.rs
- [X] T028 Integration test for buffer pool pin/unpin/evict cycle in tests/integration/test_buffer_pool.rs

**Checkpoint**: Foundation ready - user story implementation can now begin

---

## Phase 3: User Story 1 - Database Persistence Across Sessions (Priority: P1)

**Goal**: Graph data persists to disk; when application restarts, all previously created nodes, relationships, and schema remain intact.

**Independent Test**: Create database, add nodes, close application, reopen, query same data - delivers core value of a database.

### Tests for User Story 1

> **NOTE: Write these tests FIRST, ensure they FAIL before implementation**

- [X] T029 [P] [US1] Integration test: create db, add nodes, close, reopen, verify nodes in tests/integration_tests.rs
- [X] T030 [P] [US1] Integration test: create db, add schema, close, reopen, verify catalog in tests/integration_tests.rs
- [X] T031 [P] [US1] Integration test: new directory auto-creates database files in tests/integration_tests.rs
- [X] T032 [P] [US1] Contract test for node data page format in tests/contract_tests.rs

### Implementation for User Story 1

- [X] T033 [US1] Implement persistent NodeTable backed by buffer pool in src/storage/table.rs
- [X] T034 [US1] Add columnar page layout for fixed-width types (INT64, FLOAT64, BOOL) in src/storage/page/mod.rs
- [X] T035 [US1] Add columnar page layout for variable-width STRING type in src/storage/page/mod.rs
- [X] T036 [US1] Implement Database::open() that creates or opens database directory in src/lib.rs
- [X] T037 [US1] Implement Database::close() that flushes dirty pages and writes header in src/lib.rs
- [X] T038 [US1] Implement catalog load on database open (deserialize from catalog pages) in src/lib.rs
- [X] T039 [US1] Implement catalog save on database close (serialize to catalog pages) in src/lib.rs
- [X] T040 [US1] Wire up CREATE NODE TABLE to persist to catalog and allocate table pages in src/lib.rs
- [X] T041 [US1] Wire up CREATE node statement to persist to node table pages in src/lib.rs
- [X] T042 [US1] Wire up MATCH query to read from persistent node tables in src/lib.rs

**Checkpoint**: At this point, nodes persist across sessions - User Story 1 is independently testable

---

## Phase 4: User Story 2 - Crash Recovery (Priority: P1)

**Goal**: Database recovers gracefully from unexpected shutdowns; committed transactions are not lost; database remains consistent.

**Independent Test**: Perform writes, forcibly terminate before clean shutdown, verify data integrity on restart.

### Tests for User Story 2

> **NOTE: Write these tests FIRST, ensure they FAIL before implementation**

- [X] T043 [P] [US2] Contract test for WAL header and record format in tests/contract_tests.rs (wal_format_contracts module)
- [X] T044 [P] [US2] Integration test: commit, crash before checkpoint, replay WAL, verify data in tests/integration_tests.rs (crash_recovery_tests module)
- [X] T045 [P] [US2] Integration test: uncommitted transaction, crash, verify rollback in tests/integration_tests.rs (crash_recovery_tests module)
- [X] T046 [P] [US2] Integration test: corrupted WAL segment, verify error reporting in tests/integration_tests.rs (crash_recovery_tests module)

### Implementation for User Story 2

- [X] T047 [P] [US2] Define WalRecordType enum and WalPayload variants in src/storage/wal/record.rs
- [X] T048 [P] [US2] Implement WalRecord struct with serde serialization in src/storage/wal/record.rs
- [X] T049 [US2] Implement WalWriter for append-only WAL writes with checksums in src/storage/wal/writer.rs
- [X] T050 [US2] Implement WalReader for sequential WAL reading with checksum validation in src/storage/wal/reader.rs
- [X] T051 [US2] Implement WAL replay state machine (per data-model.md) in src/storage/wal/reader.rs
- [X] T052 [US2] Implement Checkpointer for checkpoint coordination in src/storage/wal/checkpointer.rs
- [X] T053 [US2] Integrate WAL write before page modifications in src/lib.rs (Database::execute_create_node)
- [X] T054 [US2] Integrate WAL replay on Database::open() when wal.log exists in src/lib.rs
- [X] T055 [US2] Implement Database::checkpoint() to force checkpoint with WAL truncation in src/lib.rs
- [X] T056 [US2] Add transaction begin/commit WAL records to CREATE operations in src/lib.rs (execute_create_node)

**Checkpoint**: At this point, crash recovery works - User Story 2 is independently testable ✅ COMPLETE

---

## Phase 5: User Story 3 - Relationship/Edge Support (Priority: P1)

**Goal**: Create relationships between nodes to model and query graph data with edges connecting entities.

**Independent Test**: Create nodes, create relationships between them, query relationships.

### Tests for User Story 3

> **NOTE: Write these tests FIRST, ensure they FAIL before implementation**

- [X] T057 [P] [US3] Contract test for CSR page format (offsets, neighbors, rel_ids) in tests/contract_tests.rs (csr_format_contracts module)
- [X] T058 [P] [US3] Integration test: CREATE REL TABLE, create relationship, query it in tests/integration_tests.rs (relationship_tests module)
- [X] T059 [P] [US3] Integration test: relationship with properties, query returns properties in tests/integration_tests.rs (relationship_tests module)
- [X] T060 [P] [US3] Integration test: node with multiple outgoing relationships in tests/integration_tests.rs (relationship_tests module)
- [X] T061 [P] [US3] Integration test: referential integrity (reject rel to non-existent node) in tests/integration_tests.rs (relationship_tests module)

### Implementation for User Story 3

- [X] T062 [P] [US3] Define RelTableSchema struct with serde in src/catalog/schema.rs
- [X] T063 [P] [US3] Define Direction enum (Forward/Backward/Both) in src/catalog/schema.rs
- [X] T064 [US3] Implement CsrNodeGroup struct per data-model.md in src/storage/rel_table.rs
- [X] T065 [US3] Implement CSR offset/neighbor/relid page serialization in src/storage/rel_table.rs
- [X] T066 [US3] Implement persistent RelTable backed by buffer pool in src/storage/rel_table.rs
- [X] T067 [US3] Implement forward and backward CSR indices in RelTable in src/storage/rel_table.rs
- [X] T068 [US3] Extend parser to support CREATE REL TABLE syntax in src/parser/grammar.pest
- [X] T069 [US3] Extend AST for relationship table creation in src/parser/ast.rs
- [X] T070 [US3] Extend binder for relationship table validation in src/lib.rs (execute_create_rel_table)
- [X] T071 [US3] Implement CREATE REL TABLE executor in src/lib.rs
- [X] T072 [US3] Extend parser to support relationship patterns in MATCH (e.g., (a)-[:REL]->(b)) in src/parser/grammar.pest
- [X] T073 [US3] Extend parser to support CREATE relationship syntax in src/parser/grammar.pest
- [X] T074 [US3] Extend AST for relationship patterns and creation in src/parser/ast.rs
- [X] T075 [US3] Extend binder for relationship patterns in src/lib.rs (execute_match_rel)
- [X] T076 [US3] Implement relationship creation executor with referential integrity check in src/lib.rs (execute_match_create)
- [X] T077 [US3] Implement RelScan operator for relationship traversal in src/lib.rs (execute_match_rel)
- [X] T078 [US3] Add WAL records for relationship insertion/deletion in src/storage/wal/record.rs (WalPayload::RelInsertion)

**Checkpoint**: At this point, relationships work end-to-end - User Story 3 is independently testable ✅ COMPLETE

---

## Phase 6: User Story 4 - Bulk CSV Ingestion (Priority: P2)

**Goal**: Import large datasets from CSV files efficiently without individual INSERT statements.

**Independent Test**: Prepare CSV with 10,000 records, import, verify data integrity and measure time.

### Tests for User Story 4

> **NOTE: Write these tests FIRST, ensure they FAIL before implementation**

- [X] T079 [P] [US4] Integration test: bulk import 10,000 nodes from CSV in tests/integration_tests.rs (csv_import_tests module)
- [X] T080 [P] [US4] Integration test: bulk import relationships with FROM/TO columns in tests/integration_tests.rs (csv_import_tests module)
- [X] T081 [P] [US4] Integration test: CSV with invalid rows, verify error reporting in tests/integration_tests.rs (csv_import_tests module)
- [X] T082 [P] [US4] Integration test: progress callback invoked during import in tests/integration_tests.rs (csv_import_tests module)

### Implementation for User Story 4

- [X] T083 [P] [US4] Define CsvImportConfig struct per data-model.md in src/storage/csv/mod.rs
- [X] T084 [P] [US4] Define ImportProgress and ImportError structs in src/storage/csv/mod.rs
- [X] T085 [US4] Implement CSV parser wrapper with configurable options in src/storage/csv/parser.rs
- [X] T086 [US4] Implement node bulk loader (batch insert 2048 rows) in src/storage/csv/node_loader.rs
- [X] T087 [US4] Implement relationship bulk loader with CSR building in src/storage/csv/rel_loader.rs
- [X] T088 [US4] Add Database::import_nodes() API in src/lib.rs
- [X] T089 [US4] Add Database::import_relationships() API in src/lib.rs
- [X] T090 [US4] Extend parser to support COPY command syntax in src/parser/grammar.pest
- [X] T091 [US4] Implement COPY command executor in src/lib.rs (execute_copy method)
- [X] T092 [US4] Add progress reporting callback support in src/storage/csv/mod.rs
- [X] T093 [US4] Add atomic vs continue-on-error mode in src/storage/csv/mod.rs

**Checkpoint**: At this point, bulk CSV import works - User Story 4 is independently testable ✅ COMPLETE

---

## Phase 7: User Story 5 - Memory-Constrained Operation (Priority: P2)

**Goal**: Database operates efficiently when dataset exceeds available memory using buffer pool eviction.

**Independent Test**: Configure small buffer pool (64MB), load dataset larger than buffer pool, verify correct operation.

### Tests for User Story 5

> **NOTE: Write these tests FIRST, ensure they FAIL before implementation**

- [X] T094 [P] [US5] Integration test: 64MB buffer pool, load 200MB data, queries work in tests/integration_tests.rs
- [X] T095 [P] [US5] Integration test: query touches evicted pages, transparent reload in tests/integration_tests.rs
- [X] T096 [P] [US5] Integration test: concurrent queries, no corruption in tests/integration_tests.rs
- [X] T097 [P] [US5] Property test: buffer pool invariants under random operations in tests/integration_tests.rs

### Implementation for User Story 5

- [X] T098 [US5] Implement configurable buffer pool size in DatabaseConfig in src/lib.rs
- [X] T099 [US5] Implement buffer pool statistics (pages_used, hit_rate, evictions) in src/storage/buffer_pool/mod.rs
- [X] T100 [US5] Add Database::buffer_pool_stats() API in src/lib.rs
- [X] T101 [US5] Implement proper dirty page flushing before eviction in src/storage/buffer_pool/mod.rs
- [X] T102 [US5] Add concurrent access safety with parking_lot RwLock in src/storage/buffer_pool/mod.rs
- [X] T103 [US5] Tune eviction batch size (64 candidates per round per research.md) in src/storage/buffer_pool/eviction.rs

**Checkpoint**: At this point, memory-constrained operation works - User Story 5 is independently testable ✅ COMPLETE

---

## Phase 8: Polish & Cross-Cutting Concerns

**Purpose**: Improvements that affect multiple user stories

- [X] T104 [P] Add /// documentation for all public APIs in src/lib.rs
- [X] T105 [P] Add /// documentation for storage module public types in src/storage/mod.rs
- [X] T106 Run cargo clippy --all-targets and fix all warnings
- [X] T107 Run cargo fmt and verify formatting
- [X] T108 [P] Add proptest property-based tests for WAL replay correctness in tests/integration_tests.rs
- [X] T109 [P] Add criterion benchmark for CSV import (target: 50K nodes/sec) in benches/csv_benchmark.rs
- [X] T110 [P] Add criterion benchmark for buffer pool operations in benches/buffer_benchmark.rs
- [X] T111 Run quickstart.md examples and verify all work correctly
- [X] T112 Verify all existing Phase 0 tests continue to pass (no regression)
- [X] T113 Update CLAUDE.md with Phase 1 completion notes

**Checkpoint**: Phase 8 complete - Feature 002-persistent-storage fully implemented ✅ COMPLETE

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies - can start immediately
- **Foundational (Phase 2)**: Depends on Setup completion - BLOCKS all user stories
- **User Stories (Phase 3-7)**: All depend on Foundational phase completion
  - US1 (Persistence): Foundation only
  - US2 (Crash Recovery): Foundation + partial US1 (needs persistent tables for WAL testing)
  - US3 (Relationships): Foundation + US1 (needs node persistence)
  - US4 (CSV Import): Foundation + US1 + US3 (needs both node and relationship tables)
  - US5 (Memory-Constrained): Foundation + US1 (needs working persistence)
- **Polish (Phase 8)**: Depends on all user stories being complete

### User Story Dependencies

- **User Story 1 (P1)**: Can start after Foundational - No dependencies on other stories
- **User Story 2 (P1)**: Can start after Foundational - Tests benefit from US1 but WAL logic is independent
- **User Story 3 (P1)**: Requires US1 (needs persistent node tables for relationship endpoints)
- **User Story 4 (P2)**: Requires US1 + US3 (needs both node and relationship persistence)
- **User Story 5 (P2)**: Can start after Foundational - Buffer pool is foundational but testing needs US1

### Recommended Execution Order

**MVP Path (fastest to working database):**
1. Phase 1: Setup (all in parallel)
2. Phase 2: Foundational (in dependency order)
3. Phase 3: User Story 1 (Persistence) - **MVP deliverable**
4. Phase 5: User Story 3 (Relationships) - Depends on US1
5. Phase 4: User Story 2 (Crash Recovery) - Can run after US1
6. Phase 6: User Story 4 (CSV Import) - Depends on US1 + US3
7. Phase 7: User Story 5 (Memory-Constrained) - After US1
8. Phase 8: Polish

### Within Each User Story

- Tests MUST be written and FAIL before implementation
- Models/structs before services
- Services before executors
- Core implementation before integration
- Story complete before moving to next priority

### Parallel Opportunities

- All Setup tasks marked [P] can run in parallel
- All Foundational tasks marked [P] can run in parallel (within Phase 2)
- Once Foundational phase completes:
  - US1 and US2 can start in parallel (US2 has some US1 dependencies but WAL core is independent)
  - US5 buffer pool testing can parallel with US1
- Within each story: all tests marked [P] can run in parallel
- Within each story: all models marked [P] can run in parallel

---

## Parallel Example: Phase 2 Foundational

```bash
# Launch all parallel tasks for Phase 2.1-2.3:
T010: Define PAGE_SIZE constant and PageId in src/storage/page/page_id.rs
T011: Implement Page struct in src/storage/page/mod.rs
T013: Define PageState enum in src/storage/buffer_pool/page_state.rs
T014: Implement BufferFrame struct in src/storage/buffer_pool/buffer_frame.rs
T015: Implement LRU eviction queue in src/storage/buffer_pool/eviction.rs
T019: Define DatabaseHeader struct in src/storage/mod.rs
T020: Add serde derives to catalog types in src/catalog/schema.rs
T021: Define SerializedCatalog wrapper in src/catalog/mod.rs
T024: Contract test for header format in tests/contract/test_header_format.rs
T025: Contract test for catalog format in tests/contract/test_catalog_format.rs
T026: Unit tests for PageState in src/storage/buffer_pool/page_state.rs
T027: Unit tests for LRU eviction in src/storage/buffer_pool/eviction.rs
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup
2. Complete Phase 2: Foundational (CRITICAL - blocks all stories)
3. Complete Phase 3: User Story 1
4. **STOP and VALIDATE**: Test User Story 1 independently
5. Deploy/demo if ready - data now persists!

### Incremental Delivery

1. Complete Setup + Foundational -> Foundation ready
2. Add User Story 1 -> Test independently -> Deploy/Demo (MVP!)
3. Add User Story 3 -> Test independently -> Graph relationships work!
4. Add User Story 2 -> Test independently -> Crash recovery!
5. Add User Story 4 -> Test independently -> Bulk loading!
6. Add User Story 5 -> Test independently -> Large datasets!
7. Each story adds value without breaking previous stories

### Parallel Team Strategy

With 2-3 developers:

1. Team completes Setup + Foundational together
2. Once Foundational is done:
   - Developer A: User Story 1 (Persistence)
   - Developer B: User Story 2 (WAL core, then integrate with US1)
3. After US1 complete:
   - Developer A: User Story 3 (Relationships)
   - Developer B: User Story 5 (Buffer pool stress)
4. After US3 complete:
   - Developer A: User Story 4 (CSV Import)
   - Developer B: Polish

---

## Notes

- [P] tasks = different files, no dependencies
- [Story] label maps task to specific user story for traceability
- Each user story should be independently completable and testable
- Verify tests fail before implementing
- Commit after each task or logical group
- Stop at any checkpoint to validate story independently
- Avoid: vague tasks, same file conflicts, cross-story dependencies that break independence
- All `unsafe` code MUST have `// SAFETY:` comments per constitution
- Target performance: 50K nodes/sec CSV import, 100K rels/sec, <30 sec crash recovery for 10GB DB
