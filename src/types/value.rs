//! Value and `DataType` definitions for ruzu.

use std::cmp::Ordering;
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Supported data types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DataType {
    /// 64-bit signed integer.
    Int64,
    /// 32-bit floating point.
    Float32,
    /// 64-bit floating point.
    Float64,
    /// Boolean.
    Bool,
    /// UTF-8 string.
    String,
    /// Date (stored as days since epoch).
    Date,
    /// Timestamp (stored as microseconds since epoch).
    Timestamp,
}

impl DataType {
    /// Returns the name of the data type as used in Cypher syntax.
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            DataType::Int64 => "INT64",
            DataType::Float32 => "FLOAT32",
            DataType::Float64 => "FLOAT64",
            DataType::Bool => "BOOL",
            DataType::String => "STRING",
            DataType::Date => "DATE",
            DataType::Timestamp => "TIMESTAMP",
        }
    }

    /// Returns whether this type is a fixed-width type.
    #[must_use]
    pub fn is_fixed_width(&self) -> bool {
        matches!(
            self,
            DataType::Int64
                | DataType::Float32
                | DataType::Float64
                | DataType::Bool
                | DataType::Date
                | DataType::Timestamp
        )
    }

    /// Returns the byte size for fixed-width types.
    #[must_use]
    pub fn byte_size(&self) -> Option<usize> {
        match self {
            DataType::Int64 | DataType::Float64 | DataType::Timestamp => Some(8),
            DataType::Float32 | DataType::Date => Some(4),
            DataType::Bool => Some(1),
            DataType::String => None, // variable width
        }
    }

    /// Returns whether this type is numeric.
    #[must_use]
    pub fn is_numeric(&self) -> bool {
        matches!(
            self,
            DataType::Int64 | DataType::Float32 | DataType::Float64
        )
    }

    /// Returns whether this type is orderable.
    #[must_use]
    pub fn is_orderable(&self) -> bool {
        matches!(
            self,
            DataType::Int64
                | DataType::Float32
                | DataType::Float64
                | DataType::String
                | DataType::Date
                | DataType::Timestamp
        )
    }

    /// Converts to an Arrow data type.
    #[must_use]
    pub fn to_arrow(&self) -> arrow::datatypes::DataType {
        match self {
            DataType::Int64 => arrow::datatypes::DataType::Int64,
            DataType::Float32 => arrow::datatypes::DataType::Float32,
            DataType::Float64 => arrow::datatypes::DataType::Float64,
            DataType::Bool => arrow::datatypes::DataType::Boolean,
            DataType::String => arrow::datatypes::DataType::Utf8,
            DataType::Date => arrow::datatypes::DataType::Date32,
            DataType::Timestamp => {
                arrow::datatypes::DataType::Timestamp(arrow::datatypes::TimeUnit::Microsecond, None)
            }
        }
    }

    /// Converts from an Arrow data type.
    ///
    /// Returns None for unsupported Arrow types.
    #[must_use]
    pub fn from_arrow(arrow_type: &arrow::datatypes::DataType) -> Option<Self> {
        match arrow_type {
            arrow::datatypes::DataType::Int64 => Some(DataType::Int64),
            arrow::datatypes::DataType::Float32 => Some(DataType::Float32),
            arrow::datatypes::DataType::Float64 => Some(DataType::Float64),
            arrow::datatypes::DataType::Boolean => Some(DataType::Bool),
            arrow::datatypes::DataType::Utf8 | arrow::datatypes::DataType::LargeUtf8 => {
                Some(DataType::String)
            }
            arrow::datatypes::DataType::Date32 | arrow::datatypes::DataType::Date64 => {
                Some(DataType::Date)
            }
            arrow::datatypes::DataType::Timestamp(_, _) => Some(DataType::Timestamp),
            _ => None,
        }
    }
}

/// Runtime value container for data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Value {
    /// 64-bit signed integer value.
    Int64(i64),
    /// 32-bit floating point value.
    Float32(f32),
    /// 64-bit floating point value.
    Float64(f64),
    /// Boolean value.
    Bool(bool),
    /// String value.
    String(String),
    /// Date value (days since Unix epoch).
    Date(i32),
    /// Timestamp value (microseconds since Unix epoch).
    Timestamp(i64),
    /// Null value.
    Null,
}

// Manual Hash implementation because f32/f64 doesn't implement Hash
impl std::hash::Hash for Value {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            Value::Int64(v) | Value::Timestamp(v) => v.hash(state),
            Value::Float32(v) => v.to_bits().hash(state),
            Value::Float64(v) => v.to_bits().hash(state),
            Value::Bool(v) => v.hash(state),
            Value::String(v) => v.hash(state),
            Value::Date(v) => v.hash(state),
            Value::Null => {}
        }
    }
}

// Manual Eq implementation because f64 doesn't implement Eq
impl Eq for Value {}

impl Value {
    /// Returns true if this value is null.
    #[must_use]
    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    /// Attempts to extract an i64 value.
    #[must_use]
    pub fn as_int64(&self) -> Option<i64> {
        match self {
            Value::Int64(i) => Some(*i),
            _ => None,
        }
    }

    /// Attempts to extract a string reference.
    #[must_use]
    pub fn as_string(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s.as_str()),
            _ => None,
        }
    }

    /// Returns the data type of this value, or None for Null.
    #[must_use]
    pub fn data_type(&self) -> Option<DataType> {
        match self {
            Value::Int64(_) => Some(DataType::Int64),
            Value::Float32(_) => Some(DataType::Float32),
            Value::Float64(_) => Some(DataType::Float64),
            Value::Bool(_) => Some(DataType::Bool),
            Value::String(_) => Some(DataType::String),
            Value::Date(_) => Some(DataType::Date),
            Value::Timestamp(_) => Some(DataType::Timestamp),
            Value::Null => None,
        }
    }

    /// Attempts to extract an f32 value.
    #[must_use]
    pub fn as_float32(&self) -> Option<f32> {
        match self {
            Value::Float32(f) => Some(*f),
            _ => None,
        }
    }

    /// Attempts to extract a timestamp value.
    #[must_use]
    pub fn as_timestamp(&self) -> Option<i64> {
        match self {
            Value::Timestamp(t) => Some(*t),
            _ => None,
        }
    }

    /// Attempts to extract an f64 value.
    #[must_use]
    pub fn as_float64(&self) -> Option<f64> {
        match self {
            Value::Float64(f) => Some(*f),
            _ => None,
        }
    }

    /// Attempts to extract a bool value.
    #[must_use]
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Compares two values using SQL null semantics.
    ///
    /// Returns None if either value is null or types don't match.
    #[must_use]
    pub fn compare(&self, other: &Value) -> Option<Ordering> {
        match (self, other) {
            (Value::Int64(a), Value::Int64(b))
            | (Value::Timestamp(a), Value::Timestamp(b)) => Some(a.cmp(b)),
            (Value::Float32(a), Value::Float32(b)) => a.partial_cmp(b),
            (Value::Float64(a), Value::Float64(b)) => a.partial_cmp(b),
            (Value::Bool(a), Value::Bool(b)) => Some(a.cmp(b)),
            (Value::String(a), Value::String(b)) => Some(a.cmp(b)),
            (Value::Date(a), Value::Date(b)) => Some(a.cmp(b)),
            // Null or type mismatch
            _ => None,
        }
    }
}

/// Represents a single row result from query execution.
#[derive(Debug, Clone, Default)]
pub struct Row {
    values: HashMap<String, Value>,
}

impl Row {
    /// Creates a new empty row.
    #[must_use]
    pub fn new() -> Self {
        Row {
            values: HashMap::new(),
        }
    }

    /// Sets a column value in the row.
    pub fn set(&mut self, column: String, value: Value) {
        self.values.insert(column, value);
    }

    /// Gets a value by column name.
    #[must_use]
    pub fn get(&self, column: &str) -> Option<&Value> {
        self.values.get(column)
    }

    /// Returns the number of columns in the row.
    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Returns true if the row has no columns.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Returns an iterator over the columns and values.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &Value)> {
        self.values.iter()
    }

    /// Returns true if the row contains the given column.
    #[must_use]
    pub fn contains_key(&self, column: &str) -> bool {
        self.values.contains_key(column)
    }

    /// Inserts a column value into the row (alias for set).
    pub fn insert(&mut self, column: String, value: Value) {
        self.values.insert(column, value);
    }
}

/// Result of query execution containing rows and metadata.
#[derive(Debug, Default)]
pub struct QueryResult {
    /// Ordered list of column names.
    pub columns: Vec<String>,
    /// Result rows.
    pub rows: Vec<Row>,
}

impl QueryResult {
    /// Creates a new empty result with the given column names.
    #[must_use]
    pub fn new(columns: Vec<String>) -> Self {
        QueryResult {
            columns,
            rows: Vec::new(),
        }
    }

    /// Creates an empty result (for DDL/DML statements).
    #[must_use]
    pub fn empty() -> Self {
        QueryResult {
            columns: Vec::new(),
            rows: Vec::new(),
        }
    }

    /// Appends a row to the result.
    pub fn add_row(&mut self, row: Row) {
        self.rows.push(row);
    }

    /// Returns the number of rows in the result.
    #[must_use]
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// Gets a row by index.
    #[must_use]
    pub fn get_row(&self, index: usize) -> Option<&Row> {
        self.rows.get(index)
    }

    /// Creates a result for import operations showing rows imported/failed.
    #[must_use]
    pub fn import_result(rows_imported: u64, rows_failed: u64) -> Self {
        let mut result =
            QueryResult::new(vec!["rows_imported".to_string(), "rows_failed".to_string()]);
        let mut row = Row::new();
        row.set(
            "rows_imported".to_string(),
            Value::Int64(rows_imported as i64),
        );
        row.set("rows_failed".to_string(), Value::Int64(rows_failed as i64));
        result.add_row(row);
        result
    }

    /// Creates a result for EXPLAIN output showing the query plan.
    #[must_use]
    #[allow(non_snake_case)]
    pub fn Explain(plan_text: String) -> Self {
        let mut result = QueryResult::new(vec!["plan".to_string()]);
        let mut row = Row::new();
        row.set("plan".to_string(), Value::String(plan_text));
        result.add_row(row);
        result
    }
}
