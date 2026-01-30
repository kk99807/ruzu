//! Disk manager for page-level I/O.

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use crate::error::{Result, RuzuError};
use crate::storage::page::{Page, PageId, PAGE_SIZE};

/// Manages disk I/O for database pages.
///
/// The disk manager handles:
/// - Reading and writing pages to/from disk
/// - Allocating new pages
/// - Managing the database file
pub struct DiskManager {
    /// Path to the database file.
    path: PathBuf,
    /// File handle for the database file.
    file: File,
    /// Next available page index.
    next_page_idx: AtomicU32,
}

impl DiskManager {
    /// Opens or creates a database file.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be opened or created.
    pub fn new(path: &Path) -> Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(|e| RuzuError::StorageError(format!("Failed to open database file: {e}")))?;

        let file_len = file
            .metadata()
            .map_err(|e| RuzuError::StorageError(format!("Failed to get file metadata: {e}")))?
            .len();

        // Calculate next page index from file size
        let next_page_idx = if file_len == 0 {
            0
        } else {
            file_len.div_ceil(PAGE_SIZE as u64) as u32
        };

        Ok(Self {
            path: path.to_path_buf(),
            file,
            next_page_idx: AtomicU32::new(next_page_idx),
        })
    }

    /// Returns the path to the database file.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the number of pages in the database file.
    #[must_use]
    pub fn num_pages(&self) -> u32 {
        self.next_page_idx.load(Ordering::Relaxed)
    }

    /// Reads a page from disk.
    ///
    /// If the page doesn't exist yet (beyond current file size), returns a zeroed page.
    ///
    /// # Errors
    ///
    /// Returns an error if the read fails.
    pub fn read_page(&mut self, page_id: PageId) -> Result<Page> {
        let offset = page_id.offset();

        // Seek to page offset
        self.file
            .seek(SeekFrom::Start(offset))
            .map_err(|e| RuzuError::StorageError(format!("Failed to seek to page: {e}")))?;

        let mut data = [0u8; PAGE_SIZE];

        // Try to read the page
        match self.file.read_exact(&mut data) {
            Ok(()) => Ok(Page::from_data(page_id, data)),
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                // Page doesn't exist yet - return empty page
                Ok(Page::new(page_id))
            }
            Err(e) => Err(RuzuError::StorageError(format!(
                "Failed to read page {page_id}: {e}"
            ))),
        }
    }

    /// Writes a page to disk.
    ///
    /// # Errors
    ///
    /// Returns an error if the write fails.
    pub fn write_page(&mut self, page: &Page) -> Result<()> {
        let offset = page.id.offset();

        // Seek to page offset
        self.file
            .seek(SeekFrom::Start(offset))
            .map_err(|e| RuzuError::StorageError(format!("Failed to seek to page: {e}")))?;

        // Write page data
        self.file
            .write_all(&page.data)
            .map_err(|e| RuzuError::StorageError(format!("Failed to write page: {e}")))?;

        Ok(())
    }

    /// Allocates a new page and returns its ID.
    ///
    /// The page is not written to disk until explicitly written.
    ///
    /// # Errors
    ///
    /// Returns an error if allocation fails.
    pub fn allocate_page(&mut self) -> Result<PageId> {
        let page_idx = self.next_page_idx.fetch_add(1, Ordering::Relaxed);
        let page_id = PageId::main(page_idx);

        // Extend the file to include the new page
        let new_size = (u64::from(page_idx) + 1) * PAGE_SIZE as u64;
        self.file
            .set_len(new_size)
            .map_err(|e| RuzuError::StorageError(format!("Failed to extend file: {e}")))?;

        Ok(page_id)
    }

    /// Allocates a contiguous range of pages in the database file.
    ///
    /// Extends the file to accommodate `num_pages` new pages and returns
    /// a `PageRange` identifying the allocated region.
    ///
    /// # Errors
    ///
    /// Returns an error if `num_pages` is 0 or if file extension fails.
    pub fn allocate_page_range(&mut self, num_pages: u32) -> Result<crate::storage::PageRange> {
        if num_pages == 0 {
            return Err(RuzuError::StorageError(
                "Cannot allocate 0 pages".into(),
            ));
        }

        let start_page = self.next_page_idx.fetch_add(num_pages, Ordering::Relaxed);
        let new_size = (u64::from(start_page) + u64::from(num_pages)) * PAGE_SIZE as u64;
        self.file
            .set_len(new_size)
            .map_err(|e| RuzuError::StorageError(format!("Failed to extend file: {e}")))?;

        Ok(crate::storage::PageRange::new(start_page, num_pages))
    }

    /// Flushes all buffered writes to disk.
    ///
    /// # Errors
    ///
    /// Returns an error if the sync fails.
    pub fn sync(&mut self) -> Result<()> {
        self.file
            .sync_all()
            .map_err(|e| RuzuError::StorageError(format!("Failed to sync file: {e}")))
    }

    /// Returns the size of the database file in bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if the metadata cannot be read.
    pub fn file_size(&self) -> Result<u64> {
        self.file
            .metadata()
            .map(|m| m.len())
            .map_err(|e| RuzuError::StorageError(format!("Failed to get file size: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_dm() -> (DiskManager, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let dm = DiskManager::new(&db_path).unwrap();
        (dm, temp_dir)
    }

    #[test]
    fn test_create_disk_manager() {
        let (dm, _temp) = create_test_dm();
        assert_eq!(dm.num_pages(), 0);
    }

    #[test]
    fn test_allocate_page() {
        let (mut dm, _temp) = create_test_dm();

        let page1 = dm.allocate_page().unwrap();
        assert_eq!(page1.page_idx, 0);

        let page2 = dm.allocate_page().unwrap();
        assert_eq!(page2.page_idx, 1);

        assert_eq!(dm.num_pages(), 2);
    }

    #[test]
    fn test_read_write_page() {
        let (mut dm, _temp) = create_test_dm();

        let page_id = dm.allocate_page().unwrap();

        // Create and write a page
        let mut page = Page::new(page_id);
        page.data[0] = 42;
        page.data[100] = 0xFF;
        dm.write_page(&page).unwrap();

        // Read it back
        let read_page = dm.read_page(page_id).unwrap();
        assert_eq!(read_page.data[0], 42);
        assert_eq!(read_page.data[100], 0xFF);
    }

    #[test]
    fn test_read_nonexistent_page() {
        let (mut dm, _temp) = create_test_dm();

        // Try to read a page that doesn't exist
        let page = dm.read_page(PageId::main(100)).unwrap();

        // Should return empty page
        assert!(page.data.iter().all(|&b| b == 0));
    }

    #[test]
    fn test_persistence() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        // Write data
        {
            let mut dm = DiskManager::new(&db_path).unwrap();
            let page_id = dm.allocate_page().unwrap();

            let mut page = Page::new(page_id);
            page.data[0..4].copy_from_slice(&[1, 2, 3, 4]);
            dm.write_page(&page).unwrap();
            dm.sync().unwrap();
        }

        // Read in new instance
        {
            let mut dm = DiskManager::new(&db_path).unwrap();
            assert_eq!(dm.num_pages(), 1);

            let page = dm.read_page(PageId::main(0)).unwrap();
            assert_eq!(&page.data[0..4], &[1, 2, 3, 4]);
        }
    }

    #[test]
    fn test_file_size() {
        let (mut dm, _temp) = create_test_dm();

        dm.allocate_page().unwrap();
        dm.allocate_page().unwrap();

        let size = dm.file_size().unwrap();
        assert_eq!(size, 2 * PAGE_SIZE as u64);
    }
}
