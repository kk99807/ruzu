# Feature Specification: Query Engine with DataFusion Integration

**Feature Branch**: `005-query-engine`
**Created**: 2025-12-07
**Status**: Draft
**Input**: User description: "Phase 2: Query Engine. References: README.md, C:\dev\ruzu\docs\feasibility-assessment.md, & KuzuDB checked out at c:/dev/kuzu/."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Basic Query Optimization (Priority: P1)

As a developer, I want my graph queries to be automatically optimized so that common patterns like filter pushdown and projection pruning happen without manual intervention, reducing query execution time.

**Why this priority**: Query optimization is the foundation of a performant query engine. Without it, complex queries will be prohibitively slow, making the database unusable for real-world workloads.

**Independent Test**: Can be fully tested by executing queries with redundant filters and projections, measuring that execution time decreases compared to naive execution.

**Acceptance Scenarios**:

1. **Given** a query `MATCH (p:Person) WHERE p.age > 25 RETURN p.name`, **When** executed, **Then** the filter is pushed down to the scan operator (not applied after full table scan)
2. **Given** a query `MATCH (p:Person) RETURN p.name`, **When** executed, **Then** only the `name` column is read (projection pushdown)
3. **Given** a query with a predicate that is always false (e.g., `WHERE 1 = 0`), **When** the plan is generated, **Then** the optimizer produces an empty result operator

---

### User Story 2 - Hash Join for Multi-Table Queries (Priority: P1)

As a developer, I want to join nodes across relationship patterns so that I can query connected data efficiently using hash-based join algorithms.

**Why this priority**: Joins are essential for graph traversal queries. Without efficient join operators, queries like `MATCH (a)-[:KNOWS]->(b)` cannot be executed performantly.

**Independent Test**: Can be tested by creating two node tables with relationships and executing a pattern match query that joins them.

**Acceptance Scenarios**:

1. **Given** tables `Person` and `Company` with a `WORKS_AT` relationship, **When** I execute `MATCH (p:Person)-[:WORKS_AT]->(c:Company) RETURN p.name, c.name`, **Then** the query returns all matching pairs using hash join
2. **Given** a join query, **When** the build side has fewer unique keys, **Then** the optimizer selects the smaller table for the hash build phase
3. **Given** a join query with additional filters, **When** executed, **Then** filters are applied before the join to minimize the join input size

---

### User Story 3 - Aggregation Functions (Priority: P1)

As a developer, I want to use aggregation functions like COUNT, SUM, MIN, MAX, and AVG so that I can compute summary statistics over my graph data.

**Why this priority**: Aggregations are a core SQL/Cypher capability needed for analytics queries. Without aggregations, users cannot answer questions like "how many users have logged in?" or "what is the average transaction value?"

**Independent Test**: Can be tested by inserting known data and executing aggregation queries to verify correct results.

**Acceptance Scenarios**:

1. **Given** a table with 100 nodes, **When** I execute `MATCH (p:Person) RETURN COUNT(*)`, **Then** the result is 100
2. **Given** a table with ages [20, 30, 40, 50], **When** I execute `MATCH (p:Person) RETURN AVG(p.age)`, **Then** the result is 35.0
3. **Given** a table with values, **When** I execute `MATCH (p:Person) RETURN MIN(p.age), MAX(p.age)`, **Then** the correct minimum and maximum values are returned
4. **Given** a query with GROUP BY, **When** executed on `MATCH (p:Person) RETURN p.city, COUNT(*)`, **Then** results are grouped correctly by city

---

### User Story 4 - Multi-Hop Path Traversal (Priority: P2)

As a developer, I want to traverse multiple hops in the graph with a single query so that I can find indirect connections (e.g., friends-of-friends).

**Why this priority**: Multi-hop traversals are what distinguish graph databases from relational databases. Without this capability, users must execute multiple queries and join results manually.

**Independent Test**: Can be tested by creating a chain of connected nodes and querying for paths of specific lengths.

**Acceptance Scenarios**:

1. **Given** a path A->B->C, **When** I execute `MATCH (a:Person)-[:KNOWS*2]->(c:Person) WHERE a.name = 'Alice' RETURN c.name`, **Then** 'Charlie' is returned (2-hop path)
2. **Given** a graph with cycles, **When** I execute a variable-length path query, **Then** the engine handles cycles without infinite loops
3. **Given** a path query with filters on intermediate nodes, **When** executed, **Then** only paths matching the filter criteria are returned

---

### User Story 5 - ORDER BY and LIMIT (Priority: P2)

As a developer, I want to sort query results and limit the number of rows returned so that I can implement pagination and top-N queries.

**Why this priority**: Sorting and limiting are essential for user interfaces that display paginated results. Without them, applications must load all results into memory for client-side processing.

**Independent Test**: Can be tested by inserting unordered data and verifying sorted output with correct limit.

**Acceptance Scenarios**:

1. **Given** a table with unordered data, **When** I execute `MATCH (p:Person) RETURN p.name ORDER BY p.age`, **Then** results are sorted by age ascending
2. **Given** 1000 nodes, **When** I execute `MATCH (p:Person) RETURN p.name LIMIT 10`, **Then** only 10 results are returned
3. **Given** a sorted query with SKIP and LIMIT, **When** I execute `ORDER BY p.age SKIP 10 LIMIT 5`, **Then** rows 11-15 are returned
4. **Given** an ORDER BY with DESC, **When** executed, **Then** results are sorted in descending order

---

### User Story 6 - Vectorized Execution (Priority: P2)

As a developer, I want query execution to process data in batches (vectors) so that I benefit from CPU cache efficiency and potential SIMD optimizations.

**Why this priority**: Vectorized execution is the modern approach for analytical query processing. It provides significant performance improvements over row-at-a-time processing.

**Independent Test**: Can be tested by measuring throughput on large datasets and comparing to row-based execution baseline.

**Acceptance Scenarios**:

1. **Given** a scan operator, **When** processing 10,000 rows, **Then** data is processed in batches of 2048 elements (default vector size)
2. **Given** a filter operator, **When** applied to a batch, **Then** the output is a filtered batch maintaining columnar format
3. **Given** multiple operators in a pipeline, **When** executed, **Then** batches flow through the pipeline without per-row function calls

---

### User Story 7 - Logical Plan Visualization (Priority: P3)

As a developer, I want to see the query plan before execution so that I can understand and debug query performance.

**Why this priority**: Plan visibility is essential for performance tuning. Without it, developers cannot diagnose slow queries or verify that optimizations are being applied.

**Independent Test**: Can be tested by executing EXPLAIN on a query and verifying the output contains expected operators.

**Acceptance Scenarios**:

1. **Given** a query, **When** I prepend EXPLAIN, **Then** the logical plan is returned as formatted text showing operator tree
2. **Given** a plan with filter pushdown, **When** EXPLAIN is run, **Then** the plan shows Filter operator below Scan
3. **Given** a complex query with joins, **When** EXPLAIN is run, **Then** the plan shows join order and join types

---

### Edge Cases

- What happens when a query references a table that doesn't exist?
- How does the system handle NULL values in aggregations (e.g., SUM of nullable column)?
- What happens when ORDER BY column contains NULL values?
- How does the system handle variable-length paths with min/max bounds (e.g., `*1..5`)?
- What happens when a hash join exceeds available memory?
- How does the system handle empty intermediate results in multi-table queries?
- What happens when GROUP BY is used without aggregation functions?

## Requirements *(mandatory)*

### Functional Requirements

**Query Pipeline Architecture**

- **FR-001**: System MUST implement a three-stage query pipeline: Binding -> Planning -> Execution
- **FR-002**: System MUST transform parsed AST into a bound representation with resolved table references and type information
- **FR-003**: System MUST generate logical plans from bound queries with operator trees
- **FR-004**: System MUST translate logical plans to physical plans for execution

**Apache DataFusion Integration**

- **FR-005**: System MUST use Apache Arrow columnar format for internal data representation
- **FR-006**: System MUST integrate Apache DataFusion for relational operators (filter, project, join, aggregate, sort)
- **FR-007**: System MUST implement custom table providers for ruzu's storage layer to expose tables to DataFusion
- **FR-008**: System MUST handle Cypher-to-DataFusion translation for supported operations

**Graph-Specific Operators**

- **FR-009**: System MUST implement a NodeScan operator that scans node tables with optional filters
- **FR-010**: System MUST implement a RelScan operator that scans relationship tables
- **FR-011**: System MUST implement an Extend operator for relationship traversal (single hop)
- **FR-012**: System MUST implement a PathExpand operator for variable-length path traversal
- **FR-013**: System MUST maintain CSR (Compressed Sparse Row) index access during relationship scans

**Query Optimization**

- **FR-014**: System MUST implement filter pushdown optimization
- **FR-015**: System MUST implement projection pushdown optimization
- **FR-016**: System MUST implement predicate simplification (constant folding, dead code elimination)
- **FR-017**: System SHOULD implement join reordering for queries with multiple joins (heuristic-based)

**Result Processing**

- **FR-018**: System MUST support ORDER BY with ASC/DESC directions
- **FR-019**: System MUST support LIMIT and SKIP clauses
- **FR-020**: System MUST support COUNT, SUM, MIN, MAX, AVG aggregation functions
- **FR-021**: System MUST support GROUP BY for aggregation queries
- **FR-022**: System MUST return results as Arrow RecordBatches for efficient downstream processing

**Type System Extensions**

- **FR-023**: System MUST support boolean type for filter results
- **FR-024**: System MUST support floating-point types (FLOAT, DOUBLE) for numeric operations
- **FR-025**: System MUST support DATE and TIMESTAMP types for temporal data
- **FR-026**: System MUST handle NULL values correctly in all operators

### Key Entities

- **LogicalPlan**: Tree of logical operators representing the query semantics (what to compute, not how)
- **PhysicalPlan**: Tree of physical operators representing the execution strategy (how to compute)
- **Binder**: Component that resolves names, validates types, and produces a bound AST
- **Optimizer**: Component that transforms logical plans to apply optimization rules
- **RecordBatch**: Arrow's columnar batch format; the unit of data flow between operators
- **TableProvider**: DataFusion trait that ruzu implements to expose storage tables

## Assumptions

- DataFusion version 44+ will be used for integration
- Batch size defaults to 2048 rows (configurable) based on KuzuDB's vector capacity
- Single-threaded execution for MVP; parallel execution deferred to Phase 4
- Cost-based optimization deferred; heuristic-based join ordering sufficient for MVP
- Variable-length paths limited to reasonable bounds (default max 10 hops) to prevent runaway queries
- Cypher subset supported: MATCH, WHERE, RETURN, ORDER BY, LIMIT, SKIP (no MERGE, UNION, or subqueries)

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Queries with 1-hop relationship traversals complete in under 100ms for datasets with 100,000 nodes and 500,000 relationships
- **SC-002**: 2-hop path queries complete in under 1 second for the same dataset size
- **SC-003**: Aggregation queries (COUNT, SUM, AVG) over 1 million rows complete in under 500ms
- **SC-004**: Filter pushdown reduces execution time by at least 50% compared to post-filter approach on selective queries
- **SC-005**: All existing Phase 0 and Phase 1 tests continue to pass (no regression)
- **SC-006**: Memory usage during query execution stays within 2x the result set size (no unnecessary materialization)
- **SC-007**: EXPLAIN output correctly reflects applied optimizations (testable via plan inspection)

## Dependencies

- Phase 1 (Persistent Storage) implementation - **Completed**
- Apache Arrow crate (arrow)
- Apache DataFusion crate (datafusion) version 44+
- KuzuDB C++ reference implementation at c:/dev/kuzu for architectural guidance
