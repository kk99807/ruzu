# Data Model: Multi-Page Storage

**Feature**: 007-multi-page-storage
**Date**: 2026-01-30

## Entities

### PageRange (Existing — Enhanced)

**Location**: `src/storage/mod.rs`

| Field | Type | Description |
|-------|------|-------------|
| `start_page` | `u32` | Starting page index in the database file |
| `num_pages` | `u32` | Number of contiguous pages in the range |

**Existing methods**: `new()`, `end_page()`, `is_empty()`

**New methods needed**:
- `byte_capacity(&self) -> usize` — Returns `num_pages as usize * PAGE_SIZE`
- `overlaps(&self, other: &PageRange) -> bool` — Checks if two ranges share any pages
- `contains_page(&self, page_idx: u32) -> bool` — Checks if a page index is within the range

**Validation rules**:
- `start_page > 0` (page 0 is always the header)
- `num_pages >= 1` for active ranges, or `num_pages == 0` for empty
- No two active ranges may overlap
- `end_page() <= file_page_count` (within file bounds)

---

### DatabaseHeader (Existing — Version Bump)

**Location**: `src/storage/mod.rs`

| Field | Type | Change |
|-------|------|--------|
| `magic` | `[u8; 8]` | No change |
| `version` | `u32` | 2 → 3 |
| `database_id` | `Uuid` | No change |
| `catalog_range` | `PageRange` | Now supports `num_pages > 1` |
| `metadata_range` | `PageRange` | Now supports `num_pages > 1` |
| `rel_metadata_range` | `PageRange` | Now supports `num_pages > 1` |
| `checksum` | `u32` | No change |

**State transitions**:
- **New database**: Ranges start empty (0, 0), allocated on first save
- **V2 database opened**: Header read as v2, ranges are (1,1), (2,1), (3,1) — single pages
- **After first save**: Ranges dynamically allocated to fit actual data size
- **Subsequent saves**: New ranges allocated, header updated

**Version migration**:
- V1 → V2: Existing (adds `rel_metadata_range`)
- V2 → V3: New. V2 headers have fixed single-page ranges. On open, they work as-is since `num_pages=1` is a valid multi-page range. The version bump signals that ranges may now be > 1 page. On first save, ranges are dynamically re-allocated.

---

### Multi-Page Data Format

The on-disk format for each metadata section (catalog, node data, rel data):

```
┌─────────────────────────────────────────────────────┐
│ Page N (first page in range)                         │
│ ┌──────────┬────────────────────────────────────────┐│
│ │ 4 bytes  │ Remaining bytes of page                ││
│ │ length   │ (start of serialized data)             ││
│ │ (u32 LE) │                                         ││
│ └──────────┴────────────────────────────────────────┘│
├─────────────────────────────────────────────────────┤
│ Page N+1 (second page, if needed)                    │
│ ┌──────────────────────────────────────────────────┐│
│ │ Full 4096 bytes of serialized data (continued)   ││
│ └──────────────────────────────────────────────────┘│
├─────────────────────────────────────────────────────┤
│ ...more pages as needed...                           │
├─────────────────────────────────────────────────────┤
│ Page N+K (last page in range)                        │
│ ┌──────────────────────────────────────────────────┐│
│ │ Remaining data bytes + unused padding (zeroed)   ││
│ └──────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────┘
```

- **Length prefix**: First 4 bytes of the first page, `u32` little-endian, total serialized data length (excluding the 4-byte prefix itself)
- **Data**: Serialized bytes spanning across pages contiguously
- **Padding**: Unused bytes in the last page are zeroed (ignored on read)
- **Pages needed**: `ceil((data_len + 4) / PAGE_SIZE)` — the +4 accounts for the length prefix

**Backward compatibility**: When `num_pages = 1`, this format is identical to the current single-page format.

---

### PageAllocator (New — Conceptual)

This is not a separate struct but a pattern implemented in `DiskManager` and used by `save_all_data()`:

**Allocation flow on save**:
1. Reset allocator cursor to page 1 (first page after header)
2. Serialize catalog → calculate pages needed → allocate range starting at cursor → advance cursor
3. Serialize node data → calculate pages needed → allocate range starting at cursor → advance cursor
4. Serialize rel data → calculate pages needed → allocate range starting at cursor → advance cursor
5. Update header with new ranges

**Note**: This is a simple sequential allocator. Pages from previous saves are not reclaimed — the file grows monotonically. Free-space management is out of scope for this interim feature.

---

## Relationships

```
DatabaseHeader (page 0)
    ├── catalog_range ──→ [Page 1..N] → Catalog (schemas)
    ├── metadata_range ──→ [Page M..M+K] → Node table data
    └── rel_metadata_range ──→ [Page P..P+J] → Rel table data
```

All three metadata sections are independently sized and located. The header is the single root that anchors all metadata locations.

## File Layout Example

**Small database** (all metadata fits in 1 page each):
```
Page 0: Header (catalog_range={1,1}, metadata_range={2,1}, rel_metadata_range={3,1})
Page 1: Catalog data
Page 2: Node table data
Page 3: Rel table data
```

**Growing database** (node data exceeds 1 page):
```
Page 0: Header (catalog_range={1,1}, metadata_range={2,3}, rel_metadata_range={5,1})
Page 1: Catalog data
Page 2: Node table data (first page, with length prefix)
Page 3: Node table data (continued)
Page 4: Node table data (continued)
Page 5: Rel table data
```

**Large database** (all sections multi-page):
```
Page 0: Header (catalog_range={1,2}, metadata_range={3,10}, rel_metadata_range={13,5})
Page 1-2: Catalog data
Page 3-12: Node table data
Page 13-17: Rel table data
```
