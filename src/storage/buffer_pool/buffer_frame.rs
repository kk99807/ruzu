//! Buffer frame for holding a single cached page.

use crate::storage::page::{PageId, PAGE_SIZE};

/// A frame in the buffer pool that holds a single page.
///
/// Each frame tracks:
/// - The page currently loaded (if any)
/// - Whether the page has been modified (dirty)
/// - How many operations are currently using the page (pin count)
/// - When the page was last accessed (for LRU eviction)
#[derive(Debug)]
pub struct BufferFrame {
    /// Index of this frame in the buffer pool.
    pub frame_id: usize,
    /// The page currently loaded in this frame, if any.
    pub page_id: Option<PageId>,
    /// Raw page data.
    pub page_data: [u8; PAGE_SIZE],
    /// Number of active references to this page.
    pub pin_count: u32,
    /// Whether the page has been modified since last flush.
    pub dirty: bool,
    /// Timestamp/counter of last access for LRU ordering.
    pub last_access: u64,
}

impl BufferFrame {
    /// Creates a new empty buffer frame.
    #[must_use]
    pub fn new(frame_id: usize) -> Self {
        Self {
            frame_id,
            page_id: None,
            page_data: [0u8; PAGE_SIZE],
            pin_count: 0,
            dirty: false,
            last_access: 0,
        }
    }

    /// Increments the pin count.
    pub fn pin(&mut self) {
        self.pin_count = self.pin_count.saturating_add(1);
    }

    /// Decrements the pin count.
    pub fn unpin(&mut self) {
        self.pin_count = self.pin_count.saturating_sub(1);
    }

    /// Returns whether this frame can be evicted.
    ///
    /// A frame can be evicted if:
    /// - It has a page loaded
    /// - Its pin count is 0
    #[must_use]
    pub fn is_evictable(&self) -> bool {
        self.page_id.is_some() && self.pin_count == 0
    }

    /// Returns whether this frame is empty (no page loaded).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.page_id.is_none()
    }

    /// Resets the frame to empty state.
    pub fn reset(&mut self) {
        self.page_id = None;
        self.page_data = [0u8; PAGE_SIZE];
        self.pin_count = 0;
        self.dirty = false;
        self.last_access = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_frame() {
        let frame = BufferFrame::new(0);
        assert!(frame.is_empty());
        assert!(!frame.is_evictable());
        assert_eq!(frame.pin_count, 0);
        assert!(!frame.dirty);
    }

    #[test]
    fn test_pin_unpin() {
        let mut frame = BufferFrame::new(0);
        frame.page_id = Some(PageId::new(0, 0));

        frame.pin();
        assert_eq!(frame.pin_count, 1);
        assert!(!frame.is_evictable());

        frame.pin();
        assert_eq!(frame.pin_count, 2);

        frame.unpin();
        assert_eq!(frame.pin_count, 1);
        assert!(!frame.is_evictable());

        frame.unpin();
        assert_eq!(frame.pin_count, 0);
        assert!(frame.is_evictable());
    }

    #[test]
    fn test_reset() {
        let mut frame = BufferFrame::new(0);
        frame.page_id = Some(PageId::new(0, 1));
        frame.pin_count = 5;
        frame.dirty = true;
        frame.last_access = 100;

        frame.reset();

        assert!(frame.is_empty());
        assert_eq!(frame.pin_count, 0);
        assert!(!frame.dirty);
        assert_eq!(frame.last_access, 0);
    }
}
