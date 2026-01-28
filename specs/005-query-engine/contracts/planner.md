# Planner API Contract

**Module**: `src/planner/`
**Version**: 1.0.0

## Overview

The Planner transforms bound statements into logical query plans and applies optimization rules.

---

## Public API

### `Planner::new`

Creates a new planner instance.

```rust
pub fn new<'a>(
    catalog: &'a Catalog,
    tables: &'a HashMap<String, Arc<NodeTable>>,
    rel_tables: &'a HashMap<String, RelTable>,
) -> Planner<'a>;
```

**Parameters**:
- `catalog`: Database catalog for schema information
- `tables`: Node tables for cardinality estimation
- `rel_tables`: Relationship tables for traversal planning

**Returns**: New Planner instance

---

### `Planner::plan`

Generates a logical plan from a bound query.

```rust
pub fn plan(&self, query: &BoundQuery) -> Result<LogicalPlan>;
```

**Parameters**:
- `query`: Bound query from the Binder

**Returns**:
- `Ok(LogicalPlan)`: Logical query plan
- `Err(PlanError)`: Planning failed

**Errors**:
| Error | Condition |
|-------|-----------|
| `EmptyPattern` | MATCH clause has no patterns |
| `UnsupportedFeature(desc)` | Query uses unsupported Cypher feature |
| `PlanningFailed(reason)` | Internal planning error |

---

### `Planner::optimize`

Applies optimization rules to a logical plan.

```rust
pub fn optimize(&self, plan: LogicalPlan) -> Result<LogicalPlan>;
```

**Parameters**:
- `plan`: Unoptimized logical plan

**Returns**:
- `Ok(LogicalPlan)`: Optimized plan (may be unchanged)
- `Err(PlanError)`: Optimization failed

**Optimization Rules Applied**:
1. `FilterPushdownRule`: Push filters toward data sources
2. `ProjectionPushdownRule`: Prune unused columns
3. `PredicateSimplificationRule`: Simplify constant expressions
4. `ConstantFoldingRule`: Evaluate constant subexpressions

---

### `PlanMapper::map`

Converts logical plan to physical execution plan.

```rust
pub fn map(&self, plan: &LogicalPlan) -> Result<Arc<dyn ExecutionPlan>>;
```

**Parameters**:
- `plan`: Logical plan

**Returns**:
- `Ok(ExecutionPlan)`: Physical plan ready for execution
- `Err(PlanError)`: Mapping failed

**Mapping Rules**:
| Logical | Physical |
|---------|----------|
| `NodeScan` | `NodeScanExec` (custom) |
| `RelScan` | `RelScanExec` (custom) |
| `Extend` | `ExtendExec` (custom) |
| `PathExpand` | `PathExpandExec` (custom) |
| `Filter` | `FilterExec` (DataFusion) |
| `Project` | `ProjectionExec` (DataFusion) |
| `HashJoin` | `HashJoinExec` (DataFusion) |
| `Aggregate` | `AggregateExec` (DataFusion) |
| `Sort` | `SortExec` (DataFusion) |
| `Limit` | `GlobalLimitExec` (DataFusion) |

---

## Invariants

1. **Schema Preservation**: Output schema of plan matches expected columns
2. **Filter Safety**: Filters only reference available columns
3. **Join Correctness**: Join keys exist in both inputs
4. **Aggregate Validity**: GROUP BY covers all non-aggregate outputs

---

## Contract Tests

```rust
#[cfg(test)]
mod contract_tests {
    use super::*;

    /// NodeScan produces correct schema
    #[test]
    fn node_scan_schema_matches_table() {
        let catalog = create_test_catalog();
        let planner = Planner::new(&catalog, &tables, &rel_tables);

        let query = bind("MATCH (p:Person) RETURN p.name, p.age");
        let plan = planner.plan(&query).unwrap();

        // Schema should have name and age columns
        let schema = plan.schema();
        assert!(schema.contains("p.name"));
        assert!(schema.contains("p.age"));
    }

    /// Filter pushdown moves filter below project
    #[test]
    fn filter_pushdown_optimizes_plan() {
        let catalog = create_test_catalog();
        let planner = Planner::new(&catalog, &tables, &rel_tables);

        let query = bind("MATCH (p:Person) WHERE p.age > 25 RETURN p.name");
        let unoptimized = planner.plan(&query).unwrap();
        let optimized = planner.optimize(unoptimized).unwrap();

        // Filter should be pushed to NodeScan
        if let LogicalPlan::Project { input, .. } = optimized {
            if let LogicalPlan::NodeScan { pushed_filters, .. } = input.as_ref() {
                assert!(!pushed_filters.is_empty());
            } else {
                panic!("Expected NodeScan after optimization");
            }
        }
    }

    /// Projection pushdown removes unused columns
    #[test]
    fn projection_pushdown_prunes_columns() {
        let catalog = create_test_catalog();
        let planner = Planner::new(&catalog, &tables, &rel_tables);

        // Only selecting name, not age
        let query = bind("MATCH (p:Person) RETURN p.name");
        let optimized = planner.plan(&query).and_then(|p| planner.optimize(p)).unwrap();

        // NodeScan should only project 'name'
        if let LogicalPlan::Project { input, .. } = optimized {
            if let LogicalPlan::NodeScan { projection, .. } = input.as_ref() {
                assert_eq!(projection, &Some(vec!["name".into()]));
            }
        }
    }

    /// Constant folding simplifies WHERE 1 = 0
    #[test]
    fn constant_false_produces_empty_plan() {
        let catalog = create_test_catalog();
        let planner = Planner::new(&catalog, &tables, &rel_tables);

        let query = bind("MATCH (p:Person) WHERE 1 = 0 RETURN p.name");
        let optimized = planner.plan(&query).and_then(|p| planner.optimize(p)).unwrap();

        assert!(matches!(optimized, LogicalPlan::Empty { .. }));
    }

    /// Extend operator connects correct tables
    #[test]
    fn extend_follows_relationship() {
        let catalog = create_test_catalog_with_rels();
        let planner = Planner::new(&catalog, &tables, &rel_tables);

        let query = bind("MATCH (p:Person)-[:KNOWS]->(f:Person) RETURN p.name, f.name");
        let plan = planner.plan(&query).unwrap();

        // Should have Extend operator
        assert!(plan_contains_extend(&plan));
    }

    /// Aggregate produces correct output schema
    #[test]
    fn aggregate_schema_includes_group_and_agg() {
        let catalog = create_test_catalog();
        let planner = Planner::new(&catalog, &tables, &rel_tables);

        let query = bind("MATCH (p:Person) RETURN p.city, COUNT(*)");
        let plan = planner.plan(&query).unwrap();

        let schema = plan.schema();
        assert!(schema.contains("p.city"));
        assert!(schema.contains("COUNT(*)"));
    }
}
```

---

## Performance Requirements

| Operation | Target | Notes |
|-----------|--------|-------|
| Plan simple query | <5ms | Single pattern, few operators |
| Plan complex query | <50ms | Multi-pattern with joins |
| Optimize | <10ms | All rules applied |
| Memory | O(n) | Where n = operators in plan |

---

## EXPLAIN Output Format

The planner produces human-readable plan output:

```
EXPLAIN MATCH (p:Person)-[:KNOWS]->(f) WHERE p.age > 25 RETURN p.name, COUNT(f)

Aggregate [group_by: [p.name], agg: [COUNT(f)]]
└── Filter [p.age > 25]
    └── Extend [KNOWS, FORWARD]
        └── NodeScan [Person as p, projection: [name, age]]

Applied Optimizations:
  ✓ FilterPushdown: Filter moved to Extend input
  ✓ ProjectionPushdown: NodeScan projects [name, age] only
```

---

## Thread Safety

- `Planner` is `Sync` (immutable after creation)
- Can be shared across threads for concurrent planning
- Each `plan()` call is independent
