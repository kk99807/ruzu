//! Storage module for columnar data storage.
//!
//! This module provides the storage layer for ruzu, including:
//! - In-memory columnar storage ([`ColumnStorage`], [`NodeTable`])
//! - Relationship storage using CSR format ([`RelTable`], [`CsrNodeGroup`])
//! - Buffer pool management ([`buffer_pool`])
//! - Page-level I/O ([`page`])
//! - Write-ahead logging ([`wal`])
//! - Bulk CSV import ([`csv`])

mod column;
mod rel_table;
mod table;

pub mod buffer_pool;
pub mod csv;
pub mod page;
pub mod wal;

pub use column::ColumnStorage;
pub use rel_table::{CsrNodeGroup, RelTable, RelTableData, NODE_GROUP_SIZE};
pub use table::{NodeTable, TableData};

// Re-export commonly used types
pub use buffer_pool::{BufferPool, BufferPoolStats, PageHandle};
pub use csv::{CsvImportConfig, ImportProgress, ImportResult};
pub use page::{DiskManager, NodeDataPage, Page, PageId, PageType, PAGE_SIZE};
pub use wal::{
    Checkpointer, ReplayResult, WalPayload, WalReader, WalRecord, WalRecordType, WalReplayer,
    WalWriter,
};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Magic bytes for database file identification.
pub const MAGIC_BYTES: &[u8; 8] = b"RUZUDB\0\0";

/// Current database format version.
pub const CURRENT_VERSION: u32 = 1;

/// Range of pages in the database file.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct PageRange {
    /// Starting page index.
    pub start_page: u32,
    /// Number of pages in the range.
    pub num_pages: u32,
}

impl PageRange {
    /// Creates a new page range.
    #[must_use]
    pub const fn new(start_page: u32, num_pages: u32) -> Self {
        Self {
            start_page,
            num_pages,
        }
    }

    /// Returns the ending page index (exclusive).
    #[must_use]
    pub const fn end_page(&self) -> u32 {
        self.start_page + self.num_pages
    }

    /// Returns whether the range is empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.num_pages == 0
    }
}

/// Database header stored in page 0.
///
/// This header contains metadata about the database including:
/// - File format identification (magic bytes)
/// - Format version for compatibility
/// - Unique database ID
/// - Location of catalog and metadata pages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseHeader {
    /// Magic bytes for file identification ("RUZUDB\0\0").
    pub magic: [u8; 8],
    /// Database format version.
    pub version: u32,
    /// Unique database identifier.
    pub database_id: Uuid,
    /// Page range containing the serialized catalog.
    pub catalog_range: PageRange,
    /// Page range containing database metadata.
    pub metadata_range: PageRange,
    /// CRC32 checksum of the header (excluding this field).
    pub checksum: u32,
}

impl DatabaseHeader {
    /// Creates a new database header with default values.
    #[must_use]
    pub fn new(database_id: Uuid) -> Self {
        Self {
            magic: *MAGIC_BYTES,
            version: CURRENT_VERSION,
            database_id,
            catalog_range: PageRange::new(1, 0), // Catalog starts at page 1
            metadata_range: PageRange::new(0, 0),
            checksum: 0,
        }
    }

    /// Validates the header.
    ///
    /// # Errors
    ///
    /// Returns an error if the header is invalid.
    pub fn validate(&self) -> crate::error::Result<()> {
        use crate::error::RuzuError;

        if self.magic != *MAGIC_BYTES {
            return Err(RuzuError::StorageError(
                "Invalid database magic bytes".into(),
            ));
        }

        if self.version > CURRENT_VERSION {
            return Err(RuzuError::StorageError(format!(
                "Unsupported database version: {} (max supported: {})",
                self.version, CURRENT_VERSION
            )));
        }

        Ok(())
    }

    /// Computes the checksum for this header.
    #[must_use]
    pub fn compute_checksum(&self) -> u32 {
        // Serialize without checksum field
        let mut header_copy = self.clone();
        header_copy.checksum = 0;

        if let Ok(bytes) = bincode::serialize(&header_copy) {
            crc32fast::hash(&bytes)
        } else {
            0
        }
    }

    /// Updates the checksum field.
    pub fn update_checksum(&mut self) {
        self.checksum = self.compute_checksum();
    }

    /// Verifies the header checksum.
    #[must_use]
    pub fn verify_checksum(&self) -> bool {
        self.checksum == self.compute_checksum()
    }

    /// Serializes the header to bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails.
    pub fn serialize(&self) -> crate::error::Result<Vec<u8>> {
        bincode::serialize(self).map_err(|e| {
            crate::error::RuzuError::StorageError(format!("Failed to serialize header: {e}"))
        })
    }

    /// Deserializes a header from bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if deserialization fails.
    pub fn deserialize(data: &[u8]) -> crate::error::Result<Self> {
        bincode::deserialize(data).map_err(|e| {
            crate::error::RuzuError::StorageError(format!("Failed to deserialize header: {e}"))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_page_range() {
        let range = PageRange::new(10, 5);
        assert_eq!(range.start_page, 10);
        assert_eq!(range.num_pages, 5);
        assert_eq!(range.end_page(), 15);
        assert!(!range.is_empty());

        let empty = PageRange::new(0, 0);
        assert!(empty.is_empty());
    }

    #[test]
    fn test_database_header() {
        let db_id = Uuid::new_v4();
        let mut header = DatabaseHeader::new(db_id);

        assert_eq!(header.magic, *MAGIC_BYTES);
        assert_eq!(header.version, CURRENT_VERSION);
        assert_eq!(header.database_id, db_id);

        // Test checksum
        header.update_checksum();
        assert!(header.verify_checksum());
    }

    #[test]
    fn test_header_validation() {
        let header = DatabaseHeader::new(Uuid::new_v4());
        assert!(header.validate().is_ok());

        // Invalid magic
        let mut bad_header = header.clone();
        bad_header.magic = [0u8; 8];
        assert!(bad_header.validate().is_err());

        // Future version
        let mut future_header = header.clone();
        future_header.version = CURRENT_VERSION + 1;
        assert!(future_header.validate().is_err());
    }

    #[test]
    fn test_header_serialization() {
        let header = DatabaseHeader::new(Uuid::new_v4());
        let bytes = header.serialize().unwrap();
        let restored = DatabaseHeader::deserialize(&bytes).unwrap();

        assert_eq!(header.magic, restored.magic);
        assert_eq!(header.version, restored.version);
        assert_eq!(header.database_id, restored.database_id);
    }
}
