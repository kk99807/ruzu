# Feature Specification: Persistent Storage with Edge Support

**Feature Branch**: `002-persistent-storage`
**Created**: 2025-12-06
**Status**: Draft
**Input**: User description: "Phase 1 from the README.md - Persistent Storage: Disk-based storage with buffer pool management, Write-Ahead Logging (WAL), catalog persistence, crash recovery, bulk CSV ingestion, and edge/relationship support"

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Database Persistence Across Sessions (Priority: P1)

As a developer, I want my graph data to persist to disk so that when I restart my application, all previously created nodes, relationships, and schema remain intact.

**Why this priority**: Without persistence, all data is lost when the process terminates. This is the foundational capability that transforms ruzu from a demo into a real database.

**Independent Test**: Can be fully tested by creating a database, adding nodes/edges, closing the application, reopening it, and querying the same data - delivers the core value of a database.

**Acceptance Scenarios**:

1. **Given** a database with 1000 nodes and 5000 relationships, **When** the database is closed and reopened, **Then** all nodes and relationships are queryable with identical values
2. **Given** a database with custom schemas (node tables, relationship tables), **When** the database is reopened, **Then** the catalog contains all previously defined schemas
3. **Given** a new directory path, **When** a database is opened for the first time, **Then** the necessary database files are created automatically

---

### User Story 2 - Crash Recovery (Priority: P1)

As a developer, I want the database to recover gracefully from unexpected shutdowns so that committed transactions are not lost and the database remains consistent.

**Why this priority**: Data durability is a core database requirement. Without crash recovery, users cannot trust the database for any serious workload.

**Independent Test**: Can be tested by performing writes, forcibly terminating the process before clean shutdown, and verifying data integrity on restart.

**Acceptance Scenarios**:

1. **Given** a committed transaction that wrote 100 nodes, **When** the process crashes before checkpoint, **Then** after restart, all 100 nodes are present
2. **Given** an uncommitted transaction in progress, **When** the process crashes, **Then** after restart, the partial writes are rolled back and the database is consistent
3. **Given** a corrupted WAL segment (simulated), **When** database startup occurs, **Then** the system reports a clear error rather than silently corrupting data

---

### User Story 3 - Relationship/Edge Support (Priority: P1)

As a developer, I want to create relationships between nodes so that I can model and query graph data with edges connecting entities.

**Why this priority**: Relationships are fundamental to graph databases. Without edges, users cannot perform graph traversals or model real-world connections.

**Independent Test**: Can be tested by creating nodes, creating relationships between them, and querying the relationships.

**Acceptance Scenarios**:

1. **Given** a `Person` node table and a `KNOWS` relationship table, **When** I create a relationship `(alice)-[:KNOWS]->(bob)`, **Then** the relationship is stored and queryable
2. **Given** relationships with properties (e.g., `since: 2020`), **When** I query relationships, **Then** relationship properties are returned correctly
3. **Given** a node with multiple outgoing relationships, **When** I query `MATCH (a)-[:KNOWS]->(b)`, **Then** all connected nodes are returned

---

### User Story 4 - Bulk CSV Ingestion (Priority: P2)

As a developer, I want to import large datasets from CSV files so that I can efficiently load millions of nodes and relationships without individual INSERT statements.

**Why this priority**: Real-world graph databases often need to ingest large initial datasets. Individual INSERT statements are too slow for datasets with millions of records.

**Independent Test**: Can be tested by preparing a CSV file with 10,000 records and measuring import time and data integrity.

**Acceptance Scenarios**:

1. **Given** a CSV file with 100,000 rows matching a node table schema, **When** I execute a bulk import command, **Then** all rows are imported
2. **Given** a CSV file with relationships (FROM, TO, properties), **When** I execute a bulk import command, **Then** all relationships are created with correct endpoints
3. **Given** a CSV file with invalid data in some rows, **When** bulk import is executed, **Then** the system reports which rows failed and continues importing valid rows (or fails atomically based on configuration)

---

### User Story 5 - Memory-Constrained Operation (Priority: P2)

As a developer, I want the database to operate efficiently even when the dataset exceeds available memory so that I can work with large graphs without running out of RAM.

**Why this priority**: Without buffer pool management, the database cannot scale beyond available memory.

**Independent Test**: Can be tested by configuring a small buffer pool (e.g., 64MB) and loading a dataset larger than the buffer pool.

**Acceptance Scenarios**:

1. **Given** a buffer pool configured to 64MB, **When** I load 200MB of data, **Then** the database operates correctly by evicting pages as needed
2. **Given** a query that touches cold (evicted) pages, **When** the query executes, **Then** pages are transparently loaded from disk and the query returns correct results
3. **Given** high query concurrency, **When** multiple queries run simultaneously, **Then** the buffer pool handles concurrent page access without corruption

---

### Edge Cases

- What happens when disk space is exhausted during a write operation?
- How does the system handle a database file that was created on a different platform (endianness)?
- What happens when a CSV file has more columns than the target table schema?
- What happens when a CSV file has fewer columns than the target table schema?
- How does the system handle relationship creation when the referenced node doesn't exist?
- What happens when the buffer pool is configured larger than available system memory?
- What happens when WAL replay is interrupted by another crash?

## Requirements *(mandatory)*

### Functional Requirements

**Storage Layer**

- **FR-001**: System MUST persist all node data to disk in a structured file format
- **FR-002**: System MUST persist all relationship data to disk, including start/end node references and properties
- **FR-003**: System MUST support configurable database directory location
- **FR-004**: System MUST create database files automatically on first access

**Buffer Pool Management**

- **FR-005**: System MUST implement a buffer pool with configurable maximum memory usage
- **FR-006**: System MUST use page-based I/O with a fixed page size (4KB recommended)
- **FR-007**: System MUST implement page eviction when buffer pool is full (LRU or similar policy)
- **FR-008**: System MUST pin pages during active use to prevent eviction of in-use data
- **FR-009**: System MUST track dirty pages and write them to disk before eviction

**Write-Ahead Logging (WAL)**

- **FR-010**: System MUST write all modifications to a WAL before modifying data pages
- **FR-011**: System MUST support WAL replay on startup to recover committed transactions
- **FR-012**: System MUST implement WAL checkpointing to bound recovery time
- **FR-013**: System MUST use checksums or similar integrity verification for WAL records

**Catalog Persistence**

- **FR-014**: System MUST persist the catalog (node table schemas, relationship table schemas) to disk
- **FR-015**: System MUST load the catalog automatically when opening an existing database
- **FR-016**: System MUST support schema evolution detection (warning when schema changes)

**Relationship/Edge Support**

- **FR-017**: System MUST support `CREATE REL TABLE` statements to define relationship schemas
- **FR-018**: System MUST support creating relationships between existing nodes
- **FR-019**: System MUST support relationship properties (typed columns on relationships)
- **FR-020**: System MUST support querying relationships in MATCH patterns (e.g., `(a)-[:REL]->(b)`)
- **FR-021**: System MUST validate that relationship endpoints reference existing nodes (referential integrity)

**Bulk CSV Ingestion**

- **FR-022**: System MUST support bulk loading nodes from CSV files via a COPY command
- **FR-023**: System MUST support bulk loading relationships from CSV files with FROM/TO columns
- **FR-024**: System MUST support configurable CSV parsing options (delimiter, quote character, header row)
- **FR-025**: System MUST report progress during bulk operations (rows processed count)
- **FR-026**: System MUST support either atomic (all-or-nothing) or continue-on-error modes for bulk import

### Key Entities

- **Page**: Fixed-size block of data (4KB), the unit of I/O between disk and memory. Contains node data, relationship data, or metadata.
- **Buffer Frame**: A memory slot in the buffer pool that holds one page. Tracks pin count, dirty flag, and eviction metadata.
- **WAL Record**: A log entry describing a single modification (insert, update, delete). Contains transaction ID, operation type, and before/after data.
- **Relationship Table Schema**: Defines a relationship type including source node table, destination node table, directionality, and property columns.
- **Relationship**: An edge instance connecting two nodes, with optional properties. Stored with references to source and destination node IDs.

## Assumptions

- Single-writer model: Only one transaction can write at a time (concurrent reads allowed)
- The system targets workloads where datasets fit on local disk (not distributed storage)
- CSV files are expected to be UTF-8 encoded
- Platform is little-endian (x86_64, ARM64); big-endian platforms are out of scope for MVP
- Buffer pool size default is 80% of available system memory or 256MB, whichever is smaller

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Database with 100,000 nodes and 500,000 relationships persists and reopens correctly
- **SC-002**: Crash recovery completes within 30 seconds for databases with up to 10GB of data
- **SC-003**: Bulk CSV import processes at least 50,000 nodes per second on commodity hardware
- **SC-004**: Bulk CSV import processes at least 100,000 relationships per second on commodity hardware
- **SC-005**: Database operates correctly with datasets 4x larger than configured buffer pool
- **SC-006**: All existing Phase 0 tests continue to pass (no regression)
- **SC-007**: System achieves 95% of operations completing without page faults when working set fits in buffer pool

## Dependencies

- Phase 0 PoC implementation (parser, in-memory storage, basic executor) - **Completed**
- KuzuDB C++ reference implementation at c:/dev/kuzu for architectural guidance
