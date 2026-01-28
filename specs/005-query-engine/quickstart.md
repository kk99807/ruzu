# Query Engine Quickstart

**Feature**: 005-query-engine
**Date**: 2025-12-07

This guide helps you get started with ruzu's query engine features.

---

## Prerequisites

- Rust 1.75+ installed
- ruzu Phase 1 complete (persistent storage working)

## Installation

Add the following to your `Cargo.toml`:

```toml
[dependencies]
ruzu = "0.1"
```

For development, clone the repo and build:

```bash
git clone https://github.com/your-org/ruzu
cd ruzu
git checkout 005-query-engine
cargo build --release
```

---

## Basic Usage

### Creating a Database

```rust
use ruzu::{Database, DatabaseConfig};
use std::path::Path;

// In-memory database
let mut db = Database::new();

// Or persistent database
let mut db = Database::open(
    Path::new("./my_database"),
    DatabaseConfig::default(),
)?;
```

### Defining Schema

```rust
// Create a node table
db.execute("CREATE NODE TABLE Person(
    name STRING,
    age INT64,
    city STRING,
    PRIMARY KEY(name)
)")?;

// Create a relationship table
db.execute("CREATE REL TABLE KNOWS(
    FROM Person TO Person,
    since INT64
)")?;
```

### Inserting Data

```rust
// Insert nodes
db.execute("CREATE (:Person {name: 'Alice', age: 30, city: 'NYC'})")?;
db.execute("CREATE (:Person {name: 'Bob', age: 25, city: 'SF'})")?;
db.execute("CREATE (:Person {name: 'Charlie', age: 35, city: 'NYC'})")?;

// Create relationships
db.execute("
    MATCH (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'})
    CREATE (a)-[:KNOWS {since: 2020}]->(b)
")?;
```

### Bulk Import from CSV

```rust
// Import nodes from CSV
db.execute("COPY Person FROM 'persons.csv'")?;

// Import relationships
db.execute("COPY KNOWS FROM 'relationships.csv'")?;
```

---

## Query Examples

### Simple Queries

```rust
// Select all persons
let result = db.execute("MATCH (p:Person) RETURN p.name, p.age")?;
for row in result.rows() {
    println!("{}: {}", row.get("p.name"), row.get("p.age"));
}

// Filter with WHERE
let result = db.execute("
    MATCH (p:Person)
    WHERE p.age > 25
    RETURN p.name, p.city
")?;
```

### Relationship Traversal

```rust
// Single-hop traversal
let result = db.execute("
    MATCH (a:Person)-[:KNOWS]->(b:Person)
    RETURN a.name, b.name
")?;

// With filter on both ends
let result = db.execute("
    MATCH (a:Person {city: 'NYC'})-[:KNOWS]->(b:Person)
    WHERE b.age > 30
    RETURN a.name, b.name
")?;
```

### Multi-Hop Paths (Phase 2)

```rust
// Friends of friends (2 hops)
let result = db.execute("
    MATCH (a:Person)-[:KNOWS*2]->(c:Person)
    WHERE a.name = 'Alice'
    RETURN c.name
")?;

// Variable-length paths (1 to 3 hops)
let result = db.execute("
    MATCH (a:Person)-[:KNOWS*1..3]->(b:Person)
    WHERE a.name = 'Alice'
    RETURN b.name
")?;
```

### Aggregations

```rust
// Count all persons
let result = db.execute("MATCH (p:Person) RETURN COUNT(*)")?;

// Count by city
let result = db.execute("
    MATCH (p:Person)
    RETURN p.city, COUNT(*)
")?;

// Average age by city
let result = db.execute("
    MATCH (p:Person)
    RETURN p.city, AVG(p.age)
")?;

// Min/Max
let result = db.execute("
    MATCH (p:Person)
    RETURN MIN(p.age), MAX(p.age)
")?;
```

### Sorting and Pagination

```rust
// Order by age descending
let result = db.execute("
    MATCH (p:Person)
    RETURN p.name, p.age
    ORDER BY p.age DESC
")?;

// Top 10 oldest
let result = db.execute("
    MATCH (p:Person)
    RETURN p.name, p.age
    ORDER BY p.age DESC
    LIMIT 10
")?;

// Pagination: page 2, 10 per page
let result = db.execute("
    MATCH (p:Person)
    RETURN p.name
    ORDER BY p.name
    SKIP 10 LIMIT 10
")?;
```

### Query Plan Inspection

```rust
// See the query plan
let result = db.execute("
    EXPLAIN MATCH (p:Person)-[:KNOWS]->(f:Person)
    WHERE p.age > 25
    RETURN p.name, COUNT(f)
")?;
println!("{}", result.plan());
// Output:
// Aggregate [group_by: [p.name], agg: [COUNT(f)]]
// └── Filter [p.age > 25]
//     └── Extend [KNOWS, FORWARD]
//         └── NodeScan [Person as p]
```

---

## Advanced Features

### DataFusion Integration

ruzu uses Apache DataFusion for query execution. You can access the underlying DataFusion context for advanced use cases:

```rust
use ruzu::QueryEngine;

let engine = db.query_engine();

// Execute with custom batch size
let result = engine.execute_with_config(
    "MATCH (p:Person) RETURN p.name",
    ExecutorConfig {
        batch_size: 4096,
        memory_limit: 512 * 1024 * 1024, // 512MB
        ..Default::default()
    },
)?;
```

### Streaming Results

For large result sets, use streaming to avoid loading everything into memory:

```rust
use futures::StreamExt;

let stream = db.execute_stream("MATCH (p:Person) RETURN p.name")?;

while let Some(batch) = stream.next().await {
    let batch = batch?;
    // Process batch (Arrow RecordBatch)
    println!("Got {} rows", batch.num_rows());
}
```

### Arrow RecordBatch Access

Results are backed by Arrow RecordBatches for efficient processing:

```rust
use arrow::array::{StringArray, Int64Array};

let batches = db.execute_to_batches("
    MATCH (p:Person) RETURN p.name, p.age
")?;

for batch in batches {
    let names = batch.column(0).as_any().downcast_ref::<StringArray>().unwrap();
    let ages = batch.column(1).as_any().downcast_ref::<Int64Array>().unwrap();

    for i in 0..batch.num_rows() {
        println!("{}: {}", names.value(i), ages.value(i));
    }
}
```

---

## Type Reference

### Supported Data Types

| Type | Cypher Syntax | Rust Type | Arrow Type |
|------|---------------|-----------|------------|
| Integer | `INT64` | `i64` | `Int64` |
| String | `STRING` | `String` | `Utf8` |
| Boolean | `BOOL` | `bool` | `Boolean` |
| Float | `FLOAT64` | `f64` | `Float64` |
| Date | `DATE` | `i32` (days) | `Date32` |
| Timestamp | `TIMESTAMP` | `i64` (μs) | `Timestamp` |

### Aggregate Functions

| Function | Description | Input Types | Output Type |
|----------|-------------|-------------|-------------|
| `COUNT(*)` | Count all rows | Any | Int64 |
| `COUNT(col)` | Count non-null values | Any | Int64 |
| `SUM(col)` | Sum values | Numeric | Same as input |
| `AVG(col)` | Average value | Numeric | Float64 |
| `MIN(col)` | Minimum value | Ordered | Same as input |
| `MAX(col)` | Maximum value | Ordered | Same as input |

### Comparison Operators

| Operator | Description |
|----------|-------------|
| `=` | Equal |
| `<>` | Not equal |
| `<` | Less than |
| `<=` | Less than or equal |
| `>` | Greater than |
| `>=` | Greater than or equal |

### Logical Operators

| Operator | Description |
|----------|-------------|
| `AND` | Logical AND |
| `OR` | Logical OR |
| `NOT` | Logical NOT |

---

## Performance Tips

### 1. Use Projections

Only return the columns you need:

```rust
// Good: Only fetch name
db.execute("MATCH (p:Person) RETURN p.name")?;

// Bad: Fetch everything
db.execute("MATCH (p:Person) RETURN *")?;
```

### 2. Apply Filters Early

Put selective filters in WHERE to enable pushdown:

```rust
// Good: Filter pushed to scan
db.execute("MATCH (p:Person) WHERE p.city = 'NYC' RETURN p")?;

// Less optimal: Filter after projection
db.execute("MATCH (p:Person) RETURN p WHERE p.city = 'NYC'")?;
```

### 3. Limit Multi-Hop Paths

Set reasonable bounds for variable-length paths:

```rust
// Good: Bounded path
db.execute("MATCH (a)-[:KNOWS*1..3]->(b) RETURN b")?;

// Risky: Unbounded (uses default max of 10)
db.execute("MATCH (a)-[:KNOWS*]->(b) RETURN b")?;
```

### 4. Use LIMIT for Top-N

Don't fetch all results if you only need a few:

```rust
// Good: Early termination
db.execute("MATCH (p:Person) RETURN p ORDER BY p.age DESC LIMIT 10")?;
```

---

## Troubleshooting

### Common Errors

**"Table 'X' does not exist"**
- Check table name spelling
- Ensure CREATE NODE TABLE was executed

**"Column 'X' does not exist"**
- Check column name in schema
- Ensure property exists in table definition

**"Type mismatch"**
- Check that comparison types match (e.g., comparing STRING to INT64)
- Use explicit type casting if needed

**"Aggregate in WHERE clause"**
- Move aggregate conditions to HAVING (not yet supported)
- Or restructure query

### Debugging

Enable query plan output:

```rust
// See what the optimizer does
let result = db.execute("EXPLAIN ...")?;
println!("{}", result.plan());
```

Check execution statistics:

```rust
let result = db.execute_with_stats("MATCH ...")?;
println!("Rows scanned: {}", result.stats().rows_scanned);
println!("Time: {:?}", result.stats().execution_time);
```

---

## Next Steps

- See [API Contracts](./contracts/) for detailed API documentation
- See [Data Model](./data-model.md) for internal data structures
- Run benchmarks: `cargo bench --bench query_benchmark`
