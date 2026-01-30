# Feature Specification: Fix Relationship Table Persistence

**Feature Branch**: `001-fix-rel-persistence`
**Created**: 2026-01-29
**Status**: Draft
**Input**: User description: "Fix relationship persistence bug where relationship table data is not saved to or loaded from disk during database open/close operations, causing silent data loss on restart"

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Relationship Data Survives Database Restart (Priority: P1)

A database developer creates relationship tables with edges between nodes, closes the database, then reopens it expecting all relationship data to be preserved and queryable.

**Why this priority**: This is the core bug - complete data loss for all relationship data after restart. Without this, relationship tables are unusable in persistent databases, making the feature effectively broken for production use.

**Independent Test**: Can be fully tested by creating a database with nodes and relationships, closing it, reopening it, and querying the relationships. Delivers a working persistent relationship storage system.

**Acceptance Scenarios**:

1. **Given** a database with node table Person and relationship table Knows, **When** I create relationships between nodes, close the database, reopen it, and query for relationships, **Then** all created relationships are returned with correct source, destination, and properties
2. **Given** a database with existing relationships, **When** I close the database normally and reopen it, **Then** the number of relationships in each relationship table matches the count before closing
3. **Given** a database with no relationships (empty relationship tables), **When** I close and reopen the database, **Then** the relationship tables exist with their schemas intact and contain zero relationships

---

### User Story 2 - CSV-Imported Relationships Persist After Restart (Priority: P2)

A database developer imports large volumes of relationship data from CSV files using COPY FROM, then needs to restart the database and continue working with the imported data.

**Why this priority**: Bulk import is a common way to populate databases. Without persistence, users must re-import data after every restart, making bulk operations impractical.

**Independent Test**: Can be independently tested by importing relationships via CSV, closing the database, reopening it, and verifying imported relationships are present. Delivers persistent bulk import capability.

**Acceptance Scenarios**:

1. **Given** a CSV file with 1000 relationship records, **When** I import them using COPY FROM, close the database, reopen it, and query the relationship table, **Then** all 1000 relationships are present with correct data
2. **Given** multiple relationship tables each with CSV-imported data, **When** I close and reopen the database, **Then** each relationship table contains all its imported relationships

---

### User Story 3 - Recovery After Uncommitted Relationship Changes (Priority: P3)

A database developer makes changes to relationships but the database crashes before committing. On restart, the database should recover to the last committed state without corrupted relationship data.

**Why this priority**: Crash recovery is important for data integrity, but the primary bug is about basic persistence. Once P1 is fixed, this ensures the WAL recovery mechanism also handles relationships correctly.

**Independent Test**: Can be independently tested by creating committed relationships, making uncommitted changes, simulating a crash (force kill), reopening the database, and verifying only committed relationships are present. Delivers crash-safe relationship storage.

**Acceptance Scenarios**:

1. **Given** a database with committed relationships, **When** I create new relationships without committing, simulate a crash, and reopen the database, **Then** only the committed relationships are present
2. **Given** a transaction that creates relationships and is committed, **When** the database crashes after commit and restarts, **Then** all committed relationships are recovered through WAL replay

---

### Edge Cases

- What happens when a relationship table has schema (metadata in catalog) but zero relationships after loading?
- How does the system handle loading a database where relationship table data is corrupted or missing on disk?
- What happens if the buffer pool runs out of space while loading large relationship tables during database open?
- How does the system handle relationship tables that reference node tables which no longer exist?
- What happens when loading relationship data that exceeds the reserved metadata page size?

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST serialize all relationship table data when saving the database to disk
- **FR-002**: System MUST deserialize all relationship table data when opening an existing database from disk
- **FR-003**: System MUST preserve relationship properties during save and load operations
- **FR-004**: System MUST preserve bidirectional CSR structures (forward and backward groups) during persistence
- **FR-005**: System MUST handle databases with zero relationships (empty relationship tables) without errors
- **FR-006**: System MUST maintain relationship table schemas in the catalog independently of relationship data persistence
- **FR-007**: System MUST load relationship data alongside node data during database open operation
- **FR-008**: System MUST save relationship data alongside node data during checkpoint and close operations
- **FR-009**: System MUST handle multiple relationship tables, persisting and loading each independently
- **FR-010**: WAL replay MUST reconstruct relationship data from committed transactions after a crash
- **FR-011**: System MUST detect when relationship data cannot be loaded and report a clear error message
- **FR-012**: System MUST handle cases where relationship table schemas exist in catalog but data pages are missing or corrupted

### Key Entities

- **Relationship Table Data**: Contains forward CSR groups (source to destination edges), backward CSR groups (destination to source edges), next relationship ID counter, and relationship properties. Must be serializable and deserializable without data loss.
- **Relationship Table**: In-memory structure that uses RelTableData for persistence. Each relationship table is identified by name and has an associated schema.
- **Database Metadata Pages**: Reserved pages in the database file format where serialized relationship data will be stored, similar to how node table data is currently stored.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A database with relationships can be closed and reopened 100 times without losing any relationship data
- **SC-002**: Queries for relationships return identical results before and after database restart
- **SC-003**: CSV import of 10,000 relationships completes successfully and all relationships remain queryable after database restart
- **SC-004**: Zero silent data loss occurs - any failure to persist or load relationships produces an explicit error message
- **SC-005**: Relationship table schemas and relationship data are both present after database restart (no schema-data mismatch)
- **SC-006**: WAL recovery correctly restores committed relationships after simulated crash
- **SC-007**: Database open time increases linearly with the number of relationships (no performance regression)
- **SC-008**: Memory usage during database open remains constant regardless of relationship table size (data loads on-demand)
