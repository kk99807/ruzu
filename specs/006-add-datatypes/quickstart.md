# Quickstart: Add Additional Datatypes

**Feature**: 006-add-datatypes

## Prerequisites

- Rust 1.75+ installed
- Repository cloned and on branch `006-add-datatypes`
- All existing tests passing: `cargo test`

## Development Workflow

### 1. Verify Baseline

```bash
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

All 440 tests should pass with zero warnings.

### 2. Key Files to Modify

| File | Change | Purpose |
|------|--------|---------|
| `src/parser/grammar.pest` | Add `FLOAT64`/`BOOL` to `data_type`; add `float_literal`/`bool_literal` to `literal` | Grammar recognizes new types |
| `src/parser/ast.rs` | Add `Float64(f64)` and `Bool(bool)` to `Literal` enum | AST represents new literals |
| `src/parser/grammar.rs` | Handle `float_literal` and `bool_literal` in `build_literal()` | Parser builds AST from grammar |
| `src/lib.rs` | Handle `"FLOAT64"`/`"BOOL"` in DDL; handle new Literal variants in DML/queries | Execution supports new types |
| `src/executor/mod.rs` | Handle new Literal variants in WHERE evaluation; Int64→Float64 promotion | Queries filter on new types |
| `src/storage/csv/node_loader.rs` | Tighten Bool parsing to true/false only | CSV import matches spec |

### 3. TDD Cycle

For each layer (grammar → AST → parser → DDL → DML → queries → CSV):

1. **Red**: Write test asserting new behavior (e.g., `CREATE NODE TABLE Product(name STRING, price FLOAT64, PRIMARY KEY(name))` succeeds)
2. **Green**: Implement minimal code change
3. **Refactor**: Clean up
4. Run `cargo test` — all tests pass

### 4. Quick Smoke Test

After implementation, verify end-to-end:

```rust
let db = Database::create("test.db")?;
db.execute("CREATE NODE TABLE Product(name STRING, price FLOAT64, active BOOL, PRIMARY KEY(name))")?;
db.execute("CREATE (:Product {name: 'Widget', price: 19.99, active: true})")?;
let result = db.execute("MATCH (p:Product) WHERE p.price > 10.0 RETURN p.name, p.price, p.active")?;
assert_eq!(result.rows[0].get("p.price"), Some(&Value::Float64(19.99)));
assert_eq!(result.rows[0].get("p.active"), Some(&Value::Bool(true)));
```

### 5. Verify No Regressions

```bash
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo bench --bench csv_benchmark
cargo bench --bench storage_benchmark
```
