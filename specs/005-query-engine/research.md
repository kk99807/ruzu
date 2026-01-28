# Research: Query Engine with DataFusion Integration

**Feature**: 005-query-engine
**Date**: 2025-12-07
**Status**: Complete

This document consolidates research findings for all NEEDS CLARIFICATION items and technology decisions.

---

## 1. DataFusion Version Selection

**Decision**: Use Apache DataFusion 44+ (recommend 50.3.0 for latest stable)

**Rationale**:
- Version 44+ introduced `ScalarUDFImpl::invoke_with_args` for better UDF type handling
- Version 50+ has enhanced dynamic filter pushdown for hash joins
- Active monthly releases with deprecation windows (6 months/6 versions)
- SIGMOD 2024 paper validates production-readiness

**Alternatives Considered**:
- DataFusion 40.x: Older API, missing recent optimizations
- Building from scratch: 12-16 weeks vs 4-6 weeks with DataFusion
- Polars: More DataFrame-focused, less customizable for graph operations

**Cargo Configuration**:
```toml
[dependencies]
datafusion = "50.3"
arrow = "53"
async-trait = "0.1"
tokio = { version = "1", features = ["rt-multi-thread"] }
```

---

## 2. DataFusion TableProvider Integration

**Decision**: Implement `TableProvider` trait for NodeTable and RelTable

**Rationale**:
- Standard interface for custom storage in DataFusion
- Enables filter pushdown via `supports_filters_pushdown()`
- Returns `ExecutionPlan` for scan operations

**Implementation Pattern** (from DataFusion documentation):
```rust
use datafusion::catalog::{TableProvider, Session};
use datafusion::datasource::TableType;
use datafusion::physical_plan::ExecutionPlan;
use datafusion::logical_expr::Expr;
use arrow::datatypes::SchemaRef;

#[async_trait]
impl TableProvider for NodeTableProvider {
    fn schema(&self) -> SchemaRef {
        // Return Arrow schema from NodeTableSchema
        Arc::new(self.table_schema.to_arrow_schema())
    }

    fn table_type(&self) -> TableType {
        TableType::Base
    }

    async fn scan(
        &self,
        _state: &dyn Session,
        projection: Option<&Vec<usize>>,
        filters: &[Expr],
        limit: Option<usize>,
    ) -> Result<Arc<dyn ExecutionPlan>> {
        Ok(Arc::new(NodeScanExec::new(
            self.table.clone(),
            self.schema(),
            projection.cloned(),
            filters.to_vec(),
            limit,
        )))
    }

    fn supports_filters_pushdown(
        &self,
        filters: &[&Expr],
    ) -> Result<Vec<TableProviderFilterPushDown>> {
        // Return Exact for simple comparisons, Unsupported for complex expressions
        filters.iter().map(|f| {
            match f {
                Expr::BinaryExpr { .. } => Ok(TableProviderFilterPushDown::Exact),
                _ => Ok(TableProviderFilterPushDown::Unsupported),
            }
        }).collect()
    }
}
```

**Key Files in KuzuDB Reference**:
- `c:/dev/kuzu/src/include/processor/operator/scan/scan_node_table.h`
- `c:/dev/kuzu/src/processor/operator/scan/scan_node_table.cpp`

---

## 3. Custom Graph Operators

**Decision**: Implement `ExecutionPlan` trait for graph-specific operators

**Rationale**:
- DataFusion only provides relational operators
- Graph traversal requires custom Extend and PathExpand operators
- CSR index access for relationship traversal

**Operators to Implement**:

### 3.1 ExtendExec (Single-Hop Traversal)

**Purpose**: Traverse one relationship hop from input nodes

**Reference**: `c:/dev/kuzu/src/include/processor/operator/scan/scan_rel_table.h`

```rust
pub struct ExtendExec {
    input: Arc<dyn ExecutionPlan>,
    rel_table: Arc<RelTable>,
    direction: Direction,  // Forward, Backward, Both
    schema: SchemaRef,
}

impl ExecutionPlan for ExtendExec {
    fn execute(
        &self,
        partition: usize,
        context: Arc<TaskContext>,
    ) -> Result<SendableRecordBatchStream> {
        let input_stream = self.input.execute(partition, context)?;
        Ok(Box::pin(ExtendStream::new(
            input_stream,
            self.rel_table.clone(),
            self.direction,
            self.schema.clone(),
        )))
    }
}
```

### 3.2 PathExpandExec (Variable-Length Traversal)

**Purpose**: BFS/DFS traversal with min/max hop bounds

**Reference**: `c:/dev/kuzu/src/include/processor/operator/recursive_extend/recursive_extend.h`

```rust
pub struct PathExpandExec {
    input: Arc<dyn ExecutionPlan>,
    rel_table: Arc<RelTable>,
    min_hops: usize,
    max_hops: usize,
    direction: Direction,
    schema: SchemaRef,
}
```

**Cycle Detection**:
- Track visited nodes in HashSet per path
- Default max_hops = 10 to prevent runaway queries
- Configurable via query option

---

## 4. Binder Architecture

**Decision**: Follow KuzuDB's binder pattern with Rust adaptations

**Reference Files**:
- `c:/dev/kuzu/src/include/binder/binder.h`
- `c:/dev/kuzu/src/include/binder/expression/expression.h`
- `c:/dev/kuzu/src/include/binder/query/query_graph.h`

**Key Components**:

### 4.1 Binder Struct
```rust
pub struct Binder<'a> {
    catalog: &'a Catalog,
    scope: BinderScope,
}

pub struct BinderScope {
    variables: HashMap<String, BoundVariable>,
    parent: Option<Box<BinderScope>>,
}

pub struct BoundVariable {
    name: String,
    table_schema: Arc<NodeTableSchema>,
    variable_type: VariableType,  // Node, Relationship, Property
}
```

### 4.2 BoundExpression
```rust
pub enum BoundExpression {
    Literal(Value),
    PropertyAccess {
        variable: String,
        property: String,
        data_type: DataType,
    },
    Comparison {
        left: Box<BoundExpression>,
        op: ComparisonOp,
        right: Box<BoundExpression>,
    },
    Aggregation {
        function: AggregateFunction,
        input: Box<BoundExpression>,
        distinct: bool,
    },
    // ...
}
```

### 4.3 QueryGraph (Bound Pattern)
```rust
pub struct QueryGraph {
    nodes: Vec<BoundNode>,
    relationships: Vec<BoundRelationship>,
    predicates: Vec<BoundExpression>,
}

pub struct BoundNode {
    variable: String,
    table_schema: Arc<NodeTableSchema>,
    properties: Vec<BoundExpression>,
}

pub struct BoundRelationship {
    variable: Option<String>,
    rel_schema: Arc<RelTableSchema>,
    src_node: String,
    dst_node: String,
    direction: Direction,
}
```

---

## 5. Planner Architecture

**Decision**: Generate LogicalPlan, then translate to DataFusion PhysicalPlan

**Reference Files**:
- `c:/dev/kuzu/src/include/planner/planner.h`
- `c:/dev/kuzu/src/include/planner/operator/logical_operator.h`

### 5.1 LogicalPlan Enum
```rust
pub enum LogicalPlan {
    // Scan operators
    NodeScan {
        table: String,
        variable: String,
        filters: Vec<BoundExpression>,
        projections: Vec<String>,
    },
    RelScan {
        table: String,
        variable: Option<String>,
        filters: Vec<BoundExpression>,
    },

    // Graph operators
    Extend {
        input: Box<LogicalPlan>,
        rel_type: String,
        direction: Direction,
        dst_variable: String,
    },
    PathExpand {
        input: Box<LogicalPlan>,
        rel_type: String,
        min_hops: usize,
        max_hops: usize,
        path_variable: Option<String>,
    },

    // Relational operators (delegated to DataFusion)
    Filter {
        input: Box<LogicalPlan>,
        predicate: BoundExpression,
    },
    Project {
        input: Box<LogicalPlan>,
        expressions: Vec<(String, BoundExpression)>,
    },
    HashJoin {
        left: Box<LogicalPlan>,
        right: Box<LogicalPlan>,
        left_keys: Vec<String>,
        right_keys: Vec<String>,
    },
    Aggregate {
        input: Box<LogicalPlan>,
        group_by: Vec<BoundExpression>,
        aggregates: Vec<(String, BoundExpression)>,
    },
    Sort {
        input: Box<LogicalPlan>,
        order_by: Vec<(BoundExpression, SortOrder)>,
    },
    Limit {
        input: Box<LogicalPlan>,
        skip: Option<usize>,
        limit: Option<usize>,
    },
}
```

### 5.2 PlanMapper (Logical to Physical)
```rust
pub struct PlanMapper<'a> {
    catalog: &'a Catalog,
    tables: &'a HashMap<String, Arc<NodeTable>>,
    rel_tables: &'a HashMap<String, RelTable>,
}

impl PlanMapper<'_> {
    pub fn map(&self, plan: &LogicalPlan) -> Result<Arc<dyn ExecutionPlan>> {
        match plan {
            LogicalPlan::NodeScan { table, .. } => {
                // Create NodeScanExec
            }
            LogicalPlan::Extend { input, .. } => {
                // Create ExtendExec wrapping mapped input
            }
            LogicalPlan::Filter { input, predicate } => {
                // Use DataFusion's FilterExec
            }
            // ...
        }
    }
}
```

---

## 6. Optimizer Rules

**Decision**: Implement core optimization rules, defer cost-based optimization

**Rules to Implement**:

### 6.1 Filter Pushdown
Push filters as close to data sources as possible.

```rust
pub struct FilterPushdownRule;

impl OptimizerRule for FilterPushdownRule {
    fn rewrite(
        &self,
        plan: LogicalPlan,
        _config: &dyn OptimizerConfig,
    ) -> Result<Transformed<LogicalPlan>> {
        // Push filters below Project, through Extend where safe
    }
}
```

### 6.2 Projection Pushdown
Only read columns that are needed.

```rust
pub struct ProjectionPushdownRule;

impl OptimizerRule for ProjectionPushdownRule {
    fn rewrite(
        &self,
        plan: LogicalPlan,
        _config: &dyn OptimizerConfig,
    ) -> Result<Transformed<LogicalPlan>> {
        // Collect required columns, push projections to scans
    }
}
```

### 6.3 Predicate Simplification
Simplify constant expressions.

```rust
// WHERE 1 = 0 → EmptyResult
// WHERE true AND x > 5 → WHERE x > 5
```

**Deferred**:
- Cost-based join ordering (Phase 4+)
- Statistics-based cardinality estimation
- WCOJ (Worst-Case Optimal Join) for multi-way patterns

---

## 7. Vectorized Execution

**Decision**: Use Arrow RecordBatch with 2048-row batches

**Rationale**:
- 2048 is KuzuDB's DEFAULT_VECTOR_CAPACITY
- Fits in L2 cache for most processors
- Standard for analytical databases (DuckDB, Velox)

**Reference**:
- `c:/dev/kuzu/src/include/common/vector/value_vector.h`
- `c:/dev/kuzu/src/include/common/data_chunk/data_chunk.h`

**Implementation**:
```rust
pub const DEFAULT_BATCH_SIZE: usize = 2048;

pub struct VectorizedBatch {
    batch: RecordBatch,
    selection_vector: Option<SelectionVector>,
}

pub struct SelectionVector {
    indices: Vec<u32>,
    size: usize,
}
```

---

## 8. Expression Evaluation

**Decision**: Use DataFusion's PhysicalExpr for expression evaluation

**Rationale**:
- Vectorized evaluation on Arrow arrays
- Built-in support for comparisons, arithmetic, functions
- Extensible for custom functions

**Pattern**:
```rust
use datafusion::physical_expr::PhysicalExpr;

// Convert BoundExpression to DataFusion PhysicalExpr
fn to_physical_expr(
    expr: &BoundExpression,
    schema: &Schema,
) -> Result<Arc<dyn PhysicalExpr>> {
    match expr {
        BoundExpression::PropertyAccess { variable, property, .. } => {
            let col_name = format!("{}.{}", variable, property);
            Ok(Arc::new(Column::new(&col_name, schema.index_of(&col_name)?)))
        }
        BoundExpression::Comparison { left, op, right } => {
            let left_expr = to_physical_expr(left, schema)?;
            let right_expr = to_physical_expr(right, schema)?;
            Ok(Arc::new(BinaryExpr::new(left_expr, op.to_df_op(), right_expr)))
        }
        // ...
    }
}
```

---

## 9. Type System Extensions

**Decision**: Extend ruzu types to include FLOAT, DOUBLE, DATE, TIMESTAMP

**Current Types** (from `src/types/mod.rs`):
- INT64
- STRING
- NULL

**New Types**:
```rust
pub enum DataType {
    // Existing
    Int64,
    String,
    Null,

    // New in Phase 2
    Bool,
    Float32,
    Float64,
    Date,
    Timestamp,

    // Deferred
    List(Box<DataType>),
    Struct(Vec<(String, DataType)>),
}

impl DataType {
    pub fn to_arrow_type(&self) -> arrow::datatypes::DataType {
        match self {
            DataType::Int64 => arrow::datatypes::DataType::Int64,
            DataType::String => arrow::datatypes::DataType::Utf8,
            DataType::Bool => arrow::datatypes::DataType::Boolean,
            DataType::Float32 => arrow::datatypes::DataType::Float32,
            DataType::Float64 => arrow::datatypes::DataType::Float64,
            DataType::Date => arrow::datatypes::DataType::Date32,
            DataType::Timestamp => arrow::datatypes::DataType::Timestamp(TimeUnit::Microsecond, None),
            DataType::Null => arrow::datatypes::DataType::Null,
            // ...
        }
    }
}
```

---

## 10. Aggregation Functions

**Decision**: Implement COUNT, SUM, MIN, MAX, AVG using DataFusion's AggregateExec

**Supported Functions**:

| Function | Input Types | Output Type | NULL Handling |
|----------|-------------|-------------|---------------|
| COUNT(*) | Any | Int64 | Counts all rows |
| COUNT(col) | Any | Int64 | Ignores NULLs |
| SUM(col) | Numeric | Same as input | Ignores NULLs |
| AVG(col) | Numeric | Float64 | Ignores NULLs |
| MIN(col) | Ordered | Same as input | Ignores NULLs |
| MAX(col) | Ordered | Same as input | Ignores NULLs |

**Implementation**:
```rust
use datafusion::physical_plan::aggregates::{AggregateExec, AggregateMode};
use datafusion::physical_expr::aggregate::AggregateFunction;

fn create_aggregate_exec(
    input: Arc<dyn ExecutionPlan>,
    group_by: Vec<Arc<dyn PhysicalExpr>>,
    aggregates: Vec<(AggregateFunction, Arc<dyn PhysicalExpr>)>,
) -> Result<AggregateExec> {
    // Use DataFusion's AggregateExec with partial/final modes
}
```

---

## 11. EXPLAIN Output Format

**Decision**: Tree-structured text output showing operator hierarchy

**Format**:
```
EXPLAIN MATCH (p:Person)-[:KNOWS]->(f) WHERE p.age > 25 RETURN p.name, COUNT(f)

Aggregate [group_by: [p.name], agg: [COUNT(f)]]
└── Filter [p.age > 25]
    └── Extend [KNOWS, FORWARD]
        └── NodeScan [Person as p]

Applied optimizations:
- Filter pushed below Extend
- Projection pushed to NodeScan [name, age]
```

---

## 12. Edge Cases

**Resolved Decisions**:

| Edge Case | Decision |
|-----------|----------|
| Non-existent table reference | Return `RuzuError::SchemaError` at binding time |
| NULL in aggregations | Follow SQL semantics: ignore NULLs in SUM/AVG/MIN/MAX |
| NULL in ORDER BY | NULLs sort last (NULLS LAST default) |
| Variable-length path bounds | Default max 10 hops; configurable via `MAXHOPS` option |
| Hash join memory exceeded | Spill to disk deferred to Phase 4; return error for now |
| Empty intermediate results | Propagate empty RecordBatch; short-circuit execution |
| GROUP BY without aggregation | Return distinct values of group columns |

---

## 13. KuzuDB Reference Architecture Summary

**Query Pipeline** (from c:/dev/kuzu):

1. **Parser** (`src/antlr4/Cypher.g4`): ANTLR4-based Cypher parser
2. **Binder** (`src/binder/`): Semantic analysis, type checking
3. **Planner** (`src/planner/`): Logical plan with DP join ordering
4. **Optimizer**: Rule-based + cost-based optimization
5. **PlanMapper** (`src/processor/plan_mapper.h`): Logical → Physical
6. **Processor** (`src/processor/`): Morsel-driven parallel execution

**Key Differences for ruzu**:
- Use pest instead of ANTLR4 (already implemented)
- Use DataFusion for relational operators
- Use Arrow instead of custom ValueVector
- Single-threaded for MVP (no morsel-driven parallelism yet)

---

## References

### DataFusion Documentation
- [TableProvider Guide](https://datafusion.apache.org/library-user-guide/custom-table-providers.html)
- [Custom Operators](https://datafusion.apache.org/library-user-guide/extending-operators.html)
- [Optimizer Rules](https://datafusion.apache.org/library-user-guide/query-optimizer.html)

### KuzuDB Source Files
- `c:/dev/kuzu/src/include/binder/binder.h`
- `c:/dev/kuzu/src/include/planner/planner.h`
- `c:/dev/kuzu/src/include/processor/processor.h`
- `c:/dev/kuzu/src/include/common/vector/value_vector.h`

### Research Papers
- DataFusion: A Query Execution Engine for Apache Arrow (SIGMOD 2024)
- LeanStore Buffer Manager (TUM Database Group)

### Rust Crates
- [datafusion](https://docs.rs/datafusion/latest/datafusion/)
- [arrow](https://docs.rs/arrow/latest/arrow/)
