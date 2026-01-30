# Research: Multi-Page Storage

**Feature**: 007-multi-page-storage
**Date**: 2026-01-30

## Research Tasks

### R1: Multi-Page Allocation Strategy

**Context**: How should we allocate multiple contiguous pages for metadata that exceeds a single 4KB page?

**Decision**: Append-only contiguous allocation using `DiskManager.allocate_page_range(num_pages)`.

**Rationale**:
- KuzuDB C++ uses `PageManager.allocatePageRange(numPages)` which allocates contiguous page ranges
- Our `DiskManager` already has `allocate_page()` which appends to the file — extending to `allocate_page_range(n)` is straightforward (call `allocate_page()` n times, or extend file by n*PAGE_SIZE in one call)
- Contiguous allocation is specified in the assumptions ("Page ranges are contiguous")
- No free-space management needed (out of scope per spec)

**Alternatives Considered**:
1. **Non-contiguous with page directory**: More complex, allows reuse of freed pages, but explicitly out of scope
2. **Pre-allocate large ranges**: Would waste disk space for small databases
3. **Linked-list of pages**: No random access, complicates reading, KuzuDB uses this only for DiskArray (very large structures)

---

### R2: Multi-Page Write Pattern

**Context**: How should serialized data be written across multiple pages?

**Decision**: Serialize to in-memory `Vec<u8>`, calculate `ceil(len / PAGE_SIZE)` pages, allocate range, write page-by-page.

**Rationale**:
- KuzuDB uses `InMemFileWriter`: accumulate in memory buffers, then `flush()` to allocated page range
- Our data is already fully serialized into a `Vec<u8>` via bincode before writing — we just need to chunk it across pages instead of writing to a single page
- Length prefix (4 bytes) is already used — we extend the format: `[4-byte total_length][data across N pages]`
- The length prefix tells the reader exactly how many bytes to reassemble

**Alternatives Considered**:
1. **Streaming write**: Serialize directly to pages without buffering. More complex, doesn't match current pattern where we `bincode::serialize()` first
2. **Per-page length prefix**: Each page has its own length. Redundant since total length + page count is sufficient

---

### R3: Multi-Page Read Pattern

**Context**: How should data spanning multiple pages be reassembled on load?

**Decision**: Read all pages in the range into a single `Vec<u8>`, then deserialize from the assembled buffer.

**Rationale**:
- The header's `PageRange` tells us exactly which pages to read and how many
- The 4-byte length prefix at the start of the first page tells us the exact byte count
- Read `num_pages` pages sequentially, concatenate their content, then take `length` bytes and deserialize
- Simple and matches the current pattern where we read a single page and deserialize

**Alternatives Considered**:
1. **Memory-mapped read**: Use mmap to get a contiguous view. Our buffer pool already handles page-level access, so this would bypass it
2. **Streaming deserialize**: Bincode supports readers, but assembling a Vec is simpler and data is small (MB range)

---

### R4: Page Allocation on Save — Fixed vs Dynamic Start Pages

**Context**: Should metadata sections always start at fixed pages (1, 2, 3) or use dynamically allocated pages?

**Decision**: Dynamic allocation on each save. The header's `PageRange` fields are updated to reflect the new allocation.

**Rationale**:
- KuzuDB dynamically allocates page ranges on each checkpoint — old ranges are freed, new ranges allocated
- With dynamic allocation, sections can grow independently without conflicting
- Fixed pages would require reserving enough pages upfront (impossible to predict) or complex in-place growth
- The header (page 0) is always fixed — that's sufficient as the root anchor

**Implementation Detail**:
- On save: allocate new ranges starting after the header (page 1+)
- Write catalog → get catalog_range, write node data → get metadata_range, write rel data → get rel_metadata_range
- Update header with new ranges
- Old pages become unreachable (no free-space reclaim in this interim step)

**Alternatives Considered**:
1. **Fixed start pages with overflow**: Pages 1, 2, 3 are always the start, overflow extends forward. Creates complex overlap management
2. **Pre-allocated regions**: Reserve N pages per section. Wastes space, still has limits

---

### R5: Database Version Migration (v2 → v3)

**Context**: How should existing v2 databases be handled when the multi-page format is introduced?

**Decision**: Bump version to 3. V2 databases are auto-migrated on open (same pattern as v1→v2 migration).

**Rationale**:
- The existing codebase already implements v1→v2 migration in `DatabaseHeader::deserialize_with_migration_flag()`
- V2 databases have fixed single-page ranges (catalog=page 1, metadata=page 2, rel_metadata=page 3)
- On open: read the v2 header, interpret the page ranges as before (single pages), load data normally
- On first save/checkpoint: data is written using the new multi-page allocation, header updated to v3
- No data loss — the migration is seamless

**Alternatives Considered**:
1. **In-place migration on open**: Rewrite the file immediately. More complex, risk of corruption if interrupted
2. **Keep version 2**: Don't bump version. Loses ability to distinguish formats, complicates future migrations

---

### R6: Page Range Validation on Load

**Context**: What integrity checks should be performed when reading page ranges from the header?

**Decision**: Validate on load that:
1. No page ranges overlap with each other or with page 0 (header)
2. All pages in each range are within file bounds
3. Length prefix in data does not exceed the range's byte capacity (`num_pages * PAGE_SIZE`)

**Rationale**:
- FR-011 requires page range integrity validation
- KuzuDB validates ranges during checkpoint reading
- Early detection prevents reading garbage data or buffer overflows
- These checks are cheap (no I/O, just arithmetic on header fields)

**Alternatives Considered**:
1. **CRC per page**: More thorough but significant overhead, not required by spec
2. **No validation**: Relies on header checksum only. Insufficient — header could be valid but point to wrong pages

---

### R7: Disk Space Handling

**Context**: What happens when disk space is insufficient for page allocation?

**Decision**: `DiskManager.allocate_page_range()` returns `Err(RuzuError::StorageError)` if `file.set_len()` fails due to insufficient disk space. This propagates up through save/checkpoint.

**Rationale**:
- FR-010 requires a clear error for insufficient disk space
- The OS returns an error from `set_len()` when disk is full
- We already map I/O errors to `StorageError` — no new error variant needed for this
- The database remains in its pre-save state (WAL provides the recovery guarantee)

---

### R8: Partially Written Multi-Page Save (Crash Mid-Write)

**Context**: What happens if the system crashes while writing a multi-page save?

**Decision**: Rely on existing WAL mechanism. The WAL records logical operations. On crash recovery, the WAL is replayed against the last known-good checkpoint state.

**Rationale**:
- The checkpoint flow is: (1) write data pages, (2) write header, (3) flush, (4) WAL checkpoint record, (5) truncate WAL
- If crash occurs before step (4), WAL still has uncommitted records that will be replayed
- If crash occurs during step (1), the header still points to old page ranges (not yet updated)
- The old pages remain valid until the header is updated
- This is the same crash safety guarantee as before — multi-page doesn't change the logical flow

---

## Summary of NEEDS CLARIFICATION Resolutions

All technical unknowns from the Technical Context have been resolved:

| Unknown | Resolution |
|---------|-----------|
| Page allocation strategy | Append-only contiguous via `DiskManager` (R1) |
| Multi-page write pattern | Serialize to Vec, chunk across pages (R2) |
| Multi-page read pattern | Assemble Vec from page range, deserialize (R3) |
| Fixed vs dynamic page starts | Dynamic allocation on each save (R4) |
| Version migration | v2→v3 auto-migration on open (R5) |
| Page range validation | Overlap, bounds, and capacity checks (R6) |
| Disk space errors | OS error propagated through StorageError (R7) |
| Crash mid-write | Existing WAL mechanism handles it (R8) |
