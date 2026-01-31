//! Executor module for query execution.
//!
//! This module provides physical operators for query execution.
//! It includes both traditional row-based operators and vectorized
//! batch operators using Apache Arrow.

mod extend;
mod filter;
mod project;
mod scan;
pub mod vectorized;

use std::cmp::Ordering;
use std::sync::Arc;

use arrow::record_batch::RecordBatch;
use datafusion::execution::context::SessionContext;
use datafusion::execution::runtime_env::RuntimeEnvBuilder;
use datafusion::physical_plan::ExecutionPlan;
use datafusion::prelude::SessionConfig;
use futures::StreamExt;

use crate::error::{Result, RuzuError};
use crate::parser::ast::{ComparisonOp, Expression, Literal};
use crate::planner::LogicalPlan;
use crate::types::{Row, Value};

use self::vectorized::DEFAULT_BATCH_SIZE;

pub use extend::ExtendOperator;
pub use filter::FilterOperator;
pub use project::ProjectOperator;
pub use scan::ScanOperator;

/// Configuration for the query executor.
#[derive(Debug, Clone)]
pub struct ExecutorConfig {
    /// Number of rows per batch for vectorized execution.
    pub batch_size: usize,
    /// Memory limit in bytes (0 = unlimited).
    pub memory_limit: usize,
    /// Number of partitions for parallel execution.
    pub partitions: usize,
}

impl Default for ExecutorConfig {
    fn default() -> Self {
        Self {
            batch_size: DEFAULT_BATCH_SIZE,
            memory_limit: 0,
            partitions: 1,
        }
    }
}

impl ExecutorConfig {
    /// Creates a new executor configuration.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the batch size.
    #[must_use]
    pub fn with_batch_size(mut self, batch_size: usize) -> Self {
        self.batch_size = batch_size;
        self
    }

    /// Sets the memory limit in bytes.
    #[must_use]
    pub fn with_memory_limit(mut self, memory_limit: usize) -> Self {
        self.memory_limit = memory_limit;
        self
    }

    /// Sets the number of partitions.
    #[must_use]
    pub fn with_partitions(mut self, partitions: usize) -> Self {
        self.partitions = partitions;
        self
    }
}

/// Query executor for executing logical plans.
pub struct QueryExecutor {
    /// Executor configuration.
    config: ExecutorConfig,
    /// `DataFusion` session context.
    session_ctx: SessionContext,
}

impl QueryExecutor {
    /// Creates a new query executor with the given configuration.
    ///
    /// Configures the `DataFusion` session context with the specified batch size
    /// and memory limits.
    #[must_use]
    pub fn new(config: ExecutorConfig) -> Self {
        // Configure DataFusion session with batch size
        let session_config = SessionConfig::new()
            .with_batch_size(config.batch_size);

        // Configure runtime environment
        let runtime_env = RuntimeEnvBuilder::new()
            .build_arc()
            .expect("Failed to create runtime environment");

        let session_ctx = SessionContext::new_with_config_rt(session_config, runtime_env);

        Self { config, session_ctx }
    }

    /// Creates a query executor with default batch size.
    #[must_use]
    pub fn with_default_batch_size() -> Self {
        let config = ExecutorConfig::default();
        Self::new(config)
    }

    /// Returns the configured batch size.
    #[must_use]
    pub fn batch_size(&self) -> usize {
        self.config.batch_size
    }

    /// Returns the configured memory limit.
    #[must_use]
    pub fn memory_limit(&self) -> usize {
        self.config.memory_limit
    }

    /// Returns the executor configuration.
    #[must_use]
    pub fn config(&self) -> &ExecutorConfig {
        &self.config
    }

    /// Checks if the current memory usage exceeds the limit.
    ///
    /// # Errors
    ///
    /// Returns `MemoryLimitExceeded` if usage exceeds the configured limit.
    pub fn check_memory_limit(&self, current_usage: usize) -> Result<()> {
        if self.config.memory_limit > 0 && current_usage > self.config.memory_limit {
            return Err(RuzuError::MemoryLimitExceeded {
                used: current_usage,
                limit: self.config.memory_limit,
            });
        }
        Ok(())
    }

    /// Executes a logical plan and returns all record batches.
    ///
    /// # Errors
    ///
    /// Returns an error if execution fails.
    pub async fn execute(&self, plan: &LogicalPlan) -> Result<Vec<RecordBatch>> {
        // For now, return empty result for Empty plans
        if matches!(plan, LogicalPlan::Empty { .. }) {
            return Ok(vec![]);
        }

        // Convert logical plan to DataFusion physical plan
        let physical_plan = self.to_physical_plan(plan)?;

        // Execute and collect all batches
        let mut batches = Vec::new();
        let stream = physical_plan
            .execute(0, self.session_ctx.task_ctx())
            .map_err(|e| RuzuError::ExecutionError(e.to_string()))?;

        let mut stream = stream;
        while let Some(batch_result) = stream.next().await {
            let batch = batch_result.map_err(|e| RuzuError::ExecutionError(e.to_string()))?;
            batches.push(batch);
        }

        Ok(batches)
    }

    /// Executes a logical plan and returns a streaming result.
    ///
    /// # Errors
    ///
    /// Returns an error if execution setup fails.
    pub fn execute_stream(
        &self,
        plan: &LogicalPlan,
    ) -> Result<datafusion::physical_plan::SendableRecordBatchStream> {
        // For Empty plans, return an empty stream
        if matches!(plan, LogicalPlan::Empty { .. }) {
            // Create an empty RecordBatch stream
            let schema = Arc::new(arrow::datatypes::Schema::empty());
            let empty_batch = RecordBatch::new_empty(schema);
            let stream = futures::stream::once(async move { Ok(empty_batch) });
            return Ok(Box::pin(datafusion::physical_plan::stream::RecordBatchStreamAdapter::new(
                Arc::new(arrow::datatypes::Schema::empty()),
                stream,
            )));
        }

        let physical_plan = self.to_physical_plan(plan)?;
        physical_plan
            .execute(0, self.session_ctx.task_ctx())
            .map_err(|e| RuzuError::ExecutionError(e.to_string()))
    }

    /// Converts a logical plan to a `DataFusion` physical plan.
    fn to_physical_plan(&self, plan: &LogicalPlan) -> Result<Arc<dyn ExecutionPlan>> {
        // For now, create a simple placeholder physical plan
        // This will be expanded in the User Story phases
        match plan {
            LogicalPlan::Empty { schema } => {
                // Create an empty memory exec
                let arrow_schema = Arc::new(arrow::datatypes::Schema::new(
                    schema
                        .iter()
                        .map(|(name, dtype)| {
                            arrow::datatypes::Field::new(name, dtype.to_arrow(), true)
                        })
                        .collect::<Vec<_>>(),
                ));
                let batches: Vec<RecordBatch> = vec![];
                let exec = datafusion::physical_plan::memory::MemoryExec::try_new(
                    &[batches],
                    arrow_schema,
                    None,
                )
                .map_err(|e| RuzuError::ExecutionError(e.to_string()))?;
                Ok(Arc::new(exec))
            }
            _ => {
                // For other plan types, return an error for now
                // These will be implemented in subsequent user story phases
                Err(RuzuError::ExecutionError(format!(
                    "Physical plan conversion not yet implemented for {:?}",
                    std::mem::discriminant(plan)
                )))
            }
        }
    }
}

/// Trait for physical operators in the execution pipeline.
pub trait PhysicalOperator {
    /// Returns the next row, or None if exhausted.
    ///
    /// # Errors
    ///
    /// Returns an error if evaluation of the expression fails.
    fn next(&mut self) -> Result<Option<Row>>;
}

/// Promotes values for cross-type comparison (Int64 vs Float64).
#[allow(clippy::cast_precision_loss)]
fn promote_for_comparison(a: Value, b: Value) -> (Value, Value) {
    match (&a, &b) {
        (Value::Int64(n), Value::Float64(_)) => (Value::Float64(*n as f64), b),
        (Value::Float64(_), Value::Int64(n)) => (a, Value::Float64(*n as f64)),
        _ => (a, b),
    }
}

/// Evaluates an expression against a row.
///
/// # Errors
///
/// This function currently does not return errors, but the signature is
/// designed for future extension with type checking.
pub fn evaluate_expression(expr: &Expression, row: &Row) -> Result<bool> {
    // Get the value from the row using the full qualified name (var.property)
    let column_name = format!("{}.{}", expr.var, expr.property);
    let value = row.get(&column_name);

    let Some(value) = value else {
        // Column not found, return false
        return Ok(false);
    };

    // Convert the literal to a Value for comparison
    let literal_value = match &expr.value {
        Literal::Int64(n) => Value::Int64(*n),
        Literal::String(s) => Value::String(s.clone()),
        Literal::Float64(f) => Value::Float64(*f),
        Literal::Bool(b) => Value::Bool(*b),
    };

    // Promote for cross-type comparison (Int64 vs Float64)
    let (value, literal_value) = promote_for_comparison(value.clone(), literal_value);

    // Compare based on the operator
    let cmp = value.compare(&literal_value);

    match cmp {
        None => Ok(false), // Null or type mismatch
        Some(ordering) => {
            let result = match expr.op {
                ComparisonOp::Gt => ordering == Ordering::Greater,
                ComparisonOp::Lt => ordering == Ordering::Less,
                ComparisonOp::Eq => ordering == Ordering::Equal,
                ComparisonOp::Gte => ordering != Ordering::Less,
                ComparisonOp::Lte => ordering != Ordering::Greater,
                ComparisonOp::Neq => ordering != Ordering::Equal,
            };
            Ok(result)
        }
    }
}
