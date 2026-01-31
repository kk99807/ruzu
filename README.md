# ruzu

**ruzu** is an embeddable graph database written in pure Rust, inspired by [KuzuDB](https://github.com/kuzudb/kuzu). It uses a subset of the Cypher query language and targets use cases where you want a lightweight, embedded graph database with no separate server process.

> **v0.0.2 — Early Development.** This is a working database with persistence, crash recovery, multi-page storage, and a real query engine, but it is not yet production-ready. See [Current Limitations](#current-limitations) below.

## Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
ruzu = "0.0.2"
```

```rust
use ruzu::Database;

let db = Database::open("my_graph.db").unwrap();

db.execute("CREATE NODE TABLE Person(name STRING, age INT64, PRIMARY KEY(name))").unwrap();
db.execute("CREATE (:Person {name: 'Alice', age: 30})").unwrap();
db.execute("CREATE (:Person {name: 'Bob', age: 25})").unwrap();

let results = db.execute("MATCH (p:Person) WHERE p.age > 20 RETURN p.name, p.age").unwrap();
```

## Features

- **Embedded** — No server, no network. Open a file and query it.
- **Persistent** — Multi-page storage with write-ahead logging and crash recovery.
- **Cypher queries** — Supports a subset of Cypher (see [Supported Cypher](#supported-cypher) below).
- **Relationships** — CSR (Compressed Sparse Row) edge storage with bidirectional traversal.
- **Bulk import** — `COPY FROM` CSV with parallel parsing via rayon.
- **Buffer pool** — LRU page eviction for memory-constrained operation.

## Supported Data Types

| Type | Description | Parser support |
|------|-------------|----------------|
| `INT64` | 64-bit signed integer | Yes |
| `FLOAT64` | 64-bit floating point | Yes |
| `BOOL` | Boolean | Yes |
| `STRING` | UTF-8 string | Yes |
| `Date` | Days since Unix epoch | Code only* |
| `Timestamp` | Microseconds since Unix epoch | Code only* |
| `Float32` | 32-bit floating point | Code only* |

\* These types exist in the type system but cannot yet be used in `CREATE NODE TABLE` DDL statements.

## Supported Cypher

**DDL:**
- `CREATE NODE TABLE Name(col1 TYPE, col2 TYPE, PRIMARY KEY(col1))`
- `CREATE REL TABLE Name(FROM Table1 TO Table2, prop1 TYPE, ...)`

**DML:**
- `CREATE (:Label {prop: value, ...})`
- `MATCH (n:Label) RETURN n.prop` with optional `WHERE`, `ORDER BY`, `SKIP`, `LIMIT`
- `MATCH (a:Label)-[:REL]->(b:Label) RETURN a.prop, b.prop`
- `MATCH (a)-[r:REL*min..max]->(b) RETURN ...` (variable-length paths)
- `MATCH (a:Label), (b:Label) CREATE (a)-[:REL {props}]->(b)`
- Aggregates: `COUNT(*)`, `COUNT(expr)`, `SUM`, `AVG`, `MIN`, `MAX`
- `EXPLAIN` prefix for query plans

**Bulk import:**
- `COPY table FROM 'file.csv'` with options: `HEADER`, `DELIM`, `SKIP`, `IGNORE_ERRORS`

**Not yet supported:** `SET`, `DELETE`, `MERGE`, `WITH`, `OPTIONAL MATCH`, `UNWIND`, subqueries, list/map types, path functions, string functions.

## Current Limitations

1. **No columnar-file storage.** Data is stored across multiple 4KB pages (no single-page limit), but not yet in the file-per-column layout that KuzuDB uses for performant multi-hop traversals. Near-term plan is refactoring toward a columnar-file architecture.

2. **Limited data types.** Only 4 types (`INT64`, `FLOAT64`, `BOOL`, `STRING`) are usable end-to-end in DDL. `Date`, `Timestamp`, and `Float32` exist in the type system but are not yet wired into the parser.

3. **Cypher subset.** The query language covers basic MATCH/RETURN with filtering, ordering, aggregation, and variable-length paths, but does not yet support mutations (`SET`/`DELETE`), `WITH` chaining, `OPTIONAL MATCH`, or most Cypher functions. Near-term plans include expanding MATCH capabilities, adding `EXISTS`/`NOT EXISTS`, and multi-hop chained MATCH patterns.

4. **Single-writer.** No concurrent transactions. One writer at a time.

5. **No indexes.** Queries use full scans. No B-tree or hash indexes yet.

## Roadmap

### Phase 0: Proof of Concept — Done
- Basic Cypher parser, in-memory columnar storage, simple query execution

### Phase 1: Persistent Storage — Done
- Disk-based storage with buffer pool, WAL with crash recovery, relationship/edge support (CSR), bulk CSV import

### Phase 2: Storage & Query Language — In Progress
- Columnar-file storage (file-per-column, matching KuzuDB architecture)
- Full Cypher support
- Apache DataFusion integration, graph-specific operators, query optimization

### Phase 3: Transactions & MVP
- MVCC, checkpointing, performance tuning, production-ready v0.1.0

### Phase 4+: Future
- Multi-writer MVCC, advanced compression, Parquet import/export

## Reference

Inspired by [KuzuDB](https://github.com/kuzudb/kuzu) ([docs](https://docs.kuzudb.com/)).

## Benchmarks

Benchmarks run on Windows x86_64, Rust 1.75+ (release mode).

### CSV Bulk Import Performance (Optimized)

With parallel parsing and memory-mapped I/O enabled:

| Dataset Size | Mode | Time | Throughput |
|--------------|------|------|------------|
| 100,000 nodes | Sequential | ~94 ms | **1.07M nodes/sec** |
| 100,000 nodes | Parallel | ~11 ms | **8.9M nodes/sec** |
| 2,400,000 edges | Sequential | ~2.9 s | **820K edges/sec** |
| 2,400,000 edges | Parallel | ~630 ms | **3.8M edges/sec** |

**Parallel speedup**: ~4.8x for nodes, ~4.6x for edges

### Comparison with KuzuDB (Published Data)

From [kuzudb-study](https://github.com/prrao87/kuzudb-study) benchmarks (KuzuDB v0.9.0, M3 MacBook Pro):

| Metric | KuzuDB* | ruzu (parallel) | Ratio |
|--------|---------|-----------------|-------|
| Node import (100K) | ~769K nodes/sec | **8.9M nodes/sec** | ruzu 11.6x faster |
| Edge import (2.4M) | ~5.3M edges/sec | **3.8M edges/sec** | ruzu 1.4x slower |

\* Calculated from published results: 100K nodes in 0.13 sec, 2.4M edges in 0.45 sec

**Important Caveats**:
- Comparison uses published KuzuDB data, not direct testing on same hardware
- KuzuDB benchmarks include disk I/O; ruzu benchmark is parse + in-memory storage
- Different hardware (M3 Mac vs Windows x86_64) affects results
- ruzu now uses parallel parsing with rayon + memory-mapped I/O

### Running Benchmarks

```bash
# All benchmarks
cargo bench

# Specific benchmarks
cargo bench --bench csv_benchmark      # CSV import performance
cargo bench --bench buffer_benchmark   # Buffer pool operations
cargo bench --bench storage_benchmark  # Storage layer
cargo bench --bench e2e_benchmark      # End-to-end queries
```

## Development

### Setup

After cloning, enable the pre-commit hook that blocks commits with clippy warnings:

```bash
git config core.hooksPath .githooks
```

This project uses [SpecKit](https://github.com/cased/speckit) for structured development workflow:

- **Constitution**: See [.specify/memory/constitution.md](.specify/memory/constitution.md) for development principles
- **Workflow**: Features → Specifications → Plans → Tasks
- **Commands**: `/speckit.specify`, `/speckit.plan`, `/speckit.tasks`, `/speckit.implement`

## Contributing

Contributions are welcome! Please read:
1. [Constitution](.specify/memory/constitution.md) - Core principles and quality gates
2. [CONTRIBUTING.md](CONTRIBUTING.md) - Detailed contribution guidelines

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

This project is a port of [KuzuDB](https://github.com/kuzudb/kuzu) which is licensed under MIT. We've adopted the dual MIT/Apache-2.0 license following Rust ecosystem conventions.

## Acknowledgments

This project is a port of [KuzuDB](https://github.com/kuzudb/kuzu), created by the KuzuDB team. We are deeply grateful for their excellent work on the original implementation.
