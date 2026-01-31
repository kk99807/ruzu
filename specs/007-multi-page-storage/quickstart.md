# Quickstart: Multi-Page Storage Implementation

**Feature**: 007-multi-page-storage
**Date**: 2026-01-30

## Overview

This feature removes the 4KB single-page limit for metadata storage (catalog, node tables, relationship tables). After implementation, each metadata section can span as many contiguous pages as needed.

## Key Changes at a Glance

| Component | File | Change |
|-----------|------|--------|
| Page allocator | `src/storage/page/disk_manager.rs` | Add `allocate_page_range(num_pages)` |
| PageRange helpers | `src/storage/mod.rs` | Add `overlaps()`, `byte_capacity()`, `contains_page()` |
| Header validation | `src/storage/mod.rs` | Add `validate_ranges()`, bump `CURRENT_VERSION` to 3 |
| Multi-page write | `src/lib.rs` | New `write_multi_page()` helper |
| Multi-page read | `src/lib.rs` | New `read_multi_page()` helper |
| Save logic | `src/lib.rs` | `save_all_data()` uses dynamic allocation instead of fixed pages |
| Load logic | `src/lib.rs` | `load_table_data()` and `load_rel_table_data()` use multi-page read |
| V2 migration | `src/storage/mod.rs` | Add v2→v3 migration path in `deserialize_with_migration_flag()` |

## Implementation Order

### Step 1: Foundation (PageRange + DiskManager)
Add `PageRange` helper methods and `DiskManager::allocate_page_range()`. These are pure additions with no breaking changes.

### Step 2: Multi-Page Read/Write Helpers
Implement `write_multi_page()` and `read_multi_page()` in `src/lib.rs`. These are new functions — no existing code changes yet.

### Step 3: Node Data Multi-Page (User Story 1)
Modify `save_all_data()` to use `write_multi_page()` for node table data. Modify `load_table_data()` to use `read_multi_page()`. Update header with dynamic ranges.

### Step 4: Rel Data Multi-Page (User Story 2)
Same pattern as Step 3 for relationship table data. Remove the existing size validation check (`rel_data_len > PAGE_SIZE - 4`).

### Step 5: Catalog Multi-Page (User Story 3)
Same pattern for catalog data.

### Step 6: Version Bump + Migration (User Story 4)
Bump `CURRENT_VERSION` to 3. Add v2→v3 migration in `deserialize_with_migration_flag()`. Ensure v2 databases load correctly and are re-saved in v3 format.

### Step 7: Validation + Edge Cases (User Story 5 + Edge Cases)
Add `validate_ranges()` to header. Handle page boundary cases. Verify WAL replay works with multi-page storage.

## Running Tests

```bash
# Run all tests
cargo test

# Run specific test categories
cargo test --test contract_tests
cargo test --test integration_tests
cargo test --test unit_tests

# Run with output for debugging
cargo test -- --nocapture

# Run benchmarks
cargo bench --bench storage_benchmark
cargo bench --bench rel_persist_benchmark
```

## Key Design Decisions

1. **Dynamic allocation on every save**: Pages are allocated fresh each save. Old pages are not reclaimed (append-only file growth). This is intentional for simplicity — free-space management is out of scope.

2. **Sequential allocator**: Catalog is allocated first (pages 1+), then node data, then rel data. The order is deterministic.

3. **Length-prefixed format preserved**: The 4-byte length prefix at the start of each section's first page is unchanged. Multi-page is transparent to the deserialization logic.

4. **`save_all_data` becomes `&mut self`**: Because it now updates `self.header` with new page ranges.

## Verification Checklist

- [ ] Create node table with > 4KB data, close and reopen — all data intact
- [ ] Create rel table with > 4KB data, close and reopen — all data intact
- [ ] Create enough schemas to exceed 4KB catalog, close and reopen — all schemas intact
- [ ] Open a v2 database, verify data loads, close, reopen — now v3 format
- [ ] Simulate crash after multi-page write, verify WAL replay restores committed data
- [ ] All 440+ existing tests still pass
- [ ] `cargo clippy` zero warnings
- [ ] Benchmarks show no > 2x regression for equivalent data sizes
