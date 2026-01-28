# Storage Format Specification

**Feature**: 002-persistent-storage
**Version**: 1.0
**Date**: 2025-12-06
**Status**: Draft

## Overview

This document specifies the binary file formats for ruzu persistent storage. All formats use little-endian byte order and are designed for direct memory mapping.

---

## 1. Database File Format

### 1.1 File Layout

```
┌─────────────────────────────────────────────────────────────────┐
│ Page 0: Database Header (4KB)                                   │
├─────────────────────────────────────────────────────────────────┤
│ Pages 1-N: Catalog (variable, defined by header.catalog_range)  │
├─────────────────────────────────────────────────────────────────┤
│ Pages N+1-M: Metadata (variable, defined by header.metadata_range)│
├─────────────────────────────────────────────────────────────────┤
│ Pages M+1-...: Data Pages (node tables, relationship tables)    │
└─────────────────────────────────────────────────────────────────┘
```

### 1.2 Database Header (Page 0)

**Size**: 4096 bytes (1 page)
**Location**: Byte offset 0

| Offset | Size | Field | Description |
|--------|------|-------|-------------|
| 0 | 8 | magic | "RUZUDB\0\0" (0x52555A5544420000) |
| 8 | 4 | version | Format version (uint32, current = 1) |
| 12 | 4 | reserved | Reserved for flags (uint32, = 0) |
| 16 | 16 | database_id | UUID v4, unique database identifier |
| 32 | 4 | catalog_start | First page of catalog (uint32) |
| 36 | 4 | catalog_pages | Number of catalog pages (uint32) |
| 40 | 4 | metadata_start | First page of metadata (uint32) |
| 44 | 4 | metadata_pages | Number of metadata pages (uint32) |
| 48 | 8 | next_page | Next free page index (uint64) |
| 56 | 8 | node_id_counter | Next node ID to allocate (uint64) |
| 64 | 8 | rel_id_counter | Next relationship ID to allocate (uint64) |
| 72 | 4 | header_checksum | CRC32 of bytes 0-71 (uint32) |
| 76 | 4020 | padding | Zero-filled to page boundary |

**Validation**:
1. `magic == "RUZUDB\0\0"`
2. `version <= CURRENT_VERSION`
3. `header_checksum == crc32(bytes[0..72])`

### 1.3 Catalog Pages

Catalog is serialized using bincode format with the following structure:

```rust
#[derive(Serialize, Deserialize)]
struct SerializedCatalog {
    entries: Vec<CatalogEntry>,
}

#[derive(Serialize, Deserialize)]
struct CatalogEntry {
    entry_type: CatalogEntryType,
    table_id: u32,
    name: String,
    columns: Vec<ColumnDef>,
}

#[derive(Serialize, Deserialize)]
enum CatalogEntryType {
    NodeTable { primary_key: String },
    RelTable { src_table: String, dst_table: String, direction: u8 },
}

#[derive(Serialize, Deserialize)]
struct ColumnDef {
    name: String,
    data_type: DataType,
    nullable: bool,
}

#[derive(Serialize, Deserialize)]
enum DataType {
    Int64 = 1,
    Float64 = 2,
    Bool = 3,
    String = 4,
    Date = 5,
    Timestamp = 6,
}
```

**Format**:
- First 4 bytes: total serialized length (uint32)
- Remaining bytes: bincode-encoded `SerializedCatalog`
- Padded to page boundary with zeros

### 1.4 Data Pages

Each data page has a common header followed by type-specific content:

**Page Header** (16 bytes):
| Offset | Size | Field | Description |
|--------|------|-------|-------------|
| 0 | 4 | page_type | Page type enum (uint32) |
| 4 | 4 | table_id | Owning table ID (uint32) |
| 8 | 4 | page_sequence | Page number within table (uint32) |
| 12 | 4 | checksum | CRC32 of entire page (uint32) |

**Page Types**:
| Value | Type | Description |
|-------|------|-------------|
| 1 | NodeData | Node column data |
| 2 | NodeOffsets | Node ID → page mapping |
| 3 | CsrOffsets | CSR offset array |
| 4 | CsrNeighbors | CSR neighbor IDs |
| 5 | CsrRelIds | CSR relationship IDs |
| 6 | RelProperties | Relationship property columns |

---

## 2. WAL File Format

### 2.1 File Location

WAL file is stored at: `{database_path}/wal.log`

### 2.2 WAL Header

**Size**: 64 bytes
**Location**: Byte offset 0

| Offset | Size | Field | Description |
|--------|------|-------|-------------|
| 0 | 8 | magic | "RUZUWAL\0" (0x52555A5557414C00) |
| 8 | 4 | version | WAL format version (uint32, current = 1) |
| 12 | 1 | enable_checksums | 1 = checksums enabled, 0 = disabled |
| 13 | 3 | reserved | Zero padding |
| 16 | 16 | database_id | Must match database file UUID |
| 32 | 8 | first_lsn | LSN of first record in this file |
| 40 | 8 | last_checkpoint_lsn | LSN of last checkpoint |
| 48 | 16 | reserved | Reserved for future use |

### 2.3 WAL Record Format

Each record follows the header:

| Offset | Size | Field | Description |
|--------|------|-------|-------------|
| 0 | 4 | record_length | Total record size including header (uint32) |
| 4 | 1 | record_type | Record type enum (uint8) |
| 5 | 3 | reserved | Zero padding |
| 8 | 8 | transaction_id | Owning transaction ID (uint64) |
| 16 | 8 | lsn | Log Sequence Number (uint64) |
| 24 | variable | payload | Type-specific payload (bincode) |
| 24+payload_len | 4 | checksum | CRC32 of bytes [0..24+payload_len], if enabled |

**Record Types**:
| Value | Type | Payload Structure |
|-------|------|-------------------|
| 1 | BeginTransaction | `{ }` (empty) |
| 2 | Commit | `{ }` (empty) |
| 3 | Abort | `{ }` (empty) |
| 30 | TableInsertion | `{ table_id: u32, num_rows: u32, data: [u8] }` |
| 31 | NodeDeletion | `{ table_id: u32, node_offset: u64, pk: Value }` |
| 32 | NodeUpdate | `{ table_id: u32, col_id: u32, offset: u64, value: Value }` |
| 36 | RelInsertion | `{ table_id: u32, src: u64, dst: u64, props: [Value] }` |
| 33 | RelDeletion | `{ table_id: u32, src: u64, dst: u64, rel_id: u64 }` |
| 254 | Checkpoint | `{ checkpoint_id: u64 }` |

### 2.4 Value Serialization

Values are serialized using a type-tag format:

| Tag | Type | Representation |
|-----|------|----------------|
| 0 | Null | (no additional bytes) |
| 1 | Int64 | 8 bytes, little-endian |
| 2 | Float64 | 8 bytes, IEEE 754 |
| 3 | Bool | 1 byte (0 or 1) |
| 4 | String | 4-byte length + UTF-8 bytes |
| 5 | Date | 4 bytes (days since epoch) |
| 6 | Timestamp | 8 bytes (microseconds since epoch) |

---

## 3. Node Table Storage

### 3.1 Columnar Layout

Node data is stored column-by-column within pages:

```
┌─────────────────────────────────────────┐
│ Column 0 Pages (e.g., "name" STRING)    │
│ ├─ Page 0: rows 0-341                   │
│ ├─ Page 1: rows 342-683                 │
│ └─ ...                                  │
├─────────────────────────────────────────┤
│ Column 1 Pages (e.g., "age" INT64)      │
│ ├─ Page 0: rows 0-508                   │
│ ├─ Page 1: rows 509-1017                │
│ └─ ...                                  │
├─────────────────────────────────────────┤
│ ...                                     │
└─────────────────────────────────────────┘
```

### 3.2 Fixed-Width Columns

For INT64, FLOAT64, BOOL, DATE, TIMESTAMP:

**Page Layout**:
| Offset | Size | Content |
|--------|------|---------|
| 0 | 16 | Page header |
| 16 | 4 | num_values (uint32) |
| 20 | 4 | null_bitmap_size (uint32) |
| 24 | null_bitmap_size | Null bitmap (1 bit per value) |
| 24+null_bitmap_size | value_size * num_values | Values |

**Values per page** (after header overhead):
- INT64: (4096 - 24) / 8 = ~508 values
- BOOL: (4096 - 24) / 1 = ~4072 values

### 3.3 Variable-Width Columns (STRING)

**Page Layout**:
| Offset | Size | Content |
|--------|------|---------|
| 0 | 16 | Page header |
| 16 | 4 | num_values (uint32) |
| 20 | 4 | null_bitmap_size (uint32) |
| 24 | null_bitmap_size | Null bitmap |
| 24+null_bitmap_size | num_values * 4 | Offset array (uint32[]) |
| 24+null_bitmap_size+(num_values*4) | variable | String data (UTF-8) |

Strings that don't fit in a single page use overflow pages.

---

## 4. Relationship Table Storage (CSR Format)

### 4.1 CSR Overview

Relationships are stored using Compressed Sparse Row format:
- **Offsets array**: For each source node, the starting index in neighbors array
- **Neighbors array**: Destination node IDs
- **RelIds array**: Relationship IDs (parallel to neighbors)
- **Properties**: Column storage parallel to neighbors

### 4.2 CSR Offset Pages

**Page Layout**:
| Offset | Size | Content |
|--------|------|---------|
| 0 | 16 | Page header (page_type = 3) |
| 16 | 4 | first_node_offset (uint32) |
| 20 | 4 | num_offsets (uint32) |
| 24 | num_offsets * 8 | Offset values (uint64[]) |

**Capacity**: (4096 - 24) / 8 = ~509 offsets per page

### 4.3 CSR Neighbor Pages

**Page Layout**:
| Offset | Size | Content |
|--------|------|---------|
| 0 | 16 | Page header (page_type = 4) |
| 16 | 4 | first_edge_idx (uint32) |
| 20 | 4 | num_neighbors (uint32) |
| 24 | num_neighbors * 8 | Neighbor node IDs (uint64[]) |

**Capacity**: (4096 - 24) / 8 = ~509 neighbors per page

### 4.4 CSR RelId Pages

**Page Layout**:
| Offset | Size | Content |
|--------|------|---------|
| 0 | 16 | Page header (page_type = 5) |
| 16 | 4 | first_edge_idx (uint32) |
| 20 | 4 | num_rel_ids (uint32) |
| 24 | num_rel_ids * 8 | Relationship IDs (uint64[]) |

---

## 5. Integrity Verification

### 5.1 Checksums

All checksums use CRC32 (IEEE polynomial).

**Pages**: Checksum covers entire page except the checksum field itself.
**WAL Records**: Checksum covers header + payload.

### 5.2 Magic Bytes

| File Type | Magic Bytes |
|-----------|-------------|
| Database | "RUZUDB\0\0" |
| WAL | "RUZUWAL\0" |

### 5.3 Version Compatibility

- Current database version: 1
- Current WAL version: 1
- Forward compatibility: Newer versions MAY read older files
- Backward compatibility: Older versions MUST NOT read newer files

---

## 6. Example: Small Database

A database with one node table (Person) and 2 nodes:

```
Page 0 (Header):
  magic = "RUZUDB\0\0"
  version = 1
  database_id = 550e8400-e29b-41d4-a716-446655440000
  catalog_start = 1
  catalog_pages = 1
  metadata_start = 2
  metadata_pages = 1
  next_page = 3

Page 1 (Catalog):
  entries = [
    NodeTable {
      table_id = 0,
      name = "Person",
      columns = [
        { name: "name", type: String, nullable: false },
        { name: "age", type: Int64, nullable: false }
      ],
      primary_key = "name"
    }
  ]

Page 2 (Metadata):
  table_metadata = [
    { table_id = 0, num_rows = 2, column_pages = [[3], [4]] }
  ]

Page 3 (Person.name column):
  page_type = NodeData
  table_id = 0
  num_values = 2
  null_bitmap = [0x00]
  offsets = [0, 5, 8]
  data = "AliceBob"

Page 4 (Person.age column):
  page_type = NodeData
  table_id = 0
  num_values = 2
  null_bitmap = [0x00]
  values = [25, 30]  // as int64
```

---

## 7. API Contracts

### 7.1 Database Opening

```rust
/// Opens or creates a database at the specified path.
///
/// # Arguments
/// * `path` - Directory path for database files
/// * `config` - Database configuration options
///
/// # Returns
/// * `Ok(Database)` - Successfully opened database
/// * `Err(StorageError::InvalidMagic)` - File is not a ruzu database
/// * `Err(StorageError::UnsupportedVersion)` - Version too new
/// * `Err(StorageError::CorruptedFile)` - Checksum mismatch
/// * `Err(StorageError::WalReplayFailed)` - Crash recovery failed
pub fn Database::open(path: &Path, config: DatabaseConfig) -> Result<Database, StorageError>;
```

### 7.2 CSV Import

```rust
/// Imports nodes from a CSV file.
///
/// # Arguments
/// * `table_name` - Target node table
/// * `csv_path` - Path to CSV file
/// * `config` - Import configuration
/// * `progress_callback` - Optional progress callback
///
/// # Returns
/// * `Ok(ImportResult)` - Import statistics
/// * `Err(StorageError::TableNotFound)` - Table doesn't exist
/// * `Err(StorageError::SchemaMismatch)` - CSV columns don't match table
/// * `Err(StorageError::ImportFailed)` - Parse or validation errors
pub fn Database::import_nodes(
    &mut self,
    table_name: &str,
    csv_path: &Path,
    config: CsvImportConfig,
    progress_callback: Option<Box<dyn Fn(ImportProgress)>>,
) -> Result<ImportResult, StorageError>;

/// Imports relationships from a CSV file.
///
/// # Arguments
/// * `table_name` - Target relationship table
/// * `csv_path` - Path to CSV file (must have FROM, TO columns)
/// * `config` - Import configuration
/// * `progress_callback` - Optional progress callback
pub fn Database::import_relationships(
    &mut self,
    table_name: &str,
    csv_path: &Path,
    config: CsvImportConfig,
    progress_callback: Option<Box<dyn Fn(ImportProgress)>>,
) -> Result<ImportResult, StorageError>;
```

### 7.3 Relationship Queries

```rust
/// Creates a relationship table schema.
///
/// Cypher syntax: CREATE REL TABLE KNOWS(FROM Person TO Person, since INT64)
pub fn Database::create_rel_table(
    &mut self,
    name: &str,
    src_table: &str,
    dst_table: &str,
    columns: Vec<ColumnDef>,
    direction: Direction,
) -> Result<(), StorageError>;

/// Creates a relationship between two nodes.
///
/// Cypher syntax: MATCH (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'})
///                CREATE (a)-[:KNOWS {since: 2020}]->(b)
pub fn Database::create_relationship(
    &mut self,
    rel_table: &str,
    src_node_id: u64,
    dst_node_id: u64,
    properties: Vec<Value>,
) -> Result<u64, StorageError>;  // Returns relationship ID
```

---

## 8. Error Codes

| Code | Name | Description |
|------|------|-------------|
| E001 | InvalidMagic | File signature doesn't match |
| E002 | UnsupportedVersion | File version is newer than supported |
| E003 | CorruptedChecksum | CRC32 mismatch |
| E004 | IncompleteRecord | WAL record truncated |
| E005 | InvalidPageType | Unknown page type value |
| E006 | PageOutOfBounds | Page index exceeds file size |
| E007 | TableNotFound | Referenced table doesn't exist |
| E008 | ColumnNotFound | Referenced column doesn't exist |
| E009 | TypeMismatch | Value type doesn't match column type |
| E010 | ReferentialIntegrity | Node referenced by relationship doesn't exist |
| E011 | DuplicatePrimaryKey | Primary key value already exists |
| E012 | DiskFull | No space left on device |
| E013 | WalReplayFailed | Could not recover from WAL |
