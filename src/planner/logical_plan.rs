//! Logical plan definitions.

use std::sync::Arc;

use crate::binder::{BoundExpression, Direction};
use crate::catalog::{NodeTableSchema, RelTableSchema};
use crate::types::DataType;

/// Logical query plan (what to compute).
#[derive(Debug, Clone)]
pub enum LogicalPlan {
    // === Scan Operators ===
    /// Scan a node table.
    NodeScan {
        table_name: String,
        variable: String,
        schema: Arc<NodeTableSchema>,
        /// Filters that can be pushed to scan.
        pushed_filters: Vec<BoundExpression>,
        /// Columns to project (None = all).
        projection: Option<Vec<String>>,
    },

    /// Scan a relationship table directly.
    RelScan {
        table_name: String,
        variable: Option<String>,
        schema: Arc<RelTableSchema>,
        pushed_filters: Vec<BoundExpression>,
        projection: Option<Vec<String>>,
    },

    // === Graph Operators ===
    /// Extend via relationship (single hop).
    Extend {
        input: Box<LogicalPlan>,
        rel_type: String,
        rel_schema: Arc<RelTableSchema>,
        src_variable: String,
        dst_variable: String,
        rel_variable: Option<String>,
        direction: Direction,
    },

    /// Variable-length path expansion.
    PathExpand {
        input: Box<LogicalPlan>,
        rel_type: String,
        rel_schema: Arc<RelTableSchema>,
        src_variable: String,
        dst_variable: String,
        path_variable: Option<String>,
        min_hops: usize,
        max_hops: usize,
        direction: Direction,
    },

    // === Relational Operators ===
    /// Filter rows.
    Filter {
        input: Box<LogicalPlan>,
        predicate: BoundExpression,
    },

    /// Project columns/expressions.
    Project {
        input: Box<LogicalPlan>,
        /// (`output_name`, expression).
        expressions: Vec<(String, BoundExpression)>,
    },

    /// Hash join.
    HashJoin {
        left: Box<LogicalPlan>,
        right: Box<LogicalPlan>,
        left_keys: Vec<String>,
        right_keys: Vec<String>,
        join_type: JoinType,
    },

    /// Aggregation with GROUP BY.
    Aggregate {
        input: Box<LogicalPlan>,
        group_by: Vec<BoundExpression>,
        aggregates: Vec<(String, BoundExpression)>,
    },

    /// Sort results.
    Sort {
        input: Box<LogicalPlan>,
        order_by: Vec<SortExpr>,
    },

    /// Limit/Skip rows.
    Limit {
        input: Box<LogicalPlan>,
        skip: Option<usize>,
        limit: Option<usize>,
    },

    /// Set union.
    Union {
        inputs: Vec<LogicalPlan>,
        all: bool, // UNION vs UNION ALL
    },

    /// Empty result (optimized away).
    Empty {
        schema: Vec<(String, DataType)>,
    },
}

impl LogicalPlan {
    /// Creates a node scan plan.
    #[must_use]
    pub fn node_scan(
        table_name: String,
        variable: String,
        schema: Arc<NodeTableSchema>,
    ) -> Self {
        LogicalPlan::NodeScan {
            table_name,
            variable,
            schema,
            pushed_filters: Vec::new(),
            projection: None,
        }
    }

    /// Creates a filter plan.
    #[must_use]
    pub fn filter(input: LogicalPlan, predicate: BoundExpression) -> Self {
        LogicalPlan::Filter {
            input: Box::new(input),
            predicate,
        }
    }

    /// Creates a project plan.
    #[must_use]
    pub fn project(input: LogicalPlan, expressions: Vec<(String, BoundExpression)>) -> Self {
        LogicalPlan::Project {
            input: Box::new(input),
            expressions,
        }
    }

    /// Creates a limit plan.
    #[must_use]
    pub fn limit(input: LogicalPlan, skip: Option<usize>, limit: Option<usize>) -> Self {
        LogicalPlan::Limit {
            input: Box::new(input),
            skip,
            limit,
        }
    }

    /// Creates an empty plan.
    #[must_use]
    pub fn empty(schema: Vec<(String, DataType)>) -> Self {
        LogicalPlan::Empty { schema }
    }

    /// Returns the output schema of this plan as (name, type) pairs.
    #[must_use]
    pub fn output_schema(&self) -> Vec<(String, DataType)> {
        match self {
            LogicalPlan::NodeScan { schema, variable, projection, .. } => {
                let cols: Vec<_> = if let Some(proj) = projection {
                    schema.columns.iter()
                        .filter(|c| proj.contains(&c.name))
                        .map(|c| (format!("{}.{}", variable, c.name), c.data_type))
                        .collect()
                } else {
                    schema.columns.iter()
                        .map(|c| (format!("{}.{}", variable, c.name), c.data_type))
                        .collect()
                };
                cols
            }
            LogicalPlan::RelScan { schema, variable, projection, .. } => {
                let var = variable.as_deref().unwrap_or("_rel");
                let cols: Vec<_> = if let Some(proj) = projection {
                    schema.columns.iter()
                        .filter(|c| proj.contains(&c.name))
                        .map(|c| (format!("{}.{}", var, c.name), c.data_type))
                        .collect()
                } else {
                    schema.columns.iter()
                        .map(|c| (format!("{}.{}", var, c.name), c.data_type))
                        .collect()
                };
                cols
            }
            LogicalPlan::Extend { input, dst_variable, .. } => {
                let mut schema = input.output_schema();
                // Add destination node columns
                schema.push((format!("{dst_variable}.id"), DataType::Int64));
                schema
            }
            LogicalPlan::PathExpand { input, path_variable, .. } => {
                let mut schema = input.output_schema();
                if let Some(path_var) = path_variable {
                    schema.push((format!("{path_var}.path"), DataType::String));
                }
                schema
            }
            LogicalPlan::Filter { input, .. } => input.output_schema(),
            LogicalPlan::Project { expressions, .. } => {
                expressions.iter()
                    .map(|(name, expr)| (name.clone(), expr.data_type()))
                    .collect()
            }
            LogicalPlan::HashJoin { left, right, .. } => {
                let mut schema = left.output_schema();
                schema.extend(right.output_schema());
                schema
            }
            LogicalPlan::Aggregate { aggregates, group_by, .. } => {
                let mut schema: Vec<_> = group_by.iter()
                    .enumerate()
                    .map(|(i, expr)| (format!("group_{i}"), expr.data_type()))
                    .collect();
                schema.extend(aggregates.iter().map(|(name, expr)| (name.clone(), expr.data_type())));
                schema
            }
            LogicalPlan::Sort { input, .. } => input.output_schema(),
            LogicalPlan::Limit { input, .. } => input.output_schema(),
            LogicalPlan::Union { inputs, .. } => {
                inputs.first().map_or_else(Vec::new, LogicalPlan::output_schema)
            }
            LogicalPlan::Empty { schema } => schema.clone(),
        }
    }

    /// Returns the child plans.
    #[must_use]
    pub fn children(&self) -> Vec<&LogicalPlan> {
        match self {
            LogicalPlan::NodeScan { .. } | LogicalPlan::RelScan { .. } | LogicalPlan::Empty { .. } => {
                vec![]
            }
            LogicalPlan::Extend { input, .. }
            | LogicalPlan::PathExpand { input, .. }
            | LogicalPlan::Filter { input, .. }
            | LogicalPlan::Project { input, .. }
            | LogicalPlan::Aggregate { input, .. }
            | LogicalPlan::Sort { input, .. }
            | LogicalPlan::Limit { input, .. } => {
                vec![input.as_ref()]
            }
            LogicalPlan::HashJoin { left, right, .. } => {
                vec![left.as_ref(), right.as_ref()]
            }
            LogicalPlan::Union { inputs, .. } => inputs.iter().collect(),
        }
    }
}

/// Join type for hash joins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinType {
    Inner,
    LeftOuter,
    RightOuter,
    FullOuter,
}

/// Sort expression.
#[derive(Debug, Clone)]
pub struct SortExpr {
    pub expr: BoundExpression,
    pub ascending: bool,
    pub nulls_first: bool,
}

impl SortExpr {
    /// Creates a new ascending sort expression.
    #[must_use]
    pub fn asc(expr: BoundExpression) -> Self {
        SortExpr {
            expr,
            ascending: true,
            nulls_first: false,
        }
    }

    /// Creates a new descending sort expression.
    #[must_use]
    pub fn desc(expr: BoundExpression) -> Self {
        SortExpr {
            expr,
            ascending: false,
            nulls_first: false,
        }
    }
}

// =============================================================================
// Display implementation for EXPLAIN output (T114-T115)
// =============================================================================

use std::fmt;

impl fmt::Display for LogicalPlan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.format_plan(f, 0)
    }
}

impl LogicalPlan {
    /// Formats the plan as a tree with indentation.
    fn format_plan(&self, f: &mut fmt::Formatter<'_>, indent: usize) -> fmt::Result {
        let prefix = "  ".repeat(indent);
        let child_prefix = "  ".repeat(indent + 1);

        match self {
            LogicalPlan::NodeScan { table_name, variable, pushed_filters, projection, .. } => {
                writeln!(f, "{prefix}NodeScan: {table_name} as {variable}")?;
                if !pushed_filters.is_empty() {
                    writeln!(f, "{child_prefix}  filters: {} predicates pushed", pushed_filters.len())?;
                }
                if let Some(proj) = projection {
                    writeln!(f, "{child_prefix}  projection: [{}]", proj.join(", "))?;
                }
            }
            LogicalPlan::RelScan { table_name, variable, pushed_filters, projection, .. } => {
                let var = variable.as_deref().unwrap_or("_");
                writeln!(f, "{prefix}RelScan: {table_name} as {var}")?;
                if !pushed_filters.is_empty() {
                    writeln!(f, "{child_prefix}  filters: {} predicates pushed", pushed_filters.len())?;
                }
                if let Some(proj) = projection {
                    writeln!(f, "{child_prefix}  projection: [{}]", proj.join(", "))?;
                }
            }
            LogicalPlan::Extend { rel_type, src_variable, dst_variable, direction, input, .. } => {
                writeln!(f, "{prefix}Extend: {rel_type} ({src_variable} -> {dst_variable}) {direction:?}")?;
                input.format_plan(f, indent + 1)?;
            }
            LogicalPlan::PathExpand { rel_type, src_variable, dst_variable, min_hops, max_hops, direction, input, .. } => {
                writeln!(f, "{prefix}PathExpand: {rel_type} ({src_variable} -> {dst_variable}) *{min_hops}..{max_hops} {direction:?}")?;
                input.format_plan(f, indent + 1)?;
            }
            LogicalPlan::Filter { predicate, input } => {
                writeln!(f, "{prefix}Filter: {predicate:?}")?;
                input.format_plan(f, indent + 1)?;
            }
            LogicalPlan::Project { expressions, input } => {
                let expr_names: Vec<_> = expressions.iter().map(|(name, _)| name.as_str()).collect();
                writeln!(f, "{prefix}Project: [{}]", expr_names.join(", "))?;
                input.format_plan(f, indent + 1)?;
            }
            LogicalPlan::HashJoin { left_keys, right_keys, join_type, left, right, .. } => {
                writeln!(f, "{prefix}HashJoin: {join_type:?} ON {} = {}", left_keys.join(", "), right_keys.join(", "))?;
                writeln!(f, "{child_prefix}Build Side:")?;
                left.format_plan(f, indent + 2)?;
                writeln!(f, "{child_prefix}Probe Side:")?;
                right.format_plan(f, indent + 2)?;
            }
            LogicalPlan::Aggregate { group_by, aggregates, input } => {
                let agg_names: Vec<_> = aggregates.iter().map(|(name, _)| name.as_str()).collect();
                writeln!(f, "{prefix}Aggregate: [{}] GROUP BY {} columns", agg_names.join(", "), group_by.len())?;
                input.format_plan(f, indent + 1)?;
            }
            LogicalPlan::Sort { order_by, input } => {
                let orders: Vec<_> = order_by.iter().map(|s| {
                    let dir = if s.ascending { "ASC" } else { "DESC" };
                    format!("{:?} {}", s.expr, dir)
                }).collect();
                writeln!(f, "{prefix}Sort: [{}]", orders.join(", "))?;
                input.format_plan(f, indent + 1)?;
            }
            LogicalPlan::Limit { skip, limit, input } => {
                let skip_str = skip.map_or(String::new(), |s| format!("SKIP {s}"));
                let limit_str = limit.map_or(String::new(), |l| format!("LIMIT {l}"));
                writeln!(f, "{prefix}Limit: {skip_str} {limit_str}")?;
                input.format_plan(f, indent + 1)?;
            }
            LogicalPlan::Union { inputs, all } => {
                let union_type = if *all { "UNION ALL" } else { "UNION" };
                writeln!(f, "{prefix}{union_type}: {} inputs", inputs.len())?;
                for (i, input) in inputs.iter().enumerate() {
                    writeln!(f, "{child_prefix}Input {i}:")?;
                    input.format_plan(f, indent + 2)?;
                }
            }
            LogicalPlan::Empty { schema } => {
                writeln!(f, "{prefix}Empty: {} columns", schema.len())?;
            }
        }
        Ok(())
    }
}

impl fmt::Display for JoinType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JoinType::Inner => write!(f, "INNER"),
            JoinType::LeftOuter => write!(f, "LEFT OUTER"),
            JoinType::RightOuter => write!(f, "RIGHT OUTER"),
            JoinType::FullOuter => write!(f, "FULL OUTER"),
        }
    }
}
