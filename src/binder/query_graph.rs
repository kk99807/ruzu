//! Query graph representation for bound patterns.

use std::sync::Arc;

use crate::catalog::{NodeTableSchema, RelTableSchema};

use super::expression::BoundExpression;

/// Bound query graph representing MATCH patterns.
#[derive(Debug, Clone)]
pub struct QueryGraph {
    /// Node variables in the pattern.
    pub nodes: Vec<BoundNode>,
    /// Relationship patterns.
    pub relationships: Vec<BoundRelationship>,
    /// WHERE predicates.
    pub predicates: Vec<BoundExpression>,
}

impl Default for QueryGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl QueryGraph {
    /// Creates a new empty query graph.
    #[must_use]
    pub fn new() -> Self {
        QueryGraph {
            nodes: Vec::new(),
            relationships: Vec::new(),
            predicates: Vec::new(),
        }
    }

    /// Adds a node to the query graph.
    pub fn add_node(&mut self, node: BoundNode) {
        self.nodes.push(node);
    }

    /// Adds a relationship to the query graph.
    pub fn add_relationship(&mut self, rel: BoundRelationship) {
        self.relationships.push(rel);
    }

    /// Adds a predicate to the query graph.
    pub fn add_predicate(&mut self, pred: BoundExpression) {
        self.predicates.push(pred);
    }

    /// Returns the node with the given variable name.
    #[must_use]
    pub fn get_node(&self, variable: &str) -> Option<&BoundNode> {
        self.nodes.iter().find(|n| n.variable == variable)
    }

    /// Returns true if the query graph has any relationships.
    #[must_use]
    pub fn has_relationships(&self) -> bool {
        !self.relationships.is_empty()
    }
}

/// Bound node pattern.
#[derive(Debug, Clone)]
pub struct BoundNode {
    /// Variable name (e.g., "p" in (p:Person)).
    pub variable: String,
    /// Node table schema.
    pub table_schema: Arc<NodeTableSchema>,
    /// Property filters from inline patterns (e.g., {age: 25}).
    pub property_filters: Vec<(String, BoundExpression)>,
}

impl BoundNode {
    /// Creates a new bound node.
    #[must_use]
    pub fn new(variable: String, table_schema: Arc<NodeTableSchema>) -> Self {
        BoundNode {
            variable,
            table_schema,
            property_filters: Vec::new(),
        }
    }

    /// Adds a property filter to this node.
    pub fn add_property_filter(&mut self, property: String, value: BoundExpression) {
        self.property_filters.push((property, value));
    }

    /// Returns the table name for this node.
    #[must_use]
    pub fn table_name(&self) -> &str {
        &self.table_schema.name
    }
}

/// Bound relationship pattern.
#[derive(Debug, Clone)]
pub struct BoundRelationship {
    /// Optional variable name (e.g., "r" in -[r:KNOWS]->).
    pub variable: Option<String>,
    /// Relationship table schema.
    pub rel_schema: Arc<RelTableSchema>,
    /// Source node variable.
    pub src_variable: String,
    /// Destination node variable.
    pub dst_variable: String,
    /// Traversal direction.
    pub direction: Direction,
    /// For variable-length paths: (min, max) hops.
    pub path_bounds: Option<(usize, usize)>,
}

impl BoundRelationship {
    /// Creates a new bound relationship.
    #[must_use]
    pub fn new(
        variable: Option<String>,
        rel_schema: Arc<RelTableSchema>,
        src_variable: String,
        dst_variable: String,
        direction: Direction,
    ) -> Self {
        BoundRelationship {
            variable,
            rel_schema,
            src_variable,
            dst_variable,
            direction,
            path_bounds: None,
        }
    }

    /// Sets path bounds for variable-length paths.
    #[must_use]
    pub fn with_path_bounds(mut self, min: usize, max: usize) -> Self {
        self.path_bounds = Some((min, max));
        self
    }

    /// Returns the relationship type name.
    #[must_use]
    pub fn rel_type(&self) -> &str {
        &self.rel_schema.name
    }

    /// Returns true if this is a variable-length path.
    #[must_use]
    pub fn is_variable_length(&self) -> bool {
        self.path_bounds.is_some()
    }
}

/// Traversal direction for relationships.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Forward direction (->).
    Forward,
    /// Backward direction (<-).
    Backward,
    /// Both directions (-).
    Both,
}

impl Direction {
    /// Returns the opposite direction.
    #[must_use]
    pub fn reverse(&self) -> Self {
        match self {
            Direction::Forward => Direction::Backward,
            Direction::Backward => Direction::Forward,
            Direction::Both => Direction::Both,
        }
    }
}
