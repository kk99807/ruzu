//! Write-Ahead Logging (WAL) for crash recovery.
//!
//! This module implements a write-ahead log that ensures durability
//! and crash recovery for database operations.
//!
//! # Architecture
//!
//! The WAL consists of:
//! - A header with magic bytes, version, and database ID
//! - Sequential log records with checksums
//! - Checkpoint markers for recovery bounds
//!
//! # Recovery Process
//!
//! On startup:
//! 1. Check if WAL file exists
//! 2. If exists, validate header and replay committed transactions
//! 3. Rollback uncommitted transactions
//! 4. Clear WAL after successful recovery

mod checkpointer;
mod reader;
mod record;
mod writer;

pub use checkpointer::Checkpointer;
pub use reader::{ReplayResult, WalReader, WalReplayer};
pub use record::{WalPayload, WalRecord, WalRecordType};
pub use writer::WalWriter;

/// Magic bytes for WAL file identification.
pub const WAL_MAGIC: &[u8; 8] = b"RUZUWAL\0";

/// Current WAL format version.
pub const WAL_VERSION: u32 = 1;

/// WAL header stored at the beginning of the WAL file.
#[derive(Debug, Clone)]
pub struct WalHeader {
    /// Magic bytes for file identification.
    pub magic: [u8; 8],
    /// WAL format version.
    pub version: u32,
    /// Database UUID for validation.
    pub database_id: uuid::Uuid,
    /// Whether checksums are enabled for records.
    pub enable_checksums: bool,
}

impl WalHeader {
    /// Creates a new WAL header with the given database ID.
    #[must_use]
    pub fn new(database_id: uuid::Uuid, enable_checksums: bool) -> Self {
        Self {
            magic: *WAL_MAGIC,
            version: WAL_VERSION,
            database_id,
            enable_checksums,
        }
    }

    /// Validates the WAL header.
    ///
    /// # Errors
    ///
    /// Returns an error if the header is invalid.
    pub fn validate(&self) -> crate::error::Result<()> {
        use crate::error::RuzuError;

        if self.magic != *WAL_MAGIC {
            return Err(RuzuError::StorageError("Invalid WAL magic bytes".into()));
        }

        if self.version > WAL_VERSION {
            return Err(RuzuError::StorageError(format!(
                "Unsupported WAL version: {} (max supported: {})",
                self.version, WAL_VERSION
            )));
        }

        Ok(())
    }

    /// Returns the serialized size of the header.
    #[must_use]
    pub const fn serialized_size() -> usize {
        8  // magic
        + 4  // version
        + 16 // database_id
        + 1 // enable_checksums
    }
}
