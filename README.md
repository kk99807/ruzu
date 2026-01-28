# ruzu

**ruzu** (Rust + Kuzu) is a pure Rust port of [KuzuDB](https://github.com/kuzudb/kuzu), an embeddable graph database optimized for query speed and scalability.

## Project Status

🚧 **Active Development** - Phase 1 (Persistent Storage) is complete. Working toward MVP.

## Motivation

KuzuDB is a high-performance, embeddable graph database written in C++ that excels at:
- Blazing fast query performance
- Storage efficiency with columnar compression
- Embedded architecture (no separate server process)

However, KuzuDB development has been discontinued and the current codebase does not compile on Windows. This Rust port aims to:

1. **Preserve the excellent design** - Port the core architecture and algorithms from the C++ implementation
2. **Improve safety** - Leverage Rust's memory safety guarantees to eliminate entire classes of bugs
3. **Enable integration** - Provide seamless integration with Rust-based data processing toolkits
4. **Add concurrency** - Extend beyond the single-writer model with Rust's fearless concurrency
5. **Ensure longevity** - Maintain and evolve the codebase as an active open-source project

## Roadmap

### Phase 0: Proof of Concept ✅
- [x] Basic Cypher parser (CREATE NODE TABLE, CREATE node, MATCH with WHERE/RETURN)
- [x] In-memory columnar storage
- [x] Simple query execution
- [x] Establish baseline benchmarks

### Phase 1: Persistent Storage ✅
- [x] Disk-based storage with buffer pool management
- [x] Write-Ahead Logging (WAL) with crash recovery
- [x] Catalog and data persistence
- [x] Relationship/edge support (CSR format)
- [x] Bulk CSV import (COPY FROM)

### Phase 2: Query Engine
- [ ] Full query pipeline with Apache DataFusion integration
- [ ] Graph-specific operators (path expansion, relationship traversal)
- [ ] Query optimization

### Phase 3: Transactions & MVP
- [ ] MVCC transaction management
- [ ] Checkpointing
- [ ] Performance tuning (target: 2x slower than C++ KuzuDB)
- [ ] Production-ready v0.1.0

### Phase 4+: Future Enhancements
- Concurrent write transactions (multi-writer MVCC)
- Advanced compression algorithms
- Full Cypher support
- Parquet import/export
- Performance parity with C++ (0.8-1.2x)

## Reference Implementation

The original C++ KuzuDB codebase serves as our authoritative reference:
- **Repository**: https://github.com/kuzudb/kuzu
- **Documentation**: https://docs.kuzudb.com/

## Benchmarks

Benchmarks run on Windows x86_64, Rust 1.75+ (release mode).

### CSV Bulk Import Performance (Optimized - v0.0.2)

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
