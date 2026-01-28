# CSV Import API Contract

**Feature**: 003-optimize-csv-import
**Date**: 2025-12-06
**Version**: 1.0

This document defines the public API contract for the optimized CSV import module.

---

## Module: `ruzu::storage::csv`

### Re-exports (Existing)

```rust
pub use node_loader::NodeLoader;
pub use rel_loader::{ParsedRelationship, RelLoader};
pub use parser::CsvParser;

// NEW exports
pub use parallel::ParallelCsvReader;
pub use mmap_reader::MmapReader;
```

---

## CsvImportConfig

### Constructor

```rust
impl CsvImportConfig {
    /// Creates config with default settings
    pub fn new() -> Self;

    /// Creates config for sequential processing (no parallelism)
    pub fn sequential() -> Self;

    /// Creates config for maximum parallelism
    pub fn parallel() -> Self;
}
```

### Builder Methods

```rust
impl CsvImportConfig {
    // Existing
    pub fn with_delimiter(self, delimiter: char) -> Self;
    pub fn with_quote(self, quote: char) -> Self;
    pub fn with_header(self, has_header: bool) -> Self;
    pub fn with_skip_rows(self, skip_rows: usize) -> Self;
    pub fn with_ignore_errors(self, ignore_errors: bool) -> Self;
    pub fn with_batch_size(self, batch_size: usize) -> Self;

    // NEW
    pub fn with_parallel(self, parallel: bool) -> Self;
    pub fn with_num_threads(self, threads: usize) -> Self;
    pub fn with_mmap(self, use_mmap: bool) -> Self;
    pub fn with_mmap_threshold(self, threshold: u64) -> Self;
    pub fn with_block_size(self, size: usize) -> Self;
    pub fn with_intern_strings(self, intern: bool) -> Self;
}
```

### Validation

```rust
impl CsvImportConfig {
    /// Validates configuration, returns error if invalid
    pub fn validate(&self) -> Result<()>;
}
```

**Validation Rules**:
- `block_size` must be in range [65536, 16777216] (64KB - 16MB)
- `batch_size` must be in range [1, 1000000]
- `mmap_threshold` must be >= 1048576 (1MB)
- `num_threads` if Some, must be >= 1

---

## ImportProgress

### Existing Methods

```rust
impl ImportProgress {
    pub fn new() -> Self;
    pub fn percent_complete(&self) -> Option<f64>;
    pub fn add_error(&mut self, error: ImportError);
    pub fn increment_rows(&mut self, count: u64);
    pub fn increment_bytes(&mut self, count: u64);
}
```

### NEW Methods

```rust
impl ImportProgress {
    /// Start timing (call at beginning of import)
    pub fn start(&mut self);

    /// Update progress with new data
    pub fn update(&mut self, rows_added: u64, bytes_added: u64);

    /// Returns overall throughput in rows/second
    pub fn throughput(&self) -> Option<f64>;

    /// Returns smoothed throughput (exponential moving average)
    pub fn smoothed_throughput(&self) -> Option<f64>;

    /// Returns estimated time remaining in seconds
    pub fn eta_seconds(&self) -> Option<f64>;

    /// Returns elapsed time since start
    pub fn elapsed(&self) -> Option<Duration>;
}
```

---

## NodeLoader

### Existing API (No Changes)

```rust
impl NodeLoader {
    pub fn new(schema: Arc<NodeTableSchema>, config: CsvImportConfig) -> Self;
    pub fn validate_headers(&self, headers: &[String]) -> Result<Vec<usize>>;
    pub fn load(
        &self,
        path: &Path,
        progress_callback: Option<ProgressCallback>,
    ) -> Result<(Vec<Vec<Value>>, ImportResult)>;
}
```

### Behavioral Changes

- When `config.parallel == true` and file size >= `config.mmap_threshold`:
  - Uses `ParallelCsvReader` internally
  - Processes file in blocks
  - Progress callback may be called from multiple threads

- When `config.parallel == false` or file is small:
  - Uses existing sequential implementation
  - Backward compatible behavior

---

## RelLoader

### Existing API (No Changes)

```rust
impl RelLoader {
    pub fn new(
        from_column: String,
        to_column: String,
        property_columns: Vec<(String, DataType)>,
        config: CsvImportConfig,
    ) -> Self;

    pub fn with_default_columns(
        property_columns: Vec<(String, DataType)>,
        config: CsvImportConfig,
    ) -> Self;

    pub fn validate_headers(&self, headers: &[String]) -> Result<(usize, usize, Vec<usize>)>;

    pub fn load(
        &self,
        path: &Path,
        progress_callback: Option<ProgressCallback>,
    ) -> Result<(Vec<ParsedRelationship>, ImportResult)>;
}
```

### Behavioral Changes

Same as `NodeLoader` - parallel processing when enabled and file is large enough.

---

## ParallelCsvReader (NEW)

### Public API

```rust
pub struct ParallelCsvReader { /* private fields */ }

impl ParallelCsvReader {
    /// Creates a new parallel reader
    ///
    /// # Errors
    /// - File does not exist
    /// - File cannot be opened
    /// - Configuration is invalid
    pub fn new(path: &Path, config: CsvImportConfig) -> Result<Self>;

    /// Returns the number of threads that will be used
    pub fn num_threads(&self) -> usize;

    /// Returns the number of blocks the file is split into
    pub fn num_blocks(&self) -> usize;

    /// Returns total file size in bytes
    pub fn file_size(&self) -> u64;

    /// Process all blocks and return parsed batches
    ///
    /// # Returns
    /// Vector of ParsedBatch, one per block, in block order
    ///
    /// # Errors
    /// - I/O errors during reading
    /// - Parse errors (if ignore_errors == false)
    pub fn read_all(&self) -> Result<Vec<ParsedBatch>>;

    /// Process with progress reporting
    ///
    /// # Arguments
    /// - `callback`: Called periodically with progress updates
    ///
    /// # Thread Safety
    /// Callback may be invoked from multiple threads
    pub fn read_with_progress<F>(&self, callback: F) -> Result<Vec<ParsedBatch>>
    where
        F: Fn(ImportProgress) + Send + Sync;
}
```

---

## MmapReader (NEW)

### Public API

```rust
pub enum MmapReader {
    Mmap(Mmap),
    Buffered(BufReader<File>),
}

impl MmapReader {
    /// Opens file, using mmap if appropriate
    ///
    /// Falls back to buffered I/O if:
    /// - File size < config.mmap_threshold
    /// - config.use_mmap == false
    /// - mmap fails (logs warning)
    pub fn open(path: &Path, config: &CsvImportConfig) -> Result<Self>;

    /// Returns byte slice for memory-mapped files, None for buffered
    pub fn as_slice(&self) -> Option<&[u8]>;

    /// Returns file size
    pub fn len(&self) -> u64;

    /// Returns true if using memory mapping
    pub fn is_mmap(&self) -> bool;
}
```

---

## Error Handling

### ImportError (Existing)

No changes to the struct. Row numbers in parallel mode are correctly assigned based on the actual file position.

### New Error Variants

```rust
impl RuzuError {
    // Existing variants...

    // NEW: Parallel-specific errors
    /// Thread panicked during parallel processing
    ThreadPanic(String),

    /// Quoted newline encountered in parallel mode
    /// Suggests: "Please specify parallel=false in the configuration"
    QuotedNewlineInParallel { row: u64 },
}
```

---

## Progress Callback Contract

### Callback Signature

```rust
pub type ProgressCallback = Box<dyn Fn(ImportProgress) + Send + Sync>;
```

### Callback Guarantees

| Guarantee | Description |
|-----------|-------------|
| Ordering | `rows_processed` is monotonically increasing |
| Frequency | Called at least once per `batch_size` rows |
| Thread safety | May be called from multiple threads (use `Arc<Mutex<_>>` for mutable state) |
| Final call | Always called once when import completes |

### Example Usage

```rust
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

let rows_seen = Arc::new(AtomicU64::new(0));
let rows_seen_clone = Arc::clone(&rows_seen);

let callback: ProgressCallback = Box::new(move |progress| {
    rows_seen_clone.store(progress.rows_processed, Ordering::SeqCst);
    if let Some(throughput) = progress.throughput() {
        println!("{} rows at {:.0} rows/sec", progress.rows_processed, throughput);
    }
    if let Some(eta) = progress.eta_seconds() {
        println!("ETA: {:.1} seconds", eta);
    }
});
```

---

## Backward Compatibility

### Unchanged Behaviors

| Aspect | Guarantee |
|--------|-----------|
| COPY FROM syntax | No changes |
| Default behavior | `CsvImportConfig::default()` produces equivalent results to v0.0.1 |
| Error messages | Row numbers correctly identify error location |
| Transaction semantics | Batch commit/rollback unchanged |

### Breaking Changes

None. All new functionality is opt-in via configuration.

### Deprecations

None.

---

## Performance Contracts

### Throughput Targets

| Operation | Target | Conditions |
|-----------|--------|------------|
| Node import | >= 1M nodes/sec | 4+ cores, SSD, file >= 100MB |
| Edge import | >= 2.5M edges/sec | 4+ cores, SSD, file >= 100MB |

### Memory Limits

| Operation | Max Memory | Conditions |
|-----------|------------|------------|
| Import 1GB CSV | < 500MB | Excluding OS file cache |
| Per-thread buffer | < 10MB | During parallel parsing |

### Scalability

| Threads | Expected Speedup |
|---------|------------------|
| 1 | 1x (baseline) |
| 2 | 1.8-2x |
| 4 | 3-3.5x |
| 8 | 4-5x (diminishing returns) |
