# KuzuDB Rust Rewrite - Initial Feasibility Assessment

**Date:** 2025-12-05
**Project:** Rewrite KuzuDB graph database in Rust
**MVP Scope:** Core database functionality + minimal Cypher support
**Source Codebase:** C:\dev\kuzu (C++20, ~326K LOC)

---

## Executive Summary

**Feasibility Rating: 7/10 (MODERATE TO HIGH)**

Rewriting KuzuDB in Rust is **achievable** but requires significant effort. The C++ codebase is well-architected with clean separation of concerns, making translation feasible. The Rust ecosystem provides critical building blocks (Apache Arrow, DataFusion, pest parser) that can reduce development time by 30-40%. However, the codebase is substantial (~326K lines), and performance-critical components like the buffer manager will require careful low-level optimization.

**Key Finding:** KuzuDB does NOT load everything in memory as initially thought. It uses sophisticated mmap-based buffer pool management with page eviction, similar to traditional database systems.

**Recommended Approach:** 6-week proof-of-concept followed by phased MVP development targeting 6-month timeline with 2-3 senior Rust engineers.

---

## 1. Codebase Overview

### Size & Complexity
- **Total Lines of Code:** ~326,000 lines (212 lines/file average)
- **Source Files:** 1,535 files (734 .cpp + 801 .h)
- **Repository Size:** 729MB
- **Language:** C++20 with modern idioms
- **Architecture Maturity:** Production-grade, well-documented

### Main Components (by size)

| Component | Size | Files | Description |
|-----------|------|-------|-------------|
| **Processor** | ~1.3MB | 164 | Query execution engine (76 operators) |
| **Storage** | ~1.1MB | 73 | Disk-based columnar storage + buffer mgmt |
| **Function** | ~1.1MB | 154 | Built-in functions (agg, cast, string, math) |
| **Common** | ~707KB | 77 | Shared utilities, types, vectors |
| **Binder** | ~487KB | 72 | Semantic analysis, type checking |
| **Planner** | ~413KB | 83 | Query planning, join optimization |
| **Parser** | ~206KB | 31 | Cypher grammar (ANTLR4-based) |
| **Catalog** | ~100KB | 14 | Schema metadata management |
| **Transaction** | ~50KB | 3 | MVCC transaction management |
| **Main** | ~150KB | 16 | Database entry point, connections |

---

## 2. Core Database Components for MVP

### Essential Components (Must Implement)

#### A. Storage Engine (~15-20K LOC to port)

**Architecture:**
- **Buffer Manager:** Virtual memory-based page management using mmap with `MADV_DONTNEED`
- **Page Size:** 4KB (configurable via `KUZU_PAGE_SIZE_LOG2`)
- **Vector Capacity:** 2048 elements per batch (2^11)
- **Node Groups:** 131,072 nodes per group (2^17)
- **Layout:** Columnar CSR (Compressed Sparse Row) for adjacency lists
- **Compression:** Built-in support for columnar compression

**Key Files:**
- `src/include/storage/buffer_manager/buffer_manager.h` (299 lines)
- `src/include/storage/storage_manager.h` (104 lines)
- `src/storage/table/node_table.cpp` (856 lines)
- `src/storage/table/column_chunk_data.cpp` (1,094 lines)

**Page State Machine:**
```
EVICTED → pin() → LOCKED → unpin() → MARKED → evict() → EVICTED
                              ↓
                          UNLOCKED (optimistic read)
```

**Memory Management:**
- VM regions for both disk pages and temp buffers
- Second-chance eviction policy
- Configurable buffer pool (default: 80% of available RAM)
- **NOT in-memory only** - sophisticated virtual memory system

#### B. Parser (~5-7K LOC to port)

**Current Implementation:**
- **Grammar:** 917 lines of ANTLR4 Cypher grammar (`src/antlr4/Cypher.g4`)
- **Coverage:** Full openCypher + Kuzu extensions (DDL, DML, queries)
- **Generated Code:** ~10K lines of C++ from ANTLR4

**Minimal Cypher Subset for MVP:**

```cypher
-- Schema Definition
CREATE NODE TABLE Person(name STRING, age INT64, PRIMARY KEY(name));
CREATE REL TABLE KNOWS(FROM Person TO Person);

-- Data Manipulation
CREATE (:Person {name: 'Alice', age: 25});

-- Queries
MATCH (a:Person) RETURN a.name, a.age;
MATCH (a:Person)-[:KNOWS]->(b:Person) WHERE a.age > 20 RETURN a, b;
```

**Required Grammar (~200-300 lines in pest/nom):**
- Node patterns: `(variable:Label {prop: value})`
- Relationship patterns: `-[:REL_TYPE]->`
- WHERE clauses: property access, comparisons, AND/OR
- RETURN with projections
- CREATE statements
- Literals: string, integer, boolean, float

**Phase 2 Extensions:**
- ORDER BY, LIMIT, SKIP
- Aggregations: COUNT, SUM, AVG, MIN, MAX
- WITH clause for query composition
- Variable-length paths: `-[:KNOWS*1..3]->`

**Deferred:**
- OPTIONAL MATCH, UNION, subqueries
- MERGE semantics
- List comprehensions, map projections
- Stored procedures (CALL)

#### C. Binder (~10-12K LOC to port)

**Responsibilities:**
- Semantic analysis and validation
- Symbol resolution (table names, variables)
- Type checking and inference
- Query graph construction (nodes, relationships, patterns)
- Property access validation

**Key Files:**
- `src/include/binder/binder.h` (333 lines)
- `src/binder/bind/bind_graph_pattern.cpp` (695 lines)

#### D. Planner (~12-15K LOC to port)

**Components:**
- Logical plan generation
- Join order optimization (cost-based)
- Filter pushdown, projection pushdown
- 80+ logical operator types

**MVP Simplifications:**
- Heuristic join ordering (defer cost-based to Phase 2)
- Essential rewrites only (filter/projection pushdown)
- 15-20 logical operators initially

**Key Files:**
- `src/planner/plan/plan_join_order.cpp` (627 lines)

#### E. Processor (~25-30K LOC to port, or leverage DataFusion)

**Current Implementation:**
- **76 physical operators** (scan, filter, hash join, aggregation, etc.)
- **Vectorized execution:** 2048 tuples per batch
- **Factorized query processing:** Avoids materialization
- **Multi-threaded parallelism:** Push-based execution model

**MVP Essential Operators (~10-15):**
1. NodeScan
2. RelScan (relationship traversal)
3. Filter
4. Project
5. HashJoin
6. HashAggregate
7. Sort
8. Limit
9. Create (insert nodes/rels)
10. PathExpand (multi-hop traversal)

**DataFusion Integration Opportunity:**
- Reuse 30-40% of execution engine via Apache DataFusion
- Add custom graph operators (PathExpand, RelScan)
- See Section 7 for details

#### F. Catalog (~5-7K LOC to port)

**Manages:**
- Node table schemas
- Relationship table schemas
- Indexes
- Sequences
- User-defined types (deferred for MVP)

**Key Files:**
- `src/include/catalog/catalog.h`
- `src/catalog/catalog.cpp` (607 lines)

**Serialization:**
- Use `serde` for Rust
- Store in reserved pages at beginning of database file

#### G. Transaction Manager (~3-5K LOC to port)

**Features:**
- **MVCC:** Multi-Version Concurrency Control
- **Isolation:** Serializable (simplifies to single-writer for MVP)
- **WAL:** Write-Ahead Logging with checksums
- **Checkpointing:** Background async checkpoints

**Key Files:**
- `src/include/transaction/transaction_manager.h` (83 lines)
- `src/transaction/transaction.cpp` (222 lines)

**MVP Simplification:**
- Single-writer model (defer concurrent transactions)
- Basic WAL replay for crash recovery

### Components Deferred for Post-MVP

- ❌ Full-text search extensions
- ❌ Vector indices (for embeddings)
- ❌ GDS (Graph Data Science) functions
- ❌ Parquet import/export
- ❌ Extension framework
- ❌ Advanced compression (FSST, DuckDB-style)
- ❌ Multi-database (ATTACH/DETACH)
- ❌ User management and permissions
- ❌ Stored procedures

---

## 3. Memory Management: Critical Insight

### Misconception: "Loads Everything in Memory"

**Reality:** KuzuDB uses a sophisticated disk-based storage system with buffer pool management, similar to PostgreSQL or other traditional RDBMS.

### Current C++ Implementation

**Virtual Memory Architecture:**
```
┌─────────────────────────────────────────┐
│         Virtual Memory Region           │
│              (8TB default)               │
├─────────────────────────────────────────┤
│  Database File (mmap'd)                 │
│  ├─ Page 0: Catalog                     │
│  ├─ Page 1-1000: Node table             │
│  ├─ Page 1001-2000: Rel table           │
│  └─ ...                                  │
├─────────────────────────────────────────┤
│  Buffer Pool (configurable, default:    │
│               80% of available RAM)     │
│  ├─ Pin/unpin semantics                 │
│  ├─ Second-chance eviction              │
│  └─ Optimistic reads                    │
├─────────────────────────────────────────┤
│  Temp Buffers (256KB pages)             │
│  └─ In-memory scratch space             │
└─────────────────────────────────────────┘
```

**Key Mechanisms:**
1. **mmap with MADV_DONTNEED:** Explicit control over page eviction
2. **Pin/Unpin:** Reference counting prevents eviction of active pages
3. **Optimistic Reads:** Lock-free reads with validation
4. **Eviction Policy:** Second-chance (clock algorithm)

### Rust Translation Strategy

**Crates:**
```rust
use memmap2::MmapMut;              // Memory-mapped files
use parking_lot::{RwLock, Mutex};  // Faster than std::sync
use crossbeam::queue::SegQueue;    // Lock-free eviction queue
```

**Ownership Model:**
```rust
struct BufferManager {
    vm_region: MmapMut,  // mmap'd database file
    page_states: Vec<Arc<RwLock<PageState>>>,
    eviction_candidates: SegQueue<PageId>,
    buffer_pool_size: usize,
}

struct PageHandle<'a> {
    page_id: PageId,
    data: &'a [u8],  // Lifetime-bound to prevent use-after-free
    _guard: PageGuard,  // RAII unlocking
}
```

**Advantages in Rust:**
- Lifetime guarantees prevent dangling page pointers
- Arc/RwLock eliminates manual reference counting bugs
- `Drop` trait ensures pages are always unpinned
- No null pointer dereferences

**Challenges:**
- mmap requires `unsafe` blocks
- Memory ordering for lock-free reads needs careful attention
- Pin/unpin lifecycle must prevent moves (`std::pin::Pin`)

---

## 4. Dependencies Analysis

### Current C++ Dependencies (from CMakeLists.txt)

| C++ Library | Purpose | MVP Critical? | Rust Equivalent |
|-------------|---------|---------------|-----------------|
| **antlr4_runtime** | Parser generation | ✅ Yes | pest / nom / lalrpop |
| **re2** | Regular expressions | ✅ Yes | regex (native Rust) |
| **utf8proc** | Unicode handling | ✅ Yes | unicode-normalization |
| **zstd** | Compression | ✅ Yes | zstd (bindings) |
| **lz4** | Fast compression | ⚠️ Optional | lz4-sys |
| **parquet** | Columnar file format | ❌ No | parquet (Arrow) |
| **fast_float** | String→float parsing | ✅ Yes | fast-float (Rust port) |
| **mbedtls** | Checksums, hashing | ✅ Yes | sha2 / blake3 (pure Rust) |
| **nlohmann_json** | JSON parsing | ⚠️ Optional | serde_json |
| **yyjson** | Fast JSON | ⚠️ Optional | simd-json |
| **simsimd** | SIMD for vectors | ❌ No | - |
| **roaring_bitmap** | Compressed bitmaps | ❌ No | roaring (Rust port exists) |
| **httplib** | HTTP client | ❌ No | reqwest |

### MVP Dependency Strategy

**Phase 1 (Proof of Concept):**
- pest (parser)
- regex
- memmap2 (storage)
- parking_lot (concurrency)

**Phase 2 (Full MVP):**
- Add: zstd, serde, crossbeam
- Add: Apache Arrow + DataFusion (optional but recommended)

**Post-MVP:**
- parquet support
- JSON functions
- Advanced compression

---

## 5. Complexity Assessment

### Most Challenging Components

#### 1. Buffer Manager (Difficulty: 9/10)
**Complexity:**
- Lock-free data structures (eviction queue)
- Page state machine with 4 states
- Platform-specific mmap optimizations
- Memory ordering guarantees

**Risks:**
- Performance regression vs. C++ (critical path)
- Unsafe code bugs (mmap, raw pointers)

**Mitigation:**
- Start with simpler LRU eviction (mutex-based)
- Benchmark early and often
- Consider using `sled` or `redb` as temporary substitute
- Study DataFusion's buffer pool implementation

**Estimated Effort:** 3-4 weeks (2 engineers)

#### 2. Vectorized Execution Engine (Difficulty: 8/10)
**Complexity:**
- 76 operators to port
- Factorized execution model
- Multi-threaded pipeline parallelism
- SIMD optimizations

**Risks:**
- 25-30K LOC to port
- Performance-critical code

**Mitigation:**
- **Use Apache DataFusion** (recommended, see Section 7)
- Start with 10-15 operators only
- Defer SIMD to post-MVP
- Use Arrow columnar format

**Estimated Effort with DataFusion:** 4-6 weeks (2 engineers)
**Estimated Effort without:** 12-16 weeks (3 engineers)

#### 3. Query Optimizer (Difficulty: 7/10)
**Complexity:**
- Join order optimization (NP-complete, dynamic programming)
- Cost model requires statistics
- 20+ rewrite rules

**Mitigation:**
- Heuristic join ordering for MVP (left-deep trees)
- Essential rules only (filter pushdown, projection pushdown)
- Steal ideas from DataFusion's optimizer

**Estimated Effort:** 2-3 weeks (1 engineer)

#### 4. Type System (Difficulty: 6/10)
**Complexity:**
- 30+ data types in C++ (INT8-INT128, UINT8-UINT128, FLOAT, DOUBLE, STRING, DATE, TIMESTAMP, INTERVAL, UUID, LIST, STRUCT, MAP, UNION, NODE, REL, PATH, etc.)
- Nested types (list of structs, map of lists)
- Type inference and coercion (1,969 LOC in types.cpp)
- Cast functions (1,206 LOC)

**Mitigation:**
- Start with 10 primitive types:
  - INT64, FLOAT, DOUBLE, BOOL, STRING
  - DATE, TIMESTAMP
  - NODE, REL (graph semantics)
  - NULL
- Defer: INT128, UUID, LIST, STRUCT, MAP, UNION, PATH
- Use Arrow's type system where possible

**Estimated Effort:** 2-3 weeks (1 engineer)

#### 5. Transaction/WAL (Difficulty: 5/10)
**Complexity:**
- MVCC versioning infrastructure
- WAL format and replay logic
- Checkpoint coordination

**Mitigation:**
- Single-writer model (no concurrent writes)
- Reuse WAL format from C++ (forward compatibility)
- Defer advanced recovery scenarios

**Estimated Effort:** 2-3 weeks (1 engineer)

### Easier Components

| Component | Difficulty | Effort | Notes |
|-----------|-----------|--------|-------|
| **Parser** | 3/10 | 1-2 weeks | Well-defined grammar, good Rust tools |
| **Catalog** | 4/10 | 1 week | Straightforward metadata, use serde |
| **Common Utils** | 2/10 | 1 week | Vectors, I/O, many std equivalents |
| **Main/API** | 3/10 | 1 week | Database entry point, connection mgmt |

---

## 6. Rust Ecosystem: Game-Changing Libraries

### Apache Arrow + DataFusion: The 30-40% Solution

**Why This Changes Everything:**

#### Apache Arrow
- **Columnar in-memory format** (same as KuzuDB)
- Zero-copy data sharing across processes/languages
- Vectorized operations with SIMD
- 2048-element batches (configurable)
- Battle-tested (used by Spark, Pandas, Polars)

#### DataFusion (SIGMOD 2024)
- **SQL query planner and optimizer**
- **Physical execution engine** with 40+ operators
- Parallel execution runtime (Tokio-based)
- Expression evaluation framework
- Proven performance (InfluxDB 3.0, Comet, Ballista)

**Integration Strategy:**
```rust
use datafusion::prelude::*;
use datafusion::logical_plan::{LogicalPlan, LogicalPlanBuilder};
use arrow::array::{Int64Array, StringArray};

// Workflow:
// 1. Parse Cypher → Internal AST
// 2. Translate to DataFusion LogicalPlan
// 3. Use DataFusion's optimizer (free optimizations!)
// 4. Add custom physical operators for graph operations
// 5. Execute with DataFusion runtime
```

**What You Get for Free:**
- ✅ Filter, Project, HashJoin, HashAggregate, Sort, Limit
- ✅ Expression evaluation (comparisons, arithmetic, functions)
- ✅ Parallel execution
- ✅ Memory management
- ✅ Optimizer passes (filter pushdown, predicate simplification)

**What You Still Build:**
- ❌ Cypher parser (DataFusion only does SQL)
- ❌ Graph storage (node/rel tables, adjacency lists)
- ❌ Graph-specific operators (PathExpand, RelScan)
- ❌ Pattern matching for Cypher
- ❌ Catalog integration

**Code Savings:** ~10-15K LOC (execution engine + optimizer)

**Risks:**
- SQL-centric design may not fit graph queries perfectly
- Additional abstraction layer (performance overhead?)
- Less control over execution internals

**Recommendation:** **Strongly consider DataFusion integration**. The code reuse and proven performance outweigh the abstraction costs.

### Parser Options

| Library | Type | Pros | Cons | Recommendation |
|---------|------|------|------|----------------|
| **pest** | PEG | Clean syntax, great errors, easy to learn | Slower parsing | ✅ MVP |
| **nom** | Combinators | Fast, flexible, composable | Verbose, steep learning curve | Phase 2+ |
| **lalrpop** | LR(1) | Type-safe AST, traditional | Less flexible, compile-time errors | If you prefer YACC-style |
| **antlr4rust** | ANTLR4 | Reuse existing .g4 grammar | Less mature, requires nightly (older versions) | If reusing C++ grammar exactly |

**Recommendation:** **pest** for MVP. Example grammar:
```pest
match_clause = { "MATCH" ~ pattern ~ where_clause? }
pattern = { node ~ (edge ~ node)* }
node = { "(" ~ identifier? ~ label? ~ properties? ~ ")" }
edge = { "-[" ~ identifier? ~ ":" ~ rel_type ~ "]->" }
```

### Other Critical Crates

**Storage:**
- `memmap2` - Memory-mapped I/O (replaces C++ mmap)
- `sled` - Alternative embedded DB (if you want to skip custom storage)
- `redb` - LMDB-like embedded DB with MVCC

**Concurrency:**
- `crossbeam` - Lock-free queues, epoch-based GC
- `parking_lot` - Faster RwLock/Mutex than std
- `rayon` - Data parallelism (parallel iterators)

**Serialization:**
- `serde` - Serialization framework (catalog, WAL)
- `bincode` - Fast binary serialization
- `rkyv` - Zero-copy deserialization

**Compression:**
- `zstd` - Zstandard (same as C++)
- `lz4` - Fast compression
- `snap` - Snappy compression

**Graph Algorithms (if needed):**
- `petgraph` - General-purpose graph algorithms
- Note: Not optimized for disk-based graphs, use sparingly

---

## 7. Effort Estimates

### Optimistic Scenario (with DataFusion)

**Team:** 2 senior Rust engineers + 1 database/graph expert
**Timeline:** 4-6 months to MVP
**LOC to Write:** ~40-50K (vs. 326K full rewrite)

| Component | Weeks | Priority | LOC |
|-----------|-------|----------|-----|
| Cypher parser (pest, subset) | 2 | P0 | 3K |
| Type system (10 basic types) | 2 | P0 | 4K |
| Storage layer (nodes/rels) | 4 | P0 | 12K |
| Buffer manager (simplified LRU) | 3 | P0 | 5K |
| Catalog & schema | 1 | P0 | 2K |
| Transaction/WAL | 3 | P0 | 5K |
| DataFusion integration | 2 | P0 | 3K |
| Graph operators (scan, expand) | 4 | P0 | 8K |
| Binder (semantic analysis) | 2 | P0 | 6K |
| Basic optimizations | 2 | P1 | 3K |
| Testing & validation | 3 | P1 | - |
| **Total** | **28 weeks** | | **~51K** |

**Parallelization:** With 2 engineers, 28 weeks of work = ~14 calendar weeks = ~3.5 months. Add buffer for integration/testing → **4-6 months**.

### Realistic Scenario (minimal DataFusion use)

**Team:** 3 senior Rust engineers
**Timeline:** 8-12 months to MVP
**LOC to Write:** ~80-100K

Add ~6-8 weeks for building custom execution engine and optimizer from scratch.

### Pessimistic Scenario (no external libraries)

**Team:** 4-5 engineers
**Timeline:** 18-24 months to production-ready
**LOC to Write:** ~150-200K

Full rewrite of execution engine, optimizer, and all utilities. Not recommended.

---

## 8. Phased Development Plan

### Phase 0: Proof of Concept (6-8 weeks)

**Goal:** Execute a simple graph query end-to-end in Rust

**Target Query:**
```cypher
CREATE NODE TABLE Person(name STRING, age INT64, PRIMARY KEY(name));
CREATE (:Person {name: 'Alice', age: 25});
CREATE (:Person {name: 'Bob', age: 30});
MATCH (p:Person) WHERE p.age > 20 RETURN p.name, p.age;
```

**Deliverables:**
- [ ] Pest-based Cypher parser (MATCH, CREATE, WHERE, RETURN)
- [ ] In-memory columnar storage (Vec-based, no disk yet)
- [ ] Simple query executor (scan, filter, project)
- [ ] Basic type system (INT64, STRING, BOOL, FLOAT, DATE)
- [ ] 10-20 passing tests

**Team:** 2 engineers
**LOC:** ~5-7K
**Success Criteria:** Parse → Execute → Return correct results

**Decision Point:** If PoC takes >8 weeks or performance is >10x slower than C++, reassess approach.

---

### Phase 1: Persistent Storage (6-8 weeks)

**Goal:** Disk-based storage with buffer management

**Deliverables:**
- [ ] Mmap-based page management (memmap2)
- [ ] Simple LRU eviction policy
- [ ] Catalog persistence (serde-based)
- [ ] WAL for durability
- [ ] Node table and relationship table storage
- [ ] 50+ passing tests

**Team:** 2-3 engineers
**LOC:** ~15-20K cumulative

**Milestones:**
- Week 1-2: Page management infrastructure
- Week 3-4: Node/rel table implementation
- Week 5-6: WAL and recovery
- Week 7-8: Testing and bug fixes

---

### Phase 2: Query Engine (8-10 weeks)

**Goal:** Full query pipeline with joins and aggregations

**Deliverables:**
- [ ] DataFusion integration (if using)
- [ ] Binder (semantic analysis)
- [ ] Planner (logical plan generation)
- [ ] Core operators:
  - [ ] NodeScan, RelScan
  - [ ] Filter, Project
  - [ ] HashJoin
  - [ ] HashAggregate
  - [ ] Sort, Limit
  - [ ] PathExpand (multi-hop)
- [ ] Basic optimizer (filter pushdown)
- [ ] 100+ passing tests

**Team:** 3 engineers
**LOC:** ~35-40K cumulative

**Milestones:**
- Week 1-2: Binder implementation
- Week 3-4: Planner + DataFusion integration
- Week 5-7: Physical operators
- Week 8-10: Optimization and testing

---

### Phase 3: Transactions & Polish (4-6 weeks)

**Goal:** ACID compliance and production-readiness

**Deliverables:**
- [ ] MVCC transactions (single-writer)
- [ ] Checkpointing
- [ ] Error handling and recovery
- [ ] Performance benchmarks (LDBC Social Network SF-1)
- [ ] Documentation
- [ ] 200+ passing tests

**Team:** 3 engineers
**LOC:** ~45-50K cumulative

**Success Criteria:**
- [ ] Pass LDBC SF-1 benchmarks
- [ ] Performance within 2x of C++ KuzuDB on microbenchmarks
- [ ] No data corruption in crash tests
- [ ] API stability for v0.1.0 release

---

### Phase 4: Post-MVP (ongoing)

**Features to Add:**
1. Concurrent transactions (multi-writer MVCC)
2. Advanced compression (FSST, dictionary encoding)
3. Indexes (B-tree, vector indices)
4. More Cypher features (MERGE, subqueries, UNION)
5. Parquet import/export
6. Performance optimizations (SIMD, code generation)
7. Extension framework
8. Full-text search

---

## 9. Risk Analysis & Mitigation

### High-Risk Areas

#### Risk 1: Performance Regression (Likelihood: HIGH, Impact: HIGH)

**Concern:** Rust version is significantly slower than C++

**Mitigation:**
1. **Establish baselines early:**
   - Run LDBC benchmark on C++ KuzuDB (week 1)
   - Set target: within 2x for MVP, parity by v1.0
2. **Profile hot paths:**
   - Use `cargo flamegraph`, `perf`, `valgrind`
   - Identify bottlenecks early (weeks 4, 8, 12, 16)
3. **Incremental optimization:**
   - Don't optimize prematurely
   - Focus on algorithmic efficiency first
   - Add SIMD in Phase 4 if needed
4. **Leverage Arrow:**
   - Use Arrow's vectorized operations
   - Benefit from their SIMD implementations

**Abort Criteria:** If >5x slower after Phase 1, consider hybrid C++/Rust approach.

#### Risk 2: Buffer Manager Complexity (Likelihood: MEDIUM, Impact: HIGH)

**Concern:** Low-level memory management is buggy or inefficient

**Mitigation:**
1. **Start simple:**
   - Week 1-2: Mutex-based LRU (not lock-free)
   - Get correctness first, optimize later
2. **Consider alternatives:**
   - Use `sled` or `redb` as temporary storage backend
   - Prove rest of system works before tackling buffer mgmt
3. **Study references:**
   - DataFusion's memory pool
   - RocksDB's block cache
   - LeanStore's buffer manager (research paper)
4. **Extensive testing:**
   - Stress tests with limited memory
   - Concurrent access patterns
   - Crash recovery scenarios

#### Risk 3: Scope Creep (Likelihood: MEDIUM, Impact: MEDIUM)

**Concern:** Feature additions delay MVP

**Mitigation:**
1. **Strict MVP definition:**
   - Document in `mvp-scope.md`
   - Anything not listed → deferred to Phase 4
2. **Review gates:**
   - Weekly check: "Is this MVP-critical?"
   - Monthly: Re-evaluate scope vs. timeline
3. **Modular design:**
   - Plan for extensions from day 1
   - Make it easy to add features later

#### Risk 4: Team Expertise Gap (Likelihood: LOW, Impact: MEDIUM)

**Concern:** Team lacks Rust or database expertise

**Mitigation:**
1. **Hire/contract database expert**
2. **Study references:**
   - Read DataFusion, Polars, Glaredb source code
   - Study CMU database course materials
3. **Prototype early:**
   - PoC validates team can execute

### Medium-Risk Areas

- **WAL format compatibility:** Defer cross-version compatibility to v1.0
- **Windows build issues:** Test on Windows early (week 2)
- **Unsafe code bugs:** Extensive use of `miri` and sanitizers
- **Documentation:** Dedicate 10% of time to inline docs from start

---

## 10. Success Criteria

### MVP Definition (v0.1.0)

**Functional Requirements:**
- [ ] Parse basic Cypher (MATCH, WHERE, RETURN, CREATE for nodes/rels)
- [ ] Store data on disk (nodes, relationships, properties)
- [ ] Execute graph traversals (1-hop and multi-hop patterns)
- [ ] Support transactions (single-writer, serializable isolation)
- [ ] Basic aggregations (COUNT, SUM, MIN, MAX)
- [ ] WHERE clause filtering (comparisons, AND, OR, NOT)

**Non-Functional Requirements:**
- [ ] Performance: Within 2x of C++ KuzuDB on LDBC SF-1 queries
- [ ] Correctness: 200+ passing tests
- [ ] Reliability: No data corruption in crash tests
- [ ] Documentation: README, API docs, examples

**Anti-Goals (explicitly out of scope):**
- ❌ Binary compatibility with C++ KuzuDB databases
- ❌ Concurrent write transactions
- ❌ Extensions or plugins
- ❌ Advanced Cypher (MERGE, CALL, subqueries)
- ❌ Performance parity with C++ (2x slower is acceptable)

### v1.0 Production-Ready Criteria

- [ ] Performance parity with C++ KuzuDB
- [ ] Concurrent transactions (multi-writer MVCC)
- [ ] Full Cypher support (95%+ of openCypher spec)
- [ ] Advanced indexes (B-tree, vector indices)
- [ ] Parquet import/export
- [ ] Comprehensive test suite (1000+ tests, fuzzing)
- [ ] Battle-tested in production (3+ months uptime)

---

## 11. Key Decision Points

### Decision 1: DataFusion Integration Level

**Options:**

| Approach | Pros | Cons | Recommendation |
|----------|------|------|----------------|
| **Full:** Cypher → DataFusion LogicalPlan | Massive code reuse, proven optimizer | SQL-centric, less control | ⚠️ Risky |
| **Partial:** Use execution + Arrow format | Balanced reuse/control | More integration work | ✅ Recommended |
| **Minimal:** Arrow format only | Full control | Build optimizer from scratch | ❌ Too much work |

**Recommendation:** **Partial integration**
- Use DataFusion for physical operators (Filter, HashJoin, Aggregate)
- Build custom planner for graph patterns
- Use Arrow columnar format throughout
- Add custom operators for graph-specific logic (PathExpand, RelScan)

**Timeline:** Add 2 weeks for integration vs. building from scratch, saves 8-10 weeks on execution engine.

---

### Decision 2: Parser Strategy

**Options:**

| Tool | Best For | MVP Time | Learning Curve | Recommendation |
|------|----------|----------|----------------|----------------|
| **pest** | Rapid prototyping | 1-2 weeks | Low | ✅ MVP |
| **nom** | Performance-critical | 3-4 weeks | High | Phase 2+ |
| **lalrpop** | Traditional parsers | 2-3 weeks | Medium | If you prefer LR |
| **antlr4rust** | Reusing exact grammar | 2 weeks | Low | If exact compat needed |

**Recommendation:** **pest for MVP**, consider nom if parsing becomes bottleneck (>5% of query time).

**Rationale:**
- pest has clean syntax close to ANTLR
- Great error messages (important for user experience)
- Mature and stable
- 1-2 week implementation vs. 3-4 for nom

---

### Decision 3: Storage Format

**Options:**

| Approach | Pros | Cons | Recommendation |
|----------|------|------|----------------|
| **Binary compatible with C++** | Migration path from old DBs | Must replicate exact format, limits design | ❌ Too constraining |
| **New Rust-native format** | Optimize for Rust/Arrow, cleaner design | No migration path | ✅ MVP |
| **Arrow IPC format** | Interoperability, proven | Less control over layout | ⚠️ Consider for v1.0 |

**Recommendation:** **New Rust-native format for MVP**, add C++ database conversion tool post-MVP.

**Rationale:**
- Don't let legacy format constrain MVP design
- Few users to migrate (KuzuDB is abandoned)
- Arrow IPC is worth considering for v1.0 (interop with other Arrow tools)

---

### Decision 4: Concurrency Model

**Options:**

| Model | Complexity | MVP Time | Performance | Recommendation |
|-------|-----------|----------|-------------|----------------|
| **Single-writer MVCC** | Low | Baseline | Good for small loads | ✅ MVP |
| **Multi-writer MVCC** | High | +6-8 weeks | Excellent | Phase 4 |
| **Optimistic concurrency** | Medium | +3-4 weeks | Good | Consider for v1.0 |

**Recommendation:** **Single-writer for MVP**, multi-writer in Phase 4.

**Rationale:**
- Graph workloads are often read-heavy
- Single-writer simplifies transaction manager significantly
- Can add multi-writer later without breaking API

---

## 12. Benchmarking Strategy

### Reference Benchmarks

**LDBC Social Network Benchmark:**
- Industry-standard graph database benchmark
- Includes:
  - Interactive workload (short read queries)
  - Business Intelligence workload (complex analytics)
- Available at: https://github.com/ldbc/ldbc_snb_interactive_v2_impls

**Start with:**
- Scale Factor 1 (SF-1): ~11M nodes, ~62M edges (~1GB)
- 14 interactive queries (I1-I14)
- Target: Run all queries correctly, measure latency

### Performance Targets

| Milestone | Target vs. C++ KuzuDB | Acceptable? |
|-----------|----------------------|-------------|
| PoC (Phase 0) | 5-10x slower | ✅ Yes (in-memory only) |
| Phase 1 (storage) | 3-5x slower | ✅ Yes (unoptimized) |
| Phase 2 (execution) | 2-3x slower | ✅ Yes (no SIMD yet) |
| MVP (Phase 3) | 2x slower | ✅ Yes (acceptable) |
| v1.0 | 0.8-1.2x (parity) | ✅ Goal |

### Continuous Benchmarking

**Setup:**
1. Automated benchmark suite in CI
2. Run on every PR to `main`
3. Alert if >20% regression
4. Track trends over time (use `criterion` crate)

**Queries to Track:**
1. Simple node scan: `MATCH (p:Person) RETURN p`
2. 1-hop traversal: `MATCH (p:Person)-[:KNOWS]->(f) RETURN f`
3. 2-hop traversal: `MATCH (p:Person)-[:KNOWS*2]->(f) RETURN f`
4. Aggregation: `MATCH (p:Person) RETURN COUNT(p), AVG(p.age)`
5. Join: `MATCH (p:Person)-[:KNOWS]->(f:Person)-[:LIKES]->(m:Message) RETURN p, m`

---

## 13. Go/No-Go Recommendation

### **RECOMMENDATION: GO (with conditions)**

### Conditions for Proceeding

1. **✅ Commit to 6-week PoC first**
   - Must successfully parse and execute basic query
   - If >10x slower than C++ or PoC incomplete, reassess

2. **✅ Secure experienced team**
   - Minimum: 2 senior Rust engineers
   - Preferred: +1 database/graph expert
   - Can contract/hire if needed

3. **✅ Use DataFusion + Arrow**
   - Don't rebuild execution engine from scratch
   - Leverage mature Rust ecosystem
   - Accept some abstraction overhead

4. **✅ Accept performance tradeoff initially**
   - 2x slower than C++ is acceptable for MVP
   - Can optimize to parity by v1.0
   - Rust's safety may offset performance difference for users

5. **✅ Strictly scope MVP**
   - No extensions, no Parquet, no multi-DB
   - Only essential Cypher (MATCH, CREATE, WHERE, RETURN)
   - Defer advanced features to post-MVP

### Why This Is Feasible

**✅ Technical Factors:**
1. Clean, well-documented C++ codebase (good reference)
2. Mature Rust ecosystem (Arrow, DataFusion, pest)
3. 30-40% code reuse via libraries
4. Rust's safety eliminates entire bug classes
5. No legacy constraints (KuzuDB is abandoned)

**✅ Market Factors:**
1. No dominant Rust graph database exists (market gap)
2. Genomics use case is well-defined
3. Open source aligns with scientific community
4. Crate publication reaches Rust ecosystem

**✅ Risk Mitigation:**
1. Phased approach with early validation (PoC)
2. Multiple abort points if infeasible
3. Can fall back to hybrid C++/Rust if needed

### Why This Is Challenging

**⚠️ Technical Risks:**
1. Large codebase (~326K LOC original)
2. Performance-critical systems programming
3. Buffer management requires unsafe code
4. Limited prior art in Rust graph DBs

**⚠️ Resource Risks:**
1. 4-6 months of 2-3 senior engineers = significant investment
2. Requires domain expertise (graph databases, query processing)
3. Ongoing maintenance burden

### What Could Cause This to Fail

**❌ Abort Scenarios:**
1. **PoC fails:** Can't get basic query working in 8 weeks
2. **Performance disaster:** >10x slower with no clear path to improvement
3. **Buffer manager too complex:** Can't get mmap-based storage working safely
4. **Team attrition:** Lose key engineers mid-project
5. **Scope creep:** MVP balloons to 12+ months

---

## 14. Next Steps

### Immediate (Week 1-2)

1. **Set up project:**
   - [ ] Create Rust workspace: `cargo init --lib ruzu`
   - [ ] Add dependencies: pest, memmap2, arrow, datafusion
   - [ ] Set up CI/CD (GitHub Actions)

2. **Benchmark C++ KuzuDB:**
   - [ ] Install C++ version
   - [ ] Run LDBC SF-1 queries
   - [ ] Document baseline performance

3. **Design document:**
   - [ ] Architecture overview (components, interfaces)
   - [ ] Storage format specification
   - [ ] API design (public crate interface)

### Short-term (Week 3-8): PoC

4. **Implement parser:**
   - [ ] Write pest grammar for minimal Cypher
   - [ ] Parse to AST
   - [ ] Unit tests for parser

5. **Implement in-memory storage:**
   - [ ] Arrow-based columnar storage
   - [ ] Node table, relationship table
   - [ ] Simple insert/scan

6. **Implement executor:**
   - [ ] Scan operator
   - [ ] Filter operator
   - [ ] Project operator
   - [ ] Wire up: parse → plan → execute

7. **PoC demo:**
   - [ ] Run target query successfully
   - [ ] Measure performance vs. baseline
   - [ ] Decision: continue or pivot?

### Medium-term (Month 3-6): MVP

8. **Implement full stack per Phases 1-3**

9. **Continuous:**
   - [ ] Weekly benchmarking
   - [ ] Bi-weekly architecture reviews
   - [ ] Monthly scope reviews

### Long-term (Month 7+): Production

10. **Harden MVP → v1.0**
11. **Add concurrency, optimizations, advanced features**
12. **Publish crate, write docs, evangelize**

---

## 15. Resources & References

### Code References
- **C++ KuzuDB:** C:\dev\kuzu
- **Key files to study:**
  - `src/include/storage/buffer_manager/buffer_manager.h`
  - `src/include/processor/physical_operator.h`
  - `src/antlr4/Cypher.g4`

### Rust Ecosystem
- **Apache Arrow:** https://docs.rs/arrow
- **DataFusion:** https://docs.rs/datafusion
- **pest:** https://pest.rs
- **memmap2:** https://docs.rs/memmap2

### Graph Database Theory
- **LDBC Benchmark:** https://github.com/ldbc/ldbc_snb_docs
- **DataFusion paper (SIGMOD 2024):** https://arxiv.org/abs/2403.03665
- **LeanStore paper:** https://db.in.tum.de/~leis/papers/leanstore.pdf

### Learning Resources
- **CMU Database Course:** https://15445.courses.cs.cmu.edu
- **Rust Atomics and Locks:** https://marabos.nl/atomics/
- **Database Internals (book):** Alex Petrov

---

## 16. Conclusion

Rewriting KuzuDB in Rust is **ambitious but achievable**. The key to success is:

1. **Leverage existing infrastructure** (DataFusion, Arrow, pest)
2. **Start with strict MVP scope** (no bells and whistles)
3. **Validate early with PoC** (6-8 weeks)
4. **Accept initial performance tradeoff** (2x slower is OK)
5. **Hire/secure experienced team** (2-3 senior Rust engineers)

The Rust ecosystem has matured significantly in the database space. Projects like DataFusion, Polars, and InfluxDB 3.0 demonstrate that high-performance analytical engines are feasible in Rust. The safety guarantees and modern tooling (cargo, clippy, miri) can actually accelerate development compared to C++.

**The biggest risk is not technical feasibility, but resource commitment.** This is a 6-12 month project requiring sustained effort from experienced engineers. However, if you're committed to building a genomics toolkit and need an embeddable graph database, this rewrite could provide exactly the foundation you need—with the added benefits of Rust's safety, concurrency, and ecosystem.

**Final verdict: Proceed with PoC. Reassess after 6-8 weeks based on performance and technical challenges encountered.**

---

**Document Version:** 1.0
**Author:** AI Assessment (Claude)
**Date:** 2025-12-05
**Next Review:** After PoC completion (target: 2025-01-20)
