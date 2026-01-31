//! ruzu - Rust Graph Database
//!
//! Phase 2: Query engine with `DataFusion` integration.

pub mod binder;
pub mod catalog;
pub mod datafusion;
pub mod error;
pub mod executor;
pub mod parser;
pub mod planner;
pub mod storage;
pub mod types;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub use error::{Result, RuzuError};
pub use types::{QueryResult, Row, Value};

use catalog::{Catalog, ColumnDef, Direction, NodeTableSchema, RelTableSchema};

/// Shared query clause parameters for MATCH execution.
struct QueryModifiers<'a> {
    projections: &'a [ReturnItem],
    order_by: Option<&'a Vec<parser::ast::OrderByItem>>,
    skip: Option<i64>,
    limit: Option<i64>,
}

/// Relationship pattern parameters for MATCH-REL execution.
struct RelPattern<'a> {
    src_node: &'a NodeFilter,
    rel_var: Option<&'a String>,
    rel_type: &'a str,
    dst_node: &'a NodeFilter,
    filter: Option<&'a parser::ast::Expression>,
    path_bounds: Option<(u32, u32)>,
}
use executor::{FilterOperator, PhysicalOperator, ProjectOperator, ScanOperator};
pub use executor::{ExecutorConfig, QueryExecutor};
use parser::ast::{CopyOptions, Literal, NodeFilter, ReturnItem, Statement};
use std::sync::atomic::{AtomicU64, Ordering};
use storage::{
    BufferPool, Checkpointer, DatabaseHeader, DiskManager, NodeTable, PageRange, RelTable,
    WalPayload, WalReader, WalRecord, WalRecordType, WalReplayer, WalWriter, PAGE_SIZE,
};
use types::DataType;
use uuid::Uuid;

/// Calculates the number of pages needed to store data with a 4-byte length prefix.
///
/// Formula: `ceil((data_len + 4) / PAGE_SIZE)`
/// Minimum: 1 page (even for empty data, to store the length prefix).
#[must_use]
pub fn calculate_pages_needed(data_len: usize) -> u32 {
    let total = data_len + 4; // 4 bytes for length prefix
    total.div_ceil(PAGE_SIZE) as u32
}

/// Writes serialized data across a contiguous page range using the buffer pool.
///
/// Format: [4-byte length prefix (u32 LE)] [data bytes spanning pages]
fn write_multi_page(
    buffer_pool: &BufferPool,
    range: PageRange,
    data: &[u8],
) -> Result<()> {
    use storage::PageId;

    let total_len = data.len() + 4;
    if total_len > range.byte_capacity() {
        return Err(RuzuError::StorageError(format!(
            "Data ({} bytes + 4 prefix) exceeds page range capacity ({} bytes)",
            data.len(),
            range.byte_capacity()
        )));
    }

    // Build the full payload: [4-byte length][data]
    let mut payload = Vec::with_capacity(total_len);
    payload.extend_from_slice(&(data.len() as u32).to_le_bytes());
    payload.extend_from_slice(data);

    // Write page by page
    let mut offset = 0usize;
    for i in 0..range.num_pages {
        let page_id = PageId::new(0, range.start_page + i);
        let mut handle = buffer_pool.pin(page_id)?;
        let page_data = handle.data_mut();

        let chunk_start = offset;
        let chunk_end = (offset + PAGE_SIZE).min(payload.len());

        if chunk_start < payload.len() {
            let chunk = &payload[chunk_start..chunk_end];
            page_data[..chunk.len()].copy_from_slice(chunk);
            // Zero remaining bytes in this page
            if chunk.len() < PAGE_SIZE {
                page_data[chunk.len()..].fill(0);
            }
        } else {
            // Entire page is padding
            page_data.fill(0);
        }

        offset += PAGE_SIZE;
    }

    Ok(())
}

/// Reads serialized data from a contiguous page range using the buffer pool.
///
/// Reads the 4-byte length prefix from the first page, then assembles
/// the data bytes from all pages in the range.
fn read_multi_page(
    buffer_pool: &BufferPool,
    range: PageRange,
) -> Result<Vec<u8>> {
    use storage::PageId;

    if range.is_empty() {
        return Err(RuzuError::StorageError(
            "Cannot read from empty page range".into(),
        ));
    }

    // Read all pages into a contiguous buffer
    let mut raw = Vec::with_capacity(range.byte_capacity());
    for i in 0..range.num_pages {
        let page_id = PageId::new(0, range.start_page + i);
        let handle = buffer_pool.pin(page_id)?;
        raw.extend_from_slice(handle.data());
    }

    // Parse length prefix
    if raw.len() < 4 {
        return Err(RuzuError::MultiPageDataCorrupted(
            "Page data too short for length prefix".into(),
        ));
    }
    let data_len = u32::from_le_bytes(raw[0..4].try_into().unwrap()) as usize;

    if data_len + 4 > raw.len() {
        return Err(RuzuError::MultiPageDataCorrupted(format!(
            "Length prefix {} exceeds range capacity {}",
            data_len,
            raw.len() - 4
        )));
    }

    Ok(raw[4..4 + data_len].to_vec())
}

/// Configuration for opening or creating a database.
#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    /// Size of the buffer pool in bytes (default: 256MB).
    pub buffer_pool_size: usize,
    /// Enable WAL checksums (default: true).
    pub wal_checksums: bool,
    /// Force WAL sync after each write (default: true).
    pub wal_sync: bool,
    /// Open in read-only mode (default: false).
    pub read_only: bool,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            buffer_pool_size: 256 * 1024 * 1024, // 256 MB
            wal_checksums: true,
            wal_sync: true,
            read_only: false,
        }
    }
}

/// The main database struct that provides query execution.
///
/// Can be used in two modes:
/// - In-memory: Use `Database::new()` for transient data
/// - Persistent: Use `Database::open()` for disk-backed storage
pub struct Database {
    /// Schema catalog.
    catalog: Catalog,
    /// In-memory node tables (for in-memory mode or as cache).
    tables: HashMap<String, Arc<NodeTable>>,
    /// In-memory relationship tables.
    rel_tables: HashMap<String, RelTable>,
    /// Database directory path (None for in-memory mode).
    db_path: Option<PathBuf>,
    /// Buffer pool for page management (None for in-memory mode).
    buffer_pool: Option<BufferPool>,
    /// Database configuration.
    config: DatabaseConfig,
    /// Database header (None for in-memory mode).
    header: Option<DatabaseHeader>,
    /// Whether the database needs to be saved on close.
    dirty: bool,
    /// WAL writer for crash recovery (None for in-memory mode).
    wal_writer: Option<WalWriter>,
    /// Checkpointer for WAL management.
    checkpointer: Checkpointer,
    /// Next transaction ID.
    next_tx_id: AtomicU64,
}

impl Default for Database {
    fn default() -> Self {
        Self::new()
    }
}

impl Database {
    /// Creates a new in-memory database.
    #[must_use]
    pub fn new() -> Self {
        Database {
            catalog: Catalog::new(),
            tables: HashMap::new(),
            rel_tables: HashMap::new(),
            db_path: None,
            buffer_pool: None,
            config: DatabaseConfig::default(),
            header: None,
            dirty: false,
            wal_writer: None,
            checkpointer: Checkpointer::new(),
            next_tx_id: AtomicU64::new(1),
        }
    }

    /// Opens or creates a persistent database at the given path.
    ///
    /// # Arguments
    ///
    /// * `path` - Directory where the database files will be stored
    /// * `config` - Database configuration options
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The directory cannot be created
    /// - The database file is corrupted
    /// - The database version is unsupported
    pub fn open(path: &Path, config: DatabaseConfig) -> Result<Self> {
        // Create database directory if it doesn't exist
        if !path.exists() {
            std::fs::create_dir_all(path).map_err(|e| {
                RuzuError::StorageError(format!("Failed to create database directory: {e}"))
            })?;
        }

        let db_file_path = path.join("data.ruzu");
        let wal_file_path = path.join("wal.log");
        let is_new = !db_file_path.exists();

        // Create disk manager
        let disk_manager = DiskManager::new(&db_file_path)?;

        // Calculate number of buffer frames from config
        let num_frames = config.buffer_pool_size / PAGE_SIZE;
        let buffer_pool = BufferPool::new(num_frames, disk_manager)?;

        let (mut catalog, header, was_migrated) = if is_new {
            // Initialize new database
            let db_id = Uuid::new_v4();
            let mut header = DatabaseHeader::new(db_id);
            header.catalog_range = PageRange::new(1, 1); // Reserve page 1 for catalog
            header.metadata_range = PageRange::new(2, 1); // Reserve page 2 for table data
            header.rel_metadata_range = PageRange::new(3, 1); // Reserve page 3 for rel table data
            header.update_checksum();

            // Pre-allocate reserved pages (0-3) so the allocator doesn't reuse them
            buffer_pool.allocate_page_range(4)?;

            (Catalog::new(), header, false)
        } else {
            // Load existing database
            Self::load_database(&buffer_pool)?
        };

        // Load table data from disk if available
        let mut tables = if is_new {
            // Create empty tables for new database
            let mut tables = HashMap::new();
            for table_name in catalog.table_names() {
                if let Some(schema) = catalog.get_table(table_name) {
                    let table = NodeTable::new(schema);
                    tables.insert(table_name.to_string(), Arc::new(table));
                }
            }
            tables
        } else {
            Self::load_table_data(&buffer_pool, &catalog, &header)?
        };

        // Initialize WAL writer
        let wal_writer = WalWriter::new(&wal_file_path, header.database_id, config.wal_checksums)?;

        // T023: Load relationship table data from disk
        let mut rel_tables = if is_new {
            // Create empty relationship tables for new database
            let mut rel_tables = HashMap::new();
            for rel_name in catalog.rel_table_names() {
                if let Some(schema) = catalog.get_rel_table(rel_name) {
                    let rel_table = RelTable::new(schema);
                    rel_tables.insert(rel_name.to_string(), rel_table);
                }
            }
            rel_tables
        } else {
            Self::load_rel_table_data(&buffer_pool, &catalog, &header)?
        };

        // Perform WAL recovery if WAL file exists and has records
        if wal_file_path.exists() && !is_new {
            Self::replay_wal(&wal_file_path, &mut catalog, &mut tables, &mut rel_tables)?;
        }

        Ok(Database {
            catalog,
            tables,
            rel_tables,
            db_path: Some(path.to_path_buf()),
            buffer_pool: Some(buffer_pool),
            config,
            header: Some(header),
            dirty: is_new || was_migrated, // New databases or migrated databases need to be saved
            wal_writer: Some(wal_writer),
            checkpointer: Checkpointer::new(),
            next_tx_id: AtomicU64::new(1),
        })
    }

    /// Replays WAL records to recover database state after a crash.
    fn replay_wal(
        wal_path: &Path,
        catalog: &mut Catalog,
        tables: &mut HashMap<String, Arc<NodeTable>>,
        rel_tables: &mut HashMap<String, RelTable>,
    ) -> Result<()> {
        // Open WAL reader
        let Ok(mut reader) = WalReader::open(wal_path) else {
            return Ok(()); // WAL doesn't exist or is empty, nothing to replay
        };

        // Analyze WAL to find committed transactions
        let mut replayer = WalReplayer::new();
        replayer.analyze(&mut reader)?;

        let result = replayer.result();

        // Log recovery info
        if result.records_replayed > 0 {
            eprintln!(
                "WAL recovery: {} records, {} committed, {} rolled back",
                result.records_replayed,
                result.transactions_committed,
                result.transactions_rolled_back
            );
        }

        // Apply only records from committed transactions
        for record in replayer.records_to_apply() {
            Self::apply_wal_record(record, catalog, tables, rel_tables)?;
        }

        Ok(())
    }

    /// Applies a single WAL record to the database state.
    fn apply_wal_record(
        record: &WalRecord,
        catalog: &Catalog,
        tables: &mut HashMap<String, Arc<NodeTable>>,
        rel_tables: &mut HashMap<String, RelTable>,
    ) -> Result<()> {
        match &record.payload {
            WalPayload::TableInsertion { table_id, rows } => {
                // Find table by ID
                if let Some(table_name) = catalog.table_name_by_id(*table_id) {
                    if let Some(table) = tables.get_mut(&table_name) {
                        let table = Arc::get_mut(table).ok_or_else(|| {
                            RuzuError::ExecutionError("Cannot modify table during recovery".into())
                        })?;

                        // Get schema to map values to columns
                        if let Some(schema) = catalog.get_table(&table_name) {
                            for row_values in rows {
                                let mut row: HashMap<String, Value> = HashMap::new();
                                for (i, col) in schema.columns.iter().enumerate() {
                                    if i < row_values.len() {
                                        row.insert(col.name.clone(), row_values[i].clone());
                                    }
                                }
                                // Insert without checking duplicates during recovery
                                // (WAL already validated this at write time)
                                let _ = table.insert(&row);
                            }
                        }
                    }
                }
            }
            WalPayload::RelInsertion {
                table_id,
                src,
                dst,
                props,
            } => {
                // Find relationship table by ID (table_name_by_id checks both node and rel tables)
                if let Some(rel_table_name) = catalog.table_name_by_id(*table_id) {
                    // Ensure RelTable exists in memory (create if not present)
                    // This handles the case where CREATE REL TABLE was checkpointed (in catalog)
                    // but relationship data was not yet saved (only in WAL)
                    if !rel_tables.contains_key(&rel_table_name) {
                        // Get schema from catalog to create empty RelTable
                        if let Some(rel_schema) = catalog.get_rel_table(&rel_table_name) {
                            let rel_table = RelTable::new(rel_schema);
                            rel_tables.insert(rel_table_name.clone(), rel_table);
                        }
                    }

                    if let Some(rel_table) = rel_tables.get_mut(&rel_table_name) {
                        // Insert relationship
                        // During WAL replay, we don't need to validate (already validated at write time)
                        let _ = rel_table.insert(*src, *dst, props.clone());
                    }
                }
            }
            // Other payload types are not applied during recovery (schema changes, etc.)
            // They would be persisted via catalog serialization
            _ => {}
        }

        Ok(())
    }

    /// Loads the database header and catalog from disk.
    ///
    /// Returns (catalog, header, `was_migrated`) where `was_migrated` is true if the database
    /// was upgraded from version 1 to version 2.
    fn load_database(buffer_pool: &BufferPool) -> Result<(Catalog, DatabaseHeader, bool)> {
        use storage::PageId;

        // Read header from page 0
        let header_handle = buffer_pool.pin(PageId::new(0, 0))?;
        let header_data = header_handle.data();
        let (header, was_migrated) = DatabaseHeader::deserialize_with_migration_flag(header_data)?;
        header.validate()?;

        // Skip checksum verification for migrated headers since the checksum was computed
        // for the v1 structure and won't match the v2 structure
        if !was_migrated && !header.verify_checksum() {
            return Err(RuzuError::ChecksumError(
                "Database header checksum mismatch".into(),
            ));
        }

        drop(header_handle);

        // T053: Validate page ranges are within file bounds
        // Skip for migrated databases — v1 databases have rel_metadata_range pointing
        // to page 3 which may not exist yet (will be created on first save).
        if !was_migrated {
            let file_page_count = buffer_pool.file_page_count();
            for (name, range) in [
                ("catalog", &header.catalog_range),
                ("metadata", &header.metadata_range),
                ("rel_metadata", &header.rel_metadata_range),
            ] {
                if range.num_pages > 0 {
                    let end_page = range.start_page + range.num_pages;
                    if end_page > file_page_count {
                        return Err(RuzuError::StorageError(format!(
                            "{} page range [{}, {}) exceeds file size ({} pages)",
                            name, range.start_page, end_page, file_page_count
                        )));
                    }
                }
            }
        }

        // T037: Read catalog from catalog pages using multi-page support
        let catalog = if header.catalog_range.num_pages > 0 {
            let catalog_bytes = read_multi_page(buffer_pool, header.catalog_range)?;
            if catalog_bytes.is_empty() {
                Catalog::new()
            } else {
                Catalog::deserialize(&catalog_bytes)?
            }
        } else {
            Catalog::new()
        };

        Ok((catalog, header, was_migrated))
    }

    /// Loads table data from disk.
    fn load_table_data(
        buffer_pool: &BufferPool,
        catalog: &Catalog,
        header: &DatabaseHeader,
    ) -> Result<HashMap<String, Arc<NodeTable>>> {
        use storage::TableData;

        let mut tables = HashMap::new();

        // T021: Read node table data using multi-page support
        if header.metadata_range.num_pages > 0 {
            let table_data_bytes = read_multi_page(buffer_pool, header.metadata_range)?;

            if !table_data_bytes.is_empty() {
                if let Ok(table_data_map) =
                    bincode::deserialize::<HashMap<String, TableData>>(&table_data_bytes)
                {
                    for (table_name, table_data) in table_data_map {
                        if let Some(schema) = catalog.get_table(&table_name) {
                            let table = NodeTable::from_data(schema, table_data);
                            tables.insert(table_name, Arc::new(table));
                        }
                    }
                }
            }
        }

        // Create empty tables for any schemas not in the loaded data
        for table_name in catalog.table_names() {
            if !tables.contains_key(table_name) {
                if let Some(schema) = catalog.get_table(table_name) {
                    let table = NodeTable::new(schema);
                    tables.insert(table_name.to_string(), Arc::new(table));
                }
            }
        }

        Ok(tables)
    }

    /// Loads relationship table data from the database file on disk.
    ///
    /// Reads serialized `HashMap<String, RelTableData>` from page 3 (the relationship
    /// metadata page) and reconstructs in-memory `RelTable` instances for each
    /// relationship table defined in the catalog.
    ///
    /// # Page Layout
    ///
    /// The relationship metadata page uses a length-prefixed format:
    /// - Bytes `[0..4]`: `u32` LE length of the serialized data
    /// - Bytes `[4..4+len]`: bincode-serialized `HashMap<String, RelTableData>`
    ///
    /// # Behavior
    ///
    /// - If `rel_metadata_range.num_pages == 0` (e.g., migrated v1 database with no
    ///   rel data page allocated), returns empty `RelTable` instances for each schema.
    /// - If the length prefix is 0, no relationship data was saved; empty tables are created.
    /// - If deserialization succeeds, `RelTable::from_data()` reconstructs the CSR
    ///   structures and validates invariants in debug builds.
    /// - Any relationship schema in the catalog without persisted data gets an empty table.
    ///
    /// # Errors
    ///
    /// Returns [`RuzuError::RelTableCorrupted`] if the length prefix exceeds the page
    /// capacity, or [`RuzuError::RelTableLoadError`] if bincode deserialization fails,
    /// or if persisted data references a relationship table name absent from the catalog.
    fn load_rel_table_data(
        buffer_pool: &BufferPool,
        catalog: &Catalog,
        header: &DatabaseHeader,
    ) -> Result<HashMap<String, RelTable>> {
        use storage::RelTableData;

        let mut rel_tables = HashMap::new();

        // Handle empty database case (no rel metadata pages allocated)
        if header.rel_metadata_range.num_pages == 0 {
            for rel_name in catalog.rel_table_names() {
                if let Some(schema) = catalog.get_rel_table(rel_name) {
                    let rel_table = RelTable::new(schema);
                    rel_tables.insert(rel_name.to_string(), rel_table);
                }
            }
            return Ok(rel_tables);
        }

        // T029: Read rel table data using multi-page support
        let rel_data_bytes = read_multi_page(buffer_pool, header.rel_metadata_range)
            .map_err(|e| RuzuError::RelTableLoadError(format!("Failed to read rel_table data: {e}")))?;

        if !rel_data_bytes.is_empty() {
            let rel_data_map: HashMap<String, RelTableData> =
                bincode::deserialize(&rel_data_bytes).map_err(|e| {
                    RuzuError::RelTableLoadError(format!("Failed to deserialize rel_tables: {e}"))
                })?;

            for (table_name, rel_data) in rel_data_map {
                if let Some(schema) = catalog.get_rel_table(&table_name) {
                    let rel_table = RelTable::from_data(schema, rel_data);
                    rel_tables.insert(table_name, rel_table);
                } else {
                    return Err(RuzuError::RelTableCorrupted(format!(
                        "Rel table '{table_name}' has data but no schema in catalog"
                    )));
                }
            }
        }

        // Create empty tables for any relationship schemas not in the loaded data
        for rel_name in catalog.rel_table_names() {
            if !rel_tables.contains_key(rel_name) {
                if let Some(schema) = catalog.get_rel_table(rel_name) {
                    let rel_table = RelTable::new(schema);
                    rel_tables.insert(rel_name.to_string(), rel_table);
                }
            }
        }

        Ok(rel_tables)
    }

    /// Saves all data (catalog and table data) to disk.
    fn save_all_data(&mut self) -> Result<()> {
        use storage::TableData;
        use storage::RelTableData;

        let buffer_pool = self
            .buffer_pool
            .as_ref()
            .ok_or_else(|| RuzuError::StorageError("No buffer pool in in-memory mode".into()))?;
        let header = self
            .header
            .as_ref()
            .ok_or_else(|| RuzuError::StorageError("No header in in-memory mode".into()))?;

        // T036: Serialize catalog using multi-page support
        let catalog_bytes = self.catalog.serialize()?;

        let catalog_pages_needed = calculate_pages_needed(catalog_bytes.len());
        let current_catalog_range = header.catalog_range;

        let catalog_data_range = if catalog_pages_needed <= current_catalog_range.num_pages {
            // Reuse existing range (fits in already allocated pages)
            current_catalog_range
        } else {
            // Allocate new contiguous range
            buffer_pool.allocate_page_range(catalog_pages_needed)?
        };

        write_multi_page(buffer_pool, catalog_data_range, &catalog_bytes)?;

        // T038: Update header catalog_range to reflect the (possibly new) range
        if let Some(ref mut header) = self.header {
            header.catalog_range = catalog_data_range;
        }

        // Re-borrow header after catalog range mutation
        let header = self
            .header
            .as_ref()
            .ok_or_else(|| RuzuError::StorageError("No header in in-memory mode".into()))?;

        // Serialize table data
        let mut table_data_map: HashMap<String, TableData> = HashMap::new();
        for (table_name, table) in &self.tables {
            table_data_map.insert(table_name.clone(), table.to_data());
        }

        let table_data_bytes = bincode::serialize(&table_data_map)
            .map_err(|e| RuzuError::StorageError(format!("Failed to serialize table data: {e}")))?;

        // T020: Write node table data using multi-page support
        let pages_needed = calculate_pages_needed(table_data_bytes.len());
        let current_metadata_range = header.metadata_range;

        let node_data_range = if pages_needed <= current_metadata_range.num_pages {
            // Reuse existing range (fits in already allocated pages)
            current_metadata_range
        } else {
            // Allocate new contiguous range
            buffer_pool.allocate_page_range(pages_needed)?
        };

        write_multi_page(buffer_pool, node_data_range, &table_data_bytes)?;

        // T022: Update header metadata_range to reflect the (possibly new) range
        if let Some(ref mut header) = self.header {
            header.metadata_range = node_data_range;
        }

        // Re-borrow header after mutation
        let header = self
            .header
            .as_ref()
            .ok_or_else(|| RuzuError::StorageError("No header in in-memory mode".into()))?;

        // T028: Serialize relationship table data using multi-page support
        let mut rel_data_map: HashMap<String, RelTableData> = HashMap::new();
        for (table_name, rel_table) in &self.rel_tables {
            rel_data_map.insert(table_name.clone(), rel_table.to_data());
        }

        let rel_data_bytes = bincode::serialize(&rel_data_map)
            .map_err(|e| RuzuError::StorageError(format!("Failed to serialize rel_tables: {e}")))?;

        // T031: Size validation removed — multi-page allocation handles arbitrary sizes
        let rel_pages_needed = calculate_pages_needed(rel_data_bytes.len());
        let current_rel_range = header.rel_metadata_range;

        let rel_data_range = if rel_pages_needed <= current_rel_range.num_pages {
            // Reuse existing range (fits in already allocated pages)
            current_rel_range
        } else {
            // Allocate new contiguous range
            buffer_pool.allocate_page_range(rel_pages_needed)?
        };

        write_multi_page(buffer_pool, rel_data_range, &rel_data_bytes)?;

        // T030: Update header rel_metadata_range to reflect the (possibly new) range
        if let Some(ref mut header) = self.header {
            header.rel_metadata_range = rel_data_range;
        }

        Ok(())
    }

    /// Saves the database header to disk.
    fn save_header(&mut self) -> Result<()> {
        use storage::PageId;

        let buffer_pool = self
            .buffer_pool
            .as_ref()
            .ok_or_else(|| RuzuError::StorageError("No buffer pool in in-memory mode".into()))?;
        let header = self
            .header
            .as_mut()
            .ok_or_else(|| RuzuError::StorageError("No header in in-memory mode".into()))?;

        header.update_checksum();
        let header_bytes = header.serialize()?;

        // Write to page 0
        let mut header_handle = buffer_pool.pin(PageId::new(0, 0))?;
        let data = header_handle.data_mut();
        data[..header_bytes.len()].copy_from_slice(&header_bytes);

        Ok(())
    }

    /// Flushes all changes to disk and closes the database gracefully.
    ///
    /// This is called automatically when the Database is dropped, but
    /// calling it explicitly allows error handling.
    ///
    /// # Errors
    ///
    /// Returns an error if saving catalog, header, or flushing pages fails.
    pub fn close(&mut self) -> Result<()> {
        if self.db_path.is_none() {
            return Ok(()); // In-memory mode, nothing to close
        }

        if self.dirty {
            // Perform a final checkpoint to save all data and truncate WAL
            self.checkpoint()?;
        }

        Ok(())
    }

    /// Forces a checkpoint, writing all dirty pages to disk and truncating WAL.
    ///
    /// This operation:
    /// 1. Writes a checkpoint record to WAL
    /// 2. Flushes all dirty pages from buffer pool
    /// 3. Saves catalog and header
    /// 4. Truncates WAL (removes replayed records)
    ///
    /// # Errors
    ///
    /// Returns an error if saving catalog, header, or flushing pages fails.
    pub fn checkpoint(&mut self) -> Result<()> {
        if self.buffer_pool.is_some() {
            // Save all data (catalog, node tables, and relationship tables)
            self.save_all_data()?;

            // Save header
            self.save_header()?;

            // Flush buffer pool
            if let Some(ref pool) = self.buffer_pool {
                pool.flush_all()?;
            }

            // Write checkpoint record and truncate WAL
            if let Some(ref mut wal_writer) = self.wal_writer {
                let checkpoint_id = self.checkpointer.next_id();
                let lsn = wal_writer.next_lsn();
                let record = WalRecord::checkpoint(0, lsn, checkpoint_id);
                wal_writer.append(&record)?;
                wal_writer.sync()?;
                wal_writer.truncate()?;
            }

            self.dirty = false;
        }
        Ok(())
    }

    /// Returns a reference to the catalog.
    #[must_use]
    pub fn catalog(&self) -> &Catalog {
        &self.catalog
    }

    /// Returns buffer pool statistics.
    ///
    /// Returns `None` if the database is in-memory mode (no buffer pool).
    ///
    /// # Example
    ///
    /// ```ignore
    /// let db = Database::open(&path, DatabaseConfig::default())?;
    /// if let Some(stats) = db.buffer_pool_stats() {
    ///     println!("Cache hit rate: {:?}", stats.hit_rate());
    ///     println!("Evictions: {}", stats.evictions);
    /// }
    /// ```
    #[must_use]
    pub fn buffer_pool_stats(&self) -> Option<storage::BufferPoolStats> {
        self.buffer_pool
            .as_ref()
            .map(storage::buffer_pool::BufferPool::stats)
    }

    /// Executes a Cypher query and returns the result.
    ///
    /// # Errors
    ///
    /// Returns an error if parsing fails, the schema is invalid,
    /// or execution encounters an error.
    pub fn execute(&mut self, query: &str) -> Result<QueryResult> {
        let statement = parser::parse_query(query)?;

        match statement {
            Statement::CreateNodeTable {
                table_name,
                columns,
                primary_key,
            } => self.execute_create_node_table(table_name, columns, primary_key),

            Statement::CreateNode { label, properties } => {
                self.execute_create_node(&label, &properties)
            }

            Statement::Match {
                var,
                label,
                filter,
                projections,
                order_by,
                skip,
                limit,
            } => self.execute_match(&var, &label, filter, &QueryModifiers {
                projections: &projections,
                order_by: order_by.as_ref(),
                skip,
                limit,
            }),

            Statement::CreateRelTable {
                table_name,
                src_table,
                dst_table,
                columns,
            } => self.execute_create_rel_table(table_name, src_table, dst_table, columns),

            Statement::MatchCreate {
                src_node,
                dst_node,
                rel_type,
                rel_props,
                src_var,
                dst_var,
            } => {
                self.execute_match_create(&src_node, &dst_node, &rel_type, rel_props, src_var, dst_var)
            }

            Statement::MatchRel {
                src_node,
                rel_var,
                rel_type,
                dst_node,
                filter,
                projections,
                order_by,
                skip,
                limit,
                path_bounds,
            } => self.execute_match_rel(
                &RelPattern {
                    src_node: &src_node,
                    rel_var: rel_var.as_ref(),
                    rel_type: &rel_type,
                    dst_node: &dst_node,
                    filter: filter.as_ref(),
                    path_bounds,
                },
                &QueryModifiers {
                    projections: &projections,
                    order_by: order_by.as_ref(),
                    skip,
                    limit,
                },
            ),

            Statement::Copy {
                table_name,
                file_path,
                options,
            } => self.execute_copy(&table_name, &file_path, &options),

            Statement::Explain { inner } => self.execute_explain(*inner),
        }
    }

    #[allow(clippy::unused_self)]
    fn execute_explain(&mut self, inner: Statement) -> Result<QueryResult> {
        // For EXPLAIN, we parse and bind the inner query but don't execute it
        // Instead, we return the query plan as text
        match inner {
            Statement::Match {
                var,
                label,
                filter,
                projections,
                ..
            } => {
                // Build a logical plan description
                let mut plan_text = String::new();
                plan_text.push_str("NodeScan: ");
                plan_text.push_str(&label);
                plan_text.push_str(" as ");
                plan_text.push_str(&var);
                plan_text.push('\n');

                if filter.is_some() {
                    plan_text.push_str("  Filter: predicate\n");
                }

                if !projections.is_empty() {
                    plan_text.push_str("  Project: [");
                    let proj_names: Vec<String> = projections.iter().map(|p| {
                        match p {
                            parser::ast::ReturnItem::Projection { var, property } => {
                                format!("{var}.{property}")
                            }
                            parser::ast::ReturnItem::Aggregate(agg) => {
                                if let Some((var, prop)) = &agg.input {
                                    format!("{:?}({}.{})", agg.function, var, prop)
                                } else {
                                    format!("{:?}(*)", agg.function)
                                }
                            }
                        }
                    }).collect();
                    plan_text.push_str(&proj_names.join(", "));
                    plan_text.push_str("]\n");
                }

                Ok(QueryResult::Explain(plan_text))
            }
            Statement::MatchRel {
                src_node,
                rel_type,
                dst_node,
                filter,
                projections,
                ..
            } => {
                let mut plan_text = String::new();
                plan_text.push_str("NodeScan: ");
                plan_text.push_str(&src_node.label);
                plan_text.push_str(" as ");
                plan_text.push_str(&src_node.var);
                plan_text.push('\n');

                plan_text.push_str("  Extend: ");
                plan_text.push_str(&rel_type);
                plan_text.push_str(" (");
                plan_text.push_str(&src_node.var);
                plan_text.push_str(" -> ");
                plan_text.push_str(&dst_node.var);
                plan_text.push_str(")\n");

                if filter.is_some() {
                    plan_text.push_str("    Filter: predicate\n");
                }

                if !projections.is_empty() {
                    plan_text.push_str("    Project: [");
                    let proj_names: Vec<String> = projections.iter().map(|p| {
                        match p {
                            parser::ast::ReturnItem::Projection { var, property } => {
                                format!("{var}.{property}")
                            }
                            parser::ast::ReturnItem::Aggregate(agg) => {
                                if let Some((var, prop)) = &agg.input {
                                    format!("{:?}({}.{})", agg.function, var, prop)
                                } else {
                                    format!("{:?}(*)", agg.function)
                                }
                            }
                        }
                    }).collect();
                    plan_text.push_str(&proj_names.join(", "));
                    plan_text.push_str("]\n");
                }

                Ok(QueryResult::Explain(plan_text))
            }
            _ => Err(RuzuError::ParseError {
                line: 0,
                col: 0,
                message: "EXPLAIN only supports MATCH queries".into(),
            }),
        }
    }

    fn execute_create_node_table(
        &mut self,
        table_name: String,
        columns: Vec<(String, String)>,
        primary_key: Vec<String>,
    ) -> Result<QueryResult> {
        // Convert column definitions
        let column_defs: Vec<ColumnDef> = columns
            .into_iter()
            .map(|(name, type_str)| {
                let data_type = match type_str.to_uppercase().as_str() {
                    "INT64" => DataType::Int64,
                    "STRING" => DataType::String,
                    "FLOAT64" => DataType::Float64,
                    "BOOL" => DataType::Bool,
                    _ => {
                        return Err(RuzuError::SchemaError(format!(
                            "Unknown data type: {type_str}"
                        )))
                    }
                };
                ColumnDef::new(name, data_type)
            })
            .collect::<Result<Vec<_>>>()?;

        // Create and validate schema
        let schema = NodeTableSchema::new(table_name.clone(), column_defs, primary_key)?;

        // Register in catalog
        self.catalog.create_table(schema.clone())?;

        // Create storage table
        let table = NodeTable::new(Arc::new(schema));
        self.tables.insert(table_name, Arc::new(table));

        // Mark database as dirty
        self.dirty = true;

        Ok(QueryResult::empty())
    }

    fn execute_create_node(
        &mut self,
        label: &str,
        properties: &[(String, Literal)],
    ) -> Result<QueryResult> {
        // Get table schema for table_id and column ordering
        let schema = self
            .catalog
            .get_table(label)
            .ok_or_else(|| RuzuError::SchemaError(format!("Table '{label}' does not exist")))?;
        let table_id = schema.table_id;

        // Get the table
        let table = self
            .tables
            .get_mut(label)
            .ok_or_else(|| RuzuError::SchemaError(format!("Table '{label}' does not exist")))?;

        // Convert properties to a row, with type promotion for FLOAT64 columns
        let mut row: HashMap<String, Value> = HashMap::new();
        for (name, literal) in properties {
            let value = literal_to_value(literal);
            // Promote Int64 to Float64 if the column type is FLOAT64
            #[allow(clippy::cast_precision_loss)]
            let value = if let Value::Int64(n) = &value {
                if let Some(col) = schema.columns.iter().find(|c| c.name == *name) {
                    if col.data_type == DataType::Float64 {
                        Value::Float64(*n as f64)
                    } else {
                        value
                    }
                } else {
                    value
                }
            } else {
                value
            };
            row.insert(name.clone(), value);
        }

        // Build row values in schema column order for WAL
        let mut row_values: Vec<Value> = Vec::new();
        for col in &schema.columns {
            if let Some(val) = row.get(&col.name) {
                row_values.push(val.clone());
            } else {
                row_values.push(Value::Null);
            }
        }

        // Write WAL record BEFORE modifying data (Write-Ahead Logging principle)
        if let Some(ref mut wal_writer) = self.wal_writer {
            let tx_id = self.next_tx_id.fetch_add(1, Ordering::Relaxed);

            // Begin transaction
            let begin_lsn = wal_writer.next_lsn();
            let begin_record = WalRecord::begin_transaction(tx_id, begin_lsn);
            wal_writer.append(&begin_record)?;

            // Table insertion record
            let insert_lsn = wal_writer.next_lsn();
            let insert_record = WalRecord::new(
                WalRecordType::TableInsertion,
                tx_id,
                insert_lsn,
                WalPayload::TableInsertion {
                    table_id,
                    rows: vec![row_values],
                },
            );
            wal_writer.append(&insert_record)?;

            // Commit transaction
            let commit_lsn = wal_writer.next_lsn();
            let commit_record = WalRecord::commit(tx_id, commit_lsn);
            wal_writer.append(&commit_record)?;

            // Flush WAL to ensure durability
            if self.config.wal_sync {
                wal_writer.sync()?;
            } else {
                wal_writer.flush()?;
            }
        }

        // Need to get mutable access to the table
        // Since we're using Arc, we need to get inner mutable reference
        let table = Arc::get_mut(table).ok_or_else(|| {
            RuzuError::ExecutionError("Cannot modify table: multiple references exist".into())
        })?;

        // Insert the row
        table.insert(&row)?;

        // Mark database as dirty
        self.dirty = true;

        Ok(QueryResult::empty())
    }

    fn execute_match(
        &self,
        var: &str,
        label: &str,
        filter: Option<parser::ast::Expression>,
        modifiers: &QueryModifiers<'_>,
    ) -> Result<QueryResult> {
        use parser::ast::AstAggregateFunction;
        let projections = modifiers.projections;
        let order_by = modifiers.order_by;
        let skip = modifiers.skip;
        let limit = modifiers.limit;

        // Get the table
        let table = self
            .tables
            .get(label)
            .ok_or_else(|| RuzuError::SchemaError(format!("Table '{label}' does not exist")))?;

        // Convert ReturnItem to (String, String) for simple projections
        let mut simple_projections: Vec<(String, String)> = Vec::new();
        let mut has_aggregates = false;

        for item in projections {
            match item {
                ReturnItem::Projection { var, property } => {
                    simple_projections.push((var.clone(), property.clone()));
                }
                ReturnItem::Aggregate(_) => {
                    has_aggregates = true;
                }
            }
        }

        // Build the execution pipeline
        let scan = ScanOperator::new(Arc::clone(table), var.to_string());
        let mut operator: Box<dyn PhysicalOperator> = Box::new(scan);

        // Add filter if present
        if let Some(expr) = filter {
            operator = Box::new(FilterOperator::new(operator, expr));
        }

        // If we have aggregates, handle them specially
        if has_aggregates {
            // Collect all rows first for aggregation
            let mut rows: Vec<Row> = Vec::new();
            while let Some(row) = operator.next()? {
                rows.push(row);
            }

            // Compute aggregates
            let mut result_row = Row::new();
            let mut output_columns = Vec::new();

            for item in projections {
                match item {
                    ReturnItem::Projection { var: v, property } => {
                        let col_name = format!("{v}.{property}");
                        output_columns.push(col_name.clone());
                        // For non-aggregates with aggregates, use the first row's value (if any)
                        if let Some(first_row) = rows.first() {
                            if let Some(val) = first_row.get(&col_name) {
                                result_row.set(col_name, val.clone());
                            }
                        }
                    }
                    ReturnItem::Aggregate(agg) => {
                        let agg_name = match agg.function {
                            AstAggregateFunction::Count => "COUNT",
                            AstAggregateFunction::Sum => "SUM",
                            AstAggregateFunction::Avg => "AVG",
                            AstAggregateFunction::Min => "MIN",
                            AstAggregateFunction::Max => "MAX",
                        };

                        let col_name = if let Some((v, p)) = &agg.input {
                            format!("{agg_name}({v}.{p})")
                        } else {
                            format!("{agg_name}(*)")
                        };
                        output_columns.push(col_name.clone());

                        // Compute aggregate value
                        let agg_value = match agg.function {
                            AstAggregateFunction::Count => {
                                if agg.input.is_none() {
                                    // COUNT(*)
                                    Value::Int64(rows.len() as i64)
                                } else {
                                    // COUNT(property) - count non-null values
                                    let (v, p) = agg.input.as_ref().unwrap();
                                    let prop_name = format!("{v}.{p}");
                                    let count = rows.iter()
                                        .filter(|r| r.get(&prop_name).is_some() && !matches!(r.get(&prop_name), Some(Value::Null)))
                                        .count();
                                    Value::Int64(count as i64)
                                }
                            }
                            AstAggregateFunction::Sum => {
                                let (v, p) = agg.input.as_ref().ok_or_else(|| {
                                    RuzuError::ExecutionError("SUM requires an argument".into())
                                })?;
                                let prop_name = format!("{v}.{p}");
                                let sum: i64 = rows.iter()
                                    .filter_map(|r| r.get(&prop_name))
                                    .filter_map(|v| match v {
                                        Value::Int64(n) => Some(*n),
                                        _ => None,
                                    })
                                    .sum();
                                Value::Int64(sum)
                            }
                            AstAggregateFunction::Avg => {
                                let (v, p) = agg.input.as_ref().ok_or_else(|| {
                                    RuzuError::ExecutionError("AVG requires an argument".into())
                                })?;
                                let prop_name = format!("{v}.{p}");
                                let values: Vec<i64> = rows.iter()
                                    .filter_map(|r| r.get(&prop_name))
                                    .filter_map(|v| match v {
                                        Value::Int64(n) => Some(*n),
                                        _ => None,
                                    })
                                    .collect();
                                if values.is_empty() {
                                    Value::Null
                                } else {
                                    let sum: i64 = values.iter().sum();
                                    #[allow(clippy::cast_precision_loss)]
                                    let avg = sum as f64 / values.len() as f64;
                                    Value::Float64(avg)
                                }
                            }
                            AstAggregateFunction::Min => {
                                let (v, p) = agg.input.as_ref().ok_or_else(|| {
                                    RuzuError::ExecutionError("MIN requires an argument".into())
                                })?;
                                let prop_name = format!("{v}.{p}");
                                let values: Vec<&Value> = rows.iter()
                                    .filter_map(|r| r.get(&prop_name))
                                    .filter(|v| !v.is_null())
                                    .collect();
                                if values.is_empty() {
                                    Value::Null
                                } else {
                                    let mut min_val = values[0].clone();
                                    for v in &values[1..] {
                                        if let Some(std::cmp::Ordering::Less) = (*v).compare(&min_val) {
                                            min_val = (*v).clone();
                                        }
                                    }
                                    min_val
                                }
                            }
                            AstAggregateFunction::Max => {
                                let (v, p) = agg.input.as_ref().ok_or_else(|| {
                                    RuzuError::ExecutionError("MAX requires an argument".into())
                                })?;
                                let prop_name = format!("{v}.{p}");
                                let values: Vec<&Value> = rows.iter()
                                    .filter_map(|r| r.get(&prop_name))
                                    .filter(|v| !v.is_null())
                                    .collect();
                                if values.is_empty() {
                                    Value::Null
                                } else {
                                    let mut max_val = values[0].clone();
                                    for v in &values[1..] {
                                        if let Some(std::cmp::Ordering::Greater) = (*v).compare(&max_val) {
                                            max_val = (*v).clone();
                                        }
                                    }
                                    max_val
                                }
                            }
                        };
                        result_row.set(col_name, agg_value);
                    }
                }
            }

            let mut result = QueryResult::new(output_columns);
            result.add_row(result_row);
            return Ok(result);
        }

        // Add projection for non-aggregate queries
        let project = ProjectOperator::new(operator, simple_projections);
        let output_columns = project.output_columns();
        let mut operator: Box<dyn PhysicalOperator> = Box::new(project);

        // Execute and collect results
        let mut rows: Vec<Row> = Vec::new();
        while let Some(row) = operator.next()? {
            rows.push(row);
        }

        // Apply ORDER BY if present
        if let Some(order_items) = order_by {
            rows.sort_by(|a, b| {
                for order_item in order_items {
                    let col_name = format!("{}.{}", order_item.var, order_item.property);
                    let val_a = a.get(&col_name);
                    let val_b = b.get(&col_name);

                    let ordering = match (val_a, val_b) {
                        (Some(va), Some(vb)) => va.compare(vb).unwrap_or(std::cmp::Ordering::Equal),
                        (None, Some(_)) => std::cmp::Ordering::Greater, // NULLs last
                        (Some(_), None) => std::cmp::Ordering::Less,
                        (None, None) => std::cmp::Ordering::Equal,
                    };

                    if ordering != std::cmp::Ordering::Equal {
                        return if order_item.ascending {
                            ordering
                        } else {
                            ordering.reverse()
                        };
                    }
                }
                std::cmp::Ordering::Equal
            });
        }

        // Apply SKIP
        let skip_count = usize::try_from(skip.unwrap_or(0).max(0)).unwrap_or(0);
        let rows = rows.into_iter().skip(skip_count);

        // Apply LIMIT
        let rows: Vec<Row> = if let Some(limit_count) = limit {
            rows.take(usize::try_from(limit_count.max(0)).unwrap_or(0))
                .collect()
        } else {
            rows.collect()
        };

        let mut result = QueryResult::new(output_columns);
        for row in rows {
            result.add_row(row);
        }

        Ok(result)
    }

    fn execute_create_rel_table(
        &mut self,
        table_name: String,
        src_table: String,
        dst_table: String,
        columns: Vec<(String, String)>,
    ) -> Result<QueryResult> {
        // Validate source table exists
        if !self.catalog.table_exists(&src_table) {
            return Err(RuzuError::SchemaError(format!(
                "Source table '{src_table}' does not exist"
            )));
        }

        // Validate destination table exists
        if !self.catalog.table_exists(&dst_table) {
            return Err(RuzuError::SchemaError(format!(
                "Destination table '{dst_table}' does not exist"
            )));
        }

        // Convert column definitions
        let column_defs: Vec<ColumnDef> = columns
            .into_iter()
            .map(|(name, type_str)| {
                let data_type = match type_str.to_uppercase().as_str() {
                    "INT64" => DataType::Int64,
                    "STRING" => DataType::String,
                    "FLOAT64" => DataType::Float64,
                    "BOOL" => DataType::Bool,
                    _ => {
                        return Err(RuzuError::SchemaError(format!(
                            "Unknown data type: {type_str}"
                        )))
                    }
                };
                ColumnDef::new(name, data_type)
            })
            .collect::<Result<Vec<_>>>()?;

        // Create relationship table schema
        let schema = RelTableSchema::new(
            table_name.clone(),
            src_table,
            dst_table,
            column_defs,
            Direction::Both,
        )?;

        // Register in catalog
        self.catalog.create_rel_table(schema.clone())?;

        // Create storage table
        let rel_table = RelTable::new(Arc::new(schema));
        self.rel_tables.insert(table_name, rel_table);

        // Mark database as dirty
        self.dirty = true;

        Ok(QueryResult::empty())
    }

    fn execute_match_create(
        &mut self,
        src_node: &NodeFilter,
        dst_node: &NodeFilter,
        rel_type: &str,
        rel_props: Vec<(String, Literal)>,
        _src_var: String,
        _dst_var: String,
    ) -> Result<QueryResult> {
        // Validate relationship table exists
        let rel_schema = self.catalog.get_rel_table(rel_type).ok_or_else(|| {
            RuzuError::SchemaError(format!("Relationship table '{rel_type}' does not exist"))
        })?;
        let rel_table_id = rel_schema.table_id;

        // Find source node
        let src_table = self.tables.get(&src_node.label).ok_or_else(|| {
            RuzuError::SchemaError(format!("Table '{}' does not exist", src_node.label))
        })?;

        let src_node_offset = if let Some((key, value)) = &src_node.property_filter {
            let val = literal_to_value(value);
            src_table.find_by_pk(key, &val)
        } else {
            None
        };

        // Find destination node
        let dst_table = self.tables.get(&dst_node.label).ok_or_else(|| {
            RuzuError::SchemaError(format!("Table '{}' does not exist", dst_node.label))
        })?;

        let dst_node_offset = if let Some((key, value)) = &dst_node.property_filter {
            let val = literal_to_value(value);
            dst_table.find_by_pk(key, &val)
        } else {
            None
        };

        // If either source or destination not found, nothing to create
        let (src_offset, dst_offset) = match (src_node_offset, dst_node_offset) {
            (Some(s), Some(d)) => (s as u64, d as u64),
            _ => return Ok(QueryResult::empty()), // No match, no relationship created
        };

        // Convert relationship properties
        let props: Vec<Value> = rel_props
            .into_iter()
            .map(|(_, literal)| literal_into_value(literal))
            .collect();

        // Get mutable reference to relationship table
        let rel_table = self.rel_tables.get_mut(rel_type).ok_or_else(|| {
            RuzuError::ExecutionError(format!(
                "Relationship table '{rel_type}' not found in storage"
            ))
        })?;

        // Insert the relationship
        let _rel_id = rel_table.insert(src_offset, dst_offset, props.clone())?;

        // Write WAL record
        if let Some(ref mut wal_writer) = self.wal_writer {
            let tx_id = self.next_tx_id.fetch_add(1, Ordering::Relaxed);

            let begin_lsn = wal_writer.next_lsn();
            let begin_record = WalRecord::begin_transaction(tx_id, begin_lsn);
            wal_writer.append(&begin_record)?;

            let insert_lsn = wal_writer.next_lsn();
            let insert_record = WalRecord::new(
                WalRecordType::RelInsertion,
                tx_id,
                insert_lsn,
                WalPayload::RelInsertion {
                    table_id: rel_table_id,
                    src: src_offset,
                    dst: dst_offset,
                    props,
                },
            );
            wal_writer.append(&insert_record)?;

            let commit_lsn = wal_writer.next_lsn();
            let commit_record = WalRecord::commit(tx_id, commit_lsn);
            wal_writer.append(&commit_record)?;

            if self.config.wal_sync {
                wal_writer.sync()?;
            } else {
                wal_writer.flush()?;
            }
        }

        self.dirty = true;

        Ok(QueryResult::empty())
    }

    fn execute_match_rel(
        &self,
        rel: &RelPattern<'_>,
        modifiers: &QueryModifiers<'_>,
    ) -> Result<QueryResult> {
        let src_node = rel.src_node;
        let rel_var = rel.rel_var;
        let rel_type = rel.rel_type;
        let dst_node = rel.dst_node;
        let filter = rel.filter;
        let path_bounds = rel.path_bounds;
        let projections = modifiers.projections;
        let order_by = modifiers.order_by;
        let skip = modifiers.skip;
        let limit = modifiers.limit;
        // Convert ReturnItem to (String, String) for now - aggregates in rel queries handled later
        let simple_projections: Vec<(String, String)> = projections.iter().filter_map(|item| {
            match item {
                ReturnItem::Projection { var, property } => Some((var.clone(), property.clone())),
                ReturnItem::Aggregate(_) => None, // TODO: Handle aggregates in rel queries
            }
        }).collect();
        // Validate relationship table exists
        let rel_schema = self.catalog.get_rel_table(rel_type).ok_or_else(|| {
            RuzuError::SchemaError(format!("Relationship table '{rel_type}' does not exist"))
        })?;

        // Get tables
        let src_table = self.tables.get(&src_node.label).ok_or_else(|| {
            RuzuError::SchemaError(format!("Table '{}' does not exist", src_node.label))
        })?;

        let dst_table = self.tables.get(&dst_node.label).ok_or_else(|| {
            RuzuError::SchemaError(format!("Table '{}' does not exist", dst_node.label))
        })?;

        // Get relationship table
        let rel_table = self.rel_tables.get(rel_type).ok_or_else(|| {
            RuzuError::ExecutionError(format!(
                "Relationship table '{rel_type}' not found in storage"
            ))
        })?;

        // Determine output columns
        let output_columns: Vec<String> = simple_projections
            .iter()
            .map(|(var, prop)| format!("{var}.{prop}"))
            .collect();

        // Check if we have a filter on source node
        let src_offsets: Vec<usize> = if let Some((key, value)) = &src_node.property_filter {
            let val = literal_to_value(value);
            if let Some(offset) = src_table.find_by_pk(key, &val) {
                vec![offset]
            } else {
                vec![]
            }
        } else {
            // All source nodes
            (0..src_table.len()).collect()
        };

        // Check if we have a filter on destination node
        let dst_filter = dst_node.property_filter.as_ref().map(|(key, value)| {
            let val = literal_to_value(value);
            (key.clone(), val)
        });

        // Collect all rows
        let mut rows: Vec<Row> = Vec::new();

        // Multi-hop traversal using BFS when path_bounds is set
        if let Some((min_hops, max_hops)) = path_bounds {
            // BFS-based multi-hop traversal with cycle detection
            use std::collections::VecDeque;

            for src_offset in &src_offsets {
                // Queue entries: (current_node, depth, path)
                let mut queue: VecDeque<(u64, u32, Vec<u64>)> = VecDeque::new();
                queue.push_back((*src_offset as u64, 0, vec![*src_offset as u64]));

                while let Some((current_node, depth, path)) = queue.pop_front() {
                    // If we've reached max depth, stop exploring
                    if depth >= max_hops {
                        continue;
                    }

                    // Get edges from current node
                    let edges = rel_table.get_forward_edges(current_node);

                    for (next_node, _rel_id) in edges {
                        // Cycle detection - skip if we've already visited this node
                        if path.contains(&next_node) {
                            continue;
                        }

                        let new_depth = depth + 1;

                        // If within valid hop range, emit this result
                        if new_depth >= min_hops && new_depth <= max_hops {
                            // Apply destination filter if present
                            let passes_dst_filter = if let Some((ref key, ref expected_val)) = dst_filter {
                                if let Some(actual_val) = dst_table.get(next_node as usize, key) {
                                    &actual_val == expected_val
                                } else {
                                    false
                                }
                            } else {
                                true
                            };

                            if passes_dst_filter {
                                // Build result row
                                let mut row = Row::new();
                                for (var, prop) in &simple_projections {
                                    let col_name = format!("{var}.{prop}");

                                    if var == &src_node.var {
                                        if let Some(val) = src_table.get(*src_offset, prop) {
                                            row.set(col_name, val);
                                        }
                                    } else if var == &dst_node.var {
                                        if let Some(val) = dst_table.get(next_node as usize, prop) {
                                            row.set(col_name, val);
                                        }
                                    }
                                }
                                rows.push(row);
                            }
                        }

                        // Add to queue for further exploration if not at max depth
                        if new_depth < max_hops {
                            let mut new_path = path.clone();
                            new_path.push(next_node);
                            queue.push_back((next_node, new_depth, new_path));
                        }
                    }
                }
            }
        } else {
            // Single-hop traversal (original behavior)
            for src_offset in src_offsets {
                let edges = rel_table.get_forward_edges(src_offset as u64);

                for (dst_offset, rel_id) in edges {
                // Apply destination filter if present
                if let Some((ref key, ref expected_val)) = dst_filter {
                    if let Some(actual_val) = dst_table.get(dst_offset as usize, key) {
                        if &actual_val != expected_val {
                            continue; // Filter out
                        }
                    } else {
                        continue; // Key not found
                    }
                }

                // Apply WHERE clause filter if present
                // We need to evaluate the filter before building the output row
                if let Some(expr) = filter {
                    // Get the filter value directly from the table
                    let filter_val = if expr.var == src_node.var {
                        src_table.get(src_offset, &expr.property)
                    } else if expr.var == dst_node.var {
                        dst_table.get(dst_offset as usize, &expr.property)
                    } else if rel_var == Some(&expr.var) {
                        // Relationship property
                        if let Some(props) = rel_table.get_properties(rel_id) {
                            rel_schema
                                .columns
                                .iter()
                                .position(|c| c.name == expr.property)
                                .and_then(|idx| props.get(idx).cloned())
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    let matches = match filter_val {
                        Some(val) => {
                            let literal_val = literal_to_value(&expr.value);
                            // Promote for cross-type comparison (Int64 vs Float64)
                            let (val, literal_val) = promote_for_comparison(val, literal_val);
                            let cmp_result = val.compare(&literal_val);
                            match cmp_result {
                                None => false,
                                Some(ordering) => match expr.op {
                                    parser::ast::ComparisonOp::Gt => {
                                        ordering == std::cmp::Ordering::Greater
                                    }
                                    parser::ast::ComparisonOp::Lt => {
                                        ordering == std::cmp::Ordering::Less
                                    }
                                    parser::ast::ComparisonOp::Eq => {
                                        ordering == std::cmp::Ordering::Equal
                                    }
                                    parser::ast::ComparisonOp::Gte => {
                                        ordering != std::cmp::Ordering::Less
                                    }
                                    parser::ast::ComparisonOp::Lte => {
                                        ordering != std::cmp::Ordering::Greater
                                    }
                                    parser::ast::ComparisonOp::Neq => {
                                        ordering != std::cmp::Ordering::Equal
                                    }
                                },
                            }
                        }
                        None => false,
                    };

                    if !matches {
                        continue;
                    }
                }

                // Build result row (only projected columns)
                let mut row = Row::new();
                for (var, prop) in &simple_projections {
                    let col_name = format!("{var}.{prop}");

                    if var == &src_node.var {
                        // Source node property
                        if let Some(val) = src_table.get(src_offset, prop) {
                            row.set(col_name, val);
                        }
                    } else if var == &dst_node.var {
                        // Destination node property
                        if let Some(val) = dst_table.get(dst_offset as usize, prop) {
                            row.set(col_name, val);
                        }
                    } else if rel_var == Some(var) {
                        // Relationship property
                        if let Some(props) = rel_table.get_properties(rel_id) {
                            // Find property by name from schema
                            for (idx, col) in rel_schema.columns.iter().enumerate() {
                                if &col.name == prop {
                                    if let Some(val) = props.get(idx) {
                                        row.set(col_name.clone(), val.clone());
                                    }
                                    break;
                                }
                            }
                        }
                    }
                }

                rows.push(row);
            }
            } // end single-hop for dst_offset in edges
        } // end else (single-hop traversal)

        // Apply ORDER BY if present
        if let Some(order_items) = order_by {
            rows.sort_by(|a, b| {
                for order_item in order_items {
                    let col_name = format!("{}.{}", order_item.var, order_item.property);
                    let val_a = a.get(&col_name);
                    let val_b = b.get(&col_name);

                    let ordering = match (val_a, val_b) {
                        (Some(va), Some(vb)) => va.compare(vb).unwrap_or(std::cmp::Ordering::Equal),
                        (None, Some(_)) => std::cmp::Ordering::Greater, // NULLs last
                        (Some(_), None) => std::cmp::Ordering::Less,
                        (None, None) => std::cmp::Ordering::Equal,
                    };

                    if ordering != std::cmp::Ordering::Equal {
                        return if order_item.ascending {
                            ordering
                        } else {
                            ordering.reverse()
                        };
                    }
                }
                std::cmp::Ordering::Equal
            });
        }

        // Apply SKIP
        let skip_count = usize::try_from(skip.unwrap_or(0).max(0)).unwrap_or(0);
        let rows = rows.into_iter().skip(skip_count);

        // Apply LIMIT
        let rows: Vec<Row> = if let Some(limit_count) = limit {
            rows.take(usize::try_from(limit_count.max(0)).unwrap_or(0))
                .collect()
        } else {
            rows.collect()
        };

        let mut result = QueryResult::new(output_columns);
        for row in rows {
            result.add_row(row);
        }

        Ok(result)
    }

    /// Executes a COPY command to import data from a CSV file.
    ///
    /// Automatically detects whether the target is a node table or relationship table.
    fn execute_copy(
        &mut self,
        table_name: &str,
        file_path: &str,
        options: &CopyOptions,
    ) -> Result<QueryResult> {
        let path = std::path::Path::new(file_path);

        // Build CSV import config from copy options
        let mut config = storage::CsvImportConfig::default();
        if let Some(has_header) = options.has_header {
            config = config.with_header(has_header);
        }
        if let Some(delimiter) = options.delimiter {
            config = config.with_delimiter(delimiter);
        }
        if let Some(skip_rows) = options.skip_rows {
            config = config.with_skip_rows(skip_rows as usize);
        }
        if let Some(ignore_errors) = options.ignore_errors {
            config = config.with_ignore_errors(ignore_errors);
        }

        // Determine if this is a node table or relationship table
        if self.catalog.table_exists(table_name) {
            // Node table import
            let result = self.import_nodes(table_name, path, config, None)?;
            Ok(QueryResult::import_result(
                result.rows_imported,
                result.rows_failed,
            ))
        } else if self.catalog.rel_table_exists(table_name) {
            // Relationship table import
            let result = self.import_relationships(table_name, path, config, None)?;
            Ok(QueryResult::import_result(
                result.rows_imported,
                result.rows_failed,
            ))
        } else {
            Err(RuzuError::SchemaError(format!(
                "Table '{table_name}' does not exist"
            )))
        }
    }

    // =========================================================================
    // Bulk CSV Import API (User Story 4 - Phase 6)
    // =========================================================================

    /// Imports nodes from a CSV file into a table.
    ///
    /// # Arguments
    ///
    /// * `table_name` - Name of the node table to import into
    /// * `csv_path` - Path to the CSV file
    /// * `config` - Import configuration (delimiter, error handling, etc.)
    /// * `progress_callback` - Optional callback for progress reporting
    ///
    /// # Returns
    ///
    /// Import result containing the number of rows imported and any errors.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The table doesn't exist
    /// - The CSV file cannot be opened
    /// - CSV columns don't match the table schema
    /// - Parsing fails and `ignore_errors` is false
    ///
    /// # Example
    ///
    /// ```ignore
    /// use ruzu::{Database, CsvImportConfig};
    ///
    /// let mut db = Database::new();
    /// db.execute("CREATE NODE TABLE Person(name STRING, age INT64, PRIMARY KEY(name))")?;
    ///
    /// let result = db.import_nodes(
    ///     "Person",
    ///     Path::new("persons.csv"),
    ///     CsvImportConfig::default(),
    ///     None,
    /// )?;
    /// println!("Imported {} rows", result.rows_imported);
    /// ```
    pub fn import_nodes(
        &mut self,
        table_name: &str,
        csv_path: &std::path::Path,
        config: storage::CsvImportConfig,
        progress_callback: Option<storage::csv::ProgressCallback>,
    ) -> Result<storage::ImportResult> {
        use storage::csv::NodeLoader;

        // Get table schema
        let schema = self.catalog.get_table(table_name).ok_or_else(|| {
            RuzuError::SchemaError(format!("Table '{table_name}' does not exist"))
        })?;

        // Get column names for batch insert
        let columns: Vec<String> = schema.columns.iter().map(|c| c.name.clone()).collect();

        // Get table for insertion (need mutable reference for batch callback)
        let table = self.tables.get_mut(table_name).ok_or_else(|| {
            RuzuError::SchemaError(format!("Table '{table_name}' not found in storage"))
        })?;

        // Get mutable reference to table
        let table = Arc::get_mut(table).ok_or_else(|| {
            RuzuError::ExecutionError("Cannot modify table: multiple references exist".into())
        })?;

        // Create node loader
        let loader = NodeLoader::new(schema.clone(), config);

        // Use streaming import - process batches incrementally without accumulating all rows
        let import_result = loader.load_streaming(
            csv_path,
            |batch| {
                // Insert batch directly into table
                table.insert_batch(batch, &columns)?;
                Ok(())
            },
            progress_callback,
        )?;

        // Mark database as dirty
        if import_result.rows_imported > 0 {
            self.dirty = true;
        }

        Ok(import_result)
    }

    /// Imports relationships from a CSV file into a relationship table.
    ///
    /// The CSV file must have `FROM` and `TO` columns that reference the primary keys
    /// of the source and destination node tables. Additional columns are treated as
    /// relationship properties.
    ///
    /// # Arguments
    ///
    /// * `rel_table_name` - Name of the relationship table to import into
    /// * `csv_path` - Path to the CSV file
    /// * `config` - Import configuration (delimiter, error handling, etc.)
    /// * `progress_callback` - Optional callback for progress reporting
    ///
    /// # Returns
    ///
    /// Import result containing the number of relationships imported and any errors.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The relationship table doesn't exist
    /// - The CSV file cannot be opened
    /// - FROM or TO columns are missing
    /// - Referenced nodes don't exist
    /// - Parsing fails and `ignore_errors` is false
    ///
    /// # Example
    ///
    /// ```ignore
    /// use ruzu::{Database, CsvImportConfig};
    ///
    /// let mut db = Database::new();
    /// // ... create node and relationship tables ...
    ///
    /// // CSV format: FROM,TO,since
    /// let result = db.import_relationships(
    ///     "KNOWS",
    ///     Path::new("relationships.csv"),
    ///     CsvImportConfig::default(),
    ///     None,
    /// )?;
    /// println!("Imported {} relationships", result.rows_imported);
    /// ```
    pub fn import_relationships(
        &mut self,
        rel_table_name: &str,
        csv_path: &std::path::Path,
        config: storage::CsvImportConfig,
        progress_callback: Option<storage::csv::ProgressCallback>,
    ) -> Result<storage::ImportResult> {
        use storage::csv::RelLoader;

        // Get relationship table schema
        let rel_schema = self.catalog.get_rel_table(rel_table_name).ok_or_else(|| {
            RuzuError::SchemaError(format!(
                "Relationship table '{rel_table_name}' does not exist"
            ))
        })?;

        // Get property columns (excluding FROM/TO which are handled specially)
        let property_columns: Vec<(String, types::DataType)> = rel_schema
            .columns
            .iter()
            .map(|c| (c.name.clone(), c.data_type))
            .collect();

        // Get source and destination tables for node lookups
        let src_table_name = rel_schema.src_table.clone();
        let dst_table_name = rel_schema.dst_table.clone();

        let src_table = self.tables.get(&src_table_name).ok_or_else(|| {
            RuzuError::SchemaError(format!("Source table '{src_table_name}' not found"))
        })?;

        let dst_table = self.tables.get(&dst_table_name).ok_or_else(|| {
            RuzuError::SchemaError(format!("Destination table '{dst_table_name}' not found"))
        })?;

        // Get source and destination table schemas for primary key lookup
        let src_schema = self.catalog.get_table(&src_table_name).ok_or_else(|| {
            RuzuError::SchemaError(format!("Source table schema '{src_table_name}' not found"))
        })?;

        let dst_schema = self.catalog.get_table(&dst_table_name).ok_or_else(|| {
            RuzuError::SchemaError(format!(
                "Destination table schema '{dst_table_name}' not found"
            ))
        })?;

        // Get primary key columns
        let src_pk_col = src_schema
            .primary_key
            .first()
            .ok_or_else(|| RuzuError::SchemaError("Source table has no primary key".into()))?
            .clone();

        let dst_pk_col = dst_schema
            .primary_key
            .first()
            .ok_or_else(|| RuzuError::SchemaError("Destination table has no primary key".into()))?
            .clone();

        // Get the relationship table for insertion
        let rel_table = self.rel_tables.get_mut(rel_table_name).ok_or_else(|| {
            RuzuError::ExecutionError(format!(
                "Relationship table '{rel_table_name}' not found in storage"
            ))
        })?;

        // Track inserted count across batches
        let mut total_inserted = 0u64;
        let mut total_failed = 0u64;
        let ignore_errors = config.ignore_errors;

        // Create relationship loader
        let loader = RelLoader::with_default_columns(property_columns, config);

        // Use streaming import - process batches incrementally
        let import_result = loader.load_streaming(
            csv_path,
            |batch| {
                // Process each relationship in the batch
                for parsed_rel in batch {
                    // Look up source node offset
                    let src_offset = src_table.find_by_pk(&src_pk_col, &parsed_rel.from_key);
                    let dst_offset = dst_table.find_by_pk(&dst_pk_col, &parsed_rel.to_key);

                    match (src_offset, dst_offset) {
                        (Some(src), Some(dst)) => {
                            // Insert the relationship
                            rel_table.insert(src as u64, dst as u64, parsed_rel.properties)?;
                            total_inserted += 1;
                        }
                        (None, _) if !ignore_errors => {
                            return Err(RuzuError::ExecutionError(format!(
                                "Source node with key {:?} not found",
                                parsed_rel.from_key
                            )));
                        }
                        (_, None) if !ignore_errors => {
                            return Err(RuzuError::ExecutionError(format!(
                                "Destination node with key {:?} not found",
                                parsed_rel.to_key
                            )));
                        }
                        _ => {
                            // ignore_errors is true, skip this relationship
                            total_failed += 1;
                        }
                    }
                }
                Ok(())
            },
            progress_callback,
        )?;

        // Build final result with actual counts
        let mut final_result = import_result;
        final_result.rows_imported = total_inserted;
        final_result.rows_failed += total_failed;

        // Mark database as dirty
        if total_inserted > 0 {
            self.dirty = true;
        }

        Ok(final_result)
    }
}

/// Promotes values for cross-type comparison (Int64 vs Float64).
/// Returns the pair with appropriate type promotion applied.
#[allow(clippy::cast_precision_loss)]
fn promote_for_comparison(a: Value, b: Value) -> (Value, Value) {
    match (&a, &b) {
        (Value::Int64(n), Value::Float64(_)) => (Value::Float64(*n as f64), b),
        (Value::Float64(_), Value::Int64(n)) => (a, Value::Float64(*n as f64)),
        _ => (a, b),
    }
}

/// Converts a Literal to a Value.
fn literal_to_value(literal: &Literal) -> Value {
    match literal {
        Literal::Int64(n) => Value::Int64(*n),
        Literal::String(s) => Value::String(s.clone()),
        Literal::Float64(f) => Value::Float64(*f),
        Literal::Bool(b) => Value::Bool(*b),
    }
}

/// Converts a Literal to a Value (owned version).
fn literal_into_value(literal: Literal) -> Value {
    match literal {
        Literal::Int64(n) => Value::Int64(n),
        Literal::String(s) => Value::String(s),
        Literal::Float64(f) => Value::Float64(f),
        Literal::Bool(b) => Value::Bool(b),
    }
}

/// Test helper: exposes write_multi_page for integration tests.
#[doc(hidden)]
pub fn write_multi_page_test(
    buffer_pool: &BufferPool,
    range: PageRange,
    data: &[u8],
) -> Result<()> {
    write_multi_page(buffer_pool, range, data)
}

/// Test helper: exposes read_multi_page for integration tests.
#[doc(hidden)]
pub fn read_multi_page_test(
    buffer_pool: &BufferPool,
    range: PageRange,
) -> Result<Vec<u8>> {
    read_multi_page(buffer_pool, range)
}

impl Drop for Database {
    fn drop(&mut self) {
        // Attempt to close gracefully, ignore errors during drop
        let _ = self.close();
    }
}
