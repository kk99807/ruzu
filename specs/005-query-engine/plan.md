# Implementation Plan: Query Engine with DataFusion Integration

**Branch**: `005-query-engine` | **Date**: 2025-12-07 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/005-query-engine/spec.md`

## Summary

Phase 2 Query Engine implementation integrating Apache DataFusion for vectorized query execution, graph-specific operators (NodeScan, RelScan, Extend, PathExpand), query optimization (filter/projection pushdown), and aggregation functions (COUNT, SUM, MIN, MAX, AVG). This phase transforms ruzu from a basic storage engine into a full query processing system capable of efficient graph traversals and analytical queries.

## Technical Context

**Language/Version**: Rust 1.75+ (stable, 2021 edition)
**Primary Dependencies**:
- Apache Arrow (arrow 53+) - Columnar data format
- Apache DataFusion (datafusion 44+) - Query execution engine
- pest (existing) - Cypher parser
- memmap2, parking_lot, crossbeam (existing) - Storage layer

**Storage**: Custom page-based format with 4KB pages, WAL, buffer pool (Phase 1 complete)
**Testing**: cargo test, criterion benchmarks, property-based testing (proptest)
**Target Platform**: Windows x86_64, Linux x86_64/aarch64, macOS x86_64/aarch64
**Project Type**: Single Rust library crate
**Performance Goals**:
- 1-hop traversals: <100ms for 100K nodes, 500K edges
- 2-hop traversals: <1s for same dataset
- Aggregations: <500ms for 1M rows
- Filter pushdown: 50%+ improvement on selective queries

**Constraints**:
- Memory: 2x result set size max during execution
- Single-threaded execution for MVP (parallel deferred to Phase 4)
- Batch size: 2048 rows (configurable)

**Scale/Scope**:
- Target dataset: 100K-1M nodes, 500K-5M edges
- Cypher subset: MATCH, WHERE, RETURN, ORDER BY, LIMIT, SKIP
- 7 user stories, ~15-20 physical operators

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Evidence |
|-----------|--------|----------|
| **I. Port-First** | PASS | DataFusion integration follows feasibility assessment recommendation; KuzuDB C++ architecture at c:/dev/kuzu referenced for graph operators (binder, planner, processor patterns) |
| **II. TDD (Red-Green-Refactor)** | PASS | Test-first approach for all operators; contract tests for API, integration tests for query pipelines |
| **III. Benchmarking** | PASS | Success criteria SC-001 through SC-007 define measurable performance targets; criterion benchmarks exist |
| **IV. Rust Best Practices** | PASS | Using Arrow/DataFusion (approved ecosystem libraries); clippy/fmt requirements continue |
| **V. Safety First** | PASS | Correctness before optimization; simple algorithms first (hash join before cost-based optimization) |

**Pre-Design Gate**: PASSED - All principles satisfied.

## Project Structure

### Documentation (this feature)

```text
specs/005-query-engine/
├── plan.md              # This file
├── research.md          # Phase 0: DataFusion integration research
├── data-model.md        # Phase 1: Logical/Physical operators, Binder types
├── quickstart.md        # Phase 1: Getting started guide
├── contracts/           # Phase 1: API contracts
│   ├── binder.md        # Binder API contract
│   ├── planner.md       # Planner API contract
│   └── executor.md      # Executor API contract
└── tasks.md             # Phase 2 output (/speckit.tasks command)
```

### Source Code (repository root)

```text
src/
├── lib.rs               # Database entry point (existing)
├── binder/              # NEW: Semantic analysis
│   ├── mod.rs           # Binder main entry
│   ├── expression.rs    # Expression binding
│   ├── query_graph.rs   # Bound query graph
│   └── scope.rs         # Variable scoping
├── planner/             # NEW: Query planning
│   ├── mod.rs           # Planner main entry
│   ├── logical_plan.rs  # Logical operators
│   ├── physical_plan.rs # Physical operators
│   ├── optimizer/       # Optimization rules
│   │   ├── mod.rs
│   │   ├── filter_pushdown.rs
│   │   └── projection_pushdown.rs
│   └── cost.rs          # Cost estimation (heuristic)
├── executor/            # EXTEND: Execution engine (Phase 1 exists)
│   ├── mod.rs           # Executor orchestration
│   ├── scan.rs          # NodeScan, RelScan (extend existing)
│   ├── filter.rs        # Filter operator (extend existing)
│   ├── project.rs       # Project operator (extend existing)
│   ├── hash_join.rs     # NEW: Hash join operator
│   ├── aggregate.rs     # NEW: Aggregation operators
│   ├── sort.rs          # NEW: Sort operator
│   ├── limit.rs         # NEW: Limit/Skip operator
│   ├── extend.rs        # NEW: Relationship extend operator
│   ├── path_expand.rs   # NEW: Variable-length path expansion
│   └── vectorized/      # NEW: Vectorized execution
│       ├── mod.rs
│       ├── batch.rs     # RecordBatch wrapper
│       └── evaluator.rs # Expression evaluator
├── datafusion/          # NEW: DataFusion integration
│   ├── mod.rs           # Module root
│   ├── table_provider.rs# TableProvider for nodes/edges
│   ├── graph_operators.rs # Custom graph physical operators
│   └── cypher_to_df.rs  # Cypher to DataFusion translation
├── parser/              # EXTEND: Add new Cypher clauses
│   ├── mod.rs
│   ├── ast.rs           # Extend AST for ORDER BY, aggregations
│   └── grammar.rs       # Extend pest grammar
├── catalog/             # Existing
├── storage/             # Existing (Phase 1)
├── types/               # EXTEND: Add FLOAT, DOUBLE, DATE, TIMESTAMP
│   ├── mod.rs
│   └── value.rs
└── error.rs             # Existing

tests/
├── contract/
│   ├── test_binder_contract.rs
│   ├── test_planner_contract.rs
│   └── test_executor_contract.rs
├── integration/
│   ├── test_query_pipeline.rs
│   ├── test_aggregations.rs
│   ├── test_multi_hop.rs
│   └── test_optimization.rs
└── unit/
    ├── test_logical_operators.rs
    ├── test_physical_operators.rs
    └── test_expression_eval.rs

benches/
├── query_benchmark.rs   # NEW: Query execution benchmarks
└── ... (existing benchmarks)
```

**Structure Decision**: Single project layout maintained. New modules (`binder`, `planner`, `datafusion`) added alongside existing structure. Executor module extended with new operators.

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| DataFusion dependency (~large) | Provides battle-tested execution engine, optimizer, Arrow integration | Building from scratch would take 12-16 weeks vs 4-6 weeks with DataFusion (per feasibility assessment) |
| Deferred parallel execution | Single-threaded for MVP | Multi-threaded adds concurrency complexity; correctness first (Principle V) |

---

## Architecture Overview

### Query Pipeline Flow

```
┌─────────────────────────────────────────────────────────────┐
│                     Cypher Query                            │
│  MATCH (p:Person)-[:KNOWS]->(f) WHERE p.age > 25           │
│  RETURN p.name, COUNT(f) ORDER BY p.name LIMIT 10          │
└────────────────────────────┬────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────┐
│                   1. Parser (pest)                          │
│  Parse Cypher → AST with pattern, filter, aggregation      │
└────────────────────────────┬────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────┐
│                   2. Binder                                 │
│  - Resolve table/column names against catalog              │
│  - Type checking and inference                             │
│  - Build QueryGraph (bound representation)                 │
└────────────────────────────┬────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────┐
│                   3. Planner                                │
│  - Generate LogicalPlan from QueryGraph                    │
│  - Apply optimization rules (filter/projection pushdown)   │
│  - Translate to PhysicalPlan                               │
└────────────────────────────┬────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────┐
│                   4. Executor                               │
│  - DataFusion execution context                            │
│  - Custom graph operators (Extend, PathExpand)             │
│  - Vectorized processing (2048-row batches)                │
└────────────────────────────┬────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────┐
│                   5. Results                                │
│  Arrow RecordBatches → QueryResult                          │
└─────────────────────────────────────────────────────────────┘
```

### DataFusion Integration Strategy

**Approach: Partial Integration** (per feasibility assessment recommendation)

1. **Use DataFusion for**:
   - Physical operators: Filter, Project, HashJoin, Aggregate, Sort, Limit
   - Expression evaluation framework
   - Arrow RecordBatch handling
   - Optimizer passes (filter/projection pushdown)

2. **Build custom**:
   - Cypher parser (pest, already exists)
   - Binder for graph semantics
   - Custom TableProvider for ruzu storage
   - Graph-specific operators: NodeScan, RelScan, Extend, PathExpand
   - Cypher-to-LogicalPlan translation

3. **Integration Points**:
   - `TableProvider` trait: Expose NodeTable/RelTable to DataFusion
   - `ExecutionPlan` trait: Custom graph operators
   - `OptimizerRule` trait: Graph-aware optimizations

### Operator Hierarchy

**Logical Operators** (what to compute):
```
LogicalPlan
├── Scan
│   ├── NodeScan(table, filter, projection)
│   └── RelScan(table, filter, projection)
├── Extend(input, rel_type, direction)
├── PathExpand(input, rel_type, min_hops, max_hops)
├── Filter(input, predicate)
├── Project(input, expressions)
├── HashJoin(left, right, keys)
├── Aggregate(input, group_by, aggregates)
├── Sort(input, order_by)
├── Limit(input, skip, limit)
└── Union(inputs)
```

**Physical Operators** (how to compute):
```
PhysicalPlan
├── NodeScanExec(table_provider, filters)
├── RelScanExec(table_provider, filters)
├── ExtendExec(input, csr_index, direction)
├── PathExpandExec(input, csr_index, bounds)
├── FilterExec(input, predicate)  [DataFusion]
├── ProjectionExec(input, exprs)  [DataFusion]
├── HashJoinExec(left, right)     [DataFusion]
├── AggregateExec(input, mode)    [DataFusion]
├── SortExec(input, exprs)        [DataFusion]
└── GlobalLimitExec(input, skip, fetch) [DataFusion]
```

### Key Data Structures

**From KuzuDB Reference** (c:/dev/kuzu):

1. **ValueVector** → Arrow Array
   - Columnar batch of 2048 elements
   - Null bitmap for NULL handling
   - Type-specific buffers

2. **DataChunk** → Arrow RecordBatch
   - Multiple columns (Arrays)
   - Shared selection vector
   - Batch processing unit

3. **ResultSet** → Vec<RecordBatch>
   - Collection of batches
   - Lazy evaluation

4. **Expression** (from binder/expression/):
   - Tree structure: ExpressionType + children
   - Unique name for hashing
   - Data type for type inference

5. **Schema** (from planner/operator/schema.h):
   - Factorization groups (graph-specific)
   - Column metadata
   - Data flow tracking

---

## Implementation Phases

### Phase 0: Research & Prototyping (1-2 weeks)
- DataFusion TableProvider prototype
- Cypher-to-LogicalPlan translation POC
- Vectorized execution baseline

### Phase 1: Core Pipeline (3-4 weeks)
- Binder implementation
- Basic planner (no optimization)
- NodeScan/Filter/Project with DataFusion
- Type system extensions

### Phase 2: Graph Operators (2-3 weeks)
- Extend operator (single-hop)
- HashJoin integration
- RelScan operator
- CSR index access

### Phase 3: Aggregation & Sorting (2 weeks)
- COUNT, SUM, MIN, MAX, AVG
- GROUP BY support
- ORDER BY, LIMIT, SKIP

### Phase 4: Optimization (1-2 weeks)
- Filter pushdown
- Projection pushdown
- Predicate simplification
- EXPLAIN output

### Phase 5: Multi-Hop & Polish (2 weeks)
- PathExpand operator
- Cycle detection
- Performance tuning
- Documentation

---

## Dependencies

**New Cargo Dependencies**:
```toml
# Query Engine (Phase 2)
arrow = "53"
datafusion = "44"
async-trait = "0.1"
tokio = { version = "1", features = ["rt-multi-thread"] }
```

**Existing Dependencies** (continue using):
- pest, pest_derive (parser)
- memmap2 (storage)
- parking_lot (synchronization)
- serde, bincode (serialization)
- criterion (benchmarks)

---

## Risk Mitigation

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| DataFusion API changes | Medium | Medium | Pin to version 44+; monitor changelog |
| Performance regression | Medium | High | Continuous benchmarking; criterion baselines |
| Graph operator complexity | Medium | Medium | Reference KuzuDB C++ implementation |
| Memory usage | Low | Medium | Arrow's zero-copy; streaming execution |

---

## Success Metrics

From spec.md Success Criteria:

| ID | Metric | Target | Measurement |
|----|--------|--------|-------------|
| SC-001 | 1-hop traversal latency | <100ms | Benchmark with 100K nodes, 500K edges |
| SC-002 | 2-hop traversal latency | <1s | Same dataset |
| SC-003 | Aggregation throughput | <500ms for 1M rows | COUNT/SUM/AVG benchmark |
| SC-004 | Filter pushdown improvement | 50%+ | Before/after comparison |
| SC-005 | Regression tests | 100% pass | All Phase 0/1 tests |
| SC-006 | Memory efficiency | 2x result set | Memory profiling |
| SC-007 | EXPLAIN accuracy | Correct plan | Manual verification |
