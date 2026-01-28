# Feature Specification: Phase 0 Proof of Concept

**Feature Branch**: `001-poc-basic-functionality`
**Created**: 2025-12-05
**Status**: Draft
**Input**: User description: "Phase 0: Proof of Concept - Basic graph database functionality with minimal Cypher support. Goal: Parse simple Cypher queries (CREATE, MATCH, RETURN), execute against in-memory columnar storage, and establish baseline benchmarks. Target query: CREATE NODE TABLE Person(name STRING, age INT64, PRIMARY KEY(name)); CREATE (:Person {name: 'Alice', age: 25}); MATCH (p:Person) WHERE p.age > 20 RETURN p.name, p.age;"

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Define Graph Schema (Priority: P1)

A developer needs to define the structure of their graph data by creating node tables with typed properties and primary keys.

**Why this priority**: Schema definition is the foundation - without it, no data can be stored or queried. This is the absolute minimum viable functionality.

**Independent Test**: Can be fully tested by executing a CREATE NODE TABLE statement and verifying the schema is stored and can be retrieved. Delivers value by allowing developers to model their domain.

**Acceptance Scenarios**:

1. **Given** an empty database, **When** I execute `CREATE NODE TABLE Person(name STRING, age INT64, PRIMARY KEY(name))`, **Then** the schema is stored with columns name (STRING) and age (INT64) with name as primary key
2. **Given** a database with a Person table, **When** I attempt to create another table with the same name, **Then** I receive an error indicating the table already exists
3. **Given** an empty database, **When** I execute a CREATE NODE TABLE with invalid syntax, **Then** I receive a clear parse error message
4. **Given** a CREATE NODE TABLE statement, **When** I specify multiple properties of different types (STRING, INT64), **Then** all properties are correctly stored in the schema

---

### User Story 2 - Insert Graph Data (Priority: P2)

A developer needs to populate their graph database with nodes containing property values.

**Why this priority**: After defining schema, inserting data is the next essential step. Without data insertion, the database is just an empty schema.

**Independent Test**: Can be tested by executing CREATE statements to insert nodes and then verifying the data is stored in memory. Delivers value by allowing developers to populate their graph.

**Acceptance Scenarios**:

1. **Given** a Person node table exists, **When** I execute `CREATE (:Person {name: 'Alice', age: 25})`, **Then** a new Person node is created with name='Alice' and age=25
2. **Given** a Person table with name as primary key, **When** I attempt to create a node with a duplicate primary key value, **Then** I receive a unique constraint violation error
3. **Given** a Person table, **When** I create a node with missing required properties, **Then** I receive a validation error
4. **Given** a Person table, **When** I create multiple nodes with different property values, **Then** all nodes are stored correctly with their respective property values

---

### User Story 3 - Query Graph Data (Priority: P3)

A developer needs to retrieve data from their graph database using pattern matching and filtering.

**Why this priority**: Querying is what makes the data useful. This validates the entire end-to-end flow (parse → execute → return results).

**Independent Test**: Can be tested by inserting test data and executing MATCH queries with WHERE clauses, verifying correct results are returned. Delivers value by enabling data retrieval and analysis.

**Acceptance Scenarios**:

1. **Given** a Person table with nodes for Alice (age 25) and Bob (age 30), **When** I execute `MATCH (p:Person) RETURN p.name, p.age`, **Then** I receive rows for both Alice and Bob with their respective ages
2. **Given** a Person table with multiple nodes, **When** I execute `MATCH (p:Person) WHERE p.age > 20 RETURN p.name`, **Then** I receive only the names of persons whose age is greater than 20
3. **Given** a Person table, **When** I execute a MATCH query for a non-existent table, **Then** I receive an error indicating the table does not exist
4. **Given** a Person table, **When** I execute a MATCH with invalid WHERE clause syntax, **Then** I receive a clear parse error message
5. **Given** an empty Person table, **When** I execute a MATCH query, **Then** I receive an empty result set (zero rows)

---

### User Story 4 - Measure Performance Baseline (Priority: P4)

Developers and project maintainers need to understand how the Rust implementation performs compared to the C++ KuzuDB reference to validate the port's feasibility.

**Why this priority**: This is validation/measurement rather than user-facing functionality, but critical for Phase 0 success gate criteria.

**Independent Test**: Can be tested by running benchmark suite against both Rust and C++ implementations on identical queries and comparing execution times. Delivers value by providing data-driven go/no-go decision for continuing the port.

**Acceptance Scenarios**:

1. **Given** the C++ KuzuDB baseline is established, **When** I run the benchmark suite on the Rust PoC, **Then** I receive performance metrics (query execution time, memory usage) for comparison
2. **Given** benchmark results, **When** Rust PoC is slower than 10x the C++ baseline, **Then** the performance is within acceptable Phase 0 tolerance
3. **Given** the target PoC query, **When** I execute it in both C++ and Rust implementations, **Then** I can measure and compare parse time, execution time, and total time
4. **Given** benchmark results, **When** performance exceeds 10x slower, **Then** I can identify the bottleneck component (parser, storage, executor)

---

### Edge Cases

- What happens when a CREATE NODE TABLE statement has duplicate column names?
- How does the system handle property type mismatches (e.g., assigning a string to an INT64 column)?
- What happens when a MATCH query references a column that doesn't exist in the schema?
- How does the system handle empty strings or null values in properties?
- What happens when memory is exhausted during data insertion?
- How does the system handle Cypher keywords used as identifiers (e.g., table named "MATCH")?
- What happens when a WHERE clause has logical errors (e.g., comparing STRING to INT64)?
- How does the system handle concurrent access to in-memory storage (note: Phase 0 may not support concurrency)?

## Requirements *(mandatory)*

### Functional Requirements

**Schema Definition:**
- **FR-001**: System MUST parse CREATE NODE TABLE statements with table name, column definitions, and primary key specification
- **FR-002**: System MUST support basic data types: STRING and INT64
- **FR-003**: System MUST validate that table names are unique within the database
- **FR-004**: System MUST store table schemas in an in-memory catalog
- **FR-005**: System MUST validate that primary key columns are defined in the table schema

**Data Insertion:**
- **FR-006**: System MUST parse CREATE statements for inserting nodes with property values
- **FR-007**: System MUST validate that property names in CREATE statements match the table schema
- **FR-008**: System MUST validate that property values match their column data types
- **FR-009**: System MUST enforce primary key uniqueness constraints
- **FR-010**: System MUST store node data in columnar format in memory

**Data Retrieval:**
- **FR-011**: System MUST parse MATCH queries with node patterns and variable bindings
- **FR-012**: System MUST parse WHERE clauses with comparison operators (>, <, =, >=, <=)
- **FR-013**: System MUST parse RETURN clauses specifying which properties to output
- **FR-014**: System MUST execute MATCH queries by scanning the appropriate node table
- **FR-015**: System MUST filter results based on WHERE clause predicates
- **FR-016**: System MUST project results to include only columns specified in RETURN clause
- **FR-017**: System MUST return query results as a collection of rows with named columns

**Error Handling:**
- **FR-018**: System MUST provide clear error messages for syntax errors in Cypher queries
- **FR-019**: System MUST provide clear error messages for schema violations (e.g., unknown table, type mismatch)
- **FR-020**: System MUST provide clear error messages for constraint violations (e.g., duplicate primary key)

**Performance Measurement:**
- **FR-021**: System MUST provide a benchmarking framework for measuring query execution time
- **FR-022**: System MUST support executing the same query multiple times for statistical analysis
- **FR-023**: System MUST track and report parse time, execution time, and total time separately

### Key Entities

- **NodeTable**: Represents a type of node in the graph with a name, set of columns (name, type), and primary key column(s). Stored in the catalog.

- **Column**: Represents a property of a node with a name and data type (STRING or INT64). Belongs to a NodeTable.

- **Node**: An instance of a NodeTable with specific property values. Stored in columnar format in memory.

- **Query**: A Cypher statement (CREATE NODE TABLE, CREATE, or MATCH) that has been parsed into an abstract syntax tree and can be executed against the database.

- **QueryResult**: The output of executing a query, containing a set of rows with named columns and values.

- **Benchmark**: A measurement of query performance including parse time, execution time, total time, and memory usage for a specific query.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Developers can define a node table schema and receive immediate confirmation (under 100ms)
- **SC-002**: Developers can insert at least 1000 nodes into a table without errors
- **SC-003**: Developers can query data and receive correct results matching their WHERE clause predicates
- **SC-004**: The target PoC query executes successfully from end-to-end (parse → insert → query → return results)
- **SC-005**: Query execution performance is within 10x of C++ KuzuDB baseline on equivalent operations
- **SC-006**: Memory usage for 1000 nodes is under 10MB (excluding Rust runtime overhead)
- **SC-007**: 100% of test cases pass for all three user stories (schema, insert, query)
- **SC-008**: Benchmark suite produces consistent, reproducible results (variance under 10% across runs)

### Phase 0 Gate Criteria (from Constitution)

- **SC-009**: Simple query executes end-to-end (parse → plan → execute) successfully
- **SC-010**: Baseline benchmarks are established and documented
- **SC-011**: Performance is within 10x of C++ KuzuDB (acceptable for PoC)
- **SC-012**: Decision point: Continue to Phase 1 or pivot based on technical feasibility and performance data

## Assumptions

- **In-memory only**: Phase 0 does not persist data to disk; all data is lost when the process ends
- **No concurrency**: Single-threaded execution; no concurrent query support
- **Limited data types**: Only STRING and INT64 supported; other types (FLOAT, BOOL, DATE, etc.) deferred to later phases
- **No relationships**: Only node tables; relationship tables deferred to Phase 1
- **No indexes**: Full table scans only; indexing deferred to Phase 2
- **No transactions**: No ACID guarantees; operations are immediate and atomic
- **Minimal Cypher**: Only CREATE NODE TABLE, CREATE (nodes), MATCH, WHERE (simple comparisons), and RETURN supported
- **No aggregations**: No COUNT, SUM, AVG, etc.; deferred to Phase 2
- **No sorting/limits**: No ORDER BY, LIMIT, SKIP; deferred to Phase 2
- **ASCII strings only**: No Unicode support in Phase 0; UTF-8 deferred to Phase 1
- **Baseline measurement**: C++ KuzuDB benchmarks assumed to be run on same hardware for fair comparison

## Out of Scope (Explicitly Excluded)

- Disk persistence (WAL, checkpointing) - Phase 1
- Relationship tables and graph traversal - Phase 1
- Complex Cypher features (OPTIONAL MATCH, MERGE, subqueries) - Phase 2+
- Query optimization (join ordering, filter pushdown) - Phase 2
- Indexes (hash, B-tree) - Phase 2
- Aggregation functions (COUNT, SUM, AVG, MIN, MAX) - Phase 2
- Concurrent transactions - Phase 3
- Multi-writer MVCC - Phase 4+
- Compression - Phase 1+
- Full Unicode support - Phase 1+
- User authentication/permissions - Out of MVP
- Extension framework - Out of MVP
- Parquet import/export - Phase 4+

## Dependencies

- **C++ KuzuDB source code** (C:\dev\kuzu): Required for reference implementation and benchmarking baseline
- **Rust 1.75+**: Minimum Rust version per project constitution
- **Pest or Nom** (parser generator): Will be selected during planning phase
- **Criterion** (benchmarking framework): For performance measurement
- **Apache Arrow** (optional for PoC): Columnar data format; may be simplified vector-based storage for Phase 0

## Risks

- **Performance risk**: If Rust PoC is >10x slower, may indicate architectural issues requiring redesign
- **Complexity risk**: Even "minimal" Cypher parser may be more complex than estimated
- **Ecosystem risk**: Rust parser libraries (pest/nom) may have different ergonomics than ANTLR4, requiring more effort
- **Columnar storage risk**: Implementing columnar storage from scratch may be time-consuming; may simplify to row-based for PoC
- **Benchmark environment risk**: Need identical hardware for C++ vs Rust comparison; cloud/VM variance could skew results

## Next Steps

After this specification is approved:
1. Run `/speckit.plan` to generate implementation plan with technical approach
2. Establish C++ KuzuDB baseline benchmarks (document hardware, query times)
3. Select parser library (pest vs nom) based on feasibility study
4. Decide on columnar storage approach (Arrow vs simplified vectors)
5. Run `/speckit.tasks` to generate detailed task breakdown
6. Begin TDD implementation following Red-Green-Refactor cycle
