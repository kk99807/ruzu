//! Projection pushdown optimization rule.

use std::collections::HashSet;

use crate::binder::BoundExpression;
use crate::error::Result;

use super::{OptimizerRule, Transformed};
use crate::planner::logical_plan::LogicalPlan;

/// Projection pushdown rule.
///
/// Pushes projection (column selection) closer to the data source
/// to avoid reading unnecessary columns.
pub struct ProjectionPushdownRule;

impl OptimizerRule for ProjectionPushdownRule {
    fn name(&self) -> &str {
        "ProjectionPushdown"
    }

    fn rewrite(&self, plan: LogicalPlan) -> Result<Transformed<LogicalPlan>> {
        match plan {
            LogicalPlan::Project { input, expressions } => {
                // Collect required columns from expressions
                let required_columns = collect_required_columns(&expressions);

                match *input {
                    LogicalPlan::NodeScan {
                        table_name,
                        variable,
                        schema,
                        pushed_filters,
                        projection: _,
                    } => {
                        // Extract column names (without variable prefix) that belong to this variable
                        let prefix = format!("{}.", variable);
                        let projection: Vec<String> = required_columns
                            .iter()
                            .filter(|col| col.starts_with(&prefix))
                            .map(|col| col.strip_prefix(&prefix).unwrap().to_string())
                            .collect();

                        if projection.is_empty() {
                            // No columns needed from this scan, keep original
                            Ok(Transformed::No(LogicalPlan::Project {
                                input: Box::new(LogicalPlan::NodeScan {
                                    table_name,
                                    variable,
                                    schema,
                                    pushed_filters,
                                    projection: None,
                                }),
                                expressions,
                            }))
                        } else {
                            // Push projection to scan
                            Ok(Transformed::Yes(LogicalPlan::Project {
                                input: Box::new(LogicalPlan::NodeScan {
                                    table_name,
                                    variable,
                                    schema,
                                    pushed_filters,
                                    projection: Some(projection),
                                }),
                                expressions,
                            }))
                        }
                    }
                    other => {
                        // Can't push further
                        Ok(Transformed::No(LogicalPlan::Project {
                            input: Box::new(other),
                            expressions,
                        }))
                    }
                }
            }
            _ => Ok(Transformed::No(plan)),
        }
    }
}

/// Collects all column references from a list of expressions.
fn collect_required_columns(expressions: &[(String, BoundExpression)]) -> HashSet<String> {
    let mut columns = HashSet::new();
    for (_, expr) in expressions {
        collect_columns_from_expr(expr, &mut columns);
    }
    columns
}

/// Recursively collects column references from an expression.
fn collect_columns_from_expr(expr: &BoundExpression, columns: &mut HashSet<String>) {
    match expr {
        BoundExpression::PropertyAccess { variable, property, .. } => {
            columns.insert(format!("{}.{}", variable, property));
        }
        BoundExpression::VariableRef { variable, .. } => {
            columns.insert(variable.clone());
        }
        BoundExpression::Comparison { left, right, .. } => {
            collect_columns_from_expr(left, columns);
            collect_columns_from_expr(right, columns);
        }
        BoundExpression::Logical { operands, .. } => {
            for operand in operands {
                collect_columns_from_expr(operand, columns);
            }
        }
        BoundExpression::Arithmetic { left, right, .. } => {
            collect_columns_from_expr(left, columns);
            collect_columns_from_expr(right, columns);
        }
        BoundExpression::Aggregate { input, .. } => {
            if let Some(inner) = input {
                collect_columns_from_expr(inner, columns);
            }
        }
        BoundExpression::IsNull { operand, .. } => {
            collect_columns_from_expr(operand, columns);
        }
        BoundExpression::Literal { .. } => {
            // Literals don't reference columns
        }
    }
}
