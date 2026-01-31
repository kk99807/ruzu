//! Streaming CSV import configuration and error types.
//!
//! This module provides types for configuring memory-bounded streaming
//! imports of large CSV files.

use serde::{Deserialize, Serialize};

use crate::error::RuzuError;

/// Default batch size for streaming imports (100,000 rows).
pub const DEFAULT_STREAMING_BATCH_SIZE: usize = 100_000;

/// Default file size threshold for auto-enabling streaming (100 MB).
pub const DEFAULT_STREAMING_THRESHOLD: u64 = 100 * 1024 * 1024;

/// Configuration for memory-bounded streaming imports.
///
/// Streaming mode processes CSV files in batches, writing each batch
/// to storage before loading the next. This bounds memory usage
/// regardless of file size.
///
/// # Example
///
/// ```ignore
/// let config = StreamingConfig::default();
/// assert_eq!(config.batch_size, 100_000);
/// assert!(config.streaming_enabled);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamingConfig {
    /// Number of rows per batch before flush to storage.
    ///
    /// Larger batches improve throughput but use more memory.
    /// Default: 100,000 rows (~200MB memory for typical schemas).
    pub batch_size: usize,

    /// Pre-allocate buffer capacity.
    ///
    /// Defaults to `batch_size` if not specified.
    pub buffer_capacity: usize,

    /// Enable streaming mode.
    ///
    /// When enabled, rows are written in batches instead of
    /// being accumulated in memory. Default: true.
    pub streaming_enabled: bool,

    /// File size threshold for auto-enabling streaming (bytes).
    ///
    /// Files larger than this threshold will use streaming mode
    /// even if not explicitly requested. Default: 100 MB.
    pub streaming_threshold: u64,
}

impl Default for StreamingConfig {
    fn default() -> Self {
        Self {
            batch_size: DEFAULT_STREAMING_BATCH_SIZE,
            buffer_capacity: DEFAULT_STREAMING_BATCH_SIZE,
            streaming_enabled: true,
            streaming_threshold: DEFAULT_STREAMING_THRESHOLD,
        }
    }
}

impl StreamingConfig {
    /// Creates a new streaming config with default settings.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the batch size.
    #[must_use]
    pub fn with_batch_size(mut self, batch_size: usize) -> Self {
        self.batch_size = batch_size;
        self.buffer_capacity = batch_size;
        self
    }

    /// Sets the buffer capacity explicitly.
    #[must_use]
    pub fn with_buffer_capacity(mut self, capacity: usize) -> Self {
        self.buffer_capacity = capacity;
        self
    }

    /// Sets whether streaming is enabled.
    #[must_use]
    pub fn with_streaming_enabled(mut self, enabled: bool) -> Self {
        self.streaming_enabled = enabled;
        self
    }

    /// Sets the file size threshold for auto-enabling streaming.
    #[must_use]
    pub fn with_streaming_threshold(mut self, threshold: u64) -> Self {
        self.streaming_threshold = threshold;
        self
    }

    /// Creates a config with streaming disabled (legacy mode).
    #[must_use]
    pub fn disabled() -> Self {
        Self {
            streaming_enabled: false,
            ..Self::default()
        }
    }

    /// Validates the configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - `batch_size` is 0 or greater than 10,000,000
    /// - `buffer_capacity` is 0
    /// - `streaming_threshold` is 0
    pub fn validate(&self) -> Result<(), RuzuError> {
        if self.batch_size == 0 {
            return Err(RuzuError::ValidationError(
                "streaming batch_size must be at least 1".to_string(),
            ));
        }

        const MAX_BATCH_SIZE: usize = 10_000_000;
        if self.batch_size > MAX_BATCH_SIZE {
            return Err(RuzuError::ValidationError(format!(
                "streaming batch_size must be at most {MAX_BATCH_SIZE}"
            )));
        }

        if self.buffer_capacity == 0 {
            return Err(RuzuError::ValidationError(
                "streaming buffer_capacity must be at least 1".to_string(),
            ));
        }

        if self.streaming_threshold == 0 {
            return Err(RuzuError::ValidationError(
                "streaming_threshold must be greater than 0".to_string(),
            ));
        }

        Ok(())
    }

    /// Returns whether streaming should be used for the given file size.
    #[must_use]
    pub fn should_stream(&self, file_size: u64) -> bool {
        self.streaming_enabled && file_size >= self.streaming_threshold
    }
}

/// Errors specific to streaming operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamingError {
    /// The row buffer is full and must be flushed.
    BufferFull,
    /// A batch write operation failed.
    BatchWriteFailed(String),
    /// Streaming was interrupted.
    Interrupted,
    /// Invalid streaming configuration.
    InvalidConfig(String),
}

impl std::fmt::Display for StreamingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BufferFull => write!(f, "Streaming buffer is full"),
            Self::BatchWriteFailed(msg) => write!(f, "Batch write failed: {msg}"),
            Self::Interrupted => write!(f, "Streaming operation was interrupted"),
            Self::InvalidConfig(msg) => write!(f, "Invalid streaming config: {msg}"),
        }
    }
}

impl std::error::Error for StreamingError {}

impl From<StreamingError> for RuzuError {
    fn from(err: StreamingError) -> Self {
        RuzuError::StorageError(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = StreamingConfig::default();
        assert_eq!(config.batch_size, 100_000);
        assert_eq!(config.buffer_capacity, 100_000);
        assert!(config.streaming_enabled);
        assert_eq!(config.streaming_threshold, 100 * 1024 * 1024);
    }

    #[test]
    fn test_config_builder() {
        let config = StreamingConfig::new()
            .with_batch_size(50_000)
            .with_streaming_threshold(50 * 1024 * 1024);

        assert_eq!(config.batch_size, 50_000);
        assert_eq!(config.buffer_capacity, 50_000);
        assert_eq!(config.streaming_threshold, 50 * 1024 * 1024);
    }

    #[test]
    fn test_config_validation() {
        // Valid config
        assert!(StreamingConfig::default().validate().is_ok());

        // Invalid batch_size (0)
        let mut config = StreamingConfig::default();
        config.batch_size = 0;
        assert!(config.validate().is_err());

        // Invalid batch_size (too large)
        let mut config = StreamingConfig::default();
        config.batch_size = 20_000_000;
        assert!(config.validate().is_err());

        // Invalid buffer_capacity
        let mut config = StreamingConfig::default();
        config.buffer_capacity = 0;
        assert!(config.validate().is_err());

        // Invalid streaming_threshold
        let mut config = StreamingConfig::default();
        config.streaming_threshold = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_should_stream() {
        let config = StreamingConfig::default();

        // File smaller than threshold
        assert!(!config.should_stream(50 * 1024 * 1024));

        // File at threshold
        assert!(config.should_stream(100 * 1024 * 1024));

        // File larger than threshold
        assert!(config.should_stream(200 * 1024 * 1024));

        // Streaming disabled
        let disabled = StreamingConfig::disabled();
        assert!(!disabled.should_stream(200 * 1024 * 1024));
    }

    #[test]
    fn test_streaming_error_display() {
        assert_eq!(
            StreamingError::BufferFull.to_string(),
            "Streaming buffer is full"
        );
        assert_eq!(
            StreamingError::BatchWriteFailed("disk full".to_string()).to_string(),
            "Batch write failed: disk full"
        );
        assert_eq!(
            StreamingError::Interrupted.to_string(),
            "Streaming operation was interrupted"
        );
    }
}
