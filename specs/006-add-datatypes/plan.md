# Implementation Plan: Add Additional Datatypes

**Branch**: `006-add-datatypes` | **Date**: 2026-01-30 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/006-add-datatypes/spec.md`

## Summary

Add FLOAT64 and BOOL datatype support to the ruzu graph database. The type system already has `DataType::Float64`, `DataType::Bool`, `Value::Float64(f64)`, and `Value::Bool(bool)` enum variants implemented, along with CSV parsing, storage serialization, and comparison logic. This feature focuses on enabling these types through the PEST parser grammar (literal syntax + DDL type names), AST literal variants, and the execution paths in `lib.rs` that convert between grammar/AST and runtime types.

## Technical Context

**Language/Version**: Rust 1.75+ (stable, 2021 edition)
**Primary Dependencies**: pest (parser), serde + bincode (serialization), parking_lot (locks), csv (parsing), memmap2 (mmap)
**Storage**: Custom page-based file format with 4KB pages, WAL, buffer pool
**Testing**: `cargo test` (unit, contract, integration, lib tests — 440 total)
**Target Platform**: Linux (x86_64, aarch64), macOS (x86_64, aarch64), Windows (x86_64)
**Project Type**: Single Rust library + binary
**Performance Goals**: No regression on existing benchmarks (<5% threshold)
**Constraints**: No new dependencies required
**Scale/Scope**: ~6 files modified, ~200 lines changed

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Notes |
|-----------|--------|-------|
| I. Port-First | PASS | KuzuDB C++ supports DOUBLE and BOOL with case-insensitive keywords and IEEE 754 semantics. Our implementation follows the same approach. |
| II. TDD (Red-Green-Refactor) | PASS | Tests will be written first for each change layer (grammar, parser, DDL, DML, queries, persistence). |
| III. Benchmarking | PASS | Existing benchmarks will be run to verify no regression. No new benchmarks needed — type parsing is not a hot path. |
| IV. Rust Best Practices | PASS | Uses existing enum variants, serde derives, pattern matching. No unsafe code. |
| V. Safety & Correctness | PASS | NaN/Infinity rejection, strict bool parsing (true/false only), float comparison via partial_cmp. |

**Gate Result**: PASS — no violations.

## Project Structure

### Documentation (this feature)

```text
specs/006-add-datatypes/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/           # Phase 1 output
│   └── type-system.md   # Type system contract
└── tasks.md             # Phase 2 output (NOT created by /speckit.plan)
```

### Source Code (repository root)

```text
src/
├── parser/
│   ├── grammar.pest     # MODIFY: Add FLOAT64, BOOL type keywords + float/bool literals
│   ├── ast.rs           # MODIFY: Add Float64 and Bool variants to Literal enum
│   └── grammar.rs       # MODIFY: Add build logic for float and bool literals
├── types/
│   └── value.rs         # NO CHANGE: Float64/Bool already implemented
├── storage/
│   ├── csv/
│   │   └── node_loader.rs  # NO CHANGE: Float64/Bool CSV parsing already implemented
│   └── ...              # NO CHANGE: Serialization already works via serde
├── executor/
│   └── mod.rs           # MODIFY: Handle Float64/Bool literal-to-Value conversion in WHERE evaluation
└── lib.rs               # MODIFY: Handle FLOAT64/BOOL in DDL parsing + literal-to-Value conversion

tests/
├── contract_tests.rs    # ADD: Type system contract tests
├── integration_tests.rs # ADD: End-to-end tests for FLOAT64/BOOL
└── ...
```

**Structure Decision**: Single Rust project. All changes are within existing `src/` and `tests/` directories. No new modules or files needed in `src/`.

## Complexity Tracking

No violations. All changes follow existing patterns.

## Change Impact Analysis

### Layer 1: Grammar (grammar.pest)
- Add `^"FLOAT64"` and `^"BOOL"` to the `data_type` rule (line 39)
- Add `float_literal` and `bool_literal` rules to the `literal` rule (line 161)
- Float literal: optional minus, digits, dot, digits (e.g., `3.14`, `-0.5`, `42.0`)
- Bool literal: case-insensitive `true` / `false` (reuse existing `bool_literal` rule from line 191)
- Integer-to-float promotion: an integer literal like `42` provided where FLOAT64 expected handled at execution time, not grammar

### Layer 2: AST (ast.rs)
- Add `Float64(f64)` and `Bool(bool)` variants to `Literal` enum

### Layer 3: Parser (grammar.rs)
- Update `build_literal()` to parse float and bool grammar tokens into AST literals
- Float parsing: `str::parse::<f64>()` with NaN/Infinity rejection
- Bool parsing: case-insensitive match on "true"/"false"

### Layer 4: DDL Execution (lib.rs)
- Update `execute_create_node_table()` to recognize "FLOAT64" and "BOOL" type strings
- Update `execute_create_rel_table()` to recognize "FLOAT64" and "BOOL" type strings

### Layer 5: DML/Query Execution (lib.rs + executor/mod.rs)
- Update literal-to-Value conversion in CREATE node execution
- Update literal-to-Value conversion in WHERE clause evaluation
- Add Int64-to-Float64 promotion when comparing Int64 literal against Float64 column

### Layer 6: Persistence (NO CHANGES)
- Value::Float64 and Value::Bool already serialize/deserialize via serde + bincode
- Storage layer, WAL, and catalog already handle all Value variants

### Layer 7: CSV Import (NO CHANGES)
- `parse_field()` in node_loader.rs already handles Float64 and Bool types
- Note: existing CSV Bool parsing accepts "true"/"false"/"1"/"0"/"yes"/"no" — spec says only true/false. Need to align. **Decision**: Tighten CSV bool parsing to match spec (case-insensitive true/false only), matching KuzuDB behavior.
