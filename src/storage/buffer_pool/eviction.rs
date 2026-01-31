//! LRU eviction queue for the buffer pool.
//!
//! This implements a simple LRU (Least Recently Used) eviction policy.
//! Pages are added to the queue when unpinned and removed when evicted.

use std::collections::VecDeque;

/// LRU eviction queue for selecting pages to evict.
///
/// This is a simplified implementation using a FIFO queue.
/// Pages are added when unpinned and removed from the front when eviction is needed.
#[derive(Debug)]
pub struct LruEvictionQueue {
    /// Queue of frame indices in eviction order (oldest first).
    queue: VecDeque<usize>,
    /// Maximum size of the queue.
    capacity: usize,
}

impl LruEvictionQueue {
    /// Creates a new eviction queue with the given capacity.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            queue: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    /// Adds a frame to the eviction queue.
    ///
    /// If the frame is already in the queue, it's moved to the back (most recently used).
    pub fn push(&mut self, frame_idx: usize) {
        // Remove if already present (to avoid duplicates)
        self.queue.retain(|&idx| idx != frame_idx);

        // Add to back (most recently used)
        self.queue.push_back(frame_idx);

        // Trim if over capacity
        while self.queue.len() > self.capacity {
            self.queue.pop_front();
        }
    }

    /// Removes and returns the least recently used frame index.
    ///
    /// Returns `None` if the queue is empty.
    pub fn pop(&mut self) -> Option<usize> {
        self.queue.pop_front()
    }

    /// Removes a specific frame from the queue.
    ///
    /// Used when a page is pinned before being evicted.
    pub fn remove(&mut self, frame_idx: usize) {
        self.queue.retain(|&idx| idx != frame_idx);
    }

    /// Returns the number of frames in the eviction queue.
    #[must_use]
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// Returns whether the queue is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Clears all entries from the queue.
    pub fn clear(&mut self) {
        self.queue.clear();
    }

    /// Returns the frame indices in eviction order (oldest first).
    pub fn iter(&self) -> impl Iterator<Item = &usize> {
        self.queue.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_pop() {
        let mut queue = LruEvictionQueue::new(10);

        queue.push(0);
        queue.push(1);
        queue.push(2);

        assert_eq!(queue.len(), 3);
        assert_eq!(queue.pop(), Some(0)); // Oldest first
        assert_eq!(queue.pop(), Some(1));
        assert_eq!(queue.pop(), Some(2));
        assert_eq!(queue.pop(), None);
    }

    #[test]
    fn test_push_moves_to_back() {
        let mut queue = LruEvictionQueue::new(10);

        queue.push(0);
        queue.push(1);
        queue.push(2);
        queue.push(0); // Move 0 to back

        assert_eq!(queue.pop(), Some(1)); // 1 is now oldest
        assert_eq!(queue.pop(), Some(2));
        assert_eq!(queue.pop(), Some(0)); // 0 is now newest
    }

    #[test]
    fn test_remove() {
        let mut queue = LruEvictionQueue::new(10);

        queue.push(0);
        queue.push(1);
        queue.push(2);

        queue.remove(1);

        assert_eq!(queue.len(), 2);
        assert_eq!(queue.pop(), Some(0));
        assert_eq!(queue.pop(), Some(2));
    }

    #[test]
    fn test_capacity_limit() {
        let mut queue = LruEvictionQueue::new(3);

        queue.push(0);
        queue.push(1);
        queue.push(2);
        queue.push(3); // Should evict 0

        assert_eq!(queue.len(), 3);
        assert_eq!(queue.pop(), Some(1)); // 0 was evicted
    }

    #[test]
    fn test_clear() {
        let mut queue = LruEvictionQueue::new(10);

        queue.push(0);
        queue.push(1);

        queue.clear();

        assert!(queue.is_empty());
        assert_eq!(queue.pop(), None);
    }
}
