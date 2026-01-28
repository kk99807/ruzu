//! Schema definitions for node and relationship tables.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::error::{Result, RuzuError};
use crate::types::DataType;

/// Central registry of all table schemas in the database.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Catalog {
    /// Node table schemas.
    tables: HashMap<String, NodeTableSchema>,
    /// Relationship table schemas.
    #[serde(default)]
    rel_tables: HashMap<String, RelTableSchema>,
    /// Next table ID for auto-increment.
    #[serde(default)]
    next_table_id: u32,
}

impl Catalog {
    /// Creates a new empty catalog.
    #[must_use]
    pub fn new() -> Self {
        Catalog {
            tables: HashMap::new(),
            rel_tables: HashMap::new(),
            next_table_id: 0,
        }
    }

    /// Returns the next table ID and increments the counter.
    fn next_id(&mut self) -> u32 {
        let id = self.next_table_id;
        self.next_table_id += 1;
        id
    }

    /// Registers a new node table schema in the catalog.
    ///
    /// # Errors
    ///
    /// Returns an error if a table with the same name already exists.
    pub fn create_table(&mut self, mut schema: NodeTableSchema) -> Result<u32> {
        if self.tables.contains_key(&schema.name) || self.rel_tables.contains_key(&schema.name) {
            return Err(RuzuError::SchemaError(format!(
                "Table '{}' already exists",
                schema.name
            )));
        }
        let table_id = self.next_id();
        schema.table_id = table_id;
        self.tables.insert(schema.name.clone(), schema);
        Ok(table_id)
    }

    /// Registers a new relationship table schema in the catalog.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - A table with the same name already exists
    /// - Source or destination tables don't exist
    pub fn create_rel_table(&mut self, mut schema: RelTableSchema) -> Result<u32> {
        if self.tables.contains_key(&schema.name) || self.rel_tables.contains_key(&schema.name) {
            return Err(RuzuError::SchemaError(format!(
                "Table '{}' already exists",
                schema.name
            )));
        }

        // Validate source and destination tables exist
        if !self.tables.contains_key(&schema.src_table) {
            return Err(RuzuError::SchemaError(format!(
                "Source table '{}' does not exist",
                schema.src_table
            )));
        }
        if !self.tables.contains_key(&schema.dst_table) {
            return Err(RuzuError::SchemaError(format!(
                "Destination table '{}' does not exist",
                schema.dst_table
            )));
        }

        let table_id = self.next_id();
        schema.table_id = table_id;
        self.rel_tables.insert(schema.name.clone(), schema);
        Ok(table_id)
    }

    /// Retrieves a node table schema by name.
    #[must_use]
    pub fn get_table(&self, name: &str) -> Option<Arc<NodeTableSchema>> {
        self.tables.get(name).map(|s| Arc::new(s.clone()))
    }

    /// Retrieves a relationship table schema by name.
    #[must_use]
    pub fn get_rel_table(&self, name: &str) -> Option<Arc<RelTableSchema>> {
        self.rel_tables.get(name).map(|s| Arc::new(s.clone()))
    }

    /// Checks if a node table exists in the catalog.
    #[must_use]
    pub fn table_exists(&self, name: &str) -> bool {
        self.tables.contains_key(name)
    }

    /// Checks if a relationship table exists in the catalog.
    #[must_use]
    pub fn rel_table_exists(&self, name: &str) -> bool {
        self.rel_tables.contains_key(name)
    }

    /// Returns all node table names.
    #[must_use]
    pub fn table_names(&self) -> Vec<&str> {
        self.tables.keys().map(String::as_str).collect()
    }

    /// Returns all relationship table names.
    #[must_use]
    pub fn rel_table_names(&self) -> Vec<&str> {
        self.rel_tables.keys().map(String::as_str).collect()
    }

    /// Finds a table name by its ID.
    #[must_use]
    pub fn table_name_by_id(&self, table_id: u32) -> Option<String> {
        // Check node tables
        for (name, schema) in &self.tables {
            if schema.table_id == table_id {
                return Some(name.clone());
            }
        }
        // Check relationship tables
        for (name, schema) in &self.rel_tables {
            if schema.table_id == table_id {
                return Some(name.clone());
            }
        }
        None
    }

    /// Serializes the catalog to bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails.
    pub fn serialize(&self) -> Result<Vec<u8>> {
        bincode::serialize(self)
            .map_err(|e| RuzuError::CatalogError(format!("Failed to serialize catalog: {e}")))
    }

    /// Deserializes a catalog from bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if deserialization fails.
    pub fn deserialize(data: &[u8]) -> Result<Self> {
        bincode::deserialize(data)
            .map_err(|e| RuzuError::CatalogError(format!("Failed to deserialize catalog: {e}")))
    }
}

/// Schema definition for a node table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeTableSchema {
    /// Internal table ID.
    pub table_id: u32,
    /// Table name.
    pub name: String,
    /// Ordered list of column definitions.
    pub columns: Vec<ColumnDef>,
    /// Column names forming the primary key.
    pub primary_key: Vec<String>,
}

impl NodeTableSchema {
    /// Creates a new node table schema with validation.
    ///
    /// # Errors
    ///
    /// Returns an error if validation fails (empty columns, duplicate names, invalid PK).
    pub fn new(name: String, columns: Vec<ColumnDef>, primary_key: Vec<String>) -> Result<Self> {
        let schema = NodeTableSchema {
            table_id: 0, // Will be set by catalog
            name,
            columns,
            primary_key,
        };
        schema.validate()?;
        Ok(schema)
    }

    fn validate(&self) -> Result<()> {
        // Check at least one column
        if self.columns.is_empty() {
            return Err(RuzuError::SchemaError(
                "Table must have at least one column".into(),
            ));
        }

        // Check column name uniqueness
        let mut seen = HashSet::new();
        for col in &self.columns {
            if !seen.insert(&col.name) {
                return Err(RuzuError::SchemaError(format!(
                    "Duplicate column name '{}'",
                    col.name
                )));
            }
        }

        // Check primary key columns exist
        for pk_col in &self.primary_key {
            if !self.columns.iter().any(|c| &c.name == pk_col) {
                return Err(RuzuError::SchemaError(format!(
                    "Primary key column '{pk_col}' not found in table"
                )));
            }
        }

        // Check primary key not empty
        if self.primary_key.is_empty() {
            return Err(RuzuError::SchemaError(
                "Primary key must specify at least one column".into(),
            ));
        }

        Ok(())
    }

    /// Finds a column definition by name.
    #[must_use]
    pub fn get_column(&self, name: &str) -> Option<&ColumnDef> {
        self.columns.iter().find(|c| c.name == name)
    }

    /// Finds the index of a column by name.
    #[must_use]
    pub fn get_column_index(&self, name: &str) -> Option<usize> {
        self.columns.iter().position(|c| c.name == name)
    }
}

/// Definition of a single column in a table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ColumnDef {
    /// Column name.
    pub name: String,
    /// Column data type.
    pub data_type: DataType,
}

impl ColumnDef {
    /// Creates a new column definition.
    ///
    /// # Errors
    ///
    /// Returns an error if the column name is empty.
    pub fn new(name: String, data_type: DataType) -> Result<Self> {
        if name.is_empty() {
            return Err(RuzuError::SchemaError("Column name cannot be empty".into()));
        }
        Ok(ColumnDef { name, data_type })
    }
}

/// Direction for relationship storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Direction {
    /// Store only forward adjacency (src → dst).
    Forward,
    /// Store only backward adjacency (dst → src).
    Backward,
    /// Store both directions (default).
    #[default]
    Both,
}

/// Schema definition for a relationship table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelTableSchema {
    /// Internal table ID.
    pub table_id: u32,
    /// Relationship type name.
    pub name: String,
    /// Source node table name.
    pub src_table: String,
    /// Destination node table name.
    pub dst_table: String,
    /// Relationship property columns.
    pub columns: Vec<ColumnDef>,
    /// Storage direction.
    pub direction: Direction,
}

impl RelTableSchema {
    /// Creates a new relationship table schema.
    ///
    /// # Errors
    ///
    /// Returns an error if validation fails (duplicate column names, etc.).
    pub fn new(
        name: String,
        src_table: String,
        dst_table: String,
        columns: Vec<ColumnDef>,
        direction: Direction,
    ) -> Result<Self> {
        let schema = RelTableSchema {
            table_id: 0, // Will be set by catalog
            name,
            src_table,
            dst_table,
            columns,
            direction,
        };
        schema.validate()?;
        Ok(schema)
    }

    fn validate(&self) -> Result<()> {
        // Check column name uniqueness
        let mut seen = HashSet::new();
        for col in &self.columns {
            if !seen.insert(&col.name) {
                return Err(RuzuError::SchemaError(format!(
                    "Duplicate column name '{}'",
                    col.name
                )));
            }
        }

        Ok(())
    }

    /// Finds a column definition by name.
    #[must_use]
    pub fn get_column(&self, name: &str) -> Option<&ColumnDef> {
        self.columns.iter().find(|c| c.name == name)
    }

    /// Finds the index of a column by name.
    #[must_use]
    pub fn get_column_index(&self, name: &str) -> Option<usize> {
        self.columns.iter().position(|c| c.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_catalog_serialization() {
        let mut catalog = Catalog::new();

        let schema = NodeTableSchema::new(
            "Person".to_string(),
            vec![
                ColumnDef::new("name".to_string(), DataType::String).unwrap(),
                ColumnDef::new("age".to_string(), DataType::Int64).unwrap(),
            ],
            vec!["name".to_string()],
        )
        .unwrap();

        catalog.create_table(schema).unwrap();

        // Serialize and deserialize
        let bytes = catalog.serialize().unwrap();
        let restored = Catalog::deserialize(&bytes).unwrap();

        assert!(restored.table_exists("Person"));
        let table = restored.get_table("Person").unwrap();
        assert_eq!(table.columns.len(), 2);
    }

    #[test]
    fn test_rel_table_creation() {
        let mut catalog = Catalog::new();

        // Create node tables first
        let person = NodeTableSchema::new(
            "Person".to_string(),
            vec![ColumnDef::new("name".to_string(), DataType::String).unwrap()],
            vec!["name".to_string()],
        )
        .unwrap();
        catalog.create_table(person).unwrap();

        // Create relationship table
        let knows = RelTableSchema::new(
            "KNOWS".to_string(),
            "Person".to_string(),
            "Person".to_string(),
            vec![ColumnDef::new("since".to_string(), DataType::Int64).unwrap()],
            Direction::Both,
        )
        .unwrap();

        let id = catalog.create_rel_table(knows).unwrap();
        assert_eq!(id, 1); // Second table

        assert!(catalog.rel_table_exists("KNOWS"));
    }

    #[test]
    fn test_rel_table_invalid_src() {
        let mut catalog = Catalog::new();

        let knows = RelTableSchema::new(
            "KNOWS".to_string(),
            "NonExistent".to_string(),
            "Person".to_string(),
            vec![],
            Direction::Both,
        )
        .unwrap();

        let result = catalog.create_rel_table(knows);
        assert!(result.is_err());
    }
}
