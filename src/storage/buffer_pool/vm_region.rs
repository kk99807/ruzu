//! Virtual memory region for memory-mapped I/O.
//!
//! This module provides a safe wrapper around memory-mapped files
//! for efficient page I/O operations.

use std::fs::{File, OpenOptions};
use std::io;
use std::path::Path;

use memmap2::MmapMut;

use crate::storage::page::PAGE_SIZE;

/// A memory-mapped region for efficient page I/O.
///
/// This struct manages a memory-mapped file that can be used for
/// reading and writing database pages directly to/from memory.
///
/// # Safety
///
/// The mmap operations require `unsafe` code because:
/// 1. The mapped memory can be invalidated if the file is modified by another process
/// 2. Care must be taken to ensure proper synchronization
///
/// This implementation assumes exclusive access to the database file.
pub struct VmRegion {
    /// The underlying memory-mapped file.
    mmap: MmapMut,
    /// Size of each frame in bytes.
    frame_size: usize,
    /// Number of frames in the region.
    num_frames: usize,
    /// The backing file handle (kept open for the lifetime of the mmap).
    _file: File,
}

impl VmRegion {
    /// Creates a new memory-mapped region.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the file to map
    /// * `num_frames` - Number of frames to allocate
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be opened or mapped.
    ///
    /// # Safety
    ///
    /// The caller must ensure exclusive access to the file.
    #[allow(unsafe_code)]
    pub fn new(path: &Path, num_frames: usize) -> io::Result<Self> {
        let size = num_frames * PAGE_SIZE;

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;

        // Ensure file is large enough
        let current_len = file.metadata()?.len();
        if current_len < size as u64 {
            file.set_len(size as u64)?;
        }

        // SAFETY: We assume exclusive access to the database file.
        // The file handle is kept alive for the lifetime of the mmap.
        let mmap = unsafe { MmapMut::map_mut(&file)? };

        Ok(Self {
            mmap,
            frame_size: PAGE_SIZE,
            num_frames,
            _file: file,
        })
    }

    /// Returns the number of frames in the region.
    #[must_use]
    pub fn num_frames(&self) -> usize {
        self.num_frames
    }

    /// Returns the size of each frame in bytes.
    #[must_use]
    pub fn frame_size(&self) -> usize {
        self.frame_size
    }

    /// Returns the total size of the region in bytes.
    #[must_use]
    pub fn total_size(&self) -> usize {
        self.mmap.len()
    }

    /// Returns a read-only view of a frame.
    ///
    /// # Panics
    ///
    /// Panics if `frame_idx` is out of bounds.
    #[must_use]
    pub fn get_frame(&self, frame_idx: usize) -> &[u8] {
        assert!(frame_idx < self.num_frames, "Frame index out of bounds");
        let start = frame_idx * self.frame_size;
        let end = start + self.frame_size;
        &self.mmap[start..end]
    }

    /// Returns a mutable view of a frame.
    ///
    /// # Panics
    ///
    /// Panics if `frame_idx` is out of bounds.
    pub fn get_frame_mut(&mut self, frame_idx: usize) -> &mut [u8] {
        assert!(frame_idx < self.num_frames, "Frame index out of bounds");
        let start = frame_idx * self.frame_size;
        let end = start + self.frame_size;
        &mut self.mmap[start..end]
    }

    /// Flushes changes to the underlying file.
    ///
    /// # Errors
    ///
    /// Returns an error if the flush fails.
    pub fn flush(&self) -> io::Result<()> {
        self.mmap.flush()
    }

    /// Flushes changes to the underlying file asynchronously.
    ///
    /// # Errors
    ///
    /// Returns an error if the flush fails.
    pub fn flush_async(&self) -> io::Result<()> {
        self.mmap.flush_async()
    }

    /// Flushes a specific range of the region.
    ///
    /// # Errors
    ///
    /// Returns an error if the flush fails.
    pub fn flush_range(&self, frame_idx: usize, count: usize) -> io::Result<()> {
        let start = frame_idx * self.frame_size;
        let len = count * self.frame_size;
        self.mmap.flush_range(start, len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_create_region() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("test.db");

        let region = VmRegion::new(&path, 10).unwrap();

        assert_eq!(region.num_frames(), 10);
        assert_eq!(region.frame_size(), PAGE_SIZE);
        assert_eq!(region.total_size(), 10 * PAGE_SIZE);
    }

    #[test]
    fn test_read_write_frame() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("test.db");

        let mut region = VmRegion::new(&path, 10).unwrap();

        // Write to frame 0
        let frame = region.get_frame_mut(0);
        frame[0] = 0x42;
        frame[1] = 0x43;

        // Read back
        let frame = region.get_frame(0);
        assert_eq!(frame[0], 0x42);
        assert_eq!(frame[1], 0x43);
    }

    #[test]
    fn test_persistence() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("test.db");

        // Write data
        {
            let mut region = VmRegion::new(&path, 10).unwrap();
            let frame = region.get_frame_mut(5);
            frame[0..4].copy_from_slice(&[1, 2, 3, 4]);
            region.flush().unwrap();
        }

        // Read back in new region
        {
            let region = VmRegion::new(&path, 10).unwrap();
            let frame = region.get_frame(5);
            assert_eq!(&frame[0..4], &[1, 2, 3, 4]);
        }
    }

    #[test]
    #[should_panic(expected = "Frame index out of bounds")]
    fn test_out_of_bounds() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("test.db");

        let region = VmRegion::new(&path, 10).unwrap();
        let _ = region.get_frame(10); // Should panic
    }
}
