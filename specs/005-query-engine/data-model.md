# Data Model: Query Engine

**Feature**: 005-query-engine
**Date**: 2025-12-07

This document defines the core data structures for the query engine implementation.

---

## 1. Core Entities

### 1.1 Bound Expression System

Bound expressions represent type-checked, catalog-resolved expressions.

```rust
/// Bound expression after semantic analysis
#[derive(Debug, Clone)]
pub enum BoundExpression {
    /// Literal value (constant)
    Literal {
        value: Value,
        data_type: DataType,
    },

    /// Reference to a variable's property
    PropertyAccess {
        variable: String,
        property: String,
        data_type: DataType,
    },

    /// Reference to entire node/relationship variable
    VariableRef {
        variable: String,
        data_type: DataType,
    },

    /// Binary comparison
    Comparison {
        left: Box<BoundExpression>,
        op: ComparisonOp,
        right: Box<BoundExpression>,
        data_type: DataType,  // Always Bool
    },

    /// Logical AND/OR/NOT
    Logical {
        op: LogicalOp,
        operands: Vec<BoundExpression>,
        data_type: DataType,  // Always Bool
    },

    /// Arithmetic operations
    Arithmetic {
        left: Box<BoundExpression>,
        op: ArithmeticOp,
        right: Box<BoundExpression>,
        data_type: DataType,
    },

    /// Aggregation function call
    Aggregate {
        function: AggregateFunction,
        input: Option<Box<BoundExpression>>,  // None for COUNT(*)
        distinct: bool,
        data_type: DataType,
    },

    /// CASE expression
    Case {
        operand: Option<Box<BoundExpression>>,
        when_clauses: Vec<(BoundExpression, BoundExpression)>,
        else_clause: Option<Box<BoundExpression>>,
        data_type: DataType,
    },

    /// IS NULL / IS NOT NULL
    IsNull {
        operand: Box<BoundExpression>,
        negated: bool,
        data_type: DataType,  // Always Bool
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComparisonOp {
    Eq,    // =
    Neq,   // <>
    Lt,    // <
    Lte,   // <=
    Gt,    // >
    Gte,   // >=
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogicalOp {
    And,
    Or,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArithmeticOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AggregateFunction {
    Count,
    Sum,
    Avg,
    Min,
    Max,
}
```

### 1.2 Query Graph (Bound Pattern)

Represents a bound graph pattern after semantic analysis.

```rust
/// Bound query graph representing MATCH patterns
#[derive(Debug, Clone)]
pub struct QueryGraph {
    /// Node variables in the pattern
    pub nodes: Vec<BoundNode>,
    /// Relationship patterns
    pub relationships: Vec<BoundRelationship>,
    /// WHERE predicates
    pub predicates: Vec<BoundExpression>,
}

/// Bound node pattern
#[derive(Debug, Clone)]
pub struct BoundNode {
    /// Variable name (e.g., "p" in (p:Person))
    pub variable: String,
    /// Node table schema
    pub table_schema: Arc<NodeTableSchema>,
    /// Property filters from inline patterns (e.g., {age: 25})
    pub property_filters: Vec<(String, BoundExpression)>,
}

/// Bound relationship pattern
#[derive(Debug, Clone)]
pub struct BoundRelationship {
    /// Optional variable name (e.g., "r" in -[r:KNOWS]->)
    pub variable: Option<String>,
    /// Relationship table schema
    pub rel_schema: Arc<RelTableSchema>,
    /// Source node variable
    pub src_variable: String,
    /// Destination node variable
    pub dst_variable: String,
    /// Traversal direction
    pub direction: Direction,
    /// For variable-length paths: (min, max) hops
    pub path_bounds: Option<(usize, usize)>,
}

/// Traversal direction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Forward,   // ->
    Backward,  // <-
    Both,      // -
}
```

### 1.3 Logical Plan

Represents the logical query plan (what to compute).

```rust
/// Logical query plan
#[derive(Debug, Clone)]
pub enum LogicalPlan {
    // === Scan Operators ===

    /// Scan a node table
    NodeScan {
        table_name: String,
        variable: String,
        schema: Arc<NodeTableSchema>,
        /// Filters that can be pushed to scan
        pushed_filters: Vec<BoundExpression>,
        /// Columns to project (None = all)
        projection: Option<Vec<String>>,
    },

    /// Scan a relationship table directly
    RelScan {
        table_name: String,
        variable: Option<String>,
        schema: Arc<RelTableSchema>,
        pushed_filters: Vec<BoundExpression>,
        projection: Option<Vec<String>>,
    },

    // === Graph Operators ===

    /// Extend via relationship (single hop)
    Extend {
        input: Box<LogicalPlan>,
        rel_type: String,
        rel_schema: Arc<RelTableSchema>,
        src_variable: String,
        dst_variable: String,
        rel_variable: Option<String>,
        direction: Direction,
    },

    /// Variable-length path expansion
    PathExpand {
        input: Box<LogicalPlan>,
        rel_type: String,
        rel_schema: Arc<RelTableSchema>,
        src_variable: String,
        dst_variable: String,
        path_variable: Option<String>,
        min_hops: usize,
        max_hops: usize,
        direction: Direction,
    },

    // === Relational Operators ===

    /// Filter rows
    Filter {
        input: Box<LogicalPlan>,
        predicate: BoundExpression,
    },

    /// Project columns/expressions
    Project {
        input: Box<LogicalPlan>,
        /// (output_name, expression)
        expressions: Vec<(String, BoundExpression)>,
    },

    /// Hash join
    HashJoin {
        left: Box<LogicalPlan>,
        right: Box<LogicalPlan>,
        left_keys: Vec<String>,
        right_keys: Vec<String>,
        join_type: JoinType,
    },

    /// Aggregation with GROUP BY
    Aggregate {
        input: Box<LogicalPlan>,
        group_by: Vec<BoundExpression>,
        aggregates: Vec<(String, BoundExpression)>,
    },

    /// Sort results
    Sort {
        input: Box<LogicalPlan>,
        order_by: Vec<SortExpr>,
    },

    /// Limit/Skip rows
    Limit {
        input: Box<LogicalPlan>,
        skip: Option<usize>,
        limit: Option<usize>,
    },

    /// Set union
    Union {
        inputs: Vec<LogicalPlan>,
        all: bool,  // UNION vs UNION ALL
    },

    /// Empty result (optimized away)
    Empty {
        schema: Vec<(String, DataType)>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinType {
    Inner,
    LeftOuter,
    RightOuter,
    FullOuter,
}

#[derive(Debug, Clone)]
pub struct SortExpr {
    pub expr: BoundExpression,
    pub ascending: bool,
    pub nulls_first: bool,
}
```

### 1.4 Physical Plan

Physical operators implement execution strategies.

```rust
/// Physical execution plan (trait-based for DataFusion integration)
pub trait PhysicalPlan: Send + Sync {
    /// Return output schema
    fn schema(&self) -> SchemaRef;

    /// Execute and return record batch stream
    fn execute(
        &self,
        partition: usize,
        context: Arc<TaskContext>,
    ) -> Result<SendableRecordBatchStream>;

    /// Child plans for tree traversal
    fn children(&self) -> Vec<Arc<dyn PhysicalPlan>>;

    /// Plan name for EXPLAIN
    fn name(&self) -> &str;
}

/// Node scan physical operator
pub struct NodeScanExec {
    pub table: Arc<NodeTable>,
    pub schema: SchemaRef,
    pub projection: Option<Vec<usize>>,
    pub filters: Vec<Arc<dyn PhysicalExpr>>,
    pub limit: Option<usize>,
}

/// Relationship extend physical operator
pub struct ExtendExec {
    pub input: Arc<dyn ExecutionPlan>,
    pub rel_table: Arc<RelTable>,
    pub direction: Direction,
    pub output_schema: SchemaRef,
}

/// Variable-length path expansion
pub struct PathExpandExec {
    pub input: Arc<dyn ExecutionPlan>,
    pub rel_table: Arc<RelTable>,
    pub min_hops: usize,
    pub max_hops: usize,
    pub direction: Direction,
    pub output_schema: SchemaRef,
}
```

---

## 2. Binder Components

### 2.1 Binder State

```rust
/// Main binder for semantic analysis
pub struct Binder<'a> {
    /// Reference to database catalog
    catalog: &'a Catalog,
    /// Current variable scope
    scope: BinderScope,
    /// Error accumulator
    errors: Vec<BindError>,
}

/// Variable scope for name resolution
#[derive(Debug, Clone)]
pub struct BinderScope {
    /// Variable -> (table_schema, variable_type)
    variables: HashMap<String, BoundVariable>,
    /// Parent scope for subqueries
    parent: Option<Box<BinderScope>>,
}

/// Bound variable information
#[derive(Debug, Clone)]
pub struct BoundVariable {
    pub name: String,
    pub variable_type: VariableType,
    pub data_type: DataType,
    /// Schema if node/relationship
    pub schema: Option<Arc<dyn TableSchema>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VariableType {
    Node,
    Relationship,
    Path,
    Property,
    Aggregate,
}
```

### 2.2 Bound Statement

```rust
/// Bound statement after semantic analysis
#[derive(Debug)]
pub enum BoundStatement {
    /// Bound query
    Query(BoundQuery),
    /// DDL statements pass through unchanged
    CreateNodeTable(NodeTableSchema),
    CreateRelTable(RelTableSchema),
    Copy {
        table_name: String,
        file_path: String,
        options: CopyOptions,
    },
}

/// Bound query with all components
#[derive(Debug)]
pub struct BoundQuery {
    /// Bound MATCH pattern
    pub query_graph: QueryGraph,
    /// Bound WHERE clause
    pub where_clause: Option<BoundExpression>,
    /// Bound RETURN clause
    pub return_clause: BoundReturn,
    /// Bound ORDER BY
    pub order_by: Option<Vec<SortExpr>>,
    /// SKIP amount
    pub skip: Option<usize>,
    /// LIMIT amount
    pub limit: Option<usize>,
}

/// Bound RETURN clause
#[derive(Debug)]
pub struct BoundReturn {
    /// (alias, expression)
    pub projections: Vec<(String, BoundExpression)>,
    /// Is RETURN DISTINCT?
    pub distinct: bool,
    /// GROUP BY columns (extracted from aggregates)
    pub group_by: Vec<BoundExpression>,
}
```

---

## 3. Planner Components

### 3.1 Planner State

```rust
/// Query planner
pub struct Planner<'a> {
    catalog: &'a Catalog,
    tables: &'a HashMap<String, Arc<NodeTable>>,
    rel_tables: &'a HashMap<String, RelTable>,
}

impl<'a> Planner<'a> {
    /// Generate logical plan from bound query
    pub fn plan(&self, query: &BoundQuery) -> Result<LogicalPlan>;

    /// Apply optimization rules
    pub fn optimize(&self, plan: LogicalPlan) -> Result<LogicalPlan>;
}
```

### 3.2 Plan Mapper

```rust
/// Maps logical plan to physical plan
pub struct PlanMapper<'a> {
    catalog: &'a Catalog,
    tables: &'a HashMap<String, Arc<NodeTable>>,
    rel_tables: &'a HashMap<String, RelTable>,
    session_context: &'a SessionContext,
}

impl<'a> PlanMapper<'a> {
    /// Convert logical plan to physical execution plan
    pub fn map(&self, plan: &LogicalPlan) -> Result<Arc<dyn ExecutionPlan>>;
}
```

### 3.3 Optimizer Rules

```rust
/// Optimizer rule trait
pub trait OptimizerRule: Send + Sync {
    fn name(&self) -> &str;
    fn rewrite(&self, plan: LogicalPlan) -> Result<Transformed<LogicalPlan>>;
}

/// Filter pushdown rule
pub struct FilterPushdownRule;

/// Projection pushdown rule
pub struct ProjectionPushdownRule;

/// Predicate simplification rule
pub struct PredicateSimplificationRule;

/// Constant folding rule
pub struct ConstantFoldingRule;

/// Result of optimization transformation
pub enum Transformed<T> {
    Yes(T),  // Plan was modified
    No(T),   // Plan unchanged
}
```

---

## 4. Executor Components

### 4.1 Query Executor

```rust
/// Main query executor
pub struct QueryExecutor {
    runtime: Arc<Runtime>,
    session_context: SessionContext,
}

impl QueryExecutor {
    /// Execute a physical plan
    pub async fn execute(
        &self,
        plan: Arc<dyn ExecutionPlan>,
    ) -> Result<Vec<RecordBatch>>;

    /// Execute with streaming results
    pub async fn execute_stream(
        &self,
        plan: Arc<dyn ExecutionPlan>,
    ) -> Result<SendableRecordBatchStream>;
}
```

### 4.2 Record Batch Stream

```rust
/// Streaming result batches
pub struct QueryResultStream {
    inner: SendableRecordBatchStream,
    schema: SchemaRef,
}

impl QueryResultStream {
    /// Collect all batches into QueryResult
    pub async fn collect(self) -> Result<QueryResult>;

    /// Get next batch
    pub async fn next(&mut self) -> Option<Result<RecordBatch>>;
}
```

---

## 5. Type System Extensions

### 5.1 Extended DataType

```rust
/// Data types supported by ruzu
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DataType {
    // Existing types
    Int64,
    String,
    Null,

    // Phase 2 additions
    Bool,
    Float32,
    Float64,
    Date,       // Days since epoch
    Timestamp,  // Microseconds since epoch

    // Graph-specific
    Node,
    Relationship,
    Path,

    // Future (deferred)
    // List(Box<DataType>),
    // Struct(Vec<(String, DataType)>),
    // Map(Box<DataType>, Box<DataType>),
}

impl DataType {
    /// Convert to Arrow DataType
    pub fn to_arrow(&self) -> arrow::datatypes::DataType {
        match self {
            DataType::Int64 => arrow::datatypes::DataType::Int64,
            DataType::String => arrow::datatypes::DataType::Utf8,
            DataType::Bool => arrow::datatypes::DataType::Boolean,
            DataType::Float32 => arrow::datatypes::DataType::Float32,
            DataType::Float64 => arrow::datatypes::DataType::Float64,
            DataType::Date => arrow::datatypes::DataType::Date32,
            DataType::Timestamp => arrow::datatypes::DataType::Timestamp(
                arrow::datatypes::TimeUnit::Microsecond,
                None,
            ),
            DataType::Null => arrow::datatypes::DataType::Null,
            DataType::Node | DataType::Relationship | DataType::Path => {
                // Represented as struct internally
                arrow::datatypes::DataType::Struct(vec![])
            }
        }
    }

    /// Check if type is numeric
    pub fn is_numeric(&self) -> bool {
        matches!(self, DataType::Int64 | DataType::Float32 | DataType::Float64)
    }

    /// Check if type is orderable
    pub fn is_orderable(&self) -> bool {
        matches!(
            self,
            DataType::Int64
                | DataType::Float32
                | DataType::Float64
                | DataType::String
                | DataType::Date
                | DataType::Timestamp
        )
    }
}
```

### 5.2 Extended Value

```rust
/// Value representation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Value {
    // Existing
    Null,
    Int64(i64),
    String(String),

    // Phase 2 additions
    Bool(bool),
    Float32(f32),
    Float64(f64),
    Date(i32),       // Days since epoch
    Timestamp(i64),  // Microseconds since epoch
}

impl Value {
    /// Get the data type of this value
    pub fn data_type(&self) -> DataType {
        match self {
            Value::Null => DataType::Null,
            Value::Int64(_) => DataType::Int64,
            Value::String(_) => DataType::String,
            Value::Bool(_) => DataType::Bool,
            Value::Float32(_) => DataType::Float32,
            Value::Float64(_) => DataType::Float64,
            Value::Date(_) => DataType::Date,
            Value::Timestamp(_) => DataType::Timestamp,
        }
    }

    /// Try to coerce to target type
    pub fn coerce_to(&self, target: DataType) -> Option<Value>;
}
```

---

## 6. Relationships Between Entities

```
                    ┌─────────────────┐
                    │   AST (Parser)  │
                    └────────┬────────┘
                             │ parse
                             ▼
                    ┌─────────────────┐
                    │     Binder      │
                    │  BinderScope    │
                    └────────┬────────┘
                             │ bind
                             ▼
              ┌──────────────────────────────┐
              │        BoundStatement        │
              │  ├─ QueryGraph               │
              │  │  ├─ BoundNode[]           │
              │  │  └─ BoundRelationship[]   │
              │  ├─ BoundExpression[]        │
              │  └─ BoundReturn              │
              └──────────────┬───────────────┘
                             │ plan
                             ▼
              ┌──────────────────────────────┐
              │         LogicalPlan          │
              │  ├─ NodeScan                 │
              │  ├─ Extend                   │
              │  ├─ Filter                   │
              │  ├─ Project                  │
              │  ├─ Aggregate                │
              │  └─ ...                      │
              └──────────────┬───────────────┘
                             │ optimize
                             ▼
              ┌──────────────────────────────┐
              │    Optimized LogicalPlan     │
              └──────────────┬───────────────┘
                             │ map (PlanMapper)
                             ▼
              ┌──────────────────────────────┐
              │        PhysicalPlan          │
              │  ├─ NodeScanExec             │
              │  ├─ ExtendExec               │
              │  ├─ FilterExec (DataFusion)  │
              │  ├─ ProjectExec (DataFusion) │
              │  ├─ AggregateExec (DataFusion│
              │  └─ ...                      │
              └──────────────┬───────────────┘
                             │ execute
                             ▼
              ┌──────────────────────────────┐
              │    RecordBatch Stream        │
              │  └─ Arrow columnar batches   │
              └──────────────┬───────────────┘
                             │ collect
                             ▼
              ┌──────────────────────────────┐
              │        QueryResult           │
              └──────────────────────────────┘
```

---

## 7. Validation Rules

### 7.1 Expression Validation

| Rule | Error |
|------|-------|
| Property access on undefined variable | `UndefinedVariable(name)` |
| Property access on non-node/rel variable | `InvalidPropertyAccess(variable, property)` |
| Type mismatch in comparison | `TypeMismatch(expected, actual)` |
| Aggregate in WHERE clause | `AggregateInWhereClause` |
| Non-aggregate in GROUP BY select | `NonAggregateInGroupBy(expr)` |
| Division by zero (literal) | `DivisionByZero` |

### 7.2 Pattern Validation

| Rule | Error |
|------|-------|
| Undefined node table | `UndefinedTable(name)` |
| Undefined relationship table | `UndefinedRelTable(name)` |
| Relationship connects wrong tables | `InvalidRelationship(rel, src, dst)` |
| Duplicate variable name | `DuplicateVariable(name)` |
| Invalid path bounds (min > max) | `InvalidPathBounds(min, max)` |

---

## 8. State Transitions

### 8.1 Query Processing States

```
      [Parsed]
         │
         │ Binder.bind()
         ▼
      [Bound]
         │
         │ Planner.plan()
         ▼
    [Logical Plan]
         │
         │ Optimizer.optimize()
         ▼
[Optimized Logical Plan]
         │
         │ PlanMapper.map()
         ▼
   [Physical Plan]
         │
         │ Executor.execute()
         ▼
    [Executing] ──────► [Complete]
         │
         └──────────► [Error]
```

### 8.2 Operator Execution States

```
   [Created]
       │
       │ execute()
       ▼
  [Streaming] ◄────┐
       │           │ poll_next()
       │───────────┘
       │
       │ stream exhausted
       ▼
   [Complete]
```
