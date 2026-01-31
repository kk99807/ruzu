//! `TableProvider` implementations for ruzu storage.

use std::any::Any;
use std::sync::Arc;

use arrow::datatypes::{DataType as ArrowDataType, Field, Schema, SchemaRef, TimeUnit};
use async_trait::async_trait;
use datafusion::catalog::Session;
use datafusion::datasource::{TableProvider, TableType};
use datafusion::error::Result as DfResult;
use datafusion::logical_expr::{Expr, TableProviderFilterPushDown};
use datafusion::physical_plan::ExecutionPlan;

use crate::catalog::NodeTableSchema;
use crate::storage::NodeTable;
use crate::types::DataType;

/// `TableProvider` implementation for `NodeTable`.
#[derive(Debug)]
pub struct NodeTableProvider {
    /// Reference to the underlying node table.
    table: Arc<NodeTable>,
    /// Table schema.
    table_schema: Arc<NodeTableSchema>,
    /// Arrow schema for this table.
    arrow_schema: SchemaRef,
}

impl NodeTableProvider {
    /// Creates a new node table provider.
    #[must_use]
    pub fn new(table: Arc<NodeTable>, table_schema: Arc<NodeTableSchema>) -> Self {
        let arrow_schema = Arc::new(node_schema_to_arrow(&table_schema));
        NodeTableProvider {
            table,
            table_schema,
            arrow_schema,
        }
    }

    /// Returns the underlying node table.
    #[must_use]
    pub fn table(&self) -> &Arc<NodeTable> {
        &self.table
    }

    /// Returns the table schema.
    #[must_use]
    pub fn table_schema(&self) -> &Arc<NodeTableSchema> {
        &self.table_schema
    }
}

#[async_trait]
impl TableProvider for NodeTableProvider {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn schema(&self) -> SchemaRef {
        self.arrow_schema.clone()
    }

    fn table_type(&self) -> TableType {
        TableType::Base
    }

    async fn scan(
        &self,
        _state: &dyn Session,
        _projection: Option<&Vec<usize>>,
        _filters: &[Expr],
        _limit: Option<usize>,
    ) -> DfResult<Arc<dyn ExecutionPlan>> {
        // TODO: Implement NodeScanExec
        // For now, return an unimplemented error
        Err(datafusion::error::DataFusionError::NotImplemented(
            "NodeTableProvider::scan not yet implemented".to_string(),
        ))
    }

    fn supports_filters_pushdown(
        &self,
        filters: &[&Expr],
    ) -> DfResult<Vec<TableProviderFilterPushDown>> {
        // Support simple comparison filters
        filters
            .iter()
            .map(|f| {
                match f {
                    Expr::BinaryExpr { .. } => Ok(TableProviderFilterPushDown::Inexact),
                    _ => Ok(TableProviderFilterPushDown::Unsupported),
                }
            })
            .collect()
    }
}

/// `TableProvider` implementation for relationship tables.
#[derive(Debug)]
pub struct RelTableProvider {
    /// Arrow schema for this table.
    arrow_schema: SchemaRef,
}

impl RelTableProvider {
    /// Creates a new relationship table provider.
    #[must_use]
    pub fn new(arrow_schema: SchemaRef) -> Self {
        RelTableProvider { arrow_schema }
    }
}

#[async_trait]
impl TableProvider for RelTableProvider {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn schema(&self) -> SchemaRef {
        self.arrow_schema.clone()
    }

    fn table_type(&self) -> TableType {
        TableType::Base
    }

    async fn scan(
        &self,
        _state: &dyn Session,
        _projection: Option<&Vec<usize>>,
        _filters: &[Expr],
        _limit: Option<usize>,
    ) -> DfResult<Arc<dyn ExecutionPlan>> {
        Err(datafusion::error::DataFusionError::NotImplemented(
            "RelTableProvider::scan not yet implemented".to_string(),
        ))
    }
}

/// Converts a ruzu `NodeTableSchema` to an Arrow Schema.
fn node_schema_to_arrow(schema: &NodeTableSchema) -> Schema {
    let fields: Vec<Field> = schema
        .columns
        .iter()
        .map(|col| {
            let arrow_type = datatype_to_arrow(col.data_type);
            Field::new(&col.name, arrow_type, true) // All columns nullable for now
        })
        .collect();

    Schema::new(fields)
}

/// Converts a ruzu `DataType` to an Arrow `DataType`.
#[must_use]
pub fn datatype_to_arrow(dt: DataType) -> ArrowDataType {
    match dt {
        DataType::Int64 => ArrowDataType::Int64,
        DataType::Float32 => ArrowDataType::Float32,
        DataType::Float64 => ArrowDataType::Float64,
        DataType::Bool => ArrowDataType::Boolean,
        DataType::String => ArrowDataType::Utf8,
        DataType::Date => ArrowDataType::Date32,
        DataType::Timestamp => ArrowDataType::Timestamp(TimeUnit::Microsecond, None),
    }
}

/// Converts an Arrow `DataType` to a ruzu `DataType`.
#[must_use]
pub fn arrow_to_datatype(dt: &ArrowDataType) -> Option<DataType> {
    match dt {
        ArrowDataType::Int64 => Some(DataType::Int64),
        ArrowDataType::Float64 => Some(DataType::Float64),
        ArrowDataType::Float32 => Some(DataType::Float32),
        ArrowDataType::Boolean => Some(DataType::Bool),
        ArrowDataType::Utf8 | ArrowDataType::LargeUtf8 => Some(DataType::String),
        ArrowDataType::Date32 => Some(DataType::Date),
        ArrowDataType::Timestamp(_, _) => Some(DataType::Timestamp),
        _ => None,
    }
}
