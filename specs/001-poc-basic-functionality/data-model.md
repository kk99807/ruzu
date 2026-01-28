# Data Model: Phase 0 Proof of Concept

**Feature**: Phase 0 Proof of Concept - Basic Graph Database
**Date**: 2025-12-05
**Status**: Complete

This document defines the core data entities and their relationships for the Phase 0 proof-of-concept implementation.

---

## Entity Relationship Overview

```
┌─────────────────┐
│    Catalog      │ 1:N relationship with NodeTableSchema
│                 │
│ - tables: Map  │────┐
└─────────────────┘    │
                       │
                       ▼
              ┌─────────────────────┐
              │ NodeTableSchema     │ 1:N relationship with ColumnDef
              │                     │
              │ - name: String      │───┐
              │ - columns: Vec      │   │
              │ - primary_key: Vec  │   │
              └─────────────────────┘   │
                       │                │
                       │ 1:1            ▼
                       │       ┌──────────────┐
                       │       │  ColumnDef   │
                       │       │              │
                       │       │ - name       │
                       │       │ - data_type  │
                       ▼       └──────────────┘
              ┌─────────────────────┐
              │   NodeTable         │ Stores actual node data
              │                     │
              │ - schema: Schema    │
              │ - columns: Vec      │
              │ - row_count: usize  │
              └─────────────────────┘
                       │
                       │ 1:N
                       ▼
              ┌─────────────────────┐
              │   ColumnStorage     │ Columnar data storage
              │                     │
              │ - data: Vec<Value>  │
              └─────────────────────┘
```

---

## Core Entities

### 1. Catalog

**Purpose**: Central registry of all table schemas in the database

**Attributes**:
| Attribute | Type | Description | Validation |
|-----------|------|-------------|------------|
| `tables` | `HashMap<String, Arc<NodeTableSchema>>` | Map of table name to schema | Keys are case-sensitive identifiers |

**Responsibilities**:
- Store and retrieve table schemas by name
- Enforce unique table names
- Provide table existence checks

**Operations**:
- `create_table(schema: NodeTableSchema) -> Result<()>` - Register new table, error if duplicate
- `get_table(name: &str) -> Option<Arc<NodeTableSchema>>` - Retrieve table schema
- `table_exists(name: &str) -> bool` - Check if table exists

**Validation Rules**:
- Table names must be unique (case-sensitive)
- Table names must be valid identifiers (alphanumeric + underscore, not starting with digit)

**State Transitions**: N/A (immutable after table creation in Phase 0)

**Rust Implementation**:
```rust
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
```

---

### 2. NodeTableSchema

**Purpose**: Defines the structure of a node table (column definitions and primary key)

**Attributes**:
| Attribute | Type | Description | Validation |
|-----------|------|-------------|------------|
| `name` | `String` | Table name | Non-empty, unique in catalog |
| `columns` | `Vec<ColumnDef>` | Ordered list of column definitions | At least 1 column |
| `primary_key` | `Vec<String>` | Column names forming primary key | All names must exist in columns |

**Responsibilities**:
- Define table structure
- Validate column names are unique within table
- Validate primary key references existing columns
- Provide column lookup by name

**Operations**:
- `new(name: String, columns: Vec<ColumnDef>, primary_key: Vec<String>) -> Result<Self>` - Create schema with validation
- `get_column(&self, name: &str) -> Option<&ColumnDef>` - Find column by name
- `get_column_index(&self, name: &str) -> Option<usize>` - Find column index
- `validate(&self) -> Result<()>` - Validate schema invariants

**Validation Rules**:
- Column names must be unique within table
- Primary key columns must exist in column list
- At least one column required
- Primary key must have at least one column (Phase 0 requirement)

**Rust Implementation**:
```rust
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
        // Check at least one column
        if self.columns.is_empty() {
            return Err(RuzuError::SchemaError("Table must have at least one column".into()));
        }

        // Check column names are unique
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

        // Check primary key not empty
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
```

---

### 3. ColumnDef

**Purpose**: Defines a single column in a table (name and data type)

**Attributes**:
| Attribute | Type | Description | Validation |
|-----------|------|-------------|------------|
| `name` | `String` | Column name | Non-empty, valid identifier |
| `data_type` | `DataType` | Column data type | Must be INT64 or STRING in Phase 0 |

**Responsibilities**:
- Store column metadata
- Validate column names

**Validation Rules**:
- Column name must be valid identifier (alphanumeric + underscore, not starting with digit)
- Data type must be supported (INT64 or STRING in Phase 0)

**Rust Implementation**:
```rust
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
        // Additional validation: valid identifier
        if !is_valid_identifier(&name) {
            return Err(RuzuError::SchemaError(
                format!("Invalid column name '{}'", name)
            ));
        }
        Ok(ColumnDef { name, data_type })
    }
}

fn is_valid_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    if let Some(first) = chars.next() {
        first.is_alphabetic() || first == '_'
    } && chars.all(|c| c.is_alphanumeric() || c == '_')
    } else {
        false
    }
}
```

---

### 4. DataType

**Purpose**: Enum representing supported data types

**Variants**:
| Variant | Description | Storage Size |
|---------|-------------|--------------|
| `Int64` | 64-bit signed integer | 8 bytes |
| `String` | UTF-8 string (heap-allocated) | 24 bytes (String struct) + heap allocation |

**Rust Implementation**:
```rust
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
```

**Future Extensions (Phase 1+)**:
- Bool, Float, Double
- Date, Timestamp, Interval
- UUID, List, Struct, Map

---

### 5. Value

**Purpose**: Runtime value container for data

**Variants**:
| Variant | Description | Use Case |
|---------|-------------|----------|
| `Int64(i64)` | Integer value | Age, count, ID |
| `String(String)` | String value | Name, label, text |
| `Null` | Null value | Missing data |

**Responsibilities**:
- Store typed data
- Provide type-safe comparisons for WHERE clauses
- Support null semantics

**Operations**:
- `data_type(&self) -> Option<DataType>` - Get type of value (None for Null)
- `compare(&self, other: &Value) -> Option<Ordering>` - Type-safe comparison
- `is_null(&self) -> bool` - Check if null

**Validation Rules**:
- Comparisons between different types return `None` (type error)
- Null compared to anything returns `None` (SQL null semantics)

**Rust Implementation**:
```rust
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int64(i64),
    String(String),
    Null,
}

impl Value {
    pub fn data_type(&self) -> Option<DataType> {
        match self {
            Value::Int64(_) => Some(DataType::Int64),
            Value::String(_) => Some(DataType::String),
            Value::Null => None,
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    pub fn compare(&self, other: &Value) -> Option<Ordering> {
        match (self, other) {
            (Value::Int64(a), Value::Int64(b)) => Some(a.cmp(b)),
            (Value::String(a), Value::String(b)) => Some(a.cmp(b)),
            (Value::Null, _) | (_, Value::Null) => None, // SQL null semantics
            _ => None, // Type mismatch
        }
    }
}
```

---

### 6. NodeTable

**Purpose**: Stores actual node data in columnar format

**Attributes**:
| Attribute | Type | Description | Validation |
|-----------|------|-------------|------------|
| `schema` | `Arc<NodeTableSchema>` | Table schema | Immutable reference |
| `columns` | `Vec<ColumnStorage>` | Column data (one per schema column) | Length matches schema.columns.len() |
| `row_count` | `usize` | Number of rows | All columns must have same row_count |

**Responsibilities**:
- Store columnar data
- Enforce schema constraints (type matching, primary key uniqueness)
- Provide row insertion and scanning

**Operations**:
- `new(schema: Arc<NodeTableSchema>) -> Self` - Create empty table
- `insert(&mut self, row: HashMap<String, Value>) -> Result<()>` - Insert row with validation
- `scan(&self) -> RowIterator` - Scan all rows
- `get_column(&self, index: usize) -> &ColumnStorage` - Access column data

**Validation Rules**:
- Inserted row must have values for all columns (or use default/null)
- Values must match column data types
- Primary key values must be unique
- All columns must have same row count (invariant)

**Rust Implementation**:
```rust
pub struct NodeTable {
    schema: Arc<NodeTableSchema>,
    columns: Vec<ColumnStorage>,
    row_count: usize,
    pk_index: HashMap<Vec<Value>, usize>, // Primary key -> row index for uniqueness check
}

impl NodeTable {
    pub fn new(schema: Arc<NodeTableSchema>) -> Self {
        let columns = schema.columns.iter()
            .map(|_| ColumnStorage::new())
            .collect();
        NodeTable {
            schema,
            columns,
            row_count: 0,
            pk_index: HashMap::new(),
        }
    }

    pub fn insert(&mut self, row: HashMap<String, Value>) -> Result<()> {
        // Validate all columns present
        for col_def in &self.schema.columns {
            if !row.contains_key(&col_def.name) {
                return Err(RuzuError::SchemaError(
                    format!("Missing value for column '{}'", col_def.name)
                ));
            }
        }

        // Validate types
        for (col_name, value) in &row {
            let col_def = self.schema.get_column(col_name)
                .ok_or_else(|| RuzuError::SchemaError(
                    format!("Unknown column '{}'", col_name)
                ))?;

            if !value.is_null() {
                if let Some(val_type) = value.data_type() {
                    if val_type != col_def.data_type {
                        return Err(RuzuError::TypeError {
                            expected: col_def.data_type.name().into(),
                            actual: val_type.name().into(),
                        });
                    }
                }
            }
        }

        // Extract primary key values
        let pk_values: Vec<Value> = self.schema.primary_key.iter()
            .map(|col_name| row.get(col_name).unwrap().clone())
            .collect();

        // Check primary key uniqueness
        if self.pk_index.contains_key(&pk_values) {
            return Err(RuzuError::ConstraintViolation(
                format!("Duplicate primary key: {:?}", pk_values)
            ));
        }

        // Insert values into columns
        for (i, col_def) in self.schema.columns.iter().enumerate() {
            let value = row.get(&col_def.name).unwrap().clone();
            self.columns[i].push(value);
        }

        // Update primary key index
        self.pk_index.insert(pk_values, self.row_count);
        self.row_count += 1;

        Ok(())
    }

    pub fn scan(&self) -> RowIterator {
        RowIterator::new(self)
    }

    pub fn get_column(&self, index: usize) -> &ColumnStorage {
        &self.columns[index]
    }
}
```

---

### 7. ColumnStorage

**Purpose**: Simple columnar storage using Vec<Value>

**Attributes**:
| Attribute | Type | Description |
|-----------|------|-------------|
| `data` | `Vec<Value>` | Column values in row order |

**Responsibilities**:
- Store column data contiguously
- Provide indexed access to values

**Operations**:
- `new() -> Self` - Create empty column
- `push(&mut self, value: Value)` - Append value
- `get(&self, index: usize) -> Option<&Value>` - Get value by row index
- `len(&self) -> usize` - Number of values

**Rust Implementation**:
```rust
pub struct ColumnStorage {
    data: Vec<Value>,
}

impl ColumnStorage {
    pub fn new() -> Self {
        ColumnStorage { data: Vec::new() }
    }

    pub fn push(&mut self, value: Value) {
        self.data.push(value);
    }

    pub fn get(&self, index: usize) -> Option<&Value> {
        self.data.get(index)
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}
```

---

### 8. Row

**Purpose**: Represents a single row result from query execution

**Attributes**:
| Attribute | Type | Description |
|-----------|------|-------------|
| `values` | `HashMap<String, Value>` | Column name to value mapping |

**Responsibilities**:
- Store query result row
- Provide value access by column name

**Operations**:
- `new() -> Self` - Create empty row
- `set(&mut self, column: String, value: Value)` - Set column value
- `get(&self, column: &str) -> Option<&Value>` - Get value by column name

**Rust Implementation**:
```rust
#[derive(Debug, Clone)]
pub struct Row {
    values: HashMap<String, Value>,
}

impl Row {
    pub fn new() -> Self {
        Row { values: HashMap::new() }
    }

    pub fn set(&mut self, column: String, value: Value) {
        self.values.insert(column, value);
    }

    pub fn get(&self, column: &str) -> Option<&Value> {
        self.values.get(column)
    }
}
```

---

### 9. QueryResult

**Purpose**: Result of query execution containing rows and metadata

**Attributes**:
| Attribute | Type | Description |
|-----------|------|-------------|
| `columns` | `Vec<String>` | Ordered list of column names |
| `rows` | `Vec<Row>` | Result rows |

**Responsibilities**:
- Store query results
- Provide iteration over rows
- Support formatting for display

**Operations**:
- `new(columns: Vec<String>) -> Self` - Create empty result with schema
- `add_row(&mut self, row: Row)` - Append row
- `row_count(&self) -> usize` - Number of rows

**Rust Implementation**:
```rust
#[derive(Debug)]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Row>,
}

impl QueryResult {
    pub fn new(columns: Vec<String>) -> Self {
        QueryResult {
            columns,
            rows: Vec::new(),
        }
    }

    pub fn add_row(&mut self, row: Row) {
        self.rows.push(row);
    }

    pub fn row_count(&self) -> usize {
        self.rows.len()
    }
}
```

---

## Data Flow Examples

### Example 1: CREATE NODE TABLE

**Input Query**:
```cypher
CREATE NODE TABLE Person(name STRING, age INT64, PRIMARY KEY(name));
```

**Data Flow**:
1. Parser produces AST with table name, columns, primary key
2. Binder creates `NodeTableSchema`:
   - `name = "Person"`
   - `columns = [ColumnDef("name", DataType::String), ColumnDef("age", DataType::Int64)]`
   - `primary_key = ["name"]`
3. Schema validation executes
4. Catalog stores schema: `catalog.create_table(schema)`
5. NodeTable created: `tables.insert("Person", NodeTable::new(schema))`

### Example 2: CREATE Node

**Input Query**:
```cypher
CREATE (:Person {name: 'Alice', age: 25});
```

**Data Flow**:
1. Parser produces AST with label "Person" and properties
2. Binder resolves table schema from catalog
3. Executor creates row:
   - `row = {"name": Value::String("Alice"), "age": Value::Int64(25)}`
4. Table validation:
   - Check all columns present ✓
   - Check type matching ✓
   - Check primary key uniqueness ✓
5. Insert into columns:
   - `columns[0].push(Value::String("Alice"))`
   - `columns[1].push(Value::Int64(25))`
6. Update pk_index: `pk_index.insert([Value::String("Alice")], 0)`

### Example 3: MATCH Query

**Input Query**:
```cypher
MATCH (p:Person) WHERE p.age > 20 RETURN p.name, p.age;
```

**Data Flow**:
1. Parser produces AST with pattern, filter, projection
2. Binder resolves table "Person", validates properties exist
3. Executor builds pipeline:
   - `ScanOperator(Person)` → yields all rows
   - `FilterOperator(age > 20)` → filters rows
   - `ProjectOperator([name, age])` → projects columns
4. Execution:
   - Scan row 0: `{"name": "Alice", "age": 25}`
   - Filter: 25 > 20 → true, pass row
   - Project: `{"name": "Alice", "age": 25}`
   - Add to QueryResult
5. Return QueryResult with columns ["name", "age"] and matching rows

---

## Validation Summary

| Entity | Key Invariants |
|--------|----------------|
| **Catalog** | Unique table names, case-sensitive |
| **NodeTableSchema** | Unique column names, valid primary key references, at least 1 column |
| **ColumnDef** | Valid identifier name, supported data type |
| **NodeTable** | All columns same row count, primary key uniqueness, type matching |
| **Value** | Type-safe comparisons, null semantics |

---

## Performance Considerations

**Memory Estimate for 1000 Nodes** (2 columns: name STRING, age INT64):
- String column: 1000 × ~50 bytes average = ~50KB
- Int64 column: 1000 × 8 bytes = 8KB
- Primary key index: 1000 × (24 bytes String + 8 bytes usize) = ~32KB
- **Total**: ~90KB (well within 10MB constraint)

**Storage Layout**:
- Columnar storage enables efficient WHERE clause filtering (sequential access to single column)
- Primary key HashMap provides O(1) uniqueness check on insert
- No indexes in Phase 0, so queries are full table scans

---

**Document Status**: ✅ Complete
**Next Step**: Generate contracts/ (API specifications)
**Dependencies**: All entities defined and validated
