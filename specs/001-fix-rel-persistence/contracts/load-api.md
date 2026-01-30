# Contract: Relationship Table Load API

**Feature**: 001-fix-rel-persistence
**Version**: 2.0.0
**Status**: Stable

## Overview

This contract defines the API for loading relationship table data from disk during database open operations. The load operation must deserialize `RelTableData` from page 3, validate integrity, and construct in-memory `RelTable` instances.

## API Specification

### Function Signature

```rust
impl Database {
    /// Loads relationship table data from disk.
    ///
    /// # Arguments
    ///
    /// * `buffer_pool` - Buffer pool for page access
    /// * `catalog` - Catalog containing relationship schemas
    /// * `header` - Database header with page range information
    ///
    /// # Returns
    ///
    /// * `Ok(HashMap<String, Arc<RelTable>>)` - Loaded relationship tables
    /// * `Err(RuzuError)` - If data is corrupted or cannot be loaded
    ///
    /// # Errors
    ///
    /// * `RelTableLoadError` - Deserialization failure
    /// * `RelTableCorrupted` - Invalid data format or schema mismatch
    fn load_rel_table_data(
        buffer_pool: &BufferPool,
        catalog: &Catalog,
        header: &DatabaseHeader,
    ) -> Result<HashMap<String, Arc<RelTable>>>;
}
```

### Preconditions

- `buffer_pool` is initialized and valid
- `catalog` is loaded and contains relationship schemas
- `header.rel_metadata_range` is valid (may be empty for version 1 databases)
- Page 3 exists in database file (if `rel_metadata_range.num_pages > 0`)

### Algorithm

```rust
fn load_rel_table_data(
    buffer_pool: &BufferPool,
    catalog: &Catalog,
    header: &DatabaseHeader,
) -> Result<HashMap<String, Arc<RelTable>>> {
    use storage::{PageId, RelTableData};

    // 1. Check if rel_metadata_range exists
    if header.rel_metadata_range.num_pages == 0 {
        // Version 1 database or empty database
        return Ok(HashMap::new());
    }

    // 2. Pin page 3
    let rel_page_id = PageId::new(0, header.rel_metadata_range.start_page);
    let rel_handle = buffer_pool.pin(rel_page_id)?;
    let data = rel_handle.data();

    // 3. Read length prefix
    let len = u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize;

    if len == 0 {
        // Valid: empty relationship tables
        return Ok(HashMap::new());
    }

    if len > PAGE_SIZE - 4 {
        return Err(RuzuError::RelTableCorrupted(
            format!("Invalid rel_table data length: {} (max {})", len, PAGE_SIZE - 4)
        ));
    }

    // 4. Deserialize HashMap<String, RelTableData>
    let rel_data_bytes = &data[4..4 + len];
    let rel_data_map: HashMap<String, RelTableData> =
        bincode::deserialize(rel_data_bytes)
            .map_err(|e| RuzuError::RelTableLoadError(
                format!("Failed to deserialize rel_table data: {}", e)
            ))?;

    // 5. Validate schema consistency
    for table_name in rel_data_map.keys() {
        if catalog.get_relationship(table_name).is_none() {
            return Err(RuzuError::RelTableCorrupted(
                format!(
                    "Relationship table '{}' has data but no schema in catalog",
                    table_name
                )
            ));
        }
    }

    // 6. Convert RelTableData to RelTable instances
    let mut rel_tables: HashMap<String, Arc<RelTable>> = HashMap::new();
    for (table_name, rel_data) in rel_data_map {
        if let Some(schema) = catalog.get_relationship(&table_name) {
            let rel_table = RelTable::from_data(schema, rel_data);
            rel_tables.insert(table_name, Arc::new(rel_table));
        }
    }

    // 7. Create empty RelTable for schemas not in loaded data
    for rel_name in catalog.relationship_names() {
        if !rel_tables.contains_key(rel_name) {
            if let Some(schema) = catalog.get_relationship(rel_name) {
                rel_tables.insert(rel_name.to_string(), Arc::new(RelTable::new(schema)));
            }
        }
    }

    Ok(rel_tables)
}
```

### Postconditions

- Returned HashMap contains `RelTable` for every `RelTableSchema` in catalog
- All tables with saved data are populated with edges
- Tables without saved data are empty (newly created)
- All CSR invariants are maintained (forward/backward symmetry)
- No data loss

### Error Conditions

| Error Type | Condition | Message Format |
|------------|-----------|----------------|
| `RelTableCorrupted` | `len > PAGE_SIZE - 4` | "Invalid rel_table data length: {len} (max {max})" |
| `RelTableLoadError` | bincode deserialization fails | "Failed to deserialize rel_table data: {bincode_error}" |
| `RelTableCorrupted` | Data exists but schema missing | "Relationship table '{name}' has data but no schema in catalog" |
| `BufferPoolError` | Page pin fails | (propagated from buffer pool) |

## Load Scenarios

### Scenario 1: Empty Database (New Database)

**Input**:
- `header.rel_metadata_range = PageRange::new(0, 0)` (no pages allocated)

**Behavior**:
```rust
// Early return at step 1
return Ok(HashMap::new());
```

**Output**:
- Empty HashMap

**Rationale**: New databases have no relationship data to load.

### Scenario 2: Version 1 Database (No rel_metadata_range)

**Input**:
- `header.version = 1`
- `header` does not have `rel_metadata_range` field

**Behavior** (in `Database::open`):
```rust
// Header is upgraded to version 2 during load
let header = DatabaseHeader::from_v1(v1_header);
assert_eq!(header.rel_metadata_range, PageRange::new(0, 0));

// load_rel_table_data returns empty
let rel_tables = Self::load_rel_table_data(...)?;
assert!(rel_tables.is_empty());
```

**Output**:
- Empty HashMap
- Schemas from catalog are used to create empty tables (step 7)

### Scenario 3: Empty Relationship Tables (Zero Edges)

**Input**:
- Page 3: `[0x00, 0x00, 0x00, 0x00, ...]` (length = 0)

**Behavior**:
```rust
// Early return at step 3
return Ok(HashMap::new());
```

**Output**:
- Empty HashMap (schemas will be used to create empty tables in step 7)

### Scenario 4: Single Relationship Table with Data

**Input**:
- Catalog has schema: `Knows: Person → Person`
- Page 3 contains:
  ```text
  [0x28, 0x00, 0x00, 0x00]  // length = 40 bytes
  [bincode serialized HashMap with one entry]
  ```

**Behavior**:
```rust
// Deserialize at step 4
let rel_data_map = {
    "Knows": RelTableData {
        forward_groups: [...],
        backward_groups: [...],
        next_rel_id: 10,
        properties: {...},
    }
};

// Validate at step 5 (schema exists)
// Convert at step 6
let rel_table = RelTable::from_data(schema, rel_data);
rel_tables.insert("Knows", Arc::new(rel_table));
```

**Output**:
- HashMap with one entry: `{"Knows": Arc<RelTable>}`
- `RelTable` has 10 edges (or however many in the data)

### Scenario 5: Multiple Relationship Tables

**Input**:
- Catalog has schemas: `Knows`, `Follows`, `Likes`
- Page 3 contains data for `Knows` and `Follows` only

**Behavior**:
```rust
// Step 4-6: Load Knows and Follows from page 3
rel_tables = {
    "Knows": Arc<RelTable>,
    "Follows": Arc<RelTable>,
};

// Step 7: Create empty table for Likes (schema exists but no data)
if let Some(schema) = catalog.get_relationship("Likes") {
    rel_tables.insert("Likes", Arc::new(RelTable::new(schema)));
}
```

**Output**:
- HashMap with three entries:
  - `Knows`: populated with edges
  - `Follows`: populated with edges
  - `Likes`: empty (newly created)

### Scenario 6: Corrupted Length Prefix

**Input**:
- Page 3: `[0xFF, 0x0F, 0x00, 0x00, ...]` (length = 4095, exceeds max)

**Behavior**:
```rust
// Error at step 3
return Err(RuzuError::RelTableCorrupted(
    "Invalid rel_table data length: 4095 (max 4092)"
));
```

**Output**:
- Error, database open fails

### Scenario 7: Schema Mismatch (Data Without Schema)

**Input**:
- Page 3 contains data for table `Knows`
- Catalog does NOT have schema for `Knows`

**Behavior**:
```rust
// Error at step 5
return Err(RuzuError::RelTableCorrupted(
    "Relationship table 'Knows' has data but no schema in catalog"
));
```

**Output**:
- Error, database open fails

**Rationale**: Data without schema is orphaned and unqueryable. This indicates corruption or manual catalog manipulation.

### Scenario 8: Deserialization Failure

**Input**:
- Page 3 has valid length but corrupted bincode data

**Behavior**:
```rust
// Error at step 4
return Err(RuzuError::RelTableLoadError(
    "Failed to deserialize rel_table data: <bincode error details>"
));
```

**Output**:
- Error, database open fails

## Integration with Database::open()

### Modified Database::open() Flow

```rust
pub fn open(path: &Path, config: DatabaseConfig) -> Result<Self> {
    // ... existing code: create buffer pool, load header, load catalog ...

    // Load node tables (existing)
    let tables = Self::load_table_data(&buffer_pool, &catalog, &header)?;

    // NEW: Load relationship tables
    let mut rel_tables = Self::load_rel_table_data(&buffer_pool, &catalog, &header)?;

    // Replay WAL (existing, but now with rel_tables parameter)
    Self::replay_wal(&wal_file_path, &mut catalog, &mut tables, &mut rel_tables)?;

    Ok(Database {
        catalog,
        tables,
        rel_tables,  // CHANGED: was HashMap::new()
        db_path: Some(path.to_path_buf()),
        buffer_pool: Some(buffer_pool),
        config,
        header: Some(header),
        dirty: is_new,
        wal_writer: Some(wal_writer),
        checkpointer: Checkpointer::new(),
        next_tx_id: AtomicU64::new(1),
    })
}
```

## Validation Requirements

### CSR Invariant Validation (in RelTable::from_data)

```rust
impl RelTable {
    pub fn from_data(schema: Arc<RelTableSchema>, data: RelTableData) -> Self {
        // Convert data to internal structures
        let forward_groups = data.forward_groups.into_iter()
            .map(|g| (g.group_id, g))
            .collect();

        let backward_groups = data.backward_groups.into_iter()
            .map(|g| (g.group_id, g))
            .collect();

        // Validate CSR invariants (in debug mode)
        #[cfg(debug_assertions)]
        {
            for group in forward_groups.values() {
                assert_eq!(group.offsets.len(), 2);
                let num_edges = (group.offsets[1] - group.offsets[0]) as usize;
                assert_eq!(group.neighbors.len(), num_edges);
                assert_eq!(group.rel_ids.len(), num_edges);
            }

            // Similar validation for backward_groups
        }

        Self {
            schema,
            forward_groups,
            backward_groups,
            next_rel_id: data.next_rel_id,
            properties: data.properties,
        }
    }
}
```

**Rationale**: Catches corruption early, prevents crashes from invalid CSR structures.

## Performance Characteristics

### Time Complexity

- **O(n)** where n = total number of edges across all relationship tables
- Dominated by bincode deserialization
- Single page read (page 3) is constant time

### Memory Usage

- **O(n)** heap allocation for deserialized data
- Each `RelTable` is wrapped in `Arc` for cheap cloning
- Buffer pool manages page 3 memory

### Benchmarks

**Target Performance**:
- Load 0 relationships: < 1 ms
- Load 1,000 relationships: < 5 ms
- Load 10,000 relationships: < 50 ms
- Load 100,000 relationships: < 500 ms

**Acceptance Criteria**:
- No more than 5% regression on existing `Database::open()` benchmarks
- Linear scaling with relationship count

## Error Handling Best Practices

### 1. Fail-Fast on Corruption

```rust
// DO: Return error immediately
if len > PAGE_SIZE - 4 {
    return Err(RuzuError::RelTableCorrupted(...));
}

// DON'T: Log warning and continue (silent data loss)
if len > PAGE_SIZE - 4 {
    warn!("Invalid length, using empty rel_tables");
    return Ok(HashMap::new());  // ❌ WRONG
}
```

### 2. Provide Diagnostic Information

```rust
// DO: Include details in error message
Err(RuzuError::RelTableLoadError(
    format!("Failed to deserialize rel_table data: {}. \
             This may indicate file corruption. Try restoring from backup.", e)
))

// DON'T: Generic error
Err(RuzuError::RelTableLoadError("Load failed".into()))  // ❌ WRONG
```

### 3. Distinguish Error Types

```rust
// DO: Use specific error variants
match result {
    Err(RuzuError::RelTableCorrupted(_)) => {
        // Data corruption, cannot recover
        panic!("Database corrupted");
    }
    Err(RuzuError::RelTableLoadError(_)) => {
        // Deserialization failure, possibly version mismatch
        eprintln!("Cannot load database. Check version compatibility.");
    }
    _ => {}
}
```

## Testing Requirements

### Unit Tests

1. **`test_load_empty_database`**
   - Input: `rel_metadata_range.num_pages == 0`
   - Output: Empty HashMap
   - Verify: No page read, immediate return

2. **`test_load_zero_length`**
   - Input: Page 3 with length = 0
   - Output: Empty HashMap
   - Verify: Correct handling of valid empty state

3. **`test_load_single_rel_table`**
   - Input: Page 3 with one rel table, 10 edges
   - Output: HashMap with one entry, table has 10 edges
   - Verify: Edges match input data

4. **`test_load_multiple_rel_tables`**
   - Input: Page 3 with 3 rel tables
   - Output: HashMap with 3 entries
   - Verify: All tables loaded correctly

5. **`test_load_invalid_length`**
   - Input: Page 3 with length > 4092
   - Output: `Err(RelTableCorrupted)`
   - Verify: Error message includes length

6. **`test_load_deserialization_failure`**
   - Input: Page 3 with corrupted bincode data
   - Output: `Err(RelTableLoadError)`
   - Verify: Error message includes bincode error

7. **`test_load_schema_mismatch`**
   - Input: Page 3 has data for "Knows", catalog does not
   - Output: `Err(RelTableCorrupted)`
   - Verify: Error message includes table name

8. **`test_load_creates_empty_tables`**
   - Input: Catalog has 3 schemas, page 3 has data for 2
   - Output: HashMap with 3 entries (2 populated, 1 empty)
   - Verify: Third table is empty but exists

### Integration Tests

1. **`test_database_restart_preserves_relationships`**
   - Create database, add relationships, close, reopen
   - Verify: All relationships present after reopen

2. **`test_csv_import_and_restart`**
   - Import relationships from CSV, close, reopen
   - Verify: All imported relationships present

3. **`test_version_1_to_2_migration`**
   - Open version 1 database (no rel_metadata_range)
   - Verify: rel_tables loads as empty
   - Add relationships, save, reopen
   - Verify: Relationships persist in version 2 format

## References

- Implementation: [src/lib.rs:308-354](../../src/lib.rs#L308-L354) (load_table_data pattern)
- Save format: [save-format.md](./save-format.md)
- Data model: [data-model.md](../data-model.md)
- RelTable::from_data: [src/storage/rel_table.rs:291-310](../../src/storage/rel_table.rs#L291-L310)
