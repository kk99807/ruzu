# Database API Contract

**Version**: 1.0.0
**Date**: 2025-12-05
**Status**: Draft

This document defines the public API contract for the ruzu embedded graph database library (Phase 0).

---

## Public API Surface

### Module: `ruzu`

Top-level library entry point.

```rust
pub struct Database { /* ... */ }
pub struct QueryResult { /* ... */ }
pub type Result<T> = std::result::Result<T, RuzuError>;

pub enum RuzuError { /* ... */ }
```

---

## 1. Database

### Constructor

```rust
impl Database {
    pub fn new() -> Self
}
```

**Description**: Creates a new in-memory database instance.

**Parameters**: None

**Returns**: `Database` instance

**Behavior**:
- Initializes empty catalog
- No tables exist initially
- Thread-safe for single-threaded use (Phase 0 does not support concurrent access)

**Example**:
```rust
use ruzu::Database;

let db = Database::new();
```

---

### Method: `execute`

```rust
impl Database {
    pub fn execute(&mut self, query: &str) -> Result<QueryResult>
}
```

**Description**: Executes a Cypher query and returns results.

**Parameters**:
- `query: &str` - Cypher query string

**Returns**: `Result<QueryResult>`
- `Ok(QueryResult)` - Query executed successfully
- `Err(RuzuError)` - Parse error, schema error, type error, constraint violation, or execution error

**Supported Queries** (Phase 0):
1. **DDL**: `CREATE NODE TABLE <name>(<columns>) [PRIMARY KEY(<pk_cols>)]`
2. **DML**: `CREATE (:<label> {<properties>})`
3. **Query**: `MATCH (<var>:<label>) [WHERE <condition>] RETURN <projections>`

**Behavior**:
- Parses query string to AST
- Validates against schema (binder phase)
- Executes query (executor phase)
- Returns results or error with detailed message

**Error Cases**:
| Error Type | Condition | Example |
|------------|-----------|---------|
| `ParseError` | Invalid syntax | `MATCH (p:Person` (missing closing paren) |
| `SchemaError` | Table/column not found | `MATCH (p:NonExistent) RETURN p.name` |
| `TypeError` | Type mismatch | `CREATE (:Person {name: 123, age: "twenty"})` |
| `ConstraintViolation` | Primary key duplicate | `CREATE (:Person {name: 'Alice', age: 25})` twice |
| `ExecutionError` | Runtime error | Internal error during execution |

**Examples**:

**Example 1: DDL Success**
```rust
let mut db = Database::new();
let result = db.execute(
    "CREATE NODE TABLE Person(name STRING, age INT64, PRIMARY KEY(name))"
);
assert!(result.is_ok());
```

**Example 2: DML Success**
```rust
let mut db = Database::new();
db.execute("CREATE NODE TABLE Person(name STRING, age INT64, PRIMARY KEY(name))").unwrap();
let result = db.execute("CREATE (:Person {name: 'Alice', age: 25})");
assert!(result.is_ok());
```

**Example 3: Query Success**
```rust
let mut db = Database::new();
db.execute("CREATE NODE TABLE Person(name STRING, age INT64, PRIMARY KEY(name))").unwrap();
db.execute("CREATE (:Person {name: 'Alice', age: 25})").unwrap();
db.execute("CREATE (:Person {name: 'Bob', age: 30})").unwrap();

let result = db.execute("MATCH (p:Person) WHERE p.age > 20 RETURN p.name, p.age").unwrap();
assert_eq!(result.row_count(), 2);
assert_eq!(result.columns, vec!["p.name", "p.age"]);
```

**Example 4: Parse Error**
```rust
let mut db = Database::new();
let result = db.execute("MATCH (p:Person");
assert!(matches!(result, Err(RuzuError::ParseError { .. })));
```

**Example 5: Schema Error**
```rust
let mut db = Database::new();
db.execute("CREATE NODE TABLE Person(name STRING, age INT64, PRIMARY KEY(name))").unwrap();
let result = db.execute("MATCH (p:NonExistent) RETURN p.name");
assert!(matches!(result, Err(RuzuError::SchemaError(_))));
```

**Example 6: Constraint Violation**
```rust
let mut db = Database::new();
db.execute("CREATE NODE TABLE Person(name STRING, age INT64, PRIMARY KEY(name))").unwrap();
db.execute("CREATE (:Person {name: 'Alice', age: 25})").unwrap();
let result = db.execute("CREATE (:Person {name: 'Alice', age: 30})");
assert!(matches!(result, Err(RuzuError::ConstraintViolation(_))));
```

---

## 2. QueryResult

### Structure

```rust
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Row>,
}
```

**Description**: Result of query execution.

**Fields**:
- `columns: Vec<String>` - Ordered list of result column names
- `rows: Vec<Row>` - Result rows

---

### Method: `row_count`

```rust
impl QueryResult {
    pub fn row_count(&self) -> usize
}
```

**Description**: Returns the number of rows in the result.

**Returns**: `usize` - Number of rows

**Example**:
```rust
let result = db.execute("MATCH (p:Person) RETURN p.name").unwrap();
println!("Returned {} rows", result.row_count());
```

---

### Method: `get_row`

```rust
impl QueryResult {
    pub fn get_row(&self, index: usize) -> Option<&Row>
}
```

**Description**: Returns a reference to a row by index.

**Parameters**:
- `index: usize` - Zero-based row index

**Returns**: `Option<&Row>`
- `Some(&Row)` if index valid
- `None` if index out of bounds

**Example**:
```rust
let result = db.execute("MATCH (p:Person) RETURN p.name, p.age").unwrap();
if let Some(row) = result.get_row(0) {
    let name = row.get("p.name").unwrap();
    println!("First person: {:?}", name);
}
```

---

## 3. Row

### Structure

```rust
pub struct Row {
    // Internal: HashMap<String, Value>
}
```

**Description**: A single result row containing column values.

---

### Method: `get`

```rust
impl Row {
    pub fn get(&self, column: &str) -> Option<&Value>
}
```

**Description**: Retrieves value for a column.

**Parameters**:
- `column: &str` - Column name (as appears in QueryResult.columns)

**Returns**: `Option<&Value>`
- `Some(&Value)` if column exists
- `None` if column not found

**Example**:
```rust
let result = db.execute("MATCH (p:Person) RETURN p.name, p.age").unwrap();
let row = result.get_row(0).unwrap();
let name = row.get("p.name").unwrap();
let age = row.get("p.age").unwrap();

match name {
    Value::String(s) => println!("Name: {}", s),
    _ => panic!("Expected string"),
}

match age {
    Value::Int64(i) => println!("Age: {}", i),
    _ => panic!("Expected int64"),
}
```

---

## 4. Value

### Enum

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int64(i64),
    String(String),
    Null,
}
```

**Description**: Runtime value type.

**Variants**:
- `Int64(i64)` - 64-bit signed integer
- `String(String)` - UTF-8 string
- `Null` - SQL null value

---

### Method: `is_null`

```rust
impl Value {
    pub fn is_null(&self) -> bool
}
```

**Description**: Checks if value is null.

**Returns**: `bool` - true if Null, false otherwise

**Example**:
```rust
let value = Value::Null;
assert!(value.is_null());

let value = Value::Int64(42);
assert!(!value.is_null());
```

---

### Method: `as_int64`

```rust
impl Value {
    pub fn as_int64(&self) -> Option<i64>
}
```

**Description**: Extracts i64 value if variant is Int64.

**Returns**: `Option<i64>`
- `Some(i64)` if variant is Int64
- `None` otherwise

**Example**:
```rust
let value = Value::Int64(42);
assert_eq!(value.as_int64(), Some(42));

let value = Value::String("hello".into());
assert_eq!(value.as_int64(), None);
```

---

### Method: `as_string`

```rust
impl Value {
    pub fn as_string(&self) -> Option<&str>
}
```

**Description**: Extracts string slice if variant is String.

**Returns**: `Option<&str>`
- `Some(&str)` if variant is String
- `None` otherwise

**Example**:
```rust
let value = Value::String("Alice".into());
assert_eq!(value.as_string(), Some("Alice"));

let value = Value::Int64(42);
assert_eq!(value.as_string(), None);
```

---

## 5. RuzuError

### Enum

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
```

**Description**: Error types for database operations.

**Variants**:

**ParseError**:
- Raised when Cypher query has invalid syntax
- Includes line, column, and descriptive message
- Example: `Parse error at line 1, column 15: expected ')' but found EOF`

**SchemaError**:
- Raised when referencing non-existent table or column
- Raised when creating duplicate table
- Example: `Schema error: Table 'Person' already exists`

**TypeError**:
- Raised when value type doesn't match schema
- Raised when comparing incompatible types
- Example: `Type error: expected INT64, got STRING`

**ConstraintViolation**:
- Raised when primary key uniqueness violated
- Example: `Constraint violation: Duplicate primary key: ["Alice"]`

**ExecutionError**:
- Raised for internal execution errors
- Example: `Execution error: division by zero` (Phase 1+)

---

## Cypher Query Grammar (Phase 0 Subset)

### CREATE NODE TABLE

**Syntax**:
```cypher
CREATE NODE TABLE <table_name> ( <column_definitions> ) [ PRIMARY KEY ( <pk_columns> ) ]
```

**Components**:
- `<table_name>`: Identifier (alphanumeric + underscore, not starting with digit)
- `<column_definitions>`: Comma-separated list of `<name> <type>`
- `<type>`: `STRING` or `INT64`
- `<pk_columns>`: Comma-separated list of column names

**Examples**:
```cypher
CREATE NODE TABLE Person(name STRING, age INT64, PRIMARY KEY(name))
CREATE NODE TABLE Product(id INT64, name STRING, price INT64, PRIMARY KEY(id))
```

---

### CREATE (node)

**Syntax**:
```cypher
CREATE ( : <label> { <properties> } )
```

**Components**:
- `<label>`: Node table name
- `<properties>`: Comma-separated `<name>: <value>` pairs

**Supported Values**:
- String literals: `'value'` (single quotes only)
- Integer literals: `123`, `-456`

**Examples**:
```cypher
CREATE (:Person {name: 'Alice', age: 25})
CREATE (:Product {id: 1, name: 'Widget', price: 999})
```

---

### MATCH ... WHERE ... RETURN

**Syntax**:
```cypher
MATCH ( <var> : <label> ) [ WHERE <condition> ] RETURN <projections>
```

**Components**:
- `<var>`: Variable name for node
- `<label>`: Node table name
- `<condition>`: Boolean expression with comparisons
- `<projections>`: Comma-separated `<var>.<property>` references

**Supported Operators** (WHERE clause):
- Comparison: `>`, `<`, `=`, `>=`, `<=`, `<>`
- Logical: `AND`, `OR`, `NOT` (Phase 1+, PoC may support only simple comparisons)

**Examples**:
```cypher
MATCH (p:Person) RETURN p.name, p.age
MATCH (p:Person) WHERE p.age > 20 RETURN p.name
MATCH (p:Person) WHERE p.name = 'Alice' RETURN p.age
```

---

## Contract Tests

All acceptance scenarios from the spec translate to contract tests:

**Test File**: `tests/contract/test_query_api.rs`

**Test Coverage**:
- ✅ User Story 1: Schema Definition (4 tests)
- ✅ User Story 2: Insert Data (4 tests)
- ✅ User Story 3: Query Data (5 tests)
- ✅ Total: 13 contract tests

**Example Test**:
```rust
#[test]
fn test_create_node_table_success() {
    let mut db = Database::new();
    let result = db.execute(
        "CREATE NODE TABLE Person(name STRING, age INT64, PRIMARY KEY(name))"
    );
    assert!(result.is_ok());
}

#[test]
fn test_create_duplicate_table_error() {
    let mut db = Database::new();
    db.execute("CREATE NODE TABLE Person(name STRING, age INT64, PRIMARY KEY(name))").unwrap();
    let result = db.execute("CREATE NODE TABLE Person(name STRING, age INT64, PRIMARY KEY(name))");
    assert!(matches!(result, Err(RuzuError::SchemaError(_))));
}
```

---

## API Stability Guarantees

**Phase 0 (PoC)**:
- API is **unstable** and may change between commits
- No semantic versioning guarantees
- Breaking changes allowed without notice

**Phase 1+**:
- API stabilizes with semantic versioning
- Breaking changes only in MAJOR version bumps
- Deprecation warnings for API changes

---

## Performance Contracts

**Phase 0 Targets** (from spec):
- Parse time: < 10ms per query
- Execution time: < 100ms for 1000 node scan + filter
- Total end-to-end: < 200ms for target query
- Memory usage: < 10MB for 1000 nodes

**No Guarantees**:
- Thread safety (single-threaded only in Phase 0)
- Concurrent access (undefined behavior)
- Performance on large datasets (>10,000 nodes not tested)

---

**Document Status**: ✅ Complete
**Test Implementation**: Required for Phase 0 completion
**Dependencies**: None (top-level API contract)
