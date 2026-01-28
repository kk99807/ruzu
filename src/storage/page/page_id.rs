//! Page identifier type.

use serde::{Deserialize, Serialize};

/// Unique identifier for a page in the database.
///
/// A page is identified by:
/// - `file_id`: Which database file the page belongs to (for future multi-file support)
/// - `page_idx`: The page number within the file
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PageId {
    /// File identifier (0 for main database file).
    pub file_id: u32,
    /// Page index within the file.
    pub page_idx: u32,
}

impl PageId {
    /// Creates a new page ID.
    #[must_use]
    pub const fn new(file_id: u32, page_idx: u32) -> Self {
        Self { file_id, page_idx }
    }

    /// Creates a page ID for the main database file.
    #[must_use]
    pub const fn main(page_idx: u32) -> Self {
        Self {
            file_id: 0,
            page_idx,
        }
    }

    /// Returns the byte offset of this page within its file.
    #[must_use]
    pub const fn offset(&self) -> u64 {
        (self.page_idx as u64) * (super::PAGE_SIZE as u64)
    }

    /// Returns the next page ID (same file, incremented index).
    #[must_use]
    pub const fn next(&self) -> Self {
        Self {
            file_id: self.file_id,
            page_idx: self.page_idx + 1,
        }
    }

    /// Returns true if this is the first page (header page).
    #[must_use]
    pub const fn is_header(&self) -> bool {
        self.page_idx == 0
    }
}

impl std::fmt::Display for PageId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Page({}/{})", self.file_id, self.page_idx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::page::PAGE_SIZE;

    #[test]
    fn test_page_id_creation() {
        let id = PageId::new(1, 42);
        assert_eq!(id.file_id, 1);
        assert_eq!(id.page_idx, 42);
    }

    #[test]
    fn test_page_id_main() {
        let id = PageId::main(10);
        assert_eq!(id.file_id, 0);
        assert_eq!(id.page_idx, 10);
    }

    #[test]
    fn test_page_id_offset() {
        let id = PageId::new(0, 0);
        assert_eq!(id.offset(), 0);

        let id = PageId::new(0, 1);
        assert_eq!(id.offset(), PAGE_SIZE as u64);

        let id = PageId::new(0, 10);
        assert_eq!(id.offset(), 10 * PAGE_SIZE as u64);
    }

    #[test]
    fn test_page_id_next() {
        let id = PageId::new(1, 5);
        let next = id.next();
        assert_eq!(next.file_id, 1);
        assert_eq!(next.page_idx, 6);
    }

    #[test]
    fn test_page_id_is_header() {
        assert!(PageId::new(0, 0).is_header());
        assert!(!PageId::new(0, 1).is_header());
        assert!(PageId::new(1, 0).is_header());
    }

    #[test]
    fn test_page_id_equality() {
        let id1 = PageId::new(0, 5);
        let id2 = PageId::new(0, 5);
        let id3 = PageId::new(0, 6);
        let id4 = PageId::new(1, 5);

        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
        assert_ne!(id1, id4);
    }

    #[test]
    fn test_page_id_hash() {
        use std::collections::HashSet;

        let mut set = HashSet::new();
        set.insert(PageId::new(0, 1));
        set.insert(PageId::new(0, 2));
        set.insert(PageId::new(0, 1)); // Duplicate

        assert_eq!(set.len(), 2);
    }
}
