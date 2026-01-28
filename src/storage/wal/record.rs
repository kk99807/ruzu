//! WAL record types and serialization.

use serde::{Deserialize, Serialize};

use crate::types::Value;

/// Type of WAL record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum WalRecordType {
    /// Begin a new transaction.
    BeginTransaction = 1,
    /// Commit a transaction.
    Commit = 2,
    /// Abort a transaction.
    Abort = 3,
    /// Insert rows into a table.
    TableInsertion = 30,
    /// Delete a node.
    NodeDeletion = 31,
    /// Update a node property.
    NodeUpdate = 32,
    /// Delete a relationship.
    RelDeletion = 33,
    /// Insert a relationship.
    RelInsertion = 36,
    /// Checkpoint marker.
    Checkpoint = 254,
}

impl TryFrom<u8> for WalRecordType {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(WalRecordType::BeginTransaction),
            2 => Ok(WalRecordType::Commit),
            3 => Ok(WalRecordType::Abort),
            30 => Ok(WalRecordType::TableInsertion),
            31 => Ok(WalRecordType::NodeDeletion),
            32 => Ok(WalRecordType::NodeUpdate),
            33 => Ok(WalRecordType::RelDeletion),
            36 => Ok(WalRecordType::RelInsertion),
            254 => Ok(WalRecordType::Checkpoint),
            _ => Err(()),
        }
    }
}

/// Payload for a WAL record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WalPayload {
    /// Begin transaction payload.
    BeginTransaction {
        /// Transaction ID.
        tx_id: u64,
    },
    /// Commit transaction payload.
    Commit {
        /// Transaction ID.
        tx_id: u64,
    },
    /// Abort transaction payload.
    Abort {
        /// Transaction ID.
        tx_id: u64,
    },
    /// Table insertion payload.
    TableInsertion {
        /// Table ID.
        table_id: u32,
        /// Rows to insert (each row is a vector of values).
        rows: Vec<Vec<Value>>,
    },
    /// Node deletion payload.
    NodeDeletion {
        /// Table ID.
        table_id: u32,
        /// Node offset in the table.
        node_offset: u64,
        /// Primary key value of the deleted node.
        pk: Value,
    },
    /// Node update payload.
    NodeUpdate {
        /// Table ID.
        table_id: u32,
        /// Column ID being updated.
        col_id: u32,
        /// Node offset in the table.
        node_offset: u64,
        /// New value.
        value: Value,
    },
    /// Relationship insertion payload.
    RelInsertion {
        /// Relationship table ID.
        table_id: u32,
        /// Source node offset.
        src: u64,
        /// Destination node offset.
        dst: u64,
        /// Relationship properties.
        props: Vec<Value>,
    },
    /// Relationship deletion payload.
    RelDeletion {
        /// Relationship table ID.
        table_id: u32,
        /// Source node offset.
        src: u64,
        /// Destination node offset.
        dst: u64,
        /// Relationship ID.
        rel_id: u64,
    },
    /// Checkpoint payload.
    Checkpoint {
        /// Checkpoint ID.
        checkpoint_id: u64,
    },
}

/// A single WAL record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalRecord {
    /// Type of this record.
    pub record_type: WalRecordType,
    /// Transaction ID that owns this record.
    pub transaction_id: u64,
    /// Log Sequence Number (monotonically increasing).
    pub lsn: u64,
    /// Record payload.
    pub payload: WalPayload,
}

impl WalRecord {
    /// Creates a new WAL record.
    #[must_use]
    pub fn new(
        record_type: WalRecordType,
        transaction_id: u64,
        lsn: u64,
        payload: WalPayload,
    ) -> Self {
        Self {
            record_type,
            transaction_id,
            lsn,
            payload,
        }
    }

    /// Creates a begin transaction record.
    #[must_use]
    pub fn begin_transaction(tx_id: u64, lsn: u64) -> Self {
        Self::new(
            WalRecordType::BeginTransaction,
            tx_id,
            lsn,
            WalPayload::BeginTransaction { tx_id },
        )
    }

    /// Creates a commit record.
    #[must_use]
    pub fn commit(tx_id: u64, lsn: u64) -> Self {
        Self::new(
            WalRecordType::Commit,
            tx_id,
            lsn,
            WalPayload::Commit { tx_id },
        )
    }

    /// Creates an abort record.
    #[must_use]
    pub fn abort(tx_id: u64, lsn: u64) -> Self {
        Self::new(
            WalRecordType::Abort,
            tx_id,
            lsn,
            WalPayload::Abort { tx_id },
        )
    }

    /// Creates a checkpoint record.
    #[must_use]
    pub fn checkpoint(tx_id: u64, lsn: u64, checkpoint_id: u64) -> Self {
        Self::new(
            WalRecordType::Checkpoint,
            tx_id,
            lsn,
            WalPayload::Checkpoint { checkpoint_id },
        )
    }

    /// Serializes the record to bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails.
    pub fn serialize(&self) -> crate::error::Result<Vec<u8>> {
        bincode::serialize(self).map_err(|e| {
            crate::error::RuzuError::StorageError(format!("Failed to serialize WAL record: {e}"))
        })
    }

    /// Deserializes a record from bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if deserialization fails.
    pub fn deserialize(data: &[u8]) -> crate::error::Result<Self> {
        bincode::deserialize(data).map_err(|e| {
            crate::error::RuzuError::StorageError(format!("Failed to deserialize WAL record: {e}"))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_type_conversion() {
        assert_eq!(
            WalRecordType::try_from(1),
            Ok(WalRecordType::BeginTransaction)
        );
        assert_eq!(WalRecordType::try_from(2), Ok(WalRecordType::Commit));
        assert_eq!(WalRecordType::try_from(254), Ok(WalRecordType::Checkpoint));
        assert!(WalRecordType::try_from(255).is_err());
    }

    #[test]
    fn test_begin_transaction_record() {
        let record = WalRecord::begin_transaction(42, 1);
        assert_eq!(record.record_type, WalRecordType::BeginTransaction);
        assert_eq!(record.transaction_id, 42);
        assert_eq!(record.lsn, 1);
    }

    #[test]
    fn test_serialization_roundtrip() {
        let record = WalRecord::commit(100, 50);
        let bytes = record.serialize().unwrap();
        let deserialized = WalRecord::deserialize(&bytes).unwrap();

        assert_eq!(deserialized.record_type, record.record_type);
        assert_eq!(deserialized.transaction_id, record.transaction_id);
        assert_eq!(deserialized.lsn, record.lsn);
    }

    #[test]
    fn test_table_insertion_record() {
        let rows = vec![
            vec![Value::Int64(1), Value::String("Alice".into())],
            vec![Value::Int64(2), Value::String("Bob".into())],
        ];

        let record = WalRecord::new(
            WalRecordType::TableInsertion,
            1,
            10,
            WalPayload::TableInsertion { table_id: 0, rows },
        );

        let bytes = record.serialize().unwrap();
        let deserialized = WalRecord::deserialize(&bytes).unwrap();

        match deserialized.payload {
            WalPayload::TableInsertion { table_id, rows } => {
                assert_eq!(table_id, 0);
                assert_eq!(rows.len(), 2);
            }
            _ => panic!("Wrong payload type"),
        }
    }
}
