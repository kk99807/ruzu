//! Query planner module.
//!
//! The planner transforms bound queries into logical plans and then
//! applies optimization rules to produce efficient execution plans.

pub mod logical_plan;
pub mod optimizer;
pub mod physical_plan;

pub use logical_plan::{JoinType, LogicalPlan, SortExpr};
pub use optimizer::{
    ConstantFoldingRule, FilterPushdownRule, OptimizerRule, PredicateSimplificationRule,
    ProjectionPushdownRule, Transformed,
};
pub use physical_plan::PlanMapper;

use crate::binder::{BoundQuery, QueryGraph};
use crate::catalog::Catalog;
use crate::error::{Result, RuzuError};

/// Query planner.
pub struct Planner<'a> {
    _catalog: &'a Catalog,
    optimizer_rules: Vec<Box<dyn OptimizerRule>>,
}

impl<'a> Planner<'a> {
    /// Creates a new planner with default optimizer rules.
    #[must_use]
    pub fn new(catalog: &'a Catalog) -> Self {
        let optimizer_rules: Vec<Box<dyn OptimizerRule>> = vec![
            Box::new(FilterPushdownRule),
            Box::new(ProjectionPushdownRule),
            Box::new(PredicateSimplificationRule),
            Box::new(ConstantFoldingRule),
        ];

        Planner {
            _catalog: catalog,
            optimizer_rules,
        }
    }

    /// Creates a planner without any optimizer rules.
    #[must_use]
    pub fn without_optimization(catalog: &'a Catalog) -> Self {
        Planner {
            _catalog: catalog,
            optimizer_rules: Vec::new(),
        }
    }

    /// Adds an optimizer rule.
    pub fn add_rule(&mut self, rule: Box<dyn OptimizerRule>) {
        self.optimizer_rules.push(rule);
    }

    /// Generates a logical plan from a bound query.
    pub fn plan(&self, query: &BoundQuery) -> Result<LogicalPlan> {
        // Start with scan operators for each node in the query graph
        let mut plan = self.plan_query_graph(&query.query_graph)?;

        // Add WHERE clause filter
        if let Some(ref where_clause) = query.where_clause {
            plan = LogicalPlan::filter(plan, where_clause.clone());
        }

        // Add projections from RETURN clause
        if !query.return_clause.projections.is_empty() {
            plan = LogicalPlan::project(plan, query.return_clause.projections.clone());
        }

        // Add ORDER BY
        if let Some(ref order_by) = query.order_by {
            plan = LogicalPlan::Sort {
                input: Box::new(plan),
                order_by: order_by
                    .iter()
                    .map(|s| SortExpr {
                        expr: s.expr.clone(),
                        ascending: s.ascending,
                        nulls_first: s.nulls_first,
                    })
                    .collect(),
            };
        }

        // Add SKIP/LIMIT
        if query.skip.is_some() || query.limit.is_some() {
            plan = LogicalPlan::limit(plan, query.skip, query.limit);
        }

        Ok(plan)
    }

    /// Plans the query graph (MATCH pattern).
    fn plan_query_graph(&self, graph: &QueryGraph) -> Result<LogicalPlan> {
        if graph.nodes.is_empty() {
            return Err(RuzuError::PlanError("Empty query graph".into()));
        }

        // Start with the first node
        let first_node = &graph.nodes[0];
        let mut plan = LogicalPlan::node_scan(
            first_node.table_name().to_string(),
            first_node.variable.clone(),
            first_node.table_schema.clone(),
        );

        // Add relationships as Extend operations
        for rel in &graph.relationships {
            plan = LogicalPlan::Extend {
                input: Box::new(plan),
                rel_type: rel.rel_type().to_string(),
                rel_schema: rel.rel_schema.clone(),
                src_variable: rel.src_variable.clone(),
                dst_variable: rel.dst_variable.clone(),
                rel_variable: rel.variable.clone(),
                direction: rel.direction,
            };

            // If this is a variable-length path, convert to PathExpand
            if let Some((min, max)) = rel.path_bounds {
                plan = LogicalPlan::PathExpand {
                    input: Box::new(plan),
                    rel_type: rel.rel_type().to_string(),
                    rel_schema: rel.rel_schema.clone(),
                    src_variable: rel.src_variable.clone(),
                    dst_variable: rel.dst_variable.clone(),
                    path_variable: rel.variable.clone(),
                    min_hops: min,
                    max_hops: max,
                    direction: rel.direction,
                };
            }
        }

        Ok(plan)
    }

    /// Applies all optimizer rules to the logical plan.
    pub fn optimize(&self, plan: LogicalPlan) -> Result<LogicalPlan> {
        let mut current_plan = plan;

        for rule in &self.optimizer_rules {
            let transformed = rule.rewrite(current_plan)?;
            current_plan = transformed.into_inner();
        }

        Ok(current_plan)
    }

    /// Applies all optimizer rules and returns both the plan and applied rules.
    pub fn optimize_with_tracking(&self, plan: LogicalPlan) -> Result<(LogicalPlan, Vec<String>)> {
        let mut current_plan = plan;
        let mut applied_rules = Vec::new();

        for rule in &self.optimizer_rules {
            let transformed = rule.rewrite(current_plan)?;
            if transformed.was_transformed() {
                applied_rules.push(rule.name().to_string());
            }
            current_plan = transformed.into_inner();
        }

        Ok((current_plan, applied_rules))
    }

    /// Returns a textual description of the plan for EXPLAIN.
    #[must_use]
    pub fn explain(&self, plan: &LogicalPlan) -> String {
        // Use the Display trait for formatted output
        format!("{}", plan)
    }

    /// Returns a detailed EXPLAIN with optimization info.
    #[must_use]
    pub fn explain_verbose(&self, plan: &LogicalPlan) -> String {
        let mut output = String::new();

        output.push_str("=== Logical Plan ===\n");
        output.push_str(&format!("{}", plan));

        output.push_str("\n=== Output Schema ===\n");
        for (name, dtype) in plan.output_schema() {
            output.push_str(&format!("  {} : {:?}\n", name, dtype));
        }

        output
    }
}
