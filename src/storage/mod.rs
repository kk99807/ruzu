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
pub const CURRENT_VERSION: u32 = 2;

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

    /// Returns the total byte capacity of this page range.
    #[must_use]
    pub const fn byte_capacity(&self) -> usize {
        self.num_pages as usize * PAGE_SIZE
    }

    /// Returns true if this range shares any pages with `other`.
    ///
    /// Empty ranges (num_pages == 0) never overlap with anything.
    #[must_use]
    pub const fn overlaps(&self, other: &PageRange) -> bool {
        if self.num_pages == 0 || other.num_pages == 0 {
            return false;
        }
        self.start_page < other.end_page() && other.start_page < self.end_page()
    }

    /// Returns true if the given page index is within this range.
    #[must_use]
    pub const fn contains_page(&self, page_idx: u32) -> bool {
        if self.num_pages == 0 {
            return false;
        }
        page_idx >= self.start_page && page_idx < self.end_page()
    }
}

/// Database header stored in page 0 of the database file.
///
/// The header occupies the first 4 KB page and serves as the entry point for
/// all database metadata.  It stores:
/// - **File format identification** via magic bytes (`RUZUDB\0\0`), allowing
///   tools to recognize a ruzu database file.
/// - **Format version** for forward/backward compatibility.  Version 1
///   databases (pre-relationship-persistence) are automatically migrated to
///   version 2 on open via [`DatabaseHeader::from_v1`].
/// - **Unique database ID** (UUID v4) to distinguish databases and validate
///   WAL file association.
/// - **Page ranges** locating the catalog (page 1), node table metadata
///   (page 2), and relationship table metadata (page 3).
/// - **CRC32 checksum** for integrity verification on load.
///
/// # Page Layout
///
/// | Page | Contents                      |
/// |------|-------------------------------|
/// | 0    | This header                   |
/// | 1    | Catalog (table/rel schemas)   |
/// | 2    | Node table data               |
/// | 3    | Relationship table data       |
/// | 4+   | Reserved for future expansion |
///
/// # Version History
///
/// - **Version 1**: Original format without `rel_metadata_range`.
/// - **Version 2**: Adds `rel_metadata_range` for relationship persistence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseHeader {
    /// Magic bytes for file identification.
    ///
    /// Must equal `b"RUZUDB\0\0"`.  Validated on load by [`DatabaseHeader::validate`].
    pub magic: [u8; 8],

    /// Database format version number.
    ///
    /// Current version is 2.  Version 1 databases are migrated automatically
    /// when opened (see [`DatabaseHeader::from_v1`]).
    pub version: u32,

    /// Unique identifier for this database instance.
    ///
    /// Generated as UUID v4 on database creation. Used to verify that WAL files
    /// belong to this database.
    pub database_id: Uuid,

    /// Page range containing the serialized [`Catalog`](crate::catalog::Catalog).
    ///
    /// Typically `PageRange { start_page: 1, num_pages: 1 }`.
    pub catalog_range: PageRange,

    /// Page range containing serialized node table data (`HashMap<String, TableData>`).
    ///
    /// Typically `PageRange { start_page: 2, num_pages: 1 }`.
    pub metadata_range: PageRange,

    /// Page range containing serialized relationship table data
    /// (`HashMap<String, RelTableData>`).
    ///
    /// Typically `PageRange { start_page: 3, num_pages: 1 }`.  For databases
    /// migrated from version 1, this is initially set to `PageRange::new(3, 1)`
    /// with an empty data payload (length prefix = 0).
    pub rel_metadata_range: PageRange,

    /// CRC32 checksum computed over all preceding fields (with this field zeroed).
    ///
    /// Used to detect header corruption on database open.
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
            rel_metadata_range: PageRange::new(0, 0),
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

    /// Validates that all page ranges in the header are consistent.
    ///
    /// Checks:
    /// 1. No range overlaps with page 0 (header page)
    /// 2. No two ranges overlap with each other
    /// 3. All range start pages are > 0 (for non-empty ranges)
    ///
    /// # Errors
    ///
    /// Returns `PageRangeOverlap` if any ranges overlap with each other or with page 0.
    pub fn validate_ranges(&self) -> crate::error::Result<()> {
        use crate::error::RuzuError;

        let header_page = PageRange::new(0, 1);
        let ranges = [
            ("catalog_range", &self.catalog_range),
            ("metadata_range", &self.metadata_range),
            ("rel_metadata_range", &self.rel_metadata_range),
        ];

        // Check each range doesn't overlap with header
        for (name, range) in &ranges {
            if !range.is_empty() {
                if range.start_page == 0 {
                    return Err(RuzuError::PageRangeOverlap(format!(
                        "{name} overlaps with header page 0"
                    )));
                }
                if range.overlaps(&header_page) {
                    return Err(RuzuError::PageRangeOverlap(format!(
                        "{name} overlaps with header page"
                    )));
                }
            }
        }

        // Check pairwise overlaps
        for i in 0..ranges.len() {
            for j in (i + 1)..ranges.len() {
                let (name_a, range_a) = &ranges[i];
                let (name_b, range_b) = &ranges[j];
                if range_a.overlaps(range_b) {
                    return Err(RuzuError::PageRangeOverlap(format!(
                        "{name_a} overlaps with {name_b}"
                    )));
                }
            }
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
    /// This method automatically handles version migration. If the database is version 1
    /// (missing the `rel_metadata_range` field), it will be automatically migrated to version 2.
    ///
    /// Returns a tuple of (header, was_migrated) where was_migrated is true if the database
    /// was upgraded from version 1 to version 2.
    ///
    /// # Errors
    ///
    /// Returns an error if deserialization fails for both version 2 and version 1 formats.
    pub fn deserialize(data: &[u8]) -> crate::error::Result<Self> {
        // Try to deserialize as version 2 (current format) first
        match bincode::deserialize::<Self>(data) {
            Ok(header) => Ok(header),
            Err(_) => {
                // If v2 deserialization fails, try v1 format and migrate
                let v1_header: DatabaseHeaderV1 = bincode::deserialize(data).map_err(|e| {
                    crate::error::RuzuError::StorageError(format!(
                        "Failed to deserialize header as v1 or v2: {e}"
                    ))
                })?;

                // Migrate v1 to v2
                let mut v2_header = Self::from_v1(v1_header);
                // Recompute checksum after migration
                v2_header.update_checksum();
                Ok(v2_header)
            }
        }
    }

    /// Deserializes a header and reports if migration occurred.
    ///
    /// Returns (header, was_migrated) where was_migrated is true if v1->v2 migration happened.
    ///
    /// # Errors
    ///
    /// Returns an error if deserialization fails.
    pub fn deserialize_with_migration_flag(data: &[u8]) -> crate::error::Result<(Self, bool)> {
        // Check the version field to determine which format to use
        // Version is at offset 8 (after magic bytes)
        if data.len() < 12 {
            return Err(crate::error::RuzuError::StorageError(
                "Header data too short".into(),
            ));
        }

        let version = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);

        if version == 1 {
            // Deserialize as v1 and migrate
            let v1_header: DatabaseHeaderV1 = bincode::deserialize(data).map_err(|e| {
                crate::error::RuzuError::StorageError(format!(
                    "Failed to deserialize v1 header: {e}"
                ))
            })?;

            // Migrate v1 to v2
            let mut v2_header = Self::from_v1(v1_header);
            v2_header.update_checksum();
            Ok((v2_header, true)) // Migration occurred
        } else if version == 2 {
            // Deserialize as v2
            let header = bincode::deserialize::<Self>(data).map_err(|e| {
                crate::error::RuzuError::StorageError(format!(
                    "Failed to deserialize v2 header: {e}"
                ))
            })?;
            Ok((header, false)) // No migration needed
        } else {
            Err(crate::error::RuzuError::StorageError(format!(
                "Unsupported database version: {version}"
            )))
        }
    }

    /// Migrates a version 1 header to version 2.
    ///
    /// Version 1 databases do not have the `rel_metadata_range` field.
    /// This function creates a version 2 header with relationship metadata allocated at page 3.
    #[must_use]
    pub fn from_v1(v1: DatabaseHeaderV1) -> Self {
        Self {
            magic: v1.magic,
            version: 2,
            database_id: v1.database_id,
            catalog_range: v1.catalog_range,
            metadata_range: v1.metadata_range,
            rel_metadata_range: PageRange::new(3, 1), // Allocate page 3 for relationships
            checksum: 0, // Will be recomputed
        }
    }
}

/// Database header version 1 (for migration).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseHeaderV1 {
    pub magic: [u8; 8],
    pub version: u32,
    pub database_id: Uuid,
    pub catalog_range: PageRange,
    pub metadata_range: PageRange,
    pub checksum: u32,
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
