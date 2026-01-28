# Research: Phase 0 Proof of Concept

**Feature**: Phase 0 Proof of Concept - Basic Graph Database
**Date**: 2025-12-05
**Status**: Complete

This document consolidates research findings for technology choices and design decisions in the Phase 0 proof-of-concept.

---

## 1. Parser Library Selection (pest vs nom vs ANTLR4)

### Decision: **pest**

### Rationale:
- **Clean declarative syntax** similar to ANTLR4/EBNF, making it easy to port C++ grammar
- **Excellent error messages** out of the box, critical for user-facing query errors
- **Low learning curve** - can implement minimal Cypher grammar in 1-2 weeks
- **Mature and stable** (v2.7+, widely used in Rust ecosystem)
- **PEG-based** - unambiguous parsing, no shift/reduce conflicts

### Alternatives Considered:

**nom** (parser combinators):
- **Pros**: Faster parsing performance, more flexible, composable
- **Cons**: Verbose combinator chains, steep learning curve, harder to maintain grammar
- **Rejected because**: Phase 0 prioritizes rapid development over parsing performance. Parsing is unlikely to be a bottleneck (<10ms target for simple queries).

**ANTLR4 (antlr4rust)**:
- **Pros**: Could reuse exact C++ grammar from `src/antlr4/Cypher.g4`
- **Cons**: Rust bindings less mature, requires grammar file compilation, heavier dependency
- **Rejected because**: pest provides better Rust ecosystem integration with similar grammar syntax

### Implementation Notes:
- pest grammar file: `src/parser/grammar.pest`
- Reference C++ grammar: `C:\dev\kuzu\src\antlr4\Cypher.g4` (917 lines full Cypher)
- PoC subset: ~100-150 lines covering CREATE NODE TABLE, CREATE, MATCH, WHERE, RETURN

### Example Grammar Structure:
```pest
// Simplified structure (not complete grammar)
cypher_query = { ddl_statement | dml_statement | match_query }

ddl_statement = { create_node_table }
create_node_table = { "CREATE" ~ "NODE" ~ "TABLE" ~ identifier ~ "(" ~ column_list ~ ")" }

dml_statement = { create_node }
create_node = { "CREATE" ~ node_pattern }

match_query = { "MATCH" ~ pattern ~ where_clause? ~ return_clause }
node_pattern = { "(" ~ identifier? ~ (":" ~ label)? ~ properties? ~ ")" }
where_clause = { "WHERE" ~ expression }
return_clause = { "RETURN" ~ projection_list }
```

**References**:
- pest documentation: https://pest.rs/
- C++ KuzuDB grammar: `C:\dev\kuzu\src\antlr4\Cypher.g4`

---

## 2. Columnar Storage Approach (Apache Arrow vs Vec-based)

### Decision: **Vec-based storage for Phase 0, Arrow for Phase 1+**

### Rationale:
- **Phase 0 goal**: Validate end-to-end query execution, not optimize storage
- **Simplicity**: `Vec<Value>` is trivial to implement (~200 LOC vs 1000+ for Arrow integration)
- **No external dependencies**: Reduces PoC surface area
- **Performance acceptable**: For 1000 nodes, Vec overhead is negligible (<1ms)
- **Easy migration path**: Phase 1 can replace Vec with Arrow RecordBatch without changing executor interface

### Apache Arrow (deferred to Phase 1):
- **Pros**:
  - Columnar format matches KuzuDB architecture
  - SIMD-optimized operations
  - Zero-copy interop with DataFusion, Polars, Parquet
  - Battle-tested in production systems
- **Cons**:
  - Steeper learning curve
  - Adds dependency weight to PoC
  - Schema management overhead
- **When to adopt**: Phase 1 when adding disk persistence and performance optimization

### Implementation Notes:
```rust
// Phase 0 simplified storage
pub struct ColumnStorage {
    data: Vec<Value>,  // Simple vector of values
}

pub enum Value {
    Int64(i64),
    String(String),
    Null,
}

// Phase 1 migration to Arrow
use arrow::array::{Int64Array, StringArray};
use arrow::record_batch::RecordBatch;
```

**Performance Target**: <10MB memory for 1000 nodes with 2 properties each
**Estimated Memory**: 1000 nodes × 2 properties × ~100 bytes/value = ~200KB (well within target)

**References**:
- C++ KuzuDB columnar storage: `C:\dev\kuzu\src\storage\table\column_chunk_data.cpp` (1,094 LOC)
- Apache Arrow Rust: https://docs.rs/arrow

---

## 3. Benchmarking Framework (criterion)

### Decision: **criterion crate**

### Rationale:
- **Standard in Rust ecosystem** for micro-benchmarks
- **Statistical analysis** built-in (mean, std dev, outlier detection)
- **HTML reports** with charts for visualizing performance trends
- **Comparison baselines** for detecting regressions
- **Matches constitution requirement**: Principle III mandates continuous performance tracking

### Implementation Notes:
```rust
// benches/e2e_benchmark.rs
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_target_query(c: &mut Criterion) {
    let db = setup_database();
    c.bench_function("target_query", |b| {
        b.iter(|| {
            execute_query(black_box("MATCH (p:Person) WHERE p.age > 20 RETURN p.name, p.age"))
        })
    });
}

criterion_group!(benches, bench_target_query);
criterion_main!(benches);
```

### Benchmark Categories:
1. **Parse time**: Cypher string → AST (target: <10ms)
2. **Execution time**: AST → QueryResult (target: <100ms for 1000 nodes)
3. **Total time**: End-to-end including parsing (target: <200ms)

### C++ Baseline Establishment:
- **Prerequisite**: Run equivalent queries on C++ KuzuDB at `C:\dev\kuzu`
- **Hardware**: Document CPU, RAM, OS for fair comparison
- **Methodology**:
  - Warm-up runs (3 iterations)
  - Measurement runs (10 iterations)
  - Report median, p95, p99
- **Acceptance**: Rust PoC within 10x of C++ median time

**References**:
- criterion documentation: https://docs.rs/criterion
- C++ KuzuDB: `C:\dev\kuzu` (use built-in benchmark or write custom query timer)

---

## 4. Type System Design (Minimal vs Full)

### Decision: **Two types only: INT64 and STRING**

### Rationale:
- **Spec requirement**: Target query uses only `name STRING` and `age INT64`
- **Constitution Principle V**: Correctness over completeness in PoC
- **Reduced complexity**: Avoids type coercion rules, null handling edge cases
- **Easy extension**: Phase 1 can add BOOL, FLOAT, DATE without refactoring

### Type System Structure:
```rust
#[derive(Debug, Clone, PartialEq)]
pub enum DataType {
    Int64,
    String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int64(i64),
    String(String),
    Null,  // Minimal null support
}

impl Value {
    pub fn data_type(&self) -> Option<DataType> {
        match self {
            Value::Int64(_) => Some(DataType::Int64),
            Value::String(_) => Some(DataType::String),
            Value::Null => None,
        }
    }

    pub fn compare(&self, other: &Value) -> Option<Ordering> {
        // Type-safe comparison for WHERE clauses
    }
}
```

### Deferred Types (Phase 1+):
- BOOL, FLOAT, DOUBLE
- DATE, TIMESTAMP, INTERVAL
- UUID
- LIST, STRUCT, MAP (complex types)
- NODE, REL (graph semantics - may add lightweight version in PoC)

### Null Handling:
- **Approach**: Simple `Value::Null` variant
- **Semantics**: SQL null semantics (null > 20 = null, not false)
- **Primary keys**: Disallow null (validation in CREATE)

**References**:
- C++ KuzuDB types: `C:\dev\kuzu\src\include\common\types\types.h` (30+ types)
- C++ type inference: `C:\dev\kuzu\src\binder\bind_expression\bind_comparison_expression.cpp`

---

## 5. Query Execution Model (Pull vs Push)

### Decision: **Pull-based iterator model for PoC**

### Rationale:
- **Simpler implementation**: Each operator yields rows via `next()` method
- **Familiar pattern**: Similar to Rust Iterator trait
- **No threading complexity**: Single-threaded execution acceptable for PoC
- **C++ uses push-based**: KuzuDB uses factorized push model, but that's Phase 2 optimization

### Execution Interface:
```rust
pub trait PhysicalOperator {
    fn next(&mut self) -> Result<Option<Row>, ExecutionError>;
}

pub struct ScanOperator {
    table: Arc<NodeTable>,
    cursor: usize,
}

impl PhysicalOperator for ScanOperator {
    fn next(&mut self) -> Result<Option<Row>, ExecutionError> {
        // Return next row or None if exhausted
    }
}

pub struct FilterOperator {
    child: Box<dyn PhysicalOperator>,
    predicate: Expression,
}

impl PhysicalOperator for FilterOperator {
    fn next(&mut self) -> Result<Option<Row>, ExecutionError> {
        while let Some(row) = self.child.next()? {
            if self.predicate.eval(&row)? {
                return Ok(Some(row));
            }
        }
        Ok(None)
    }
}
```

### Alternatives Considered:

**Push-based (factorized)**:
- Used in C++ KuzuDB for performance
- Pros: Better CPU cache utilization, vectorized operations
- Cons: Complex control flow, harder to debug
- Deferred to Phase 2+ when performance optimization begins

**DataFusion integration**:
- Per feasibility assessment, DataFusion saves 30-40% of code
- Pros: Proven execution engine, parallel execution
- Cons: Adds dependency, SQL-centric design
- Decision: Evaluate in Phase 1 after PoC proves basic approach

**References**:
- C++ KuzuDB operators: `C:\dev\kuzu\src\include\processor\physical_operator.h` (76 operators)
- Pull model reference: https://db.in.tum.de/~leis/papers/morsels.pdf (Morsel-Driven Parallelism)

---

## 6. Error Handling Strategy

### Decision: **Result<T, E> with custom error types**

### Rationale:
- **Constitution Principle IV**: Rust idioms over C++ patterns
- **No panics in library code**: All errors returned via Result
- **User-facing errors**: Parser errors, schema errors, constraint violations
- **Internal errors**: Storage errors, type errors

### Error Type Structure:
```rust
#[derive(Debug, thiserror::Error)]
pub enum RuzuError {
    #[error("Parse error at line {line}, column {col}: {message}")]
    ParseError {
        line: usize,
        col: usize,
        message: String,
    },

    #[error("Schema error: {0}")]
    SchemaError(String),

    #[error("Type error: expected {expected}, got {actual}")]
    TypeError {
        expected: String,
        actual: String,
    },

    #[error("Constraint violation: {0}")]
    ConstraintViolation(String),

    #[error("Execution error: {0}")]
    ExecutionError(String),
}

pub type Result<T> = std::result::Result<T, RuzuError>;
```

### Error Message Requirements (from FR-018 to FR-020):
- **Parse errors**: Include line, column, and snippet of problematic query
- **Schema errors**: Specify table name, column name when applicable
- **Constraint errors**: Specify which constraint violated and conflicting value

### Library Dependency:
- **thiserror**: Derives `std::error::Error` trait with clean syntax
- Alternative: Manual impl (adds boilerplate but no dependency)

**References**:
- C++ KuzuDB exceptions: `C:\dev\kuzu\src\include\common\exception.h`
- Rust error handling: https://doc.rust-lang.org/book/ch09-00-error-handling.html

---

## 7. Test Strategy (Red-Green-Refactor)

### Decision: **Contract-first testing with acceptance scenarios**

### Rationale:
- **Constitution Principle II**: TDD is non-negotiable
- **Spec provides tests**: Each acceptance scenario = 1 test case
- **Test categories**: Contract > Integration > Unit (per constitution)

### Test Mapping from Spec:

**User Story 1 (Schema Definition) → 4 contract tests**:
- `test_create_node_table_success()`
- `test_create_duplicate_table_error()`
- `test_create_table_invalid_syntax_error()`
- `test_create_table_multiple_types()`

**User Story 2 (Insert Data) → 4 contract tests**:
- `test_create_node_success()`
- `test_create_node_duplicate_pk_error()`
- `test_create_node_missing_property_error()`
- `test_create_multiple_nodes()`

**User Story 3 (Query Data) → 5 contract tests**:
- `test_match_return_all()`
- `test_match_where_filter()`
- `test_match_nonexistent_table_error()`
- `test_match_invalid_where_syntax_error()`
- `test_match_empty_table()`

**User Story 4 (Benchmarking) → 4 integration tests**:
- Benchmark suite execution (not xUnit tests, but criterion benchmarks)

### TDD Workflow Example:
```rust
// 1. RED: Write failing test
#[test]
fn test_create_node_table_success() {
    let db = Database::new();
    let result = db.execute("CREATE NODE TABLE Person(name STRING, age INT64, PRIMARY KEY(name))");
    assert!(result.is_ok());

    // Verify schema stored
    let schema = db.catalog().get_table("Person").unwrap();
    assert_eq!(schema.name(), "Person");
    assert_eq!(schema.columns().len(), 2);
}

// 2. GREEN: Implement minimal code to pass
// (implement parser, catalog, execute logic)

// 3. REFACTOR: Improve code quality
// (extract helper functions, improve error messages, etc.)
```

### Test Coverage Targets (per constitution):
- **Phase 0**: 50% line coverage (focus on critical paths)
- **Phase 1**: 70% line coverage
- **MVP (Phase 3)**: 85% line coverage

**Tool**: `cargo-tarpaulin` for coverage measurement

**References**:
- Spec acceptance scenarios: `specs/001-poc-basic-functionality/spec.md`
- Constitution testing requirements: `.specify/memory/constitution.md` lines 237-256

---

## 8. C++ KuzuDB Baseline Benchmarking

### Approach: **Document C++ performance before implementing Rust**

### Methodology:

1. **Setup C++ KuzuDB**:
   ```bash
   cd C:\dev\kuzu
   # Build if not already built
   mkdir build && cd build
   cmake ..
   cmake --build . --config Release
   ```

2. **Create benchmark database**:
   ```cypher
   CREATE NODE TABLE Person(name STRING, age INT64, PRIMARY KEY(name));
   -- Insert 1000 nodes
   CREATE (:Person {name: 'Person_0', age: 0});
   CREATE (:Person {name: 'Person_1', age: 1});
   ...
   CREATE (:Person {name: 'Person_999', age: 999});
   ```

3. **Measure target query**:
   ```cypher
   MATCH (p:Person) WHERE p.age > 20 RETURN p.name, p.age;
   ```

4. **Metrics to capture**:
   - Parse time (if available)
   - Execution time
   - Total time
   - Memory usage (process RSS)
   - Number of rows returned (should be 979 rows: age 21-999)

5. **Hardware documentation**:
   - CPU model and frequency
   - RAM size
   - OS and version
   - Storage type (SSD/HDD)

### Expected C++ Performance (estimates based on feasibility assessment):
- **Parse time**: <1ms (ANTLR4 is fast)
- **Execution time**: ~5-10ms for 1000 node scan with filter
- **Total time**: <20ms

### Rust PoC Target (10x slower):
- **Parse time**: <10ms (pest overhead acceptable)
- **Execution time**: <100ms (Vec scan + filter)
- **Total time**: <200ms

### Abort Criteria:
- If Rust PoC is >10x slower (>200ms total), investigate bottleneck:
  - If parser >100ms → consider nom migration
  - If executor >1000ms → reconsider Vec storage or algorithm
  - If both slow → may need to adopt Arrow + DataFusion earlier

**References**:
- C++ KuzuDB CLI: `C:\dev\kuzu\build\Release\kuzu.exe` (or similar path)
- Benchmark approach: Manually time queries or write C++ benchmark harness

---

## Research Summary

| Decision Area | Choice | Deferred Alternative | Rationale |
|---------------|--------|---------------------|-----------|
| **Parser** | pest | nom (Phase 2) | Rapid development, clean syntax |
| **Storage** | Vec-based | Apache Arrow (Phase 1) | PoC simplicity, easy migration |
| **Benchmarks** | criterion | N/A | Rust standard, constitution requirement |
| **Types** | INT64, STRING only | 30+ types (Phase 1+) | Minimal scope for PoC |
| **Execution** | Pull-based iterators | Push/DataFusion (Phase 1) | Simple, debuggable |
| **Errors** | Result + thiserror | Manual impl | Rust idioms, clean code |
| **Testing** | TDD (Red-Green-Refactor) | N/A | Constitution requirement |
| **Baseline** | C++ KuzuDB benchmarks | N/A | Performance gate requirement |

All decisions align with constitution principles:
- ✅ **Port-First**: References C++ for algorithms, adapts for Rust idioms
- ✅ **TDD**: Test-first approach with acceptance scenarios
- ✅ **Benchmarking**: criterion + C++ baseline comparison
- ✅ **Rust Best Practices**: pest, Result, thiserror, no unsafe
- ✅ **Safety Over Performance**: Simple Vec storage, pull model

---

**Document Status**: ✅ Complete
**Next Step**: Proceed to Phase 1 (data-model.md, contracts/)
**Dependencies Resolved**: All "NEEDS CLARIFICATION" items from Technical Context are now specified.
