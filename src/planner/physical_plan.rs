//! Physical plan mapping from logical to physical operators.

use std::fmt::Write;

use crate::binder::Direction;
use crate::catalog::Catalog;

use super::logical_plan::LogicalPlan;

/// Maps logical plan to physical plan.
pub struct PlanMapper<'a> {
    _catalog: &'a Catalog,
}

impl<'a> PlanMapper<'a> {
    /// Creates a new plan mapper.
    #[must_use]
    pub fn new(catalog: &'a Catalog) -> Self {
        PlanMapper { _catalog: catalog }
    }

    /// Converts a logical plan to a description string for debugging.
    ///
    /// In the full implementation, this would return Arc<dyn ExecutionPlan>.
    #[must_use]
    pub fn describe(&self, plan: &LogicalPlan) -> String {
        Self::describe_plan(plan, 0)
    }

    fn describe_plan(plan: &LogicalPlan, indent: usize) -> String {
        let prefix = "  ".repeat(indent);
        match plan {
            LogicalPlan::NodeScan { table_name, variable, pushed_filters, projection, .. } => {
                let mut desc = format!("{prefix}NodeScan [{table_name} as {variable}]");
                if !pushed_filters.is_empty() {
                    let _ = write!(desc, " filters={}", pushed_filters.len());
                }
                if let Some(proj) = projection {
                    let _ = write!(desc, " projection={proj:?}");
                }
                desc
            }
            LogicalPlan::RelScan { table_name, variable, .. } => {
                let var = variable.as_deref().unwrap_or("_");
                format!("{prefix}RelScan [{table_name} as {var}]")
            }
            LogicalPlan::Extend { input, rel_type, direction, dst_variable, .. } => {
                let dir = match direction {
                    Direction::Forward => "FORWARD",
                    Direction::Backward => "BACKWARD",
                    Direction::Both => "BOTH",
                };
                let child = Self::describe_plan(input, indent + 1);
                format!("{prefix}Extend [{rel_type}, {dir}] -> {dst_variable}\n{child}")
            }
            LogicalPlan::PathExpand { input, rel_type, min_hops, max_hops, .. } => {
                let child = Self::describe_plan(input, indent + 1);
                format!("{prefix}PathExpand [{rel_type}*{min_hops}..{max_hops}]\n{child}")
            }
            LogicalPlan::Filter { input, .. } => {
                let child = Self::describe_plan(input, indent + 1);
                format!("{prefix}Filter\n{child}")
            }
            LogicalPlan::Project { input, expressions } => {
                let cols: Vec<_> = expressions.iter().map(|(name, _)| name.as_str()).collect();
                let child = Self::describe_plan(input, indent + 1);
                format!("{prefix}Project [{cols:?}]\n{child}")
            }
            LogicalPlan::HashJoin { left, right, left_keys, right_keys, join_type } => {
                let left_child = Self::describe_plan(left, indent + 1);
                let right_child = Self::describe_plan(right, indent + 1);
                format!(
                    "{prefix}HashJoin [{join_type:?}] on {left_keys:?} = {right_keys:?}\n{left_child}\n{right_child}"
                )
            }
            LogicalPlan::Aggregate { input, group_by, aggregates } => {
                let aggs: Vec<_> = aggregates.iter().map(|(name, _)| name.as_str()).collect();
                let child = Self::describe_plan(input, indent + 1);
                format!(
                    "{prefix}Aggregate [group_by: {}, agg: {:?}]\n{child}",
                    group_by.len(), aggs
                )
            }
            LogicalPlan::Sort { input, order_by } => {
                let child = Self::describe_plan(input, indent + 1);
                format!("{prefix}Sort [{}]\n{child}", order_by.len())
            }
            LogicalPlan::Limit { input, skip, limit } => {
                let child = Self::describe_plan(input, indent + 1);
                format!("{prefix}Limit [skip={skip:?}, limit={limit:?}]\n{child}")
            }
            LogicalPlan::Union { inputs, all } => {
                let children: Vec<_> = inputs.iter()
                    .map(|p| Self::describe_plan(p, indent + 1))
                    .collect();
                let kind = if *all { "UNION ALL" } else { "UNION" };
                format!("{prefix}{kind}\n{}", children.join("\n"))
            }
            LogicalPlan::Empty { schema } => {
                format!("{prefix}Empty [cols={}]", schema.len())
            }
        }
    }
}
