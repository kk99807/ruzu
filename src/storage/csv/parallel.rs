//! Parallel CSV reading infrastructure.
//!
//! This module provides block-based parallel CSV parsing for improved throughput
//! on multi-core systems. Files are split into fixed-size blocks, and each thread
//! processes blocks independently.
//!
//! # Limitations
//!
//! - Quoted newlines are NOT supported in parallel mode. The reader will error
//!   if a quoted newline is detected. Use sequential mode for such files.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────┐
//! │              CSV File                    │
//! ├───────────┬───────────┬───────────┬─────┤
//! │  Block 0  │  Block 1  │  Block 2  │ ... │
//! │  (header) │           │           │     │
//! └───────────┴───────────┴───────────┴─────┘
//!      ▼           ▼           ▼
//!   Thread 0   Thread 1   Thread 2
//!      │           │           │
//!      └───────────┴───────────┘
//!               │
//!               ▼
//!         ParsedBatch[]
//! ```

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use parking_lot::Mutex;
use rayon::prelude::*;

use crate::error::RuzuError;
use crate::storage::csv::{CsvImportConfig, ImportError, ImportProgress};
use crate::types::Value;

/// Work assignment for a single block of the CSV file.
#[derive(Debug, Clone)]
pub struct BlockAssignment {
    /// Block index (0-based).
    pub block_idx: usize,
    /// Byte offset in file where this block starts.
    pub start_offset: u64,
    /// Expected end offset (actual end may differ due to row boundaries).
    pub end_offset: u64,
    /// Whether this is the first block (contains header if present).
    pub is_first_block: bool,
}

impl BlockAssignment {
    /// Creates a new block assignment.
    #[must_use]
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

    /// Returns the byte length of this block.
    #[must_use]
    pub fn len(&self) -> u64 {
        self.end_offset - self.start_offset
    }

    /// Returns whether this block is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A batch of parsed rows ready for insertion.
#[derive(Debug)]
pub struct ParsedBatch {
    /// Block index this batch came from.
    pub block_idx: usize,
    /// Parsed rows (each row is a Vec of Values).
    pub rows: Vec<Vec<Value>>,
    /// Starting row number (1-indexed, for error reporting).
    pub start_row_number: u64,
    /// Bytes processed in this batch.
    pub bytes_processed: u64,
    /// Errors encountered during parsing.
    pub errors: Vec<ImportError>,
}

impl ParsedBatch {
    /// Creates a new empty batch.
    #[must_use]
    pub fn new(block_idx: usize, start_row: u64) -> Self {
        Self {
            block_idx,
            rows: Vec::new(),
            start_row_number: start_row,
            bytes_processed: 0,
            errors: Vec::new(),
        }
    }

    /// Number of successfully parsed rows.
    #[must_use]
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// Number of failed rows.
    #[must_use]
    pub fn error_count(&self) -> usize {
        self.errors.len()
    }
}

/// Collects errors from multiple threads.
#[derive(Debug, Default)]
pub struct ThreadLocalErrors {
    /// Errors indexed by block_idx.
    errors_by_block: Arc<Mutex<HashMap<usize, Vec<ImportError>>>>,
}

impl ThreadLocalErrors {
    /// Creates a new error collector.
    #[must_use]
    pub fn new() -> Self {
        Self {
            errors_by_block: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Add errors for a specific block.
    pub fn add_errors(&self, block_idx: usize, errors: Vec<ImportError>) {
        if errors.is_empty() {
            return;
        }
        let mut map = self.errors_by_block.lock();
        map.entry(block_idx).or_insert_with(Vec::new).extend(errors);
    }

    /// Collect all errors in block order.
    #[must_use]
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

impl Clone for ThreadLocalErrors {
    fn clone(&self) -> Self {
        Self {
            errors_by_block: Arc::clone(&self.errors_by_block),
        }
    }
}

/// Coordinates parallel CSV reading.
pub struct ParallelCsvReader {
    /// Path to CSV file.
    path: PathBuf,
    /// Total file size in bytes.
    file_size: u64,
    /// Block size in bytes.
    block_size: usize,
    /// Number of blocks.
    num_blocks: usize,
    /// Number of threads to use.
    num_threads: usize,
    /// Shared error collection.
    errors: ThreadLocalErrors,
}

impl ParallelCsvReader {
    /// Creates a new parallel reader for the given file.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be opened or its size cannot be determined.
    pub fn new(
        path: &Path,
        block_size: usize,
        num_threads: Option<usize>,
    ) -> Result<Self, RuzuError> {
        let metadata = std::fs::metadata(path)
            .map_err(|e| RuzuError::StorageError(format!("Failed to get file metadata: {}", e)))?;
        let file_size = metadata.len();

        // Calculate number of blocks
        let num_blocks = if file_size == 0 {
            1
        } else {
            ((file_size as usize + block_size - 1) / block_size).max(1)
        };

        // Determine thread count
        let available_threads = std::thread::available_parallelism()
            .map(|p| p.get())
            .unwrap_or(1);
        let num_threads = num_threads
            .unwrap_or(available_threads)
            .min(num_blocks)
            .max(1);

        Ok(Self {
            path: path.to_path_buf(),
            file_size,
            block_size,
            num_blocks,
            num_threads,
            errors: ThreadLocalErrors::new(),
        })
    }

    /// Returns the number of threads that will be used.
    #[must_use]
    pub fn num_threads(&self) -> usize {
        self.num_threads
    }

    /// Returns the number of blocks the file is split into.
    #[must_use]
    pub fn num_blocks(&self) -> usize {
        self.num_blocks
    }

    /// Returns the file path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the file size in bytes.
    #[must_use]
    pub fn file_size(&self) -> u64 {
        self.file_size
    }

    /// Returns the block size in bytes.
    #[must_use]
    pub fn block_size(&self) -> usize {
        self.block_size
    }

    /// Generate block assignments for all blocks.
    #[must_use]
    pub fn generate_block_assignments(&self) -> Vec<BlockAssignment> {
        (0..self.num_blocks)
            .map(|idx| BlockAssignment::new(idx, self.block_size, self.file_size, idx == 0))
            .collect()
    }

    /// Returns a clone of the error collector for use in threads.
    #[must_use]
    pub fn error_collector(&self) -> ThreadLocalErrors {
        self.errors.clone()
    }

    /// Collect all errors that were recorded.
    #[must_use]
    pub fn collect_errors(&self) -> Vec<ImportError> {
        self.errors.collect_ordered()
    }
}

/// Shared progress state for parallel processing.
#[derive(Debug)]
pub struct SharedProgress {
    /// Total rows processed across all threads.
    pub rows_processed: AtomicU64,
    /// Total rows failed across all threads.
    pub rows_failed: AtomicU64,
    /// Total bytes processed.
    pub bytes_processed: AtomicU64,
    /// Whether any thread has encountered a fatal error.
    pub has_fatal_error: parking_lot::Mutex<Option<RuzuError>>,
}

impl SharedProgress {
    /// Creates a new shared progress tracker.
    #[must_use]
    pub fn new() -> Self {
        Self {
            rows_processed: AtomicU64::new(0),
            rows_failed: AtomicU64::new(0),
            bytes_processed: AtomicU64::new(0),
            has_fatal_error: parking_lot::Mutex::new(None),
        }
    }

    /// Add to rows processed.
    pub fn add_rows(&self, count: u64) {
        self.rows_processed.fetch_add(count, Ordering::Relaxed);
    }

    /// Add to rows failed.
    pub fn add_failed(&self, count: u64) {
        self.rows_failed.fetch_add(count, Ordering::Relaxed);
    }

    /// Add to bytes processed.
    pub fn add_bytes(&self, count: u64) {
        self.bytes_processed.fetch_add(count, Ordering::Relaxed);
    }

    /// Set a fatal error if none is already set.
    pub fn set_fatal_error(&self, error: RuzuError) {
        let mut guard = self.has_fatal_error.lock();
        if guard.is_none() {
            *guard = Some(error);
        }
    }

    /// Check if there's a fatal error.
    pub fn take_fatal_error(&self) -> Option<RuzuError> {
        self.has_fatal_error.lock().take()
    }

    /// Get current progress snapshot.
    pub fn snapshot(&self, total: Option<u64>) -> ImportProgress {
        let mut progress = ImportProgress::new();
        progress.rows_processed = self.rows_processed.load(Ordering::Relaxed);
        progress.rows_failed = self.rows_failed.load(Ordering::Relaxed);
        progress.bytes_read = self.bytes_processed.load(Ordering::Relaxed);
        progress.rows_total = total;
        progress
    }
}

impl Default for SharedProgress {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of processing a single block.
#[derive(Debug)]
pub struct BlockResult {
    /// The block index that was processed.
    pub block_idx: usize,
    /// Rows parsed from this block.
    pub rows: Vec<Vec<Value>>,
    /// Errors encountered in this block.
    pub errors: Vec<ImportError>,
    /// Bytes processed.
    pub bytes_processed: u64,
    /// Row numbers for error reporting (first row number in block).
    pub start_row_number: u64,
}

/// Process a single block of data.
///
/// This function is designed to be called in parallel by rayon.
///
/// # Arguments
///
/// * `data` - The entire file data as bytes
/// * `block` - The block assignment to process
/// * `config` - Import configuration
/// * `parse_row` - Function to parse a single CSV row into Values
/// * `row_offset` - The estimated starting row number for this block
///
/// # Returns
///
/// A `BlockResult` containing parsed rows and any errors encountered.
pub fn process_block<F>(
    data: &[u8],
    block: &BlockAssignment,
    config: &CsvImportConfig,
    parse_row: &F,
    row_offset: u64,
) -> Result<BlockResult, RuzuError>
where
    F: Fn(&csv::ByteRecord, u64) -> Result<Vec<Value>, ImportError> + Sync,
{
    let start = block.start_offset as usize;
    let end = block.end_offset as usize;

    // For non-first blocks, seek to the first complete row
    let actual_start = if block.is_first_block {
        start
    } else {
        seek_to_row_start(data, start)
    };

    // If we've seeked past the end, this block is empty
    if actual_start >= data.len() || actual_start >= end {
        return Ok(BlockResult {
            block_idx: block.block_idx,
            rows: Vec::new(),
            errors: Vec::new(),
            bytes_processed: 0,
            start_row_number: row_offset,
        });
    }

    // Calculate where to stop: process until we cross the next block boundary
    // (at a row boundary)
    let next_block_start = end;

    // Extract the slice for this block
    let block_data = &data[actual_start..data.len().min(end + config.block_size)];

    // Check for quoted newlines in the block (this is the limitation check)
    if has_quoted_newline(block_data, config.quote as u8) {
        return Err(RuzuError::QuotedNewlineInParallel { row: row_offset });
    }

    let mut rows = Vec::new();
    let mut errors = Vec::new();
    let mut current_row = row_offset;

    // Build a CSV reader for this block
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(config.delimiter as u8)
        .quote(config.quote as u8)
        .has_headers(block.is_first_block && config.has_header)
        .flexible(true)
        .from_reader(block_data);

    // Skip header for first block if present
    if block.is_first_block && config.has_header {
        current_row += 1; // Header is row 1
    }

    // Iterate using byte_records() but track position separately
    let mut byte_record = csv::ByteRecord::new();
    while reader.read_byte_record(&mut byte_record).unwrap_or(false) {
        current_row += 1;

        // Check if we've crossed into the next block using the record's position
        let position = byte_record.position().map_or(0, |p| p.byte());
        if actual_start as u64 + position > next_block_start as u64 {
            // We've processed all rows that belong to this block
            break;
        }

        match parse_row(&byte_record, current_row) {
            Ok(values) => rows.push(values),
            Err(e) => {
                if config.ignore_errors {
                    errors.push(e);
                } else {
                    return Err(RuzuError::ImportError(e.to_string()));
                }
            }
        }
    }

    // Calculate bytes processed as the block data we actually used
    let bytes_processed = (block_data.len() as u64).min(block.len());

    Ok(BlockResult {
        block_idx: block.block_idx,
        rows,
        errors,
        bytes_processed,
        start_row_number: row_offset,
    })
}

/// Estimate the starting row number for each block.
///
/// This is an approximation since we don't know exact row boundaries without
/// scanning the file. We estimate based on average bytes per row from block 0.
pub fn estimate_row_offsets(
    data: &[u8],
    blocks: &[BlockAssignment],
    config: &CsvImportConfig,
) -> Vec<u64> {
    if blocks.is_empty() {
        return Vec::new();
    }

    // Sample the first block to estimate average row size
    let sample_size = blocks[0].len().min(64 * 1024) as usize; // Sample up to 64KB
    let sample = &data[..sample_size.min(data.len())];

    let mut line_count = 0u64;
    for &byte in sample {
        if byte == b'\n' {
            line_count += 1;
        }
    }

    let avg_bytes_per_row = if line_count > 0 {
        sample.len() as f64 / line_count as f64
    } else {
        100.0 // Default estimate
    };

    blocks
        .iter()
        .map(|block| {
            if block.is_first_block {
                if config.has_header {
                    1
                } else {
                    0
                } // Start after header
            } else {
                (block.start_offset as f64 / avg_bytes_per_row) as u64
            }
        })
        .collect()
}

/// Parallel CSV reading using rayon.
///
/// This function processes all blocks in parallel using rayon's parallel iterator,
/// then aggregates the results in block order.
///
/// # Arguments
///
/// * `data` - The file data as a byte slice (typically from mmap or read into memory)
/// * `config` - Import configuration
/// * `parse_row` - Function to parse a single CSV row into Values
///
/// # Returns
///
/// A vector of all parsed rows in file order, plus aggregated errors and statistics.
pub fn parallel_read_all<F>(
    data: &[u8],
    config: &CsvImportConfig,
    parse_row: F,
) -> Result<(Vec<Vec<Value>>, Vec<ImportError>, u64), RuzuError>
where
    F: Fn(&csv::ByteRecord, u64) -> Result<Vec<Value>, ImportError> + Sync + Send,
{
    if data.is_empty() {
        return Ok((Vec::new(), Vec::new(), 0));
    }

    // Create block assignments
    let file_size = data.len() as u64;
    let num_blocks = ((file_size as usize + config.block_size - 1) / config.block_size).max(1);

    let blocks: Vec<BlockAssignment> = (0..num_blocks)
        .map(|idx| BlockAssignment::new(idx, config.block_size, file_size, idx == 0))
        .collect();

    // Estimate row offsets for each block
    let row_offsets = estimate_row_offsets(data, &blocks, config);

    // Process blocks in parallel
    let results: Vec<Result<BlockResult, RuzuError>> = blocks
        .par_iter()
        .zip(row_offsets.par_iter())
        .map(|(block, &row_offset)| process_block(data, block, config, &parse_row, row_offset))
        .collect();

    // Check for fatal errors
    for result in &results {
        if let Err(e) = result {
            return Err(RuzuError::ImportError(e.to_string()));
        }
    }

    // Aggregate results in block order
    let mut block_results: Vec<BlockResult> = results.into_iter().filter_map(|r| r.ok()).collect();
    block_results.sort_by_key(|r| r.block_idx);

    let mut all_rows = Vec::new();
    let mut all_errors = Vec::new();
    let mut total_bytes = 0u64;

    for result in block_results {
        all_rows.extend(result.rows);
        all_errors.extend(result.errors);
        total_bytes += result.bytes_processed;
    }

    Ok((all_rows, all_errors, total_bytes))
}

/// Seek to the start of the next complete row after a block boundary.
///
/// For non-first blocks, we need to skip to the next newline character
/// since the previous block will have consumed the partial row.
///
/// # Arguments
///
/// * `data` - The file data as a byte slice
/// * `offset` - The byte offset to start seeking from
///
/// # Returns
///
/// The byte offset of the first character after the newline, or the original
/// offset if this is at the start of the file.
#[must_use]
pub fn seek_to_row_start(data: &[u8], offset: usize) -> usize {
    if offset == 0 {
        return 0;
    }

    // Find the next newline after offset
    for (i, &byte) in data[offset..].iter().enumerate() {
        if byte == b'\n' {
            return offset + i + 1;
        }
    }

    // No newline found, return end of data
    data.len()
}

/// Check if a quoted newline is present in the given range.
///
/// This is a simplified check that looks for patterns like `"\n` or `\n"` within quotes.
/// It's conservative - may return true for valid cases, but won't miss actual quoted newlines.
///
/// # Arguments
///
/// * `data` - The byte slice to check
/// * `quote_char` - The quote character (typically `"`)
///
/// # Returns
///
/// `true` if a potential quoted newline is detected.
#[must_use]
pub fn has_quoted_newline(data: &[u8], quote_char: u8) -> bool {
    let mut in_quotes = false;

    for &byte in data {
        if byte == quote_char {
            in_quotes = !in_quotes;
        } else if byte == b'\n' && in_quotes {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_assignment_new() {
        let block = BlockAssignment::new(0, 1024, 4096, true);
        assert_eq!(block.block_idx, 0);
        assert_eq!(block.start_offset, 0);
        assert_eq!(block.end_offset, 1024);
        assert!(block.is_first_block);
        assert_eq!(block.len(), 1024);
    }

    #[test]
    fn test_block_assignment_last_block() {
        let block = BlockAssignment::new(3, 1024, 3500, false);
        assert_eq!(block.block_idx, 3);
        assert_eq!(block.start_offset, 3072);
        assert_eq!(block.end_offset, 3500); // Clamped to file size
        assert!(!block.is_first_block);
    }

    #[test]
    fn test_parsed_batch() {
        let mut batch = ParsedBatch::new(0, 1);
        assert_eq!(batch.row_count(), 0);
        assert_eq!(batch.error_count(), 0);

        batch.rows.push(vec![Value::Int64(1)]);
        assert_eq!(batch.row_count(), 1);

        batch.errors.push(ImportError::row_error(2, "test error"));
        assert_eq!(batch.error_count(), 1);
    }

    #[test]
    fn test_thread_local_errors() {
        let errors = ThreadLocalErrors::new();

        errors.add_errors(2, vec![ImportError::row_error(10, "error 2a")]);
        errors.add_errors(0, vec![ImportError::row_error(1, "error 0")]);
        errors.add_errors(2, vec![ImportError::row_error(11, "error 2b")]);

        let collected = errors.collect_ordered();
        assert_eq!(collected.len(), 3);
        assert_eq!(collected[0].row_number, 1);
        assert_eq!(collected[1].row_number, 10);
        assert_eq!(collected[2].row_number, 11);
    }

    #[test]
    fn test_seek_to_row_start() {
        let data = b"first line\nsecond line\nthird line";

        // At start, return 0
        assert_eq!(seek_to_row_start(data, 0), 0);

        // In middle of first line, find next line start
        assert_eq!(seek_to_row_start(data, 5), 11); // After "first line\n"

        // At newline position
        assert_eq!(seek_to_row_start(data, 10), 11);
    }

    #[test]
    fn test_has_quoted_newline() {
        assert!(!has_quoted_newline(b"hello,world", b'"'));
        assert!(!has_quoted_newline(b"hello\nworld", b'"'));
        assert!(has_quoted_newline(b"\"hello\nworld\"", b'"'));
        assert!(!has_quoted_newline(b"\"hello\",\"world\"", b'"'));
        assert!(has_quoted_newline(b"\"multi\nline\",value", b'"'));
    }
}
