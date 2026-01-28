# Quick Start Guide: Phase 0 Proof of Concept

**Version**: 1.0.0
**Date**: 2025-12-05
**Target Audience**: Developers implementing Phase 0

This guide provides step-by-step instructions for building the Phase 0 proof-of-concept from scratch using Test-Driven Development.

---

## Prerequisites

**Required**:
- Rust 1.75+ (`rustup update stable`)
- Git
- Text editor or IDE (VS Code with rust-analyzer recommended)

**Optional**:
- C++ KuzuDB installation at `C:\dev\kuzu` for baseline benchmarking

**Verify Installation**:
```bash
rustc --version  # Should be 1.75 or newer
cargo --version
```

---

## Phase 0 Workflow Overview

```
┌─────────────────────────────────────────────────────────────┐
│ Phase 0: Proof of Concept (6-8 weeks, TDD Red-Green-Refactor)│
└─────────────────────────────────────────────────────────────┘
        │
        ├─> Week 1-2: Parser (pest grammar for Cypher)
        ├─> Week 2-3: Catalog & Type System
        ├─> Week 3-4: Storage (Vec-based columnar)
        ├─> Week 4-5: Executor (Scan, Filter, Project operators)
        ├─> Week 5-6: Integration & Testing
        └─> Week 6-8: Benchmarking & Documentation
```

---

## Step 0: Project Setup

### Initialize Project

```bash
cd C:\dev\ruzu

# Verify Cargo.toml exists (should have been created during initialization)
# If not, create it manually or run: cargo init --lib
```

### Add Dependencies to Cargo.toml

```toml
[package]
name = "ruzu"
version = "0.0.1"
edition = "2021"
rust-version = "1.75"

[dependencies]
pest = "2.7"
pest_derive = "2.7"
thiserror = "1.0"

[dev-dependencies]
criterion = "0.5"

[[bench]]
name = "e2e_benchmark"
harness = false

[[bench]]
name = "parse_benchmark"
harness = false

[[bench]]
name = "storage_benchmark"
harness = false
```

### Create Directory Structure

```bash
# Create source directories
mkdir src\parser
mkdir src\catalog
mkdir src\storage
mkdir src\executor
mkdir src\types

# Create test directories
mkdir tests\contract
mkdir tests\integration
mkdir tests\unit

# Create benchmark directory
mkdir benches
```

### Verify Build

```bash
cargo build
cargo test
```

**Expected**: Clean build with 0 tests (we'll add tests next).

---

## Step 1: Type System (TDD Red-Green-Refactor)

### RED: Write Failing Test

**File**: `tests/unit/types_tests.rs`

```rust
use ruzu::types::{DataType, Value};

#[test]
fn test_value_int64() {
    let value = Value::Int64(42);
    assert_eq!(value.as_int64(), Some(42));
    assert_eq!(value.is_null(), false);
}

#[test]
fn test_value_string() {
    let value = Value::String("hello".into());
    assert_eq!(value.as_string(), Some("hello"));
    assert_eq!(value.is_null(), false);
}

#[test]
fn test_value_null() {
    let value = Value::Null;
    assert!(value.is_null());
    assert_eq!(value.as_int64(), None);
    assert_eq!(value.as_string(), None);
}

#[test]
fn test_value_compare_int64() {
    let a = Value::Int64(10);
    let b = Value::Int64(20);
    assert_eq!(a.compare(&b), Some(std::cmp::Ordering::Less));
}

#[test]
fn test_value_compare_null() {
    let a = Value::Int64(10);
    let b = Value::Null;
    assert_eq!(a.compare(&b), None); // SQL null semantics
}
```

**Run**: `cargo test types_tests`

**Expected**: Compilation errors (types module doesn't exist yet) - **RED** ✓

---

### GREEN: Implement Minimal Code

**File**: `src/types/mod.rs`

```rust
mod value;

pub use value::{DataType, Value};
```

**File**: `src/types/value.rs`

```rust
use std::cmp::Ordering;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataType {
    Int64,
    String,
}

impl DataType {
    pub fn name(&self) -> &'static str {
        match self {
            DataType::Int64 => "INT64",
            DataType::String => "STRING",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int64(i64),
    String(String),
    Null,
}

impl Value {
    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    pub fn as_int64(&self) -> Option<i64> {
        match self {
            Value::Int64(i) => Some(*i),
            _ => None,
        }
    }

    pub fn as_string(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn data_type(&self) -> Option<DataType> {
        match self {
            Value::Int64(_) => Some(DataType::Int64),
            Value::String(_) => Some(DataType::String),
            Value::Null => None,
        }
    }

    pub fn compare(&self, other: &Value) -> Option<Ordering> {
        match (self, other) {
            (Value::Int64(a), Value::Int64(b)) => Some(a.cmp(b)),
            (Value::String(a), Value::String(b)) => Some(a.cmp(b)),
            (Value::Null, _) | (_, Value::Null) => None,
            _ => None, // Type mismatch
        }
    }
}
```

**File**: `src/lib.rs`

```rust
pub mod types;
```

**Run**: `cargo test types_tests`

**Expected**: All tests pass - **GREEN** ✓

---

### REFACTOR: Improve Code

- Add doc comments to public types
- Extract constants if needed
- Ensure clippy passes: `cargo clippy`

**Run**: `cargo test types_tests` again

**Expected**: Still green after refactoring ✓

---

## Step 2: Catalog & Schema (TDD)

### RED: Write Failing Tests

**File**: `tests/unit/catalog_tests.rs`

```rust
use ruzu::catalog::{Catalog, NodeTableSchema, ColumnDef};
use ruzu::types::DataType;

#[test]
fn test_create_table_success() {
    let mut catalog = Catalog::new();
    let schema = NodeTableSchema::new(
        "Person".into(),
        vec![
            ColumnDef::new("name".into(), DataType::String).unwrap(),
            ColumnDef::new("age".into(), DataType::Int64).unwrap(),
        ],
        vec!["name".into()],
    ).unwrap();

    let result = catalog.create_table(schema);
    assert!(result.is_ok());
    assert!(catalog.table_exists("Person"));
}

#[test]
fn test_create_duplicate_table_error() {
    let mut catalog = Catalog::new();
    let schema = NodeTableSchema::new(
        "Person".into(),
        vec![ColumnDef::new("name".into(), DataType::String).unwrap()],
        vec!["name".into()],
    ).unwrap();

    catalog.create_table(schema.clone()).unwrap();
    let result = catalog.create_table(schema);
    assert!(result.is_err());
}

#[test]
fn test_schema_validation_duplicate_columns() {
    let result = NodeTableSchema::new(
        "Person".into(),
        vec![
            ColumnDef::new("name".into(), DataType::String).unwrap(),
            ColumnDef::new("name".into(), DataType::Int64).unwrap(), // Duplicate
        ],
        vec!["name".into()],
    );
    assert!(result.is_err());
}

#[test]
fn test_schema_validation_invalid_pk() {
    let result = NodeTableSchema::new(
        "Person".into(),
        vec![ColumnDef::new("name".into(), DataType::String).unwrap()],
        vec!["nonexistent".into()], // PK column doesn't exist
    );
    assert!(result.is_err());
}
```

**Run**: `cargo test catalog_tests`

**Expected**: Compilation errors - **RED** ✓

---

### GREEN: Implement Catalog

**File**: `src/catalog/mod.rs`

```rust
mod schema;

pub use schema::{Catalog, NodeTableSchema, ColumnDef};
```

**File**: `src/catalog/schema.rs`

```rust
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use crate::types::DataType;
use crate::error::{Result, RuzuError};

pub struct Catalog {
    tables: HashMap<String, Arc<NodeTableSchema>>,
}

impl Catalog {
    pub fn new() -> Self {
        Catalog {
            tables: HashMap::new(),
        }
    }

    pub fn create_table(&mut self, schema: NodeTableSchema) -> Result<()> {
        if self.tables.contains_key(&schema.name) {
            return Err(RuzuError::SchemaError(
                format!("Table '{}' already exists", schema.name)
            ));
        }
        self.tables.insert(schema.name.clone(), Arc::new(schema));
        Ok(())
    }

    pub fn get_table(&self, name: &str) -> Option<Arc<NodeTableSchema>> {
        self.tables.get(name).cloned()
    }

    pub fn table_exists(&self, name: &str) -> bool {
        self.tables.contains_key(name)
    }
}

#[derive(Debug, Clone)]
pub struct NodeTableSchema {
    pub name: String,
    pub columns: Vec<ColumnDef>,
    pub primary_key: Vec<String>,
}

impl NodeTableSchema {
    pub fn new(name: String, columns: Vec<ColumnDef>, primary_key: Vec<String>) -> Result<Self> {
        let schema = NodeTableSchema { name, columns, primary_key };
        schema.validate()?;
        Ok(schema)
    }

    fn validate(&self) -> Result<()> {
        if self.columns.is_empty() {
            return Err(RuzuError::SchemaError("Table must have at least one column".into()));
        }

        // Check column name uniqueness
        let mut seen = HashSet::new();
        for col in &self.columns {
            if !seen.insert(&col.name) {
                return Err(RuzuError::SchemaError(
                    format!("Duplicate column name '{}'", col.name)
                ));
            }
        }

        // Check primary key columns exist
        for pk_col in &self.primary_key {
            if !self.columns.iter().any(|c| &c.name == pk_col) {
                return Err(RuzuError::SchemaError(
                    format!("Primary key column '{}' not found in table", pk_col)
                ));
            }
        }

        if self.primary_key.is_empty() {
            return Err(RuzuError::SchemaError("Primary key must specify at least one column".into()));
        }

        Ok(())
    }

    pub fn get_column(&self, name: &str) -> Option<&ColumnDef> {
        self.columns.iter().find(|c| c.name == name)
    }

    pub fn get_column_index(&self, name: &str) -> Option<usize> {
        self.columns.iter().position(|c| c.name == name)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ColumnDef {
    pub name: String,
    pub data_type: DataType,
}

impl ColumnDef {
    pub fn new(name: String, data_type: DataType) -> Result<Self> {
        if name.is_empty() {
            return Err(RuzuError::SchemaError("Column name cannot be empty".into()));
        }
        Ok(ColumnDef { name, data_type })
    }
}
```

**File**: `src/error.rs`

```rust
use thiserror::Error;

#[derive(Debug, Error)]
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

**File**: `src/lib.rs`

```rust
pub mod types;
pub mod catalog;
pub mod error;

pub use error::{RuzuError, Result};
```

**Run**: `cargo test catalog_tests`

**Expected**: All tests pass - **GREEN** ✓

---

## Step 3: Parser (pest grammar)

### Create Grammar File

**File**: `src/parser/grammar.pest`

```pest
WHITESPACE = _{ " " | "\t" | "\n" | "\r" }
COMMENT = _{ "//" ~ (!"\n" ~ ANY)* }

cypher_query = { SOI ~ statement ~ EOI }
statement = { create_node_table | create_node | match_query }

// DDL: CREATE NODE TABLE
create_node_table = {
    ^"CREATE" ~ ^"NODE" ~ ^"TABLE" ~ identifier ~
    "(" ~ column_list ~ ")" ~
    (^"PRIMARY" ~ ^"KEY" ~ "(" ~ identifier_list ~ ")")?
}

column_list = { column_def ~ ("," ~ column_def)* }
column_def = { identifier ~ data_type }
data_type = { ^"STRING" | ^"INT64" }

// DML: CREATE node
create_node = {
    ^"CREATE" ~ node_pattern
}

node_pattern = {
    "(" ~ ":" ~ identifier ~ properties ~ ")"
}

properties = { "{" ~ property_list ~ "}" }
property_list = { property ~ ("," ~ property)* }
property = { identifier ~ ":" ~ literal }

// Query: MATCH ... WHERE ... RETURN
match_query = {
    ^"MATCH" ~ match_pattern ~
    (^"WHERE" ~ expression)? ~
    ^"RETURN" ~ projection_list
}

match_pattern = {
    "(" ~ identifier ~ ":" ~ identifier ~ ")"
}

projection_list = { projection ~ ("," ~ projection)* }
projection = { identifier ~ "." ~ identifier }

// Expressions
expression = { comparison }
comparison = { projection ~ comparison_op ~ literal }
comparison_op = { ">" | "<" | "=" | ">=" | "<=" | "<>" }

// Literals
literal = { string_literal | integer_literal }
string_literal = @{ "'" ~ (!"'" ~ ANY)* ~ "'" }
integer_literal = @{ "-"? ~ ASCII_DIGIT+ }

// Identifiers
identifier = @{ (ASCII_ALPHA | "_") ~ (ASCII_ALPHANUMERIC | "_")* }
identifier_list = { identifier ~ ("," ~ identifier)* }
```

### RED: Write Parser Tests

**File**: `tests/unit/parser_tests.rs`

```rust
use ruzu::parser::parse_query;

#[test]
fn test_parse_create_node_table() {
    let query = "CREATE NODE TABLE Person(name STRING, age INT64, PRIMARY KEY(name))";
    let result = parse_query(query);
    assert!(result.is_ok());
}

#[test]
fn test_parse_create_node() {
    let query = "CREATE (:Person {name: 'Alice', age: 25})";
    let result = parse_query(query);
    assert!(result.is_ok());
}

#[test]
fn test_parse_match_query() {
    let query = "MATCH (p:Person) WHERE p.age > 20 RETURN p.name, p.age";
    let result = parse_query(query);
    assert!(result.is_ok());
}

#[test]
fn test_parse_invalid_syntax() {
    let query = "MATCH (p:Person";  // Missing closing paren
    let result = parse_query(query);
    assert!(result.is_err());
}
```

### GREEN: Implement Parser

**File**: `src/parser/mod.rs`

```rust
use pest::Parser;
use pest_derive::Parser;
use crate::error::{Result, RuzuError};

#[derive(Parser)]
#[grammar = "parser/grammar.pest"]
pub struct CypherParser;

pub mod ast;

pub fn parse_query(query: &str) -> Result<ast::Statement> {
    let pairs = CypherParser::parse(Rule::cypher_query, query)
        .map_err(|e| RuzuError::ParseError {
            line: e.line(),
            col: e.location.col(),
            message: e.variant.message().to_string(),
        })?;

    // Parse pairs into AST (implement ast module next)
    ast::build_ast(pairs)
}
```

**File**: `src/parser/ast.rs`

```rust
use pest::iterators::Pairs;
use crate::error::Result;
use crate::parser::Rule;

#[derive(Debug, Clone)]
pub enum Statement {
    CreateNodeTable {
        table_name: String,
        columns: Vec<(String, String)>, // (name, type)
        primary_key: Vec<String>,
    },
    CreateNode {
        label: String,
        properties: Vec<(String, Literal)>,
    },
    Match {
        var: String,
        label: String,
        filter: Option<Expression>,
        projections: Vec<(String, String)>, // (var, property)
    },
}

#[derive(Debug, Clone)]
pub enum Literal {
    String(String),
    Int64(i64),
}

#[derive(Debug, Clone)]
pub struct Expression {
    pub var: String,
    pub property: String,
    pub op: ComparisonOp,
    pub value: Literal,
}

#[derive(Debug, Clone)]
pub enum ComparisonOp {
    Gt, Lt, Eq, Gte, Lte, Neq,
}

pub fn build_ast(pairs: Pairs<Rule>) -> Result<Statement> {
    // Implementation: Walk pest Pairs and construct Statement
    // This is tedious but straightforward - match on rules and extract values
    // See pest documentation for examples
    todo!("Implement AST builder")
}
```

**File**: `src/lib.rs` (add parser module)

```rust
pub mod types;
pub mod catalog;
pub mod error;
pub mod parser;

pub use error::{RuzuError, Result};
```

**Note**: Full AST builder implementation is lengthy. For quickstart, focus on getting tests to pass incrementally.

---

## Step 4: Storage (Columnar Vec-based)

Continue TDD cycle for:
- `ColumnStorage` (Vec<Value>)
- `NodeTable` (insert, scan, primary key index)
- Row iteration

**Follow same RED-GREEN-REFACTOR pattern.**

---

## Step 5: Executor (Scan, Filter, Project)

Implement operators:
- `ScanOperator` - yields all rows from table
- `FilterOperator` - filters based on WHERE expression
- `ProjectOperator` - projects columns for RETURN

**Follow TDD pattern with integration tests.**

---

## Step 6: End-to-End Integration

### Contract Test (Target Query)

**File**: `tests/contract/test_query_api.rs`

```rust
use ruzu::Database;

#[test]
fn test_target_query_end_to_end() {
    let mut db = Database::new();

    // DDL
    db.execute("CREATE NODE TABLE Person(name STRING, age INT64, PRIMARY KEY(name))").unwrap();

    // DML
    db.execute("CREATE (:Person {name: 'Alice', age: 25})").unwrap();
    db.execute("CREATE (:Person {name: 'Bob', age: 30})").unwrap();
    db.execute("CREATE (:Person {name: 'Charlie', age: 20})").unwrap();

    // Query
    let result = db.execute("MATCH (p:Person) WHERE p.age > 20 RETURN p.name, p.age").unwrap();

    // Verify
    assert_eq!(result.row_count(), 2); // Alice (25) and Bob (30)
    assert_eq!(result.columns, vec!["p.name", "p.age"]);

    let row0 = result.get_row(0).unwrap();
    let name0 = row0.get("p.name").unwrap();
    assert!(matches!(name0, Value::String(_)));
}
```

**Run**: `cargo test test_target_query_end_to_end`

**Expected**: This test passes when ALL components are implemented ✓

---

## Step 7: Benchmarking

### Create C++ Baseline

```bash
cd C:\dev\kuzu\build\Release
.\kuzu.exe
```

**In KuzuDB shell**:
```cypher
CREATE NODE TABLE Person(name STRING, age INT64, PRIMARY KEY(name));
CREATE (:Person {name: 'Person_0', age: 0});
-- (Insert 1000 nodes via script)
.timer on
MATCH (p:Person) WHERE p.age > 20 RETURN p.name, p.age;
-- Record time
```

**Document baseline**: e.g., "C++ KuzuDB: 5ms total time"

### Create Rust Benchmark

**File**: `benches/e2e_benchmark.rs`

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use ruzu::Database;

fn setup_database() -> Database {
    let mut db = Database::new();
    db.execute("CREATE NODE TABLE Person(name STRING, age INT64, PRIMARY KEY(name))").unwrap();
    for i in 0..1000 {
        db.execute(&format!("CREATE (:Person {{name: 'Person_{}', age: {}}})", i, i)).unwrap();
    }
    db
}

fn bench_target_query(c: &mut Criterion) {
    let mut db = setup_database();
    c.bench_function("target_query_1000_nodes", |b| {
        b.iter(|| {
            db.execute(black_box("MATCH (p:Person) WHERE p.age > 20 RETURN p.name, p.age")).unwrap()
        })
    });
}

criterion_group!(benches, bench_target_query);
criterion_main!(benches);
```

**Run**: `cargo bench`

**Expected**: Benchmark report with time per iteration

**Compare**: Rust time should be <10x C++ baseline (e.g., <50ms if C++ is 5ms)

---

## Step 8: Documentation & Polish

### Add README Example

**File**: `README.md`

```markdown
# ruzu - Rust Graph Database (Phase 0 PoC)

## Quick Example

\`\`\`rust
use ruzu::Database;

fn main() {
    let mut db = Database::new();

    // Create schema
    db.execute("CREATE NODE TABLE Person(name STRING, age INT64, PRIMARY KEY(name))").unwrap();

    // Insert data
    db.execute("CREATE (:Person {name: 'Alice', age: 25})").unwrap();
    db.execute("CREATE (:Person {name: 'Bob', age: 30})").unwrap();

    // Query
    let result = db.execute("MATCH (p:Person) WHERE p.age > 20 RETURN p.name, p.age").unwrap();

    for row in &result.rows {
        let name = row.get("p.name").unwrap();
        let age = row.get("p.age").unwrap();
        println!("{:?}: {:?}", name, age);
    }
}
\`\`\`
```

### Run Clippy

```bash
cargo clippy --all-targets --all-features -- -D warnings
```

**Fix all warnings.**

### Run Formatter

```bash
cargo fmt -- --check
cargo fmt  # Apply formatting
```

---

## Success Checklist

Phase 0 is complete when:

- ✅ All contract tests pass (13 tests from spec)
- ✅ Integration test for target query passes
- ✅ Benchmarks run and show <10x C++ baseline
- ✅ Zero clippy warnings
- ✅ Code formatted with rustfmt
- ✅ README with example usage
- ✅ Memory usage <10MB for 1000 nodes (verify with profiler)

---

## Next Steps

After Phase 0:
1. Create GitHub PR with Phase 0 implementation
2. Run `/speckit.tasks` to generate Phase 1 task breakdown
3. Begin Phase 1: Persistent Storage (disk-based storage, buffer manager, WAL)

---

## Troubleshooting

**Issue**: Tests fail with parse errors
- **Solution**: Check `grammar.pest` syntax, run `cargo build` to see pest errors

**Issue**: Benchmark is >10x slower
- **Solution**: Profile with `cargo flamegraph`, identify bottleneck (likely parser or storage scan)

**Issue**: Memory usage exceeds 10MB
- **Solution**: Check for Vec reallocations, ensure columns don't over-allocate

**Issue**: Clippy warnings
- **Solution**: Address each warning, use `#[allow(clippy::...)]` only if justified

---

**Document Status**: ✅ Complete
**Estimated Time**: 6-8 weeks following this guide with TDD discipline
**Next Phase**: Phase 1 planning after PoC validation
