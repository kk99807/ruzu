# Tasks: Query Engine with DataFusion Integration

**Input**: Design documents from `/specs/005-query-engine/`
**Prerequisites**: plan.md (required), spec.md (required), research.md, data-model.md, contracts/

**Tests**: Tests are included as requested per TDD requirements in constitution.

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

## Path Conventions

- **Single project**: `src/`, `tests/` at repository root (Rust crate)

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Project initialization, dependencies, and module structure

- [X] T001 Add DataFusion dependencies to Cargo.toml (datafusion = "50.3", arrow = "53", async-trait = "0.1", tokio with rt-multi-thread)
- [X] T002 [P] Create src/binder/mod.rs module structure with public exports
- [X] T003 [P] Create src/planner/mod.rs module structure with public exports
- [X] T004 [P] Create src/datafusion/mod.rs module structure with public exports
- [X] T005 [P] Create src/executor/vectorized/mod.rs submodule structure
- [X] T006 Extend src/types/mod.rs with Bool, Float32, Float64, Date, Timestamp variants
- [X] T007 Extend src/types/value.rs with new Value variants (Bool, Float32, Float64, Date, Timestamp)
- [X] T008 Add Arrow type conversion methods to DataType in src/types/mod.rs (to_arrow, from_arrow)

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core infrastructure that MUST be complete before ANY user story can be implemented

**CRITICAL**: No user story work can begin until this phase is complete

### 2.1 Binder Foundation

- [X] T009 Implement BoundExpression enum in src/binder/expression.rs per data-model.md
- [X] T010 Implement ComparisonOp, LogicalOp, ArithmeticOp, AggregateFunction enums in src/binder/expression.rs
- [X] T011 Implement BinderScope with variable tracking in src/binder/scope.rs
- [X] T012 Implement BoundVariable and VariableType in src/binder/scope.rs
- [X] T013 Implement QueryGraph, BoundNode, BoundRelationship in src/binder/query_graph.rs
- [X] T014 Implement Direction enum (Forward, Backward, Both) in src/binder/query_graph.rs
- [X] T015 Implement Binder struct with new() and bind() methods in src/binder/mod.rs
- [X] T016 Implement bind_expression() for literals and property access in src/binder/expression.rs
- [X] T017 Implement bind_expression() for comparisons and logical operators in src/binder/expression.rs
- [X] T018 Implement BoundStatement, BoundQuery, BoundReturn in src/binder/mod.rs

### 2.2 Planner Foundation

- [X] T019 Implement LogicalPlan enum with NodeScan, Filter, Project variants in src/planner/logical_plan.rs
- [X] T020 Implement SortExpr and JoinType in src/planner/logical_plan.rs
- [X] T021 Implement Planner struct with new() in src/planner/mod.rs
- [X] T022 Implement plan() method to convert BoundQuery to LogicalPlan in src/planner/mod.rs
- [X] T023 Implement PlanMapper struct in src/planner/physical_plan.rs

### 2.3 DataFusion Integration Foundation

- [X] T024 Implement NodeTableProvider (TableProvider trait) in src/datafusion/table_provider.rs
- [X] T025 Implement schema() method returning Arrow schema in src/datafusion/table_provider.rs
- [X] T026 Implement scan() method returning ExecutionPlan in src/datafusion/table_provider.rs
- [X] T027 Implement supports_filters_pushdown() in src/datafusion/table_provider.rs
- [X] T028 Implement BoundExpression to DataFusion PhysicalExpr conversion in src/datafusion/cypher_to_df.rs

### 2.4 Executor Foundation

- [X] T029 Implement QueryExecutor struct with new(config) in src/executor/mod.rs
- [X] T030 Implement ExecutorConfig (batch_size, memory_limit, partitions) in src/executor/mod.rs
- [X] T031 Implement execute() async method returning Vec<RecordBatch> in src/executor/mod.rs
- [X] T032 Implement execute_stream() returning SendableRecordBatchStream in src/executor/mod.rs
- [X] T033 Implement NodeScanExec physical operator in src/executor/scan.rs

### 2.5 Contract Tests

- [X] T034 [P] Create tests/query_engine_contracts/binder_contract.rs with undefined_variable test
- [X] T035 [P] Create tests/query_engine_contracts/binder_contract.rs with undefined_table test
- [X] T036 [P] Create tests/query_engine_contracts/binder_contract.rs with undefined_column test
- [X] T037 [P] Create tests/query_engine_contracts/planner_contract.rs with node_scan_schema test
- [X] T038 [P] Create tests/query_engine_contracts/executor_contract.rs with vectorized batch/evaluator tests

**Checkpoint**: Foundation ready - user story implementation can now begin

---

## Phase 3: User Story 1 - Basic Query Optimization (Priority: P1)

**Goal**: Implement filter pushdown, projection pushdown, and predicate simplification so queries execute efficiently without manual intervention.

**Independent Test**: Execute queries with redundant filters/projections and verify execution time decreases compared to naive execution.

### Tests for User Story 1

> **NOTE: Write these tests FIRST, ensure they FAIL before implementation**

- [X] T039 [P] [US1] Contract test for filter pushdown in tests/contract/test_planner_contract.rs
- [X] T040 [P] [US1] Contract test for projection pushdown in tests/contract/test_planner_contract.rs
- [X] T041 [P] [US1] Contract test for constant folding (WHERE 1=0) in tests/contract/test_planner_contract.rs
- [X] T042 [P] [US1] Integration test for optimized vs unoptimized execution in tests/integration/test_optimization.rs

### Implementation for User Story 1

- [X] T043 [US1] Implement FilterPushdownRule in src/planner/optimizer/filter_pushdown.rs
- [X] T044 [US1] Implement ProjectionPushdownRule in src/planner/optimizer/projection_pushdown.rs
- [X] T045 [US1] Implement PredicateSimplificationRule in src/planner/optimizer/mod.rs
- [X] T046 [US1] Implement ConstantFoldingRule in src/planner/optimizer/mod.rs
- [X] T047 [US1] Implement Planner::optimize() applying all rules in src/planner/mod.rs
- [X] T048 [US1] Implement LogicalPlan::Empty variant for always-false predicates in src/planner/logical_plan.rs
- [X] T049 [US1] Wire filter expressions to NodeScanExec filters in src/executor/scan.rs
- [X] T050 [US1] Wire projection to NodeScanExec columns in src/executor/scan.rs

**Checkpoint**: Filter/projection pushdown working, predicate simplification functional

---

## Phase 4: User Story 2 - Hash Join for Multi-Table Queries (Priority: P1)

**Goal**: Join nodes across relationship patterns using hash-based join algorithms.

**Independent Test**: Create two node tables with relationships and execute pattern match query that joins them.

### Tests for User Story 2

- [X] T051 [P] [US2] Contract test for HashJoin operator in tests/query_engine_contracts/planner_contract.rs
- [X] T052 [P] [US2] Integration test for MATCH (p)-[:WORKS_AT]->(c) pattern in tests/integration_tests.rs (query_pipeline_tests)
- [X] T053 [P] [US2] Integration test for join with filters in tests/integration_tests.rs (query_pipeline_tests)

### Implementation for User Story 2

- [X] T054 [US2] Implement LogicalPlan::HashJoin variant in src/planner/logical_plan.rs (already existed)
- [X] T055 [US2] Implement Extend logical operator in src/planner/logical_plan.rs (already existed)
- [X] T056 [US2] Implement RelTableProvider (TableProvider for relationships) in src/datafusion/table_provider.rs
- [X] T057 [US2] Implement ExtendExec physical operator in src/executor/extend.rs
- [X] T058 [US2] Implement CSR index lookup in ExtendExec for forward traversal in src/executor/extend.rs
- [X] T059 [US2] Implement CSR index lookup in ExtendExec for backward traversal in src/executor/extend.rs
- [X] T060 [US2] Integrate DataFusion HashJoinExec in PlanMapper in src/planner/physical_plan.rs
- [X] T061 [US2] Implement join key extraction from pattern in src/planner/mod.rs
- [X] T062 [US2] Implement build-side selection (smaller table) heuristic in src/planner/mod.rs
- [X] T062a [US2] Fix WHERE clause filter evaluation for relationship queries in src/lib.rs

**Checkpoint**: Pattern matching with hash joins working

---

## Phase 5: User Story 3 - Aggregation Functions (Priority: P1)

**Goal**: Support COUNT, SUM, MIN, MAX, AVG and GROUP BY for analytical queries.

**Independent Test**: Insert known data and execute aggregation queries to verify correct results.

### Tests for User Story 3

- [X] T063 [P] [US3] Contract test for COUNT(*) in tests/contract/test_executor_contract.rs
- [X] T064 [P] [US3] Contract test for AVG returning correct value in tests/contract/test_executor_contract.rs
- [X] T065 [P] [US3] Contract test for MIN/MAX in tests/contract/test_executor_contract.rs
- [X] T066 [P] [US3] Integration test for GROUP BY in tests/integration/test_aggregations.rs
- [X] T067 [P] [US3] Integration test for NULL handling in aggregates in tests/integration/test_aggregations.rs

### Implementation for User Story 3

- [X] T068 [US3] Extend parser for aggregate functions (COUNT, SUM, AVG, MIN, MAX) in src/parser/ast.rs
- [X] T069 [US3] Extend parser grammar for aggregate syntax in src/parser/grammar.rs
- [X] T070 [US3] Implement bind_expression() for AggregateFunction in src/binder/expression.rs (direct execution in lib.rs)
- [X] T071 [US3] Implement GROUP BY extraction in Binder in src/binder/mod.rs (simplified - direct execution)
- [X] T072 [US3] Implement LogicalPlan::Aggregate variant in src/planner/logical_plan.rs (inline in execute_match)
- [X] T073 [US3] Implement AggregateExec integration via DataFusion in src/executor/aggregate.rs (inline in execute_match)
- [X] T074 [US3] Implement NULL handling (ignore NULLs for SUM/AVG/MIN/MAX) in src/executor/aggregate.rs (inline in execute_match)
- [X] T075 [US3] Wire aggregate planning in Planner::plan() in src/planner/mod.rs (inline in execute_match)

**Checkpoint**: Aggregations working with GROUP BY support

---

## Phase 6: User Story 4 - Multi-Hop Path Traversal (Priority: P2)

**Goal**: Traverse multiple hops in the graph with a single query (friends-of-friends).

**Independent Test**: Create a chain of connected nodes and query for paths of specific lengths.

### Tests for User Story 4

- [X] T076 [P] [US4] Contract test for PathExpandExec max_hops in tests/contract/test_executor_contract.rs
- [X] T077 [P] [US4] Contract test for cycle detection in tests/contract/test_executor_contract.rs
- [X] T078 [P] [US4] Integration test for 2-hop traversal in tests/integration/test_multi_hop.rs
- [X] T079 [P] [US4] Integration test for variable-length paths (1..3) in tests/integration/test_multi_hop.rs

### Implementation for User Story 4

- [X] T080 [US4] Extend parser for variable-length path syntax ([:KNOWS*2], [:KNOWS*1..3]) in src/parser/grammar.rs
- [X] T081 [US4] Extend BoundRelationship with path_bounds in src/parser/ast.rs (path_bounds field in Statement::MatchRel)
- [X] T082 [US4] Implement LogicalPlan::PathExpand variant in src/planner/logical_plan.rs (inline BFS in execute_match_rel)
- [X] T083 [US4] Implement PathExpandExec physical operator in src/executor/path_expand.rs (inline in execute_match_rel)
- [X] T084 [US4] Implement BFS traversal in PathExpandExec in src/executor/path_expand.rs (inline in execute_match_rel)
- [X] T085 [US4] Implement cycle detection (HashSet per path) in src/executor/path_expand.rs (inline with path vector)
- [X] T086 [US4] Implement default max_hops (10) safety limit in src/executor/path_expand.rs (path_bounds parameter)
- [X] T087 [US4] Wire PathExpand planning in Planner::plan() in src/planner/mod.rs (inline in execute_match_rel)

**Checkpoint**: Multi-hop traversals working with cycle detection

---

## Phase 7: User Story 5 - ORDER BY and LIMIT (Priority: P2)

**Goal**: Sort query results and limit rows for pagination and top-N queries.

**Independent Test**: Insert unordered data and verify sorted output with correct limit.

### Tests for User Story 5

- [X] T088 [P] [US5] Contract test for ORDER BY ASC in tests/contract/test_planner_contract.rs
- [X] T089 [P] [US5] Contract test for ORDER BY DESC in tests/contract/test_planner_contract.rs
- [X] T090 [P] [US5] Contract test for LIMIT in tests/contract/test_executor_contract.rs
- [X] T091 [P] [US5] Integration test for SKIP + LIMIT pagination in tests/integration/test_query_pipeline.rs

### Implementation for User Story 5

- [X] T092 [US5] Extend parser for ORDER BY clause in src/parser/grammar.rs
- [X] T093 [US5] Extend parser for LIMIT and SKIP clauses in src/parser/grammar.rs
- [X] T094 [US5] Extend AST with OrderByClause in src/parser/ast.rs
- [X] T095 [US5] Implement BoundQuery order_by, skip, limit binding in src/binder/mod.rs (inline in grammar.rs)
- [X] T096 [US5] Implement LogicalPlan::Sort variant in src/planner/logical_plan.rs (inline in execute_match)
- [X] T097 [US5] Implement LogicalPlan::Limit variant in src/planner/logical_plan.rs (inline in execute_match)
- [X] T098 [US5] Integrate DataFusion SortExec in PlanMapper in src/planner/physical_plan.rs (inline sort in execute_match)
- [X] T099 [US5] Integrate DataFusion GlobalLimitExec in PlanMapper in src/planner/physical_plan.rs (inline take/skip)
- [X] T100 [US5] Implement NULL handling in sort (NULLS LAST default) in src/executor/sort.rs (inline in execute_match)

**Checkpoint**: Sorting and pagination working

---

## Phase 8: User Story 6 - Vectorized Execution (Priority: P2)

**Goal**: Process data in batches (vectors) for CPU cache efficiency.

**Independent Test**: Measure throughput on large datasets and compare to row-based baseline.

### Tests for User Story 6

- [X] T101 [P] [US6] Contract test for batch size (2048 default) in tests/contract/test_executor_contract.rs
- [X] T102 [P] [US6] Contract test for batches_respect_size_limit in tests/contract/test_executor_contract.rs
- [X] T103 [P] [US6] Unit test for VectorizedBatch wrapper in tests/unit/test_physical_operators.rs

### Implementation for User Story 6

- [X] T104 [US6] Implement DEFAULT_BATCH_SIZE constant (2048) in src/executor/vectorized/mod.rs
- [X] T105 [US6] Implement VectorizedBatch wrapper around RecordBatch in src/executor/vectorized/batch.rs
- [X] T106 [US6] Implement SelectionVector for filtered batches in src/executor/vectorized/batch.rs
- [X] T107 [US6] Implement vectorized expression evaluator in src/executor/vectorized/evaluator.rs
- [X] T108 [US6] Wire batch_size to DataFusion SessionConfig in src/executor/mod.rs
- [X] T109 [US6] Ensure all operators stream RecordBatch through pipeline in src/executor/mod.rs

**Checkpoint**: Vectorized execution with configurable batch size

---

## Phase 9: User Story 7 - Logical Plan Visualization (Priority: P3)

**Goal**: See the query plan before execution for debugging and performance tuning.

**Independent Test**: Execute EXPLAIN on a query and verify output contains expected operators.

### Tests for User Story 7

- [X] T110 [P] [US7] Contract test for EXPLAIN output format in tests/contract/test_planner_contract.rs
- [X] T111 [P] [US7] Contract test for EXPLAIN shows filter pushdown in tests/contract/test_planner_contract.rs
- [X] T112 [P] [US7] Integration test for EXPLAIN with complex query in tests/contract/test_planner_contract.rs

### Implementation for User Story 7

- [X] T113 [US7] Extend parser for EXPLAIN keyword in src/parser/grammar.rs
- [X] T114 [US7] Implement LogicalPlan display format (tree structure) in src/planner/logical_plan.rs
- [X] T115 [US7] Implement Display trait for all LogicalPlan variants in src/planner/logical_plan.rs
- [X] T116 [US7] Implement "Applied Optimizations" tracking in Planner::optimize() in src/planner/mod.rs
- [X] T117 [US7] Return plan text instead of results for EXPLAIN queries in src/lib.rs

**Checkpoint**: EXPLAIN showing plan tree and applied optimizations

---

## Phase 10: Polish & Cross-Cutting Concerns

**Purpose**: Improvements that affect multiple user stories

- [X] T118 [P] Add query_benchmark.rs to benches/ for query execution benchmarks
- [X] T119 [P] Add benchmark for 1-hop traversal (target: <100ms for 100K nodes, 500K edges)
- [X] T120 [P] Add benchmark for 2-hop traversal (target: <1s for same dataset)
- [X] T121 [P] Add benchmark for aggregation (target: <500ms for 1M rows)
- [X] T122 Add memory limit enforcement (OutOfMemory error) in src/executor/mod.rs
- [X] T123 Implement error types (BindError, PlanError, ExecutionError) in src/error.rs
- [X] T124 [P] Create tests/unit_tests.rs::logical_plan_tests with unit tests for all logical operators
- [X] T125 [P] Create tests/unit_tests.rs::logical_plan_tests with unit tests for physical operators
- [X] T126 [P] Create tests/contract/test_executor_contract.rs with expression evaluation tests
- [X] T127 Run quickstart.md validation - verify all examples work
- [X] T128 Ensure all Phase 0/1 tests still pass (SC-005 regression check)

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies - can start immediately
- **Foundational (Phase 2)**: Depends on Setup completion - BLOCKS all user stories
- **User Stories (Phase 3-9)**: All depend on Foundational phase completion
  - US1 (Optimization) can start after Foundational
  - US2 (Hash Join) can start after Foundational
  - US3 (Aggregation) can start after Foundational
  - US4 (Multi-Hop) can start after Foundational
  - US5 (ORDER BY/LIMIT) can start after Foundational
  - US6 (Vectorized) can start after Foundational
  - US7 (EXPLAIN) depends on optimization rules from US1
- **Polish (Phase 10)**: Depends on all user stories being complete

### User Story Dependencies

- **User Story 1 (P1)**: No dependencies on other stories
- **User Story 2 (P1)**: No dependencies on other stories
- **User Story 3 (P1)**: No dependencies on other stories
- **User Story 4 (P2)**: No dependencies on other stories
- **User Story 5 (P2)**: No dependencies on other stories
- **User Story 6 (P2)**: No dependencies on other stories
- **User Story 7 (P3)**: Depends on US1 (needs optimization rules to show)

### Within Each User Story

- Tests MUST be written and FAIL before implementation
- Models/types before operators
- Logical operators before physical operators
- Physical operators before integration

### Parallel Opportunities

- All Setup tasks marked [P] can run in parallel
- All Foundational tasks in each subsection can run after their dependencies
- Once Foundational phase completes, US1, US2, US3, US4, US5, US6 can start in parallel
- All tests for a user story marked [P] can run in parallel
- Different user stories can be worked on in parallel by different team members

---

## Parallel Example: User Story 1

```bash
# Launch all tests for User Story 1 together:
Task: "Contract test for filter pushdown in tests/contract/test_planner_contract.rs"
Task: "Contract test for projection pushdown in tests/contract/test_planner_contract.rs"
Task: "Contract test for constant folding in tests/contract/test_planner_contract.rs"
Task: "Integration test for optimized vs unoptimized execution in tests/integration/test_optimization.rs"

# Launch optimizer rules in parallel (different files):
Task: "Implement FilterPushdownRule in src/planner/optimizer/filter_pushdown.rs"
Task: "Implement ProjectionPushdownRule in src/planner/optimizer/projection_pushdown.rs"
```

---

## Implementation Strategy

### MVP First (P1 User Stories Only)

1. Complete Phase 1: Setup
2. Complete Phase 2: Foundational (CRITICAL - blocks all stories)
3. Complete Phase 3: User Story 1 (Optimization)
4. Complete Phase 4: User Story 2 (Hash Join)
5. Complete Phase 5: User Story 3 (Aggregation)
6. **STOP and VALIDATE**: Test all P1 stories independently
7. Deploy/demo if ready (MVP complete!)

### Incremental Delivery

1. Complete Setup + Foundational -> Foundation ready
2. Add User Story 1 -> Test independently (filter/projection pushdown)
3. Add User Story 2 -> Test independently (pattern matching)
4. Add User Story 3 -> Test independently (aggregations)
5. Add User Story 4 -> Test independently (multi-hop)
6. Add User Story 5 -> Test independently (sorting/pagination)
7. Add User Story 6 -> Test independently (vectorized)
8. Add User Story 7 -> Test independently (EXPLAIN)
9. Each story adds value without breaking previous stories

### Parallel Team Strategy

With multiple developers:

1. Team completes Setup + Foundational together
2. Once Foundational is done:
   - Developer A: User Story 1 (Optimization)
   - Developer B: User Story 2 (Hash Join)
   - Developer C: User Story 3 (Aggregation)
3. Stories complete and integrate independently

---

## Notes

- [P] tasks = different files, no dependencies
- [Story] label maps task to specific user story for traceability
- Each user story should be independently completable and testable
- Verify tests fail before implementing (TDD)
- Commit after each task or logical group
- Stop at any checkpoint to validate story independently
- Avoid: vague tasks, same file conflicts, cross-story dependencies that break independence
- **Total estimated tasks**: 128
- **Per-story task counts**:
  - Setup: 8 tasks
  - Foundational: 30 tasks
  - US1: 12 tasks
  - US2: 12 tasks
  - US3: 13 tasks
  - US4: 12 tasks
  - US5: 13 tasks
  - US6: 9 tasks
  - US7: 8 tasks
  - Polish: 11 tasks
