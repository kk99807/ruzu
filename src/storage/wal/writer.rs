//! WAL writer for append-only log writing.

use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::error::{Result, RuzuError};
use crate::storage::wal::{WalHeader, WalRecord};

/// Writer for appending records to the WAL.
pub struct WalWriter {
    /// Path to the WAL file.
    path: PathBuf,
    /// Buffered writer for efficient I/O.
    writer: BufWriter<File>,
    /// Whether checksums are enabled.
    enable_checksums: bool,
    /// Next LSN to assign.
    next_lsn: AtomicU64,
    /// Database ID for validation.
    database_id: uuid::Uuid,
}

impl WalWriter {
    /// Creates a new WAL file or opens an existing one.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be created or opened.
    pub fn new(path: &Path, database_id: uuid::Uuid, enable_checksums: bool) -> Result<Self> {
        let exists = path.exists();

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(|e| RuzuError::StorageError(format!("Failed to open WAL file: {e}")))?;

        let mut writer = BufWriter::new(file);

        if !exists {
            // Write header for new file
            let header = WalHeader::new(database_id, enable_checksums);
            Self::write_header_internal(&mut writer, &header)?;
        }

        // Get current file position for next LSN
        let file_len = writer
            .get_ref()
            .metadata()
            .map_err(|e| RuzuError::StorageError(format!("Failed to get WAL metadata: {e}")))?
            .len();

        // Seek to end for appending
        writer
            .seek(SeekFrom::End(0))
            .map_err(|e| RuzuError::StorageError(format!("Failed to seek WAL: {e}")))?;

        // Estimate next LSN from file position (simplified)
        let next_lsn = if file_len <= WalHeader::serialized_size() as u64 {
            1
        } else {
            // This is a simplification - in production, we'd read the last LSN
            1
        };

        Ok(Self {
            path: path.to_path_buf(),
            writer,
            enable_checksums,
            next_lsn: AtomicU64::new(next_lsn),
            database_id,
        })
    }

    fn write_header_internal(writer: &mut BufWriter<File>, header: &WalHeader) -> Result<()> {
        writer
            .write_all(&header.magic)
            .map_err(|e| RuzuError::StorageError(format!("Failed to write WAL magic: {e}")))?;

        writer
            .write_all(&header.version.to_le_bytes())
            .map_err(|e| RuzuError::StorageError(format!("Failed to write WAL version: {e}")))?;

        writer
            .write_all(header.database_id.as_bytes())
            .map_err(|e| RuzuError::StorageError(format!("Failed to write database ID: {e}")))?;

        writer
            .write_all(&[u8::from(header.enable_checksums)])
            .map_err(|e| RuzuError::StorageError(format!("Failed to write checksum flag: {e}")))?;

        writer
            .flush()
            .map_err(|e| RuzuError::StorageError(format!("Failed to flush WAL header: {e}")))?;

        Ok(())
    }

    /// Returns the path to the WAL file.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the database ID.
    #[must_use]
    pub fn database_id(&self) -> uuid::Uuid {
        self.database_id
    }

    /// Returns and increments the next LSN.
    pub fn next_lsn(&self) -> u64 {
        self.next_lsn.fetch_add(1, Ordering::Relaxed)
    }

    /// Appends a record to the WAL.
    ///
    /// Returns the LSN assigned to the record.
    ///
    /// # Errors
    ///
    /// Returns an error if the write fails.
    pub fn append(&mut self, record: &WalRecord) -> Result<u64> {
        let serialized = record.serialize()?;

        // Write record length
        let len = serialized.len() as u32;
        self.writer
            .write_all(&len.to_le_bytes())
            .map_err(|e| RuzuError::StorageError(format!("Failed to write record length: {e}")))?;

        // Write record data
        self.writer
            .write_all(&serialized)
            .map_err(|e| RuzuError::StorageError(format!("Failed to write record data: {e}")))?;

        // Write checksum if enabled
        if self.enable_checksums {
            let checksum = crc32fast::hash(&serialized);
            self.writer
                .write_all(&checksum.to_le_bytes())
                .map_err(|e| RuzuError::StorageError(format!("Failed to write checksum: {e}")))?;
        }

        Ok(record.lsn)
    }

    /// Flushes buffered writes to disk.
    ///
    /// # Errors
    ///
    /// Returns an error if the flush fails.
    pub fn flush(&mut self) -> Result<()> {
        self.writer
            .flush()
            .map_err(|e| RuzuError::StorageError(format!("Failed to flush WAL: {e}")))
    }

    /// Syncs the WAL file to disk (fsync).
    ///
    /// # Errors
    ///
    /// Returns an error if the sync fails.
    pub fn sync(&mut self) -> Result<()> {
        self.flush()?;
        self.writer
            .get_ref()
            .sync_all()
            .map_err(|e| RuzuError::StorageError(format!("Failed to sync WAL: {e}")))
    }

    /// Truncates the WAL file, keeping only the header.
    ///
    /// Used after a successful checkpoint.
    ///
    /// # Errors
    ///
    /// Returns an error if truncation fails.
    pub fn truncate(&mut self) -> Result<()> {
        // Flush any buffered writes before truncating
        self.flush()?;

        let header_size = WalHeader::serialized_size() as u64;

        self.writer
            .get_mut()
            .set_len(header_size)
            .map_err(|e| RuzuError::StorageError(format!("Failed to truncate WAL: {e}")))?;

        self.writer
            .seek(SeekFrom::End(0))
            .map_err(|e| RuzuError::StorageError(format!("Failed to seek after truncate: {e}")))?;

        self.next_lsn.store(1, Ordering::Relaxed);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::wal::record::{WalPayload, WalRecordType};
    use tempfile::TempDir;

    fn create_test_writer() -> (WalWriter, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("wal.log");
        let db_id = uuid::Uuid::new_v4();
        let writer = WalWriter::new(&wal_path, db_id, true).unwrap();
        (writer, temp_dir)
    }

    #[test]
    fn test_create_wal() {
        let (writer, _temp) = create_test_writer();
        assert!(writer.path().exists());
    }

    #[test]
    fn test_append_record() {
        let (mut writer, _temp) = create_test_writer();

        let lsn = writer.next_lsn();
        let record = WalRecord::begin_transaction(1, lsn);

        let returned_lsn = writer.append(&record).unwrap();
        assert_eq!(returned_lsn, lsn);

        writer.flush().unwrap();
    }

    #[test]
    fn test_multiple_records() {
        let (mut writer, _temp) = create_test_writer();

        for i in 1..=10 {
            let lsn = writer.next_lsn();
            let record = WalRecord::new(
                WalRecordType::TableInsertion,
                i,
                lsn,
                WalPayload::TableInsertion {
                    table_id: 0,
                    rows: vec![],
                },
            );
            writer.append(&record).unwrap();
        }

        writer.flush().unwrap();
    }

    #[test]
    fn test_truncate() {
        let (mut writer, _temp) = create_test_writer();

        // Write some records
        for i in 1..=5 {
            let lsn = writer.next_lsn();
            let record = WalRecord::begin_transaction(i, lsn);
            writer.append(&record).unwrap();
        }
        writer.flush().unwrap();

        let size_before = writer.path().metadata().unwrap().len();

        // Truncate
        writer.truncate().unwrap();

        let size_after = writer.path().metadata().unwrap().len();

        // File should be smaller after truncation
        assert!(size_after < size_before);
        assert_eq!(size_after, WalHeader::serialized_size() as u64);
    }
}
