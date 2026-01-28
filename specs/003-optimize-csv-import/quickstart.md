# Quickstart: Optimized CSV Import

**Feature**: 003-optimize-csv-import
**Date**: 2025-12-06

This guide helps developers understand and use the optimized CSV import functionality.

---

## Overview

The optimized CSV import provides:
- **Parallel parsing**: 2-4x faster on multi-core systems
- **Memory-mapped I/O**: Reduced I/O overhead for large files
- **Throughput metrics**: Real-time rows/sec and ETA in progress callbacks
- **Batch writes**: Reduced storage I/O

All optimizations are backward compatible with existing code.

---

## Quick Examples

### Basic Import (Uses Defaults)

```rust
use ruzu::catalog::{ColumnDef, NodeTableSchema};
use ruzu::storage::csv::{CsvImportConfig, NodeLoader};
use ruzu::types::DataType;
use std::sync::Arc;
use std::path::Path;

// Create schema
let schema = Arc::new(NodeTableSchema::new(
    "Person".to_string(),
    vec![
        ColumnDef::new("id".to_string(), DataType::Int64).unwrap(),
        ColumnDef::new("name".to_string(), DataType::String).unwrap(),
    ],
    vec!["id".to_string()],
).unwrap());

// Default config enables parallel processing automatically
let config = CsvImportConfig::default();
let loader = NodeLoader::new(schema, config);

// Import CSV
let (rows, result) = loader.load(Path::new("people.csv"), None).unwrap();
println!("Imported {} rows", result.rows_imported);
```

### Import with Progress Callback

```rust
use std::sync::Arc;
use ruzu::storage::csv::{CsvImportConfig, NodeLoader, ImportProgress};

let config = CsvImportConfig::default();
let loader = NodeLoader::new(schema, config);

let callback = Box::new(|progress: ImportProgress| {
    if let (Some(throughput), Some(eta)) = (progress.throughput(), progress.eta_seconds()) {
        println!(
            "Progress: {:.1}% | {:.0} rows/sec | ETA: {:.1}s",
            progress.percent_complete().unwrap_or(0.0) * 100.0,
            throughput,
            eta
        );
    }
});

let (rows, result) = loader.load(Path::new("people.csv"), Some(callback)).unwrap();
```

### Force Sequential Processing

```rust
// Use sequential mode for files with quoted newlines
let config = CsvImportConfig::new()
    .with_parallel(false);

let loader = NodeLoader::new(schema, config);
```

### Maximum Performance Configuration

```rust
let config = CsvImportConfig::new()
    .with_parallel(true)
    .with_num_threads(8)           // Use 8 threads
    .with_mmap(true)               // Use memory mapping
    .with_mmap_threshold(50_000_000)  // Mmap files > 50MB
    .with_batch_size(4096)         // Larger batches
    .with_ignore_errors(true);     // Don't stop on errors
```

---

## Configuration Options

### Parallelism Settings

| Option | Default | Description |
|--------|---------|-------------|
| `parallel` | `true` | Enable parallel parsing |
| `num_threads` | `None` (auto) | Number of worker threads |
| `block_size` | 256KB | File split size for parallel |

### I/O Settings

| Option | Default | Description |
|--------|---------|-------------|
| `use_mmap` | `true` | Use memory-mapped I/O |
| `mmap_threshold` | 100MB | Minimum file size for mmap |

### Processing Settings

| Option | Default | Description |
|--------|---------|-------------|
| `batch_size` | 2048 | Rows per batch |
| `ignore_errors` | `false` | Continue on parse errors |
| `intern_strings` | `false` | Deduplicate string values |

---

## Progress Metrics

The `ImportProgress` struct now includes throughput data:

```rust
pub struct ImportProgress {
    // Counts
    pub rows_processed: u64,
    pub rows_total: Option<u64>,
    pub rows_failed: u64,
    pub bytes_read: u64,
    pub errors: Vec<ImportError>,

    // Throughput methods
    fn throughput(&self) -> Option<f64>;        // rows/sec
    fn smoothed_throughput(&self) -> Option<f64>; // smoothed average
    fn eta_seconds(&self) -> Option<f64>;       // estimated time remaining
    fn elapsed(&self) -> Option<Duration>;      // time since start
}
```

### Progress Callback Best Practices

```rust
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

// Thread-safe progress tracking
let progress_counter = Arc::new(AtomicU64::new(0));
let counter_clone = Arc::clone(&progress_counter);

let callback = Box::new(move |p: ImportProgress| {
    counter_clone.store(p.rows_processed, Ordering::SeqCst);
});

// After import
let final_count = progress_counter.load(Ordering::SeqCst);
```

---

## When to Disable Parallel Processing

Disable parallel mode (`with_parallel(false)`) when:

1. **CSV has quoted newlines**: Parallel mode cannot handle multi-line quoted fields
2. **Very small files**: Files < 10MB won't benefit from parallelism overhead
3. **Single-core systems**: No parallelism benefit
4. **Debugging**: Sequential mode is easier to debug

```rust
// Check if file might have quoted newlines
fn might_have_quoted_newlines(path: &Path) -> bool {
    // Simple heuristic: check first few KB
    let content = std::fs::read_to_string(path).unwrap_or_default();
    let sample = &content[..content.len().min(4096)];
    sample.contains("\"\n") || sample.contains("\r\n\"")
}

let config = if might_have_quoted_newlines(path) {
    CsvImportConfig::new().with_parallel(false)
} else {
    CsvImportConfig::default()
};
```

---

## Benchmarking

Run the CSV benchmarks to measure performance:

```bash
# Run all CSV benchmarks
cargo bench --bench csv_benchmark

# Run specific benchmark
cargo bench --bench csv_benchmark -- kuzu_study
```

### Expected Results (4-core system)

| Benchmark | Sequential | Parallel | Speedup |
|-----------|------------|----------|---------|
| 100K nodes | ~0.09s | ~0.03s | ~3x |
| 2.4M edges | ~2.9s | ~0.95s | ~3x |

---

## Error Handling

### Parallel Mode Errors

```rust
// Quoted newline error
RuzuError::QuotedNewlineInParallel { row: 1234 }
// Message: "Quoted newlines not supported in parallel mode. Set parallel=false."

// Thread panic (rare)
RuzuError::ThreadPanic("Worker thread panicked")
```

### Handling Errors in Progress Callback

```rust
let callback = Box::new(|p: ImportProgress| {
    for error in &p.errors {
        eprintln!("Error at row {}: {}", error.row_number, error.message);
    }
});
```

---

## Migration Guide

### From v0.0.1

No code changes required. Default behavior is enhanced but compatible.

### Opting Out of New Features

```rust
// Revert to v0.0.1 behavior
let config = CsvImportConfig::new()
    .with_parallel(false)
    .with_mmap(false);
```

---

## Troubleshooting

### Import is Slower Than Expected

1. Check if file is on slow storage (network drive, HDD)
2. Verify `parallel=true` in config
3. Check available CPU cores (`num_threads` auto-detects)
4. Ensure file size > `mmap_threshold` for mmap benefits

### Memory Usage is High

1. Reduce `batch_size`
2. Disable `intern_strings` (enabled increases memory)
3. Set lower `mmap_threshold` to use buffered I/O

### Errors Not Reported with Correct Row Numbers

1. Ensure you're using the latest version
2. Row numbers are calculated from file byte offsets in parallel mode
3. Report as bug if row numbers are incorrect

---

## Architecture Notes

### How Parallel Processing Works

```
1. File is split into N blocks (block_size default: 256KB)
2. Each thread claims blocks via channel
3. Non-first blocks seek to next newline before parsing
4. Threads parse until they cross into next block boundary
5. Results aggregated in block order
```

### Memory Layout

```
┌─────────────────────────────────────────┐
│              CSV File                    │
├───────────┬───────────┬───────────┬─────┤
│  Block 0  │  Block 1  │  Block 2  │ ... │
│  (header) │           │           │     │
└───────────┴───────────┴───────────┴─────┘
     ▼           ▼           ▼
  Thread 0   Thread 1   Thread 2
     │           │           │
     └───────────┴───────────┘
              │
              ▼
        ParsedBatch[]
              │
              ▼
        Final Result
```

---

## See Also

- [spec.md](spec.md) - Feature specification
- [data-model.md](data-model.md) - Data structure definitions
- [contracts/csv-import-api.md](contracts/csv-import-api.md) - API contract
- [research.md](research.md) - Design decisions and rationale
