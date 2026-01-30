//! Memory-mapped file reader for CSV import.
//!
//! This module provides a wrapper around memory-mapped I/O that falls back
//! to buffered reading when mmap is not available or for small files.
//!
//! # When to Use Memory Mapping
//!
//! Memory mapping is beneficial for:
//! - Large files (>100MB by default)
//! - Sequential reads with OS page cache utilization
//! - Parallel access from multiple threads
//!
//! Memory mapping may fail on:
//! - Network drives
//! - 32-bit systems with very large files
//! - Certain filesystems (NFS edge cases)

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use memmap2::Mmap;

use crate::error::RuzuError;
use crate::storage::csv::CsvImportConfig;

/// File reader that uses mmap for large files with fallback to buffered I/O.
pub enum MmapReader {
    /// Memory-mapped file for large files.
    Mmap {
        /// The memory map.
        mmap: Mmap,
        /// File size.
        size: u64,
    },
    /// Buffered file reader for small files or when mmap fails.
    Buffered {
        /// The buffered reader.
        reader: BufReader<File>,
        /// File size.
        size: u64,
        /// Cached content (loaded on first access for slice operations).
        content: Option<Vec<u8>>,
    },
}

impl MmapReader {
    /// Opens a file, using mmap if file size exceeds threshold.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be opened.
    pub fn open(path: &Path, config: &CsvImportConfig) -> Result<Self, RuzuError> {
        let file = File::open(path).map_err(|e| {
            RuzuError::StorageError(format!("Failed to open file '{}': {}", path.display(), e))
        })?;

        let metadata = file
            .metadata()
            .map_err(|e| RuzuError::StorageError(format!("Failed to get file metadata: {}", e)))?;
        let file_size = metadata.len();

        // Try mmap for large files
        if config.use_mmap && file_size >= config.mmap_threshold {
            match Self::try_mmap(&file, file_size) {
                Ok(reader) => return Ok(reader),
                Err(e) => {
                    // Log warning and fall back to buffered I/O
                    eprintln!("Warning: mmap failed, falling back to buffered I/O: {}", e);
                }
            }
        }

        // Use buffered reader
        Ok(MmapReader::Buffered {
            reader: BufReader::new(file),
            size: file_size,
            content: None,
        })
    }

    /// Opens a file and always uses mmap (for testing or when mmap is required).
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be opened or mmap fails.
    pub fn open_mmap(path: &Path) -> Result<Self, RuzuError> {
        let file = File::open(path).map_err(|e| {
            RuzuError::StorageError(format!("Failed to open file '{}': {}", path.display(), e))
        })?;

        let metadata = file
            .metadata()
            .map_err(|e| RuzuError::StorageError(format!("Failed to get file metadata: {}", e)))?;
        let file_size = metadata.len();

        Self::try_mmap(&file, file_size)
    }

    /// Attempts to memory-map the file.
    #[allow(unsafe_code)]
    fn try_mmap(file: &File, size: u64) -> Result<Self, RuzuError> {
        // SAFETY: File is opened read-only. We assume no concurrent writes
        // to the file during the import operation. If the file is modified
        // externally, behavior is undefined but will not cause memory unsafety.
        let mmap = unsafe { Mmap::map(file) }
            .map_err(|e| RuzuError::StorageError(format!("mmap failed: {}", e)))?;

        Ok(MmapReader::Mmap { mmap, size })
    }

    /// Returns the file as a byte slice (for mmap) or loads content (for buffered).
    ///
    /// For buffered readers, this loads the entire file into memory on first call.
    /// Subsequent calls return the cached content.
    ///
    /// # Errors
    ///
    /// Returns an error if reading the buffered content fails.
    pub fn as_slice(&mut self) -> Result<&[u8], RuzuError> {
        match self {
            MmapReader::Mmap { mmap, .. } => Ok(&mmap[..]),
            MmapReader::Buffered {
                reader,
                size,
                content,
            } => {
                if content.is_none() {
                    let mut buf = Vec::with_capacity(*size as usize);
                    reader.read_to_end(&mut buf).map_err(|e| {
                        RuzuError::StorageError(format!("Failed to read file: {}", e))
                    })?;
                    *content = Some(buf);
                }
                Ok(content.as_ref().unwrap())
            }
        }
    }

    /// Returns the file size in bytes.
    #[must_use]
    pub fn len(&self) -> u64 {
        match self {
            MmapReader::Mmap { size, .. } => *size,
            MmapReader::Buffered { size, .. } => *size,
        }
    }

    /// Returns whether the file is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns whether this reader is using memory mapping.
    #[must_use]
    pub fn is_mmap(&self) -> bool {
        matches!(self, MmapReader::Mmap { .. })
    }
}

impl std::fmt::Debug for MmapReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MmapReader::Mmap { size, .. } => f
                .debug_struct("MmapReader::Mmap")
                .field("size", size)
                .finish(),
            MmapReader::Buffered { size, content, .. } => f
                .debug_struct("MmapReader::Buffered")
                .field("size", size)
                .field("content_loaded", &content.is_some())
                .finish(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_test_file(content: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();
        file.flush().unwrap();
        file
    }

    #[test]
    fn test_open_small_file_buffered() {
        let content = "id,name\n1,Alice\n2,Bob\n";
        let file = create_test_file(content);

        let config = CsvImportConfig::default();
        let mut reader = MmapReader::open(file.path(), &config).unwrap();

        // Small file should use buffered reader
        assert!(!reader.is_mmap());
        assert_eq!(reader.len(), content.len() as u64);
        assert_eq!(reader.as_slice().unwrap(), content.as_bytes());
    }

    #[test]
    fn test_open_with_mmap_disabled() {
        let content = "id,name\n1,Alice\n";
        let file = create_test_file(content);

        let mut config = CsvImportConfig::default();
        config.use_mmap = false;

        let reader = MmapReader::open(file.path(), &config).unwrap();
        assert!(!reader.is_mmap());
    }

    #[test]
    fn test_open_with_low_threshold() {
        let content = "id,name\n1,Alice\n2,Bob\n3,Charlie\n";
        let file = create_test_file(content);

        let mut config = CsvImportConfig::default();
        config.use_mmap = true;
        config.mmap_threshold = 10; // Very low threshold

        let reader = MmapReader::open(file.path(), &config).unwrap();
        // With a low threshold, it should try mmap
        // (May or may not succeed depending on platform)
        assert_eq!(reader.len(), content.len() as u64);
    }

    #[test]
    fn test_force_mmap() {
        let content = "id,name\n1,Alice\n";
        let file = create_test_file(content);

        let mut reader = MmapReader::open_mmap(file.path()).unwrap();
        assert!(reader.is_mmap());
        assert_eq!(reader.as_slice().unwrap(), content.as_bytes());
    }

    #[test]
    fn test_empty_file() {
        let file = create_test_file("");

        let config = CsvImportConfig::default();
        let reader = MmapReader::open(file.path(), &config).unwrap();

        assert!(reader.is_empty());
        assert_eq!(reader.len(), 0);
    }

    #[test]
    fn test_debug_format() {
        let content = "test";
        let file = create_test_file(content);

        let config = CsvImportConfig::default();
        let reader = MmapReader::open(file.path(), &config).unwrap();

        let debug_str = format!("{:?}", reader);
        assert!(debug_str.contains("size"));
    }
}
