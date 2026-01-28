# Quickstart: Optimize Peak Memory During CSV Import

**Feature**: 004-optimize-csv-memory
**Date**: 2025-12-07

## Prerequisites

- Rust 1.75+ (stable)
- Existing ruzu development environment
- DHAT profiler (for memory testing)

## Setup

```bash
# Clone and checkout feature branch
git clone <repo>
cd ruzu
git checkout 004-optimize-csv-memory

# Verify baseline tests pass
cargo test

# Establish baseline benchmarks
cargo bench --bench csv_benchmark
```

## Development Workflow

### 1. Run Memory Profiler (Baseline)

```bash
# Build with DHAT profiler
cargo build --release --features dhat-heap

# Run memory test (creates dhat-heap.json)
./target/release/memory_profile_test

# Analyze results
# Look for: Total bytes at peak, allocation call stacks
```

### 2. Implement Changes (TDD)

Following Red-Green-Refactor:

```bash
# RED: Write failing test
cargo test test_streaming_memory_bounded -- --nocapture

# GREEN: Implement minimal code
# Edit src/storage/csv/node_loader.rs

# Run test
cargo test test_streaming_memory_bounded

# REFACTOR: Clean up
cargo clippy
cargo fmt
```

### 3. Verify Memory Bounds

```bash
# After implementation, verify memory stays bounded
cargo build --release --features dhat-heap
./target/release/memory_profile_test

# Target: Peak memory < 500MB for 1GB input
```

### 4. Verify Throughput

```bash
# Run CSV benchmark
cargo bench --bench csv_benchmark

# Targets:
# - Node import: ≥ 7M nodes/sec (80% of 8.9M baseline)
# - Edge import: ≥ 3M edges/sec (80% of 3.8M baseline)
```

## Key Files to Modify

### Phase 1: Buffer Infrastructure

| File | Change |
|------|--------|
| `src/storage/csv/mod.rs` | Add `StreamingConfig`, export new types |
| `src/storage/csv/buffer.rs` | NEW: `RowBuffer` implementation |

### Phase 2: Batch Insert APIs

| File | Change |
|------|--------|
| `src/storage/table.rs` | Add `insert_batch()` method |
| `src/storage/rel_table.rs` | Add `insert_batch()` method |

### Phase 3: Streaming Loaders

| File | Change |
|------|--------|
| `src/storage/csv/node_loader.rs` | Add `load_streaming()` |
| `src/storage/csv/rel_loader.rs` | Add `load_streaming()` |

### Phase 4: Integration

| File | Change |
|------|--------|
| `src/lib.rs` | Update `import_nodes()` and `import_relationships()` |

## Testing Strategy

### Unit Tests

```rust
// tests/unit/buffer_tests.rs

#[test]
fn test_row_buffer_recycling() {
    let mut buffer = RowBuffer::new(1000, 5);

    // Fill buffer
    for i in 0..1000 {
        buffer.push(vec![Value::Int64(i as i64)]).unwrap();
    }

    // Take rows
    let rows = buffer.take();
    assert_eq!(rows.len(), 1000);

    // Buffer should be reusable without reallocation
    assert_eq!(buffer.len(), 0);
    assert!(buffer.rows.capacity() >= 1000); // Capacity preserved
}

#[test]
fn test_row_buffer_returns_full() {
    let mut buffer = RowBuffer::new(10, 5);

    for _ in 0..10 {
        buffer.push(vec![Value::Null]).unwrap();
    }

    assert!(buffer.is_full());
    assert!(buffer.push(vec![Value::Null]).is_err());
}
```

### Integration Tests

```rust
// tests/integration/streaming_import_tests.rs

#[test]
fn test_streaming_import_bounded_memory() {
    // Create large test CSV (100MB+)
    let csv_path = create_test_csv(1_000_000);  // 1M rows

    let db = Database::new_temp()?;
    db.create_node_table("Person", &schema)?;

    let config = CsvImportConfig {
        streaming: StreamingConfig {
            batch_size: 100_000,
            streaming_enabled: true,
            ..Default::default()
        },
        ..Default::default()
    };

    let result = db.import_nodes("Person", &csv_path, &config, None)?;

    assert_eq!(result.rows_imported, 1_000_000);
    // Memory verified via DHAT profiler in separate test
}
```

### Memory Contract Tests

```rust
// tests/contract/memory_contract_tests.rs

#[test]
#[cfg(feature = "dhat-heap")]
fn test_mc001_1gb_import_under_500mb() {
    // This test requires DHAT profiler
    // Run with: cargo test --features dhat-heap

    let _profiler = dhat::Profiler::new_heap();

    // Create 1GB test file
    let csv_path = create_test_csv_bytes(1024 * 1024 * 1024);

    let db = Database::new_temp()?;
    db.create_node_table("Test", &schema)?;

    let config = CsvImportConfig::default();  // Streaming auto-enabled
    db.import_nodes("Test", &csv_path, &config, None)?;

    let stats = dhat::Profiler::stats();
    assert!(
        stats.max_bytes < 500 * 1024 * 1024,
        "Peak memory {} exceeded 500MB",
        stats.max_bytes
    );
}
```

## Streaming Import Usage Examples

### Basic Streaming Import

The streaming import is automatically enabled for files larger than the configured threshold (default: 100MB). For explicit control:

```rust
use ruzu::{Database, CsvImportConfig};
use ruzu::storage::csv::StreamingConfig;

let mut db = Database::new();
db.execute("CREATE NODE TABLE Person(id INT64, name STRING, age INT64, PRIMARY KEY(id))")?;

// Create config with streaming enabled
let config = CsvImportConfig::new()
    .with_batch_size(100_000);  // Process 100K rows per batch

// Import with progress callback
let result = db.import_nodes(
    "Person",
    Path::new("large_dataset.csv"),
    config,
    Some(Box::new(|progress| {
        println!("Imported {} rows ({} bytes)",
            progress.rows_processed,
            progress.bytes_processed);
    })),
)?;

println!("Imported {} rows, {} skipped",
    result.rows_imported,
    result.rows_skipped);
```

### Using RowBuffer for Custom Processing

For custom streaming scenarios, use the `RowBuffer` directly:

```rust
use ruzu::storage::csv::RowBuffer;
use ruzu::types::Value;

// Create buffer with capacity for 1000 rows, 5 columns each
let mut buffer = RowBuffer::new(1000, 5);

// Fill buffer with rows
for i in 0..1000 {
    buffer.push(vec![
        Value::Int64(i as i64),
        Value::String(format!("name_{i}")),
    ])?;
}

// Check buffer state
assert!(buffer.is_full());
assert_eq!(buffer.len(), 1000);

// Take rows for processing (buffer is recycled internally)
let rows = buffer.take();
process_rows(&rows);

// Buffer is ready for reuse with same capacity
assert!(buffer.is_empty());
```

### Using RowBuffer with Recycling

For maximum memory efficiency, use buffer recycling:

```rust
let mut buffer = RowBuffer::new(100_000, 10);

loop {
    // Read next batch from CSV...
    for row in csv_reader.records() {
        buffer.push_with_recycling(parse_row(&row)?)?;

        if buffer.is_full() {
            // Get rows for writing
            let rows = buffer.take_and_prepare_recycle();

            // Write batch to storage
            table.insert_batch(rows.clone(), &column_names)?;

            // Return rows for recycling (reuses Vec allocations)
            buffer.return_for_recycling(rows);
        }
    }

    // Handle remaining rows...
}
```

### StreamingConfig Options

```rust
use ruzu::storage::csv::StreamingConfig;

// Default config (100K batch, 100MB threshold)
let default_config = StreamingConfig::default();

// Custom config with builder pattern
let config = StreamingConfig::new()
    .with_batch_size(50_000)              // Smaller batches for lower memory
    .with_streaming_threshold(50 * 1024 * 1024)  // Enable at 50MB
    .with_streaming_enabled(true);

// Disable streaming (legacy mode)
let legacy_config = StreamingConfig::disabled();

// Validate configuration
config.validate()?;

// Check if streaming should be used for a file
let file_size = std::fs::metadata("data.csv")?.len();
if config.should_stream(file_size) {
    println!("Using streaming mode for {file_size} bytes");
}
```

### Batch Insert APIs

For direct batch insertion into tables:

```rust
use ruzu::types::Value;

// Get table reference
let table = db.get_table_mut("Person")?;

// Prepare batch of rows
let rows: Vec<Vec<Value>> = vec![
    vec![Value::Int64(1), Value::String("Alice".into()), Value::Int64(30)],
    vec![Value::Int64(2), Value::String("Bob".into()), Value::Int64(25)],
    // ... more rows
];

let columns = vec!["id".into(), "name".into(), "age".into()];

// Insert batch (single validation pass, pre-allocated growth)
let inserted = table.insert_batch(rows, &columns)?;
println!("Inserted {inserted} rows");
```

### Relationship Batch Insert

```rust
// For relationship tables
let rel_table = db.get_rel_table_mut("KNOWS")?;

// Batch of (src_node_id, dst_node_id, properties)
let relationships: Vec<(u64, u64, Vec<Value>)> = vec![
    (1, 2, vec![Value::Int64(2020)]),  // Person 1 knows Person 2 since 2020
    (2, 3, vec![Value::Int64(2021)]),
    // ... more relationships
];

let inserted = rel_table.insert_batch(relationships)?;
println!("Inserted {inserted} relationships");
```

## Common Issues

### Issue: Buffer not recycling

**Symptom**: Memory grows linearly despite streaming
**Cause**: Likely forgetting to call `buffer.clear()` after batch write
**Fix**: Ensure `clear()` called in write callback, or use `take_and_prepare_recycle()` + `return_for_recycling()`

### Issue: Throughput degradation > 20%

**Symptom**: Benchmark shows < 7M nodes/sec
**Cause**: Batch size too small, causing excessive storage syncs
**Fix**: Increase `batch_size` (try 200K or 500K)

### Issue: Out of memory on large files

**Symptom**: OOM error despite streaming
**Cause**: Parallel mode collecting all block results before streaming
**Fix**: Enable sequential mode for very large files, or implement streaming parallel

## Reference

- [Feature Spec](spec.md)
- [Implementation Plan](plan.md)
- [Research Notes](research.md)
- [003-optimize-csv-import spec](../003-optimize-csv-import/spec.md) - Memory analysis source
