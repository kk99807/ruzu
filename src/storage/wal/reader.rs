//! WAL reader for sequential log reading and replay.

use std::collections::HashSet;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use crate::error::{Result, RuzuError};
use crate::storage::wal::{WalHeader, WalRecord};

/// Reader for WAL files.
pub struct WalReader {
    /// Path to the WAL file.
    path: PathBuf,
    /// Buffered reader.
    reader: BufReader<File>,
    /// WAL header.
    header: WalHeader,
    /// Current position in the file.
    position: u64,
}

impl WalReader {
    /// Opens a WAL file for reading.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be opened or has an invalid header.
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path)
            .map_err(|e| RuzuError::StorageError(format!("Failed to open WAL file: {e}")))?;

        let mut reader = BufReader::new(file);

        // Read and validate header
        let header = Self::read_header(&mut reader)?;
        header.validate()?;

        let position = WalHeader::serialized_size() as u64;

        Ok(Self {
            path: path.to_path_buf(),
            reader,
            header,
            position,
        })
    }

    fn read_header(reader: &mut BufReader<File>) -> Result<WalHeader> {
        let mut magic = [0u8; 8];
        reader
            .read_exact(&mut magic)
            .map_err(|e| RuzuError::StorageError(format!("Failed to read WAL magic: {e}")))?;

        let mut version_bytes = [0u8; 4];
        reader
            .read_exact(&mut version_bytes)
            .map_err(|e| RuzuError::StorageError(format!("Failed to read WAL version: {e}")))?;
        let version = u32::from_le_bytes(version_bytes);

        let mut uuid_bytes = [0u8; 16];
        reader
            .read_exact(&mut uuid_bytes)
            .map_err(|e| RuzuError::StorageError(format!("Failed to read database ID: {e}")))?;
        let database_id = uuid::Uuid::from_bytes(uuid_bytes);

        let mut checksum_flag = [0u8; 1];
        reader
            .read_exact(&mut checksum_flag)
            .map_err(|e| RuzuError::StorageError(format!("Failed to read checksum flag: {e}")))?;
        let enable_checksums = checksum_flag[0] != 0;

        Ok(WalHeader {
            magic,
            version,
            database_id,
            enable_checksums,
        })
    }

    /// Returns the WAL header.
    #[must_use]
    pub fn header(&self) -> &WalHeader {
        &self.header
    }

    /// Returns the path to the WAL file.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Reads the next record from the WAL.
    ///
    /// Returns `None` if at end of file.
    ///
    /// # Errors
    ///
    /// Returns an error if the record cannot be read or is corrupted.
    pub fn read_record(&mut self) -> Result<Option<WalRecord>> {
        // Try to read record length
        let mut len_bytes = [0u8; 4];
        match self.reader.read_exact(&mut len_bytes) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                return Ok(None);
            }
            Err(e) => {
                return Err(RuzuError::StorageError(format!(
                    "Failed to read record length: {e}"
                )));
            }
        }

        let len = u32::from_le_bytes(len_bytes) as usize;
        self.position += 4;

        // Read record data
        let mut data = vec![0u8; len];
        self.reader
            .read_exact(&mut data)
            .map_err(|e| RuzuError::StorageError(format!("Failed to read record data: {e}")))?;
        self.position += len as u64;

        // Read and verify checksum if enabled
        if self.header.enable_checksums {
            let mut checksum_bytes = [0u8; 4];
            self.reader
                .read_exact(&mut checksum_bytes)
                .map_err(|e| RuzuError::StorageError(format!("Failed to read checksum: {e}")))?;
            self.position += 4;

            let expected_checksum = u32::from_le_bytes(checksum_bytes);
            let actual_checksum = crc32fast::hash(&data);

            if expected_checksum != actual_checksum {
                return Err(RuzuError::StorageError(format!(
                    "WAL record checksum mismatch at position {}",
                    self.position
                )));
            }
        }

        // Deserialize record
        let record = WalRecord::deserialize(&data)?;

        Ok(Some(record))
    }

    /// Reads all records from the WAL.
    ///
    /// # Errors
    ///
    /// Returns an error if any record cannot be read.
    pub fn read_all(&mut self) -> Result<Vec<WalRecord>> {
        let mut records = Vec::new();

        while let Some(record) = self.read_record()? {
            records.push(record);
        }

        Ok(records)
    }

    /// Resets the reader to the beginning of the records.
    ///
    /// # Errors
    ///
    /// Returns an error if seeking fails.
    pub fn reset(&mut self) -> Result<()> {
        let header_size = WalHeader::serialized_size() as u64;
        self.reader
            .seek(SeekFrom::Start(header_size))
            .map_err(|e| RuzuError::StorageError(format!("Failed to reset WAL reader: {e}")))?;
        self.position = header_size;
        Ok(())
    }
}

/// Result of WAL replay.
#[derive(Debug, Default)]
pub struct ReplayResult {
    /// Number of records replayed.
    pub records_replayed: usize,
    /// Number of transactions committed.
    pub transactions_committed: usize,
    /// Number of transactions rolled back.
    pub transactions_rolled_back: usize,
    /// IDs of committed transactions.
    pub committed_txs: HashSet<u64>,
}

/// Replays WAL records to recover database state.
pub struct WalReplayer {
    /// Set of active (uncommitted) transactions.
    active_txs: HashSet<u64>,
    /// Set of committed transactions.
    committed_txs: HashSet<u64>,
    /// Records from committed transactions (for applying).
    committed_records: Vec<WalRecord>,
}

impl WalReplayer {
    /// Creates a new WAL replayer.
    #[must_use]
    pub fn new() -> Self {
        Self {
            active_txs: HashSet::new(),
            committed_txs: HashSet::new(),
            committed_records: Vec::new(),
        }
    }

    /// Processes records from a WAL reader.
    ///
    /// This is the first pass that identifies committed transactions.
    ///
    /// # Errors
    ///
    /// Returns an error if reading fails.
    pub fn analyze(&mut self, reader: &mut WalReader) -> Result<()> {
        use crate::storage::wal::record::WalRecordType;

        while let Some(record) = reader.read_record()? {
            match record.record_type {
                WalRecordType::BeginTransaction => {
                    self.active_txs.insert(record.transaction_id);
                }
                WalRecordType::Commit => {
                    self.active_txs.remove(&record.transaction_id);
                    self.committed_txs.insert(record.transaction_id);
                }
                WalRecordType::Abort => {
                    self.active_txs.remove(&record.transaction_id);
                }
                _ => {
                    // Checkpoint or data modification record
                    // Data records kept for potential replay; checkpoints implicitly
                    // commit active transactions (simplified model)
                }
            }

            // Store all records for potential replay
            self.committed_records.push(record);
        }

        Ok(())
    }

    /// Returns the replay result.
    #[must_use]
    pub fn result(&self) -> ReplayResult {
        ReplayResult {
            records_replayed: self.committed_records.len(),
            transactions_committed: self.committed_txs.len(),
            transactions_rolled_back: self.active_txs.len(),
            committed_txs: self.committed_txs.clone(),
        }
    }

    /// Returns records that should be applied (from committed transactions only).
    pub fn records_to_apply(&self) -> impl Iterator<Item = &WalRecord> {
        self.committed_records
            .iter()
            .filter(|r| self.committed_txs.contains(&r.transaction_id))
    }

    /// Returns the set of committed transaction IDs.
    #[must_use]
    pub fn committed_transactions(&self) -> &HashSet<u64> {
        &self.committed_txs
    }

    /// Returns the set of rolled-back transaction IDs.
    #[must_use]
    pub fn rolled_back_transactions(&self) -> &HashSet<u64> {
        &self.active_txs
    }
}

impl Default for WalReplayer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::wal::writer::WalWriter;
    use tempfile::TempDir;

    fn create_wal_with_records(records: &[WalRecord]) -> (PathBuf, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("wal.log");
        let db_id = uuid::Uuid::new_v4();

        let mut writer = WalWriter::new(&wal_path, db_id, true).unwrap();

        for record in records {
            writer.append(record).unwrap();
        }
        writer.flush().unwrap();

        (wal_path, temp_dir)
    }

    #[test]
    fn test_read_empty_wal() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("wal.log");
        let db_id = uuid::Uuid::new_v4();

        let _writer = WalWriter::new(&wal_path, db_id, true).unwrap();

        let mut reader = WalReader::open(&wal_path).unwrap();
        let records = reader.read_all().unwrap();

        assert!(records.is_empty());
    }

    #[test]
    fn test_read_records() {
        let records = vec![WalRecord::begin_transaction(1, 1), WalRecord::commit(1, 2)];

        let (wal_path, _temp) = create_wal_with_records(&records);

        let mut reader = WalReader::open(&wal_path).unwrap();
        let read_records = reader.read_all().unwrap();

        assert_eq!(read_records.len(), 2);
        assert_eq!(read_records[0].lsn, 1);
        assert_eq!(read_records[1].lsn, 2);
    }

    #[test]
    fn test_replayer_committed_transaction() {
        let records = vec![WalRecord::begin_transaction(1, 1), WalRecord::commit(1, 2)];

        let (wal_path, _temp) = create_wal_with_records(&records);

        let mut reader = WalReader::open(&wal_path).unwrap();
        let mut replayer = WalReplayer::new();
        replayer.analyze(&mut reader).unwrap();

        let result = replayer.result();
        assert_eq!(result.transactions_committed, 1);
        assert_eq!(result.transactions_rolled_back, 0);
        assert!(result.committed_txs.contains(&1));
    }

    #[test]
    fn test_replayer_uncommitted_transaction() {
        let records = vec![
            WalRecord::begin_transaction(1, 1),
            // No commit - simulates crash
        ];

        let (wal_path, _temp) = create_wal_with_records(&records);

        let mut reader = WalReader::open(&wal_path).unwrap();
        let mut replayer = WalReplayer::new();
        replayer.analyze(&mut reader).unwrap();

        let result = replayer.result();
        assert_eq!(result.transactions_committed, 0);
        assert_eq!(result.transactions_rolled_back, 1);
    }
}
