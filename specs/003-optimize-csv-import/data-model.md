# Data Model: Optimize Bulk CSV Import

**Feature**: 003-optimize-csv-import
**Date**: 2025-12-06
**Status**: Design Complete

This document defines the new and modified data structures for the CSV import optimization feature.

---

## Entity Overview

| Entity | Type | Location | Description |
|--------|------|----------|-------------|
| CsvImportConfig | Modified | src/storage/csv/mod.rs | Extended with parallel/mmap settings |
| ImportProgress | Modified | src/storage/csv/mod.rs | Extended with throughput metrics |
| ParallelCsvReader | New | src/storage/csv/parallel.rs | Block-based parallel CSV reader |
| MmapReader | New | src/storage/csv/mmap_reader.rs | Memory-mapped file wrapper |
| ParsedBatch | New | src/storage/csv/parallel.rs | Batch of parsed rows for parallel processing |
| BlockAssignment | New | src/storage/csv/parallel.rs | Thread work assignment |
| StringInterner | New | src/storage/csv/interner.rs | Optional string deduplication |

---

## 1. CsvImportConfig (Modified)

**File**: `src/storage/csv/mod.rs`

### Current Fields
```rust
pub struct CsvImportConfig {
    pub delimiter: char,
    pub quote: char,
    pub escape: char,
    pub has_header: bool,
    pub skip_rows: usize,
    pub parallel: bool,        // Currently unused
    pub ignore_errors: bool,
    pub batch_size: usize,
}
```

### New/Modified Fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `parallel` | `bool` | `true` | Enable parallel parsing (now implemented) |
| `num_threads` | `Option<usize>` | `None` | Thread count (None = auto-detect) |
| `use_mmap` | `bool` | `true` | Enable memory-mapped I/O for large files |
| `mmap_threshold` | `u64` | `104_857_600` | Min file size for mmap (100MB) |
| `block_size` | `usize` | `262_144` | Parallel block size (256KB) |
| `intern_strings` | `bool` | `false` | Enable string interning |

### Updated Definition

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CsvImportConfig {
    // CSV parsing options
    pub delimiter: char,
    pub quote: char,
    pub escape: char,
    pub has_header: bool,
    pub skip_rows: usize,

    // Error handling
    pub ignore_errors: bool,

    // Batching
    pub batch_size: usize,

    // NEW: Parallelism options
    pub parallel: bool,
    pub num_threads: Option<usize>,
    pub block_size: usize,

    // NEW: I/O options
    pub use_mmap: bool,
    pub mmap_threshold: u64,

    // NEW: Optimization options
    pub intern_strings: bool,
}

impl Default for CsvImportConfig {
    fn default() -> Self {
        Self {
            delimiter: ',',
            quote: '"',
            escape: '"',
            has_header: true,
            skip_rows: 0,
            ignore_errors: false,
            batch_size: 2048,

            // Parallelism defaults
            parallel: true,
            num_threads: None,  // Auto-detect
            block_size: 256 * 1024,  // 256KB

            // I/O defaults
            use_mmap: true,
            mmap_threshold: 100 * 1024 * 1024,  // 100MB

            // Optimization defaults
            intern_strings: false,
        }
    }
}
```

### Builder Methods

```rust
impl CsvImportConfig {
    pub fn with_parallel(mut self, parallel: bool) -> Self {
        self.parallel = parallel;
        self
    }

    pub fn with_num_threads(mut self, threads: usize) -> Self {
        self.num_threads = Some(threads);
        self
    }

    pub fn with_mmap(mut self, use_mmap: bool) -> Self {
        self.use_mmap = use_mmap;
        self
    }

    pub fn with_mmap_threshold(mut self, threshold: u64) -> Self {
        self.mmap_threshold = threshold;
        self
    }

    pub fn with_block_size(mut self, size: usize) -> Self {
        self.block_size = size;
        self
    }

    pub fn with_intern_strings(mut self, intern: bool) -> Self {
        self.intern_strings = intern;
        self
    }
}
```

---

## 2. ImportProgress (Modified)

**File**: `src/storage/csv/mod.rs`

### Current Fields
```rust
pub struct ImportProgress {
    pub rows_processed: u64,
    pub rows_total: Option<u64>,
    pub rows_failed: u64,
    pub bytes_read: u64,
    pub errors: Vec<ImportError>,
}
```

### New Fields

| Field | Type | Description |
|-------|------|-------------|
| `start_time` | `Option<Instant>` | When import started |
| `last_update_time` | `Option<Instant>` | Last progress update |
| `last_row_count` | `u64` | Rows at last update (for smoothing) |
| `throughput_samples` | `Vec<f64>` | Recent throughput samples |

### Updated Definition

```rust
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct ImportProgress {
    // Existing fields
    pub rows_processed: u64,
    pub rows_total: Option<u64>,
    pub rows_failed: u64,
    pub bytes_read: u64,
    pub errors: Vec<ImportError>,

    // NEW: Timing fields
    start_time: Option<Instant>,
    last_update_time: Option<Instant>,
    last_row_count: u64,
    throughput_samples: Vec<f64>,
}

impl Default for ImportProgress {
    fn default() -> Self {
        Self {
            rows_processed: 0,
            rows_total: None,
            rows_failed: 0,
            bytes_read: 0,
            errors: Vec::new(),
            start_time: None,
            last_update_time: None,
            last_row_count: 0,
            throughput_samples: Vec::with_capacity(10),
        }
    }
}

impl ImportProgress {
    pub fn start(&mut self) {
        let now = Instant::now();
        self.start_time = Some(now);
        self.last_update_time = Some(now);
    }

    /// Returns overall throughput in rows/sec
    pub fn throughput(&self) -> Option<f64> {
        let elapsed = self.start_time?.elapsed().as_secs_f64();
        if elapsed > 0.0 {
            Some(self.rows_processed as f64 / elapsed)
        } else {
            None
        }
    }

    /// Returns smoothed throughput (exponential moving average)
    pub fn smoothed_throughput(&self) -> Option<f64> {
        if self.throughput_samples.is_empty() {
            return self.throughput();
        }
        // EMA with alpha = 0.3 (recent values weighted more)
        let alpha = 0.3;
        let mut ema = self.throughput_samples[0];
        for &sample in &self.throughput_samples[1..] {
            ema = alpha * sample + (1.0 - alpha) * ema;
        }
        Some(ema)
    }

    /// Returns estimated time remaining in seconds
    pub fn eta_seconds(&self) -> Option<f64> {
        let remaining = self.rows_total?.saturating_sub(self.rows_processed);
        let throughput = self.smoothed_throughput()?;
        if throughput > 0.0 {
            Some(remaining as f64 / throughput)
        } else {
            None
        }
    }

    /// Update progress and record throughput sample
    pub fn update(&mut self, rows_added: u64, bytes_added: u64) {
        self.rows_processed += rows_added;
        self.bytes_read += bytes_added;

        // Record throughput sample for smoothing
        if let Some(last_time) = self.last_update_time {
            let elapsed = last_time.elapsed().as_secs_f64();
            if elapsed > 0.0 {
                let rows_delta = self.rows_processed - self.last_row_count;
                let sample = rows_delta as f64 / elapsed;
                self.throughput_samples.push(sample);
                // Keep only last 10 samples
                if self.throughput_samples.len() > 10 {
                    self.throughput_samples.remove(0);
                }
            }
        }
        self.last_update_time = Some(Instant::now());
        self.last_row_count = self.rows_processed;
    }
}
```

---

## 3. ParallelCsvReader (New)

**File**: `src/storage/csv/parallel.rs`

### Purpose
Coordinates parallel CSV reading by splitting file into blocks and assigning to worker threads.

### Definition

```rust
use std::path::Path;
use std::sync::Arc;
use crossbeam::channel::{bounded, Receiver, Sender};
use parking_lot::Mutex;

/// Coordinates parallel CSV reading
pub struct ParallelCsvReader {
    /// Path to CSV file
    path: Arc<Path>,
    /// Total file size in bytes
    file_size: u64,
    /// Configuration
    config: CsvImportConfig,
    /// Number of blocks
    num_blocks: usize,
    /// Block assignment channel
    block_sender: Sender<BlockAssignment>,
    block_receiver: Receiver<BlockAssignment>,
    /// Shared error collection
    errors: Arc<Mutex<Vec<ImportError>>>,
}

impl ParallelCsvReader {
    /// Creates a new parallel reader for the given file
    pub fn new(path: &Path, config: CsvImportConfig) -> Result<Self>;

    /// Returns the number of threads to use
    pub fn num_threads(&self) -> usize;

    /// Process the file in parallel, returning all parsed rows
    pub fn read_all(&self) -> Result<Vec<ParsedBatch>>;

    /// Process with progress callback
    pub fn read_with_progress<F>(&self, callback: F) -> Result<Vec<ParsedBatch>>
    where
        F: Fn(ImportProgress) + Send + Sync;
}
```

### Block Assignment

```rust
/// Work assignment for a single block
#[derive(Debug, Clone)]
pub struct BlockAssignment {
    /// Block index (0-based)
    pub block_idx: usize,
    /// Byte offset in file
    pub start_offset: u64,
    /// Expected end offset (actual end may differ due to row boundaries)
    pub end_offset: u64,
    /// Whether this is the first block (contains header)
    pub is_first_block: bool,
}

impl BlockAssignment {
    pub fn new(block_idx: usize, block_size: usize, file_size: u64, is_first: bool) -> Self {
        let start = block_idx as u64 * block_size as u64;
        let end = std::cmp::min(start + block_size as u64, file_size);
        Self {
            block_idx,
            start_offset: start,
            end_offset: end,
            is_first_block: is_first,
        }
    }
}
```

---

## 4. ParsedBatch (New)

**File**: `src/storage/csv/parallel.rs`

### Purpose
Container for a batch of parsed rows from parallel processing.

### Definition

```rust
use crate::types::Value;

/// A batch of parsed rows ready for insertion
#[derive(Debug)]
pub struct ParsedBatch {
    /// Block index this batch came from
    pub block_idx: usize,
    /// Parsed rows (each row is a Vec of Values)
    pub rows: Vec<Vec<Value>>,
    /// Starting row number (1-indexed, for error reporting)
    pub start_row_number: u64,
    /// Bytes processed in this batch
    pub bytes_processed: u64,
    /// Errors encountered during parsing
    pub errors: Vec<ImportError>,
}

impl ParsedBatch {
    pub fn new(block_idx: usize, start_row: u64) -> Self {
        Self {
            block_idx,
            rows: Vec::new(),
            start_row_number: start_row,
            bytes_processed: 0,
            errors: Vec::new(),
        }
    }

    /// Number of successfully parsed rows
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// Number of failed rows
    pub fn error_count(&self) -> usize {
        self.errors.len()
    }
}
```

---

## 5. MmapReader (New)

**File**: `src/storage/csv/mmap_reader.rs`

### Purpose
Wrapper for memory-mapped file access with fallback to buffered I/O.

### Definition

```rust
use memmap2::Mmap;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

/// File reader that uses mmap for large files
pub enum MmapReader {
    /// Memory-mapped file
    Mmap(Mmap),
    /// Buffered file reader (fallback)
    Buffered(BufReader<File>),
}

impl MmapReader {
    /// Opens a file, using mmap if file size exceeds threshold
    pub fn open(path: &Path, config: &CsvImportConfig) -> Result<Self> {
        let file = File::open(path)?;
        let metadata = file.metadata()?;
        let file_size = metadata.len();

        if config.use_mmap && file_size >= config.mmap_threshold {
            match Self::try_mmap(&file) {
                Ok(mmap) => return Ok(MmapReader::Mmap(mmap)),
                Err(e) => {
                    // Log warning and fall back
                    eprintln!("Warning: mmap failed, falling back to buffered I/O: {}", e);
                }
            }
        }

        Ok(MmapReader::Buffered(BufReader::new(file)))
    }

    fn try_mmap(file: &File) -> Result<Mmap> {
        // SAFETY: File is opened read-only, we assume no concurrent writes
        unsafe { Mmap::map(file) }
            .map_err(|e| RuzuError::StorageError(format!("mmap failed: {}", e)))
    }

    /// Returns the file as a byte slice (for mmap) or None (for buffered)
    pub fn as_slice(&self) -> Option<&[u8]> {
        match self {
            MmapReader::Mmap(mmap) => Some(&mmap[..]),
            MmapReader::Buffered(_) => None,
        }
    }

    /// Returns file size
    pub fn len(&self) -> u64;

    /// Returns whether this is memory-mapped
    pub fn is_mmap(&self) -> bool {
        matches!(self, MmapReader::Mmap(_))
    }
}
```

---

## 6. StringInterner (New)

**File**: `src/storage/csv/interner.rs`

### Purpose
Deduplicate repeated string values during import to reduce memory allocations.

### Definition

```rust
use std::collections::HashMap;
use std::sync::Arc;

/// Thread-safe string interner for deduplicating repeated values
pub struct StringInterner {
    /// Interned strings map
    strings: HashMap<Box<str>, Arc<str>>,
    /// Statistics
    hits: u64,
    misses: u64,
}

impl StringInterner {
    pub fn new() -> Self {
        Self {
            strings: HashMap::new(),
            hits: 0,
            misses: 0,
        }
    }

    /// Intern a string, returning a reference-counted string
    pub fn intern(&mut self, s: &str) -> Arc<str> {
        if let Some(existing) = self.strings.get(s) {
            self.hits += 1;
            return Arc::clone(existing);
        }
        self.misses += 1;
        let arc: Arc<str> = Arc::from(s);
        self.strings.insert(s.into(), Arc::clone(&arc));
        arc
    }

    /// Returns hit rate (0.0 to 1.0)
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total > 0 {
            self.hits as f64 / total as f64
        } else {
            0.0
        }
    }

    /// Returns number of unique strings
    pub fn unique_count(&self) -> usize {
        self.strings.len()
    }

    /// Clear all interned strings
    pub fn clear(&mut self) {
        self.strings.clear();
        self.hits = 0;
        self.misses = 0;
    }
}
```

### Thread-Safe Version

For parallel processing, use `parking_lot::RwLock`:

```rust
use parking_lot::RwLock;
use std::sync::Arc;

pub type SharedInterner = Arc<RwLock<StringInterner>>;

pub fn shared_interner() -> SharedInterner {
    Arc::new(RwLock::new(StringInterner::new()))
}
```

---

## 7. ThreadLocalErrors (New)

**File**: `src/storage/csv/parallel.rs`

### Purpose
Thread-local error collection for parallel parsing, aggregated after completion.

### Definition

```rust
use parking_lot::Mutex;
use std::sync::Arc;

/// Collects errors from multiple threads
pub struct ThreadLocalErrors {
    /// Errors indexed by block_idx
    errors_by_block: Arc<Mutex<HashMap<usize, Vec<ImportError>>>>,
}

impl ThreadLocalErrors {
    pub fn new() -> Self {
        Self {
            errors_by_block: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Add errors for a specific block
    pub fn add_errors(&self, block_idx: usize, errors: Vec<ImportError>) {
        let mut map = self.errors_by_block.lock();
        map.entry(block_idx)
            .or_insert_with(Vec::new)
            .extend(errors);
    }

    /// Collect all errors in block order
    pub fn collect_ordered(&self) -> Vec<ImportError> {
        let map = self.errors_by_block.lock();
        let mut block_indices: Vec<_> = map.keys().copied().collect();
        block_indices.sort();

        let mut all_errors = Vec::new();
        for idx in block_indices {
            if let Some(errors) = map.get(&idx) {
                all_errors.extend(errors.iter().cloned());
            }
        }
        all_errors
    }
}
```

---

## Relationships Between Entities

```
┌─────────────────────┐
│  CsvImportConfig    │──────────┐
└─────────────────────┘          │
         │                       │
         │ configures            │ configures
         ▼                       ▼
┌─────────────────────┐   ┌─────────────────────┐
│  ParallelCsvReader  │   │     MmapReader      │
└─────────────────────┘   └─────────────────────┘
         │                       │
         │ creates               │ provides data
         ▼                       │
┌─────────────────────┐          │
│  BlockAssignment    │          │
└─────────────────────┘          │
         │                       │
         │ processed to          │
         ▼                       │
┌─────────────────────┐◄─────────┘
│   ParsedBatch       │
└─────────────────────┘
         │
         │ aggregated into
         ▼
┌─────────────────────┐
│  ImportProgress     │
└─────────────────────┘
```

---

## Validation Rules

### CsvImportConfig

| Field | Validation |
|-------|------------|
| `num_threads` | Must be >= 1 if Some |
| `block_size` | Must be >= 64KB and <= 16MB |
| `batch_size` | Must be >= 1 and <= 1,000,000 |
| `mmap_threshold` | Must be >= 1MB |

### ParsedBatch

| Rule | Description |
|------|-------------|
| Row numbers must be monotonic | start_row_number + rows.len() < next batch start |
| Block index unique | No duplicate block indices in results |

---

## State Transitions

### Import Process States

```
[Not Started]
     │
     ▼ start()
[Initializing] ─── file open fails ──► [Failed]
     │
     ▼ success
[Reading] ◄──────────────────────────────┐
     │                                   │
     ├── block complete ─────────────────┤
     │                                   │
     ▼ all blocks complete               │
[Finalizing] ─── aggregate errors ───────┘
     │
     ▼
[Completed]
```

### Progress Update Triggers

| Event | Progress Update |
|-------|-----------------|
| Block complete | Update rows_processed, bytes_read, record throughput sample |
| Error encountered | Add to errors list, increment rows_failed |
| All blocks done | Calculate final throughput, ETA = 0 |
