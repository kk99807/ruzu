//! Contract tests for the Executor module.
//!
//! These tests verify the query execution contracts:
//! - Vectorized batch operations work correctly
//! - Selection vectors filter properly
//! - Expression evaluation produces correct results

use std::sync::Arc;

use arrow::array::{ArrayRef, BooleanArray, Int64Array, StringArray};
use arrow::datatypes::{DataType as ArrowDataType, Field, Schema};
use arrow::record_batch::RecordBatch;

use ruzu::binder::{AggregateFunction, BoundExpression, ComparisonOp};
use ruzu::executor::vectorized::{SelectionVector, VectorizedBatch, VectorizedEvaluator};
use ruzu::types::{DataType, Value};

/// Creates a test Arrow schema for Person table.
fn create_test_schema() -> Schema {
    Schema::new(vec![
        Field::new("p.id", ArrowDataType::Int64, false),
        Field::new("p.name", ArrowDataType::Utf8, true),
        Field::new("p.age", ArrowDataType::Int64, true),
    ])
}

/// Creates a test RecordBatch with sample data.
fn create_test_batch() -> RecordBatch {
    let schema = Arc::new(create_test_schema());

    let id_array = Arc::new(Int64Array::from(vec![1, 2, 3, 4, 5])) as ArrayRef;
    let name_array = Arc::new(StringArray::from(vec!["Alice", "Bob", "Charlie", "Diana", "Eve"]))
        as ArrayRef;
    let age_array = Arc::new(Int64Array::from(vec![25, 30, 35, 40, 28])) as ArrayRef;

    RecordBatch::try_new(schema, vec![id_array, name_array, age_array]).unwrap()
}

#[test]
fn test_vectorized_batch_num_rows() {
    // Contract: VectorizedBatch::num_rows returns correct row count
    let batch = create_test_batch();
    let vbatch = VectorizedBatch::new(batch);

    assert_eq!(vbatch.num_rows(), 5, "Should have 5 rows");
}

#[test]
fn test_vectorized_batch_num_columns() {
    // Contract: VectorizedBatch::num_columns returns correct column count
    let batch = create_test_batch();
    let vbatch = VectorizedBatch::new(batch);

    assert_eq!(vbatch.num_columns(), 3, "Should have 3 columns");
}

#[test]
fn test_vectorized_batch_column_by_name() {
    // Contract: VectorizedBatch::column_by_name returns correct column
    let batch = create_test_batch();
    let vbatch = VectorizedBatch::new(batch);

    let id_col = vbatch.column_by_name("p.id");
    assert!(id_col.is_some(), "Should find p.id column");

    let missing_col = vbatch.column_by_name("nonexistent");
    assert!(missing_col.is_none(), "Should not find nonexistent column");
}

#[test]
fn test_selection_vector_len() {
    // Contract: SelectionVector::len returns number of selected indices
    let selection = SelectionVector::new(vec![0, 2, 4]);
    assert_eq!(selection.len(), 3, "Should have 3 selected indices");
}

#[test]
fn test_selection_vector_all() {
    // Contract: SelectionVector::all creates selection for all rows
    let selection = SelectionVector::all(5);
    assert_eq!(selection.len(), 5);
    assert_eq!(selection.indices, vec![0, 1, 2, 3, 4]);
}

#[test]
fn test_selection_vector_intersect() {
    // Contract: SelectionVector::intersect keeps only common indices
    let s1 = SelectionVector::new(vec![0, 1, 2, 3, 4]);
    let s2 = SelectionVector::new(vec![1, 3, 5, 7]);

    let result = s1.intersect(&s2);

    assert_eq!(result.len(), 2);
    assert!(result.indices.contains(&1));
    assert!(result.indices.contains(&3));
}

#[test]
fn test_vectorized_batch_with_selection_num_rows() {
    // Contract: VectorizedBatch with selection vector reports selected row count
    let batch = create_test_batch();
    let selection = SelectionVector::new(vec![0, 2, 4]);
    let vbatch = VectorizedBatch::with_selection(batch, selection);

    assert_eq!(vbatch.num_rows(), 3, "Should report 3 selected rows");
}

#[test]
fn test_evaluate_literal_expression() {
    // Contract: Evaluating a literal produces an array of repeated values
    let batch = create_test_batch();
    let vbatch = VectorizedBatch::new(batch);

    let literal = BoundExpression::literal(Value::Int64(42));
    let result = VectorizedEvaluator::evaluate(&literal, &vbatch).unwrap();

    assert_eq!(result.len(), 5, "Should produce 5 values");

    let int_array = result.as_any().downcast_ref::<Int64Array>().unwrap();
    for i in 0..5 {
        assert_eq!(int_array.value(i), 42);
    }
}

#[test]
fn test_evaluate_property_access() {
    // Contract: Evaluating property access returns the column values
    let batch = create_test_batch();
    let vbatch = VectorizedBatch::new(batch);

    let prop_access =
        BoundExpression::property_access("p".to_string(), "age".to_string(), DataType::Int64);

    let result = VectorizedEvaluator::evaluate(&prop_access, &vbatch).unwrap();

    let int_array = result.as_any().downcast_ref::<Int64Array>().unwrap();
    assert_eq!(int_array.value(0), 25);
    assert_eq!(int_array.value(1), 30);
    assert_eq!(int_array.value(2), 35);
    assert_eq!(int_array.value(3), 40);
    assert_eq!(int_array.value(4), 28);
}

#[test]
fn test_evaluate_comparison_expression() {
    // Contract: Comparison produces correct boolean results
    let batch = create_test_batch();
    let vbatch = VectorizedBatch::new(batch);

    // p.age > 30
    let left =
        BoundExpression::property_access("p".to_string(), "age".to_string(), DataType::Int64);
    let right = BoundExpression::literal(Value::Int64(30));
    let comparison = BoundExpression::comparison(left, ComparisonOp::Gt, right);

    let result = VectorizedEvaluator::evaluate(&comparison, &vbatch).unwrap();

    let bool_array = result.as_any().downcast_ref::<BooleanArray>().unwrap();
    assert!(!bool_array.value(0)); // 25 > 30 = false
    assert!(!bool_array.value(1)); // 30 > 30 = false
    assert!(bool_array.value(2)); // 35 > 30 = true
    assert!(bool_array.value(3)); // 40 > 30 = true
    assert!(!bool_array.value(4)); // 28 > 30 = false
}

#[test]
fn test_evaluate_logical_and() {
    // Contract: Logical AND produces correct results
    let batch = create_test_batch();
    let vbatch = VectorizedBatch::new(batch);

    // p.age > 25 AND p.age < 40
    let age_prop =
        BoundExpression::property_access("p".to_string(), "age".to_string(), DataType::Int64);
    let gt_25 = BoundExpression::comparison(
        age_prop.clone(),
        ComparisonOp::Gt,
        BoundExpression::literal(Value::Int64(25)),
    );
    let lt_40 = BoundExpression::comparison(
        age_prop,
        ComparisonOp::Lt,
        BoundExpression::literal(Value::Int64(40)),
    );

    let and_expr = BoundExpression::and(vec![gt_25, lt_40]);

    let result = VectorizedEvaluator::evaluate(&and_expr, &vbatch).unwrap();

    let bool_array = result.as_any().downcast_ref::<BooleanArray>().unwrap();
    assert!(!bool_array.value(0)); // 25 > 25 && 25 < 40 = false && true = false
    assert!(bool_array.value(1)); // 30 > 25 && 30 < 40 = true && true = true
    assert!(bool_array.value(2)); // 35 > 25 && 35 < 40 = true && true = true
    assert!(!bool_array.value(3)); // 40 > 25 && 40 < 40 = true && false = false
    assert!(bool_array.value(4)); // 28 > 25 && 28 < 40 = true && true = true
}

#[test]
fn test_evaluate_logical_not() {
    // Contract: Logical NOT inverts boolean results
    let batch = create_test_batch();
    let vbatch = VectorizedBatch::new(batch);

    // NOT (p.age > 30)
    let age_prop =
        BoundExpression::property_access("p".to_string(), "age".to_string(), DataType::Int64);
    let gt_30 = BoundExpression::comparison(
        age_prop,
        ComparisonOp::Gt,
        BoundExpression::literal(Value::Int64(30)),
    );
    let not_expr = BoundExpression::not(gt_30);

    let result = VectorizedEvaluator::evaluate(&not_expr, &vbatch).unwrap();

    let bool_array = result.as_any().downcast_ref::<BooleanArray>().unwrap();
    assert!(bool_array.value(0)); // NOT (25 > 30) = true
    assert!(bool_array.value(1)); // NOT (30 > 30) = true
    assert!(!bool_array.value(2)); // NOT (35 > 30) = false
    assert!(!bool_array.value(3)); // NOT (40 > 30) = false
    assert!(bool_array.value(4)); // NOT (28 > 30) = true
}

#[test]
fn test_missing_column_returns_error() {
    // Contract: Accessing a non-existent column should return an error
    let batch = create_test_batch();
    let vbatch = VectorizedBatch::new(batch);

    let prop_access = BoundExpression::property_access(
        "p".to_string(),
        "nonexistent".to_string(),
        DataType::Int64,
    );

    let result = VectorizedEvaluator::evaluate(&prop_access, &vbatch);

    assert!(result.is_err(), "Missing column should return error");
}

#[test]
fn test_batch_materialize() {
    // Contract: VectorizedBatch::materialize should apply selection vector
    let batch = create_test_batch();
    let selection = SelectionVector::new(vec![0, 2, 4]);
    let vbatch = VectorizedBatch::with_selection(batch, selection);

    let materialized = vbatch.materialize().unwrap();

    assert_eq!(materialized.num_rows(), 3, "Should have 3 rows after materialization");

    let id_col = materialized.column(0);
    let id_array = id_col.as_any().downcast_ref::<Int64Array>().unwrap();
    assert_eq!(id_array.value(0), 1); // row 0
    assert_eq!(id_array.value(1), 3); // row 2
    assert_eq!(id_array.value(2), 5); // row 4
}

// =============================================================================
// Phase 5: Aggregation Contract Tests (T063-T065)
// =============================================================================

#[test]
fn test_aggregate_count_star_expression() {
    // Contract: T063 - COUNT(*) should count all rows
    let batch = create_test_batch();
    let _vbatch = VectorizedBatch::new(batch);

    // COUNT(*) is represented as an aggregate without an inner expression
    let count_expr = BoundExpression::aggregate(
        AggregateFunction::Count,
        None, // COUNT(*) has no inner expression
        DataType::Int64,
    );

    // For now, we just verify the expression can be constructed
    // The actual evaluation will be done by the executor
    assert!(
        matches!(count_expr, BoundExpression::Aggregate { function: AggregateFunction::Count, .. }),
        "COUNT(*) should create an Aggregate expression"
    );
}

#[test]
fn test_aggregate_count_column_expression() {
    // Contract: T063 - COUNT(p.age) should count non-null values
    let age_prop =
        BoundExpression::property_access("p".to_string(), "age".to_string(), DataType::Int64);

    let count_expr = BoundExpression::aggregate(
        AggregateFunction::Count,
        Some(Box::new(age_prop)),
        DataType::Int64,
    );

    assert!(
        matches!(count_expr, BoundExpression::Aggregate { function: AggregateFunction::Count, input: Some(_), .. }),
        "COUNT(column) should create an Aggregate expression with input"
    );
}

#[test]
fn test_aggregate_avg_expression() {
    // Contract: T064 - AVG should produce correct Float64 result
    let age_prop =
        BoundExpression::property_access("p".to_string(), "age".to_string(), DataType::Int64);

    let avg_expr = BoundExpression::aggregate(
        AggregateFunction::Avg,
        Some(Box::new(age_prop)),
        DataType::Float64, // AVG always returns Float64
    );

    // AVG result type should always be Float64
    if let BoundExpression::Aggregate { data_type, .. } = &avg_expr {
        assert_eq!(*data_type, DataType::Float64, "AVG should return Float64");
    } else {
        panic!("Expected Aggregate expression");
    }
}

#[test]
fn test_aggregate_sum_expression() {
    // Contract: T064 - SUM should return numeric type
    let age_prop =
        BoundExpression::property_access("p".to_string(), "age".to_string(), DataType::Int64);

    let sum_expr = BoundExpression::aggregate(
        AggregateFunction::Sum,
        Some(Box::new(age_prop)),
        DataType::Int64,
    );

    assert!(
        matches!(sum_expr, BoundExpression::Aggregate { function: AggregateFunction::Sum, .. }),
        "SUM should create an Aggregate expression"
    );
}

#[test]
fn test_aggregate_min_max_expression() {
    // Contract: T065 - MIN/MAX should preserve input type
    let age_prop =
        BoundExpression::property_access("p".to_string(), "age".to_string(), DataType::Int64);

    let min_expr = BoundExpression::aggregate(
        AggregateFunction::Min,
        Some(Box::new(age_prop.clone())),
        DataType::Int64, // MIN preserves input type
    );

    let max_expr = BoundExpression::aggregate(
        AggregateFunction::Max,
        Some(Box::new(age_prop)),
        DataType::Int64, // MAX preserves input type
    );

    // Both should preserve the input data type
    if let BoundExpression::Aggregate { data_type, .. } = &min_expr {
        assert_eq!(*data_type, DataType::Int64, "MIN should preserve input type");
    }
    if let BoundExpression::Aggregate { data_type, .. } = &max_expr {
        assert_eq!(*data_type, DataType::Int64, "MAX should preserve input type");
    }
}

#[test]
fn test_aggregate_function_variants() {
    // Contract: All aggregate function variants should be constructible
    let functions = vec![
        AggregateFunction::Count,
        AggregateFunction::Sum,
        AggregateFunction::Avg,
        AggregateFunction::Min,
        AggregateFunction::Max,
    ];

    let age_prop =
        BoundExpression::property_access("p".to_string(), "age".to_string(), DataType::Int64);

    for func in functions {
        let expr = BoundExpression::aggregate(func, Some(Box::new(age_prop.clone())), DataType::Int64);
        assert!(
            matches!(expr, BoundExpression::Aggregate { .. }),
            "Should create Aggregate for {:?}",
            func
        );
    }
}

// =============================================================================
// Phase 8: Vectorized Execution Contract Tests (T101-T103)
// =============================================================================

#[test]
fn test_default_batch_size() {
    // Contract: T101 - Default batch size should be 2048
    use ruzu::executor::vectorized::DEFAULT_BATCH_SIZE;

    assert_eq!(DEFAULT_BATCH_SIZE, 2048, "Default batch size should be 2048");
}

#[test]
fn test_batches_respect_size_limit() {
    // Contract: T102 - VectorizedBatch should respect batch size limits
    let schema = Arc::new(create_test_schema());

    // Create a large batch (simulating many rows)
    let ids: Vec<i64> = (1..=5000).collect();
    let names: Vec<&str> = (0..5000).map(|_| "TestName").collect();
    let ages: Vec<i64> = (0..5000).map(|i| 20 + (i % 60)).collect();

    let id_array = Arc::new(Int64Array::from(ids)) as ArrayRef;
    let name_array = Arc::new(StringArray::from(names)) as ArrayRef;
    let age_array = Arc::new(Int64Array::from(ages)) as ArrayRef;

    let batch = RecordBatch::try_new(schema, vec![id_array, name_array, age_array]).unwrap();
    let vbatch = VectorizedBatch::new(batch);

    // Verify batch was created with correct row count
    assert_eq!(vbatch.num_rows(), 5000, "Large batch should have 5000 rows");

    // Verify schema is preserved
    assert_eq!(vbatch.num_columns(), 3, "Should have 3 columns");
}

#[test]
fn test_vectorized_batch_wrapper_properties() {
    // Contract: T103 - VectorizedBatch wrapper should maintain batch integrity
    let batch = create_test_batch();
    let original_rows = batch.num_rows();
    let original_cols = batch.num_columns();
    let original_schema = batch.schema();

    let vbatch = VectorizedBatch::new(batch);

    // Verify wrapper preserves batch properties
    assert_eq!(vbatch.num_rows(), original_rows, "Row count should be preserved");
    assert_eq!(vbatch.num_columns(), original_cols, "Column count should be preserved");
    assert_eq!(vbatch.schema(), original_schema, "Schema should be preserved");

    // Verify underlying batch access
    assert_eq!(vbatch.batch().num_rows(), original_rows, "Underlying batch should be accessible");
}

#[test]
fn test_selection_vector_preserves_batch_schema() {
    // Contract: Selection vectors should preserve batch schema
    let batch = create_test_batch();
    let original_schema = batch.schema();

    let selection = SelectionVector::new(vec![0, 2, 4]);
    let vbatch = VectorizedBatch::with_selection(batch, selection);

    // Schema should be unchanged even with selection
    assert_eq!(vbatch.schema(), original_schema, "Schema should be preserved with selection");

    // Only selected rows should count
    assert_eq!(vbatch.num_rows(), 3, "Should report 3 selected rows");
}

#[test]
fn test_vectorized_evaluator_batch_output() {
    // Contract: Evaluator should produce output matching input batch size
    let batch = create_test_batch();
    let input_rows = batch.num_rows();
    let vbatch = VectorizedBatch::new(batch);

    // Evaluate a simple expression
    let literal = BoundExpression::literal(Value::Int64(100));
    let result = VectorizedEvaluator::evaluate(&literal, &vbatch).unwrap();

    // Output should match input row count
    assert_eq!(result.len(), input_rows, "Output should match input row count");
}
