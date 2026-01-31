//! Filter pushdown optimization rule.

use crate::error::Result;

use super::{OptimizerRule, Transformed};
use crate::planner::logical_plan::LogicalPlan;

/// Filter pushdown rule.
///
/// Pushes filter predicates closer to the data source to reduce
/// the amount of data that needs to be processed.
pub struct FilterPushdownRule;

impl OptimizerRule for FilterPushdownRule {
    fn name(&self) -> &'static str {
        "FilterPushdown"
    }

    fn rewrite(&self, plan: LogicalPlan) -> Result<Transformed<LogicalPlan>> {
        match plan {
            LogicalPlan::Filter { input, predicate } => {
                // Try to push filter below the input
                match *input {
                    LogicalPlan::NodeScan {
                        table_name,
                        variable,
                        schema,
                        mut pushed_filters,
                        projection,
                    } => {
                        // Push filter into scan
                        pushed_filters.push(predicate);
                        Ok(Transformed::Yes(LogicalPlan::NodeScan {
                            table_name,
                            variable,
                            schema,
                            pushed_filters,
                            projection,
                        }))
                    }
                    LogicalPlan::Project { input: proj_input, expressions } => {
                        // Push filter below project
                        let new_filter = LogicalPlan::Filter {
                            input: proj_input,
                            predicate,
                        };
                        Ok(Transformed::Yes(LogicalPlan::Project {
                            input: Box::new(new_filter),
                            expressions,
                        }))
                    }
                    other => {
                        // Can't push further, keep filter here
                        Ok(Transformed::No(LogicalPlan::Filter {
                            input: Box::new(other),
                            predicate,
                        }))
                    }
                }
            }
            _ => Ok(Transformed::No(plan)),
        }
    }
}

#[cfg(test)]
mod tests {
    // Tests will be added when the full binder/expression system is complete
}
