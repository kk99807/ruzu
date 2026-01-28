# Research: KuzuDB Storage Internals for Rust Port

**Feature**: 002-persistent-storage
**Date**: 2025-12-06
**Status**: Complete
**References**: c:/dev/kuzu (KuzuDB C++ source), docs/feasibility-assessment.md

## Executive Summary

This document consolidates research findings from the KuzuDB C++ codebase to inform the Rust port of persistent storage components. The research covers buffer pool management, WAL, catalog persistence, relationship storage, and CSV import. All NEEDS CLARIFICATION items from the Technical Context have been resolved.

---

## 1. Buffer Manager Architecture

### Decision: 4KB Page Size with LRU Eviction

**Rationale**: KuzuDB uses 4KB pages (configurable via `KUZU_PAGE_SIZE_LOG2`). This aligns with OS page sizes and SSD block sizes for optimal I/O.

**Alternatives Considered**:
- 8KB pages: Better for sequential scans, but wastes space for small records
- Variable page sizes: Too complex for MVP

### 1.1 Page Size Configuration

**Source**: `c:/dev/kuzu/cmake/templates/system_config.h.in:30-35`

```cpp
// Default page sizes
#define KUZU_PAGE_SIZE_LOG2 12        // 4KB = 2^12
#define TEMP_PAGE_SIZE_LOG2 18        // 256KB = 2^18
```

**Rust Implementation**:
```rust
pub const PAGE_SIZE_LOG2: u32 = 12;
pub const PAGE_SIZE: usize = 1 << PAGE_SIZE_LOG2;  // 4096 bytes
pub const TEMP_PAGE_SIZE_LOG2: u32 = 18;
pub const TEMP_PAGE_SIZE: usize = 1 << TEMP_PAGE_SIZE_LOG2;  // 262144 bytes
```

### 1.2 Page State Machine

**Source**: `c:/dev/kuzu/src/include/storage/buffer_manager/page_state.h:24-114`

The C++ implementation uses a 64-bit atomic integer encoding:
```
[dirty:1][state:8][version:55]
```

| State | Value | Description | Transitions |
|-------|-------|-------------|-------------|
| EVICTED | 3 | Page not in memory | pin() → LOCKED |
| LOCKED | 1 | Exclusive access, can modify | unpin() → MARKED |
| MARKED | 2 | In eviction queue | oRead() → UNLOCKED, evict() → EVICTED |
| UNLOCKED | 0 | Optimistically readable | oUnlock() → MARKED |

**Key Insight**: The version field allows optimistic reads without locking. Readers check version before and after reading; if changed, retry.

**Rust Implementation Strategy**:
```rust
use std::sync::atomic::{AtomicU64, Ordering};

const DIRTY_MASK: u64 = 0x0080_0000_0000_0000;
const STATE_MASK: u64 = 0xFF00_0000_0000_0000;
const VERSION_MASK: u64 = 0x00FF_FFFF_FFFF_FFFF;

#[repr(u8)]
pub enum PageStateValue {
    Unlocked = 0,
    Locked = 1,
    Marked = 2,
    Evicted = 3,
}

pub struct PageState {
    state: AtomicU64,
}
```

### 1.3 Eviction Policy

**Source**: `c:/dev/kuzu/src/storage/buffer_manager/buffer_manager.cpp:37-64`

KuzuDB uses **second-chance (clock) eviction**:
1. Pages added to eviction queue when unpinned
2. First pass: MARKED pages evicted immediately
3. Second pass: UNLOCKED pages transitioned to MARKED, given second chance
4. Batch processing: 64 candidates per eviction round

**Rust Simplification for MVP**: Use **LRU** (simpler, correct, upgrade to clock in Phase 4 if needed):
```rust
use std::collections::VecDeque;

pub struct LruEvictionQueue {
    queue: VecDeque<PageId>,
    capacity: usize,
}
```

### 1.4 Memory-Mapped I/O

**Source**: `c:/dev/kuzu/src/include/storage/buffer_manager/vm_region.h:11-50`

KuzuDB maps database files using `mmap` with `MADV_DONTNEED` for explicit page eviction.

**Rust Implementation**:
```rust
use memmap2::MmapMut;
use std::fs::OpenOptions;

pub struct VmRegion {
    mmap: MmapMut,
    frame_size: usize,
    num_frames: usize,
}

impl VmRegion {
    pub fn new(path: &Path, size: usize) -> std::io::Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(path)?;
        file.set_len(size as u64)?;

        // SAFETY: File is exclusively owned, no concurrent access from other processes
        let mmap = unsafe { MmapMut::map_mut(&file)? };

        Ok(Self { mmap, frame_size: PAGE_SIZE, num_frames: size / PAGE_SIZE })
    }

    pub fn get_frame(&self, idx: usize) -> &[u8] {
        let start = idx * self.frame_size;
        &self.mmap[start..start + self.frame_size]
    }

    pub fn get_frame_mut(&mut self, idx: usize) -> &mut [u8] {
        let start = idx * self.frame_size;
        &mut self.mmap[start..start + self.frame_size]
    }
}
```

---

## 2. Write-Ahead Logging (WAL)

### Decision: Binary WAL with Optional Checksums

**Rationale**: Matches KuzuDB format for potential future compatibility. Binary format is faster than text.

### 2.1 WAL Record Format

**Source**: `c:/dev/kuzu/src/include/storage/wal/wal_record.h:21-42`

```
WALFile {
  WALHeader {
    magic: [u8; 8]         // "KUZUWAL\0" or similar
    version: u32           // Storage format version
    database_id: [u8; 16]  // UUID
    enable_checksums: bool
  }
  [WALRecord]* {
    record_type: u8
    payload_length: u32
    payload: [u8; payload_length]
    checksum: u32          // CRC32, optional
  }
}
```

**WAL Record Types** (subset for MVP):
| Type | Value | Payload |
|------|-------|---------|
| BEGIN_TRANSACTION | 1 | transaction_id: u64 |
| COMMIT | 2 | transaction_id: u64 |
| TABLE_INSERTION | 30 | table_id, num_rows, column_data[] |
| NODE_DELETION | 31 | table_id, node_offset, pk_value |
| NODE_UPDATE | 32 | table_id, column_id, node_offset, new_value |
| REL_INSERTION | 36 | table_id, src_id, dst_id, properties[] |
| REL_DELETION | 33 | table_id, src_id, dst_id, rel_id |
| CHECKPOINT | 254 | checkpoint_id: u64 |

**Rust Implementation**:
```rust
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
pub enum WalRecordType {
    BeginTransaction = 1,
    Commit = 2,
    TableInsertion = 30,
    NodeDeletion = 31,
    NodeUpdate = 32,
    RelInsertion = 36,
    RelDeletion = 33,
    Checkpoint = 254,
}

#[derive(Serialize, Deserialize)]
pub struct WalRecord {
    pub record_type: WalRecordType,
    pub transaction_id: u64,
    pub payload: WalPayload,
}
```

### 2.2 WAL Replay on Startup

**Source**: `c:/dev/kuzu/src/storage/wal/wal_replayer.cpp:84-120`

**Algorithm**:
1. Check if WAL file exists
2. If no WAL: load checkpointed data, done
3. If WAL exists:
   a. Validate header (magic bytes, version, database ID)
   b. Read records sequentially
   c. For each record: verify checksum (if enabled), apply to in-memory state
   d. Track committed transactions
   e. Rollback uncommitted transactions (discard their changes)
   f. If successful: delete WAL and shadow files

**Error Handling**:
- Corrupted checksum: truncate WAL to last good record, warn user
- Incomplete record: treat as crash during write, truncate

### 2.3 Checkpointing

**Source**: `c:/dev/kuzu/src/storage/checkpointer.cpp:27-77`

**Checkpoint Process**:
1. Acquire exclusive lock (single-writer model)
2. Serialize catalog to shadow pages (in-memory buffer)
3. Serialize metadata (page allocator state, column metadata)
4. Write DatabaseHeader to page 0 with catalog/metadata page ranges
5. Append CHECKPOINT record to WAL
6. Flush all dirty pages to disk
7. Apply shadow pages (copy to main file)
8. Clear WAL file

**Rust Strategy**: Use fsync after each step for durability.

### 2.4 Integrity Mechanisms

**Checksums**: CRC32 per WAL record (optional, enabled by default)
**Database ID**: UUID prevents mixing WAL from different databases
**Magic Bytes**: File signature validates format
**Version Number**: Prevents reading incompatible formats

---

## 3. Catalog Persistence

### Decision: Serde + Bincode Serialization

**Rationale**: Rust-native approach using serde is safer and more maintainable than raw binary format. Bincode provides compact binary representation.

### 3.1 Catalog Storage Layout

**Source**: `c:/dev/kuzu/src/include/storage/database_header.h:12-30`

```
DatabaseHeader (Page 0) {
  magic: [u8; 8]
  version: u32
  database_id: [u8; 16]
  catalog_page_range: PageRange { start: u32, count: u32 }
  metadata_page_range: PageRange { start: u32, count: u32 }
}
```

Catalog is serialized to reserved pages starting after header page.

### 3.2 Catalog Entry Types

**Source**: `c:/dev/kuzu/src/include/storage/wal/wal_record.h:92-146`

For MVP:
- NodeTableCatalogEntry: table_id, name, columns[], primary_key
- RelTableCatalogEntry: table_id, name, src_table, dst_table, columns[], direction

Deferred:
- IndexCatalogEntry
- SequenceCatalogEntry
- FunctionCatalogEntry

**Rust Implementation**:
```rust
#[derive(Serialize, Deserialize)]
pub struct CatalogEntry {
    pub entry_type: CatalogEntryType,
    pub table_id: u32,
    pub name: String,
    pub columns: Vec<ColumnDef>,
}

#[derive(Serialize, Deserialize)]
pub enum CatalogEntryType {
    NodeTable { primary_key: String },
    RelTable { src_table: String, dst_table: String, direction: Direction },
}
```

---

## 4. Relationship/Edge Storage

### Decision: CSR (Compressed Sparse Row) Format

**Rationale**: KuzuDB uses CSR for efficient adjacency list storage. This is optimal for graph traversals.

### 4.1 CSR Data Structure

**Source**: `c:/dev/kuzu/src/include/storage/table/csr_node_group.h:21-77`

```
CSR Node Group {
  offsets: [u64; NUM_NODES + 1]   // Start offset for each node's edges
  neighbors: [u64; NUM_EDGES]     // Destination node IDs
  properties: [Column; NUM_PROPS] // Edge properties, parallel to neighbors
}
```

**Example**:
```
Node 0 has edges to [1, 3]
Node 1 has edges to [2]
Node 2 has edges to [0, 1, 3]

offsets = [0, 2, 3, 6]
neighbors = [1, 3, 2, 0, 1, 3]
```

### 4.2 Forward/Backward Adjacency

**Source**: `c:/dev/kuzu/src/include/storage/table/rel_table_data.h:56-87`

Relationships are stored with both forward and backward CSR indices:
- **Forward**: src_node → [dst_nodes]
- **Backward**: dst_node → [src_nodes]

This enables efficient traversal in both directions.

### 4.3 Node Group Size

**Source**: `c:/dev/kuzu/cmake/templates/system_config.h.in:46-60`

```cpp
NODE_GROUP_SIZE_LOG2 = 17  // 2^17 = 131,072 nodes per group
```

Large datasets are partitioned into node groups for parallel processing and memory management.

### 4.4 Rust Implementation

```rust
pub struct CsrNodeGroup {
    offsets: Vec<u64>,     // length = num_nodes + 1
    neighbors: Vec<u64>,   // destination node IDs
    properties: Vec<ColumnStorage>,  // edge properties
}

pub struct RelTableData {
    table_id: u32,
    src_table_id: u32,
    dst_table_id: u32,
    direction: Direction,
    forward_csr: CsrNodeGroup,
    backward_csr: CsrNodeGroup,
    columns: Vec<ColumnDef>,
}

impl CsrNodeGroup {
    pub fn get_neighbors(&self, node_id: u64) -> &[u64] {
        let start = self.offsets[node_id as usize] as usize;
        let end = self.offsets[node_id as usize + 1] as usize;
        &self.neighbors[start..end]
    }
}
```

---

## 5. Bulk CSV Import

### Decision: Parallel CSV Parsing with Progress Reporting

**Rationale**: Real-world datasets are large. Parallel parsing with progress reporting provides good UX.

### 5.1 CSV Parsing Configuration

**Source**: `c:/dev/kuzu/src/include/common/constants.h:95-156`

| Option | Default | Description |
|--------|---------|-------------|
| DELIMITER | ',' | Field separator |
| QUOTE | '"' | Quote character |
| ESCAPE | '"' | Escape character |
| HEADER | true | First row is header |
| SKIP | 0 | Rows to skip |
| PARALLEL | true | Enable parallel parsing |
| IGNORE_ERRORS | false | Continue on parse errors |

### 5.2 Node Bulk Insert

**Source**: `c:/dev/kuzu/src/processor/operator/persistent/node_batch_insert.cpp`

**Algorithm**:
1. Parse CSV block → DataChunk (2048 rows)
2. Validate against schema
3. Allocate node offsets from table metadata
4. Write to columnar storage
5. Log TABLE_INSERTION record to WAL
6. Report progress (rows processed)

### 5.3 Relationship Bulk Insert

**Source**: `c:/dev/kuzu/src/include/processor/operator/persistent/copy_rel_batch_insert.h:13-48`

**Algorithm**:
1. Parse CSV block with FROM, TO, property columns
2. Validate node references exist
3. Build in-memory CSR structure
4. Partition by source node (for forward index)
5. Partition by dest node (for backward index)
6. Write CSR to disk
7. Log REL_INSERTION records

### 5.4 Progress Reporting

Emit progress events during bulk operations:
```rust
pub struct ImportProgress {
    pub rows_processed: u64,
    pub rows_total: Option<u64>,  // None if file size unknown
    pub errors: Vec<ImportError>,
}
```

### 5.5 Error Handling Modes

| Mode | Behavior |
|------|----------|
| Atomic | Rollback all on any error |
| ContinueOnError | Log errors, continue importing valid rows |

---

## 6. Rust Crate Dependencies

### Core Dependencies (add to Cargo.toml)

```toml
[dependencies]
# Existing
pest = "2.7"
pest_derive = "2.7"
thiserror = "1.0"

# New for Phase 1
memmap2 = "0.9"           # Memory-mapped I/O
parking_lot = "0.12"      # Faster Mutex/RwLock
serde = { version = "1.0", features = ["derive"] }
bincode = "1.3"           # Binary serialization
csv = "1.3"               # CSV parsing
crc32fast = "1.4"         # CRC32 checksums
uuid = { version = "1.6", features = ["v4", "serde"] }  # Database IDs

[dev-dependencies]
# Existing
criterion = "0.5"
clap = { version = "4.5", features = ["derive"] }
fake = "2.9"
rand = "0.8"

# New for Phase 1
proptest = "1.4"          # Property-based testing
tempfile = "3.10"         # Temporary directories for tests
```

---

## 7. Open Questions (Resolved)

| Question | Resolution |
|----------|------------|
| Page size for MVP? | 4KB (matches KuzuDB and OS page size) |
| Eviction policy? | LRU for MVP, upgrade to clock in Phase 4 |
| WAL checksum algorithm? | CRC32 (fast, sufficient for error detection) |
| Catalog serialization? | serde + bincode (Rust-native, safe) |
| CSR vs adjacency list? | CSR (matches KuzuDB, optimal for traversals) |
| Parallel CSV parsing? | Yes, with configurable thread count |

---

## 8. Risk Mitigations

### Risk 1: mmap requires unsafe code

**Mitigation**:
- Isolate all unsafe code to `src/storage/buffer_pool/vm_region.rs`
- Add comprehensive SAFETY comments
- Test with Miri (memory sanitizer)
- Property-based tests for buffer pool invariants

### Risk 2: WAL replay correctness

**Mitigation**:
- Comprehensive integration tests for crash scenarios
- Simulate crashes at each step of checkpoint
- Verify data integrity after replay
- Use proptest to generate random operation sequences

### Risk 3: CSR format complexity

**Mitigation**:
- Start with simple in-memory CSR, add persistence incrementally
- Port exact algorithm from KuzuDB
- Extensive tests for edge insertion/deletion ordering

---

## References

1. KuzuDB Buffer Manager: `c:/dev/kuzu/src/include/storage/buffer_manager/buffer_manager.h`
2. KuzuDB WAL Records: `c:/dev/kuzu/src/include/storage/wal/wal_record.h`
3. KuzuDB CSR Storage: `c:/dev/kuzu/src/include/storage/table/csr_node_group.h`
4. KuzuDB CSV Import: `c:/dev/kuzu/src/processor/operator/persistent/node_batch_insert.cpp`
5. Feasibility Assessment: `docs/feasibility-assessment.md`
