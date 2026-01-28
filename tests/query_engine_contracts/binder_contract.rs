//! Contract tests for the Binder module.
//!
//! These tests verify the semantic analysis contracts:
//! - Undefined variables are rejected
//! - Undefined tables are rejected
//! - Undefined columns are rejected

use ruzu::binder::{Binder, Direction, VariableType};
use ruzu::catalog::{Catalog, ColumnDef, Direction as CatalogDirection, NodeTableSchema, RelTableSchema};
use ruzu::error::RuzuError;
use ruzu::types::DataType;

/// Creates a test catalog with Person and Company tables.
fn create_test_catalog() -> Catalog {
    let mut catalog = Catalog::new();

    // Create Person table
    let person_schema = NodeTableSchema::new(
        "Person".to_string(),
        vec![
            ColumnDef::new("id".to_string(), DataType::Int64).unwrap(),
            ColumnDef::new("name".to_string(), DataType::String).unwrap(),
            ColumnDef::new("age".to_string(), DataType::Int64).unwrap(),
        ],
        vec!["id".to_string()],
    )
    .unwrap();
    catalog.create_table(person_schema).unwrap();

    // Create Company table
    let company_schema = NodeTableSchema::new(
        "Company".to_string(),
        vec![
            ColumnDef::new("id".to_string(), DataType::Int64).unwrap(),
            ColumnDef::new("name".to_string(), DataType::String).unwrap(),
        ],
        vec!["id".to_string()],
    )
    .unwrap();
    catalog.create_table(company_schema).unwrap();

    // Create WORKS_AT relationship
    let works_at_schema = RelTableSchema::new(
        "WORKS_AT".to_string(),
        "Person".to_string(),
        "Company".to_string(),
        vec![ColumnDef::new("since".to_string(), DataType::Int64).unwrap()],
        CatalogDirection::Both,
    )
    .unwrap();
    catalog.create_rel_table(works_at_schema).unwrap();

    catalog
}

#[test]
fn test_bind_undefined_variable_rejected() {
    // Contract: Referencing an undefined variable in scope should fail
    let catalog = create_test_catalog();
    let binder = Binder::new(&catalog);

    // Attempt to validate a variable that was never added to scope
    let result = binder.validate_variable("x");

    assert!(result.is_err(), "Undefined variable should be rejected");
    match result {
        Err(RuzuError::BindError(msg)) => {
            assert!(
                msg.contains("Undefined variable"),
                "Error should mention undefined variable: {}",
                msg
            );
        }
        _ => panic!("Expected BindError for undefined variable"),
    }
}

#[test]
fn test_bind_undefined_table_rejected() {
    // Contract: Referencing a table that doesn't exist in catalog should fail
    let catalog = create_test_catalog();
    let mut binder = Binder::new(&catalog);

    // Attempt to bind a node with a non-existent label
    let result = binder.bind_node("p", "NonExistentTable");

    assert!(result.is_err(), "Undefined table should be rejected");
    match result {
        Err(RuzuError::BindError(msg)) => {
            assert!(
                msg.contains("Undefined table"),
                "Error should mention undefined table: {}",
                msg
            );
        }
        _ => panic!("Expected BindError for undefined table"),
    }
}

#[test]
fn test_bind_undefined_column_rejected() {
    // Contract: Referencing a column that doesn't exist on a table should fail
    let catalog = create_test_catalog();
    let mut binder = Binder::new(&catalog);

    // First, bind a node so we have a variable in scope
    binder.bind_node("p", "Person").unwrap();

    // Now try to validate a property that doesn't exist
    let result = binder.validate_property("p", "nonexistent_column");

    assert!(result.is_err(), "Undefined column should be rejected");
    match result {
        Err(RuzuError::BindError(msg)) => {
            assert!(
                msg.contains("Undefined column") || msg.contains("column"),
                "Error should mention undefined column: {}",
                msg
            );
        }
        _ => panic!("Expected BindError for undefined column"),
    }
}

#[test]
fn test_bind_duplicate_variable_rejected() {
    // Contract: Binding the same variable twice should fail
    let catalog = create_test_catalog();
    let mut binder = Binder::new(&catalog);

    // Bind first node
    binder.bind_node("p", "Person").unwrap();

    // Attempt to bind another node with the same variable name
    let result = binder.bind_node("p", "Company");

    assert!(result.is_err(), "Duplicate variable should be rejected");
    match result {
        Err(RuzuError::BindError(msg)) => {
            assert!(
                msg.contains("Duplicate variable"),
                "Error should mention duplicate variable: {}",
                msg
            );
        }
        _ => panic!("Expected BindError for duplicate variable"),
    }
}

#[test]
fn test_bind_valid_node_succeeds() {
    // Contract: Binding a valid node with existing table should succeed
    let catalog = create_test_catalog();
    let mut binder = Binder::new(&catalog);

    let result = binder.bind_node("p", "Person");

    assert!(result.is_ok(), "Valid node binding should succeed");
    let bound_node = result.unwrap();
    assert_eq!(bound_node.variable, "p");
    assert_eq!(bound_node.table_name(), "Person");
}

#[test]
fn test_bind_valid_property_returns_correct_type() {
    // Contract: Validating a valid property should return its correct type
    let catalog = create_test_catalog();
    let mut binder = Binder::new(&catalog);

    binder.bind_node("p", "Person").unwrap();

    let age_type = binder.validate_property("p", "age").unwrap();
    assert_eq!(age_type, DataType::Int64);

    let name_type = binder.validate_property("p", "name").unwrap();
    assert_eq!(name_type, DataType::String);
}

#[test]
fn test_bind_relationship_with_undefined_source_rejected() {
    // Contract: Binding a relationship with undefined source variable should fail
    let catalog = create_test_catalog();
    let mut binder = Binder::new(&catalog);

    // Don't bind any nodes, try to create relationship
    binder.bind_node("c", "Company").unwrap();

    let result = binder.bind_relationship(
        Some("r"),
        "WORKS_AT",
        "p", // undefined
        "c",
        Direction::Forward,
    );

    assert!(
        result.is_err(),
        "Relationship with undefined source should be rejected"
    );
}

#[test]
fn test_bind_relationship_with_undefined_destination_rejected() {
    // Contract: Binding a relationship with undefined destination variable should fail
    let catalog = create_test_catalog();
    let mut binder = Binder::new(&catalog);

    binder.bind_node("p", "Person").unwrap();
    // Don't bind company

    let result = binder.bind_relationship(
        Some("r"),
        "WORKS_AT",
        "p",
        "c", // undefined
        Direction::Forward,
    );

    assert!(
        result.is_err(),
        "Relationship with undefined destination should be rejected"
    );
}

#[test]
fn test_scope_lookup_returns_bound_variable() {
    // Contract: Looking up a variable in scope should return the bound variable info
    let catalog = create_test_catalog();
    let mut binder = Binder::new(&catalog);

    binder.bind_node("p", "Person").unwrap();

    let var = binder.scope().lookup("p");
    assert!(var.is_some(), "Bound variable should be found in scope");
    let var = var.unwrap();
    assert_eq!(var.name, "p");
    assert_eq!(var.variable_type, VariableType::Node);
}
