//! Contract tests for the Planner module.
//!
//! These tests verify the query planning contracts:
//! - NodeScan produces correct output schema
//! - Filter preserves input schema
//! - Project produces declared output schema

use ruzu::binder::{BoundExpression, BoundNode, BoundQuery, BoundReturn, QueryGraph};
use ruzu::catalog::{Catalog, ColumnDef, Direction, NodeTableSchema, RelTableSchema};
use ruzu::planner::{JoinType, LogicalPlan, Planner};
use ruzu::types::DataType;

/// Creates a test catalog with a Person table.
fn create_test_catalog() -> Catalog {
    let mut catalog = Catalog::new();

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

    catalog
}

#[test]
fn test_node_scan_produces_correct_schema() {
    // Contract: A NodeScan's output_schema should include all columns from the table
    //           with the variable prefix (e.g., "p.id", "p.name", "p.age")
    let catalog = create_test_catalog();
    let schema = catalog.get_table("Person").unwrap();

    let plan = LogicalPlan::node_scan("Person".to_string(), "p".to_string(), schema);

    let output_schema = plan.output_schema();

    // Should have 3 columns: id, name, age (all prefixed with "p.")
    assert_eq!(output_schema.len(), 3, "Should have 3 output columns");

    let col_names: Vec<&str> = output_schema.iter().map(|(n, _)| n.as_str()).collect();
    assert!(col_names.contains(&"p.id"), "Should include p.id");
    assert!(col_names.contains(&"p.name"), "Should include p.name");
    assert!(col_names.contains(&"p.age"), "Should include p.age");

    // Check types
    for (name, dtype) in &output_schema {
        match name.as_str() {
            "p.id" | "p.age" => assert_eq!(*dtype, DataType::Int64),
            "p.name" => assert_eq!(*dtype, DataType::String),
            _ => panic!("Unexpected column: {}", name),
        }
    }
}

#[test]
fn test_filter_preserves_input_schema() {
    // Contract: A Filter should preserve the schema of its input
    let catalog = create_test_catalog();
    let schema = catalog.get_table("Person").unwrap();

    let scan = LogicalPlan::node_scan("Person".to_string(), "p".to_string(), schema);
    let scan_schema = scan.output_schema();

    // Create a filter with a dummy predicate
    let predicate = BoundExpression::literal(ruzu::types::Value::Bool(true));
    let filter = LogicalPlan::filter(scan, predicate);

    let filter_schema = filter.output_schema();

    // Filter should not change the schema
    assert_eq!(
        filter_schema, scan_schema,
        "Filter should preserve input schema"
    );
}

#[test]
fn test_project_produces_declared_schema() {
    // Contract: A Project's output_schema should match the declared projections
    let catalog = create_test_catalog();
    let schema = catalog.get_table("Person").unwrap();

    let scan = LogicalPlan::node_scan("Person".to_string(), "p".to_string(), schema);

    // Project only name and age
    let projections = vec![
        (
            "person_name".to_string(),
            BoundExpression::property_access(
                "p".to_string(),
                "name".to_string(),
                DataType::String,
            ),
        ),
        (
            "person_age".to_string(),
            BoundExpression::property_access("p".to_string(), "age".to_string(), DataType::Int64),
        ),
    ];

    let project = LogicalPlan::project(scan, projections);

    let output_schema = project.output_schema();

    assert_eq!(output_schema.len(), 2, "Should have 2 projected columns");
    assert_eq!(output_schema[0].0, "person_name");
    assert_eq!(output_schema[0].1, DataType::String);
    assert_eq!(output_schema[1].0, "person_age");
    assert_eq!(output_schema[1].1, DataType::Int64);
}

#[test]
fn test_limit_preserves_input_schema() {
    // Contract: A Limit should preserve the schema of its input
    let catalog = create_test_catalog();
    let schema = catalog.get_table("Person").unwrap();

    let scan = LogicalPlan::node_scan("Person".to_string(), "p".to_string(), schema);
    let scan_schema = scan.output_schema();

    let limited = LogicalPlan::limit(scan, Some(10), Some(100));

    let limit_schema = limited.output_schema();

    assert_eq!(
        limit_schema, scan_schema,
        "Limit should preserve input schema"
    );
}

#[test]
fn test_empty_plan_uses_declared_schema() {
    // Contract: An Empty plan should report the schema it was constructed with
    let declared_schema = vec![
        ("col1".to_string(), DataType::Int64),
        ("col2".to_string(), DataType::String),
    ];

    let empty = LogicalPlan::empty(declared_schema.clone());

    let output_schema = empty.output_schema();

    assert_eq!(output_schema, declared_schema, "Empty should use declared schema");
}

#[test]
fn test_planner_creates_logical_plan_from_bound_query() {
    // Contract: Planner should convert a BoundQuery into a LogicalPlan
    let catalog = create_test_catalog();
    let planner = Planner::new(&catalog);

    // Create a simple bound query: MATCH (p:Person) RETURN p.name
    let schema = catalog.get_table("Person").unwrap();
    let mut query_graph = QueryGraph::new();
    query_graph.add_node(BoundNode::new("p".to_string(), schema));

    let return_clause = BoundReturn::new(vec![(
        "p.name".to_string(),
        BoundExpression::property_access("p".to_string(), "name".to_string(), DataType::String),
    )]);

    let bound_query = BoundQuery::new(query_graph, return_clause);

    let plan = planner.plan(&bound_query);

    assert!(plan.is_ok(), "Planner should create plan from bound query");
}

#[test]
fn test_planner_explain_produces_readable_output() {
    // Contract: Planner::explain should produce a readable string description
    let catalog = create_test_catalog();
    let planner = Planner::new(&catalog);
    let schema = catalog.get_table("Person").unwrap();

    let plan = LogicalPlan::node_scan("Person".to_string(), "p".to_string(), schema);

    let explanation = planner.explain(&plan);

    assert!(!explanation.is_empty(), "Explain should produce output");
    assert!(
        explanation.contains("NodeScan") || explanation.contains("Person"),
        "Explain should describe the plan: {}",
        explanation
    );
}

// =============================================================================
// Phase 4: Hash Join Contract Tests (T051)
// =============================================================================

/// Creates a test catalog with Person, Company, and WORKS_AT tables.
fn create_join_test_catalog() -> Catalog {
    let mut catalog = Catalog::new();

    // Person table
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

    // Company table
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

    // WORKS_AT relationship table
    let rel_schema = RelTableSchema::new(
        "WORKS_AT".to_string(),
        "Person".to_string(),
        "Company".to_string(),
        vec![ColumnDef::new("since".to_string(), DataType::Int64).unwrap()],
        Direction::Both,
    )
    .unwrap();
    catalog.create_rel_table(rel_schema).unwrap();

    catalog
}

#[test]
fn test_hash_join_combines_schemas() {
    // Contract: HashJoin output_schema should include all columns from both inputs
    let catalog = create_join_test_catalog();
    let person_schema = catalog.get_table("Person").unwrap();
    let company_schema = catalog.get_table("Company").unwrap();

    let left_scan = LogicalPlan::node_scan("Person".to_string(), "p".to_string(), person_schema);
    let right_scan =
        LogicalPlan::node_scan("Company".to_string(), "c".to_string(), company_schema);

    let hash_join = LogicalPlan::HashJoin {
        left: Box::new(left_scan),
        right: Box::new(right_scan),
        left_keys: vec!["p.company_id".to_string()],
        right_keys: vec!["c.id".to_string()],
        join_type: JoinType::Inner,
    };

    let output_schema = hash_join.output_schema();

    // Should include columns from both Person (3) and Company (2)
    assert_eq!(output_schema.len(), 5, "HashJoin should have 5 output columns");

    let col_names: Vec<&str> = output_schema.iter().map(|(n, _)| n.as_str()).collect();
    assert!(col_names.contains(&"p.id"), "Should include p.id");
    assert!(col_names.contains(&"p.name"), "Should include p.name");
    assert!(col_names.contains(&"p.age"), "Should include p.age");
    assert!(col_names.contains(&"c.id"), "Should include c.id");
    assert!(col_names.contains(&"c.name"), "Should include c.name");
}

#[test]
fn test_hash_join_preserves_input_types() {
    // Contract: HashJoin should preserve the types from both input plans
    let catalog = create_join_test_catalog();
    let person_schema = catalog.get_table("Person").unwrap();
    let company_schema = catalog.get_table("Company").unwrap();

    let left_scan = LogicalPlan::node_scan("Person".to_string(), "p".to_string(), person_schema);
    let right_scan =
        LogicalPlan::node_scan("Company".to_string(), "c".to_string(), company_schema);

    let hash_join = LogicalPlan::HashJoin {
        left: Box::new(left_scan),
        right: Box::new(right_scan),
        left_keys: vec!["p.company_id".to_string()],
        right_keys: vec!["c.id".to_string()],
        join_type: JoinType::Inner,
    };

    let output_schema = hash_join.output_schema();

    for (name, dtype) in &output_schema {
        match name.as_str() {
            "p.id" | "p.age" | "c.id" => {
                assert_eq!(*dtype, DataType::Int64, "{} should be Int64", name);
            }
            "p.name" | "c.name" => {
                assert_eq!(*dtype, DataType::String, "{} should be String", name);
            }
            _ => panic!("Unexpected column: {}", name),
        }
    }
}

#[test]
fn test_hash_join_has_correct_children() {
    // Contract: HashJoin should return both left and right inputs as children
    let catalog = create_join_test_catalog();
    let person_schema = catalog.get_table("Person").unwrap();
    let company_schema = catalog.get_table("Company").unwrap();

    let left_scan = LogicalPlan::node_scan("Person".to_string(), "p".to_string(), person_schema);
    let right_scan =
        LogicalPlan::node_scan("Company".to_string(), "c".to_string(), company_schema);

    let hash_join = LogicalPlan::HashJoin {
        left: Box::new(left_scan),
        right: Box::new(right_scan),
        left_keys: vec!["p.company_id".to_string()],
        right_keys: vec!["c.id".to_string()],
        join_type: JoinType::Inner,
    };

    let children = hash_join.children();

    assert_eq!(children.len(), 2, "HashJoin should have 2 children");
}

// =============================================================================
// Phase 9: EXPLAIN Contract Tests (T110-T112)
// =============================================================================

#[test]
fn test_explain_output_format() {
    // Contract: T110 - EXPLAIN should produce formatted tree output
    let catalog = create_test_catalog();
    let planner = Planner::new(&catalog);
    let schema = catalog.get_table("Person").unwrap();

    // Create a simple plan
    let scan = LogicalPlan::node_scan("Person".to_string(), "p".to_string(), schema);

    // Get EXPLAIN output
    let explanation = planner.explain(&scan);

    // Should produce non-empty output
    assert!(!explanation.is_empty(), "EXPLAIN should produce output");

    // Should contain plan operator name
    assert!(
        explanation.contains("NodeScan") || explanation.contains("Scan"),
        "EXPLAIN should mention the scan operator: {}",
        explanation
    );

    // Should contain table name
    assert!(
        explanation.contains("Person"),
        "EXPLAIN should mention the table name: {}",
        explanation
    );
}

#[test]
fn test_explain_shows_filter_pushdown() {
    // Contract: T111 - EXPLAIN should show filter pushdown optimization
    let catalog = create_test_catalog();
    let planner = Planner::new(&catalog);
    let schema = catalog.get_table("Person").unwrap();

    // Create a scan with filter pushed down
    let scan = LogicalPlan::node_scan("Person".to_string(), "p".to_string(), schema);

    // Add a filter predicate
    let predicate = BoundExpression::comparison(
        BoundExpression::property_access("p".to_string(), "age".to_string(), DataType::Int64),
        ruzu::binder::ComparisonOp::Gt,
        BoundExpression::literal(ruzu::types::Value::Int64(25)),
    );

    // Wrap with filter
    let filtered_plan = LogicalPlan::filter(scan, predicate);

    // Get EXPLAIN output
    let explanation = planner.explain(&filtered_plan);

    // Should contain filter information
    assert!(
        explanation.contains("Filter") || explanation.contains("filter"),
        "EXPLAIN should show filter: {}",
        explanation
    );
}

#[test]
fn test_explain_complex_query() {
    // Contract: T112 - EXPLAIN should handle complex queries with multiple operators
    let catalog = create_join_test_catalog();
    let planner = Planner::new(&catalog);
    let person_schema = catalog.get_table("Person").unwrap();
    let company_schema = catalog.get_table("Company").unwrap();

    // Create a complex plan with join, filter, and limit
    let left_scan = LogicalPlan::node_scan("Person".to_string(), "p".to_string(), person_schema);
    let right_scan = LogicalPlan::node_scan("Company".to_string(), "c".to_string(), company_schema);

    let hash_join = LogicalPlan::HashJoin {
        left: Box::new(left_scan),
        right: Box::new(right_scan),
        left_keys: vec!["p.company_id".to_string()],
        right_keys: vec!["c.id".to_string()],
        join_type: JoinType::Inner,
    };

    let limited = LogicalPlan::limit(hash_join, Some(5), Some(10));

    // Get EXPLAIN output
    let explanation = planner.explain(&limited);

    // Should show all operators
    assert!(
        explanation.contains("Limit") || explanation.contains("limit"),
        "EXPLAIN should show limit: {}",
        explanation
    );
    assert!(
        explanation.contains("Join") || explanation.contains("join") || explanation.contains("HashJoin"),
        "EXPLAIN should show join: {}",
        explanation
    );
}

#[test]
fn test_explain_shows_projection() {
    // Contract: EXPLAIN should show projection operators
    let catalog = create_test_catalog();
    let planner = Planner::new(&catalog);
    let schema = catalog.get_table("Person").unwrap();

    let scan = LogicalPlan::node_scan("Person".to_string(), "p".to_string(), schema);

    let projections = vec![
        (
            "name".to_string(),
            BoundExpression::property_access("p".to_string(), "name".to_string(), DataType::String),
        ),
    ];

    let projected = LogicalPlan::project(scan, projections);

    let explanation = planner.explain(&projected);

    // Should contain project information
    assert!(
        explanation.contains("Project") || explanation.contains("project"),
        "EXPLAIN should show projection: {}",
        explanation
    );
}
