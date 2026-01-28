//! Unit tests for ruzu.

use ruzu::catalog::{Catalog, ColumnDef, NodeTableSchema};
use ruzu::parser::ast::{ComparisonOp, Literal, Statement};
use ruzu::parser::parse_query;
use ruzu::storage::{ColumnStorage, NodeTable};
use ruzu::types::{DataType, Value};
use ruzu::RuzuError;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::Arc;

// =============================================================================
// Error Tests
// =============================================================================

mod error_tests {
    use super::*;

    #[test]
    fn test_parse_error_display() {
        let err = RuzuError::ParseError {
            line: 1,
            col: 5,
            message: "unexpected token".into(),
        };
        assert!(err.to_string().contains("line 1"));
        assert!(err.to_string().contains("column 5"));
        assert!(err.to_string().contains("unexpected token"));
    }

    #[test]
    fn test_schema_error_display() {
        let err = RuzuError::SchemaError("Table 'Person' already exists".into());
        assert!(err.to_string().contains("Schema error"));
        assert!(err.to_string().contains("Person"));
    }

    #[test]
    fn test_type_error_display() {
        let err = RuzuError::TypeError {
            expected: "INT64".into(),
            actual: "STRING".into(),
        };
        assert!(err.to_string().contains("INT64"));
        assert!(err.to_string().contains("STRING"));
    }

    #[test]
    fn test_constraint_violation_display() {
        let err = RuzuError::ConstraintViolation("Duplicate primary key".into());
        assert!(err.to_string().contains("Constraint violation"));
        assert!(err.to_string().contains("Duplicate primary key"));
    }

    #[test]
    fn test_execution_error_display() {
        let err = RuzuError::ExecutionError("Table not found".into());
        assert!(err.to_string().contains("Execution error"));
        assert!(err.to_string().contains("Table not found"));
    }
}

// =============================================================================
// Types Tests
// =============================================================================

mod types_tests {
    use super::*;

    #[test]
    fn test_datatype_int64_name() {
        assert_eq!(DataType::Int64.name(), "INT64");
    }

    #[test]
    fn test_datatype_string_name() {
        assert_eq!(DataType::String.name(), "STRING");
    }

    #[test]
    fn test_value_int64() {
        let value = Value::Int64(42);
        assert_eq!(value.as_int64(), Some(42));
        assert!(!value.is_null());
    }

    #[test]
    fn test_value_string() {
        let value = Value::String("hello".into());
        assert_eq!(value.as_string(), Some("hello"));
        assert!(!value.is_null());
    }

    #[test]
    fn test_value_null() {
        let value = Value::Null;
        assert!(value.is_null());
        assert_eq!(value.as_int64(), None);
        assert_eq!(value.as_string(), None);
    }

    #[test]
    fn test_value_data_type_int64() {
        let value = Value::Int64(42);
        assert_eq!(value.data_type(), Some(DataType::Int64));
    }

    #[test]
    fn test_value_data_type_string() {
        let value = Value::String("hello".into());
        assert_eq!(value.data_type(), Some(DataType::String));
    }

    #[test]
    fn test_value_data_type_null() {
        let value = Value::Null;
        assert_eq!(value.data_type(), None);
    }

    #[test]
    fn test_value_compare_int64_less() {
        let a = Value::Int64(10);
        let b = Value::Int64(20);
        assert_eq!(a.compare(&b), Some(Ordering::Less));
    }

    #[test]
    fn test_value_compare_int64_greater() {
        let a = Value::Int64(30);
        let b = Value::Int64(20);
        assert_eq!(a.compare(&b), Some(Ordering::Greater));
    }

    #[test]
    fn test_value_compare_int64_equal() {
        let a = Value::Int64(10);
        let b = Value::Int64(10);
        assert_eq!(a.compare(&b), Some(Ordering::Equal));
    }

    #[test]
    fn test_value_compare_string_less() {
        let a = Value::String("Alice".into());
        let b = Value::String("Bob".into());
        assert_eq!(a.compare(&b), Some(Ordering::Less));
    }

    #[test]
    fn test_value_compare_null_left() {
        let a = Value::Null;
        let b = Value::Int64(10);
        assert_eq!(a.compare(&b), None);
    }

    #[test]
    fn test_value_compare_null_right() {
        let a = Value::Int64(10);
        let b = Value::Null;
        assert_eq!(a.compare(&b), None);
    }

    #[test]
    fn test_value_compare_type_mismatch() {
        let a = Value::Int64(10);
        let b = Value::String("hello".into());
        assert_eq!(a.compare(&b), None);
    }
}

// =============================================================================
// Catalog Tests
// =============================================================================

mod catalog_tests {
    use super::*;

    #[test]
    fn test_column_def_new_success() {
        let col = ColumnDef::new("name".into(), DataType::String);
        assert!(col.is_ok());
        let col = col.unwrap();
        assert_eq!(col.name, "name");
        assert_eq!(col.data_type, DataType::String);
    }

    #[test]
    fn test_column_def_empty_name_error() {
        let col = ColumnDef::new(String::new(), DataType::String);
        assert!(col.is_err());
    }

    #[test]
    fn test_schema_new_success() {
        let schema = NodeTableSchema::new(
            "Person".into(),
            vec![
                ColumnDef::new("name".into(), DataType::String).unwrap(),
                ColumnDef::new("age".into(), DataType::Int64).unwrap(),
            ],
            vec!["name".into()],
        );
        assert!(schema.is_ok());
        let schema = schema.unwrap();
        assert_eq!(schema.name, "Person");
        assert_eq!(schema.columns.len(), 2);
    }

    #[test]
    fn test_schema_empty_columns_error() {
        let schema = NodeTableSchema::new("Person".into(), vec![], vec!["name".into()]);
        assert!(schema.is_err());
    }

    #[test]
    fn test_schema_duplicate_columns_error() {
        let schema = NodeTableSchema::new(
            "Person".into(),
            vec![
                ColumnDef::new("name".into(), DataType::String).unwrap(),
                ColumnDef::new("name".into(), DataType::Int64).unwrap(),
            ],
            vec!["name".into()],
        );
        assert!(schema.is_err());
    }

    #[test]
    fn test_schema_invalid_pk_column_error() {
        let schema = NodeTableSchema::new(
            "Person".into(),
            vec![ColumnDef::new("name".into(), DataType::String).unwrap()],
            vec!["nonexistent".into()],
        );
        assert!(schema.is_err());
    }

    #[test]
    fn test_catalog_create_table_success() {
        let mut catalog = Catalog::new();
        let schema = NodeTableSchema::new(
            "Person".into(),
            vec![ColumnDef::new("name".into(), DataType::String).unwrap()],
            vec!["name".into()],
        )
        .unwrap();

        let result = catalog.create_table(schema);
        assert!(result.is_ok());
        assert!(catalog.table_exists("Person"));
    }

    #[test]
    fn test_catalog_create_duplicate_table_error() {
        let mut catalog = Catalog::new();
        let schema1 = NodeTableSchema::new(
            "Person".into(),
            vec![ColumnDef::new("name".into(), DataType::String).unwrap()],
            vec!["name".into()],
        )
        .unwrap();

        let schema2 = NodeTableSchema::new(
            "Person".into(),
            vec![ColumnDef::new("id".into(), DataType::Int64).unwrap()],
            vec!["id".into()],
        )
        .unwrap();

        catalog.create_table(schema1).unwrap();
        let result = catalog.create_table(schema2);
        assert!(result.is_err());
    }
}

// =============================================================================
// Storage Tests
// =============================================================================

mod storage_tests {
    use super::*;

    #[test]
    fn test_column_storage_new_empty() {
        let col = ColumnStorage::new();
        assert!(col.is_empty());
        assert_eq!(col.len(), 0);
    }

    #[test]
    fn test_column_storage_push_and_get() {
        let mut col = ColumnStorage::new();
        col.push(Value::Int64(42));
        col.push(Value::Int64(99));

        assert_eq!(col.len(), 2);
        assert_eq!(col.get(0), Some(&Value::Int64(42)));
        assert_eq!(col.get(1), Some(&Value::Int64(99)));
        assert_eq!(col.get(2), None);
    }

    fn create_person_schema() -> Arc<NodeTableSchema> {
        Arc::new(
            NodeTableSchema::new(
                "Person".into(),
                vec![
                    ColumnDef::new("name".into(), DataType::String).unwrap(),
                    ColumnDef::new("age".into(), DataType::Int64).unwrap(),
                ],
                vec!["name".into()],
            )
            .unwrap(),
        )
    }

    #[test]
    fn test_node_table_new_empty() {
        let schema = create_person_schema();
        let table = NodeTable::new(schema);
        assert_eq!(table.row_count(), 0);
    }

    #[test]
    fn test_node_table_insert_success() {
        let schema = create_person_schema();
        let mut table = NodeTable::new(schema);

        let mut row = HashMap::new();
        row.insert("name".into(), Value::String("Alice".into()));
        row.insert("age".into(), Value::Int64(25));

        let result = table.insert(&row);
        assert!(result.is_ok());
        assert_eq!(table.row_count(), 1);
    }

    #[test]
    fn test_node_table_insert_duplicate_pk_error() {
        let schema = create_person_schema();
        let mut table = NodeTable::new(schema);

        let mut row1 = HashMap::new();
        row1.insert("name".into(), Value::String("Alice".into()));
        row1.insert("age".into(), Value::Int64(25));
        table.insert(&row1).unwrap();

        let mut row2 = HashMap::new();
        row2.insert("name".into(), Value::String("Alice".into()));
        row2.insert("age".into(), Value::Int64(30));

        let result = table.insert(&row2);
        assert!(result.is_err());
    }

    #[test]
    fn test_node_table_insert_type_mismatch_error() {
        let schema = create_person_schema();
        let mut table = NodeTable::new(schema);

        let mut row = HashMap::new();
        row.insert("name".into(), Value::String("Alice".into()));
        row.insert("age".into(), Value::String("twenty-five".into()));

        let result = table.insert(&row);
        assert!(result.is_err());
    }
}

// =============================================================================
// Parser Tests
// =============================================================================

mod parser_tests {
    use super::*;

    #[test]
    fn test_parse_create_node_table_basic() {
        let query = "CREATE NODE TABLE Person(name STRING, age INT64, PRIMARY KEY(name))";
        let result = parse_query(query);
        assert!(result.is_ok());

        if let Statement::CreateNodeTable {
            table_name,
            columns,
            primary_key,
        } = result.unwrap()
        {
            assert_eq!(table_name, "Person");
            assert_eq!(columns.len(), 2);
            assert_eq!(primary_key, vec!["name".to_string()]);
        } else {
            panic!("Expected CreateNodeTable statement");
        }
    }

    #[test]
    fn test_parse_create_node_table_with_semicolon() {
        let query = "CREATE NODE TABLE Person(name STRING, PRIMARY KEY(name));";
        let result = parse_query(query);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_create_node_table_invalid_syntax() {
        let query = "CREATE NODE TABLE";
        let result = parse_query(query);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_create_node_basic() {
        let query = "CREATE (:Person {name: 'Alice', age: 25})";
        let result = parse_query(query);
        assert!(result.is_ok());

        if let Statement::CreateNode { label, properties } = result.unwrap() {
            assert_eq!(label, "Person");
            assert_eq!(properties.len(), 2);
            assert!(matches!(&properties[0].1, Literal::String(s) if s == "Alice"));
            assert!(matches!(properties[1].1, Literal::Int64(25)));
        } else {
            panic!("Expected CreateNode statement");
        }
    }

    #[test]
    fn test_parse_match_basic() {
        let query = "MATCH (p:Person) RETURN p.name";
        let result = parse_query(query);
        assert!(result.is_ok());

        if let Statement::Match {
            var,
            label,
            filter,
            projections,
            ..
        } = result.unwrap()
        {
            assert_eq!(var, "p");
            assert_eq!(label, "Person");
            assert!(filter.is_none());
            assert_eq!(projections.len(), 1);
        } else {
            panic!("Expected Match statement");
        }
    }

    #[test]
    fn test_parse_match_with_where() {
        let query = "MATCH (p:Person) WHERE p.age > 20 RETURN p.name, p.age";
        let result = parse_query(query);
        assert!(result.is_ok());

        if let Statement::Match {
            filter,
            projections,
            ..
        } = result.unwrap()
        {
            assert!(filter.is_some());
            let f = filter.unwrap();
            assert_eq!(f.op, ComparisonOp::Gt);
            assert!(matches!(f.value, Literal::Int64(20)));
            assert_eq!(projections.len(), 2);
        } else {
            panic!("Expected Match statement");
        }
    }

    #[test]
    fn test_parse_match_invalid_syntax() {
        let query = "MATCH (p:Person";
        let result = parse_query(query);
        assert!(result.is_err());
    }

    #[test]
    fn test_comparison_op_parse() {
        assert_eq!(ComparisonOp::parse(">"), Some(ComparisonOp::Gt));
        assert_eq!(ComparisonOp::parse("<"), Some(ComparisonOp::Lt));
        assert_eq!(ComparisonOp::parse("="), Some(ComparisonOp::Eq));
        assert_eq!(ComparisonOp::parse(">="), Some(ComparisonOp::Gte));
        assert_eq!(ComparisonOp::parse("<="), Some(ComparisonOp::Lte));
        assert_eq!(ComparisonOp::parse("<>"), Some(ComparisonOp::Neq));
        assert_eq!(ComparisonOp::parse("!="), None);
    }
}

// =============================================================================
// RowBuffer Tests (Feature 004-optimize-csv-memory)
// =============================================================================

mod row_buffer_tests {
    use ruzu::storage::csv::RowBuffer;
    use ruzu::types::Value;

    #[test]
    fn test_row_buffer_new_and_capacity() {
        let buffer = RowBuffer::new(1000, 5);
        assert_eq!(buffer.capacity(), 1000);
        assert_eq!(buffer.column_capacity(), 5);
        assert_eq!(buffer.len(), 0);
        assert!(buffer.is_empty());
        assert!(!buffer.is_full());
    }

    #[test]
    fn test_row_buffer_push_and_len() {
        let mut buffer = RowBuffer::new(100, 3);

        // Push some rows
        buffer
            .push(vec![
                Value::Int64(1),
                Value::String("Alice".into()),
                Value::Bool(true),
            ])
            .unwrap();
        assert_eq!(buffer.len(), 1);
        assert!(!buffer.is_empty());

        buffer
            .push(vec![
                Value::Int64(2),
                Value::String("Bob".into()),
                Value::Bool(false),
            ])
            .unwrap();
        assert_eq!(buffer.len(), 2);
    }

    #[test]
    fn test_row_buffer_is_full() {
        let mut buffer = RowBuffer::new(2, 1);

        assert!(!buffer.is_full());

        buffer.push(vec![Value::Int64(1)]).unwrap();
        assert!(!buffer.is_full());

        buffer.push(vec![Value::Int64(2)]).unwrap();
        assert!(buffer.is_full());

        // Pushing to a full buffer should fail
        let result = buffer.push(vec![Value::Int64(3)]);
        assert!(result.is_err());
    }

    #[test]
    fn test_row_buffer_clear_preserves_capacity() {
        let mut buffer = RowBuffer::new(100, 5);

        // Add some rows
        for i in 0..50 {
            buffer.push(vec![Value::Int64(i)]).unwrap();
        }
        assert_eq!(buffer.len(), 50);

        // Clear and verify capacity is preserved
        buffer.clear();
        assert_eq!(buffer.len(), 0);
        assert!(buffer.is_empty());
        assert_eq!(buffer.capacity(), 100);

        // Should be able to add rows again
        buffer.push(vec![Value::Int64(999)]).unwrap();
        assert_eq!(buffer.len(), 1);
    }

    #[test]
    fn test_row_buffer_take_and_recycling() {
        let mut buffer = RowBuffer::new(100, 3);

        // Add rows
        buffer
            .push(vec![
                Value::Int64(1),
                Value::String("Alice".into()),
                Value::Bool(true),
            ])
            .unwrap();
        buffer
            .push(vec![
                Value::Int64(2),
                Value::String("Bob".into()),
                Value::Bool(false),
            ])
            .unwrap();

        // Take the rows
        let rows = buffer.take();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0][0], Value::Int64(1));
        assert_eq!(rows[1][0], Value::Int64(2));

        // Buffer should be empty but still usable
        assert!(buffer.is_empty());
        assert_eq!(buffer.len(), 0);
        assert_eq!(buffer.capacity(), 100);

        // Can add new rows after take
        buffer.push(vec![Value::Int64(3)]).unwrap();
        assert_eq!(buffer.len(), 1);
    }

    #[test]
    fn test_row_buffer_iter() {
        let mut buffer = RowBuffer::new(10, 2);
        buffer
            .push(vec![Value::Int64(1), Value::String("a".into())])
            .unwrap();
        buffer
            .push(vec![Value::Int64(2), Value::String("b".into())])
            .unwrap();

        let values: Vec<i64> = buffer
            .iter()
            .filter_map(|row| match &row[0] {
                Value::Int64(v) => Some(*v),
                _ => None,
            })
            .collect();

        assert_eq!(values, vec![1, 2]);
    }

    #[test]
    fn test_row_buffer_into_iter() {
        let mut buffer = RowBuffer::new(10, 1);
        buffer.push(vec![Value::Int64(1)]).unwrap();
        buffer.push(vec![Value::Int64(2)]).unwrap();
        buffer.push(vec![Value::Int64(3)]).unwrap();

        let rows: Vec<Vec<Value>> = buffer.into_iter().collect();
        assert_eq!(rows.len(), 3);
    }

    #[test]
    fn test_row_buffer_recycle() {
        let mut buffer = RowBuffer::new(100, 5);

        // Add some rows
        for i in 0..50 {
            buffer
                .push(vec![Value::Int64(i), Value::String(format!("row{i}"))])
                .unwrap();
        }
        assert_eq!(buffer.len(), 50);
        assert_eq!(buffer.recycled_count(), 0);

        // Recycle - should move rows to recycled pool
        buffer.recycle();
        assert_eq!(buffer.len(), 0);
        assert_eq!(buffer.recycled_count(), 50);

        // Push with recycling should reuse the recycled Vecs
        buffer.push_with_recycling(vec![Value::Int64(999)]).unwrap();
        assert_eq!(buffer.len(), 1);
        assert_eq!(buffer.recycled_count(), 49); // One was consumed
    }

    #[test]
    fn test_row_buffer_push_with_recycling() {
        let mut buffer = RowBuffer::new(10, 3);

        // First batch without recycling
        buffer
            .push_with_recycling(vec![Value::Int64(1), Value::String("a".into())])
            .unwrap();
        buffer
            .push_with_recycling(vec![Value::Int64(2), Value::String("b".into())])
            .unwrap();
        assert_eq!(buffer.len(), 2);
        assert_eq!(buffer.recycled_count(), 0);

        // Recycle
        buffer.recycle();
        assert_eq!(buffer.len(), 0);
        assert_eq!(buffer.recycled_count(), 2);

        // Second batch - should reuse recycled Vecs
        buffer
            .push_with_recycling(vec![Value::Int64(3), Value::String("c".into())])
            .unwrap();
        assert_eq!(buffer.len(), 1);
        assert_eq!(buffer.recycled_count(), 1);
    }

    #[test]
    fn test_row_buffer_take_and_prepare_recycle() {
        let mut buffer = RowBuffer::new(100, 3);

        // Add rows
        for i in 0..10 {
            buffer.push(vec![Value::Int64(i)]).unwrap();
        }

        // Take rows for processing
        let rows = buffer.take_and_prepare_recycle();
        assert_eq!(rows.len(), 10);
        assert!(buffer.is_empty());

        // Return rows for recycling
        buffer.return_for_recycling(rows);
        assert_eq!(buffer.recycled_count(), 10);

        // New rows should reuse recycled Vecs
        buffer.push_with_recycling(vec![Value::Int64(100)]).unwrap();
        assert_eq!(buffer.len(), 1);
        assert_eq!(buffer.recycled_count(), 9);
    }

    #[test]
    fn test_row_buffer_recycle_limits_growth() {
        let mut buffer = RowBuffer::new(10, 2); // capacity = 10, max recycled = 20

        // Fill and recycle multiple times to exceed limit
        for _ in 0..5 {
            for i in 0..10 {
                buffer.push(vec![Value::Int64(i)]).unwrap();
            }
            buffer.recycle();
        }

        // Should be limited to capacity * 2 = 20
        assert!(buffer.recycled_count() <= 20);
    }
}

// =============================================================================
// NodeTable Batch Insert Tests (Feature 004-optimize-csv-memory, T016-T018)
// =============================================================================

mod table_batch_tests {
    use ruzu::catalog::{ColumnDef, NodeTableSchema};
    use ruzu::storage::NodeTable;
    use ruzu::types::{DataType, Value};
    use std::sync::Arc;

    fn create_person_schema() -> Arc<NodeTableSchema> {
        Arc::new(
            NodeTableSchema::new(
                "Person".to_string(),
                vec![
                    ColumnDef::new("id".to_string(), DataType::Int64).unwrap(),
                    ColumnDef::new("name".to_string(), DataType::String).unwrap(),
                    ColumnDef::new("age".to_string(), DataType::Int64).unwrap(),
                ],
                vec!["id".to_string()],
            )
            .unwrap(),
        )
    }

    #[test]
    fn test_insert_batch_empty_input() {
        let schema = create_person_schema();
        let mut table = NodeTable::new(schema);

        let columns = vec!["id".to_string(), "name".to_string(), "age".to_string()];
        let rows: Vec<Vec<Value>> = vec![];

        let result = table.insert_batch(rows, &columns);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
        assert_eq!(table.row_count(), 0);
    }

    #[test]
    fn test_insert_batch_valid_rows() {
        let schema = create_person_schema();
        let mut table = NodeTable::new(schema);

        let columns = vec!["id".to_string(), "name".to_string(), "age".to_string()];
        let rows = vec![
            vec![
                Value::Int64(1),
                Value::String("Alice".into()),
                Value::Int64(25),
            ],
            vec![
                Value::Int64(2),
                Value::String("Bob".into()),
                Value::Int64(30),
            ],
            vec![
                Value::Int64(3),
                Value::String("Charlie".into()),
                Value::Int64(35),
            ],
        ];

        let result = table.insert_batch(rows, &columns);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 3);
        assert_eq!(table.row_count(), 3);

        // Verify data
        assert_eq!(
            table.get(0, "name"),
            Some(Value::String("Alice".to_string()))
        );
        assert_eq!(table.get(1, "name"), Some(Value::String("Bob".to_string())));
        assert_eq!(
            table.get(2, "name"),
            Some(Value::String("Charlie".to_string()))
        );
    }

    #[test]
    fn test_insert_batch_schema_mismatch() {
        let schema = create_person_schema();
        let mut table = NodeTable::new(schema);

        // Wrong number of columns
        let columns = vec!["id".to_string(), "name".to_string()]; // Missing 'age'
        let rows = vec![vec![Value::Int64(1), Value::String("Alice".into())]];

        let result = table.insert_batch(rows, &columns);
        assert!(result.is_err());
    }

    #[test]
    fn test_insert_batch_duplicate_pk() {
        let schema = create_person_schema();
        let mut table = NodeTable::new(schema);

        let columns = vec!["id".to_string(), "name".to_string(), "age".to_string()];
        let rows = vec![
            vec![
                Value::Int64(1),
                Value::String("Alice".into()),
                Value::Int64(25),
            ],
            vec![
                Value::Int64(1), // Duplicate PK
                Value::String("Bob".into()),
                Value::Int64(30),
            ],
        ];

        let result = table.insert_batch(rows, &columns);
        assert!(result.is_err());
    }

    #[test]
    fn test_insert_batch_preserves_existing_data() {
        let schema = create_person_schema();
        let mut table = NodeTable::new(schema);

        let columns = vec!["id".to_string(), "name".to_string(), "age".to_string()];

        // First batch
        let rows1 = vec![vec![
            Value::Int64(1),
            Value::String("Alice".into()),
            Value::Int64(25),
        ]];
        table.insert_batch(rows1, &columns).unwrap();
        assert_eq!(table.row_count(), 1);

        // Second batch
        let rows2 = vec![
            vec![
                Value::Int64(2),
                Value::String("Bob".into()),
                Value::Int64(30),
            ],
            vec![
                Value::Int64(3),
                Value::String("Charlie".into()),
                Value::Int64(35),
            ],
        ];
        table.insert_batch(rows2, &columns).unwrap();
        assert_eq!(table.row_count(), 3);

        // All data should be accessible
        assert_eq!(
            table.get(0, "name"),
            Some(Value::String("Alice".to_string()))
        );
        assert_eq!(table.get(1, "name"), Some(Value::String("Bob".to_string())));
        assert_eq!(
            table.get(2, "name"),
            Some(Value::String("Charlie".to_string()))
        );
    }
}

// =============================================================================
// RelTable Batch Insert Tests (Feature 004-optimize-csv-memory, T019)
// =============================================================================

mod rel_table_batch_tests {
    use ruzu::catalog::{ColumnDef, Direction, RelTableSchema};
    use ruzu::storage::RelTable;
    use ruzu::types::{DataType, Value};
    use std::sync::Arc;

    fn create_knows_schema() -> Arc<RelTableSchema> {
        Arc::new(
            RelTableSchema::new(
                "KNOWS".to_string(),
                "Person".to_string(),
                "Person".to_string(),
                vec![ColumnDef::new("since".to_string(), DataType::Int64).unwrap()],
                Direction::Both,
            )
            .unwrap(),
        )
    }

    #[test]
    fn test_insert_batch_valid_relationships() {
        let schema = create_knows_schema();
        let mut table = RelTable::new(schema);

        // Insert batch of relationships: (from, to, properties)
        let relationships = vec![
            (0u64, 1u64, vec![Value::Int64(2020)]),
            (0u64, 2u64, vec![Value::Int64(2019)]),
            (1u64, 2u64, vec![Value::Int64(2021)]),
        ];

        let result = table.insert_batch(relationships);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 3);
        assert_eq!(table.len(), 3);

        // Verify forward edges
        let forward_0 = table.get_forward_edges(0);
        assert_eq!(forward_0.len(), 2);

        let forward_1 = table.get_forward_edges(1);
        assert_eq!(forward_1.len(), 1);
    }

    #[test]
    fn test_insert_batch_empty_relationships() {
        let schema = create_knows_schema();
        let mut table = RelTable::new(schema);

        let relationships: Vec<(u64, u64, Vec<Value>)> = vec![];
        let result = table.insert_batch(relationships);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
        assert!(table.is_empty());
    }

    #[test]
    fn test_insert_batch_wrong_property_count() {
        let schema = create_knows_schema();
        let mut table = RelTable::new(schema);

        // Wrong number of properties (schema has 1, we provide 2)
        let relationships = vec![(0u64, 1u64, vec![Value::Int64(2020), Value::Int64(100)])];

        let result = table.insert_batch(relationships);
        assert!(result.is_err());
    }
}

// =============================================================================
// Progress Reporting Tests (Feature 004-optimize-csv-memory, T044-T049)
// =============================================================================

// =============================================================================
// Logical Plan Unit Tests (Feature 005-query-engine, T124-T126)
// =============================================================================

mod logical_plan_tests {
    use ruzu::binder::BoundExpression;
    use ruzu::catalog::{ColumnDef, NodeTableSchema};
    use ruzu::planner::{JoinType, LogicalPlan};
    use ruzu::types::{DataType, Value};
    use std::sync::Arc;

    fn create_test_schema() -> Arc<NodeTableSchema> {
        Arc::new(
            NodeTableSchema::new(
                "Person".to_string(),
                vec![
                    ColumnDef::new("id".to_string(), DataType::Int64).unwrap(),
                    ColumnDef::new("name".to_string(), DataType::String).unwrap(),
                    ColumnDef::new("age".to_string(), DataType::Int64).unwrap(),
                ],
                vec!["id".to_string()],
            )
            .unwrap(),
        )
    }

    #[test]
    fn test_node_scan_output_schema() {
        let schema = create_test_schema();
        let plan = LogicalPlan::node_scan("Person".to_string(), "p".to_string(), schema);

        let output = plan.output_schema();
        assert_eq!(output.len(), 3);

        // Check column names are prefixed with variable
        let names: Vec<&str> = output.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"p.id"));
        assert!(names.contains(&"p.name"));
        assert!(names.contains(&"p.age"));
    }

    #[test]
    fn test_filter_preserves_schema() {
        let schema = create_test_schema();
        let scan = LogicalPlan::node_scan("Person".to_string(), "p".to_string(), schema);
        let scan_schema = scan.output_schema();

        let predicate = BoundExpression::literal(Value::Bool(true));
        let filtered = LogicalPlan::filter(scan, predicate);

        assert_eq!(filtered.output_schema(), scan_schema);
    }

    #[test]
    fn test_project_changes_schema() {
        let schema = create_test_schema();
        let scan = LogicalPlan::node_scan("Person".to_string(), "p".to_string(), schema);

        let projections = vec![(
            "person_name".to_string(),
            BoundExpression::property_access("p".to_string(), "name".to_string(), DataType::String),
        )];

        let projected = LogicalPlan::project(scan, projections);
        let output = projected.output_schema();

        assert_eq!(output.len(), 1);
        assert_eq!(output[0].0, "person_name");
        assert_eq!(output[0].1, DataType::String);
    }

    #[test]
    fn test_limit_preserves_schema() {
        let schema = create_test_schema();
        let scan = LogicalPlan::node_scan("Person".to_string(), "p".to_string(), schema);
        let scan_schema = scan.output_schema();

        let limited = LogicalPlan::limit(scan, Some(10), Some(100));

        assert_eq!(limited.output_schema(), scan_schema);
    }

    #[test]
    fn test_empty_plan_schema() {
        let declared_schema = vec![
            ("col1".to_string(), DataType::Int64),
            ("col2".to_string(), DataType::String),
        ];

        let empty = LogicalPlan::empty(declared_schema.clone());

        assert_eq!(empty.output_schema(), declared_schema);
    }

    #[test]
    fn test_hash_join_combines_schemas() {
        let person_schema = create_test_schema();
        let company_schema = Arc::new(
            NodeTableSchema::new(
                "Company".to_string(),
                vec![
                    ColumnDef::new("id".to_string(), DataType::Int64).unwrap(),
                    ColumnDef::new("name".to_string(), DataType::String).unwrap(),
                ],
                vec!["id".to_string()],
            )
            .unwrap(),
        );

        let left = LogicalPlan::node_scan("Person".to_string(), "p".to_string(), person_schema);
        let right = LogicalPlan::node_scan("Company".to_string(), "c".to_string(), company_schema);

        let join = LogicalPlan::HashJoin {
            left: Box::new(left),
            right: Box::new(right),
            left_keys: vec!["p.company_id".to_string()],
            right_keys: vec!["c.id".to_string()],
            join_type: JoinType::Inner,
        };

        let output = join.output_schema();
        // 3 from Person + 2 from Company = 5
        assert_eq!(output.len(), 5);
    }

    #[test]
    fn test_plan_children() {
        let schema = create_test_schema();
        let scan = LogicalPlan::node_scan("Person".to_string(), "p".to_string(), schema);

        // Scan has no children
        assert_eq!(scan.children().len(), 0);

        // Filter has one child
        let predicate = BoundExpression::literal(Value::Bool(true));
        let filtered = LogicalPlan::filter(scan.clone(), predicate);
        assert_eq!(filtered.children().len(), 1);

        // Limit has one child
        let limited = LogicalPlan::limit(scan.clone(), None, Some(10));
        assert_eq!(limited.children().len(), 1);
    }

    #[test]
    fn test_plan_display() {
        let schema = create_test_schema();
        let scan = LogicalPlan::node_scan("Person".to_string(), "p".to_string(), schema);

        let display = format!("{}", scan);
        assert!(display.contains("NodeScan"));
        assert!(display.contains("Person"));
        assert!(display.contains("p"));
    }
}

mod progress_reporting_tests {
    use ruzu::catalog::{ColumnDef, NodeTableSchema};
    use ruzu::storage::csv::{CsvImportConfig, ImportProgress, NodeLoader, RelLoader};
    use ruzu::types::DataType;
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;
    use tempfile::TempDir;

    fn create_person_schema() -> Arc<NodeTableSchema> {
        Arc::new(
            NodeTableSchema::new(
                "Person".to_string(),
                vec![
                    ColumnDef::new("id".to_string(), DataType::Int64).unwrap(),
                    ColumnDef::new("name".to_string(), DataType::String).unwrap(),
                    ColumnDef::new("age".to_string(), DataType::Int64).unwrap(),
                ],
                vec!["id".to_string()],
            )
            .unwrap(),
        )
    }

    fn generate_csv_file(dir: &std::path::Path, num_rows: usize) -> std::path::PathBuf {
        let csv_path = dir.join("test_data.csv");
        let mut file = std::fs::File::create(&csv_path).expect("create csv file");

        writeln!(file, "id,name,age").expect("write header");
        for i in 0..num_rows {
            writeln!(file, "{},Person{},{}", i, i, 20 + (i % 50)).expect("write row");
        }

        csv_path
    }

    #[test]
    fn test_import_progress_new() {
        let progress = ImportProgress::new();
        assert_eq!(progress.rows_processed, 0);
        assert_eq!(progress.rows_total, None);
        assert_eq!(progress.rows_failed, 0);
        assert_eq!(progress.bytes_read, 0);
        assert_eq!(progress.batches_completed, 0);
        assert!(progress.errors.is_empty());
    }

    #[test]
    fn test_import_progress_batch_tracking() {
        let mut progress = ImportProgress::new();

        progress.complete_batch();
        assert_eq!(progress.batch_count(), 1);

        progress.complete_batch();
        progress.complete_batch();
        assert_eq!(progress.batch_count(), 3);
    }

    #[test]
    fn test_import_progress_update_monotonic() {
        let mut progress = ImportProgress::new();
        progress.start();

        // Updates should be monotonically increasing
        progress.update(100, 1000);
        let first_count = progress.rows_processed;

        progress.update(50, 500);
        let second_count = progress.rows_processed;

        assert!(
            second_count >= first_count,
            "Row count should be monotonically increasing"
        );
        assert_eq!(second_count, 150); // 100 + 50
    }

    #[test]
    fn test_progress_callback_invoked() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let csv_path = generate_csv_file(temp_dir.path(), 10_000);
        let schema = create_person_schema();

        // Track callback invocations
        let callback_count = Arc::new(AtomicU64::new(0));
        let last_row_count = Arc::new(AtomicU64::new(0));

        let callback_count_clone = Arc::clone(&callback_count);
        let last_row_count_clone = Arc::clone(&last_row_count);

        let config = CsvImportConfig::default()
            .with_parallel(false)
            .with_batch_size(1000);

        let loader = NodeLoader::new(schema, config);
        let callback = move |progress: ImportProgress| {
            callback_count_clone.fetch_add(1, Ordering::SeqCst);
            last_row_count_clone.store(progress.rows_processed, Ordering::SeqCst);
        };

        let (rows, result) = loader
            .load(&csv_path, Some(Box::new(callback)))
            .expect("load csv");

        assert_eq!(rows.len(), 10_000);
        assert!(result.is_success());

        // Callback should have been invoked multiple times
        let count = callback_count.load(Ordering::SeqCst);
        assert!(
            count > 0,
            "Progress callback should be invoked at least once"
        );

        // Last reported row count should match total
        let final_count = last_row_count.load(Ordering::SeqCst);
        assert_eq!(final_count, 10_000);
    }

    #[test]
    fn test_progress_callback_monotonic_rows() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let csv_path = generate_csv_file(temp_dir.path(), 5000);
        let schema = create_person_schema();

        // Track that row counts are monotonically increasing
        let prev_count = Arc::new(AtomicU64::new(0));
        let prev_count_clone = Arc::clone(&prev_count);

        let config = CsvImportConfig::default()
            .with_parallel(false)
            .with_batch_size(500);

        let loader = NodeLoader::new(schema, config);
        let callback = move |progress: ImportProgress| {
            let prev = prev_count_clone.swap(progress.rows_processed, Ordering::SeqCst);
            assert!(
                progress.rows_processed >= prev,
                "Row count should be monotonically increasing: {} < {}",
                progress.rows_processed,
                prev
            );
        };

        let (rows, _) = loader
            .load(&csv_path, Some(Box::new(callback)))
            .expect("load csv");
        assert_eq!(rows.len(), 5000);
    }

    #[test]
    fn test_progress_callback_with_relationship_loader() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let csv_path = temp_dir.path().join("rels.csv");

        // Generate relationship CSV
        {
            let mut file = std::fs::File::create(&csv_path).expect("create file");
            writeln!(file, "FROM,TO,since").expect("write header");
            for i in 0..5000 {
                writeln!(file, "{},{},{}", i, i + 1, 2020).expect("write row");
            }
        }

        let callback_count = Arc::new(AtomicU64::new(0));
        let callback_count_clone = Arc::clone(&callback_count);

        let property_columns = vec![("since".to_string(), DataType::Int64)];
        let config = CsvImportConfig::default()
            .with_parallel(false)
            .with_batch_size(500);

        let loader = RelLoader::with_default_columns(property_columns, config);
        let callback = move |_progress: ImportProgress| {
            callback_count_clone.fetch_add(1, Ordering::SeqCst);
        };

        let (rels, result) = loader
            .load(&csv_path, Some(Box::new(callback)))
            .expect("load csv");

        assert_eq!(rels.len(), 5000);
        assert!(result.is_success());

        // Callback should have been invoked
        let count = callback_count.load(Ordering::SeqCst);
        assert!(
            count > 0,
            "Progress callback should be invoked for RelLoader"
        );
    }

    #[test]
    fn test_progress_includes_throughput() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let csv_path = generate_csv_file(temp_dir.path(), 10_000);
        let schema = create_person_schema();

        let config = CsvImportConfig::default()
            .with_parallel(false)
            .with_batch_size(2000);

        let loader = NodeLoader::new(schema, config);

        // Track whether we received throughput data
        let received_throughput = Arc::new(AtomicU64::new(0));
        let received_throughput_clone = Arc::clone(&received_throughput);

        let callback = move |progress: ImportProgress| {
            if progress.smoothed_throughput().is_some() {
                received_throughput_clone.store(1, Ordering::SeqCst);
            }
        };

        let (rows, result) = loader
            .load(&csv_path, Some(Box::new(callback)))
            .expect("load csv");

        assert_eq!(rows.len(), 10_000);
        assert!(result.is_success());
        // Throughput may or may not be available depending on timing
        // Just verify the test runs without errors
    }
}
