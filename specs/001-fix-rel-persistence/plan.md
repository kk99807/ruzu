# Implementation Plan: Fix Relationship Table Persistence

**Branch**: `001-fix-rel-persistence` | **Date**: 2026-01-29 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/001-fix-rel-persistence/spec.md`

## Summary

Fix critical data loss bug where relationship table data is not persisted to or loaded from disk during database open/close operations. The bug causes all relationship data to be silently lost after database restart, even though schemas persist correctly. The fix involves adding serialization/deserialization of relationship table data using the existing `RelTableData` structure, mirroring the pattern already implemented for node tables.

**Technical Approach**: Extend the existing `save_all_data()` and `load_table_data()` functions in `lib.rs` to include relationship tables. Add new `load_rel_table_data()` function that deserializes `RelTableData` from reserved metadata pages, and modify `Database::open()` to call it. The infrastructure (serialization traits, WAL support) already exists - this fix is purely about wiring it into the persistence layer.

## Technical Context

**Language/Version**: Rust 1.75+ (stable, 2021 edition)
**Primary Dependencies**: serde + bincode (serialization, already in use), parking_lot (synchronization, already in use)
**Storage**: Custom page-based file format with 4KB pages, WAL, buffer pool (already implemented)
**Testing**: cargo test (unit, integration, contract tests already exist)
**Target Platform**: Cross-platform (Linux, macOS, Windows)
**Project Type**: Single library project
**Performance Goals**:
- Database open time must remain linear with number of relationships (O(n) deserialization)
- No performance regression for node table operations
- Memory usage during open must remain constant (on-demand loading via buffer pool)
**Constraints**:
- Metadata must fit within reserved header pages (currently page 2 for data, need to verify capacity)
- Must maintain backward compatibility with existing node table persistence
- Zero data loss - failures must produce explicit errors, not silent corruption
**Scale/Scope**:
- Fix affects single file (`src/lib.rs`) primarily, 3 functions modified/added (~100 LOC)
- Must work with databases containing 0 to millions of relationships
- Must preserve all existing functionality and test coverage

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

### Principle I: Port-First (Reference Implementation)

✅ **PASS** - This is a bug fix, not new functionality. We are completing the implementation of relationship persistence which was already partially ported from KuzuDB. The C++ reference implementation persists all table types to disk - we are fixing our incomplete Rust port.

**Reference**: C++ KuzuDB stores relationship tables in its storage manager. See `kuzu/src/storage/store/rel_table.cpp` for the reference implementation.

### Principle II: TDD with Red-Green-Refactor

✅ **PASS** - Will follow strict TDD:
1. **Red**: Write tests for relationship persistence (save/load/query after restart) - verify they FAIL with current code
2. **Green**: Implement minimal fix (add rel_table serialization to save/load functions)
3. **Refactor**: Clean up any code duplication, improve error messages
4. **Re-test**: Ensure all new and existing tests pass

**Test Strategy**:
- Contract tests: Relationship data format compatibility
- Integration tests: Full database restart scenarios with relationships
- Unit tests: Individual serialize/deserialize operations
- Regression: Ensure existing node table persistence still works

### Principle III: Benchmarking & Performance Tracking

✅ **PASS** - Will add benchmarks for:
- Database open time with varying relationship counts (0, 1K, 10K, 100K relationships)
- Relationship query performance before/after restart (should be identical)
- Memory usage during database open operation

**Acceptance Criteria**: No more than 5% performance regression on existing benchmarks. New benchmarks must show linear scaling with relationship count.

### Principle IV: Rust Best Practices & Idioms

✅ **PASS** - Implementation will:
- Use existing `Result<T, E>` error handling patterns
- Leverage existing `bincode` serialization infrastructure
- Follow existing code style in `lib.rs` (save/load functions)
- Pass `cargo clippy` with zero warnings
- Use existing `HashMap<String, Arc<RelTable>>` ownership model
- Add doc comments for new functions

**No new dependencies required** - all necessary crates already in use.

### Principle V: Safety & Correctness Over Performance

✅ **PASS** - Correctness is paramount for this data loss bug:
- Simple, correct serialization first (mirror node table approach)
- Comprehensive error handling for deserialization failures
- Validation that loaded data matches schema expectations
- Property-based testing to verify invariants (CSR structure consistency)
- Manual testing of crash recovery scenarios

**Critical Invariants**:
1. All relationship data present before close must be present after open
2. Relationship schemas and data must be in sync
3. CSR structures (forward/backward groups) must remain valid after round-trip
4. WAL replay must correctly restore relationships

## Project Structure

### Documentation (this feature)

```text
specs/001-fix-rel-persistence/
├── spec.md              # Feature specification (already exists)
├── plan.md              # This file
├── research.md          # Phase 0 output - investigate storage page allocation
├── data-model.md        # Phase 1 output - RelTableData structure and page layout
├── quickstart.md        # Phase 1 output - testing guide for relationship persistence
└── contracts/           # Phase 1 output - persistence API contracts
    ├── save-format.md   # Serialization format specification
    └── load-api.md      # Deserialization API specification
```

### Source Code (repository root)

```text
src/
├── lib.rs                    # Primary modification: add rel_table save/load
├── storage/
│   ├── rel_table.rs          # Already has to_data()/from_data() - NO CHANGES NEEDED
│   ├── table.rs              # Node table persistence (reference implementation)
│   └── mod.rs                # Exports (may need to expose RelTableData)
├── error.rs                  # May add new error variants for rel_table load failures
└── catalog/
    └── schema.rs             # RelTableSchema already persists - NO CHANGES NEEDED

tests/
├── contract/
│   └── test_rel_persistence_format.rs  # New: validate serialization format stability
├── integration/
│   ├── test_database_restart.rs        # Existing: add rel_table restart scenarios
│   └── test_wal_recovery.rs            # Existing: add rel_table WAL replay tests
└── unit/
    └── (tests in src/lib.rs)           # New: unit tests for load_rel_table_data()
```

**Structure Decision**: Single project structure, consistent with existing ruzu architecture. All changes confined to database persistence layer (`src/lib.rs` primarily), with new tests following established patterns in `tests/` hierarchy.

**Modified Files** (estimated):
- `src/lib.rs` (~50 LOC added, 3 functions modified)
- `src/error.rs` (~10 LOC, add RelTableLoadError variants)
- `tests/integration/test_database_restart.rs` (~100 LOC, new test cases)
- `tests/contract/test_rel_persistence_format.rs` (~80 LOC, new file)

**Key Implementation Points**:
1. Modify `save_all_data()` at [lib.rs:356-405](src/lib.rs#L356-L405) to serialize `rel_tables` HashMap
2. Add `load_rel_table_data()` function similar to `load_table_data()` at [lib.rs:308-354](src/lib.rs#L308-L354)
3. Modify `Database::open()` at [lib.rs:182-195](src/lib.rs#L182-L195) to call new load function
4. Update `replay_wal()` at [lib.rs:197-270](src/lib.rs#L197-L270) to handle relationship operations in WAL

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

**No violations** - This fix aligns with all constitutional principles. It completes an existing feature (relationship persistence) using established patterns and infrastructure. No new complexity is introduced.
