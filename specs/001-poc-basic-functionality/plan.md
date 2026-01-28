# Implementation Plan: Phase 0 Proof of Concept

**Branch**: `001-poc-basic-functionality` | **Date**: 2025-12-05 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/001-poc-basic-functionality/spec.md`

**Note**: This template is filled in by the `/speckit.plan` command. See `.specify/templates/commands/plan.md` for the execution workflow.

## Summary

This phase implements a minimal proof-of-concept graph database with basic Cypher query support. The goal is to validate technical feasibility by executing a simple end-to-end workflow: parse Cypher queries (CREATE NODE TABLE, CREATE node, MATCH with WHERE) → execute against in-memory columnar storage → return correct results. Success criteria: complete the target query workflow, establish baseline benchmarks, and achieve performance within 10x of C++ KuzuDB reference implementation.

## Technical Context

**Language/Version**: Rust 1.75+ (stable, 2021 edition)
**Primary Dependencies**: pest (parser), criterion (benchmarks), Apache Arrow (deferred for PoC)
**Storage**: In-memory columnar storage (Vec-based), no disk persistence in Phase 0
**Testing**: cargo test with unit/integration/contract test structure per constitution
**Target Platform**: Linux (x86_64, aarch64), macOS (x86_64, aarch64), Windows (x86_64)
**Project Type**: Single library crate with embedded database functionality
**Performance Goals**: Parse time <10ms, query execution <100ms for 1000 nodes, total end-to-end <200ms
**Constraints**: Memory usage <10MB for 1000 nodes (excluding Rust runtime), performance within 10x of C++ KuzuDB baseline
**Scale/Scope**: PoC handles 1000 nodes, 2 data types (STRING, INT64), 3 query types (CREATE NODE TABLE, CREATE, MATCH)

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

### I. Port-First (Reference Implementation)
**Status**: ✅ PASS

- References C++ KuzuDB at C:\dev\kuzu for parser grammar, type system, and query execution patterns
- Deviations documented: using pest instead of ANTLR4 (Rust ecosystem standard), Vec-based storage instead of mmap (PoC simplification)
- All algorithms will follow C++ reference implementation structure

### II. TDD with Red-Green-Refactor
**Status**: ✅ PASS

- All 4 user stories have detailed acceptance scenarios that translate to tests
- Test structure follows constitution: contract/ integration/ unit/ benches/
- PoC may defer comprehensive tests, but Phase 1+ requires full TDD compliance

### III. Benchmarking & Performance Tracking
**Status**: ✅ PASS

- Success criteria SC-005 establishes performance gate: within 10x of C++ KuzuDB
- FR-021 to FR-023 mandate benchmarking framework with separate timing measurements
- Baseline benchmark against C++ KuzuDB required before PoC completion
- criterion crate selected for Rust micro-benchmarks

### IV. Rust Best Practices & Idioms
**Status**: ✅ PASS

- Using pest (Rust-native parser) instead of ANTLR4
- Following safe Rust principles (no unsafe in PoC)
- Cargo.toml configured with clippy, rustfmt checks
- Dependencies: pest (mature), criterion (standard benchmark tool)

### V. Safety & Correctness Over Performance
**Status**: ✅ PASS

- Phase 0 prioritizes correctness: simple Vec-based storage before mmap optimization
- Performance target is 10x slower (acceptable for PoC per constitution Phase 0 gate)
- No unsafe code planned for PoC
- Focus on getting end-to-end query working correctly before optimization

## Project Structure

### Documentation (this feature)

```text
specs/[###-feature]/
├── plan.md              # This file (/speckit.plan command output)
├── research.md          # Phase 0 output (/speckit.plan command)
├── data-model.md        # Phase 1 output (/speckit.plan command)
├── quickstart.md        # Phase 1 output (/speckit.plan command)
├── contracts/           # Phase 1 output (/speckit.plan command)
└── tasks.md             # Phase 2 output (/speckit.tasks command - NOT created by /speckit.plan)
```

### Source Code (repository root)

```text
src/
├── lib.rs              # Public API: Database, Connection, QueryResult
├── parser/
│   ├── mod.rs          # Parser module entry
│   ├── grammar.pest    # Pest grammar for Cypher subset
│   └── ast.rs          # Abstract syntax tree definitions
├── catalog/
│   ├── mod.rs          # Schema catalog
│   └── schema.rs       # NodeTable, Column definitions
├── storage/
│   ├── mod.rs          # Storage module entry
│   ├── column.rs       # Columnar storage (Vec<Value>)
│   └── table.rs        # NodeTable storage
├── executor/
│   ├── mod.rs          # Executor module entry
│   ├── scan.rs         # Table scan operator
│   ├── filter.rs       # WHERE clause filtering
│   └── project.rs      # RETURN clause projection
├── types/
│   ├── mod.rs          # Type system
│   └── value.rs        # Value enum (Int64, String)
└── error.rs            # Error types

tests/
├── contract/
│   └── test_query_api.rs         # Public API contract tests
├── integration/
│   ├── test_end_to_end.rs        # Full query workflow tests
│   └── test_target_query.rs      # Specific PoC target query
└── unit/
    ├── parser_tests.rs           # Parser unit tests
    ├── storage_tests.rs          # Storage unit tests
    └── executor_tests.rs         # Executor unit tests

benches/
├── parse_benchmark.rs            # Parser performance
├── storage_benchmark.rs          # Storage performance
└── e2e_benchmark.rs              # End-to-end query performance
```

**Structure Decision**: Single library crate (Option 1). This is an embedded database library, not a client-server application, so a simple src/ layout with modular components (parser, catalog, storage, executor, types) is appropriate. Tests are separated by category following the constitution's testing requirements. Benchmarks use criterion framework per constitution Principle III.

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

No constitution violations. All gates pass.
