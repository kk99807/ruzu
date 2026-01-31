# Storage API Contracts: Multi-Page Storage

**Feature**: 007-multi-page-storage
**Date**: 2026-01-30

## Internal API Contracts

These are internal Rust module APIs, not external HTTP/RPC endpoints. Contracts define the function signatures, pre/post-conditions, and error behavior.

---

### Contract 1: `DiskManager::allocate_page_range`

**Location**: `src/storage/page/disk_manager.rs`

```rust
/// Allocates a contiguous range of pages in the database file.
///
/// Extends the file to accommodate `num_pages` new pages and returns
/// a PageRange identifying the allocated region.
///
/// # Pre-conditions
/// - `num_pages > 0`
///
/// # Post-conditions
/// - File size increased by `num_pages * PAGE_SIZE` bytes
/// - Returned PageRange has `num_pages` contiguous pages
/// - Pages are zeroed (OS guarantee for file extension)
///
/// # Errors
/// - `StorageError` if file extension fails (e.g., disk full)
pub fn allocate_page_range(&mut self, num_pages: u32) -> Result<PageRange>
```

---

### Contract 2: `write_multi_page`

**Location**: `src/lib.rs` (helper function)

```rust
/// Writes serialized data across a contiguous page range using the buffer pool.
///
/// Format: [4-byte length prefix (u32 LE)] [data bytes spanning pages]
///
/// # Pre-conditions
/// - `range.num_pages >= ceil((data.len() + 4) / PAGE_SIZE)`
/// - `buffer_pool` is available
///
/// # Post-conditions
/// - All pages in range are written with data
/// - First 4 bytes contain `data.len()` as u32 LE
/// - Data bytes follow contiguously across pages
/// - Unused bytes in last page are zeroed
///
/// # Errors
/// - `StorageError` if data exceeds range capacity
/// - `StorageError` if buffer pool pin fails
fn write_multi_page(
    buffer_pool: &BufferPool,
    range: &PageRange,
    data: &[u8],
) -> Result<()>
```

---

### Contract 3: `read_multi_page`

**Location**: `src/lib.rs` (helper function)

```rust
/// Reads serialized data from a contiguous page range using the buffer pool.
///
/// Reads the 4-byte length prefix from the first page, then assembles
/// the data bytes from all pages in the range.
///
/// # Pre-conditions
/// - `range.num_pages > 0`
/// - Pages in range exist and are readable
///
/// # Post-conditions
/// - Returns exactly `length` bytes as specified by the length prefix
/// - Data is correctly reassembled from multiple pages
///
/// # Errors
/// - `StorageError` if range is empty
/// - `StorageError` if length prefix exceeds range capacity
/// - `StorageError` if buffer pool pin fails
fn read_multi_page(
    buffer_pool: &BufferPool,
    range: &PageRange,
) -> Result<Vec<u8>>
```

---

### Contract 4: `PageRange::overlaps`

**Location**: `src/storage/mod.rs`

```rust
/// Returns true if this range shares any pages with `other`.
///
/// Empty ranges (num_pages == 0) never overlap with anything.
///
/// # Examples
/// - PageRange(1,3) overlaps PageRange(3,2) → true (page 3 shared)
/// - PageRange(1,2) overlaps PageRange(3,2) → false (pages 1-2 vs 3-4)
/// - PageRange(0,0) overlaps PageRange(1,1) → false (empty range)
pub fn overlaps(&self, other: &PageRange) -> bool
```

---

### Contract 5: `DatabaseHeader::validate_ranges`

**Location**: `src/storage/mod.rs`

```rust
/// Validates that all page ranges in the header are consistent.
///
/// Checks:
/// 1. No range overlaps with page 0 (header page)
/// 2. No two ranges overlap with each other
/// 3. All range start pages are > 0 (for non-empty ranges)
///
/// # Errors
/// - `StorageError` if any validation check fails, with description of which
///   ranges conflict
pub fn validate_ranges(&self) -> Result<()>
```

---

### Contract 6: `save_all_data` (Modified)

**Location**: `src/lib.rs`

```rust
/// Saves all data (catalog, node tables, rel tables) to disk using
/// multi-page allocation.
///
/// # Behavior change from v2:
/// - Previously: wrote to fixed pages (1, 2, 3)
/// - Now: allocates page ranges dynamically based on data size
/// - Updates header page ranges to reflect new allocation
///
/// # Flow:
/// 1. Serialize catalog → allocate pages → write
/// 2. Serialize node data → allocate pages → write
/// 3. Serialize rel data → allocate pages → write
/// 4. Update self.header with new ranges
///
/// # Pre-conditions
/// - Database is in disk-backed mode (buffer_pool and header exist)
///
/// # Post-conditions
/// - All metadata written to allocated page ranges
/// - Header ranges updated (but header NOT yet written to disk)
/// - Buffer pool pages are dirty (not yet flushed)
///
/// # Errors
/// - `StorageError` for serialization, allocation, or write failures
fn save_all_data(&mut self) -> Result<()>
```

**Note**: `save_all_data` signature changes from `&self` to `&mut self` because it now updates `self.header` with new page ranges.

---

### Contract 7: `Database::open` (Modified — v2→v3 Migration)

**Location**: `src/lib.rs`

```rust
/// Opens a database, auto-migrating v2 format to v3 on first save.
///
/// # V2 migration behavior:
/// - V2 header is read successfully (fixed pages 1, 2, 3)
/// - Data is loaded from those fixed pages (single-page read path)
/// - `was_migrated` flag is set to true
/// - On first checkpoint/close, data is re-saved using multi-page allocation
/// - Header is updated to version 3 with new dynamic ranges
///
/// # Post-conditions (for v2 migration):
/// - All v2 data is loaded correctly
/// - `dirty` flag is set (triggers save on close)
/// - After save, database file uses v3 format
```

---

### Contract 8: `calculate_pages_needed`

**Location**: `src/lib.rs` or `src/storage/mod.rs` (utility function)

```rust
/// Calculates the number of pages needed to store data with a 4-byte length prefix.
///
/// Formula: ceil((data_len + 4) / PAGE_SIZE)
/// Minimum: 1 page (even for empty data, to store the length prefix)
///
/// # Examples
/// - 0 bytes → 1 page
/// - 4092 bytes → 1 page (4092 + 4 = 4096 = exactly 1 page)
/// - 4093 bytes → 2 pages (4093 + 4 = 4097 > 4096)
/// - 8188 bytes → 2 pages (8188 + 4 = 8192 = exactly 2 pages)
pub fn calculate_pages_needed(data_len: usize) -> u32
```

---

## Format Stability Contract

### On-disk format (v3)

The multi-page format is backward-compatible with v2 when `num_pages = 1`:

```
Version 3 Header (page 0):
  magic: b"RUZUDB\0\0" (8 bytes)
  version: 3 (u32)
  database_id: UUID (16 bytes)
  catalog_range: PageRange { start_page: u32, num_pages: u32 }
  metadata_range: PageRange { start_page: u32, num_pages: u32 }
  rel_metadata_range: PageRange { start_page: u32, num_pages: u32 }
  checksum: u32 (CRC32)
```

The `DatabaseHeader` bincode serialization is unchanged — `PageRange` already has two `u32` fields. The version number is the only structural difference.

### Data section format (unchanged)

Each metadata section, whether single-page or multi-page:
```
Byte 0-3: data_length (u32 LE) — length of serialized payload
Byte 4..4+data_length: bincode-serialized payload
Byte 4+data_length..end: zero padding (ignored)
```

The payload types remain:
- Catalog: `bincode(Catalog)`
- Node data: `bincode(HashMap<String, TableData>)`
- Rel data: `bincode(HashMap<String, RelTableData>)`
