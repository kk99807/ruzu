# Feature Specification: Multi-Page Storage

**Feature Branch**: `007-multi-page-storage`
**Created**: 2026-01-30
**Status**: Draft
**Input**: User description: "Multi-page storage. We ultimately want to use '1 file per column', but as an interim step, we are going multi-page (immediate benefit with less effort)"

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Store Node Data Beyond Single Page Limit (Priority: P1)

As a database user, I need the system to store node table data that exceeds 4KB so that I can work with datasets of meaningful size without hitting a storage ceiling.

Currently, all node table data (across all node tables) must fit within a single 4KB page. This severely limits the number of rows and columns the database can persist. With multi-page storage, node data can span as many pages as needed, removing this hard cap.

**Why this priority**: This is the most fundamental limitation. Without this, the database cannot persist more than a trivial amount of node data. Node tables are the primary data structure, so lifting this limit delivers the most immediate value.

**Independent Test**: Can be fully tested by creating a node table, inserting enough rows to exceed 4KB of serialized data, closing the database, reopening it, and verifying all data is intact.

**Acceptance Scenarios**:

1. **Given** a database with a node table containing data that serializes to more than 4KB, **When** the database is closed and reopened, **Then** all node data is fully restored without loss or corruption.
2. **Given** a database with multiple node tables whose combined data exceeds 4KB, **When** the database is saved, **Then** the system allocates as many pages as needed and persists all data.
3. **Given** a database where node data previously fit in one page but grows beyond 4KB after additional inserts, **When** the database is checkpointed, **Then** the system dynamically allocates additional pages and saves successfully.

---

### User Story 2 - Store Relationship Data Beyond Single Page Limit (Priority: P2)

As a database user, I need the system to store relationship table data that exceeds 4KB so that I can persist meaningful graph structures with edges and properties.

Currently, all relationship table data must fit within a single 4KB page. CSR structures with even moderate numbers of edges and properties quickly exceed this limit. Multi-page storage removes this constraint for relationship data.

**Why this priority**: Relationship data is the second core data structure. Once node storage is multi-page, relationship storage naturally follows. Relationships tend to grow faster than node data (many edges per node), making this the next most impactful limitation to remove.

**Independent Test**: Can be fully tested by creating node and relationship tables, inserting enough relationships to exceed 4KB of serialized data, closing the database, reopening it, and verifying all relationship data (edges, properties, bidirectional indices) is intact.

**Acceptance Scenarios**:

1. **Given** a database with relationship data that serializes to more than 4KB, **When** the database is closed and reopened, **Then** all relationship data (forward groups, backward groups, properties) is fully restored.
2. **Given** a database with multiple relationship tables whose combined data exceeds 4KB, **When** the database is saved, **Then** the system allocates sufficient pages and persists all relationship data.
3. **Given** a database with relationship data that grows beyond one page after CSV import, **When** the database is checkpointed, **Then** the expanded data is correctly persisted across multiple pages.

---

### User Story 3 - Store Catalog Data Beyond Single Page Limit (Priority: P3)

As a database user, I need the catalog (schema definitions) to support multi-page storage so that I can define many tables and relationship types without hitting a schema storage limit.

Currently, the catalog is stored on a single 4KB page. For databases with many tables, complex schemas, or long column names, this could become a constraint.

**Why this priority**: The catalog is smaller than table data and less likely to hit the limit in practice, but for consistency and forward-compatibility, it should also support multi-page storage. This completes the migration so no metadata type is page-limited.

**Independent Test**: Can be fully tested by creating enough tables with sufficient columns and property definitions to exceed 4KB of catalog data, closing the database, reopening it, and verifying all schemas are intact.

**Acceptance Scenarios**:

1. **Given** a database with catalog data exceeding 4KB, **When** the database is closed and reopened, **Then** all table schemas and relationship schemas are fully restored.
2. **Given** a previously single-page catalog that grows beyond 4KB after new table creation, **When** the database is checkpointed, **Then** the catalog is correctly persisted across multiple pages.

---

### User Story 4 - Backward Compatibility with Existing Databases (Priority: P1)

As a database user, I need existing databases created before multi-page storage to continue working seamlessly after the upgrade.

Databases created with the current single-page format must open and operate correctly with the new multi-page storage system. Data must not be lost or corrupted during the transition.

**Why this priority**: This is P1 because data loss from a format change is unacceptable. Users must be able to upgrade without manual migration.

**Independent Test**: Can be fully tested by opening a database created with the current format (version 2), verifying all data loads correctly, then saving and reopening to confirm the database now uses the new multi-page format.

**Acceptance Scenarios**:

1. **Given** a version 2 database with node and relationship data, **When** it is opened by the updated system, **Then** all data loads correctly and the database operates normally.
2. **Given** a version 2 database opened by the updated system, **When** it is saved/checkpointed, **Then** the database is written in the new multi-page format and can be reopened.
3. **Given** a version 2 database with a WAL log, **When** the updated system replays the WAL, **Then** crash recovery works correctly.

---

### User Story 5 - Crash Recovery with Multi-Page Data (Priority: P2)

As a database user, I need crash recovery (WAL replay) to work correctly when data spans multiple pages so that I do not lose committed transactions.

The existing WAL mechanism must continue to function when the underlying storage uses multiple pages per metadata type. Committed data must be recoverable; uncommitted data must be rolled back.

**Why this priority**: Crash recovery is essential for data integrity but depends on the core multi-page storage being in place first.

**Independent Test**: Can be fully tested by inserting multi-page quantities of data, simulating a crash (killing the process before checkpoint), reopening the database, and verifying WAL replay restores committed data.

**Acceptance Scenarios**:

1. **Given** a database with multi-page node data and a committed transaction in the WAL, **When** the database is reopened after a crash, **Then** the committed data is restored via WAL replay.
2. **Given** a database with multi-page relationship data and an uncommitted transaction in the WAL, **When** the database is reopened, **Then** the uncommitted data is not present.

---

### Edge Cases

- What happens when the database file runs out of disk space during multi-page allocation?
- How does the system handle a partially written multi-page save (crash mid-write)?
- What happens when a page range in the header points to pages that are corrupted or zeroed?
- How does the system behave when opening a database with a version newer than it supports?
- What happens if the serialized data for a metadata type is exactly at a page boundary (e.g., exactly 4KB, exactly 8KB)?

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST support storing node table data across multiple contiguous pages when the serialized data exceeds a single page.
- **FR-002**: System MUST support storing relationship table data across multiple contiguous pages when the serialized data exceeds a single page.
- **FR-003**: System MUST support storing catalog data across multiple contiguous pages when the serialized data exceeds a single page.
- **FR-004**: System MUST dynamically allocate the number of pages needed based on the actual size of the serialized data at save time.
- **FR-005**: System MUST correctly read multi-page data by reassembling content from the page range specified in the database header.
- **FR-006**: System MUST update the database header's page ranges (catalog_range, metadata_range, rel_metadata_range) to reflect the actual number of pages used after each save.
- **FR-007**: System MUST automatically migrate version 2 databases to the new format on first open, preserving all existing data.
- **FR-008**: System MUST increment the database header version number to distinguish the new format from the previous single-page format.
- **FR-009**: System MUST maintain crash recovery (WAL replay) functionality when data spans multiple pages.
- **FR-010**: System MUST return a clear error when disk space is insufficient for page allocation.
- **FR-011**: System MUST validate page range integrity on load (e.g., ranges do not overlap, pages are within file bounds).
- **FR-012**: System MUST use the existing length-prefixed serialization format within the multi-page byte stream so that deserialization logic remains consistent.

### Key Entities

- **Page Range**: A contiguous sequence of pages identified by a start page index and a page count. Used by the database header to locate each metadata section.
- **Database Header**: The root metadata structure (page 0) containing page ranges for catalog, node metadata, and relationship metadata. Updated on each save to reflect current page allocations.
- **Page Allocator**: A mechanism to track which pages in the database file are in use and allocate new contiguous page ranges as needed.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Database can persist and reload node table data of at least 1MB without data loss or corruption.
- **SC-002**: Database can persist and reload relationship table data of at least 1MB without data loss or corruption.
- **SC-003**: Existing version 2 databases open and operate correctly with no manual migration steps required.
- **SC-004**: Crash recovery successfully restores committed multi-page data after simulated crash (100% of committed transactions recovered).
- **SC-005**: No regression in existing test suite — all current tests continue to pass.
- **SC-006**: Save and load operations for multi-page data complete within 2x the time of the current single-page operations for equivalent data sizes (no disproportionate overhead from multi-page support).

## Assumptions

- The existing 4KB page size is retained. Multi-page storage spans multiple 4KB pages rather than changing the page size.
- Page ranges are contiguous (no fragmentation support in this interim step). A metadata section occupies N consecutive pages.
- The database file grows as needed to accommodate additional pages. No pre-allocation or fixed file size.
- The existing length-prefixed bincode serialization format is preserved. The change is in how the serialized byte stream is split across pages, not in the serialization itself.
- This is an interim step toward "1 file per column" columnar storage. The design should not preclude future migration to that architecture, but does not need to anticipate it structurally.
- Single-writer model is maintained. No concurrent transaction support is added in this feature.
- WAL format and replay logic are extended minimally — the WAL continues to record logical operations, and multi-page storage affects only the checkpoint/save path.

## Out of Scope

- Columnar "1 file per column" storage (future feature)
- Page-level compression
- Non-contiguous page allocation or free-space management (fragmentation)
- Concurrent transactions or multi-writer support
- B-tree indexes or secondary indexes
- Changes to the WAL record format
