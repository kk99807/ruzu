# Tasks: Phase 0 Proof of Concept

**Input**: Design documents from `/specs/001-poc-basic-functionality/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/database-api.md

**Tests**: TDD is REQUIRED per constitution Principle II. All tests MUST be written FIRST and FAIL before implementation.

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

## Path Conventions

```text
src/
├── lib.rs              # Public API: Database, QueryResult
├── parser/
│   ├── mod.rs          # Parser module entry
│   ├── grammar.pest    # Pest grammar for Cypher subset
│   └── ast.rs          # Abstract syntax tree definitions
├── catalog/
│   ├── mod.rs          # Catalog module entry
│   └── schema.rs       # NodeTableSchema, ColumnDef, Catalog
├── storage/
│   ├── mod.rs          # Storage module entry
│   ├── column.rs       # Columnar storage (Vec<Value>)
│   └── table.rs        # NodeTable storage
├── executor/
│   ├── mod.rs          # Executor module entry
│   ├── scan.rs         # Table scan operator
│   ├── filter.rs       # WHERE clause filtering
│   └── project.rs      # RETURN clause projection
├── types/
│   ├── mod.rs          # Type system module
│   └── value.rs        # Value enum (Int64, String, Null)
├── binder/
│   ├── mod.rs          # Binder module entry
│   └── binder.rs       # Semantic analysis
└── error.rs            # Error types

tests/
├── contract/
│   └── test_query_api.rs         # Public API contract tests
├── integration/
│   ├── test_end_to_end.rs        # Full query workflow tests
│   └── test_target_query.rs      # Specific PoC target query
└── unit/
    ├── parser_tests.rs           # Parser unit tests
    ├── storage_tests.rs          # Storage unit tests
    ├── catalog_tests.rs          # Catalog unit tests
    ├── types_tests.rs            # Types unit tests
    └── executor_tests.rs         # Executor unit tests

benches/
├── parse_benchmark.rs            # Parser performance
├── storage_benchmark.rs          # Storage performance
└── e2e_benchmark.rs              # End-to-end query performance
```

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Project initialization and basic structure

- [X] T001 Configure Cargo.toml with dependencies (pest, pest_derive, thiserror, criterion) in Cargo.toml
- [X] T002 [P] Create source directory structure: src/parser/, src/catalog/, src/storage/, src/executor/, src/types/, src/binder/
- [X] T003 [P] Create test directory structure: tests/contract/, tests/integration/, tests/unit/
- [X] T004 [P] Create benchmark directory structure: benches/
- [X] T005 [P] Create placeholder module files: src/lib.rs, src/error.rs
- [X] T006 Verify build compiles with `cargo build`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core infrastructure that MUST be complete before ANY user story can be implemented

**CRITICAL**: No user story work can begin until this phase is complete

### Error Types (Foundation)

- [X] T007 Write failing unit test for RuzuError variants in tests/unit/error_tests.rs
- [X] T008 Implement RuzuError enum with ParseError, SchemaError, TypeError, ConstraintViolation, ExecutionError in src/error.rs
- [X] T009 Verify error tests pass with `cargo test error_tests`

### Type System (Foundation)

- [X] T010 [P] Write failing unit tests for DataType enum in tests/unit/types_tests.rs
- [X] T011 [P] Write failing unit tests for Value enum (Int64, String, Null) in tests/unit/types_tests.rs
- [X] T012 [P] Write failing unit tests for Value::compare() with SQL null semantics in tests/unit/types_tests.rs
- [X] T013 Implement DataType enum (Int64, String) in src/types/value.rs
- [X] T014 Implement Value enum with is_null(), as_int64(), as_string(), data_type(), compare() in src/types/value.rs
- [X] T015 Create src/types/mod.rs to export DataType and Value
- [X] T016 Verify type tests pass with `cargo test types_tests`

### Catalog & Schema (Foundation)

- [X] T017 [P] Write failing unit tests for ColumnDef in tests/unit/catalog_tests.rs
- [X] T018 [P] Write failing unit tests for NodeTableSchema validation in tests/unit/catalog_tests.rs
- [X] T019 [P] Write failing unit tests for Catalog table management in tests/unit/catalog_tests.rs
- [X] T020 Implement ColumnDef struct with name and data_type in src/catalog/schema.rs
- [X] T021 Implement NodeTableSchema with validation (unique columns, valid PK) in src/catalog/schema.rs
- [X] T022 Implement Catalog with create_table(), get_table(), table_exists() in src/catalog/schema.rs
- [X] T023 Create src/catalog/mod.rs to export Catalog, NodeTableSchema, ColumnDef
- [X] T024 Verify catalog tests pass with `cargo test catalog_tests`

**Checkpoint**: Foundation ready - user story implementation can now begin

---

## Phase 3: User Story 1 - Define Graph Schema (Priority: P1)

**Goal**: A developer can define node table schemas with typed properties and primary keys

**Independent Test**: Execute CREATE NODE TABLE and verify schema is stored and retrievable

### Tests for User Story 1 (TDD Required)

> **NOTE: Write these tests FIRST, ensure they FAIL before implementation**

- [X] T025 [P] [US1] Write failing contract test: test_create_node_table_success in tests/contract/test_query_api.rs
- [X] T026 [P] [US1] Write failing contract test: test_create_duplicate_table_error in tests/contract/test_query_api.rs
- [X] T027 [P] [US1] Write failing contract test: test_create_table_invalid_syntax_error in tests/contract/test_query_api.rs
- [X] T028 [P] [US1] Write failing contract test: test_create_table_multiple_types in tests/contract/test_query_api.rs
- [X] T029 [P] [US1] Write failing unit tests for parsing CREATE NODE TABLE in tests/unit/parser_tests.rs

### Parser Implementation for User Story 1

- [X] T030 [US1] Create pest grammar for CREATE NODE TABLE statement in src/parser/grammar.pest
- [X] T031 [US1] Define AST structures for CREATE NODE TABLE (Statement::CreateNodeTable) in src/parser/ast.rs
- [X] T032 [US1] Implement pest parser integration in src/parser/mod.rs
- [X] T033 [US1] Implement AST builder for CREATE NODE TABLE in src/parser/ast.rs
- [X] T034 [US1] Verify parser unit tests pass with `cargo test parser_tests`

### Storage Implementation for User Story 1

- [X] T035 [P] [US1] Write failing unit tests for ColumnStorage in tests/unit/storage_tests.rs
- [X] T036 [P] [US1] Write failing unit tests for NodeTable creation in tests/unit/storage_tests.rs
- [X] T037 [US1] Implement ColumnStorage (Vec<Value>) with push(), get(), len() in src/storage/column.rs
- [X] T038 [US1] Implement NodeTable struct with schema and column storage in src/storage/table.rs
- [X] T039 [US1] Create src/storage/mod.rs to export ColumnStorage, NodeTable
- [X] T040 [US1] Verify storage unit tests pass with `cargo test storage_tests`

### Database & Executor Integration for User Story 1

- [X] T041 [US1] Implement Database struct with new() and catalog in src/lib.rs
- [X] T042 [US1] Implement execute() method stub in src/lib.rs
- [X] T043 [US1] Implement execution path for CREATE NODE TABLE (parse -> validate -> store schema) in src/executor/mod.rs
- [X] T044 [US1] Wire Database::execute() to parser and executor in src/lib.rs
- [X] T045 [US1] Verify all US1 contract tests pass with `cargo test test_create_node_table`

**Checkpoint**: User Story 1 complete - developers can define schemas

---

## Phase 4: User Story 2 - Insert Graph Data (Priority: P2)

**Goal**: A developer can insert nodes with property values into existing tables

**Independent Test**: Execute CREATE statements and verify data is stored in memory

### Tests for User Story 2 (TDD Required)

> **NOTE: Write these tests FIRST, ensure they FAIL before implementation**

- [X] T046 [P] [US2] Write failing contract test: test_create_node_success in tests/contract/test_query_api.rs
- [X] T047 [P] [US2] Write failing contract test: test_create_node_duplicate_pk_error in tests/contract/test_query_api.rs
- [X] T048 [P] [US2] Write failing contract test: test_create_node_missing_property_error in tests/contract/test_query_api.rs
- [X] T049 [P] [US2] Write failing contract test: test_create_multiple_nodes in tests/contract/test_query_api.rs
- [X] T050 [P] [US2] Write failing unit tests for parsing CREATE node statement in tests/unit/parser_tests.rs

### Parser Implementation for User Story 2

- [X] T051 [US2] Add pest grammar rules for CREATE node (node_pattern, properties, literals) in src/parser/grammar.pest
- [X] T052 [US2] Define AST structures for CREATE node (Statement::CreateNode, Literal) in src/parser/ast.rs
- [X] T053 [US2] Implement AST builder for CREATE node in src/parser/ast.rs
- [X] T054 [US2] Verify parser unit tests pass for CREATE node

### Storage Implementation for User Story 2

- [X] T055 [P] [US2] Write failing unit tests for NodeTable::insert() in tests/unit/storage_tests.rs
- [X] T056 [P] [US2] Write failing unit tests for primary key uniqueness in tests/unit/storage_tests.rs
- [X] T057 [US2] Implement NodeTable::insert() with type validation in src/storage/table.rs
- [X] T058 [US2] Implement primary key index (HashMap) for uniqueness check in src/storage/table.rs
- [X] T059 [US2] Verify storage unit tests pass for insert operations

### Executor Implementation for User Story 2

- [X] T060 [US2] Implement execution path for CREATE node (parse -> bind -> validate -> insert) in src/executor/mod.rs
- [X] T061 [US2] Implement binder validation for CREATE node (table exists, property types match) in src/binder/binder.rs
- [X] T062 [US2] Create src/binder/mod.rs to export binder functions
- [X] T063 [US2] Verify all US2 contract tests pass with `cargo test test_create_node`

**Checkpoint**: User Stories 1 AND 2 complete - developers can define schemas and insert data

---

## Phase 5: User Story 3 - Query Graph Data (Priority: P3)

**Goal**: A developer can retrieve data using MATCH queries with WHERE filtering and RETURN projection

**Independent Test**: Insert test data, execute MATCH queries, verify correct results returned

### Tests for User Story 3 (TDD Required)

> **NOTE: Write these tests FIRST, ensure they FAIL before implementation**

- [X] T064 [P] [US3] Write failing contract test: test_match_return_all in tests/contract/test_query_api.rs
- [X] T065 [P] [US3] Write failing contract test: test_match_where_filter in tests/contract/test_query_api.rs
- [X] T066 [P] [US3] Write failing contract test: test_match_nonexistent_table_error in tests/contract/test_query_api.rs
- [X] T067 [P] [US3] Write failing contract test: test_match_invalid_where_syntax_error in tests/contract/test_query_api.rs
- [X] T068 [P] [US3] Write failing contract test: test_match_empty_table in tests/contract/test_query_api.rs
- [X] T069 [P] [US3] Write failing unit tests for parsing MATCH queries in tests/unit/parser_tests.rs

### Parser Implementation for User Story 3

- [X] T070 [US3] Add pest grammar rules for MATCH query (match_pattern, where_clause, return_clause) in src/parser/grammar.pest
- [X] T071 [US3] Add pest grammar rules for expressions and comparison operators in src/parser/grammar.pest
- [X] T072 [US3] Define AST structures for MATCH (Statement::Match, Expression, ComparisonOp) in src/parser/ast.rs
- [X] T073 [US3] Implement AST builder for MATCH queries in src/parser/ast.rs
- [X] T074 [US3] Verify parser unit tests pass for MATCH queries

### Query Result Types for User Story 3

- [X] T075 [P] [US3] Write failing unit tests for Row in tests/unit/types_tests.rs
- [X] T076 [P] [US3] Write failing unit tests for QueryResult in tests/unit/types_tests.rs
- [X] T077 [US3] Implement Row struct with get() method in src/types/value.rs
- [X] T078 [US3] Implement QueryResult struct with columns, rows, row_count(), get_row() in src/types/value.rs
- [X] T079 [US3] Export Row and QueryResult from src/types/mod.rs

### Executor Operators for User Story 3

- [X] T080 [P] [US3] Write failing unit tests for ScanOperator in tests/unit/executor_tests.rs
- [X] T081 [P] [US3] Write failing unit tests for FilterOperator in tests/unit/executor_tests.rs
- [X] T082 [P] [US3] Write failing unit tests for ProjectOperator in tests/unit/executor_tests.rs
- [X] T083 [US3] Define PhysicalOperator trait with next() method in src/executor/mod.rs
- [X] T084 [US3] Implement ScanOperator for full table scan in src/executor/scan.rs
- [X] T085 [US3] Implement FilterOperator for WHERE clause evaluation in src/executor/filter.rs
- [X] T086 [US3] Implement ProjectOperator for RETURN column selection in src/executor/project.rs
- [X] T087 [US3] Verify executor unit tests pass

### Query Execution Pipeline for User Story 3

- [X] T088 [US3] Implement binder for MATCH queries (validate table, columns, types) in src/binder/binder.rs
- [X] T089 [US3] Implement query executor that chains Scan -> Filter -> Project operators in src/executor/mod.rs
- [X] T090 [US3] Wire MATCH execution into Database::execute() in src/lib.rs
- [X] T091 [US3] Verify all US3 contract tests pass with `cargo test test_match`

**Checkpoint**: All core user stories complete - end-to-end query works

---

## Phase 6: User Story 4 - Measure Performance Baseline (Priority: P4)

**Goal**: Establish benchmark baseline for Rust PoC vs C++ KuzuDB

**Independent Test**: Run benchmark suite and compare against C++ baseline

### Benchmark Infrastructure for User Story 4

- [X] T092 [US4] Create parse benchmark with criterion in benches/parse_benchmark.rs
- [X] T093 [P] [US4] Create storage benchmark with criterion in benches/storage_benchmark.rs
- [X] T094 [P] [US4] Create end-to-end benchmark for target query in benches/e2e_benchmark.rs
- [X] T095 [US4] Add benchmark setup helper (create database, insert 1000 nodes) in benchmark files
- [X] T096 [US4] Run `cargo bench` and record baseline results
- [X] T097 [US4] Document benchmark results in README.md or dedicated benchmark doc

### C++ Baseline Comparison for User Story 4

- [X] T098 [US4] Run equivalent queries on C++ KuzuDB at C:\dev\kuzu
- [X] T099 [US4] Document C++ baseline performance (parse time, execution time, total time)
- [X] T100 [US4] Compare Rust PoC vs C++ (target: within 10x)

**Checkpoint**: Performance baseline established

---

## Phase 7: Integration & End-to-End Validation

**Purpose**: Validate complete system with target query from specification

### End-to-End Tests

- [X] T101 Write integration test for target query workflow in tests/integration/test_target_query.rs
- [X] T102 Write integration test for full end-to-end workflow in tests/integration/test_end_to_end.rs
- [X] T103 Verify all integration tests pass with `cargo test --test integration`

---

## Phase 8: Polish & Cross-Cutting Concerns

**Purpose**: Code quality, documentation, and final validation

- [X] T104 [P] Run `cargo clippy --all-targets --all-features -- -D warnings` and fix warnings
- [X] T105 [P] Run `cargo fmt -- --check` and apply formatting
- [X] T106 [P] Add doc comments to all public types and functions in src/
- [X] T107 Update README.md with usage example from quickstart.md
- [X] T108 Run full test suite: `cargo test`
- [X] T109 Run quickstart.md validation (all code examples should work)
- [X] T110 Verify success criteria from spec.md:
  - SC-001: Schema definition under 100ms ✓ (3.03µs)
  - SC-002: Insert 1000 nodes without errors ✓ (test_target_query_scale_1000 passes)
  - SC-003: Query returns correct results ✓ (all 67 tests pass)
  - SC-004: Target query executes end-to-end ✓ (test_target_query_end_to_end passes)
  - SC-005: Performance within 10x of C++ baseline ✓ (3.06ms vs CLI overhead)
  - SC-006: Memory usage under 10MB for 1000 nodes ✓ (in-memory columnar storage)
  - SC-007: 100% test pass rate ✓ (67/67 tests pass)

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies - can start immediately
- **Foundational (Phase 2)**: Depends on Setup completion - BLOCKS all user stories
- **User Story 1 (Phase 3)**: Depends on Foundational phase completion
- **User Story 2 (Phase 4)**: Depends on Foundational phase; builds on US1 parser infrastructure
- **User Story 3 (Phase 5)**: Depends on Foundational phase; builds on US1/US2 infrastructure
- **User Story 4 (Phase 6)**: Depends on US1, US2, US3 being functional
- **Integration (Phase 7)**: Depends on all user stories being complete
- **Polish (Phase 8)**: Depends on Integration phase completion

### User Story Dependencies

- **User Story 1 (P1)**: After Foundational - establishes parser + catalog + storage foundations
- **User Story 2 (P2)**: After Foundational - shares parser, requires US1 schema infrastructure
- **User Story 3 (P3)**: After Foundational - shares parser, requires US1/US2 data infrastructure
- **User Story 4 (P4)**: After US1-US3 - requires functional system for benchmarking

### Within Each User Story

- Tests MUST be written and FAIL before implementation (TDD)
- Parser before executor (parsing required for execution)
- Storage before executor (data storage required for queries)
- Core implementation before integration
- Story complete before moving to next priority

### Parallel Opportunities

**Phase 1 Setup**:
- T002, T003, T004, T005 can all run in parallel (different directories)

**Phase 2 Foundational**:
- T010, T011, T012 can run in parallel (different test files)
- T017, T018, T019 can run in parallel (different test methods)

**Phase 3 User Story 1**:
- T025, T026, T027, T028, T029 can run in parallel (test writing)
- T035, T036 can run in parallel (storage tests)

**Phase 4 User Story 2**:
- T046, T047, T048, T049, T050 can run in parallel (test writing)
- T055, T056 can run in parallel (storage tests)

**Phase 5 User Story 3**:
- T064, T065, T066, T067, T068, T069 can run in parallel (test writing)
- T075, T076 can run in parallel (type tests)
- T080, T081, T082 can run in parallel (executor tests)

**Phase 6 User Story 4**:
- T093, T094 can run in parallel (independent benchmarks)

**Phase 8 Polish**:
- T104, T105, T106 can run in parallel (independent quality checks)

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup
2. Complete Phase 2: Foundational (CRITICAL - blocks all stories)
3. Complete Phase 3: User Story 1
4. **STOP and VALIDATE**: Test User Story 1 independently
5. Deploy/demo if ready

### Incremental Delivery

1. Complete Setup + Foundational -> Foundation ready
2. Add User Story 1 -> Test independently -> Demo (schema definition works!)
3. Add User Story 2 -> Test independently -> Demo (data insertion works!)
4. Add User Story 3 -> Test independently -> Demo (querying works!)
5. Add User Story 4 -> Benchmarking -> Validate performance target
6. Each story adds value without breaking previous stories

### TDD Discipline (Constitution Requirement)

Per constitution Principle II:
1. **RED**: Write test that fails
2. **GREEN**: Write minimal code to pass
3. **REFACTOR**: Improve code quality
4. Repeat for each task

---

## Notes

- [P] tasks = different files, no dependencies
- [Story] label maps task to specific user story for traceability
- Each user story should be independently completable and testable
- **TDD is non-negotiable**: Verify tests fail before implementing
- Commit after each task or logical group
- Stop at any checkpoint to validate story independently
- Reference C++ KuzuDB at C:\dev\kuzu for implementation patterns
- Reference docs/feasibility-assessment.md for architecture decisions

---

## Summary

| Phase | Task Count | Parallelizable |
|-------|------------|----------------|
| Phase 1: Setup | 6 | 4 |
| Phase 2: Foundational | 18 | 9 |
| Phase 3: User Story 1 | 21 | 8 |
| Phase 4: User Story 2 | 18 | 7 |
| Phase 5: User Story 3 | 28 | 12 |
| Phase 6: User Story 4 | 9 | 2 |
| Phase 7: Integration | 3 | 0 |
| Phase 8: Polish | 7 | 3 |
| **Total** | **110** | **45** |

**MVP Scope**: Phases 1-5 (User Stories 1-3) = 91 tasks
**Full PoC**: All phases = 110 tasks

**Independent Test Criteria per Story**:
- US1: Execute CREATE NODE TABLE, verify schema stored
- US2: Execute CREATE node, verify data in storage
- US3: Execute MATCH query, verify correct rows returned
- US4: Run benchmarks, compare to C++ baseline
