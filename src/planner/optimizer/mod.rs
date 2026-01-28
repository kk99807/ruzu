//! Query optimization rules.
//!
//! This module contains optimizer rules that transform logical plans
//! to more efficient forms.

mod filter_pushdown;
mod projection_pushdown;

pub use filter_pushdown::FilterPushdownRule;
pub use projection_pushdown::ProjectionPushdownRule;

use crate::binder::{BoundExpression, ComparisonOp, LogicalOp};
use crate::error::Result;
use crate::types::Value;

use super::logical_plan::LogicalPlan;

/// Result of optimization transformation.
#[derive(Debug)]
pub enum Transformed<T> {
    /// Plan was modified.
    Yes(T),
    /// Plan unchanged.
    No(T),
}

impl<T> Transformed<T> {
    /// Returns the inner value.
    pub fn into_inner(self) -> T {
        match self {
            Transformed::Yes(v) | Transformed::No(v) => v,
        }
    }

    /// Returns true if the plan was modified.
    #[must_use]
    pub fn was_transformed(&self) -> bool {
        matches!(self, Transformed::Yes(_))
    }
}

/// Optimizer rule trait.
pub trait OptimizerRule: Send + Sync {
    /// Returns the name of this rule.
    fn name(&self) -> &str;

    /// Rewrites the logical plan if applicable.
    fn rewrite(&self, plan: LogicalPlan) -> Result<Transformed<LogicalPlan>>;
}

// ============================================================================
// Helper functions for constant evaluation
// ============================================================================

/// Result of evaluating a constant expression.
#[derive(Debug, Clone, PartialEq)]
pub enum ConstantValue {
    /// Expression is a constant true.
    True,
    /// Expression is a constant false.
    False,
    /// Expression cannot be evaluated at compile time.
    Unknown,
}

/// Tries to evaluate a BoundExpression to a constant boolean value.
fn try_evaluate_constant(expr: &BoundExpression) -> ConstantValue {
    match expr {
        BoundExpression::Literal { value, .. } => match value {
            Value::Bool(b) => {
                if *b {
                    ConstantValue::True
                } else {
                    ConstantValue::False
                }
            }
            _ => ConstantValue::Unknown,
        },
        BoundExpression::Comparison { left, op, right, .. } => {
            if let (
                BoundExpression::Literal { value: left_val, .. },
                BoundExpression::Literal { value: right_val, .. },
            ) = (left.as_ref(), right.as_ref())
            {
                let result = evaluate_comparison(left_val, *op, right_val);
                if result {
                    ConstantValue::True
                } else {
                    ConstantValue::False
                }
            } else {
                ConstantValue::Unknown
            }
        }
        BoundExpression::Logical { op, operands, .. } => match op {
            LogicalOp::And => {
                let mut all_true = true;
                for operand in operands {
                    match try_evaluate_constant(operand) {
                        ConstantValue::False => return ConstantValue::False,
                        ConstantValue::Unknown => all_true = false,
                        ConstantValue::True => {}
                    }
                }
                if all_true {
                    ConstantValue::True
                } else {
                    ConstantValue::Unknown
                }
            }
            LogicalOp::Or => {
                let mut all_false = true;
                for operand in operands {
                    match try_evaluate_constant(operand) {
                        ConstantValue::True => return ConstantValue::True,
                        ConstantValue::Unknown => all_false = false,
                        ConstantValue::False => {}
                    }
                }
                if all_false {
                    ConstantValue::False
                } else {
                    ConstantValue::Unknown
                }
            }
            LogicalOp::Not => {
                if let Some(operand) = operands.first() {
                    match try_evaluate_constant(operand) {
                        ConstantValue::True => ConstantValue::False,
                        ConstantValue::False => ConstantValue::True,
                        ConstantValue::Unknown => ConstantValue::Unknown,
                    }
                } else {
                    ConstantValue::Unknown
                }
            }
        },
        _ => ConstantValue::Unknown,
    }
}

/// Evaluates a comparison between two constant values.
fn evaluate_comparison(left: &Value, op: ComparisonOp, right: &Value) -> bool {
    let ordering = left.compare(right);
    match ordering {
        None => false,
        Some(std::cmp::Ordering::Equal) => {
            matches!(op, ComparisonOp::Eq | ComparisonOp::Lte | ComparisonOp::Gte)
        }
        Some(std::cmp::Ordering::Less) => {
            matches!(op, ComparisonOp::Lt | ComparisonOp::Lte | ComparisonOp::Neq)
        }
        Some(std::cmp::Ordering::Greater) => {
            matches!(op, ComparisonOp::Gt | ComparisonOp::Gte | ComparisonOp::Neq)
        }
    }
}

/// Simplifies a predicate expression by removing constant subexpressions.
fn simplify_predicate(expr: &BoundExpression) -> BoundExpression {
    match expr {
        BoundExpression::Logical { op, operands, data_type } => match op {
            LogicalOp::And => {
                let mut new_operands = Vec::new();
                for operand in operands {
                    let simplified = simplify_predicate(operand);
                    match try_evaluate_constant(&simplified) {
                        ConstantValue::False => {
                            return BoundExpression::literal(Value::Bool(false));
                        }
                        ConstantValue::True => {}
                        ConstantValue::Unknown => {
                            new_operands.push(simplified);
                        }
                    }
                }
                match new_operands.len() {
                    0 => BoundExpression::literal(Value::Bool(true)),
                    1 => new_operands.remove(0),
                    _ => BoundExpression::Logical {
                        op: *op,
                        operands: new_operands,
                        data_type: *data_type,
                    },
                }
            }
            LogicalOp::Or => {
                let mut new_operands = Vec::new();
                for operand in operands {
                    let simplified = simplify_predicate(operand);
                    match try_evaluate_constant(&simplified) {
                        ConstantValue::True => {
                            return BoundExpression::literal(Value::Bool(true));
                        }
                        ConstantValue::False => {}
                        ConstantValue::Unknown => {
                            new_operands.push(simplified);
                        }
                    }
                }
                match new_operands.len() {
                    0 => BoundExpression::literal(Value::Bool(false)),
                    1 => new_operands.remove(0),
                    _ => BoundExpression::Logical {
                        op: *op,
                        operands: new_operands,
                        data_type: *data_type,
                    },
                }
            }
            LogicalOp::Not => {
                if let Some(operand) = operands.first() {
                    let simplified = simplify_predicate(operand);
                    match try_evaluate_constant(&simplified) {
                        ConstantValue::True => BoundExpression::literal(Value::Bool(false)),
                        ConstantValue::False => BoundExpression::literal(Value::Bool(true)),
                        ConstantValue::Unknown => BoundExpression::not(simplified),
                    }
                } else {
                    expr.clone()
                }
            }
        },
        BoundExpression::Comparison { left, op, right, .. } => {
            if let (
                BoundExpression::Literal { value: left_val, .. },
                BoundExpression::Literal { value: right_val, .. },
            ) = (left.as_ref(), right.as_ref())
            {
                let result = evaluate_comparison(left_val, *op, right_val);
                BoundExpression::literal(Value::Bool(result))
            } else {
                expr.clone()
            }
        }
        _ => expr.clone(),
    }
}

/// Predicate simplification rule.
///
/// Simplifies constant expressions like WHERE 1=0 → EmptyResult.
pub struct PredicateSimplificationRule;

impl OptimizerRule for PredicateSimplificationRule {
    fn name(&self) -> &str {
        "PredicateSimplification"
    }

    fn rewrite(&self, plan: LogicalPlan) -> Result<Transformed<LogicalPlan>> {
        match plan {
            LogicalPlan::Filter { input, predicate } => {
                // Simplify the predicate
                let simplified = simplify_predicate(&predicate);
                let constant_val = try_evaluate_constant(&simplified);

                match constant_val {
                    ConstantValue::False => {
                        let schema = input.output_schema();
                        Ok(Transformed::Yes(LogicalPlan::Empty { schema }))
                    }
                    ConstantValue::True => {
                        Ok(Transformed::Yes(*input))
                    }
                    ConstantValue::Unknown => {
                        // Check if predicate changed
                        if format!("{:?}", predicate) != format!("{:?}", simplified) {
                            Ok(Transformed::Yes(LogicalPlan::Filter {
                                input,
                                predicate: simplified,
                            }))
                        } else {
                            Ok(Transformed::No(LogicalPlan::Filter { input, predicate }))
                        }
                    }
                }
            }
            _ => Ok(Transformed::No(plan)),
        }
    }
}

/// Constant folding rule.
///
/// Evaluates constant expressions at planning time.
pub struct ConstantFoldingRule;

impl OptimizerRule for ConstantFoldingRule {
    fn name(&self) -> &str {
        "ConstantFolding"
    }

    fn rewrite(&self, plan: LogicalPlan) -> Result<Transformed<LogicalPlan>> {
        match plan {
            LogicalPlan::Filter { input, predicate } => {
                let constant_val = try_evaluate_constant(&predicate);

                match constant_val {
                    ConstantValue::False => {
                        let schema = input.output_schema();
                        Ok(Transformed::Yes(LogicalPlan::Empty { schema }))
                    }
                    ConstantValue::True => {
                        Ok(Transformed::Yes(*input))
                    }
                    ConstantValue::Unknown => {
                        Ok(Transformed::No(LogicalPlan::Filter { input, predicate }))
                    }
                }
            }
            _ => Ok(Transformed::No(plan)),
        }
    }
}
