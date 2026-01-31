//! Vectorized expression evaluator.

use std::sync::Arc;

use arrow::array::{
    ArrayRef, BooleanArray, Float32Array, Float64Array, Int64Array, StringArray,
    TimestampMicrosecondArray,
};
use arrow::compute::kernels::boolean::{and, not, or};
use arrow::compute::kernels::cmp::{eq, gt, gt_eq, lt, lt_eq, neq};
use arrow::compute::kernels::numeric::{add, div, mul, rem, sub};
use arrow::compute::is_null;
use arrow::datatypes::DataType as ArrowDataType;
use arrow::error::Result as ArrowResult;

use crate::binder::{ArithmeticOp, BoundExpression, ComparisonOp, LogicalOp};
use crate::types::Value;

use super::batch::VectorizedBatch;

/// Vectorized expression evaluator.
///
/// Evaluates expressions over Arrow arrays in a batch-oriented fashion.
pub struct VectorizedEvaluator;

impl VectorizedEvaluator {
    /// Evaluates a bound expression against a vectorized batch.
    ///
    /// Returns an Arrow array containing the result.
    ///
    /// # Errors
    ///
    /// Returns an Arrow error if the expression references a missing column or
    /// if an arithmetic/comparison operation fails.
    pub fn evaluate(expr: &BoundExpression, batch: &VectorizedBatch) -> ArrowResult<ArrayRef> {
        match expr {
            BoundExpression::Literal { value, .. } => {
                Self::create_literal_array(value, batch.num_rows())
            }
            BoundExpression::PropertyAccess {
                variable, property, ..
            } => {
                let col_name = format!("{variable}.{property}");
                batch.column_by_name(&col_name).cloned().ok_or_else(|| {
                    arrow::error::ArrowError::InvalidArgumentError(format!(
                        "Column not found: {col_name}"
                    ))
                })
            }
            BoundExpression::VariableRef { variable, .. } => {
                batch.column_by_name(variable).cloned().ok_or_else(|| {
                    arrow::error::ArrowError::InvalidArgumentError(format!(
                        "Column not found: {variable}"
                    ))
                })
            }
            BoundExpression::Comparison {
                left, op, right, ..
            } => {
                let left_arr = Self::evaluate(left, batch)?;
                let right_arr = Self::evaluate(right, batch)?;
                Self::compare(&left_arr, *op, &right_arr)
            }
            BoundExpression::Logical { op, operands, .. } => {
                Self::evaluate_logical(*op, operands, batch)
            }
            BoundExpression::Arithmetic {
                left, op, right, ..
            } => {
                let left_arr = Self::evaluate(left, batch)?;
                let right_arr = Self::evaluate(right, batch)?;
                Self::arithmetic(&left_arr, *op, &right_arr)
            }
            BoundExpression::Aggregate { .. } => Err(arrow::error::ArrowError::NotYetImplemented(
                "Aggregates should be evaluated separately".to_string(),
            )),
            BoundExpression::IsNull { operand, negated, .. } => {
                let arr = Self::evaluate(operand, batch)?;
                let nulls = is_null(&arr)?;
                if *negated {
                    not(&nulls).map(|a| Arc::new(a) as ArrayRef)
                } else {
                    Ok(Arc::new(nulls))
                }
            }
        }
    }

    /// Creates a literal array with the same value repeated.
    fn create_literal_array(value: &Value, len: usize) -> ArrowResult<ArrayRef> {
        match value {
            Value::Int64(v) => Ok(Arc::new(Int64Array::from(vec![*v; len]))),
            Value::Float32(v) => Ok(Arc::new(Float32Array::from(vec![*v; len]))),
            Value::Float64(v) => Ok(Arc::new(Float64Array::from(vec![*v; len]))),
            Value::Bool(v) => Ok(Arc::new(BooleanArray::from(vec![*v; len]))),
            Value::String(v) => Ok(Arc::new(StringArray::from(vec![v.as_str(); len]))),
            Value::Date(v) => Ok(Arc::new(arrow::array::Date32Array::from(vec![*v; len]))),
            Value::Timestamp(v) => Ok(Arc::new(TimestampMicrosecondArray::from(vec![*v; len]))),
            Value::Null => {
                // Create a null array of appropriate type (default to Int64)
                let arr = Int64Array::from(vec![None::<i64>; len]);
                Ok(Arc::new(arr))
            }
        }
    }

    /// Compares two arrays using the given operator.
    fn compare(left: &ArrayRef, op: ComparisonOp, right: &ArrayRef) -> ArrowResult<ArrayRef> {
        match (left.data_type(), right.data_type()) {
            (ArrowDataType::Int64, ArrowDataType::Int64) => {
                let left = left.as_any().downcast_ref::<Int64Array>().unwrap();
                let right = right.as_any().downcast_ref::<Int64Array>().unwrap();
                let result = match op {
                    ComparisonOp::Eq => eq(left, right)?,
                    ComparisonOp::Neq => neq(left, right)?,
                    ComparisonOp::Lt => lt(left, right)?,
                    ComparisonOp::Lte => lt_eq(left, right)?,
                    ComparisonOp::Gt => gt(left, right)?,
                    ComparisonOp::Gte => gt_eq(left, right)?,
                };
                Ok(Arc::new(result))
            }
            (ArrowDataType::Float64, ArrowDataType::Float64) => {
                let left = left.as_any().downcast_ref::<Float64Array>().unwrap();
                let right = right.as_any().downcast_ref::<Float64Array>().unwrap();
                let result = match op {
                    ComparisonOp::Eq => eq(left, right)?,
                    ComparisonOp::Neq => neq(left, right)?,
                    ComparisonOp::Lt => lt(left, right)?,
                    ComparisonOp::Lte => lt_eq(left, right)?,
                    ComparisonOp::Gt => gt(left, right)?,
                    ComparisonOp::Gte => gt_eq(left, right)?,
                };
                Ok(Arc::new(result))
            }
            (ArrowDataType::Float32, ArrowDataType::Float32) => {
                let left = left.as_any().downcast_ref::<Float32Array>().unwrap();
                let right = right.as_any().downcast_ref::<Float32Array>().unwrap();
                let result = match op {
                    ComparisonOp::Eq => eq(left, right)?,
                    ComparisonOp::Neq => neq(left, right)?,
                    ComparisonOp::Lt => lt(left, right)?,
                    ComparisonOp::Lte => lt_eq(left, right)?,
                    ComparisonOp::Gt => gt(left, right)?,
                    ComparisonOp::Gte => gt_eq(left, right)?,
                };
                Ok(Arc::new(result))
            }
            (ArrowDataType::Utf8, ArrowDataType::Utf8) => {
                let left = left.as_any().downcast_ref::<StringArray>().unwrap();
                let right = right.as_any().downcast_ref::<StringArray>().unwrap();
                let result = match op {
                    ComparisonOp::Eq => eq(left, right)?,
                    ComparisonOp::Neq => neq(left, right)?,
                    ComparisonOp::Lt => lt(left, right)?,
                    ComparisonOp::Lte => lt_eq(left, right)?,
                    ComparisonOp::Gt => gt(left, right)?,
                    ComparisonOp::Gte => gt_eq(left, right)?,
                };
                Ok(Arc::new(result))
            }
            _ => Err(arrow::error::ArrowError::ComputeError(format!(
                "Unsupported comparison between {:?} and {:?}",
                left.data_type(),
                right.data_type()
            ))),
        }
    }

    /// Evaluates a logical operation.
    fn evaluate_logical(
        op: LogicalOp,
        operands: &[BoundExpression],
        batch: &VectorizedBatch,
    ) -> ArrowResult<ArrayRef> {
        match op {
            LogicalOp::And => {
                let mut result: Option<BooleanArray> = None;
                for operand in operands {
                    let arr = Self::evaluate(operand, batch)?;
                    let bool_arr =
                        arr.as_any()
                            .downcast_ref::<BooleanArray>()
                            .ok_or_else(|| {
                                arrow::error::ArrowError::ComputeError(
                                    "AND operand must be boolean".to_string(),
                                )
                            })?;
                    result = Some(match result {
                        Some(prev) => and(&prev, bool_arr)?,
                        None => bool_arr.clone(),
                    });
                }
                Ok(Arc::new(result.unwrap_or_else(|| {
                    BooleanArray::from(vec![true; batch.num_rows()])
                })))
            }
            LogicalOp::Or => {
                let mut result: Option<BooleanArray> = None;
                for operand in operands {
                    let arr = Self::evaluate(operand, batch)?;
                    let bool_arr =
                        arr.as_any()
                            .downcast_ref::<BooleanArray>()
                            .ok_or_else(|| {
                                arrow::error::ArrowError::ComputeError(
                                    "OR operand must be boolean".to_string(),
                                )
                            })?;
                    result = Some(match result {
                        Some(prev) => or(&prev, bool_arr)?,
                        None => bool_arr.clone(),
                    });
                }
                Ok(Arc::new(result.unwrap_or_else(|| {
                    BooleanArray::from(vec![false; batch.num_rows()])
                })))
            }
            LogicalOp::Not => {
                if let Some(first) = operands.first() {
                    let arr = Self::evaluate(first, batch)?;
                    let bool_arr =
                        arr.as_any()
                            .downcast_ref::<BooleanArray>()
                            .ok_or_else(|| {
                                arrow::error::ArrowError::ComputeError(
                                    "NOT operand must be boolean".to_string(),
                                )
                            })?;
                    Ok(Arc::new(not(bool_arr)?))
                } else {
                    Err(arrow::error::ArrowError::ComputeError(
                        "NOT requires an operand".to_string(),
                    ))
                }
            }
        }
    }

    /// Performs arithmetic on two arrays.
    fn arithmetic(left: &ArrayRef, op: ArithmeticOp, right: &ArrayRef) -> ArrowResult<ArrayRef> {
        match (left.data_type(), right.data_type()) {
            (ArrowDataType::Int64, ArrowDataType::Int64) => {
                let left = left.as_any().downcast_ref::<Int64Array>().unwrap();
                let right = right.as_any().downcast_ref::<Int64Array>().unwrap();
                let result = match op {
                    ArithmeticOp::Add => add(left, right)?,
                    ArithmeticOp::Sub => sub(left, right)?,
                    ArithmeticOp::Mul => mul(left, right)?,
                    ArithmeticOp::Div => div(left, right)?,
                    ArithmeticOp::Mod => rem(left, right)?,
                };
                Ok(Arc::new(result))
            }
            (ArrowDataType::Float64, ArrowDataType::Float64) => {
                let left = left.as_any().downcast_ref::<Float64Array>().unwrap();
                let right = right.as_any().downcast_ref::<Float64Array>().unwrap();
                let result = match op {
                    ArithmeticOp::Add => add(left, right)?,
                    ArithmeticOp::Sub => sub(left, right)?,
                    ArithmeticOp::Mul => mul(left, right)?,
                    ArithmeticOp::Div => div(left, right)?,
                    ArithmeticOp::Mod => rem(left, right)?,
                };
                Ok(Arc::new(result))
            }
            (ArrowDataType::Float32, ArrowDataType::Float32) => {
                let left = left.as_any().downcast_ref::<Float32Array>().unwrap();
                let right = right.as_any().downcast_ref::<Float32Array>().unwrap();
                let result = match op {
                    ArithmeticOp::Add => add(left, right)?,
                    ArithmeticOp::Sub => sub(left, right)?,
                    ArithmeticOp::Mul => mul(left, right)?,
                    ArithmeticOp::Div => div(left, right)?,
                    ArithmeticOp::Mod => rem(left, right)?,
                };
                Ok(Arc::new(result))
            }
            _ => Err(arrow::error::ArrowError::ComputeError(format!(
                "Unsupported arithmetic between {:?} and {:?}",
                left.data_type(),
                right.data_type()
            ))),
        }
    }
}
