# Tasks: Add Additional Datatypes

**Input**: Design documents from `/specs/006-add-datatypes/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/type-system.md

**Tests**: TDD approach specified in plan.md (Constitution Check II). Tests are included per layer.

**Organization**: Tasks grouped by user story (US1–US4) to enable independent implementation and testing.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2)
- Include exact file paths in descriptions

---

## Phase 1: Setup

**Purpose**: Verify baseline and prepare branch

- [X] T001 Verify all 440 existing tests pass with `cargo test` and `cargo clippy --all-targets --all-features -- -D warnings`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Grammar and AST changes that ALL user stories depend on. These must complete before any story-specific work.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

### Grammar Changes

- [X] T002 Add `^"FLOAT64"` and `^"BOOL"` to the `data_type` rule in `src/parser/grammar.pest` (line ~39)
- [X] T003 Add `float_literal` rule to `src/parser/grammar.pest`: `float_literal = @{ "-"? ~ ASCII_DIGIT* ~ "." ~ ASCII_DIGIT+ }`
- [X] T004 Add `float_literal` and `bool_literal` to the `literal` rule in `src/parser/grammar.pest` — ensure `float_literal` comes before `integer_literal` in the ordered choice

### AST Changes

- [X] T005 Add `Float64(f64)` and `Bool(bool)` variants to the `Literal` enum in `src/parser/ast.rs`

### Parser Builder Changes

- [X] T006 Update `build_literal()` in `src/parser/grammar.rs` to handle `float_literal` tokens — parse with `str::parse::<f64>()`, reject NaN/Infinity via `f64::is_finite()`
- [X] T007 Update `build_literal()` in `src/parser/grammar.rs` to handle `bool_literal` tokens — case-insensitive match on "true"/"false"

### DDL Execution Changes

- [X] T008 [P] Update `execute_create_node_table()` in `src/lib.rs` to recognize `"FLOAT64"` and `"BOOL"` type strings and map to `DataType::Float64` / `DataType::Bool`
- [X] T009 [P] Update `execute_create_rel_table()` in `src/lib.rs` to recognize `"FLOAT64"` and `"BOOL"` type strings and map to `DataType::Float64` / `DataType::Bool`

### CSV Bool Parsing Alignment

- [X] T010 Tighten bool parsing in `src/storage/csv/node_loader.rs` to accept only case-insensitive `true`/`false` — remove support for `1`/`0`/`yes`/`no`/`t`/`f` per research decision R4

**Checkpoint**: Foundation ready — grammar recognizes FLOAT64/BOOL types and literals, AST represents them, parser builds them, DDL creates tables with them, CSV bool parsing aligned to spec.

---

## Phase 3: User Story 1 — Define Tables with FLOAT64 Columns (Priority: P1) 🎯 MVP

**Goal**: Users can create node tables with FLOAT64 columns, insert float values, query with comparisons, import via CSV, and persist across restarts.

**Independent Test**: Create a node table with a FLOAT64 column, insert float values, query with `WHERE p.price > 10.0`, verify results. Close and reopen database, verify data persists.

### Tests for User Story 1

- [X] T011 [P] [US1] Add contract tests for C-DDL-01 (FLOAT64 in CREATE NODE TABLE) and C-LIT-01 (float literals) in `tests/contract_tests.rs`
- [X] T012 [P] [US1] Add contract tests for C-DML-01 (CREATE node with FLOAT64), C-DML-03 (integer value for FLOAT64 column), C-LIT-04 (NaN rejected), C-LIT-05 (Infinity rejected) in `tests/contract_tests.rs`
- [X] T013 [P] [US1] Add contract tests for C-QRY-01 (float comparison operators), C-QRY-02 (float comparison with integer literal), C-QRY-05 (ORDER BY FLOAT64) in `tests/contract_tests.rs`
- [X] T014 [P] [US1] Add contract tests for C-CSV-01 (FLOAT64 from CSV), C-CSV-05 (invalid FLOAT64 in CSV rejected) in `tests/contract_tests.rs`
- [X] T015 [P] [US1] Add contract test for C-PER-01 (FLOAT64 data survives restart) in `tests/contract_tests.rs`
- [X] T016 [P] [US1] Add integration test for end-to-end FLOAT64 workflow (create table, insert, query, CSV import, persistence) in `tests/integration_tests.rs`

### Implementation for User Story 1

- [X] T017 [US1] Handle `Literal::Float64` in literal-to-Value conversion for CREATE node execution in `src/lib.rs` — convert `Literal::Float64(f)` to `Value::Float64(f)`
- [X] T018 [US1] Handle `Literal::Float64` in WHERE clause evaluation in `src/executor/mod.rs` — convert float literal to `Value::Float64` for comparison
- [X] T019 [US1] Implement Int64-to-Float64 promotion in `src/executor/mod.rs` — when comparing `Value::Int64` against `Value::Float64`, promote Int64 to Float64 before comparison
- [X] T020 [US1] Handle integer literal for FLOAT64 column in CREATE node execution in `src/lib.rs` — when column type is FLOAT64 and literal is Int64, promote to Float64

**Checkpoint**: User Story 1 fully functional — FLOAT64 columns work end-to-end including DDL, DML, queries, CSV import, and persistence.

---

## Phase 4: User Story 2 — Define Tables with BOOL Columns (Priority: P1)

**Goal**: Users can create node tables with BOOL columns, insert true/false values, query with equality comparisons, import via CSV, and persist across restarts.

**Independent Test**: Create a node table with a BOOL column, insert true/false values, query with `WHERE f.enabled = true`, verify results. Close and reopen database, verify data persists.

### Tests for User Story 2

- [X] T021 [P] [US2] Add contract tests for C-DDL-02 (BOOL in CREATE NODE TABLE), C-DDL-03 (all four types in single table), C-LIT-02 (bool literals), C-LIT-03 (integer literal unchanged) in `tests/contract_tests.rs`
- [X] T022 [P] [US2] Add contract tests for C-DML-02 (CREATE node with BOOL) in `tests/contract_tests.rs`
- [X] T023 [P] [US2] Add contract tests for C-QRY-03 (bool equality), C-QRY-04 (bool inequality) in `tests/contract_tests.rs`
- [X] T024 [P] [US2] Add contract tests for C-CSV-02 (BOOL from CSV), C-CSV-03 (case-insensitive BOOL in CSV), C-CSV-04 (invalid BOOL in CSV rejected) in `tests/contract_tests.rs`
- [X] T025 [P] [US2] Add contract test for C-PER-02 (BOOL data survives restart) in `tests/contract_tests.rs`
- [X] T026 [P] [US2] Add integration test for end-to-end BOOL workflow (create table, insert, query, CSV import, persistence) in `tests/integration_tests.rs`

### Implementation for User Story 2

- [X] T027 [US2] Handle `Literal::Bool` in literal-to-Value conversion for CREATE node execution in `src/lib.rs` — convert `Literal::Bool(b)` to `Value::Bool(b)`
- [X] T028 [US2] Handle `Literal::Bool` in WHERE clause evaluation in `src/executor/mod.rs` — convert bool literal to `Value::Bool` for comparison

**Checkpoint**: User Story 2 fully functional — BOOL columns work end-to-end including DDL, DML, queries, CSV import, and persistence.

---

## Phase 5: User Story 3 — Use FLOAT64 and BOOL in Relationship Properties (Priority: P2)

**Goal**: Users can define relationship tables with FLOAT64 and BOOL properties, insert relationships with those properties, and query them back.

**Independent Test**: Create a relationship table with `weight FLOAT64` and `active BOOL`, insert a relationship with values, query it back. Import relationships via CSV.

### Tests for User Story 3

- [X] T029 [P] [US3] Add contract test for C-DDL-04 (FLOAT64/BOOL in CREATE REL TABLE) in `tests/contract_tests.rs`
- [X] T030 [P] [US3] Add integration test for relationship table with FLOAT64/BOOL properties (create, insert, query, CSV import) in `tests/integration_tests.rs`

### Implementation for User Story 3

- [X] T031 [US3] Verify relationship table creation with FLOAT64/BOOL properties works end-to-end in `src/lib.rs` — DDL execution already updated in T009, verify DML and query paths handle rel properties

**Checkpoint**: User Story 3 fully functional — relationship tables support FLOAT64 and BOOL properties.

---

## Phase 6: User Story 4 — Mixed-Type Queries (Priority: P2)

**Goal**: Users can query tables combining STRING, INT64, FLOAT64, and BOOL columns, filtering on one type while returning others.

**Independent Test**: Create a table with all four column types, insert data, run queries filtering on FLOAT64 while returning BOOL, and vice versa.

### Tests for User Story 4

- [X] T032 [P] [US4] Add contract tests for C-AGG-01 (COUNT on FLOAT64), C-AGG-02 (MIN/MAX on FLOAT64), C-AGG-03 (COUNT on BOOL) in `tests/contract_tests.rs`
- [X] T033 [P] [US4] Add integration test for mixed-type queries (table with all four types, cross-type filtering and returning) in `tests/integration_tests.rs`

### Implementation for User Story 4

- [X] T034 [US4] Verify mixed-type queries work end-to-end — all four datatypes in single table, filtering on one type returning another in `src/executor/mod.rs` and `src/lib.rs`
- [X] T035 [US4] Verify aggregation functions (COUNT, MIN, MAX) work with FLOAT64 and BOOL columns — check `src/executor/mod.rs` handles new Value variants in aggregation paths

**Checkpoint**: User Story 4 fully functional — mixed-type queries and aggregations work correctly.

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: Final validation, regression checks, and cleanup

- [X] T036 Run full test suite `cargo test` and verify all tests pass (existing + new)
- [X] T037 Run `cargo clippy --all-targets --all-features -- -D warnings` and fix any warnings
- [X] T038 Run existing benchmarks (`cargo bench --bench csv_benchmark`, `cargo bench --bench storage_benchmark`) and verify no regression (< 5% threshold)
- [X] T039 Run quickstart.md smoke test scenario manually or as integration test

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — start immediately
- **Foundational (Phase 2)**: Depends on Phase 1 — BLOCKS all user stories
- **User Story 1 (Phase 3)**: Depends on Phase 2 completion
- **User Story 2 (Phase 4)**: Depends on Phase 2 completion
- **User Story 3 (Phase 5)**: Depends on Phase 2 completion (and benefits from US1+US2 being done first)
- **User Story 4 (Phase 6)**: Depends on US1 and US2 being complete (needs all four types working)
- **Polish (Phase 7)**: Depends on all user stories being complete

### User Story Dependencies

- **US1 (P1)**: Independent after Phase 2. No dependencies on other stories.
- **US2 (P1)**: Independent after Phase 2. No dependencies on other stories. Can run in parallel with US1.
- **US3 (P2)**: Technically independent after Phase 2, but best done after US1+US2 since it combines both new types.
- **US4 (P2)**: Depends on US1+US2 being complete (needs FLOAT64 and BOOL both working for mixed-type tests).

### Within Each User Story

- Tests MUST be written first and FAIL before implementation (TDD)
- Contract tests → implementation → integration tests passing
- Core literal handling → query evaluation → type promotion

### Parallel Opportunities

- **Phase 2**: T008 and T009 can run in parallel (different functions in same file)
- **Phase 2**: T002–T004 are sequential (same file, `grammar.pest`); T005 parallel with grammar changes (different file)
- **Phase 3 tests**: T011–T016 can all run in parallel (different test functions)
- **Phase 4 tests**: T021–T026 can all run in parallel
- **Phase 3+4**: US1 and US2 can be worked on in parallel after Phase 2
- **Phase 5+6 tests**: T029–T030 and T032–T033 can run in parallel

---

## Parallel Example: User Story 1

```bash
# Launch all US1 contract tests in parallel:
Task: T011 "Contract test for FLOAT64 DDL and literals"
Task: T012 "Contract test for FLOAT64 DML and error cases"
Task: T013 "Contract test for FLOAT64 query comparisons"
Task: T014 "Contract test for FLOAT64 CSV import"
Task: T015 "Contract test for FLOAT64 persistence"

# Then implement sequentially:
Task: T017 "Literal::Float64 in CREATE node"
Task: T018 "Literal::Float64 in WHERE clause"
Task: T019 "Int64-to-Float64 promotion"
Task: T020 "Integer literal for FLOAT64 column"
```

---

## Parallel Example: User Story 1 + User Story 2 (concurrent)

```bash
# After Phase 2 completes, both stories can start simultaneously:
# Developer A: US1 (T011–T020)
# Developer B: US2 (T021–T028)
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup (verify baseline)
2. Complete Phase 2: Foundational (grammar, AST, parser, DDL, CSV alignment)
3. Complete Phase 3: User Story 1 (FLOAT64 end-to-end)
4. **STOP and VALIDATE**: Test FLOAT64 independently — create table, insert, query, CSV, persistence
5. Deploy/demo if ready

### Incremental Delivery

1. Setup + Foundational → Foundation ready
2. Add US1 (FLOAT64) → Test independently → MVP!
3. Add US2 (BOOL) → Test independently → Both core types available
4. Add US3 (Rel properties) → Test independently → Graph modeling complete
5. Add US4 (Mixed queries) → Test independently → Full feature complete
6. Polish → Regression check → Feature done

---

## Notes

- ~6 files modified, ~200 lines changed (per plan.md estimate)
- No new dependencies required
- Storage/serialization/WAL already handle Float64/Bool — no persistence code changes needed
- Key risk: grammar ordering in `literal` rule — `float_literal` must precede `integer_literal`
- CSV bool parsing tightened per research decision R4 — users with `1`/`0` CSVs will get errors (acceptable since BOOL columns are new)
