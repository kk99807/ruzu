//! Buffer pool management for page caching.
//!
//! This module implements a buffer pool that manages in-memory caching of database pages.
//! It provides:
//! - Page pinning and unpinning with reference counting
//! - LRU eviction policy for memory management
//! - RAII guards (`PageHandle`) for safe page access
//!
//! # Architecture
//!
//! The buffer pool uses memory-mapped I/O via the `VmRegion` for efficient page access.
//! Pages transition through states: EVICTED → LOCKED → MARKED → UNLOCKED → EVICTED.
//!
//! # Example
//!
//! ```ignore
//! let pool = BufferPool::new(capacity, disk_manager)?;
//! let handle = pool.pin(page_id)?;
//! // Read/write page data via handle
//! // Page automatically unpinned when handle drops
//! ```

mod buffer_frame;
mod eviction;
mod page_state;
mod vm_region;

pub use buffer_frame::BufferFrame;
pub use eviction::LruEvictionQueue;
pub use page_state::{PageState, PageStateValue};
pub use vm_region::VmRegion;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use parking_lot::RwLock;

use crate::error::{Result, RuzuError};
use crate::storage::page::{DiskManager, Page, PageId, PAGE_SIZE};

/// Buffer pool for managing in-memory page cache.
pub struct BufferPool {
    /// Buffer frames holding cached pages.
    frames: Vec<RwLock<BufferFrame>>,
    /// Maps page IDs to frame indices.
    page_table: RwLock<HashMap<PageId, usize>>,
    /// LRU eviction queue for finding eviction candidates.
    eviction_queue: RwLock<LruEvictionQueue>,
    /// Maximum number of pages in the pool.
    capacity: usize,
    /// Disk manager for page I/O.
    disk_manager: RwLock<DiskManager>,
    /// Monotonically increasing access counter for LRU ordering.
    access_counter: AtomicU64,
    /// Counter for cache hits (page found in buffer pool).
    cache_hits: AtomicU64,
    /// Counter for cache misses (page had to be loaded from disk).
    cache_misses: AtomicU64,
    /// Counter for number of pages evicted.
    evictions: AtomicU64,
}

impl BufferPool {
    /// Creates a new buffer pool with the given capacity and disk manager.
    ///
    /// # Arguments
    ///
    /// * `capacity` - Maximum number of pages to cache in memory
    /// * `disk_manager` - Disk manager for reading/writing pages
    ///
    /// # Errors
    ///
    /// Returns an error if the buffer pool cannot be initialized.
    pub fn new(capacity: usize, disk_manager: DiskManager) -> Result<Self> {
        if capacity == 0 {
            return Err(RuzuError::StorageError(
                "Buffer pool capacity must be greater than 0".into(),
            ));
        }

        let frames = (0..capacity)
            .map(|i| RwLock::new(BufferFrame::new(i)))
            .collect();

        let eviction_queue = LruEvictionQueue::new(capacity);

        Ok(Self {
            frames,
            page_table: RwLock::new(HashMap::with_capacity(capacity)),
            eviction_queue: RwLock::new(eviction_queue),
            capacity,
            disk_manager: RwLock::new(disk_manager),
            access_counter: AtomicU64::new(0),
            cache_hits: AtomicU64::new(0),
            cache_misses: AtomicU64::new(0),
            evictions: AtomicU64::new(0),
        })
    }

    /// Returns the capacity of the buffer pool.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Returns the number of pages currently in the buffer pool.
    #[must_use]
    pub fn size(&self) -> usize {
        self.page_table.read().len()
    }

    /// Pins a page in the buffer pool, loading it from disk if necessary.
    ///
    /// Returns a `PageHandle` that provides access to the page data.
    /// The page remains pinned until all handles are dropped.
    ///
    /// # Errors
    ///
    /// Returns an error if the page cannot be loaded or no frames are available.
    pub fn pin(&self, page_id: PageId) -> Result<PageHandle<'_>> {
        // Check if page is already in buffer pool
        {
            let page_table = self.page_table.read();
            if let Some(&frame_idx) = page_table.get(&page_id) {
                let mut frame = self.frames[frame_idx].write();
                frame.pin();
                frame.last_access = self.access_counter.fetch_add(1, Ordering::Relaxed);
                // Record cache hit
                self.cache_hits.fetch_add(1, Ordering::Relaxed);
                return Ok(PageHandle {
                    pool: self,
                    frame_idx,
                    page_id,
                });
            }
        }

        // Page not in pool - need to load it (cache miss)
        self.cache_misses.fetch_add(1, Ordering::Relaxed);
        let frame_idx = self.find_or_evict_frame()?;

        // Load page from disk
        let page = {
            let mut dm = self.disk_manager.write();
            dm.read_page(page_id)?
        };

        // Initialize frame with the loaded page
        {
            let mut frame = self.frames[frame_idx].write();
            frame.page_id = Some(page_id);
            frame.page_data = page.data;
            frame.pin();
            frame.dirty = false;
            frame.last_access = self.access_counter.fetch_add(1, Ordering::Relaxed);
        }

        // Update page table
        {
            let mut page_table = self.page_table.write();
            page_table.insert(page_id, frame_idx);
        }

        Ok(PageHandle {
            pool: self,
            frame_idx,
            page_id,
        })
    }

    /// Allocates a new page and pins it in the buffer pool.
    ///
    /// # Errors
    ///
    /// Returns an error if no frames are available or allocation fails.
    pub fn new_page(&self) -> Result<PageHandle<'_>> {
        let frame_idx = self.find_or_evict_frame()?;

        // Allocate new page on disk
        let page_id = {
            let mut dm = self.disk_manager.write();
            dm.allocate_page()?
        };

        // Initialize frame with empty page
        {
            let mut frame = self.frames[frame_idx].write();
            frame.page_id = Some(page_id);
            frame.page_data = [0u8; PAGE_SIZE];
            frame.pin();
            frame.dirty = true; // New page is dirty
            frame.last_access = self.access_counter.fetch_add(1, Ordering::Relaxed);
        }

        // Update page table
        {
            let mut page_table = self.page_table.write();
            page_table.insert(page_id, frame_idx);
        }

        Ok(PageHandle {
            pool: self,
            frame_idx,
            page_id,
        })
    }

    /// Allocates a contiguous range of pages on disk without pinning them.
    ///
    /// Returns the `PageRange` for the newly allocated pages.
    ///
    /// # Errors
    ///
    /// Returns an error if disk allocation fails.
    pub fn allocate_page_range(&self, num_pages: u32) -> Result<crate::storage::PageRange> {
        let mut dm = self.disk_manager.write();
        dm.allocate_page_range(num_pages)
    }

    /// Returns the total number of pages allocated in the database file.
    #[must_use]
    pub fn file_page_count(&self) -> u32 {
        self.disk_manager.read().num_pages()
    }

    /// Flushes a specific page to disk if it's dirty.
    ///
    /// # Errors
    ///
    /// Returns an error if the page cannot be written to disk.
    pub fn flush_page(&self, page_id: PageId) -> Result<()> {
        let frame_idx = {
            let page_table = self.page_table.read();
            match page_table.get(&page_id) {
                Some(&idx) => idx,
                None => return Ok(()), // Page not in pool, nothing to flush
            }
        };

        let (data, dirty) = {
            let frame = self.frames[frame_idx].read();
            (frame.page_data, frame.dirty)
        };

        if dirty {
            let page = Page { id: page_id, data };
            let mut dm = self.disk_manager.write();
            dm.write_page(&page)?;

            let mut frame = self.frames[frame_idx].write();
            frame.dirty = false;
        }

        Ok(())
    }

    /// Flushes all dirty pages to disk.
    ///
    /// # Errors
    ///
    /// Returns an error if any page cannot be written to disk.
    pub fn flush_all(&self) -> Result<()> {
        let page_ids: Vec<PageId> = {
            let page_table = self.page_table.read();
            page_table.keys().copied().collect()
        };

        for page_id in page_ids {
            self.flush_page(page_id)?;
        }

        Ok(())
    }

    /// Internal: Unpins a page (called when `PageHandle` is dropped).
    fn unpin(&self, frame_idx: usize, is_dirty: bool) {
        let mut frame = self.frames[frame_idx].write();
        if is_dirty {
            frame.dirty = true;
        }
        frame.unpin();

        // If pin count reaches 0, add to eviction queue
        if frame.pin_count == 0 {
            let mut queue = self.eviction_queue.write();
            queue.push(frame_idx);
        }
    }

    /// Internal: Finds an empty frame or evicts one.
    fn find_or_evict_frame(&self) -> Result<usize> {
        // First, try to find an empty frame
        for (idx, frame_lock) in self.frames.iter().enumerate() {
            let frame = frame_lock.read();
            if frame.page_id.is_none() {
                return Ok(idx);
            }
        }

        // All frames are in use - need to evict
        self.evict_frame()
    }

    /// Internal: Evicts a frame using LRU policy.
    fn evict_frame(&self) -> Result<usize> {
        let mut queue = self.eviction_queue.write();

        while let Some(frame_idx) = queue.pop() {
            let mut frame = self.frames[frame_idx].write();

            // Skip if still pinned
            if frame.pin_count > 0 {
                continue;
            }

            // Found an evictable frame
            if let Some(page_id) = frame.page_id {
                // Flush if dirty
                if frame.dirty {
                    let page = Page {
                        id: page_id,
                        data: frame.page_data,
                    };
                    let mut dm = self.disk_manager.write();
                    dm.write_page(&page)?;
                }

                // Remove from page table
                let mut page_table = self.page_table.write();
                page_table.remove(&page_id);

                // Record eviction
                self.evictions.fetch_add(1, Ordering::Relaxed);
            }

            // Reset frame
            frame.page_id = None;
            frame.dirty = false;
            frame.pin_count = 0;

            return Ok(frame_idx);
        }

        Err(RuzuError::StorageError(
            "Buffer pool is full and no pages can be evicted".into(),
        ))
    }

    /// Returns buffer pool statistics.
    #[must_use]
    pub fn stats(&self) -> BufferPoolStats {
        let page_table = self.page_table.read();
        let mut dirty_count = 0;
        let mut pinned_count = 0;

        for frame_lock in &self.frames {
            let frame = frame_lock.read();
            if frame.page_id.is_some() {
                if frame.dirty {
                    dirty_count += 1;
                }
                if frame.pin_count > 0 {
                    pinned_count += 1;
                }
            }
        }

        let hits = self.cache_hits.load(Ordering::Relaxed);
        let misses = self.cache_misses.load(Ordering::Relaxed);
        let evictions = self.evictions.load(Ordering::Relaxed);

        BufferPoolStats {
            capacity: self.capacity,
            pages_used: page_table.len(),
            dirty_pages: dirty_count,
            pinned_pages: pinned_count,
            cache_hits: hits,
            cache_misses: misses,
            evictions,
        }
    }

    /// Resets the cache statistics counters.
    ///
    /// This is useful for benchmarking or monitoring specific workloads.
    pub fn reset_stats(&self) {
        self.cache_hits.store(0, Ordering::Relaxed);
        self.cache_misses.store(0, Ordering::Relaxed);
        self.evictions.store(0, Ordering::Relaxed);
    }
}

/// RAII guard for a pinned page.
///
/// Automatically unpins the page when dropped.
pub struct PageHandle<'a> {
    pool: &'a BufferPool,
    frame_idx: usize,
    page_id: PageId,
    // Note: dirty flag is tracked per-write operation
}

impl PageHandle<'_> {
    /// Returns the page ID.
    #[must_use]
    pub fn page_id(&self) -> PageId {
        self.page_id
    }

    /// Returns a read-only view of the page data.
    #[must_use]
    #[allow(unsafe_code)]
    pub fn data(&self) -> &[u8] {
        let frame = self.pool.frames[self.frame_idx].read();
        // SAFETY: The data lives as long as the frame, and we hold a reference to the pool
        // This is safe because the frame cannot be evicted while pinned
        unsafe { std::slice::from_raw_parts(frame.page_data.as_ptr(), PAGE_SIZE) }
    }

    /// Returns a mutable view of the page data and marks the page as dirty.
    #[allow(unsafe_code)]
    pub fn data_mut(&mut self) -> &mut [u8] {
        let mut frame = self.pool.frames[self.frame_idx].write();
        frame.dirty = true;
        // SAFETY: Same as above - frame cannot be evicted while pinned
        unsafe { std::slice::from_raw_parts_mut(frame.page_data.as_mut_ptr(), PAGE_SIZE) }
    }
}

impl Drop for PageHandle<'_> {
    fn drop(&mut self) {
        self.pool.unpin(self.frame_idx, false);
    }
}

/// Statistics about the buffer pool state.
#[derive(Debug, Clone)]
pub struct BufferPoolStats {
    /// Maximum number of pages the pool can hold.
    pub capacity: usize,
    /// Current number of pages in the pool.
    pub pages_used: usize,
    /// Number of dirty pages.
    pub dirty_pages: usize,
    /// Number of pinned pages.
    pub pinned_pages: usize,
    /// Number of cache hits (page found in buffer pool).
    pub cache_hits: u64,
    /// Number of cache misses (page had to be loaded from disk).
    pub cache_misses: u64,
    /// Number of pages evicted.
    pub evictions: u64,
}

impl BufferPoolStats {
    /// Calculates the cache hit rate as a percentage (0.0 to 1.0).
    ///
    /// Returns `None` if there have been no cache accesses.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn hit_rate(&self) -> Option<f64> {
        let total = self.cache_hits + self.cache_misses;
        if total == 0 {
            None
        } else {
            Some(self.cache_hits as f64 / total as f64)
        }
    }

    /// Returns the total number of cache accesses (hits + misses).
    #[must_use]
    pub fn total_accesses(&self) -> u64 {
        self.cache_hits + self.cache_misses
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_pool(capacity: usize) -> (BufferPool, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let dm = DiskManager::new(&db_path).unwrap();
        let pool = BufferPool::new(capacity, dm).unwrap();
        (pool, temp_dir)
    }

    #[test]
    fn test_new_page() {
        let (pool, _temp) = create_test_pool(10);
        let handle = pool.new_page().unwrap();
        assert_eq!(handle.page_id().page_idx, 0);
        assert_eq!(pool.size(), 1);
    }

    #[test]
    fn test_pin_unpin() {
        let (pool, _temp) = create_test_pool(10);

        // Create and modify a page
        let page_id = {
            let mut handle = pool.new_page().unwrap();
            let page_id = handle.page_id();
            handle.data_mut()[0] = 42;
            page_id
        }; // Handle dropped here, page unpinned

        // Pin the same page again
        let handle = pool.pin(page_id).unwrap();
        assert_eq!(handle.data()[0], 42);
    }

    #[test]
    fn test_flush() {
        let (pool, _temp) = create_test_pool(10);

        let page_id = {
            let mut handle = pool.new_page().unwrap();
            handle.data_mut()[0..4].copy_from_slice(&[1, 2, 3, 4]);
            handle.page_id()
        };

        pool.flush_page(page_id).unwrap();

        // Verify data persisted by re-reading
        let handle = pool.pin(page_id).unwrap();
        assert_eq!(&handle.data()[0..4], &[1, 2, 3, 4]);
    }

    #[test]
    fn test_cache_hit_miss_tracking() {
        let (pool, _temp) = create_test_pool(10);

        // Initially no hits or misses
        let stats = pool.stats();
        assert_eq!(stats.cache_hits, 0);
        assert_eq!(stats.cache_misses, 0);
        assert!(stats.hit_rate().is_none());

        // Create a new page (not a pin, no cache miss counted for new_page)
        let page_id = {
            let handle = pool.new_page().unwrap();
            handle.page_id()
        };

        // Pin the same page - should be a cache hit
        {
            let _handle = pool.pin(page_id).unwrap();
        }

        let stats = pool.stats();
        assert_eq!(stats.cache_hits, 1);
        assert_eq!(stats.cache_misses, 0);
        assert_eq!(stats.hit_rate(), Some(1.0));

        // Reset stats
        pool.reset_stats();
        let stats = pool.stats();
        assert_eq!(stats.cache_hits, 0);
        assert_eq!(stats.cache_misses, 0);
    }

    #[test]
    fn test_eviction_tracking() {
        // Small pool to force eviction
        let (pool, _temp) = create_test_pool(4);

        // Allocate more pages than capacity
        let mut page_ids = Vec::new();
        for _ in 0..6 {
            let handle = pool.new_page().unwrap();
            page_ids.push(handle.page_id());
        }

        let stats = pool.stats();
        // At least 2 evictions should have occurred (6 pages, 4 capacity)
        assert!(
            stats.evictions >= 2,
            "Expected at least 2 evictions, got {}",
            stats.evictions
        );
    }

    #[test]
    fn test_hit_rate_calculation() {
        let stats = BufferPoolStats {
            capacity: 10,
            pages_used: 5,
            dirty_pages: 1,
            pinned_pages: 2,
            cache_hits: 80,
            cache_misses: 20,
            evictions: 5,
        };

        assert_eq!(stats.hit_rate(), Some(0.8));
        assert_eq!(stats.total_accesses(), 100);
    }
}
