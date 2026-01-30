//! Bulk CSV import functionality.
//!
//! This module provides efficient bulk loading of data from CSV files.
//!
//! # Features
//!
//! - Configurable CSV parsing options (delimiter, quote, escape)
//! - Progress reporting during import
//! - Error handling modes (atomic vs continue-on-error)
//! - Batch processing for efficiency
//! - Memory-bounded streaming imports for large files

mod buffer;
mod interner;
mod mmap_reader;
mod node_loader;
mod parallel;
mod parser;
mod rel_loader;
mod streaming;

pub use buffer::RowBuffer;
pub use interner::{
    shared_interner, shared_interner_with_capacity, SharedInterner, StringInterner,
};
pub use mmap_reader::MmapReader;
pub use node_loader::NodeLoader;
pub use parallel::{
    estimate_row_offsets, has_quoted_newline, parallel_read_all, process_block, seek_to_row_start,
    BlockAssignment, BlockResult, ParallelCsvReader, ParsedBatch, SharedProgress,
    ThreadLocalErrors,
};
pub use parser::CsvParser;
pub use rel_loader::{ParsedRelationship, RelLoader};
pub use streaming::{StreamingConfig, StreamingError};

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::error::RuzuError;

/// Configuration for CSV import operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CsvImportConfig {
    // CSV parsing options
    /// Field separator (default: ',').
    pub delimiter: char,
    /// Quote character (default: '"').
    pub quote: char,
    /// Escape character (default: '"').
    pub escape: char,
    /// Whether the first row is a header (default: true).
    pub has_header: bool,
    /// Number of rows to skip before processing (default: 0).
    pub skip_rows: usize,

    // Error handling
    /// Continue on parse errors instead of aborting (default: false).
    pub ignore_errors: bool,

    // Batching
    /// Number of rows per batch (default: 2048).
    pub batch_size: usize,

    // Parallelism options
    /// Enable parallel parsing (default: true).
    pub parallel: bool,
    /// Number of worker threads. None = auto-detect based on CPU cores.
    pub num_threads: Option<usize>,
    /// Block size in bytes for parallel processing (default: 256KB).
    pub block_size: usize,

    // I/O options
    /// Enable memory-mapped I/O for large files (default: true).
    pub use_mmap: bool,
    /// Minimum file size in bytes to use mmap (default: 100MB).
    pub mmap_threshold: u64,

    // Optimization options
    /// Enable string interning for repeated values (default: false).
    pub intern_strings: bool,
}

impl Default for CsvImportConfig {
    fn default() -> Self {
        Self {
            // CSV parsing defaults
            delimiter: ',',
            quote: '"',
            escape: '"',
            has_header: true,
            skip_rows: 0,

            // Error handling defaults
            ignore_errors: false,

            // Batching defaults
            batch_size: 2048,

            // Parallelism defaults
            parallel: true,
            num_threads: None,      // Auto-detect
            block_size: 256 * 1024, // 256KB

            // I/O defaults
            use_mmap: true,
            mmap_threshold: 100 * 1024 * 1024, // 100MB

            // Optimization defaults
            intern_strings: false,
        }
    }
}

impl CsvImportConfig {
    /// Creates a new config with default settings.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the field delimiter.
    #[must_use]
    pub fn with_delimiter(mut self, delimiter: char) -> Self {
        self.delimiter = delimiter;
        self
    }

    /// Sets the quote character.
    #[must_use]
    pub fn with_quote(mut self, quote: char) -> Self {
        self.quote = quote;
        self
    }

    /// Sets whether the file has a header row.
    #[must_use]
    pub fn with_header(mut self, has_header: bool) -> Self {
        self.has_header = has_header;
        self
    }

    /// Sets the number of rows to skip.
    #[must_use]
    pub fn with_skip_rows(mut self, skip_rows: usize) -> Self {
        self.skip_rows = skip_rows;
        self
    }

    /// Sets whether to continue on errors.
    #[must_use]
    pub fn with_ignore_errors(mut self, ignore_errors: bool) -> Self {
        self.ignore_errors = ignore_errors;
        self
    }

    /// Sets the batch size.
    #[must_use]
    pub fn with_batch_size(mut self, batch_size: usize) -> Self {
        self.batch_size = batch_size;
        self
    }

    /// Sets whether to use parallel parsing.
    #[must_use]
    pub fn with_parallel(mut self, parallel: bool) -> Self {
        self.parallel = parallel;
        self
    }

    /// Sets the number of worker threads.
    #[must_use]
    pub fn with_num_threads(mut self, threads: usize) -> Self {
        self.num_threads = Some(threads);
        self
    }

    /// Sets the block size for parallel processing.
    #[must_use]
    pub fn with_block_size(mut self, size: usize) -> Self {
        self.block_size = size;
        self
    }

    /// Sets whether to use memory-mapped I/O.
    #[must_use]
    pub fn with_mmap(mut self, use_mmap: bool) -> Self {
        self.use_mmap = use_mmap;
        self
    }

    /// Sets the minimum file size for memory mapping.
    #[must_use]
    pub fn with_mmap_threshold(mut self, threshold: u64) -> Self {
        self.mmap_threshold = threshold;
        self
    }

    /// Sets whether to intern strings.
    #[must_use]
    pub fn with_intern_strings(mut self, intern: bool) -> Self {
        self.intern_strings = intern;
        self
    }

    /// Creates a config optimized for sequential processing.
    #[must_use]
    pub fn sequential() -> Self {
        Self::default().with_parallel(false)
    }

    /// Creates a config optimized for parallel processing.
    #[must_use]
    pub fn parallel_config() -> Self {
        Self::default().with_parallel(true)
    }

    /// Validates the configuration and returns an error if invalid.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - `num_threads` is Some(0)
    /// - `block_size` is less than 64KB or greater than 16MB
    /// - `batch_size` is 0 or greater than 1,000,000
    /// - `mmap_threshold` is less than 1MB
    pub fn validate(&self) -> Result<(), RuzuError> {
        if let Some(threads) = self.num_threads {
            if threads == 0 {
                return Err(RuzuError::ValidationError(
                    "num_threads must be at least 1".to_string(),
                ));
            }
        }

        const MIN_BLOCK_SIZE: usize = 64 * 1024; // 64KB
        const MAX_BLOCK_SIZE: usize = 16 * 1024 * 1024; // 16MB

        if self.block_size < MIN_BLOCK_SIZE {
            return Err(RuzuError::ValidationError(format!(
                "block_size must be at least {} bytes",
                MIN_BLOCK_SIZE
            )));
        }

        if self.block_size > MAX_BLOCK_SIZE {
            return Err(RuzuError::ValidationError(format!(
                "block_size must be at most {} bytes",
                MAX_BLOCK_SIZE
            )));
        }

        if self.batch_size == 0 {
            return Err(RuzuError::ValidationError(
                "batch_size must be at least 1".to_string(),
            ));
        }

        // Allow larger batch sizes for streaming imports (up to 10M)
        const MAX_BATCH_SIZE: usize = 10_000_000;
        if self.batch_size > MAX_BATCH_SIZE {
            return Err(RuzuError::ValidationError(format!(
                "batch_size must be at most {}",
                MAX_BATCH_SIZE
            )));
        }

        const MIN_MMAP_THRESHOLD: u64 = 1024 * 1024; // 1MB
        if self.mmap_threshold < MIN_MMAP_THRESHOLD {
            return Err(RuzuError::ValidationError(format!(
                "mmap_threshold must be at least {} bytes",
                MIN_MMAP_THRESHOLD
            )));
        }

        Ok(())
    }
}

/// Progress information during an import operation.
#[derive(Debug, Clone)]
pub struct ImportProgress {
    // Count fields
    /// Number of rows successfully processed.
    pub rows_processed: u64,
    /// Total number of rows (if known).
    pub rows_total: Option<u64>,
    /// Number of rows that failed validation.
    pub rows_failed: u64,
    /// Number of bytes read from the file.
    pub bytes_read: u64,
    /// List of errors encountered.
    pub errors: Vec<ImportError>,
    /// Number of batches completed (for streaming imports).
    pub batches_completed: u64,

    // Timing fields for throughput calculation
    /// When the import started.
    start_time: Option<Instant>,
    /// When the last progress update occurred.
    last_update_time: Option<Instant>,
    /// Row count at last update (for smoothing).
    last_row_count: u64,
    /// Recent throughput samples for EMA calculation.
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
            batches_completed: 0,
            start_time: None,
            last_update_time: None,
            last_row_count: 0,
            throughput_samples: Vec::with_capacity(10),
        }
    }
}

impl ImportProgress {
    /// Creates a new empty progress tracker.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the completion percentage (0.0 to 1.0).
    #[must_use]
    pub fn percent_complete(&self) -> Option<f64> {
        self.rows_total.map(|total| {
            if total == 0 {
                1.0
            } else {
                self.rows_processed as f64 / total as f64
            }
        })
    }

    /// Adds an error to the progress tracker.
    pub fn add_error(&mut self, error: ImportError) {
        self.errors.push(error);
        self.rows_failed += 1;
    }

    /// Increments the row count.
    pub fn increment_rows(&mut self, count: u64) {
        self.rows_processed += count;
    }

    /// Increments the bytes read count.
    pub fn increment_bytes(&mut self, count: u64) {
        self.bytes_read += count;
    }

    /// Increments the batch count (for streaming imports).
    pub fn complete_batch(&mut self) {
        self.batches_completed += 1;
    }

    /// Returns the number of completed batches.
    #[must_use]
    pub fn batch_count(&self) -> u64 {
        self.batches_completed
    }

    /// Marks the start of the import operation.
    pub fn start(&mut self) {
        let now = Instant::now();
        self.start_time = Some(now);
        self.last_update_time = Some(now);
        self.last_row_count = 0;
        self.throughput_samples.clear();
    }

    /// Updates progress and records a throughput sample.
    ///
    /// This method should be called after processing a batch of rows.
    pub fn update(&mut self, rows_added: u64, bytes_added: u64) {
        self.rows_processed += rows_added;
        self.bytes_read += bytes_added;

        // Record throughput sample for smoothing
        if let Some(last_time) = self.last_update_time {
            let elapsed = last_time.elapsed().as_secs_f64();
            if elapsed > 0.001 {
                // Only sample if at least 1ms has passed
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

    /// Returns the overall throughput in rows/second.
    #[must_use]
    pub fn throughput(&self) -> Option<f64> {
        let elapsed = self.start_time?.elapsed().as_secs_f64();
        if elapsed > 0.0 {
            Some(self.rows_processed as f64 / elapsed)
        } else {
            None
        }
    }

    /// Returns the smoothed throughput using exponential moving average.
    ///
    /// This provides a more stable throughput estimate that is less affected
    /// by momentary spikes or dips.
    #[must_use]
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

    /// Returns the estimated time remaining in seconds.
    #[must_use]
    pub fn eta_seconds(&self) -> Option<f64> {
        let remaining = self.rows_total?.saturating_sub(self.rows_processed);
        let throughput = self.smoothed_throughput()?;
        if throughput > 0.0 {
            Some(remaining as f64 / throughput)
        } else {
            None
        }
    }

    /// Returns the elapsed time since the import started.
    #[must_use]
    pub fn elapsed(&self) -> Option<Duration> {
        self.start_time.map(|t| t.elapsed())
    }
}

/// Error information for a single row during import.
#[derive(Debug, Clone)]
pub struct ImportError {
    /// Row number where the error occurred (1-indexed).
    pub row_number: u64,
    /// Column name where the error occurred (if applicable).
    pub column: Option<String>,
    /// Error message.
    pub message: String,
}

impl ImportError {
    /// Creates a new import error.
    #[must_use]
    pub fn new(row_number: u64, column: Option<String>, message: String) -> Self {
        Self {
            row_number,
            column,
            message,
        }
    }

    /// Creates an error for a specific column.
    #[must_use]
    pub fn column_error(row_number: u64, column: &str, message: impl Into<String>) -> Self {
        Self {
            row_number,
            column: Some(column.to_string()),
            message: message.into(),
        }
    }

    /// Creates a general row error.
    #[must_use]
    pub fn row_error(row_number: u64, message: impl Into<String>) -> Self {
        Self {
            row_number,
            column: None,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.column {
            Some(col) => write!(
                f,
                "Row {}, column '{}': {}",
                self.row_number, col, self.message
            ),
            None => write!(f, "Row {}: {}", self.row_number, self.message),
        }
    }
}

/// Result of an import operation.
#[derive(Debug, Clone)]
pub struct ImportResult {
    /// Number of rows successfully imported.
    pub rows_imported: u64,
    /// Number of rows that failed.
    pub rows_failed: u64,
    /// Total bytes processed.
    pub bytes_processed: u64,
    /// Errors encountered during import.
    pub errors: Vec<ImportError>,
}

impl ImportResult {
    /// Creates a new import result from progress.
    #[must_use]
    pub fn from_progress(progress: ImportProgress) -> Self {
        Self {
            rows_imported: progress.rows_processed,
            rows_failed: progress.rows_failed,
            bytes_processed: progress.bytes_read,
            errors: progress.errors,
        }
    }

    /// Returns whether the import completed without errors.
    #[must_use]
    pub fn is_success(&self) -> bool {
        self.errors.is_empty()
    }
}

/// Callback type for progress reporting.
pub type ProgressCallback = Box<dyn Fn(ImportProgress) + Send>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = CsvImportConfig::default();
        assert_eq!(config.delimiter, ',');
        assert_eq!(config.quote, '"');
        assert!(config.has_header);
        assert!(!config.ignore_errors);
        assert_eq!(config.batch_size, 2048);
    }

    #[test]
    fn test_config_builder() {
        let config = CsvImportConfig::new()
            .with_delimiter(';')
            .with_header(false)
            .with_batch_size(1000);

        assert_eq!(config.delimiter, ';');
        assert!(!config.has_header);
        assert_eq!(config.batch_size, 1000);
    }

    #[test]
    fn test_progress_tracking() {
        let mut progress = ImportProgress::new();

        progress.increment_rows(100);
        progress.rows_total = Some(200);

        assert_eq!(progress.percent_complete(), Some(0.5));

        progress.add_error(ImportError::row_error(50, "Parse error"));
        assert_eq!(progress.errors.len(), 1);
        assert_eq!(progress.rows_failed, 1);
    }

    #[test]
    fn test_import_error_display() {
        let error = ImportError::column_error(42, "name", "Invalid UTF-8");
        assert_eq!(error.to_string(), "Row 42, column 'name': Invalid UTF-8");

        let error = ImportError::row_error(10, "Missing required column");
        assert_eq!(error.to_string(), "Row 10: Missing required column");
    }

    #[test]
    fn test_config_new_fields() {
        let config = CsvImportConfig::default();

        // Check new field defaults
        assert!(config.parallel);
        assert!(config.num_threads.is_none());
        assert_eq!(config.block_size, 256 * 1024);
        assert!(config.use_mmap);
        assert_eq!(config.mmap_threshold, 100 * 1024 * 1024);
        assert!(!config.intern_strings);
    }

    #[test]
    fn test_config_new_builder_methods() {
        let config = CsvImportConfig::new()
            .with_parallel(false)
            .with_num_threads(4)
            .with_block_size(512 * 1024)
            .with_mmap(false)
            .with_mmap_threshold(50 * 1024 * 1024)
            .with_intern_strings(true);

        assert!(!config.parallel);
        assert_eq!(config.num_threads, Some(4));
        assert_eq!(config.block_size, 512 * 1024);
        assert!(!config.use_mmap);
        assert_eq!(config.mmap_threshold, 50 * 1024 * 1024);
        assert!(config.intern_strings);
    }

    #[test]
    fn test_config_sequential() {
        let config = CsvImportConfig::sequential();
        assert!(!config.parallel);
    }

    #[test]
    fn test_config_parallel() {
        let config = CsvImportConfig::parallel_config();
        assert!(config.parallel);
    }

    #[test]
    fn test_config_validation() {
        // Valid default config
        assert!(CsvImportConfig::default().validate().is_ok());

        // Invalid num_threads
        let _config = CsvImportConfig::default().with_num_threads(0);
        // Note: with_num_threads sets Some(0), we need to test the validate
        let mut config = CsvImportConfig::default();
        config.num_threads = Some(0);
        assert!(config.validate().is_err());

        // Invalid block_size (too small)
        let mut config = CsvImportConfig::default();
        config.block_size = 1024; // Less than 64KB
        assert!(config.validate().is_err());

        // Invalid block_size (too large)
        let mut config = CsvImportConfig::default();
        config.block_size = 32 * 1024 * 1024; // More than 16MB
        assert!(config.validate().is_err());

        // Invalid batch_size (zero)
        let mut config = CsvImportConfig::default();
        config.batch_size = 0;
        assert!(config.validate().is_err());

        // Invalid batch_size (too large - over 10M)
        let mut config = CsvImportConfig::default();
        config.batch_size = 20_000_000;
        assert!(config.validate().is_err());

        // Invalid mmap_threshold (too small)
        let mut config = CsvImportConfig::default();
        config.mmap_threshold = 100; // Less than 1MB
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_progress_timing() {
        let mut progress = ImportProgress::new();

        // Before start, no timing available
        assert!(progress.elapsed().is_none());
        assert!(progress.throughput().is_none());

        // After start
        progress.start();
        assert!(progress.elapsed().is_some());

        // Simulate some work
        std::thread::sleep(std::time::Duration::from_millis(10));
        progress.update(1000, 10000);

        assert!(progress.throughput().is_some());
        assert!(progress.throughput().unwrap() > 0.0);
    }

    #[test]
    fn test_progress_eta() {
        let mut progress = ImportProgress::new();
        progress.rows_total = Some(1000);
        progress.start();

        // Simulate some work
        std::thread::sleep(std::time::Duration::from_millis(10));
        progress.update(100, 1000);

        // Should have an ETA since we have rows_total and throughput
        let eta = progress.eta_seconds();
        assert!(eta.is_some());
        assert!(eta.unwrap() > 0.0);
    }
}
