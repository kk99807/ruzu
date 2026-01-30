# Research: Relationship Table Persistence

**Feature**: 001-fix-rel-persistence
**Date**: 2026-01-29
**Status**: Complete

## Overview

This document captures research findings for implementing relationship table persistence. The primary goal is to determine how to serialize/deserialize `rel_tables` alongside existing node table data without introducing new infrastructure or breaking changes.

## Research Questions

### Q1: What is the current metadata page allocation strategy?

**Decision**: Use a dedicated metadata page for relationship table data (page 3)

**Investigation**:
- Current allocation ([lib.rs:149-150](../../src/lib.rs#L149-L150)):
  - Page 0: Database header (`DatabaseHeader`)
  - Page 1: Catalog (table/relationship schemas)
  - Page 2: Node table data (`HashMap<String, TableData>`)
  - Page 3+: Available for allocation

**Rationale**:
- Following the existing pattern of one page per metadata type
- Page 2 currently stores all node tables in a single serialized `HashMap<String, TableData>`
- We should use page 3 for `HashMap<String, RelTableData>` to mirror this design
- 4KB page size provides ~4000 bytes after overhead, sufficient for metadata

**Page Size Constraints**:
- Page size: 4096 bytes
- Length prefix: 4 bytes
- Usable space: ~4092 bytes
- Current node table data on page 2: Uses bincode serialization with length prefix
- Same approach will work for relationship tables

**Alternatives Considered**:
1. ❌ Share page 2 with node tables - Rejected: Complex deserialization, harder to version independently
2. ❌ Use dynamic page allocation - Rejected: Adds complexity, current databases are small enough for single-page metadata
3. ✅ Dedicate page 3 for rel_table metadata - Chosen: Simple, mirrors node table design, easy to implement

### Q2: What serialization format should be used for RelTableData?

**Decision**: Use existing bincode serialization with `HashMap<String, RelTableData>`

**Investigation**:
- `RelTableData` already implements `Serialize` + `Deserialize` ([rel_table.rs:246-256](../../src/storage/rel_table.rs#L246-L256))
- Current node table format ([lib.rs:385-391](../../src/lib.rs#L385-L391)):
  ```rust
  let mut table_data_map: HashMap<String, TableData> = HashMap::new();
  for (table_name, table) in &self.tables {
      table_data_map.insert(table_name.clone(), table.to_data());
  }
  let table_data_bytes = bincode::serialize(&table_data_map)?;
  ```

**Rationale**:
- Consistency: Identical pattern to node tables
- Proven: Already tested and working in production code
- Simple: No new dependencies or serialization logic
- Efficient: bincode is fast and produces compact output

**Format Structure**:
```rust
// Page 3 layout:
// [0..4]: length (u32 LE)
// [4..4+length]: bincode serialized HashMap<String, RelTableData>
```

**Alternatives Considered**:
1. ❌ JSON serialization - Rejected: Larger size, slower, unnecessary
2. ❌ Custom binary format - Rejected: Premature optimization, adds maintenance burden
3. ✅ bincode with HashMap - Chosen: Matches existing pattern, proven reliable

### Q3: How should the DatabaseHeader be updated to track relationship table pages?

**Decision**: Add new `rel_metadata_range` field to `DatabaseHeader`

**Investigation**:
- Current `DatabaseHeader` structure ([storage/mod.rs:82-95](../../src/storage/mod.rs#L82-L95)):
  ```rust
  pub struct DatabaseHeader {
      pub magic: [u8; 8],
      pub version: u32,
      pub database_id: Uuid,
      pub catalog_range: PageRange,
      pub metadata_range: PageRange,  // Currently used for node tables
      pub checksum: u32,
  }
  ```

**Rationale**:
- Explicit tracking: Makes it clear where rel_table data lives
- Versioning: Enables format changes in future without breaking compatibility
- Consistency: Follows existing pattern (`catalog_range`, `metadata_range`)

**Implementation**:
```rust
pub struct DatabaseHeader {
    pub magic: [u8; 8],
    pub version: u32,
    pub database_id: Uuid,
    pub catalog_range: PageRange,
    pub metadata_range: PageRange,      // Existing: node tables (page 2)
    pub rel_metadata_range: PageRange,  // NEW: rel tables (page 3)
    pub checksum: u32,
}
```

**Backward Compatibility**:
- Version bump: Increment `CURRENT_VERSION` from 1 to 2
- Migration strategy: Old databases (version 1) will default `rel_metadata_range` to empty (0 pages)
- New databases (version 2) will allocate page 3 for relationship tables

**Alternatives Considered**:
1. ❌ Reuse `metadata_range` for both - Rejected: Ambiguous, complicates deserialization logic
2. ❌ Store metadata in catalog - Rejected: Catalog is for schemas, not data
3. ✅ Add dedicated `rel_metadata_range` field - Chosen: Clear intent, easy to implement, supports versioning

### Q4: How should WAL replay handle relationship table operations?

**Decision**: Extend existing `replay_wal()` function to handle rel_table create/insert operations

**Investigation**:
- Current WAL replay ([lib.rs:197-270](../../src/lib.rs#L197-L270)) handles:
  - `CreateTable`: Adds schema to catalog, creates empty `NodeTable`
  - `CreateRel`: Adds schema to catalog (but doesn't create `RelTable` - BUG!)
  - `InsertNode`: Inserts into `NodeTable`
  - `InsertRel`: Would insert into `RelTable` (currently broken because table not loaded)

**Findings**:
- WAL replay already reads `CreateRel` and `InsertRel` records
- But `rel_tables` HashMap is empty during replay (line 185 initializes to `HashMap::new()`)
- Need to initialize `rel_tables` in WAL replay when `CreateRel` is encountered

**Rationale**:
- Fix replay logic: Ensure `CreateRel` creates empty `RelTable`, not just schema
- Consistent with node tables: `CreateTable` in WAL creates both schema and empty table
- Complete recovery: All committed relationships must be present after replay

**Implementation Changes**:
```rust
fn replay_wal(
    wal_path: &Path,
    catalog: &mut Catalog,
    tables: &mut HashMap<String, Arc<NodeTable>>,
    rel_tables: &mut HashMap<String, Arc<RelTable>>,  // NEW PARAMETER
) -> Result<()> {
    // ... existing committed_txs analysis ...

    for record in committed_records {
        match record.data {
            // ... existing CreateTable, InsertNode cases ...

            WalData::CreateRel { name, from_table, to_table } => {
                // Add schema to catalog
                catalog.add_relationship(...)?;

                // NEW: Create empty RelTable instance
                if let Some(schema) = catalog.get_relationship(&name) {
                    rel_tables.insert(name.clone(), Arc::new(RelTable::new(schema)));
                }
            }

            WalData::InsertRel { table, src, dst, props } => {
                // NEW: Will now work because rel_tables is populated
                if let Some(table) = rel_tables.get_mut(&table) {
                    Arc::make_mut(table).insert(src, dst, props)?;
                }
            }
        }
    }
}
```

**Alternatives Considered**:
1. ❌ Skip WAL for relationships - Rejected: Data loss on crash, violates ACID guarantees
2. ❌ Separate WAL file for relationships - Rejected: Adds complexity, breaks transaction atomicity
3. ✅ Extend existing WAL replay - Chosen: Minimal changes, maintains ACID properties

### Q5: What error handling should be implemented for rel_table load failures?

**Decision**: Add explicit error variants and fail-fast on deserialization errors

**Rationale**:
- Silent failures are the root cause of this bug
- Users must know immediately if relationship data cannot be loaded
- Database should refuse to open if data is corrupted

**Error Handling Strategy**:
```rust
// In error.rs, add new variants:
pub enum RuzuError {
    // ... existing variants ...

    RelTableLoadError(String),    // Failed to load relationship table data
    RelTableCorrupted(String),    // Relationship data failed integrity check
}

// In load_rel_table_data():
fn load_rel_table_data(...) -> Result<HashMap<String, Arc<RelTable>>> {
    // Attempt to load from page 3
    if header.rel_metadata_range.num_pages == 0 {
        // Empty database or version 1 database - no rel tables to load
        return Ok(HashMap::new());
    }

    let data_page_id = PageId::new(0, header.rel_metadata_range.start_page);
    let data_handle = buffer_pool.pin(data_page_id)?;
    let data = data_handle.data();

    let len = u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize;
    if len == 0 {
        // Empty rel_tables (valid state)
        return Ok(HashMap::new());
    }

    if len > PAGE_SIZE - 4 {
        return Err(RuzuError::RelTableCorrupted(
            format!("Invalid rel_table data length: {}", len)
        ));
    }

    let rel_data_bytes = &data[4..4 + len];
    let rel_data_map: HashMap<String, RelTableData> =
        bincode::deserialize(rel_data_bytes)
            .map_err(|e| RuzuError::RelTableLoadError(
                format!("Failed to deserialize rel_table data: {}", e)
            ))?;

    // Validate against catalog schemas
    for table_name in rel_data_map.keys() {
        if catalog.get_relationship(table_name).is_none() {
            return Err(RuzuError::RelTableCorrupted(
                format!("Relationship table '{}' has data but no schema in catalog", table_name)
            ));
        }
    }

    // Convert RelTableData to RelTable instances
    let mut rel_tables = HashMap::new();
    for (table_name, rel_data) in rel_data_map {
        if let Some(schema) = catalog.get_relationship(&table_name) {
            let rel_table = RelTable::from_data(schema, rel_data);
            rel_tables.insert(table_name, Arc::new(rel_table));
        }
    }

    Ok(rel_tables)
}
```

**Error Scenarios Handled**:
1. Missing page 3 (version 1 database) → Return empty HashMap
2. Empty rel_tables (valid) → Return empty HashMap
3. Corrupted length prefix → Return error, refuse to open
4. Deserialization failure → Return error with diagnostic message
5. Schema mismatch (data without schema) → Return error, refuse to open

**Alternatives Considered**:
1. ❌ Silently skip corrupted data - Rejected: Root cause of current bug
2. ❌ Warn and continue with empty rel_tables - Rejected: Silent data loss
3. ✅ Fail-fast with explicit error - Chosen: Predictable behavior, forces user to fix corruption

## Best Practices Reference

### Rust Serialization with bincode

**Source**: Existing codebase pattern in `lib.rs` for node table persistence

**Key Patterns**:
1. Always use length prefix for variable-size data
2. Check length bounds before deserializing
3. Use `try_into().unwrap()` for fixed-size conversions (safe because size is checked)
4. Keep serialization logic symmetric (save/load should mirror each other)

**Example from node tables** ([lib.rs:385-402](../../src/lib.rs#L385-L402)):
```rust
// Save:
let table_data_map: HashMap<String, TableData> = ...;
let table_data_bytes = bincode::serialize(&table_data_map)?;
let table_data_len = table_data_bytes.len();

let data = data_handle.data_mut();
data[0..4].copy_from_slice(&(table_data_len as u32).to_le_bytes());
data[4..4 + table_data_len].copy_from_slice(&table_data_bytes);

// Load:
let len = u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize;
if len > 0 && len < PAGE_SIZE - 4 {
    let table_data_bytes = &data[4..4 + len];
    let table_data_map = bincode::deserialize::<HashMap<String, TableData>>(table_data_bytes)?;
}
```

### Database Header Versioning

**Source**: KuzuDB reference implementation (C++)

**Pattern**: Semantic versioning for file format
- **Major version** (1 → 2): Breaking changes, incompatible with older readers
- **Minor version** (2.0 → 2.1): Backward-compatible additions (new optional fields)
- **Patch version**: Bug fixes that don't affect format

**For this fix**:
- Current version: 1
- New version: 2 (adding `rel_metadata_range` field is a breaking change)
- Version 1 databases should still open (with empty rel_tables)

## Technology Choices

### No New Dependencies

**Decision**: Use only existing dependencies

**Justification**:
- `serde` + `bincode`: Already in use for node table serialization
- `parking_lot`: Already in use for `RwLock` on tables
- All necessary infrastructure exists

**Alternatives Rejected**:
- ❌ Apache Arrow for relationship storage: Overkill for MVP, adds dependency
- ❌ Custom compression (e.g., zstd): Premature optimization, metadata is small
- ❌ Memory-mapped file I/O for metadata: Unnecessary, metadata fits in single page

## Integration Patterns

### File Format Compatibility

**Strategy**: Version-based migration

```rust
// In Database::open():
let (catalog, header) = Self::load_database(&buffer_pool)?;

let rel_tables = match header.version {
    1 => {
        // Old format: no relationship table data
        // Initialize empty rel_tables from catalog schemas
        let mut rel_tables = HashMap::new();
        for rel_name in catalog.relationship_names() {
            if let Some(schema) = catalog.get_relationship(rel_name) {
                rel_tables.insert(rel_name.to_string(), Arc::new(RelTable::new(schema)));
            }
        }
        rel_tables
    }
    2 => {
        // New format: load relationship table data from page 3
        Self::load_rel_table_data(&buffer_pool, &catalog, &header)?
    }
    _ => {
        return Err(RuzuError::StorageError(
            format!("Unsupported database version: {}", header.version)
        ));
    }
};
```

### Testing Strategy

**Contract Tests** (format stability):
- Serialize a known `RelTableData` structure, verify byte-level output
- Deserialize saved bytes, verify round-trip equality
- Test version 1 → version 2 migration path

**Integration Tests** (end-to-end workflows):
- Create database, add relationships, close, reopen, query (golden path)
- Create database, close (no relationships), reopen (empty state)
- Create database, add relationships, crash (WAL only), reopen (WAL replay)
- Import CSV relationships, close, reopen, verify all data present

**Unit Tests** (individual functions):
- `load_rel_table_data()` with empty page
- `load_rel_table_data()` with valid data
- `load_rel_table_data()` with corrupted length
- `load_rel_table_data()` with schema mismatch
- `save_all_data()` with empty rel_tables
- `save_all_data()` with multiple relationship tables

## Performance Considerations

### Database Open Time

**Measurement Plan**:
- Benchmark `Database::open()` with varying relationship counts:
  - 0 relationships (baseline)
  - 1,000 relationships
  - 10,000 relationships
  - 100,000 relationships

**Expected Behavior**:
- O(n) deserialization time where n = total edges
- Should be dominated by bincode deserialization, not I/O (single page read)

**Acceptance Criteria**:
- <5% regression on existing benchmarks (node table open time)
- Linear scaling with relationship count

### Memory Usage

**Analysis**:
- Metadata fits in single page (4KB)
- Actual relationship data stored in CSR structures (separate from metadata)
- Buffer pool manages memory, not directly loaded into heap

**Concern**: Large relationship tables
- If serialized `HashMap<String, RelTableData>` exceeds 4KB, need multi-page support
- **Mitigation**: For MVP, document 4KB limit. Future work can add multi-page metadata.

**Calculation** (worst case):
- Average `RelTableData` size: ~500 bytes (forward CSR + backward CSR + properties metadata)
- Page capacity: ~4000 bytes usable
- Maximum ~8 relationship tables per database (MVP scope)
- **Conclusion**: Single page is sufficient for MVP

## Risks and Mitigations

### Risk 1: Metadata exceeds 4KB page size

**Likelihood**: Low (MVP has small databases)
**Impact**: High (database cannot save)

**Mitigation**:
1. Document limit in code comments
2. Add validation in `save_all_data()` to detect overflow
3. Return clear error message if limit exceeded
4. Future work: Implement multi-page metadata (expand `rel_metadata_range.num_pages`)

**Validation Code**:
```rust
let rel_data_bytes = bincode::serialize(&rel_data_map)?;
if rel_data_bytes.len() > PAGE_SIZE - 4 {
    return Err(RuzuError::StorageError(
        format!(
            "Relationship metadata too large ({} bytes). Maximum {} bytes. \
             Consider reducing number of relationship tables or properties.",
            rel_data_bytes.len(),
            PAGE_SIZE - 4
        )
    ));
}
```

### Risk 2: Version mismatch between code and database file

**Likelihood**: Medium (during development/deployment)
**Impact**: Medium (database cannot open)

**Mitigation**:
1. Explicit version checking in `Database::open()`
2. Clear error messages indicating required version
3. Support version 1 databases (read-only for rel_tables)
4. Document migration path in user guide

### Risk 3: WAL replay fails to restore relationships

**Likelihood**: Medium (complex logic change)
**Impact**: Critical (data loss on crash)

**Mitigation**:
1. Comprehensive integration tests for crash scenarios
2. Manual testing with simulated crashes (kill -9)
3. Verify WAL replay creates identical state to pre-crash
4. Property-based testing for WAL invariants

## Summary

All research questions resolved. Key decisions:

1. ✅ **Metadata Page**: Use page 3 for `HashMap<String, RelTableData>`
2. ✅ **Serialization**: bincode with length prefix (mirrors node tables)
3. ✅ **Header Update**: Add `rel_metadata_range: PageRange` field, bump version to 2
4. ✅ **WAL Replay**: Initialize `rel_tables` during `CreateRel` record processing
5. ✅ **Error Handling**: Fail-fast on corruption, explicit error messages

**No unknowns remain**. Ready to proceed to Phase 1 (Design).

**Estimated Implementation Effort**:
- `src/lib.rs`: ~100 LOC (3 functions modified, 1 function added)
- `src/storage/mod.rs`: ~10 LOC (DatabaseHeader field addition)
- `src/error.rs`: ~10 LOC (new error variants)
- Tests: ~200 LOC (contract + integration + unit tests)
- **Total**: ~320 LOC

**No new dependencies required**.
