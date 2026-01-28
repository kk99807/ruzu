# Quickstart: Persistent Storage with Edge Support

**Feature**: 002-persistent-storage
**Date**: 2025-12-06
**Audience**: Developers implementing or testing this feature

## Overview

This guide covers how to use ruzu's persistent storage features, including:
- Opening/creating databases on disk
- Creating relationship tables and edges
- Bulk importing data from CSV
- Crash recovery and WAL replay

---

## 1. Database Lifecycle

### 1.1 Creating a New Database

```rust
use ruzu::{Database, DatabaseConfig};
use std::path::Path;

fn main() -> ruzu::Result<()> {
    // Create a new database in the specified directory
    let config = DatabaseConfig::default();
    let mut db = Database::open(Path::new("./my_graph_db"), config)?;

    // Create schema
    db.execute("CREATE NODE TABLE Person(name STRING, age INT64, PRIMARY KEY(name))")?;

    // Insert data
    db.execute("CREATE (:Person {name: 'Alice', age: 25})")?;
    db.execute("CREATE (:Person {name: 'Bob', age: 30})")?;

    // Query data
    let result = db.execute("MATCH (p:Person) WHERE p.age > 20 RETURN p.name, p.age")?;
    for row in result.rows() {
        println!("{:?}", row);
    }

    // Database is automatically persisted on drop
    Ok(())
}
```

### 1.2 Reopening an Existing Database

```rust
use ruzu::{Database, DatabaseConfig};
use std::path::Path;

fn main() -> ruzu::Result<()> {
    // Open existing database - schema and data are preserved
    let mut db = Database::open(Path::new("./my_graph_db"), DatabaseConfig::default())?;

    // Previous data is still available
    let result = db.execute("MATCH (p:Person) RETURN COUNT(*)")?;
    assert_eq!(result.rows()[0].get("COUNT(*)"), Some(&ruzu::Value::Int64(2)));

    Ok(())
}
```

### 1.3 Database Configuration

```rust
use ruzu::DatabaseConfig;

let config = DatabaseConfig {
    // Buffer pool size (default: 256MB or 80% of RAM, whichever is smaller)
    buffer_pool_size: 512 * 1024 * 1024,  // 512 MB

    // Enable WAL checksums (default: true)
    wal_checksums: true,

    // Force WAL sync after each write (default: true for durability)
    wal_sync: true,

    // Read-only mode (default: false)
    read_only: false,
};
```

---

## 2. Relationship Tables

### 2.1 Creating Relationship Tables

```rust
// Create a relationship table between Person nodes
db.execute("CREATE REL TABLE KNOWS(FROM Person TO Person, since INT64)")?;

// Create a relationship table with multiple properties
db.execute("CREATE REL TABLE WORKS_AT(FROM Person TO Company, role STRING, start_date DATE)")?;
```

### 2.2 Creating Relationships

```rust
// First, create nodes
db.execute("CREATE (:Person {name: 'Alice', age: 25})")?;
db.execute("CREATE (:Person {name: 'Bob', age: 30})")?;

// Create a relationship between them
db.execute("MATCH (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'})
            CREATE (a)-[:KNOWS {since: 2020}]->(b)")?;
```

### 2.3 Querying Relationships

```rust
// Find all people Alice knows
let result = db.execute("MATCH (a:Person {name: 'Alice'})-[:KNOWS]->(friend)
                         RETURN friend.name")?;

// Find relationships with properties
let result = db.execute("MATCH (a:Person)-[k:KNOWS]->(b:Person)
                         WHERE k.since > 2015
                         RETURN a.name, b.name, k.since")?;

// Multi-hop traversal
let result = db.execute("MATCH (a:Person {name: 'Alice'})-[:KNOWS*1..3]->(friend)
                         RETURN DISTINCT friend.name")?;
```

---

## 3. Bulk CSV Import

### 3.1 Importing Nodes

**CSV File (persons.csv)**:
```csv
name,age
Alice,25
Bob,30
Charlie,35
Diana,28
```

**Rust Code**:
```rust
use ruzu::{Database, CsvImportConfig};
use std::path::Path;

fn main() -> ruzu::Result<()> {
    let mut db = Database::open(Path::new("./my_graph_db"), Default::default())?;

    // Create table schema
    db.execute("CREATE NODE TABLE Person(name STRING, age INT64, PRIMARY KEY(name))")?;

    // Import from CSV
    let config = CsvImportConfig::default();
    let result = db.import_nodes("Person", Path::new("persons.csv"), config, None)?;

    println!("Imported {} nodes", result.rows_imported);
    Ok(())
}
```

### 3.2 Importing Relationships

**CSV File (knows.csv)**:
```csv
FROM,TO,since
Alice,Bob,2020
Bob,Charlie,2018
Alice,Diana,2019
```

**Rust Code**:
```rust
// Assuming Person nodes already exist
let result = db.import_relationships("KNOWS", Path::new("knows.csv"), config, None)?;
println!("Imported {} relationships", result.rows_imported);
```

### 3.3 Import Configuration Options

```rust
use ruzu::CsvImportConfig;

let config = CsvImportConfig {
    // Field separator (default: ',')
    delimiter: ',',

    // Quote character (default: '"')
    quote: '"',

    // First row is header (default: true)
    has_header: true,

    // Skip N rows before header (default: 0)
    skip_rows: 0,

    // Enable parallel parsing (default: true)
    parallel: true,

    // Continue on parse errors (default: false)
    ignore_errors: false,

    // Rows per batch (default: 2048)
    batch_size: 2048,
};
```

### 3.4 Progress Reporting

```rust
use ruzu::ImportProgress;

let progress_callback = |progress: ImportProgress| {
    let pct = match progress.rows_total {
        Some(total) => (progress.rows_processed as f64 / total as f64) * 100.0,
        None => 0.0,
    };
    println!(
        "Progress: {:.1}% ({} rows, {} errors)",
        pct, progress.rows_processed, progress.rows_failed
    );
};

let result = db.import_nodes(
    "Person",
    Path::new("large_persons.csv"),
    config,
    Some(Box::new(progress_callback)),
)?;
```

### 3.5 Error Handling

```rust
let config = CsvImportConfig {
    ignore_errors: true,  // Continue on errors
    ..Default::default()
};

let result = db.import_nodes("Person", Path::new("data.csv"), config, None)?;

if !result.errors.is_empty() {
    println!("Import completed with {} errors:", result.errors.len());
    for error in &result.errors {
        println!("  Row {}: {}", error.row_number, error.message);
    }
}
```

---

## 4. Crash Recovery

### 4.1 Automatic WAL Replay

Crash recovery happens automatically when opening a database:

```rust
use ruzu::{Database, DatabaseConfig, StorageError};

fn main() {
    match Database::open(Path::new("./my_graph_db"), Default::default()) {
        Ok(db) => {
            println!("Database opened successfully");
            // If WAL existed, it was automatically replayed
        }
        Err(StorageError::WalReplayFailed(msg)) => {
            eprintln!("Could not recover from crash: {}", msg);
            // Manual intervention may be required
        }
        Err(e) => {
            eprintln!("Failed to open database: {}", e);
        }
    }
}
```

### 4.2 Manual Checkpoint

Force a checkpoint to minimize recovery time:

```rust
// Writes all dirty pages to disk and clears WAL
db.checkpoint()?;
```

### 4.3 Simulating Crash for Testing

```rust
#[cfg(test)]
mod tests {
    use ruzu::{Database, DatabaseConfig};
    use std::path::Path;

    #[test]
    fn test_crash_recovery() -> ruzu::Result<()> {
        let db_path = Path::new("./test_crash_db");

        // Create database and insert data
        {
            let mut db = Database::open(db_path, Default::default())?;
            db.execute("CREATE NODE TABLE Person(name STRING, age INT64, PRIMARY KEY(name))")?;
            db.execute("CREATE (:Person {name: 'Alice', age: 25})")?;

            // Drop without checkpoint to simulate crash
            std::mem::forget(db);  // Skip Drop, leaves WAL intact
        }

        // Reopen - WAL should be replayed
        {
            let db = Database::open(db_path, Default::default())?;
            let result = db.execute("MATCH (p:Person) RETURN COUNT(*)")?;
            assert_eq!(result.rows()[0].get("COUNT(*)"), Some(&ruzu::Value::Int64(1)));
        }

        Ok(())
    }
}
```

---

## 5. Buffer Pool Management

### 5.1 Monitoring Buffer Pool

```rust
let stats = db.buffer_pool_stats();
println!("Buffer pool: {}/{} pages used", stats.pages_used, stats.capacity);
println!("Hit rate: {:.1}%", stats.hit_rate * 100.0);
println!("Evictions: {}", stats.evictions);
```

### 5.2 Working with Large Datasets

When working with datasets larger than memory:

```rust
// Configure a smaller buffer pool to test eviction
let config = DatabaseConfig {
    buffer_pool_size: 64 * 1024 * 1024,  // 64 MB
    ..Default::default()
};

let mut db = Database::open(Path::new("./large_db"), config)?;

// Load 200MB of data - buffer pool will evict pages as needed
db.import_nodes("Person", Path::new("large_persons.csv"), Default::default(), None)?;

// Queries transparently load pages from disk
let result = db.execute("MATCH (p:Person) WHERE p.age > 50 RETURN COUNT(*)")?;
```

---

## 6. Error Handling

### 6.1 Common Errors

```rust
use ruzu::StorageError;

match db.execute(query) {
    Ok(result) => { /* success */ }
    Err(StorageError::TableNotFound(name)) => {
        println!("Table '{}' does not exist", name);
    }
    Err(StorageError::ReferentialIntegrity(msg)) => {
        println!("Cannot create relationship: {}", msg);
    }
    Err(StorageError::DiskFull) => {
        println!("Disk is full, free up space and retry");
    }
    Err(StorageError::CorruptedFile(msg)) => {
        println!("Database file is corrupted: {}", msg);
    }
    Err(e) => {
        println!("Unexpected error: {}", e);
    }
}
```

### 6.2 Validation Errors

```rust
// Invalid source node
let result = db.execute("MATCH (a:NonExistent {name: 'X'}), (b:Person {name: 'Bob'})
                         CREATE (a)-[:KNOWS]->(b)");
// Returns Err(StorageError::TableNotFound("NonExistent"))

// Missing required property
let result = db.execute("CREATE (:Person {age: 25})");  // Missing 'name' (primary key)
// Returns Err(StorageError::MissingPrimaryKey("name"))

// Type mismatch
let result = db.execute("CREATE (:Person {name: 123, age: 25})");  // name should be STRING
// Returns Err(StorageError::TypeMismatch { column: "name", expected: "STRING", actual: "INT64" })
```

---

## 7. Database Files

A ruzu database consists of these files:

```
my_graph_db/
├── data.ruzu          # Main database file (header, catalog, data pages)
└── wal.log            # Write-ahead log (deleted after clean shutdown)
```

### 7.1 File Locations

```rust
let db_path = Path::new("./my_graph_db");

// Main data file
let data_file = db_path.join("data.ruzu");

// WAL file (only exists if unclean shutdown)
let wal_file = db_path.join("wal.log");
```

### 7.2 Backing Up

```bash
# Stop the database or take a snapshot
cp -r ./my_graph_db ./my_graph_db_backup
```

For hot backups (while database is running):
```rust
// Force checkpoint to ensure consistency
db.checkpoint()?;

// Now safe to copy files
std::fs::copy(db_path.join("data.ruzu"), backup_path.join("data.ruzu"))?;
// Don't copy wal.log - it's cleared by checkpoint
```

---

## 8. Performance Tips

### 8.1 Batch Operations

```rust
// Slower: Individual inserts
for person in persons {
    db.execute(&format!("CREATE (:Person {{name: '{}', age: {}}})", person.name, person.age))?;
}

// Faster: Use CSV import for bulk data
db.import_nodes("Person", Path::new("persons.csv"), Default::default(), None)?;
```

### 8.2 Buffer Pool Sizing

```rust
// Rule of thumb: Set buffer pool to fit your working set
// - Query-heavy workloads: Larger is better
// - Write-heavy workloads: Moderate size with frequent checkpoints

let config = DatabaseConfig {
    // For 1GB dataset with 200MB working set
    buffer_pool_size: 256 * 1024 * 1024,  // 256 MB
    ..Default::default()
};
```

### 8.3 Checkpoint Frequency

```rust
// Checkpoint after batch operations to bound recovery time
db.import_nodes("Person", Path::new("persons.csv"), Default::default(), None)?;
db.checkpoint()?;

db.import_relationships("KNOWS", Path::new("knows.csv"), Default::default(), None)?;
db.checkpoint()?;
```

---

## Next Steps

- See [data-model.md](./data-model.md) for detailed entity definitions
- See [contracts/storage-format.md](./contracts/storage-format.md) for binary format specification
- See [research.md](./research.md) for implementation details from KuzuDB analysis
