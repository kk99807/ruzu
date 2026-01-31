//! Binder scope for variable tracking.

use std::collections::HashMap;
use std::sync::Arc;

use crate::catalog::NodeTableSchema;
use crate::types::DataType;

/// Variable scope for name resolution during binding.
#[derive(Debug, Clone)]
pub struct BinderScope {
    /// Variable -> bound variable info.
    variables: HashMap<String, BoundVariable>,
    /// Parent scope for subqueries.
    parent: Option<Box<BinderScope>>,
}

impl Default for BinderScope {
    fn default() -> Self {
        Self::new()
    }
}

impl BinderScope {
    /// Creates a new empty scope.
    #[must_use]
    pub fn new() -> Self {
        BinderScope {
            variables: HashMap::new(),
            parent: None,
        }
    }

    /// Creates a child scope with this scope as parent.
    #[must_use]
    pub fn child(&self) -> Self {
        BinderScope {
            variables: HashMap::new(),
            parent: Some(Box::new(self.clone())),
        }
    }

    /// Adds a variable to this scope.
    pub fn add_variable(&mut self, var: BoundVariable) {
        self.variables.insert(var.name.clone(), var);
    }

    /// Looks up a variable by name, checking parent scopes if not found.
    #[must_use]
    pub fn lookup(&self, name: &str) -> Option<&BoundVariable> {
        if let Some(var) = self.variables.get(name) {
            return Some(var);
        }
        if let Some(ref parent) = self.parent {
            return parent.lookup(name);
        }
        None
    }

    /// Returns true if a variable with the given name exists in this scope or parents.
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.lookup(name).is_some()
    }

    /// Returns all variable names in this scope (not including parents).
    #[must_use]
    pub fn variable_names(&self) -> Vec<&str> {
        self.variables.keys().map(String::as_str).collect()
    }

    /// Returns all variables in this scope (not including parents).
    pub fn variables(&self) -> impl Iterator<Item = &BoundVariable> {
        self.variables.values()
    }
}

/// Bound variable information.
#[derive(Debug, Clone)]
pub struct BoundVariable {
    /// Variable name.
    pub name: String,
    /// Type of variable (node, relationship, etc.).
    pub variable_type: VariableType,
    /// Data type of the variable.
    pub data_type: DataType,
    /// Schema if node/relationship.
    pub schema: Option<Arc<NodeTableSchema>>,
}

impl BoundVariable {
    /// Creates a new bound node variable.
    #[must_use]
    pub fn node(name: String, schema: Arc<NodeTableSchema>) -> Self {
        BoundVariable {
            name,
            variable_type: VariableType::Node,
            data_type: DataType::Int64, // Node ID type
            schema: Some(schema),
        }
    }

    /// Creates a new bound relationship variable.
    #[must_use]
    pub fn relationship(name: String, data_type: DataType) -> Self {
        BoundVariable {
            name,
            variable_type: VariableType::Relationship,
            data_type,
            schema: None,
        }
    }

    /// Creates a new bound property variable.
    #[must_use]
    pub fn property(name: String, data_type: DataType) -> Self {
        BoundVariable {
            name,
            variable_type: VariableType::Property,
            data_type,
            schema: None,
        }
    }

    /// Creates a new bound aggregate variable.
    #[must_use]
    pub fn aggregate(name: String, data_type: DataType) -> Self {
        BoundVariable {
            name,
            variable_type: VariableType::Aggregate,
            data_type,
            schema: None,
        }
    }
}

/// Type of bound variable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VariableType {
    /// Node variable.
    Node,
    /// Relationship variable.
    Relationship,
    /// Path variable.
    Path,
    /// Property variable.
    Property,
    /// Aggregate variable.
    Aggregate,
}
