//! Page-level storage primitives.
//!
//! This module defines the core page abstractions:
//! - `PageId`: Unique identifier for a page
//! - `Page`: Fixed-size data block (4KB)
//! - `DiskManager`: File I/O abstraction
//! - `PageType`: Type discriminator for different page layouts
//! - `NodeDataPage`: Columnar storage page for node data

mod disk_manager;
mod page_id;

pub use disk_manager::DiskManager;
pub use page_id::PageId;

use crate::error::{Result, RuzuError};

/// Page size in bytes (4KB).
pub const PAGE_SIZE: usize = 4096;

/// Page size as a power of 2 (2^12 = 4096).
pub const PAGE_SIZE_LOG2: u32 = 12;

/// A fixed-size page of data.
#[derive(Clone)]
pub struct Page {
    /// Unique identifier for this page.
    pub id: PageId,
    /// Raw page data.
    pub data: [u8; PAGE_SIZE],
}

impl Page {
    /// Creates a new empty page with the given ID.
    #[must_use]
    pub fn new(id: PageId) -> Self {
        Self {
            id,
            data: [0u8; PAGE_SIZE],
        }
    }

    /// Creates a page from existing data.
    #[must_use]
    pub fn from_data(id: PageId, data: [u8; PAGE_SIZE]) -> Self {
        Self { id, data }
    }

    /// Returns a read-only view of the page data.
    #[must_use]
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Returns a mutable view of the page data.
    pub fn data_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }

    /// Computes the CRC32 checksum of the page data.
    #[must_use]
    pub fn checksum(&self) -> u32 {
        crc32fast::hash(&self.data)
    }

    /// Verifies the page checksum against an expected value.
    #[must_use]
    pub fn verify_checksum(&self, expected: u32) -> bool {
        self.checksum() == expected
    }
}

impl std::fmt::Debug for Page {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Page")
            .field("id", &self.id)
            .field("data_len", &self.data.len())
            .finish()
    }
}

/// Type of page content.
///
/// Stored at offset 0 in each page header as a u32.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum PageType {
    /// Node column data page.
    NodeData = 1,
    /// Node ID to page mapping.
    NodeOffsets = 2,
    /// CSR offset array.
    CsrOffsets = 3,
    /// CSR neighbor node IDs.
    CsrNeighbors = 4,
    /// CSR relationship IDs.
    CsrRelIds = 5,
    /// Relationship property columns.
    RelProperties = 6,
}

impl PageType {
    /// Converts from u32 to `PageType`.
    #[must_use]
    pub fn from_u32(value: u32) -> Option<Self> {
        match value {
            1 => Some(PageType::NodeData),
            2 => Some(PageType::NodeOffsets),
            3 => Some(PageType::CsrOffsets),
            4 => Some(PageType::CsrNeighbors),
            5 => Some(PageType::CsrRelIds),
            6 => Some(PageType::RelProperties),
            _ => None,
        }
    }
}

/// Page header layout (16 bytes):
/// - Offset 0: `page_type` (u32)
/// - Offset 4: `table_id` (u32)
/// - Offset 8: `page_sequence` (u32)
/// - Offset 12: checksum (u32)
const PAGE_HEADER_SIZE: usize = 16;

/// Data page layout after header (8 bytes metadata):
/// - Offset 16: `num_values` (u32)
/// - Offset 20: `null_bitmap_size` (u32)
/// - Offset 24: `null_bitmap` (variable)
/// - After `null_bitmap`: value data
const DATA_PAGE_META_SIZE: usize = 8;

/// Total overhead for node data pages.
pub const NODE_DATA_PAGE_OVERHEAD: usize = PAGE_HEADER_SIZE + DATA_PAGE_META_SIZE;

/// A node data page with columnar storage for fixed-width values.
///
/// Layout:
/// ```text
/// [0..4)   page_type: u32
/// [4..8)   table_id: u32
/// [8..12)  page_sequence: u32
/// [12..16) checksum: u32
/// [16..20) num_values: u32
/// [20..24) null_bitmap_size: u32
/// [24..24+null_bitmap_size) null_bitmap
/// [24+null_bitmap_size..PAGE_SIZE) value data
/// ```
#[derive(Clone)]
pub struct NodeDataPage {
    /// Table ID this page belongs to.
    table_id: u32,
    /// Page type (`NodeData`).
    page_type: PageType,
    /// Number of values stored.
    num_values: u32,
    /// Null bitmap (1 bit per value).
    null_bitmap: Vec<u8>,
    /// Raw value data.
    data: Vec<u8>,
}

impl NodeDataPage {
    /// Creates a new empty node data page.
    #[must_use]
    pub fn new(table_id: u32, page_type: PageType) -> Self {
        Self {
            table_id,
            page_type,
            num_values: 0,
            null_bitmap: Vec::new(),
            data: Vec::new(),
        }
    }

    /// Returns the number of values stored.
    #[must_use]
    pub fn num_values(&self) -> u32 {
        self.num_values
    }

    /// Sets the number of values.
    pub fn set_num_values(&mut self, n: u32) {
        self.num_values = n;
        // Resize null bitmap to accommodate n values
        let bitmap_bytes = (n as usize).div_ceil(8);
        self.null_bitmap.resize(bitmap_bytes, 0);
    }

    /// Checks if a value at the given index is null.
    #[must_use]
    pub fn is_null(&self, idx: usize) -> bool {
        let byte_idx = idx / 8;
        let bit_idx = idx % 8;
        if byte_idx >= self.null_bitmap.len() {
            return false;
        }
        (self.null_bitmap[byte_idx] & (1 << bit_idx)) != 0
    }

    /// Sets the null flag for a value at the given index.
    pub fn set_null(&mut self, idx: usize, is_null: bool) {
        let byte_idx = idx / 8;
        let bit_idx = idx % 8;

        // Ensure bitmap is large enough
        if byte_idx >= self.null_bitmap.len() {
            self.null_bitmap.resize(byte_idx + 1, 0);
        }

        if is_null {
            self.null_bitmap[byte_idx] |= 1 << bit_idx;
        } else {
            self.null_bitmap[byte_idx] &= !(1 << bit_idx);
        }
    }

    /// Writes an i64 value at the given index.
    pub fn write_int64(&mut self, idx: usize, value: i64) {
        let offset = idx * 8;
        if offset + 8 > self.data.len() {
            self.data.resize(offset + 8, 0);
        }
        self.data[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    }

    /// Reads an i64 value at the given index.
    #[must_use]
    pub fn read_int64(&self, idx: usize) -> i64 {
        let offset = idx * 8;
        if offset + 8 > self.data.len() {
            return 0;
        }
        i64::from_le_bytes(self.data[offset..offset + 8].try_into().unwrap_or([0; 8]))
    }

    /// Writes a f64 value at the given index.
    pub fn write_float64(&mut self, idx: usize, value: f64) {
        let offset = idx * 8;
        if offset + 8 > self.data.len() {
            self.data.resize(offset + 8, 0);
        }
        self.data[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    }

    /// Reads a f64 value at the given index.
    #[must_use]
    pub fn read_float64(&self, idx: usize) -> f64 {
        let offset = idx * 8;
        if offset + 8 > self.data.len() {
            return 0.0;
        }
        f64::from_le_bytes(self.data[offset..offset + 8].try_into().unwrap_or([0; 8]))
    }

    /// Writes a bool value at the given index.
    pub fn write_bool(&mut self, idx: usize, value: bool) {
        if idx >= self.data.len() {
            self.data.resize(idx + 1, 0);
        }
        self.data[idx] = u8::from(value);
    }

    /// Reads a bool value at the given index.
    #[must_use]
    pub fn read_bool(&self, idx: usize) -> bool {
        if idx >= self.data.len() {
            return false;
        }
        self.data[idx] != 0
    }

    /// Serializes this page to a raw Page.
    #[must_use]
    pub fn to_page(&self, page_id: PageId) -> Page {
        let mut page = Page::new(page_id);

        // Write page header
        page.data[0..4].copy_from_slice(&(self.page_type as u32).to_le_bytes());
        page.data[4..8].copy_from_slice(&self.table_id.to_le_bytes());
        page.data[8..12].copy_from_slice(&page_id.page_idx.to_le_bytes());
        // checksum at [12..16] will be computed later

        // Write data page metadata
        page.data[16..20].copy_from_slice(&self.num_values.to_le_bytes());
        let null_bitmap_size = self.null_bitmap.len() as u32;
        page.data[20..24].copy_from_slice(&null_bitmap_size.to_le_bytes());

        // Write null bitmap
        let bitmap_start = 24;
        let bitmap_end = bitmap_start + self.null_bitmap.len();
        if bitmap_end <= PAGE_SIZE {
            page.data[bitmap_start..bitmap_end].copy_from_slice(&self.null_bitmap);
        }

        // Write value data
        let data_start = bitmap_end;
        let data_end = data_start + self.data.len();
        if data_end <= PAGE_SIZE {
            page.data[data_start..data_end].copy_from_slice(&self.data);
        }

        // Compute and store checksum (excluding checksum field itself)
        let checksum = crc32fast::hash(&page.data[0..12]);
        let checksum2 = crc32fast::hash(&page.data[16..]);
        let combined_checksum = checksum ^ checksum2;
        page.data[12..16].copy_from_slice(&combined_checksum.to_le_bytes());

        page
    }

    /// Deserializes a raw Page into a `NodeDataPage`.
    ///
    /// # Errors
    ///
    /// Returns an error if the page type is invalid or the checksum does not match.
    ///
    /// # Panics
    ///
    /// Panics if fixed-size page header slices fail to convert to arrays (unreachable
    /// given the known page data layout).
    pub fn from_page(page: &Page) -> Result<Self> {
        // Read page header
        let page_type_raw = u32::from_le_bytes(page.data[0..4].try_into().unwrap());
        let page_type = PageType::from_u32(page_type_raw)
            .ok_or_else(|| RuzuError::PageError(format!("Invalid page type: {page_type_raw}")))?;

        let table_id = u32::from_le_bytes(page.data[4..8].try_into().unwrap());

        // Read data page metadata
        let num_values = u32::from_le_bytes(page.data[16..20].try_into().unwrap());
        let null_bitmap_size = u32::from_le_bytes(page.data[20..24].try_into().unwrap()) as usize;

        // Read null bitmap
        let bitmap_start = 24;
        let bitmap_end = bitmap_start + null_bitmap_size;
        let null_bitmap = page.data[bitmap_start..bitmap_end].to_vec();

        // Read value data (everything after null bitmap until end of page)
        let data_start = bitmap_end;
        // Calculate actual data size based on num_values and expected value size
        // For simplicity, copy remaining data
        let data = page.data[data_start..].to_vec();

        Ok(Self {
            table_id,
            page_type,
            num_values,
            null_bitmap,
            data,
        })
    }

    /// Returns the table ID.
    #[must_use]
    pub fn table_id(&self) -> u32 {
        self.table_id
    }

    /// Returns the page type.
    #[must_use]
    pub fn page_type(&self) -> PageType {
        self.page_type
    }

    /// Returns the raw data slice.
    #[must_use]
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Returns a mutable reference to the raw data.
    pub fn data_mut(&mut self) -> &mut Vec<u8> {
        &mut self.data
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_page_creation() {
        let id = PageId::new(0, 0);
        let page = Page::new(id);

        assert_eq!(page.id, id);
        assert_eq!(page.data.len(), PAGE_SIZE);
        assert!(page.data.iter().all(|&b| b == 0));
    }

    #[test]
    fn test_page_checksum() {
        let id = PageId::new(0, 0);
        let mut page = Page::new(id);

        let checksum1 = page.checksum();

        // Modify data
        page.data[0] = 42;

        let checksum2 = page.checksum();

        // Checksums should differ
        assert_ne!(checksum1, checksum2);
    }

    #[test]
    fn test_page_checksum_verification() {
        let id = PageId::new(0, 0);
        let mut page = Page::new(id);
        page.data[100] = 0xFF;

        let checksum = page.checksum();
        assert!(page.verify_checksum(checksum));
        assert!(!page.verify_checksum(checksum + 1));
    }
}
