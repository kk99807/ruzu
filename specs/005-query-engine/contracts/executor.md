# Executor API Contract

**Module**: `src/executor/`
**Version**: 1.0.0

## Overview

The Executor runs physical query plans and produces results as Arrow RecordBatches.

---

## Public API

### `QueryExecutor::new`

Creates a new query executor.

```rust
pub fn new(config: ExecutorConfig) -> Result<QueryExecutor>;
```

**Parameters**:
- `config`: Executor configuration

```rust
pub struct ExecutorConfig {
    /// Target batch size (default: 2048)
    pub batch_size: usize,
    /// Memory limit in bytes (default: 256MB)
    pub memory_limit: usize,
    /// Number of partitions (default: 1 for single-threaded)
    pub partitions: usize,
}
```

**Returns**: New QueryExecutor instance

---

### `QueryExecutor::execute`

Executes a physical plan and collects all results.

```rust
pub async fn execute(
    &self,
    plan: Arc<dyn ExecutionPlan>,
) -> Result<Vec<RecordBatch>>;
```

**Parameters**:
- `plan`: Physical execution plan

**Returns**:
- `Ok(Vec<RecordBatch>)`: Result batches
- `Err(ExecutionError)`: Execution failed

**Errors**:
| Error | Condition |
|-------|-----------|
| `OutOfMemory` | Exceeded memory limit |
| `TypeCastError { from, to }` | Cannot cast value to target type |
| `DivisionByZero` | Division by zero in expression |
| `NullViolation(column)` | NULL in non-nullable column |
| `ExecutionFailed(reason)` | Internal execution error |

---

### `QueryExecutor::execute_stream`

Executes a physical plan with streaming results.

```rust
pub fn execute_stream(
    &self,
    plan: Arc<dyn ExecutionPlan>,
) -> Result<SendableRecordBatchStream>;
```

**Parameters**:
- `plan`: Physical execution plan

**Returns**:
- `Ok(stream)`: Streaming result batches
- `Err(ExecutionError)`: Execution failed

**Stream Behavior**:
- Produces batches of up to `batch_size` rows
- Batches are produced lazily (pull-based)
- Stream ends when no more results

---

## Physical Operators

### `NodeScanExec`

Scans a node table.

```rust
pub struct NodeScanExec {
    pub table: Arc<NodeTable>,
    pub schema: SchemaRef,
    pub projection: Option<Vec<usize>>,
    pub filters: Vec<Arc<dyn PhysicalExpr>>,
    pub limit: Option<usize>,
}
```

**Contract**:
- Returns batches containing node properties
- Applies filters during scan (filter pushdown)
- Only reads projected columns
- Respects limit if specified

---

### `ExtendExec`

Extends via relationship traversal (single hop).

```rust
pub struct ExtendExec {
    pub input: Arc<dyn ExecutionPlan>,
    pub rel_table: Arc<RelTable>,
    pub direction: Direction,
    pub output_schema: SchemaRef,
}
```

**Contract**:
- For each input row, finds matching edges via CSR index
- Produces one output row per (input, destination) pair
- If no edges, input row is not included in output
- Supports Forward, Backward, and Both directions

---

### `PathExpandExec`

Variable-length path expansion.

```rust
pub struct PathExpandExec {
    pub input: Arc<dyn ExecutionPlan>,
    pub rel_table: Arc<RelTable>,
    pub min_hops: usize,
    pub max_hops: usize,
    pub direction: Direction,
    pub output_schema: SchemaRef,
}
```

**Contract**:
- Performs BFS/DFS from input nodes
- Returns paths with length between min_hops and max_hops
- Detects cycles (each node appears at most once per path)
- Respects max_hops limit to prevent runaway queries

---

### `AggregateExec`

Aggregation with GROUP BY.

```rust
// Uses DataFusion's AggregateExec
```

**Contract**:
- Groups rows by GROUP BY expressions
- Computes aggregate functions per group
- NULL handling follows SQL semantics:
  - COUNT(*): Counts all rows including NULLs
  - COUNT(col): Ignores NULLs
  - SUM/AVG/MIN/MAX: Ignore NULLs

---

## Invariants

1. **Schema Consistency**: Output batches match operator's declared schema
2. **Batch Size**: Batches have at most `batch_size` rows (except final)
3. **Memory Bound**: Total memory usage stays within configured limit
4. **Determinism**: Same input produces same output (for same plan)
5. **Error Propagation**: Errors stop execution and propagate to caller

---

## Contract Tests

```rust
#[cfg(test)]
mod contract_tests {
    use super::*;

    /// NodeScan returns correct schema
    #[test]
    fn node_scan_output_schema() {
        let table = create_test_table();
        let scan = NodeScanExec::new(
            table.clone(),
            table.schema(),
            None, // all columns
            vec![],
            None,
        );

        let executor = QueryExecutor::new(Default::default()).unwrap();
        let batches = executor.execute(Arc::new(scan)).await.unwrap();

        for batch in &batches {
            assert_eq!(batch.schema(), table.schema());
        }
    }

    /// Filter reduces row count
    #[test]
    fn filter_reduces_rows() {
        let table = create_test_table_with_data(); // 100 rows
        let scan = NodeScanExec::new(table, ...);

        // Filter: age > 50 (should reduce to ~50 rows)
        let filter = create_filter_expr("age > 50");
        let filtered = FilterExec::try_new(filter, Arc::new(scan)).unwrap();

        let executor = QueryExecutor::new(Default::default()).unwrap();
        let batches = executor.execute(Arc::new(filtered)).await.unwrap();

        let total_rows: usize = batches.iter().map(|b| b.num_rows()).sum();
        assert!(total_rows < 100);
    }

    /// Extend produces correct fan-out
    #[test]
    fn extend_produces_fanout() {
        let (nodes, rels) = create_test_graph(); // 10 nodes, each has 3 friends
        let scan = NodeScanExec::new(nodes, ...);
        let extend = ExtendExec::new(Arc::new(scan), rels, Direction::Forward, ...);

        let executor = QueryExecutor::new(Default::default()).unwrap();
        let batches = executor.execute(Arc::new(extend)).await.unwrap();

        let total_rows: usize = batches.iter().map(|b| b.num_rows()).sum();
        assert_eq!(total_rows, 30); // 10 nodes * 3 friends each
    }

    /// Aggregate produces correct counts
    #[test]
    fn aggregate_count_correct() {
        let table = create_test_table_with_data(); // 100 rows
        let scan = NodeScanExec::new(table, ...);

        // COUNT(*)
        let aggregate = create_aggregate(vec![], vec![count_star()]);
        let agg_exec = AggregateExec::try_new(...).unwrap();

        let executor = QueryExecutor::new(Default::default()).unwrap();
        let batches = executor.execute(Arc::new(agg_exec)).await.unwrap();

        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].num_rows(), 1);
        let count_col = batches[0].column(0).as_primitive::<Int64Type>();
        assert_eq!(count_col.value(0), 100);
    }

    /// PathExpand respects max hops
    #[test]
    fn path_expand_respects_max_hops() {
        let (nodes, rels) = create_chain_graph(); // A->B->C->D->E
        let scan = NodeScanExec::new(nodes, /* filter to A */);
        let expand = PathExpandExec::new(
            Arc::new(scan),
            rels,
            1, // min_hops
            2, // max_hops
            Direction::Forward,
            ...
        );

        let executor = QueryExecutor::new(Default::default()).unwrap();
        let batches = executor.execute(Arc::new(expand)).await.unwrap();

        // Should find: A->B (1 hop), A->C (2 hops)
        // Should NOT find: A->D (3 hops), A->E (4 hops)
        let paths = collect_paths(&batches);
        assert!(paths.contains(&vec!["A", "B"]));
        assert!(paths.contains(&vec!["A", "B", "C"]));
        assert!(!paths.contains(&vec!["A", "B", "C", "D"]));
    }

    /// Cycle detection prevents infinite loops
    #[test]
    fn path_expand_detects_cycles() {
        let (nodes, rels) = create_cyclic_graph(); // A->B->C->A
        let scan = NodeScanExec::new(nodes, /* filter to A */);
        let expand = PathExpandExec::new(
            Arc::new(scan),
            rels,
            1,
            10, // Large max to test cycle detection
            Direction::Forward,
            ...
        );

        let executor = QueryExecutor::new(Default::default()).unwrap();
        let result = executor.execute(Arc::new(expand)).await;

        // Should complete without hanging
        assert!(result.is_ok());

        // No path should contain duplicate nodes
        let paths = collect_paths(&result.unwrap());
        for path in paths {
            let unique: HashSet<_> = path.iter().collect();
            assert_eq!(unique.len(), path.len());
        }
    }

    /// Batch size is respected
    #[test]
    fn batches_respect_size_limit() {
        let table = create_large_table(); // 10000 rows
        let scan = NodeScanExec::new(table, ...);

        let config = ExecutorConfig {
            batch_size: 1024,
            ..Default::default()
        };
        let executor = QueryExecutor::new(config).unwrap();
        let batches = executor.execute(Arc::new(scan)).await.unwrap();

        for batch in &batches[..batches.len()-1] {
            assert!(batch.num_rows() <= 1024);
        }
    }

    /// Memory limit is enforced
    #[test]
    fn memory_limit_enforced() {
        let table = create_huge_table(); // 10M rows
        let scan = NodeScanExec::new(table, ...);

        let config = ExecutorConfig {
            memory_limit: 1024 * 1024, // 1MB
            ..Default::default()
        };
        let executor = QueryExecutor::new(config).unwrap();
        let result = executor.execute(Arc::new(scan)).await;

        // Should fail with OutOfMemory, not crash
        assert!(matches!(result, Err(ExecutionError::OutOfMemory)));
    }
}
```

---

## Performance Requirements

| Operation | Target | Notes |
|-----------|--------|-------|
| NodeScan throughput | 10M rows/sec | Single column, no filter |
| Filter throughput | 5M rows/sec | Simple comparison |
| Extend throughput | 1M edges/sec | CSR index lookup |
| Aggregate throughput | 5M rows/sec | COUNT(*) |
| Batch overhead | <1% | Batch creation/iteration |

---

## Memory Model

```
┌─────────────────────────────────────────┐
│           Execution Context             │
├─────────────────────────────────────────┤
│  Memory Pool                            │
│  ├── Operator States (fixed)            │
│  ├── Hash Tables (grows)                │
│  ├── Sort Buffers (grows)               │
│  └── Result Batches (flows through)     │
├─────────────────────────────────────────┤
│  Memory Limit: 256MB (default)          │
└─────────────────────────────────────────┘
```

**Memory Management**:
- Each operator has fixed overhead
- Hash join/aggregate grow dynamically
- Memory tracking at allocation points
- Fail early if limit exceeded (no spilling for MVP)

---

## Thread Safety

- `QueryExecutor` is `Sync` (can be shared)
- Each `execute()` call gets independent task context
- Operators are `Send + Sync` for async execution
- Single-threaded execution for MVP (one partition)
