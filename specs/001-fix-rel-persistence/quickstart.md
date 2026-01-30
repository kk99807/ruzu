# Quickstart: Testing Relationship Persistence

**Feature**: 001-fix-rel-persistence
**Date**: 2026-01-29
**Audience**: Developers testing the fix

## Overview

This guide demonstrates how to verify that relationship table persistence is working correctly. It covers manual testing, automated test execution, and common failure modes.

## Prerequisites

- Rust 1.75+ installed
- ruzu repository cloned to `C:\dev\ruzu`
- Basic understanding of Cypher queries

## Quick Verification

### Test 1: Basic Relationship Persistence (2 minutes)

**Verify**: Relationships survive database restart

```bash
# 1. Build the project
cargo build

# 2. Run the relationship persistence test
cargo test test_relationship_persistence -- --nocapture

# Expected output:
# test integration::test_relationship_persistence ... ok
```

**What this tests**:
- Creates database with Person nodes
- Creates Knows relationship table
- Inserts edges between nodes
- Closes and reopens database
- Queries relationships and verifies they still exist

### Test 2: CSV Import Persistence (3 minutes)

**Verify**: CSV-imported relationships survive restart

```bash
# Run the CSV import persistence test
cargo test test_csv_import_relationships_persist -- --nocapture

# Expected output:
# test integration::test_csv_import_relationships_persist ... ok
```

**What this tests**:
- Imports 1000 relationships from CSV
- Closes and reopens database
- Verifies all 1000 relationships are present

### Test 3: Crash Recovery (3 minutes)

**Verify**: WAL replay restores committed relationships after crash

```bash
# Run the crash recovery test
cargo test test_rel_wal_recovery -- --nocapture

# Expected output:
# test integration::test_rel_wal_recovery ... ok
```

**What this tests**:
- Creates relationships in a transaction
- Commits transaction
- Simulates crash (doesn't call close())
- Reopens database (triggers WAL replay)
- Verifies relationships are restored

## Manual Testing

### Scenario: Create and Query Relationships

**Step 1: Create a test database**

```rust
use ruzu::{Database, DatabaseConfig};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db_path = PathBuf::from("test_rel_persist");

    // Clean up any existing test database
    if db_path.exists() {
        std::fs::remove_dir_all(&db_path)?;
    }

    // Create new database
    let mut db = Database::open(&db_path, DatabaseConfig::default())?;

    // Create node table
    db.execute("CREATE NODE TABLE Person(name STRING, PRIMARY KEY(name))")?;

    // Create relationship table
    db.execute("CREATE REL TABLE Knows(FROM Person TO Person, since INT64)")?;

    // Insert nodes
    db.execute("CREATE (:Person {name: 'Alice'})")?;
    db.execute("CREATE (:Person {name: 'Bob'})")?;
    db.execute("CREATE (:Person {name: 'Charlie'})")?;

    // Insert relationships
    db.execute("CREATE (:Person {name: 'Alice'})-[:Knows {since: 2020}]->(:Person {name: 'Bob'})")?;
    db.execute("CREATE (:Person {name: 'Bob'})-[:Knows {since: 2021}]->(:Person {name: 'Charlie'})")?;

    println!("✓ Created database with 3 nodes and 2 relationships");

    // Query relationships
    let result = db.execute("MATCH (a:Person)-[k:Knows]->(b:Person) RETURN a.name, k.since, b.name")?;
    println!("✓ Query before close:");
    println!("{:?}", result);

    // Close database (triggers save)
    db.close()?;
    println!("✓ Database closed");

    Ok(())
}
```

**Step 2: Reopen and verify**

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db_path = PathBuf::from("test_rel_persist");

    // Reopen database
    let db = Database::open(&db_path, DatabaseConfig::default())?;
    println!("✓ Database reopened");

    // Query relationships again
    let result = db.execute("MATCH (a:Person)-[k:Knows]->(b:Person) RETURN a.name, k.since, b.name")?;
    println!("✓ Query after reopen:");
    println!("{:?}", result);

    // Verify result has 2 rows
    assert_eq!(result.rows.len(), 2, "Expected 2 relationships after restart");

    println!("✅ SUCCESS: Relationships persisted correctly!");

    Ok(())
}
```

**Expected output**:
```text
✓ Created database with 3 nodes and 2 relationships
✓ Query before close:
QueryResult { columns: ["a.name", "k.since", "b.name"], rows: [["Alice", 2020, "Bob"], ["Bob", 2021, "Charlie"]] }
✓ Database closed
✓ Database reopened
✓ Query after reopen:
QueryResult { columns: ["a.name", "k.since", "b.name"], rows: [["Alice", 2020, "Bob"], ["Bob", 2021, "Charlie"]] }
✅ SUCCESS: Relationships persisted correctly!
```

## Running Automated Tests

### All Tests

```bash
# Run all tests (unit + integration + contract)
cargo test

# Expected: All tests pass, ~200+ tests
```

### Relationship-Specific Tests

```bash
# Run only relationship persistence tests
cargo test rel_persist

# Expected output:
# test integration::test_relationship_persistence ... ok
# test integration::test_rel_wal_recovery ... ok
# test contract::test_rel_table_save_load ... ok
# test contract::test_rel_table_empty ... ok
# ...
```

### Contract Tests Only

```bash
# Run only contract tests for save/load format
cargo test --test contract_tests test_rel_persistence_format

# Expected: Tests verify byte-level format stability
```

### Integration Tests Only

```bash
# Run only integration tests
cargo test --test integration_tests

# Expected: Tests verify end-to-end workflows
```

### With Detailed Output

```bash
# Show println! output from tests
cargo test test_relationship_persistence -- --nocapture

# Useful for debugging test failures
```

## Benchmarks

### Running Performance Tests

```bash
# Run relationship persistence benchmarks
cargo bench --bench rel_persist_benchmark

# Expected output:
# rel_persist/load_empty     time:   [0.5 ms 0.6 ms 0.7 ms]
# rel_persist/load_1k        time:   [3.2 ms 3.5 ms 3.8 ms]
# rel_persist/load_10k       time:   [28 ms 32 ms 36 ms]
# rel_persist/save_1k        time:   [2.8 ms 3.1 ms 3.4 ms]
```

**Acceptance Criteria**:
- Load time scales linearly with number of relationships
- No more than 5% regression on existing benchmarks

### Comparing Before/After Fix

```bash
# Checkout before fix
git checkout <before-fix-commit>
cargo bench --bench rel_persist_benchmark > before.txt

# Checkout after fix
git checkout 001-fix-rel-persistence
cargo bench --bench rel_persist_benchmark > after.txt

# Compare results
cargo benchcmp before.txt after.txt
```

## Common Failure Modes

### Failure 1: Relationships Lost After Restart

**Symptom**: Query returns empty result after reopening database

**Diagnosis**:
```rust
// Check if rel_tables is being populated
println!("rel_tables: {:?}", db.rel_tables.keys().collect::<Vec<_>>());
// If empty, load function is not being called
```

**Root Cause**: `Database::open()` not calling `load_rel_table_data()`

**Fix**: Ensure line in [lib.rs:182](../../src/lib.rs#L182) is changed from:
```rust
rel_tables: HashMap::new(),  // ❌ WRONG
```
to:
```rust
rel_tables: Self::load_rel_table_data(&buffer_pool, &catalog, &header)?,  // ✅ CORRECT
```

### Failure 2: Deserialization Error on Open

**Symptom**: Database open fails with "Failed to deserialize rel_table data"

**Diagnosis**:
```bash
# Check page 3 contents
hexdump -C test_db/data.ruzu | head -n 100
```

**Root Cause**: Corrupted bincode data or version mismatch

**Fix**: Delete database and recreate, or implement migration logic

### Failure 3: Schema Mismatch Error

**Symptom**: Error "Relationship table 'Knows' has data but no schema in catalog"

**Diagnosis**:
```rust
// Check catalog
println!("Catalog relationships: {:?}", catalog.relationship_names());
// If "Knows" is missing, catalog was corrupted
```

**Root Cause**: Catalog page was not saved or is corrupted

**Fix**: Ensure `save_all_data()` saves catalog before rel_tables

### Failure 4: WAL Replay Doesn't Restore Relationships

**Symptom**: After crash simulation, relationships are missing

**Diagnosis**:
```rust
// Check WAL contents
let wal_reader = WalReader::open(&wal_path)?;
for record in wal_reader {
    println!("WAL record: {:?}", record);
}
// Check if InsertRel records are present
```

**Root Cause**: `replay_wal()` not initializing `rel_tables` HashMap

**Fix**: Ensure `replay_wal()` signature includes `rel_tables` parameter

## Debugging Tips

### Enable Logging

```rust
use env_logger;

fn main() {
    env_logger::init();
    // ... test code ...
}
```

```bash
# Run with debug logging
RUST_LOG=debug cargo test test_relationship_persistence -- --nocapture
```

### Inspect Database File

```bash
# View database header (page 0)
hexdump -C test_db/data.ruzu -n 4096

# View catalog (page 1)
hexdump -C test_db/data.ruzu -s 4096 -n 4096

# View node table data (page 2)
hexdump -C test_db/data.ruzu -s 8192 -n 4096

# View rel table data (page 3) - NEW
hexdump -C test_db/data.ruzu -s 12288 -n 4096
```

### Check Serialized Size

```rust
// In save_all_data()
let rel_data_bytes = bincode::serialize(&rel_data_map)?;
println!("Rel table data size: {} bytes (max 4092)", rel_data_bytes.len());
```

### Verify Page 3 Allocation

```rust
// After Database::open()
println!("Header rel_metadata_range: {:?}", header.rel_metadata_range);
// Should be PageRange { start_page: 3, num_pages: 1 }
```

## Version Migration Testing

### Test Version 1 → Version 2 Upgrade

```bash
# 1. Create version 1 database (checkout old code)
git checkout main  # Assuming main is version 1
cargo build
# Create database with node tables only (no rel_tables)

# 2. Switch to version 2 code
git checkout 001-fix-rel-persistence
cargo build

# 3. Open version 1 database with version 2 code
cargo test test_version_migration -- --nocapture

# Expected: Database opens successfully, rel_tables is empty
```

## Success Criteria Checklist

- [ ] All relationships present after database restart
- [ ] CSV-imported relationships persist after restart
- [ ] WAL replay restores committed relationships
- [ ] Zero silent data loss (failures produce errors)
- [ ] Query results identical before and after restart
- [ ] Benchmarks show no significant regression (<5%)
- [ ] Empty relationship tables handled correctly
- [ ] Version 1 databases open successfully in version 2

## Next Steps

After verifying these tests pass:

1. Review [data-model.md](./data-model.md) for implementation details
2. Check [contracts/save-format.md](./contracts/save-format.md) for serialization format
3. Check [contracts/load-api.md](./contracts/load-api.md) for load API specification
4. Run full test suite: `cargo test --all-features`
5. Run benchmarks: `cargo bench`
6. Review code with `cargo clippy`

## Troubleshooting

**Q: Test fails with "No buffer pool in in-memory mode"**

A: In-memory mode doesn't persist data. Ensure test creates database with file path:
```rust
let db = Database::open(Path::new("test_db"), config)?;  // ✅ File mode
// NOT:
let db = Database::new(config)?;  // ❌ In-memory mode
```

**Q: Test fails with "Relationship metadata too large"**

A: Database has too many relationships for single page (>4092 bytes). For MVP, document limitation. Future work: multi-page metadata.

**Q: Test fails intermittently**

A: May be a timing issue with file I/O. Add explicit `db.flush()` or `db.close()` before reopening.

## Contact

For issues with this feature, see:
- Implementation plan: [plan.md](./plan.md)
- Feature spec: [spec.md](./spec.md)
- GitHub issues: https://github.com/yourorg/ruzu/issues (if applicable)
