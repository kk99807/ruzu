//! Checkpoint coordination for WAL management.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::error::Result;
use crate::storage::buffer_pool::BufferPool;
use crate::storage::wal::WalWriter;

/// Coordinates checkpoints to flush dirty pages and truncate WAL.
pub struct Checkpointer {
    /// Next checkpoint ID.
    next_checkpoint_id: AtomicU64,
}

impl Checkpointer {
    /// Creates a new checkpointer.
    #[must_use]
    pub fn new() -> Self {
        Self {
            next_checkpoint_id: AtomicU64::new(1),
        }
    }

    /// Returns and increments the next checkpoint ID.
    pub fn next_id(&self) -> u64 {
        self.next_checkpoint_id.fetch_add(1, Ordering::Relaxed)
    }

    /// Returns the current checkpoint ID (without incrementing).
    #[must_use]
    pub fn current_id(&self) -> u64 {
        self.next_checkpoint_id.load(Ordering::Relaxed)
    }

    /// Performs a checkpoint operation.
    ///
    /// This:
    /// 1. Writes a checkpoint record to WAL
    /// 2. Flushes all dirty pages from buffer pool
    /// 3. Syncs WAL to disk
    /// 4. Truncates WAL (removes replayed records)
    ///
    /// # Errors
    ///
    /// Returns an error if any step fails.
    pub fn checkpoint(&self, buffer_pool: &BufferPool, wal_writer: &mut WalWriter) -> Result<u64> {
        use crate::storage::wal::WalRecord;

        let checkpoint_id = self.next_id();

        // Write checkpoint record
        let lsn = wal_writer.next_lsn();
        let record = WalRecord::checkpoint(0, lsn, checkpoint_id);
        wal_writer.append(&record)?;

        // Flush WAL to disk
        wal_writer.sync()?;

        // Flush all dirty pages from buffer pool
        buffer_pool.flush_all()?;

        // Truncate WAL (records are now persisted)
        wal_writer.truncate()?;

        Ok(checkpoint_id)
    }
}

impl Default for Checkpointer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checkpoint_ids() {
        let checkpointer = Checkpointer::new();

        assert_eq!(checkpointer.current_id(), 1);
        assert_eq!(checkpointer.next_id(), 1);
        assert_eq!(checkpointer.current_id(), 2);
        assert_eq!(checkpointer.next_id(), 2);
        assert_eq!(checkpointer.current_id(), 3);
    }
}
