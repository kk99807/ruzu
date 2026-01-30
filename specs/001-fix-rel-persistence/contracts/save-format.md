# Contract: Relationship Table Save Format

**Feature**: 001-fix-rel-persistence
**Version**: 2.0.0
**Status**: Stable

## Overview

This contract defines the serialization format for relationship table data persistence. The format uses bincode serialization of a `HashMap<String, RelTableData>` stored in page 3 of the database file with a 4-byte length prefix.

## Page 3 Layout Specification

### Binary Format

```text
Offset (bytes)  Field             Type      Size        Description
──────────────────────────────────────────────────────────────────────
0x0000          length            u32 LE    4 bytes     Total size of serialized data
0x0004          serialized_data   bytes     variable    bincode(HashMap<String, RelTableData>)
...             (padding)         bytes     variable    Zero-padded to 4096 bytes
```

### Length Field (offset 0x0000)

**Format**: 32-bit unsigned integer, little-endian

**Constraints**:
- MUST be ≤ 4092 bytes (PAGE_SIZE - 4)
- Value of 0 indicates empty relationship tables (valid state)
- Values > 4092 are INVALID and MUST result in load failure

**Error Handling**:
```rust
let len = u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize;
if len > PAGE_SIZE - 4 {
    return Err(RuzuError::RelTableCorrupted(
        format!("Invalid rel_table data length: {}", len)
    ));
}
```

### Serialized Data (offset 0x0004)

**Format**: bincode v1.x serialization of `HashMap<String, RelTableData>`

**Structure**:
```rust
HashMap<String, RelTableData> = {
    "relationship_table_name_1": RelTableData { ... },
    "relationship_table_name_2": RelTableData { ... },
    ...
}
```

**Serialization Method**:
```rust
use serde::{Serialize, Deserialize};
use bincode;

let rel_data_map: HashMap<String, RelTableData> = ...;
let serialized_bytes: Vec<u8> = bincode::serialize(&rel_data_map)?;
```

## RelTableData Structure Contract

### Field Definitions

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RelTableData {
    /// CSR groups for forward edges (src → dst)
    pub forward_groups: Vec<CsrNodeGroup>,

    /// CSR groups for backward edges (dst → src)
    pub backward_groups: Vec<CsrNodeGroup>,

    /// Next relationship ID to allocate
    pub next_rel_id: u64,

    /// Relationship properties indexed by rel_id
    pub properties: HashMap<u64, Vec<Value>>,
}
```

### CsrNodeGroup Structure Contract

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CsrNodeGroup {
    /// Source node ID (forward) or destination node ID (backward)
    pub group_id: u64,

    /// CSR offset array (length = num_nodes_in_group + 1)
    pub offsets: Vec<u64>,

    /// Destination node IDs (forward) or source node IDs (backward)
    pub neighbors: Vec<u64>,

    /// Relationship IDs for each edge
    pub rel_ids: Vec<u64>,
}
```

## Invariants (MUST be preserved)

### 1. CSR Consistency

For each `CsrNodeGroup`:
```rust
assert_eq!(group.offsets.len(), 2);  // Single-node group
let num_edges = (group.offsets[1] - group.offsets[0]) as usize;
assert_eq!(group.neighbors.len(), num_edges);
assert_eq!(group.rel_ids.len(), num_edges);
```

**Rationale**: Ensures CSR structure is valid and prevents out-of-bounds access.

### 2. Bidirectional Symmetry

For each edge (src, dst, rel_id) in `forward_groups`, there MUST exist a corresponding entry (dst, src, rel_id) in `backward_groups`.

```rust
// Pseudo-validation code
for forward_group in forward_groups {
    for i in 0..forward_group.neighbors.len() {
        let src = forward_group.group_id;
        let dst = forward_group.neighbors[i];
        let rel_id = forward_group.rel_ids[i];

        // Must find matching backward edge
        assert!(backward_groups.contains_edge(dst, src, rel_id));
    }
}
```

**Rationale**: Graph traversals require bidirectional access (forward and backward).

### 3. Relationship ID Monotonicity

```rust
assert!(rel_table.next_rel_id > max(rel_table.properties.keys()));
```

**Rationale**: Prevents ID collisions when inserting new relationships.

### 4. Schema Consistency

Every key in `HashMap<String, RelTableData>` MUST correspond to a `RelTableSchema` entry in the catalog.

```rust
for table_name in rel_data_map.keys() {
    assert!(catalog.get_relationship(table_name).is_some());
}
```

**Rationale**: Data without schema is orphaned and unqueryable.

## Save Operation Contract

### Function Signature

```rust
impl Database {
    fn save_all_data(&self) -> Result<()>;
}
```

### Preconditions

- `self.buffer_pool` is Some (not in-memory mode)
- `self.header.rel_metadata_range.num_pages > 0`
- All `rel_tables` are in valid state (invariants hold)

### Algorithm

```rust
fn save_all_data(&self) -> Result<()> {
    // 1. Extract data from all rel_tables
    let mut rel_data_map: HashMap<String, RelTableData> = HashMap::new();
    for (table_name, rel_table) in &self.rel_tables {
        rel_data_map.insert(table_name.clone(), rel_table.to_data());
    }

    // 2. Serialize with bincode
    let rel_data_bytes = bincode::serialize(&rel_data_map)
        .map_err(|e| RuzuError::StorageError(
            format!("Failed to serialize rel_table data: {}", e)
        ))?;

    // 3. Validate size
    if rel_data_bytes.len() > PAGE_SIZE - 4 {
        return Err(RuzuError::StorageError(
            format!(
                "Relationship metadata too large ({} bytes). Maximum {} bytes.",
                rel_data_bytes.len(),
                PAGE_SIZE - 4
            )
        ));
    }

    // 4. Write to page 3
    let buffer_pool = self.buffer_pool.as_ref().unwrap();
    let header = self.header.as_ref().unwrap();

    let rel_page_id = PageId::new(0, header.rel_metadata_range.start_page);
    let mut rel_handle = buffer_pool.pin(rel_page_id)?;

    let data = rel_handle.data_mut();

    // Write length prefix
    data[0..4].copy_from_slice(&(rel_data_bytes.len() as u32).to_le_bytes());

    // Write serialized data
    data[4..4 + rel_data_bytes.len()].copy_from_slice(&rel_data_bytes);

    // Zero-pad remaining bytes (optional but recommended)
    data[4 + rel_data_bytes.len()..].fill(0);

    Ok(())
}
```

### Postconditions

- Page 3 contains valid serialized data
- Length prefix correctly reflects data size
- All relationship data is persisted
- No data loss

### Error Conditions

| Error | Condition | Error Type |
|-------|-----------|------------|
| No buffer pool | `buffer_pool.is_none()` | `StorageError("No buffer pool in in-memory mode")` |
| Metadata too large | `serialized.len() > 4092` | `StorageError("Relationship metadata too large...")` |
| Serialization failure | bincode error | `StorageError("Failed to serialize rel_table data...")` |
| Page pin failure | Buffer pool full | `BufferPoolError` |

## Example Save Scenarios

### Scenario 1: Empty Database (No Relationships)

**Input**:
```rust
rel_tables: HashMap::new()  // Empty
```

**Output (Page 3)**:
```text
[0x00, 0x00, 0x00, 0x00]  // length = 0
[0x00, 0x00, ..., 0x00]  // padding (4092 zero bytes)
```

**Interpretation**: Valid state, no relationships to persist.

### Scenario 2: Single Relationship Table (Empty)

**Input**:
```rust
rel_tables: {
    "Knows": RelTable {
        forward_groups: vec![],
        backward_groups: vec![],
        next_rel_id: 0,
        properties: HashMap::new(),
    }
}
```

**Output (Page 3)**:
```text
[0x28, 0x00, 0x00, 0x00]  // length = 40 bytes (example)
[...bincode serialized data...]  // HashMap with one entry
[0x00, 0x00, ..., 0x00]  // padding
```

### Scenario 3: Multiple Relationships with Data

**Input**:
```rust
rel_tables: {
    "Knows": RelTable { /* 100 edges */ },
    "Follows": RelTable { /* 50 edges */ },
}
```

**Output (Page 3)**:
```text
[0xE8, 0x01, 0x00, 0x00]  // length = 488 bytes (example)
[...bincode serialized data with 2 entries...]
[0x00, 0x00, ..., 0x00]  // padding
```

## Versioning

### Version 2 (Current)

**DatabaseHeader Changes**:
- Added `rel_metadata_range: PageRange` field
- Bumped `version` from 1 to 2

**Backward Compatibility**:
- Version 1 databases MUST be readable (upgrade on load)
- Version 1 databases have `rel_metadata_range = PageRange::new(0, 0)` (empty)
- On save, version 1 databases are upgraded to version 2

### Migration from Version 1

```rust
impl DatabaseHeader {
    fn from_v1(v1: DatabaseHeaderV1) -> Self {
        Self {
            magic: v1.magic,
            version: 2,  // Upgrade
            database_id: v1.database_id,
            catalog_range: v1.catalog_range,
            metadata_range: v1.metadata_range,
            rel_metadata_range: PageRange::new(0, 0),  // Default: no rel data
            checksum: 0,  // Recomputed
        }
    }
}
```

## Testing Requirements

### Contract Tests

1. **Empty Database Save/Load**
   - Save empty `rel_tables` HashMap
   - Verify page 3 has length = 0
   - Load and verify empty HashMap returned

2. **Single Rel Table Save/Load**
   - Save one rel table with 10 edges
   - Verify byte-level format matches specification
   - Load and verify equality with original

3. **Multiple Rel Tables Save/Load**
   - Save 3 rel tables with varying sizes
   - Verify serialized size < 4092 bytes
   - Load and verify all tables present

4. **Size Limit Validation**
   - Create rel_tables that exceed 4092 bytes
   - Verify save returns error
   - Verify error message includes size information

5. **CSR Invariant Preservation**
   - Save rel table with complex CSR structure
   - Load and verify offsets, neighbors, rel_ids are intact
   - Verify forward/backward symmetry maintained

6. **Version 1 → Version 2 Migration**
   - Create version 1 database (no rel_metadata_range)
   - Open with version 2 code
   - Verify rel_tables loads as empty
   - Save and verify upgrades to version 2

## References

- Implementation: [src/lib.rs:356-405](../../src/lib.rs#L356-L405) (save_all_data)
- Data structures: [src/storage/rel_table.rs:247-256](../../src/storage/rel_table.rs#L247-L256) (RelTableData)
- bincode documentation: https://github.com/bincode-org/bincode
- Page layout: [data-model.md](../data-model.md)
