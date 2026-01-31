//! Conversion from Cypher bound expressions to `DataFusion` physical expressions.

use std::sync::Arc;

use arrow::datatypes::Schema;
use datafusion::common::ScalarValue;
use datafusion::error::Result as DfResult;
use datafusion::logical_expr::Operator;
use datafusion::physical_expr::expressions::{col, lit, BinaryExpr, IsNullExpr, NotExpr};
use datafusion::physical_expr::PhysicalExpr;

use crate::binder::{ArithmeticOp, BoundExpression, ComparisonOp, LogicalOp};
use crate::types::Value;

/// Converter from Cypher expressions to `DataFusion` physical expressions.
pub struct CypherToDf;

impl CypherToDf {
    /// Converts a bound expression to a `DataFusion` physical expression.
    ///
    /// # Errors
    ///
    /// Returns a `DataFusion` error if the expression references a column not
    /// present in the schema or contains an unsupported expression type.
    pub fn to_physical_expr(
        expr: &BoundExpression,
        schema: &Schema,
    ) -> DfResult<Arc<dyn PhysicalExpr>> {
        match expr {
            BoundExpression::Literal { value, .. } => Ok(Self::value_to_scalar(value)),
            BoundExpression::PropertyAccess {
                variable, property, ..
            } => {
                let col_name = format!("{variable}.{property}");
                col(&col_name, schema)
            }
            BoundExpression::VariableRef { variable, .. } => col(variable, schema),
            BoundExpression::Comparison {
                left, op, right, ..
            } => {
                let left_expr = Self::to_physical_expr(left, schema)?;
                let right_expr = Self::to_physical_expr(right, schema)?;
                let df_op = Self::comparison_to_df_op(*op);
                Ok(Arc::new(BinaryExpr::new(left_expr, df_op, right_expr)))
            }
            BoundExpression::Logical { op, operands, .. } => match op {
                LogicalOp::And => Self::build_logical_chain(operands, schema, Operator::And),
                LogicalOp::Or => Self::build_logical_chain(operands, schema, Operator::Or),
                LogicalOp::Not => {
                    if let Some(first) = operands.first() {
                        let inner = Self::to_physical_expr(first, schema)?;
                        Ok(Arc::new(NotExpr::new(inner)))
                    } else {
                        Err(datafusion::error::DataFusionError::Plan(
                            "NOT expression requires an operand".to_string(),
                        ))
                    }
                }
            },
            BoundExpression::Arithmetic {
                left, op, right, ..
            } => {
                let left_expr = Self::to_physical_expr(left, schema)?;
                let right_expr = Self::to_physical_expr(right, schema)?;
                let df_op = Self::arithmetic_to_df_op(*op);
                Ok(Arc::new(BinaryExpr::new(left_expr, df_op, right_expr)))
            }
            BoundExpression::Aggregate { .. } => {
                Err(datafusion::error::DataFusionError::NotImplemented(
                    "Aggregate expressions should be handled by AggregateExec".to_string(),
                ))
            }
            BoundExpression::IsNull { operand, negated, .. } => {
                let inner = Self::to_physical_expr(operand, schema)?;
                let is_null = Arc::new(IsNullExpr::new(inner));
                if *negated {
                    Ok(Arc::new(NotExpr::new(is_null)))
                } else {
                    Ok(is_null)
                }
            }
        }
    }

    /// Converts a ruzu Value to a `DataFusion` scalar literal.
    fn value_to_scalar(value: &Value) -> Arc<dyn PhysicalExpr> {
        match value {
            Value::Int64(v) => lit(ScalarValue::Int64(Some(*v))),
            Value::Float32(v) => lit(ScalarValue::Float32(Some(*v))),
            Value::Float64(v) => lit(ScalarValue::Float64(Some(*v))),
            Value::Bool(v) => lit(ScalarValue::Boolean(Some(*v))),
            Value::String(v) => lit(ScalarValue::Utf8(Some(v.clone()))),
            Value::Date(v) => lit(ScalarValue::Date32(Some(*v))),
            Value::Timestamp(v) => lit(ScalarValue::TimestampMicrosecond(Some(*v), None)),
            Value::Null => lit(ScalarValue::Null),
        }
    }

    /// Converts a comparison operator to `DataFusion` operator.
    fn comparison_to_df_op(op: ComparisonOp) -> Operator {
        match op {
            ComparisonOp::Eq => Operator::Eq,
            ComparisonOp::Neq => Operator::NotEq,
            ComparisonOp::Lt => Operator::Lt,
            ComparisonOp::Lte => Operator::LtEq,
            ComparisonOp::Gt => Operator::Gt,
            ComparisonOp::Gte => Operator::GtEq,
        }
    }

    /// Converts an arithmetic operator to `DataFusion` operator.
    fn arithmetic_to_df_op(op: ArithmeticOp) -> Operator {
        match op {
            ArithmeticOp::Add => Operator::Plus,
            ArithmeticOp::Sub => Operator::Minus,
            ArithmeticOp::Mul => Operator::Multiply,
            ArithmeticOp::Div => Operator::Divide,
            ArithmeticOp::Mod => Operator::Modulo,
        }
    }

    /// Builds a chain of logical operators (AND/OR) from multiple operands.
    fn build_logical_chain(
        operands: &[BoundExpression],
        schema: &Schema,
        op: Operator,
    ) -> DfResult<Arc<dyn PhysicalExpr>> {
        if operands.is_empty() {
            return Err(datafusion::error::DataFusionError::Plan(
                "Logical expression requires at least one operand".to_string(),
            ));
        }

        let mut result = Self::to_physical_expr(&operands[0], schema)?;
        for operand in &operands[1..] {
            let right = Self::to_physical_expr(operand, schema)?;
            result = Arc::new(BinaryExpr::new(result, op.clone(), right));
        }
        Ok(result)
    }
}
