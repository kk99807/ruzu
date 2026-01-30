# Data Model: Relationship Table Persistence

**Feature**: 001-fix-rel-persistence
**Date**: 2026-01-29
**Phase**: 1 (Design)

## Overview

This document defines the data structures and page layouts for persisting relationship tables to disk. The design mirrors the existing node table persistence pattern, using bincode serialization of a `HashMap<String, RelTableData>` stored in a dedicated metadata page.

## Entities

### 1. RelTableData (Existing)

**Location**: [src/storage/rel_table.rs:247-256](../../src/storage/rel_table.rs#L247-L256)

**Purpose**: Serializable representation of a relationship table's in-memory state.

**Fields**:

| Field | Type | Description |
|-------|------|-------------|
| `forward_groups` | `Vec<CsrNodeGroup>` | CSR groups for forward edges (src → dst) |
| `backward_groups` | `Vec<CsrNodeGroup>` | CSR groups for backward edges (dst → src) |
| `next_rel_id` | `u64` | Next relationship ID to allocate |
| `properties` | `HashMap<u64, Vec<Value>>` | Relationship properties indexed by rel_id |

**Traits**: `Serialize`, `Deserialize`, `Clone`, `Debug`, `Default`

**Rust Definition**:
```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RelTableData {
    pub forward_groups: Vec<CsrNodeGroup>,
    pub backward_groups: Vec<CsrNodeGroup>,
    pub next_rel_id: u64,
    pub properties: HashMap<u64, Vec<Value>>,
}
```

**Size Estimation**:
- Empty table: ~40 bytes (overhead)
- Small table (100 edges, 2 node groups): ~200-300 bytes
- Medium table (1000 edges, 10 node groups): ~500-800 bytes
- **Typical MVP database**: 3-5 relationship tables = 1-3 KB total

**Invariants**:
- `forward_groups` and `backward_groups` must be consistent (every forward edge has a corresponding backward edge)
- `next_rel_id` must be greater than all existing relationship IDs
- `properties` keys must correspond to valid relationship IDs
- CSR groups are sorted by `group_id` (source node ID)

### 2. CsrNodeGroup (Existing)

**Location**: [src/storage/rel_table.rs:204-243](../../src/storage/rel_table.rs#L204-L243)

**Purpose**: Compressed Sparse Row (CSR) representation of edges from a single source node.

**Fields**:

| Field | Type | Description |
|-------|------|-------------|
| `group_id` | `u64` | Source node ID (for forward) or destination node ID (for backward) |
| `offsets` | `Vec<u64>` | CSR offset array (length = num_nodes + 1) |
| `neighbors` | `Vec<u64>` | Destination node IDs (forward) or source node IDs (backward) |
| `rel_ids` | `Vec<u64>` | Relationship IDs for each edge |

**Rust Definition**:
```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CsrNodeGroup {
    pub group_id: u64,
    pub offsets: Vec<u64>,
    pub neighbors: Vec<u64>,
    pub rel_ids: Vec<u64>,
}
```

**CSR Structure**:
```text
Example: Node 5 has edges to nodes [10, 20, 30]

group_id = 5
offsets = [0, 3]  // Node 5 has 3 edges (offsets[1] - offsets[0])
neighbors = [10, 20, 30]  // Destination node IDs
rel_ids = [100, 101, 102]  // Relationship IDs for each edge
```

**Invariants**:
- `offsets.len() == 2` for single-node group (start and end offset)
- `offsets[1] - offsets[0] == neighbors.len() == rel_ids.len()`
- `neighbors` and `rel_ids` are sorted by destination node ID (for efficient lookup)

### 3. DatabaseHeader (Modified)

**Location**: [src/storage/mod.rs:82-95](../../src/storage/mod.rs#L82-L95)

**Purpose**: Tracks locations of all metadata pages in the database file.

**Fields** (NEW field added):

| Field | Type | Before | After | Description |
|-------|------|--------|-------|-------------|
| `magic` | `[u8; 8]` | ✓ | ✓ | Magic bytes ("RUZUDB\0\0") |
| `version` | `u32` | `1` | `2` | Format version (CHANGED) |
| `database_id` | `Uuid` | ✓ | ✓ | Unique database ID |
| `catalog_range` | `PageRange` | ✓ | ✓ | Catalog pages (schemas) |
| `metadata_range` | `PageRange` | ✓ | ✓ | Node table data pages |
| `rel_metadata_range` | `PageRange` | ❌ | ✓ | **NEW**: Rel table data pages |
| `checksum` | `u32` | ✓ | ✓ | CRC32 of header |

**Rust Definition** (AFTER changes):
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseHeader {
    pub magic: [u8; 8],
    pub version: u32,  // CHANGED: 1 → 2
    pub database_id: Uuid,
    pub catalog_range: PageRange,
    pub metadata_range: PageRange,
    pub rel_metadata_range: PageRange,  // NEW FIELD
    pub checksum: u32,
}
```

**Version Migration**:
- **Version 1**: No `rel_metadata_range` field
  - On load: Deserialize without field, default to `PageRange::new(0, 0)` (empty)
  - On save: Cannot save version 2 format (error or auto-upgrade)
- **Version 2**: Has `rel_metadata_range` field
  - On load: Deserialize with field
  - On save: Serialize with field

**Backward Compatibility Strategy**:
```rust
impl DatabaseHeader {
    pub fn load(data: &[u8]) -> Result<Self> {
        match bincode::deserialize::<Self>(data) {
            Ok(header) => Ok(header),
            Err(_) => {
                // Try loading as version 1
                let v1_header: DatabaseHeaderV1 = bincode::deserialize(data)?;
                Ok(Self::from_v1(v1_header))
            }
        }
    }

    fn from_v1(v1: DatabaseHeaderV1) -> Self {
        Self {
            magic: v1.magic,
            version: 2,  // Upgrade to version 2
            database_id: v1.database_id,
            catalog_range: v1.catalog_range,
            metadata_range: v1.metadata_range,
            rel_metadata_range: PageRange::new(0, 0),  // Empty initially
            checksum: 0,  // Recomputed after construction
        }
    }
}
```

### 4. PageRange (Existing)

**Location**: [src/storage/mod.rs:52-72](../../src/storage/mod.rs#L52-L72)

**Purpose**: Describes a contiguous range of pages in the database file.

**Fields**:

| Field | Type | Description |
|-------|------|-------------|
| `start_page` | `u32` | First page in range (0-indexed) |
| `num_pages` | `u32` | Number of pages in range |

**Rust Definition**:
```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PageRange {
    pub start_page: u32,
    pub num_pages: u32,
}
```

**Methods**:
- `new(start_page, num_pages)`: Creates a page range
- `end_page()`: Returns `start_page + num_pages`
- `is_empty()`: Returns `num_pages == 0`

**For Relationship Metadata**:
```rust
// In Database::open() for new databases:
header.rel_metadata_range = PageRange::new(3, 1);  // Page 3, 1 page
```

## Page Layout

### Page Allocation (Database File Structure)

```text
┌─────────────────────────────────────────────────────────────┐
│ Page 0: DatabaseHeader                                       │
│  - Magic bytes, version, database ID                        │
│  - catalog_range: PageRange { start: 1, num: 1 }            │
│  - metadata_range: PageRange { start: 2, num: 1 }           │
│  - rel_metadata_range: PageRange { start: 3, num: 1 } (NEW) │
├─────────────────────────────────────────────────────────────┤
│ Page 1: Catalog (Table/Rel Schemas)                         │
│  - Bincode serialized Catalog structure                     │
│  - Contains TableSchema and RelTableSchema definitions       │
├─────────────────────────────────────────────────────────────┤
│ Page 2: Node Table Metadata                                 │
│  - [0..4]: length (u32 LE)                                  │
│  - [4..4+len]: bincode(HashMap<String, TableData>)          │
├─────────────────────────────────────────────────────────────┤
│ Page 3: Relationship Table Metadata (NEW)                   │
│  - [0..4]: length (u32 LE)                                  │
│  - [4..4+len]: bincode(HashMap<String, RelTableData>)       │
├─────────────────────────────────────────────────────────────┤
│ Page 4+: Reserved for future metadata expansion             │
│  - Additional node/rel table pages if single page exceeded  │
│  - Index metadata                                            │
│  - Statistics                                                │
└─────────────────────────────────────────────────────────────┘
```

### Page 3: Relationship Table Metadata (Detailed Layout)

```text
Byte Offset    Field                         Size        Description
────────────────────────────────────────────────────────────────────
0x0000         length                        4 bytes     Total size of serialized data (u32 LE)
0x0004         serialized_data               variable    bincode(HashMap<String, RelTableData>)
...
0x0FFC         (unused)                      variable    Zero-padded to page boundary (4096 bytes)
```

**Serialized Data Structure**:
```rust
// What gets serialized at offset 0x0004:
HashMap<String, RelTableData> = {
    "Knows": RelTableData {
        forward_groups: [CsrNodeGroup { group_id: 0, ... }, ...],
        backward_groups: [CsrNodeGroup { group_id: 1, ... }, ...],
        next_rel_id: 42,
        properties: { 10: [Value::Int64(2020)], ... },
    },
    "Follows": RelTableData { ... },
}
```

**Constraints**:
- `length` must be ≤ 4092 bytes (PAGE_SIZE - 4)
- If `length == 0`, no relationship tables exist (valid empty state)
- If `length > 4092`, error must be returned (future: multi-page support)

### Example: Small Database with 2 Relationship Tables

**Scenario**: Social network with `Person` nodes, `Knows` and `Follows` relationships

**Page 0 (Header)**:
```rust
DatabaseHeader {
    magic: *b"RUZUDB\0\0",
    version: 2,
    database_id: Uuid::parse_str("...").unwrap(),
    catalog_range: PageRange { start_page: 1, num_pages: 1 },
    metadata_range: PageRange { start_page: 2, num_pages: 1 },
    rel_metadata_range: PageRange { start_page: 3, num_pages: 1 },  // NEW
    checksum: 0x12345678,
}
```

**Page 1 (Catalog)**:
```rust
Catalog {
    tables: {
        "Person": TableSchema { columns: [...], ... },
    },
    relationships: {
        "Knows": RelTableSchema { from_table: "Person", to_table: "Person", ... },
        "Follows": RelTableSchema { from_table: "Person", to_table: "Person", ... },
    },
}
```

**Page 2 (Node Tables)**:
```rust
// length = 150 (example)
HashMap<String, TableData> {
    "Person": TableData {
        rows: vec![
            vec![Value::String("Alice".into())],
            vec![Value::String("Bob".into())],
        ],
        next_row_id: 2,
    },
}
```

**Page 3 (Relationship Tables)** - NEW:
```rust
// length = 280 (example)
HashMap<String, RelTableData> {
    "Knows": RelTableData {
        forward_groups: vec![
            CsrNodeGroup {
                group_id: 0,  // Alice's node ID
                offsets: vec![0, 1],
                neighbors: vec![1],  // Bob's node ID
                rel_ids: vec![0],
            },
        ],
        backward_groups: vec![
            CsrNodeGroup {
                group_id: 1,  // Bob's node ID
                offsets: vec![0, 1],
                neighbors: vec![0],  // Alice's node ID
                rel_ids: vec![0],
            },
        ],
        next_rel_id: 1,
        properties: {},
    },
    "Follows": RelTableData {
        forward_groups: vec![],
        backward_groups: vec![],
        next_rel_id: 0,
        properties: {},
    },
}
```

## State Transitions

### Database Lifecycle with Relationships

```text
┌─────────────────┐
│ Database.open() │
│  - Load header  │
│  - Load catalog │
│  - Load node    │
│    tables       │
│  - Load rel     │ ← NEW: Load rel_tables from page 3
│    tables (NEW) │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ Active Database │
│  - Execute      │
│    queries      │
│  - Modify data  │
│  - Write WAL    │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│Database.close() │
│  - Save node    │
│    tables       │
│  - Save rel     │ ← NEW: Save rel_tables to page 3
│    tables (NEW) │
│  - Flush WAL    │
│  - Close file   │
└─────────────────┘
```

### Relationship Table State Machine

```text
         ┌──────────────┐
         │ Non-Existent │
         └──────┬───────┘
                │ CREATE REL TABLE
                ▼
         ┌──────────────┐
         │    Empty     │ ← Newly created, no edges
         │ (size = 0)   │
         └──────┬───────┘
                │ INSERT INTO (first edge)
                ▼
         ┌──────────────┐
         │  Populated   │ ← Has edges, forward + backward CSR
         │ (size > 0)   │
         └──────┬───────┘
                │ Database.close()
                ▼
         ┌──────────────┐
         │  Persisted   │ ← Serialized to page 3
         │ (on disk)    │
         └──────┬───────┘
                │ Database.open()
                ▼
         ┌──────────────┐
         │   Loaded     │ ← Deserialized from page 3
         │ (in memory)  │
         └──────────────┘
```

## Validation Rules

### Save-Time Validation

**Before serializing `rel_tables` to page 3**:

1. **Size Check**:
   ```rust
   let serialized = bincode::serialize(&rel_data_map)?;
   if serialized.len() > PAGE_SIZE - 4 {
       return Err(RuzuError::StorageError(
           format!("Rel metadata too large: {} bytes (max {})",
                   serialized.len(), PAGE_SIZE - 4)
       ));
   }
   ```

2. **Schema Consistency**:
   ```rust
   for (table_name, rel_data) in &rel_data_map {
       if catalog.get_relationship(table_name).is_none() {
           return Err(RuzuError::StorageError(
               format!("Rel table '{}' has data but no schema", table_name)
           ));
       }
   }
   ```

3. **CSR Invariants**:
   ```rust
   for group in &rel_data.forward_groups {
       assert_eq!(group.offsets.len(), 2);  // Single-node group
       let num_edges = (group.offsets[1] - group.offsets[0]) as usize;
       assert_eq!(group.neighbors.len(), num_edges);
       assert_eq!(group.rel_ids.len(), num_edges);
   }
   ```

### Load-Time Validation

**After deserializing from page 3**:

1. **Length Check**:
   ```rust
   let len = u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize;
   if len > PAGE_SIZE - 4 {
       return Err(RuzuError::RelTableCorrupted(
           format!("Invalid length: {}", len)
       ));
   }
   ```

2. **Deserialization**:
   ```rust
   let rel_data_map: HashMap<String, RelTableData> =
       bincode::deserialize(&data[4..4+len])
           .map_err(|e| RuzuError::RelTableLoadError(format!("{}", e)))?;
   ```

3. **Schema Match**:
   ```rust
   for table_name in rel_data_map.keys() {
       if catalog.get_relationship(table_name).is_none() {
           return Err(RuzuError::RelTableCorrupted(
               format!("Rel table '{}' data exists but schema is missing", table_name)
           ));
       }
   }
   ```

4. **Empty State Handling**:
   ```rust
   if len == 0 || rel_data_map.is_empty() {
       // Valid: database with no relationships
       return Ok(HashMap::new());
   }
   ```

## Integration Points

### 1. Database::open() Integration

**Location**: [src/lib.rs:126-195](../../src/lib.rs#L126-L195)

**Changes**:
```rust
pub fn open(path: &Path, config: DatabaseConfig) -> Result<Self> {
    // ... existing code: load header, catalog, node tables ...

    // NEW: Load relationship tables from page 3
    let rel_tables = Self::load_rel_table_data(&buffer_pool, &catalog, &header)?;

    // NEW: Replay WAL for relationships
    Self::replay_wal(&wal_file_path, &mut catalog, &mut tables, &mut rel_tables)?;

    Ok(Database {
        catalog,
        tables,
        rel_tables,  // CHANGED: was HashMap::new()
        // ... rest unchanged ...
    })
}
```

### 2. Database::save_all_data() Integration

**Location**: [src/lib.rs:356-405](../../src/lib.rs#L356-L405)

**Changes**:
```rust
fn save_all_data(&self) -> Result<()> {
    // ... existing code: save catalog, save node tables ...

    // NEW: Save relationship tables to page 3
    let mut rel_data_map: HashMap<String, RelTableData> = HashMap::new();
    for (table_name, rel_table) in &self.rel_tables {
        rel_data_map.insert(table_name.clone(), rel_table.to_data());
    }

    let rel_data_bytes = bincode::serialize(&rel_data_map)
        .map_err(|e| RuzuError::StorageError(format!("Failed to serialize rel data: {}", e)))?;

    if rel_data_bytes.len() > PAGE_SIZE - 4 {
        return Err(RuzuError::StorageError(
            format!("Rel metadata too large: {} bytes", rel_data_bytes.len())
        ));
    }

    // Write to page 3
    if header.rel_metadata_range.num_pages > 0 {
        let rel_page_id = PageId::new(0, header.rel_metadata_range.start_page);
        let mut rel_handle = buffer_pool.pin(rel_page_id)?;

        let data = rel_handle.data_mut();
        data[0..4].copy_from_slice(&(rel_data_bytes.len() as u32).to_le_bytes());
        data[4..4 + rel_data_bytes.len()].copy_from_slice(&rel_data_bytes);
    }

    Ok(())
}
```

### 3. WAL Replay Integration

**Location**: [src/lib.rs:197-270](../../src/lib.rs#L197-L270)

**Changes**:
```rust
fn replay_wal(
    wal_path: &Path,
    catalog: &mut Catalog,
    tables: &mut HashMap<String, Arc<NodeTable>>,
    rel_tables: &mut HashMap<String, Arc<RelTable>>,  // NEW PARAMETER
) -> Result<()> {
    // ... existing code: read WAL, find committed transactions ...

    for record in committed_records {
        match record.data {
            WalData::CreateRel { name, from_table, to_table } => {
                // Existing: add schema to catalog
                catalog.add_relationship(...)?;

                // NEW: Create empty RelTable instance
                if let Some(schema) = catalog.get_relationship(&name) {
                    rel_tables.insert(name.clone(), Arc::new(RelTable::new(schema)));
                }
            }

            WalData::InsertRel { table, src, dst, props } => {
                // NEW: Insert into rel_tables (previously failed because empty)
                if let Some(rel_table) = rel_tables.get_mut(&table) {
                    Arc::make_mut(rel_table).insert(src, dst, props)?;
                }
            }

            // ... existing CreateTable, InsertNode cases unchanged ...
        }
    }

    Ok(())
}
```

## Summary

**Key Design Decisions**:

1. ✅ **Page Allocation**: Use page 3 for relationship metadata (mirrors page 2 for node tables)
2. ✅ **Serialization Format**: bincode of `HashMap<String, RelTableData>` with length prefix
3. ✅ **Header Extension**: Add `rel_metadata_range` field, bump version 1 → 2
4. ✅ **Error Handling**: Fail-fast on corruption, validate at save and load time
5. ✅ **WAL Integration**: Initialize `rel_tables` during `CreateRel` replay

**No Breaking Changes to Existing Features**:
- Node table persistence unchanged
- Catalog persistence unchanged
- WAL format unchanged (just handling existing `CreateRel`/`InsertRel` records properly)

**Implementation Scope**:
- ~100 LOC in `src/lib.rs`
- ~10 LOC in `src/storage/mod.rs` (DatabaseHeader)
- ~10 LOC in `src/error.rs` (error variants)
- No changes to `src/storage/rel_table.rs` (already has `to_data()`/`from_data()`)

**Ready for Phase 2 (Tasks Generation)**.
