//! Bound expression definitions.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::types::{DataType, Value};

/// Bound expression after semantic analysis.
#[derive(Debug, Clone)]
pub enum BoundExpression {
    /// Literal value (constant).
    Literal {
        value: Value,
        data_type: DataType,
    },

    /// Reference to a variable's property.
    PropertyAccess {
        variable: String,
        property: String,
        data_type: DataType,
    },

    /// Reference to entire node/relationship variable.
    VariableRef {
        variable: String,
        data_type: DataType,
    },

    /// Binary comparison.
    Comparison {
        left: Box<BoundExpression>,
        op: ComparisonOp,
        right: Box<BoundExpression>,
        data_type: DataType, // Always Bool
    },

    /// Logical AND/OR/NOT.
    Logical {
        op: LogicalOp,
        operands: Vec<BoundExpression>,
        data_type: DataType, // Always Bool
    },

    /// Arithmetic operations.
    Arithmetic {
        left: Box<BoundExpression>,
        op: ArithmeticOp,
        right: Box<BoundExpression>,
        data_type: DataType,
    },

    /// Aggregation function call.
    Aggregate {
        function: AggregateFunction,
        input: Option<Box<BoundExpression>>, // None for COUNT(*)
        distinct: bool,
        data_type: DataType,
    },

    /// IS NULL / IS NOT NULL.
    IsNull {
        operand: Box<BoundExpression>,
        negated: bool,
        data_type: DataType, // Always Bool
    },
}

impl BoundExpression {
    /// Returns the data type of this expression.
    #[must_use]
    pub fn data_type(&self) -> DataType {
        match self {
            BoundExpression::Literal { data_type, .. }
            | BoundExpression::PropertyAccess { data_type, .. }
            | BoundExpression::VariableRef { data_type, .. }
            | BoundExpression::Comparison { data_type, .. }
            | BoundExpression::Logical { data_type, .. }
            | BoundExpression::Arithmetic { data_type, .. }
            | BoundExpression::Aggregate { data_type, .. }
            | BoundExpression::IsNull { data_type, .. } => *data_type,
        }
    }

    /// Creates a literal expression.
    #[must_use]
    pub fn literal(value: Value) -> Self {
        let data_type = value.data_type().unwrap_or(DataType::String);
        BoundExpression::Literal { value, data_type }
    }

    /// Creates a property access expression.
    #[must_use]
    pub fn property_access(variable: String, property: String, data_type: DataType) -> Self {
        BoundExpression::PropertyAccess {
            variable,
            property,
            data_type,
        }
    }

    /// Creates a comparison expression.
    #[must_use]
    pub fn comparison(left: BoundExpression, op: ComparisonOp, right: BoundExpression) -> Self {
        BoundExpression::Comparison {
            left: Box::new(left),
            op,
            right: Box::new(right),
            data_type: DataType::Bool,
        }
    }

    /// Creates a logical AND expression.
    #[must_use]
    pub fn and(operands: Vec<BoundExpression>) -> Self {
        BoundExpression::Logical {
            op: LogicalOp::And,
            operands,
            data_type: DataType::Bool,
        }
    }

    /// Creates a logical OR expression.
    #[must_use]
    pub fn or(operands: Vec<BoundExpression>) -> Self {
        BoundExpression::Logical {
            op: LogicalOp::Or,
            operands,
            data_type: DataType::Bool,
        }
    }

    /// Creates a logical NOT expression.
    #[must_use]
    pub fn not(operand: BoundExpression) -> Self {
        BoundExpression::Logical {
            op: LogicalOp::Not,
            operands: vec![operand],
            data_type: DataType::Bool,
        }
    }

    /// Creates an aggregate expression.
    #[must_use]
    pub fn aggregate(
        function: AggregateFunction,
        input: Option<Box<BoundExpression>>,
        data_type: DataType,
    ) -> Self {
        BoundExpression::Aggregate {
            function,
            input,
            distinct: false,
            data_type,
        }
    }

    /// Creates a COUNT(*) expression.
    #[must_use]
    pub fn count_star() -> Self {
        BoundExpression::Aggregate {
            function: AggregateFunction::Count,
            input: None,
            distinct: false,
            data_type: DataType::Int64,
        }
    }
}

/// Comparison operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComparisonOp {
    /// Equal (=).
    Eq,
    /// Not equal (<>).
    Neq,
    /// Less than (<).
    Lt,
    /// Less than or equal (<=).
    Lte,
    /// Greater than (>).
    Gt,
    /// Greater than or equal (>=).
    Gte,
}

impl ComparisonOp {
    /// Returns the string representation of this operator.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            ComparisonOp::Eq => "=",
            ComparisonOp::Neq => "<>",
            ComparisonOp::Lt => "<",
            ComparisonOp::Lte => "<=",
            ComparisonOp::Gt => ">",
            ComparisonOp::Gte => ">=",
        }
    }
}

/// Logical operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogicalOp {
    And,
    Or,
    Not,
}

/// Arithmetic operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArithmeticOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
}

impl ArithmeticOp {
    /// Returns the string representation of this operator.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            ArithmeticOp::Add => "+",
            ArithmeticOp::Sub => "-",
            ArithmeticOp::Mul => "*",
            ArithmeticOp::Div => "/",
            ArithmeticOp::Mod => "%",
        }
    }
}

/// Aggregate functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AggregateFunction {
    Count,
    Sum,
    Avg,
    Min,
    Max,
}

impl AggregateFunction {
    /// Returns the name of this aggregate function.
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            AggregateFunction::Count => "COUNT",
            AggregateFunction::Sum => "SUM",
            AggregateFunction::Avg => "AVG",
            AggregateFunction::Min => "MIN",
            AggregateFunction::Max => "MAX",
        }
    }

    /// Returns the output data type for this aggregate function given an input type.
    #[must_use]
    pub fn output_type(&self, input_type: Option<DataType>) -> DataType {
        match self {
            AggregateFunction::Count => DataType::Int64,
            AggregateFunction::Avg => DataType::Float64,
            AggregateFunction::Sum | AggregateFunction::Min | AggregateFunction::Max => {
                input_type.unwrap_or(DataType::Int64)
            }
        }
    }
}
