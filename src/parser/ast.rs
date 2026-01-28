//! Abstract Syntax Tree definitions for Cypher queries.

/// A parsed Cypher statement.
#[derive(Debug, Clone)]
pub enum Statement {
    /// CREATE NODE TABLE statement.
    CreateNodeTable {
        table_name: String,
        columns: Vec<(String, String)>,
        primary_key: Vec<String>,
    },
    /// CREATE REL TABLE statement.
    CreateRelTable {
        table_name: String,
        src_table: String,
        dst_table: String,
        columns: Vec<(String, String)>,
    },
    /// CREATE node statement.
    CreateNode {
        label: String,
        properties: Vec<(String, Literal)>,
    },
    /// MATCH ... CREATE relationship statement.
    MatchCreate {
        /// Source node variable, label, and optional property filter (key, value)
        src_node: NodeFilter,
        /// Destination node variable, label, and optional property filter (key, value)
        dst_node: NodeFilter,
        /// Relationship type
        rel_type: String,
        /// Relationship properties
        rel_props: Vec<(String, Literal)>,
        /// Source variable name in relationship pattern
        src_var: String,
        /// Destination variable name in relationship pattern
        dst_var: String,
    },
    /// MATCH query statement (node-only).
    Match {
        var: String,
        label: String,
        filter: Option<Expression>,
        projections: Vec<ReturnItem>,
        order_by: Option<Vec<OrderByItem>>,
        skip: Option<i64>,
        limit: Option<i64>,
    },
    /// MATCH query statement with relationship pattern.
    MatchRel {
        /// Source node variable, label, and optional filter
        src_node: NodeFilter,
        /// Relationship variable (optional), type
        rel_var: Option<String>,
        rel_type: String,
        /// Destination node variable, label, and optional filter
        dst_node: NodeFilter,
        /// WHERE clause filter
        filter: Option<Expression>,
        /// Return items (projections or aggregates)
        projections: Vec<ReturnItem>,
        /// ORDER BY clause
        order_by: Option<Vec<OrderByItem>>,
        /// SKIP amount
        skip: Option<i64>,
        /// LIMIT amount
        limit: Option<i64>,
        /// Variable-length path bounds (min, max) for multi-hop traversal
        path_bounds: Option<(u32, u32)>,
    },
    /// COPY command for bulk CSV import.
    Copy {
        /// Table name to import into
        table_name: String,
        /// Path to the CSV file
        file_path: String,
        /// Import options
        options: CopyOptions,
    },
    /// EXPLAIN query statement - shows plan without executing.
    Explain {
        /// The inner query to explain
        inner: Box<Statement>,
    },
}

/// Options for the COPY command.
#[derive(Debug, Clone, Default)]
pub struct CopyOptions {
    /// Whether the CSV file has a header row (default: true).
    pub has_header: Option<bool>,
    /// Field delimiter (default: ',').
    pub delimiter: Option<char>,
    /// Number of rows to skip at the beginning.
    pub skip_rows: Option<u32>,
    /// Whether to ignore errors and continue importing (default: false).
    pub ignore_errors: Option<bool>,
}

/// Node filter for MATCH patterns.
#[derive(Debug, Clone)]
pub struct NodeFilter {
    /// Variable binding for this node
    pub var: String,
    /// Node label (table name)
    pub label: String,
    /// Optional property filter (key, value)
    pub property_filter: Option<(String, Literal)>,
}

/// Literal values in Cypher queries.
#[derive(Debug, Clone)]
pub enum Literal {
    /// String literal.
    String(String),
    /// 64-bit integer literal.
    Int64(i64),
}

/// Return item in RETURN clause.
#[derive(Debug, Clone)]
pub enum ReturnItem {
    /// Property projection (var.property).
    Projection {
        var: String,
        property: String,
    },
    /// Aggregate expression.
    Aggregate(AggregateExpr),
}

impl ReturnItem {
    /// Creates a projection return item.
    #[must_use]
    pub fn projection(var: String, property: String) -> Self {
        ReturnItem::Projection { var, property }
    }

    /// Creates an aggregate return item.
    #[must_use]
    pub fn aggregate(func: AstAggregateFunction, input: Option<(String, String)>) -> Self {
        ReturnItem::Aggregate(AggregateExpr { function: func, input })
    }
}

/// Aggregate expression in the AST.
#[derive(Debug, Clone)]
pub struct AggregateExpr {
    /// Aggregate function (COUNT, SUM, AVG, MIN, MAX).
    pub function: AstAggregateFunction,
    /// Input expression (None for COUNT(*)).
    pub input: Option<(String, String)>, // (var, property)
}

/// Aggregate functions in AST.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AstAggregateFunction {
    Count,
    Sum,
    Avg,
    Min,
    Max,
}

impl AstAggregateFunction {
    /// Parses an aggregate function from a string.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            "COUNT" => Some(AstAggregateFunction::Count),
            "SUM" => Some(AstAggregateFunction::Sum),
            "AVG" => Some(AstAggregateFunction::Avg),
            "MIN" => Some(AstAggregateFunction::Min),
            "MAX" => Some(AstAggregateFunction::Max),
            _ => None,
        }
    }
}

/// ORDER BY item.
#[derive(Debug, Clone)]
pub struct OrderByItem {
    /// Variable name.
    pub var: String,
    /// Property name.
    pub property: String,
    /// Sort direction (true = ASC, false = DESC).
    pub ascending: bool,
}

/// Expression in WHERE clause.
#[derive(Debug, Clone)]
pub struct Expression {
    pub var: String,
    pub property: String,
    pub op: ComparisonOp,
    pub value: Literal,
}

/// Comparison operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComparisonOp {
    Gt,
    Lt,
    Eq,
    Gte,
    Lte,
    Neq,
}

impl ComparisonOp {
    /// Parses a comparison operator from a string.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            ">" => Some(ComparisonOp::Gt),
            "<" => Some(ComparisonOp::Lt),
            "=" => Some(ComparisonOp::Eq),
            ">=" => Some(ComparisonOp::Gte),
            "<=" => Some(ComparisonOp::Lte),
            "<>" => Some(ComparisonOp::Neq),
            _ => None,
        }
    }
}
