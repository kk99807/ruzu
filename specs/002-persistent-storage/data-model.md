# Data Model: Persistent Storage with Edge Support

**Feature**: 002-persistent-storage
**Date**: 2025-12-06
**Source**: [spec.md](./spec.md), [research.md](./research.md)

## Overview

This document defines the core entities, their relationships, validation rules, and state transitions for the persistent storage feature. The model is derived from functional requirements in the spec and informed by KuzuDB's C++ implementation.

---

## Core Entities

### 1. Page

**Description**: Fixed-size block of data (4KB), the unit of I/O between disk and memory.

**Fields**:
| Field | Type | Constraints | Description |
|-------|------|-------------|-------------|
| page_id | `PageId` | Unique within file | Global page identifier |
| data | `[u8; 4096]` | Fixed size | Raw page content |
| checksum | `u32` | Optional | CRC32 for integrity |

**Relationships**:
- Belongs to one `BufferFrame` when pinned in memory
- Referenced by `WalRecord` for modifications

**Rust Definition**:
```rust
pub const PAGE_SIZE: usize = 4096;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct PageId {
    pub file_id: u32,   // Which database file
    pub page_idx: u32,  // Offset within file (page_idx * PAGE_SIZE)
}

pub struct Page {
    pub id: PageId,
    pub data: [u8; PAGE_SIZE],
}
```

---

### 2. BufferFrame

**Description**: A memory slot in the buffer pool that holds one page. Tracks pin count, dirty flag, and eviction metadata.

**Fields**:
| Field | Type | Constraints | Description |
|-------|------|-------------|-------------|
| frame_id | `u32` | Unique in pool | Index in buffer pool array |
| page_id | `Option<PageId>` | None if empty | Currently loaded page |
| state | `PageState` | Atomic | EVICTED/LOCKED/MARKED/UNLOCKED |
| pin_count | `u32` | >= 0 | Number of active references |
| dirty | `bool` | Atomic flag | Modified since last flush |
| last_access | `u64` | For LRU | Access timestamp or counter |

**State Transitions**:
```
EVICTED ──[pin()]──> LOCKED ──[unpin()]──> MARKED ──[access()]──> UNLOCKED
    ^                                          │                      │
    └─────────────────────[evict()]────────────┴──────────────────────┘
```

**Validation Rules**:
- Cannot evict if `pin_count > 0`
- Must flush dirty pages before eviction
- State transitions are atomic (no partial updates)

**Rust Definition**:
```rust
use std::sync::atomic::{AtomicU64, AtomicBool, Ordering};

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PageStateValue {
    Unlocked = 0,
    Locked = 1,
    Marked = 2,
    Evicted = 3,
}

pub struct PageState {
    // [dirty:1][state:8][version:55]
    state: AtomicU64,
}

pub struct BufferFrame {
    pub frame_id: u32,
    pub page_id: Option<PageId>,
    pub state: PageState,
    pub pin_count: AtomicU32,
    pub dirty: AtomicBool,
    pub last_access: AtomicU64,
}
```

---

### 3. BufferPool

**Description**: Manages buffer frames and coordinates page eviction.

**Fields**:
| Field | Type | Constraints | Description |
|-------|------|-------------|-------------|
| frames | `Vec<BufferFrame>` | Fixed capacity | All buffer frames |
| page_table | `HashMap<PageId, u32>` | Concurrent | Maps page → frame |
| eviction_queue | `VecDeque<u32>` | LRU order | Frame IDs for eviction |
| capacity | `usize` | > 0 | Max pages in memory |
| disk_manager | `DiskManager` | Required | File I/O handle |

**Validation Rules**:
- `frames.len() == capacity`
- All frames start in EVICTED state
- Page table size <= capacity

**Rust Definition**:
```rust
use parking_lot::RwLock;
use std::collections::{HashMap, VecDeque};

pub struct BufferPool {
    frames: Vec<RwLock<BufferFrame>>,
    page_table: RwLock<HashMap<PageId, u32>>,
    eviction_queue: RwLock<VecDeque<u32>>,
    capacity: usize,
    disk_manager: DiskManager,
    access_counter: AtomicU64,
}
```

---

### 4. WalRecord

**Description**: A log entry describing a single modification for crash recovery.

**Fields**:
| Field | Type | Constraints | Description |
|-------|------|-------------|-------------|
| record_type | `WalRecordType` | Valid enum | Operation type |
| transaction_id | `u64` | > 0 | Owning transaction |
| lsn | `u64` | Monotonic | Log Sequence Number |
| payload | `WalPayload` | Type-specific | Operation data |
| checksum | `u32` | Optional | CRC32 of record |

**Record Types (MVP)**:
| Type | Payload |
|------|---------|
| BeginTransaction | `{ tx_id: u64 }` |
| Commit | `{ tx_id: u64 }` |
| TableInsertion | `{ table_id: u32, rows: Vec<Row> }` |
| NodeDeletion | `{ table_id: u32, node_offset: u64, pk: Value }` |
| NodeUpdate | `{ table_id: u32, col_id: u32, offset: u64, value: Value }` |
| RelInsertion | `{ table_id: u32, src: u64, dst: u64, props: Vec<Value> }` |
| RelDeletion | `{ table_id: u32, src: u64, dst: u64, rel_id: u64 }` |
| Checkpoint | `{ checkpoint_id: u64 }` |

**Rust Definition**:
```rust
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
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
pub enum WalPayload {
    BeginTransaction { tx_id: u64 },
    Commit { tx_id: u64 },
    TableInsertion { table_id: u32, rows: Vec<Vec<Value>> },
    NodeDeletion { table_id: u32, node_offset: u64, pk: Value },
    NodeUpdate { table_id: u32, col_id: u32, offset: u64, value: Value },
    RelInsertion { table_id: u32, src: u64, dst: u64, props: Vec<Value> },
    RelDeletion { table_id: u32, src: u64, dst: u64, rel_id: u64 },
    Checkpoint { checkpoint_id: u64 },
}

#[derive(Serialize, Deserialize)]
pub struct WalRecord {
    pub record_type: WalRecordType,
    pub transaction_id: u64,
    pub lsn: u64,
    pub payload: WalPayload,
}
```

---

### 5. RelationshipTableSchema

**Description**: Defines a relationship type including source/destination node tables and properties.

**Fields**:
| Field | Type | Constraints | Description |
|-------|------|-------------|-------------|
| table_id | `u32` | Unique | Internal identifier |
| name | `String` | Non-empty, unique | Relationship type name |
| src_table | `String` | Must exist | Source node table name |
| dst_table | `String` | Must exist | Destination node table name |
| columns | `Vec<ColumnDef>` | Valid types | Relationship properties |
| direction | `Direction` | FWD/BWD/BOTH | Storage direction |

**Validation Rules**:
- `name` must be unique among all relationship tables
- `src_table` and `dst_table` must reference existing node tables
- Column names must be unique within the table
- Cannot have a primary key (relationships identified by src+dst+rel_id)

**Rust Definition**:
```rust
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    Forward,   // Only store forward adjacency (src → dst)
    Backward,  // Only store backward adjacency (dst → src)
    Both,      // Store both directions (default)
}

#[derive(Clone, Serialize, Deserialize)]
pub struct RelTableSchema {
    pub table_id: u32,
    pub name: String,
    pub src_table: String,
    pub dst_table: String,
    pub columns: Vec<ColumnDef>,
    pub direction: Direction,
}
```

---

### 6. Relationship (Edge Instance)

**Description**: An edge connecting two nodes with optional properties.

**Fields**:
| Field | Type | Constraints | Description |
|-------|------|-------------|-------------|
| rel_id | `u64` | Unique within table | Internal identifier |
| src_node_id | `u64` | Must exist | Source node offset |
| dst_node_id | `u64` | Must exist | Destination node offset |
| properties | `Vec<Value>` | Match schema | Relationship properties |

**Validation Rules**:
- `src_node_id` must reference existing node in source table
- `dst_node_id` must reference existing node in destination table
- Properties must match schema column types

**Storage**: Stored in CSR format (see CsrNodeGroup)

---

### 7. CsrNodeGroup

**Description**: Compressed Sparse Row storage for relationships in a node group.

**Fields**:
| Field | Type | Constraints | Description |
|-------|------|-------------|-------------|
| group_id | `u32` | Unique | Node group identifier |
| num_nodes | `u32` | <= 131072 | Nodes in this group |
| offsets | `Vec<u64>` | len = num_nodes + 1 | Edge start offsets |
| neighbors | `Vec<u64>` | Sorted per node | Destination node IDs |
| rel_ids | `Vec<u64>` | Parallel to neighbors | Relationship IDs |
| properties | `Vec<ColumnStorage>` | Parallel to neighbors | Edge properties |

**Invariants**:
- `offsets[0] == 0`
- `offsets[num_nodes] == neighbors.len()`
- `offsets[i] <= offsets[i+1]` for all i
- `rel_ids.len() == neighbors.len()`

**Rust Definition**:
```rust
pub const NODE_GROUP_SIZE: usize = 131072;  // 2^17

pub struct CsrNodeGroup {
    pub group_id: u32,
    pub num_nodes: u32,
    pub offsets: Vec<u64>,
    pub neighbors: Vec<u64>,
    pub rel_ids: Vec<u64>,
    pub properties: Vec<ColumnStorage>,
}

impl CsrNodeGroup {
    pub fn get_neighbors(&self, local_node_id: u32) -> &[u64] {
        let start = self.offsets[local_node_id as usize] as usize;
        let end = self.offsets[local_node_id as usize + 1] as usize;
        &self.neighbors[start..end]
    }

    pub fn get_rel_ids(&self, local_node_id: u32) -> &[u64] {
        let start = self.offsets[local_node_id as usize] as usize;
        let end = self.offsets[local_node_id as usize + 1] as usize;
        &self.rel_ids[start..end]
    }

    pub fn num_edges(&self) -> usize {
        self.neighbors.len()
    }
}
```

---

### 8. DatabaseHeader

**Description**: Metadata stored in page 0 of the database file.

**Fields**:
| Field | Type | Constraints | Description |
|-------|------|-------------|-------------|
| magic | `[u8; 8]` | "RUZUDB\0\0" | File signature |
| version | `u32` | Current = 1 | Format version |
| database_id | `Uuid` | Random v4 | Unique database ID |
| catalog_range | `PageRange` | Valid pages | Catalog page location |
| metadata_range | `PageRange` | Valid pages | Metadata page location |
| checksum | `u32` | CRC32 | Header integrity |

**Rust Definition**:
```rust
use uuid::Uuid;

pub const MAGIC_BYTES: [u8; 8] = *b"RUZUDB\0\0";
pub const CURRENT_VERSION: u32 = 1;

#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct PageRange {
    pub start_page: u32,
    pub num_pages: u32,
}

#[derive(Serialize, Deserialize)]
pub struct DatabaseHeader {
    pub magic: [u8; 8],
    pub version: u32,
    pub database_id: Uuid,
    pub catalog_range: PageRange,
    pub metadata_range: PageRange,
}
```

---

### 9. CsvImportConfig

**Description**: Configuration for bulk CSV import operations.

**Fields**:
| Field | Type | Default | Description |
|-------|------|---------|-------------|
| delimiter | `char` | ',' | Field separator |
| quote | `char` | '"' | Quote character |
| escape | `char` | '"' | Escape character |
| has_header | `bool` | true | First row is header |
| skip_rows | `usize` | 0 | Rows to skip |
| parallel | `bool` | true | Enable parallel parsing |
| ignore_errors | `bool` | false | Continue on parse errors |
| batch_size | `usize` | 2048 | Rows per batch |

**Rust Definition**:
```rust
#[derive(Clone)]
pub struct CsvImportConfig {
    pub delimiter: char,
    pub quote: char,
    pub escape: char,
    pub has_header: bool,
    pub skip_rows: usize,
    pub parallel: bool,
    pub ignore_errors: bool,
    pub batch_size: usize,
}

impl Default for CsvImportConfig {
    fn default() -> Self {
        Self {
            delimiter: ',',
            quote: '"',
            escape: '"',
            has_header: true,
            skip_rows: 0,
            parallel: true,
            ignore_errors: false,
            batch_size: 2048,
        }
    }
}
```

---

### 10. ImportProgress

**Description**: Progress reporting for bulk import operations.

**Fields**:
| Field | Type | Description |
|-------|------|-------------|
| rows_processed | `u64` | Rows successfully imported |
| rows_total | `Option<u64>` | Total rows (if known) |
| rows_failed | `u64` | Rows that failed validation |
| bytes_read | `u64` | Bytes processed |
| errors | `Vec<ImportError>` | Error details |

**Rust Definition**:
```rust
#[derive(Clone)]
pub struct ImportProgress {
    pub rows_processed: u64,
    pub rows_total: Option<u64>,
    pub rows_failed: u64,
    pub bytes_read: u64,
    pub errors: Vec<ImportError>,
}

#[derive(Clone)]
pub struct ImportError {
    pub row_number: u64,
    pub column: Option<String>,
    pub message: String,
}
```

---

## Entity Relationships Diagram

```
                                  ┌──────────────┐
                                  │ DatabaseHeader│
                                  │ (page 0)     │
                                  └──────┬───────┘
                                         │ references
                         ┌───────────────┼───────────────┐
                         ▼               ▼               ▼
                  ┌──────────┐    ┌───────────┐   ┌──────────────┐
                  │ Catalog  │    │ Metadata  │   │ Data Pages   │
                  │ Pages    │    │ Pages     │   │              │
                  └────┬─────┘    └───────────┘   └──────┬───────┘
                       │                                  │
         ┌─────────────┼─────────────┐                   │
         ▼             ▼             ▼                   ▼
  ┌──────────────┐ ┌──────────────┐ ┌────────────┐ ┌──────────────┐
  │NodeTableSchema│ │RelTableSchema│ │ Sequences  │ │ BufferPool   │
  └──────┬───────┘ └──────┬───────┘ └────────────┘ └──────┬───────┘
         │                │                               │
         │                │                        ┌──────┴──────┐
         ▼                ▼                        ▼             ▼
  ┌──────────────┐ ┌──────────────┐         ┌───────────┐ ┌───────────┐
  │  NodeTable   │ │  RelTable    │         │BufferFrame│ │ PageTable │
  │  (columns)   │ │  (CSR)       │         └───────────┘ └───────────┘
  └──────────────┘ └──────┬───────┘
                          │
              ┌───────────┼───────────┐
              ▼           ▼           ▼
       ┌───────────┐ ┌───────────┐ ┌───────────┐
       │CsrNodeGroup│ │CsrNodeGroup│ │   ...     │
       │ (forward)  │ │ (backward)│ │           │
       └───────────┘ └───────────┘ └───────────┘
```

---

## State Transitions

### BufferFrame State Machine

```
                          ┌──────────────────────────┐
                          │                          │
                          ▼                          │
    ┌─────────┐       ┌────────┐       ┌────────┐   │
    │ EVICTED │──────>│ LOCKED │──────>│ MARKED │───┘
    └─────────┘  pin  └────────┘ unpin └────────┘
         ▲                                  │
         │                                  │ access
         │            ┌──────────┐          │
         └────────────│ UNLOCKED │<─────────┘
            evict     └──────────┘
```

### Transaction Lifecycle

```
    ┌─────────┐   begin   ┌──────────┐
    │  IDLE   │──────────>│  ACTIVE  │
    └─────────┘           └────┬─────┘
                               │
               ┌───────────────┼───────────────┐
               │ commit        │               │ abort
               ▼               ▼               ▼
        ┌───────────┐   ┌───────────┐   ┌───────────┐
        │ COMMITTED │   │  ABORTED  │   │  ABORTED  │
        └───────────┘   └───────────┘   └───────────┘
```

### WAL Replay State Machine

```
    ┌─────────────┐
    │   START     │
    └──────┬──────┘
           │
           ▼
    ┌─────────────┐   no WAL   ┌─────────────┐
    │ CHECK WAL   │───────────>│ LOAD CHKPT  │──> READY
    └──────┬──────┘            └─────────────┘
           │ WAL exists
           ▼
    ┌─────────────┐
    │ VALIDATE HDR│───error───> ABORT
    └──────┬──────┘
           │ valid
           ▼
    ┌─────────────┐            ┌─────────────┐
    │ READ RECORD │───error───>│ TRUNCATE    │──> READY
    └──────┬──────┘            └─────────────┘
           │ record
           ▼
    ┌─────────────┐
    │ APPLY RECORD│
    └──────┬──────┘
           │
           ├───more records──> READ RECORD
           │
           ▼ no more records
    ┌─────────────┐
    │ CLEANUP WAL │──────────> READY
    └─────────────┘
```

---

## Validation Rules Summary

| Entity | Rule | Error |
|--------|------|-------|
| Page | size == 4096 bytes | InvalidPageSize |
| BufferFrame | pin_count >= 0 | InvalidPinCount |
| BufferFrame | cannot evict if pin_count > 0 | PagePinned |
| BufferFrame | must flush dirty before evict | DirtyPageEviction |
| WalRecord | LSN monotonically increasing | InvalidLsn |
| WalRecord | checksum matches content | CorruptedRecord |
| RelTableSchema | src_table exists | InvalidSourceTable |
| RelTableSchema | dst_table exists | InvalidDestTable |
| Relationship | src_node exists | InvalidSourceNode |
| Relationship | dst_node exists | InvalidDestNode |
| CsrNodeGroup | offsets[0] == 0 | InvalidCsrOffset |
| CsrNodeGroup | offsets monotonic | InvalidCsrOffset |
| DatabaseHeader | magic == "RUZUDB\0\0" | InvalidMagic |
| DatabaseHeader | version <= CURRENT | UnsupportedVersion |
