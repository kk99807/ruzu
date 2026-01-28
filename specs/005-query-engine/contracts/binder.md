# Binder API Contract

**Module**: `src/binder/`
**Version**: 1.0.0

## Overview

The Binder performs semantic analysis on parsed AST, resolving names, checking types, and producing a bound representation suitable for query planning.

---

## Public API

### `Binder::new`

Creates a new binder instance.

```rust
pub fn new(catalog: &Catalog) -> Binder<'_>;
```

**Parameters**:
- `catalog`: Reference to the database catalog for name resolution

**Returns**: New Binder instance

---

### `Binder::bind`

Binds a parsed statement to produce a bound statement.

```rust
pub fn bind(&mut self, statement: &Statement) -> Result<BoundStatement>;
```

**Parameters**:
- `statement`: Parsed AST statement

**Returns**:
- `Ok(BoundStatement)`: Successfully bound statement
- `Err(BindError)`: Binding failed

**Errors**:
| Error | Condition |
|-------|-----------|
| `UndefinedTable(name)` | Referenced table doesn't exist in catalog |
| `UndefinedColumn(table, column)` | Column doesn't exist in table |
| `UndefinedVariable(name)` | Variable used but not defined in pattern |
| `DuplicateVariable(name)` | Same variable name used twice |
| `TypeMismatch { expected, actual }` | Expression type doesn't match expected |
| `InvalidRelationship { rel, src, dst }` | Relationship doesn't connect specified tables |
| `AggregateInWhereClause` | Aggregate function used in WHERE |
| `NonAggregateInGroupBy(expr)` | Non-grouped column in SELECT with aggregates |

---

### `Binder::bind_expression`

Binds an expression within the current scope.

```rust
pub fn bind_expression(&self, expr: &Expression) -> Result<BoundExpression>;
```

**Parameters**:
- `expr`: Parsed expression

**Returns**:
- `Ok(BoundExpression)`: Bound expression with resolved types
- `Err(BindError)`: Binding failed

---

## Invariants

1. **Scope Validity**: All variables referenced in expressions must be in scope
2. **Type Consistency**: All expressions have a resolved `DataType`
3. **Schema Resolution**: All table/column references resolve to catalog entries
4. **No Side Effects**: Binding is read-only; catalog is not modified

---

## Contract Tests

```rust
#[cfg(test)]
mod contract_tests {
    use super::*;

    /// Variables must be defined before use
    #[test]
    fn undefined_variable_returns_error() {
        let catalog = Catalog::new();
        let mut binder = Binder::new(&catalog);

        // Query references 'p' but no MATCH pattern defines it
        let stmt = parse("RETURN p.name").unwrap();
        let result = binder.bind(&stmt);

        assert!(matches!(result, Err(BindError::UndefinedVariable(_))));
    }

    /// Table must exist in catalog
    #[test]
    fn undefined_table_returns_error() {
        let catalog = Catalog::new(); // Empty catalog
        let mut binder = Binder::new(&catalog);

        let stmt = parse("MATCH (p:Person) RETURN p").unwrap();
        let result = binder.bind(&stmt);

        assert!(matches!(result, Err(BindError::UndefinedTable(_))));
    }

    /// Column must exist in table
    #[test]
    fn undefined_column_returns_error() {
        let mut catalog = Catalog::new();
        catalog.create_table(NodeTableSchema::new(
            "Person".into(),
            vec![ColumnDef::new("name", DataType::String)],
            vec!["name".into()],
        ).unwrap()).unwrap();

        let mut binder = Binder::new(&catalog);
        let stmt = parse("MATCH (p:Person) RETURN p.nonexistent").unwrap();
        let result = binder.bind(&stmt);

        assert!(matches!(result, Err(BindError::UndefinedColumn(_, _))));
    }

    /// Bound expression has resolved type
    #[test]
    fn bound_expression_has_type() {
        let mut catalog = Catalog::new();
        catalog.create_table(NodeTableSchema::new(
            "Person".into(),
            vec![
                ColumnDef::new("name", DataType::String),
                ColumnDef::new("age", DataType::Int64),
            ],
            vec!["name".into()],
        ).unwrap()).unwrap();

        let mut binder = Binder::new(&catalog);
        let stmt = parse("MATCH (p:Person) WHERE p.age > 25 RETURN p.name").unwrap();
        let bound = binder.bind(&stmt).unwrap();

        // WHERE expression should be Bool
        if let BoundStatement::Query(query) = bound {
            let where_type = query.where_clause.unwrap().data_type();
            assert_eq!(where_type, DataType::Bool);
        }
    }

    /// Aggregates not allowed in WHERE
    #[test]
    fn aggregate_in_where_returns_error() {
        let mut catalog = Catalog::new();
        catalog.create_table(NodeTableSchema::new(
            "Person".into(),
            vec![ColumnDef::new("name", DataType::String)],
            vec!["name".into()],
        ).unwrap()).unwrap();

        let mut binder = Binder::new(&catalog);
        let stmt = parse("MATCH (p:Person) WHERE COUNT(*) > 5 RETURN p").unwrap();
        let result = binder.bind(&stmt);

        assert!(matches!(result, Err(BindError::AggregateInWhereClause)));
    }

    /// Relationship must connect correct tables
    #[test]
    fn invalid_relationship_returns_error() {
        let mut catalog = Catalog::new();
        catalog.create_table(NodeTableSchema::new(
            "Person".into(),
            vec![ColumnDef::new("name", DataType::String)],
            vec!["name".into()],
        ).unwrap()).unwrap();
        catalog.create_table(NodeTableSchema::new(
            "Company".into(),
            vec![ColumnDef::new("name", DataType::String)],
            vec!["name".into()],
        ).unwrap()).unwrap();
        // KNOWS connects Person to Person, not Person to Company
        catalog.create_rel_table(RelTableSchema::new(
            "KNOWS".into(),
            "Person".into(),
            "Person".into(),
            vec![],
            Direction::Both,
        ).unwrap()).unwrap();

        let mut binder = Binder::new(&catalog);
        // This tries to use KNOWS from Person to Company
        let stmt = parse("MATCH (p:Person)-[:KNOWS]->(c:Company) RETURN p, c").unwrap();
        let result = binder.bind(&stmt);

        assert!(matches!(result, Err(BindError::InvalidRelationship { .. })));
    }
}
```

---

## Performance Requirements

| Operation | Target | Notes |
|-----------|--------|-------|
| Bind simple query | <1ms | Single table, few columns |
| Bind complex query | <10ms | Multiple patterns, aggregations |
| Memory per bind | O(n) | Where n = number of variables |

---

## Thread Safety

- `Binder` is **not** `Sync` (holds mutable scope state)
- Safe to use from single thread
- Create new `Binder` instance per query
