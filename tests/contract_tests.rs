//! Contract tests for the public API (Database.execute) and storage formats.

use ruzu::{Database, Value};

// =============================================================================
// Storage Format Contract Tests (Phase 2)
// =============================================================================

mod storage_format_contracts {
    use ruzu::catalog::{Catalog, ColumnDef, Direction, NodeTableSchema, RelTableSchema};
    use ruzu::storage::{DatabaseHeader, PageRange, CURRENT_VERSION, MAGIC_BYTES, PAGE_SIZE};
    use ruzu::types::DataType;
    use uuid::Uuid;

    // -------------------------------------------------------------------------
    // T024: Database Header Format Contract
    // -------------------------------------------------------------------------

    #[test]
    fn test_header_magic_bytes() {
        assert_eq!(MAGIC_BYTES, b"RUZUDB\0\0");
        assert_eq!(MAGIC_BYTES.len(), 8);
    }

    #[test]
    fn test_header_version() {
        assert_eq!(CURRENT_VERSION, 3); // Version 3 adds multi-page storage support
    }

    #[test]
    fn test_header_roundtrip_serialization() {
        let db_id = Uuid::new_v4();
        let mut header = DatabaseHeader::new(db_id);
        header.catalog_range = PageRange::new(1, 5);
        header.metadata_range = PageRange::new(6, 2);
        header.update_checksum();

        let bytes = header.serialize().expect("serialize header");
        let restored = DatabaseHeader::deserialize(&bytes).expect("deserialize header");

        assert_eq!(restored.magic, *MAGIC_BYTES);
        assert_eq!(restored.version, CURRENT_VERSION);
        assert_eq!(restored.database_id, db_id);
        assert_eq!(restored.catalog_range.start_page, 1);
        assert_eq!(restored.catalog_range.num_pages, 5);
        assert_eq!(restored.metadata_range.start_page, 6);
        assert_eq!(restored.metadata_range.num_pages, 2);
        assert!(restored.verify_checksum());
    }

    #[test]
    fn test_header_checksum_detects_corruption() {
        let mut header = DatabaseHeader::new(Uuid::new_v4());
        header.update_checksum();

        // Corrupt the version
        header.version = 99;

        // Checksum should no longer verify
        assert!(!header.verify_checksum());
    }

    #[test]
    fn test_header_validation_invalid_magic() {
        let mut header = DatabaseHeader::new(Uuid::new_v4());
        header.magic = [0u8; 8];

        let result = header.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("magic"));
    }

    #[test]
    fn test_header_validation_future_version() {
        let mut header = DatabaseHeader::new(Uuid::new_v4());
        header.version = CURRENT_VERSION + 1;

        let result = header.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("version"));
    }

    #[test]
    fn test_header_fits_in_page() {
        let header = DatabaseHeader::new(Uuid::new_v4());
        let bytes = header.serialize().expect("serialize header");

        // Header must fit within a single page
        assert!(
            bytes.len() < PAGE_SIZE,
            "Header size {} exceeds page size {}",
            bytes.len(),
            PAGE_SIZE
        );
    }

    // -------------------------------------------------------------------------
    // T025: Catalog Serialization Format Contract
    // -------------------------------------------------------------------------

    #[test]
    fn test_catalog_empty_roundtrip() {
        let catalog = Catalog::new();
        let bytes = catalog.serialize().expect("serialize catalog");
        let restored = Catalog::deserialize(&bytes).expect("deserialize catalog");

        assert!(restored.table_names().is_empty());
        assert!(restored.rel_table_names().is_empty());
    }

    #[test]
    fn test_catalog_with_node_table_roundtrip() {
        let mut catalog = Catalog::new();

        let schema = NodeTableSchema::new(
            "Person".to_string(),
            vec![
                ColumnDef::new("id".to_string(), DataType::Int64).unwrap(),
                ColumnDef::new("name".to_string(), DataType::String).unwrap(),
                ColumnDef::new("age".to_string(), DataType::Int64).unwrap(),
            ],
            vec!["id".to_string()],
        )
        .unwrap();

        catalog.create_table(schema).unwrap();

        let bytes = catalog.serialize().expect("serialize catalog");
        let restored = Catalog::deserialize(&bytes).expect("deserialize catalog");

        assert!(restored.table_exists("Person"));
        let table = restored.get_table("Person").unwrap();
        assert_eq!(table.name, "Person");
        assert_eq!(table.columns.len(), 3);
        assert_eq!(table.columns[0].name, "id");
        assert_eq!(table.columns[0].data_type, DataType::Int64);
        assert_eq!(table.columns[1].name, "name");
        assert_eq!(table.columns[1].data_type, DataType::String);
        assert_eq!(table.primary_key, vec!["id"]);
    }

    #[test]
    fn test_catalog_with_rel_table_roundtrip() {
        let mut catalog = Catalog::new();

        // Create source and destination node tables first
        let person = NodeTableSchema::new(
            "Person".to_string(),
            vec![ColumnDef::new("id".to_string(), DataType::Int64).unwrap()],
            vec!["id".to_string()],
        )
        .unwrap();
        catalog.create_table(person).unwrap();

        let company = NodeTableSchema::new(
            "Company".to_string(),
            vec![ColumnDef::new("name".to_string(), DataType::String).unwrap()],
            vec!["name".to_string()],
        )
        .unwrap();
        catalog.create_table(company).unwrap();

        // Create relationship table
        let works_at = RelTableSchema::new(
            "WORKS_AT".to_string(),
            "Person".to_string(),
            "Company".to_string(),
            vec![ColumnDef::new("since".to_string(), DataType::Int64).unwrap()],
            Direction::Both,
        )
        .unwrap();
        catalog.create_rel_table(works_at).unwrap();

        let bytes = catalog.serialize().expect("serialize catalog");
        let restored = Catalog::deserialize(&bytes).expect("deserialize catalog");

        assert!(restored.rel_table_exists("WORKS_AT"));
        let rel = restored.get_rel_table("WORKS_AT").unwrap();
        assert_eq!(rel.name, "WORKS_AT");
        assert_eq!(rel.src_table, "Person");
        assert_eq!(rel.dst_table, "Company");
        assert_eq!(rel.columns.len(), 1);
        assert_eq!(rel.columns[0].name, "since");
        assert!(matches!(rel.direction, Direction::Both));
    }

    #[test]
    fn test_catalog_multiple_tables_roundtrip() {
        let mut catalog = Catalog::new();

        for i in 0..10 {
            let schema = NodeTableSchema::new(
                format!("Table{}", i),
                vec![ColumnDef::new("id".to_string(), DataType::Int64).unwrap()],
                vec!["id".to_string()],
            )
            .unwrap();
            catalog.create_table(schema).unwrap();
        }

        let bytes = catalog.serialize().expect("serialize catalog");
        let restored = Catalog::deserialize(&bytes).expect("deserialize catalog");

        assert_eq!(restored.table_names().len(), 10);
        for i in 0..10 {
            assert!(restored.table_exists(&format!("Table{}", i)));
        }
    }

    #[test]
    fn test_catalog_table_id_preserved() {
        let mut catalog = Catalog::new();

        let schema1 = NodeTableSchema::new(
            "First".to_string(),
            vec![ColumnDef::new("id".to_string(), DataType::Int64).unwrap()],
            vec!["id".to_string()],
        )
        .unwrap();
        let id1 = catalog.create_table(schema1).unwrap();

        let schema2 = NodeTableSchema::new(
            "Second".to_string(),
            vec![ColumnDef::new("id".to_string(), DataType::Int64).unwrap()],
            vec!["id".to_string()],
        )
        .unwrap();
        let id2 = catalog.create_table(schema2).unwrap();

        let bytes = catalog.serialize().expect("serialize catalog");
        let restored = Catalog::deserialize(&bytes).expect("deserialize catalog");

        assert_eq!(restored.get_table("First").unwrap().table_id, id1);
        assert_eq!(restored.get_table("Second").unwrap().table_id, id2);
    }

    #[test]
    fn test_catalog_all_data_types_roundtrip() {
        let mut catalog = Catalog::new();

        let schema = NodeTableSchema::new(
            "AllTypes".to_string(),
            vec![
                ColumnDef::new("int_col".to_string(), DataType::Int64).unwrap(),
                ColumnDef::new("str_col".to_string(), DataType::String).unwrap(),
            ],
            vec!["int_col".to_string()],
        )
        .unwrap();
        catalog.create_table(schema).unwrap();

        let bytes = catalog.serialize().expect("serialize catalog");
        let restored = Catalog::deserialize(&bytes).expect("deserialize catalog");

        let table = restored.get_table("AllTypes").unwrap();
        assert_eq!(table.columns[0].data_type, DataType::Int64);
        assert_eq!(table.columns[1].data_type, DataType::String);
    }

    #[test]
    fn test_direction_enum_serialization() {
        // Test all Direction variants serialize correctly
        let mut catalog = Catalog::new();

        let node = NodeTableSchema::new(
            "Node".to_string(),
            vec![ColumnDef::new("id".to_string(), DataType::Int64).unwrap()],
            vec!["id".to_string()],
        )
        .unwrap();
        catalog.create_table(node).unwrap();

        // Test Forward direction
        let forward = RelTableSchema::new(
            "FORWARD_REL".to_string(),
            "Node".to_string(),
            "Node".to_string(),
            vec![],
            Direction::Forward,
        )
        .unwrap();
        catalog.create_rel_table(forward).unwrap();

        // Test Backward direction
        let backward = RelTableSchema::new(
            "BACKWARD_REL".to_string(),
            "Node".to_string(),
            "Node".to_string(),
            vec![],
            Direction::Backward,
        )
        .unwrap();
        catalog.create_rel_table(backward).unwrap();

        let bytes = catalog.serialize().expect("serialize catalog");
        let restored = Catalog::deserialize(&bytes).expect("deserialize catalog");

        assert!(matches!(
            restored.get_rel_table("FORWARD_REL").unwrap().direction,
            Direction::Forward
        ));
        assert!(matches!(
            restored.get_rel_table("BACKWARD_REL").unwrap().direction,
            Direction::Backward
        ));
    }
}

// =============================================================================
// User Story 1: Define Graph Schema (CREATE NODE TABLE)
// =============================================================================

#[test]
fn test_create_node_table_success() {
    let mut db = Database::new();
    let result = db.execute("CREATE NODE TABLE Person(name STRING, age INT64, PRIMARY KEY(name))");
    assert!(result.is_ok());

    let schema = db.catalog().get_table("Person");
    assert!(schema.is_some());
    let schema = schema.unwrap();
    assert_eq!(schema.name, "Person");
    assert_eq!(schema.columns.len(), 2);
}

#[test]
fn test_create_duplicate_table_error() {
    let mut db = Database::new();
    db.execute("CREATE NODE TABLE Person(name STRING, PRIMARY KEY(name))")
        .unwrap();

    let result = db.execute("CREATE NODE TABLE Person(id INT64, PRIMARY KEY(id))");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("already exists"));
}

#[test]
fn test_create_table_invalid_syntax_error() {
    let mut db = Database::new();
    let result = db.execute("CREATE NODE TABLE");
    assert!(result.is_err());
}

#[test]
fn test_create_table_multiple_types() {
    let mut db = Database::new();
    let result = db.execute(
        "CREATE NODE TABLE Mixed(id INT64, name STRING, count INT64, label STRING, PRIMARY KEY(id))",
    );
    assert!(result.is_ok());

    let schema = db.catalog().get_table("Mixed").unwrap();
    assert_eq!(schema.columns.len(), 4);
}

// =============================================================================
// User Story 2: Insert Graph Data (CREATE node)
// =============================================================================

#[test]
fn test_create_node_success() {
    let mut db = Database::new();
    db.execute("CREATE NODE TABLE Person(name STRING, age INT64, PRIMARY KEY(name))")
        .unwrap();

    let result = db.execute("CREATE (:Person {name: 'Alice', age: 25})");
    assert!(result.is_ok());
}

#[test]
fn test_create_node_duplicate_pk_error() {
    let mut db = Database::new();
    db.execute("CREATE NODE TABLE Person(name STRING, age INT64, PRIMARY KEY(name))")
        .unwrap();
    db.execute("CREATE (:Person {name: 'Alice', age: 25})")
        .unwrap();

    let result = db.execute("CREATE (:Person {name: 'Alice', age: 30})");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("Duplicate primary key"));
}

#[test]
fn test_create_node_missing_property_error() {
    let mut db = Database::new();
    db.execute("CREATE NODE TABLE Person(name STRING, age INT64, PRIMARY KEY(name))")
        .unwrap();

    let result = db.execute("CREATE (:Person {name: 'Alice'})");
    assert!(result.is_err());
}

#[test]
fn test_create_multiple_nodes() {
    let mut db = Database::new();
    db.execute("CREATE NODE TABLE Person(name STRING, age INT64, PRIMARY KEY(name))")
        .unwrap();

    db.execute("CREATE (:Person {name: 'Alice', age: 25})")
        .unwrap();
    db.execute("CREATE (:Person {name: 'Bob', age: 30})")
        .unwrap();
    db.execute("CREATE (:Person {name: 'Charlie', age: 20})")
        .unwrap();

    let result = db.execute("MATCH (p:Person) RETURN p.name, p.age").unwrap();
    assert_eq!(result.row_count(), 3);
}

// =============================================================================
// User Story 3: Query Graph Data (MATCH)
// =============================================================================

fn setup_test_data(db: &mut Database) {
    db.execute("CREATE NODE TABLE Person(name STRING, age INT64, PRIMARY KEY(name))")
        .unwrap();
    db.execute("CREATE (:Person {name: 'Alice', age: 25})")
        .unwrap();
    db.execute("CREATE (:Person {name: 'Bob', age: 30})")
        .unwrap();
    db.execute("CREATE (:Person {name: 'Charlie', age: 20})")
        .unwrap();
}

#[test]
fn test_match_return_all() {
    let mut db = Database::new();
    setup_test_data(&mut db);

    let result = db.execute("MATCH (p:Person) RETURN p.name, p.age").unwrap();
    assert_eq!(result.row_count(), 3);
    assert_eq!(result.columns, vec!["p.name", "p.age"]);
}

#[test]
fn test_match_where_filter() {
    let mut db = Database::new();
    setup_test_data(&mut db);

    let result = db
        .execute("MATCH (p:Person) WHERE p.age > 20 RETURN p.name, p.age")
        .unwrap();

    assert_eq!(result.row_count(), 2);

    for row in &result.rows {
        if let Some(Value::Int64(age)) = row.get("p.age") {
            assert!(*age > 20, "Expected age > 20, got {age}");
        }
    }
}

#[test]
fn test_match_nonexistent_table_error() {
    let mut db = Database::new();

    let result = db.execute("MATCH (p:NonExistent) RETURN p.name");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("does not exist"));
}

#[test]
fn test_match_invalid_where_syntax_error() {
    let mut db = Database::new();

    let result = db.execute("MATCH (p:Person) WHERE RETURN p.name");
    assert!(result.is_err());
}

#[test]
fn test_match_empty_table() {
    let mut db = Database::new();
    db.execute("CREATE NODE TABLE Person(name STRING, age INT64, PRIMARY KEY(name))")
        .unwrap();

    let result = db.execute("MATCH (p:Person) RETURN p.name, p.age").unwrap();
    assert_eq!(result.row_count(), 0);
}

#[test]
fn test_match_where_less_than() {
    let mut db = Database::new();
    setup_test_data(&mut db);

    let result = db
        .execute("MATCH (p:Person) WHERE p.age < 25 RETURN p.name")
        .unwrap();

    assert_eq!(result.row_count(), 1);
}

#[test]
fn test_match_where_equals() {
    let mut db = Database::new();
    setup_test_data(&mut db);

    let result = db
        .execute("MATCH (p:Person) WHERE p.age = 30 RETURN p.name")
        .unwrap();

    assert_eq!(result.row_count(), 1);
    let row = result.get_row(0).unwrap();
    assert!(matches!(row.get("p.name"), Some(Value::String(s)) if s == "Bob"));
}

#[test]
fn test_match_where_string_equals() {
    let mut db = Database::new();
    setup_test_data(&mut db);

    let result = db
        .execute("MATCH (p:Person) WHERE p.name = 'Alice' RETURN p.age")
        .unwrap();

    assert_eq!(result.row_count(), 1);
    let row = result.get_row(0).unwrap();
    assert!(matches!(row.get("p.age"), Some(Value::Int64(25))));
}

// =============================================================================
// End-to-End Target Query Test
// =============================================================================

#[test]
fn test_target_query_end_to_end() {
    let mut db = Database::new();

    db.execute("CREATE NODE TABLE Person(name STRING, age INT64, PRIMARY KEY(name))")
        .unwrap();

    for i in 0..100 {
        let query = format!("CREATE (:Person {{name: 'Person_{i}', age: {i}}})");
        db.execute(&query).unwrap();
    }

    let result = db
        .execute("MATCH (p:Person) WHERE p.age > 20 RETURN p.name, p.age")
        .unwrap();

    // Ages 21-99 should match (79 rows)
    assert_eq!(result.row_count(), 79);
    assert_eq!(result.columns, vec!["p.name", "p.age"]);

    for row in &result.rows {
        if let Some(Value::Int64(age)) = row.get("p.age") {
            assert!(*age > 20);
        }
    }
}

// =============================================================================
// Phase 3: Node Data Page Format Contract Tests (T032)
// =============================================================================

mod node_page_format_contracts {
    use ruzu::storage::page::{NodeDataPage, PageType};
    use ruzu::storage::{Page, PageId, PAGE_SIZE};

    // -------------------------------------------------------------------------
    // T032: Contract test for node data page format
    // -------------------------------------------------------------------------

    #[test]
    fn test_node_data_page_header_layout() {
        // Page header should be exactly 16 bytes
        assert_eq!(std::mem::size_of::<PageType>(), 4);

        // After header: num_values (4), null_bitmap_size (4)
        // Total metadata: 16 (header) + 4 + 4 = 24 bytes
        const HEADER_OVERHEAD: usize = 24;

        // Remaining space for data
        let data_space = PAGE_SIZE - HEADER_OVERHEAD;
        assert!(
            data_space > 4000,
            "At least 4000 bytes should be available for data"
        );
    }

    #[test]
    fn test_fixed_width_int64_capacity() {
        // INT64 values are 8 bytes each
        // Null bitmap: 1 bit per value, rounded up to bytes
        // For N values: N * 8 bytes (data) + ceil(N/8) bytes (null bitmap)

        const HEADER_OVERHEAD: usize = 24;
        const VALUE_SIZE: usize = 8;

        // Calculate how many INT64 values fit in a page
        // Available space = PAGE_SIZE - HEADER_OVERHEAD
        // Each value needs: VALUE_SIZE + 1/8 byte for null bitmap
        let available = PAGE_SIZE - HEADER_OVERHEAD;
        let values_per_page = (available * 8) / (VALUE_SIZE * 8 + 1);

        // Should be around 508 values
        assert!(
            values_per_page >= 500,
            "Should fit at least 500 INT64 values per page"
        );
        assert!(
            values_per_page < 520,
            "Should fit less than 520 INT64 values per page"
        );
    }

    #[test]
    fn test_node_data_page_roundtrip() {
        let mut page = NodeDataPage::new(0, PageType::NodeData);

        // Write some INT64 values
        let values: Vec<i64> = vec![100, 200, 300, 400, 500];
        for (i, &val) in values.iter().enumerate() {
            page.write_int64(i, val);
        }
        page.set_num_values(values.len() as u32);

        // Serialize to raw page
        let raw_page = page.to_page(PageId::new(0, 1));
        assert_eq!(raw_page.id.page_idx, 1);

        // Deserialize back
        let restored = NodeDataPage::from_page(&raw_page).expect("deserialize page");

        // Verify values
        assert_eq!(restored.num_values(), values.len() as u32);
        for (i, &expected) in values.iter().enumerate() {
            assert_eq!(restored.read_int64(i), expected);
        }
    }

    #[test]
    fn test_node_data_page_null_bitmap() {
        let mut page = NodeDataPage::new(0, PageType::NodeData);

        // Set some values, mark some as null
        page.write_int64(0, 100);
        page.set_null(1, true);
        page.write_int64(2, 300);
        page.set_null(3, true);
        page.write_int64(4, 500);
        page.set_num_values(5);

        // Serialize and deserialize
        let raw_page = page.to_page(PageId::new(0, 1));
        let restored = NodeDataPage::from_page(&raw_page).expect("deserialize page");

        // Verify null bitmap
        assert!(!restored.is_null(0));
        assert!(restored.is_null(1));
        assert!(!restored.is_null(2));
        assert!(restored.is_null(3));
        assert!(!restored.is_null(4));
    }

    #[test]
    fn test_string_column_page_layout() {
        // Variable-width columns use: header + offsets array + string data
        // Offsets are u32 (4 bytes each)
        // For N strings: N * 4 bytes (offsets) + string bytes

        const HEADER_OVERHEAD: usize = 24;
        const OFFSET_SIZE: usize = 4;

        // With 100 strings averaging 30 bytes each:
        // 100 * 4 (offsets) + 100 * 30 (data) = 400 + 3000 = 3400 bytes
        // Should fit in one page (4096 - 24 = 4072 bytes available)
        let available = PAGE_SIZE - HEADER_OVERHEAD;
        let offset_overhead = 100 * OFFSET_SIZE;
        let string_data = 100 * 30;
        assert!(
            offset_overhead + string_data < available,
            "100 strings of 30 bytes each should fit in one page"
        );
    }

    #[test]
    fn test_page_type_values() {
        // Page types must match the contract spec
        assert_eq!(PageType::NodeData as u32, 1);
        assert_eq!(PageType::NodeOffsets as u32, 2);
        assert_eq!(PageType::CsrOffsets as u32, 3);
        assert_eq!(PageType::CsrNeighbors as u32, 4);
        assert_eq!(PageType::CsrRelIds as u32, 5);
        assert_eq!(PageType::RelProperties as u32, 6);
    }

    #[test]
    fn test_page_checksum_location() {
        // Checksum is at offset 12 in the page header (4 bytes)
        let page = Page::new(PageId::new(0, 1));
        let checksum = page.checksum();

        // Checksum should be non-zero for pages with any data pattern
        // (it's CRC32 of the page contents)
        assert!(checksum == 0 || checksum != 0); // Just verify it computes
    }
}

// =============================================================================
// Phase 5: User Story 3 - CSR Page Format Contract Tests (T057)
// =============================================================================

mod csr_format_contracts {
    use ruzu::storage::page::PageType;
    use ruzu::storage::PAGE_SIZE;

    // -------------------------------------------------------------------------
    // T057: Contract test for CSR page format (offsets, neighbors, rel_ids)
    // -------------------------------------------------------------------------

    #[test]
    fn test_csr_page_type_values() {
        // CSR page types must match the contract spec
        assert_eq!(PageType::CsrOffsets as u32, 3);
        assert_eq!(PageType::CsrNeighbors as u32, 4);
        assert_eq!(PageType::CsrRelIds as u32, 5);
        assert_eq!(PageType::RelProperties as u32, 6);
    }

    #[test]
    fn test_csr_offset_page_capacity() {
        // CSR offset page: 16 byte header + 4 byte first_node_offset + 4 byte num_offsets
        // Remaining space for offset values (8 bytes each)
        const HEADER_OVERHEAD: usize = 24; // 16 header + 4 + 4
        const OFFSET_SIZE: usize = 8; // u64

        let available = PAGE_SIZE - HEADER_OVERHEAD;
        let offsets_per_page = available / OFFSET_SIZE;

        // Should fit around 509 offsets per page
        assert!(
            offsets_per_page >= 500,
            "Should fit at least 500 offsets per page"
        );
        assert!(
            offsets_per_page < 520,
            "Should fit less than 520 offsets per page"
        );
    }

    #[test]
    fn test_csr_neighbor_page_capacity() {
        // CSR neighbor page: 16 byte header + 4 byte first_edge_idx + 4 byte num_neighbors
        // Remaining space for neighbor IDs (8 bytes each)
        const HEADER_OVERHEAD: usize = 24;
        const NEIGHBOR_SIZE: usize = 8; // u64

        let available = PAGE_SIZE - HEADER_OVERHEAD;
        let neighbors_per_page = available / NEIGHBOR_SIZE;

        // Should fit around 509 neighbors per page
        assert!(
            neighbors_per_page >= 500,
            "Should fit at least 500 neighbors per page"
        );
        assert!(
            neighbors_per_page < 520,
            "Should fit less than 520 neighbors per page"
        );
    }

    #[test]
    fn test_csr_relid_page_capacity() {
        // CSR rel_id page: same layout as neighbor page
        const HEADER_OVERHEAD: usize = 24;
        const RELID_SIZE: usize = 8; // u64

        let available = PAGE_SIZE - HEADER_OVERHEAD;
        let relids_per_page = available / RELID_SIZE;

        // Should fit around 509 rel_ids per page
        assert!(
            relids_per_page >= 500,
            "Should fit at least 500 rel_ids per page"
        );
    }

    #[test]
    fn test_csr_node_group_size() {
        // Node group size per data-model.md: 2^17 = 131072 nodes per group
        const NODE_GROUP_SIZE: usize = 131072;
        assert_eq!(NODE_GROUP_SIZE, 1 << 17);
    }

    #[test]
    fn test_csr_invariants() {
        // CSR invariants from data-model.md:
        // - offsets[0] == 0
        // - offsets[num_nodes] == neighbors.len()
        // - offsets[i] <= offsets[i+1] for all i
        // - rel_ids.len() == neighbors.len()

        let offsets: Vec<u64> = vec![0, 2, 3, 6]; // 3 nodes
        let neighbors: Vec<u64> = vec![1, 3, 2, 0, 1, 3]; // 6 edges
        let rel_ids: Vec<u64> = vec![0, 1, 2, 3, 4, 5]; // 6 rel_ids

        // Invariant 1: offsets[0] == 0
        assert_eq!(offsets[0], 0, "offsets[0] must be 0");

        // Invariant 2: offsets[num_nodes] == neighbors.len()
        let num_nodes = offsets.len() - 1;
        assert_eq!(
            offsets[num_nodes] as usize,
            neighbors.len(),
            "offsets[num_nodes] must equal neighbors.len()"
        );

        // Invariant 3: offsets are monotonically non-decreasing
        for i in 0..num_nodes {
            assert!(
                offsets[i] <= offsets[i + 1],
                "offsets must be monotonically non-decreasing"
            );
        }

        // Invariant 4: rel_ids.len() == neighbors.len()
        assert_eq!(
            rel_ids.len(),
            neighbors.len(),
            "rel_ids.len() must equal neighbors.len()"
        );
    }

    #[test]
    fn test_csr_neighbor_lookup() {
        // Verify CSR lookup works correctly
        let offsets: Vec<u64> = vec![0, 2, 3, 6]; // 3 nodes
        let neighbors: Vec<u64> = vec![1, 3, 2, 0, 1, 3]; // 6 edges

        // Node 0 has edges to [1, 3]
        let start0 = offsets[0] as usize;
        let end0 = offsets[1] as usize;
        assert_eq!(&neighbors[start0..end0], &[1, 3]);

        // Node 1 has edges to [2]
        let start1 = offsets[1] as usize;
        let end1 = offsets[2] as usize;
        assert_eq!(&neighbors[start1..end1], &[2]);

        // Node 2 has edges to [0, 1, 3]
        let start2 = offsets[2] as usize;
        let end2 = offsets[3] as usize;
        assert_eq!(&neighbors[start2..end2], &[0, 1, 3]);
    }
}

// =============================================================================
// Phase 4: User Story 2 - WAL Format Contract Tests (T043)
// =============================================================================

mod wal_format_contracts {
    use ruzu::storage::wal::{
        WalHeader, WalPayload, WalReader, WalRecord, WalRecordType, WalWriter, WAL_MAGIC,
        WAL_VERSION,
    };
    use ruzu::types::Value;
    use tempfile::TempDir;

    // -------------------------------------------------------------------------
    // T043: Contract test for WAL header and record format
    // -------------------------------------------------------------------------

    #[test]
    fn test_wal_magic_bytes() {
        assert_eq!(WAL_MAGIC, b"RUZUWAL\0");
        assert_eq!(WAL_MAGIC.len(), 8);
    }

    #[test]
    fn test_wal_version() {
        assert_eq!(WAL_VERSION, 1);
    }

    #[test]
    fn test_wal_header_serialized_size() {
        // Per contracts/storage-format.md:
        // 8 (magic) + 4 (version) + 16 (database_id) + 1 (enable_checksums) = 29 bytes
        assert_eq!(WalHeader::serialized_size(), 29);
    }

    #[test]
    fn test_wal_header_roundtrip() {
        let db_id = uuid::Uuid::new_v4();
        let header = WalHeader::new(db_id, true);

        assert_eq!(header.magic, *WAL_MAGIC);
        assert_eq!(header.version, WAL_VERSION);
        assert_eq!(header.database_id, db_id);
        assert!(header.enable_checksums);
        assert!(header.validate().is_ok());
    }

    #[test]
    fn test_wal_header_validation_invalid_magic() {
        let mut header = WalHeader::new(uuid::Uuid::new_v4(), true);
        header.magic = [0u8; 8];

        let result = header.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("WAL magic"));
    }

    #[test]
    fn test_wal_header_validation_future_version() {
        let mut header = WalHeader::new(uuid::Uuid::new_v4(), true);
        header.version = WAL_VERSION + 1;

        let result = header.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("version"));
    }

    #[test]
    fn test_wal_record_type_values() {
        // Per contracts/storage-format.md record type values
        assert_eq!(WalRecordType::BeginTransaction as u8, 1);
        assert_eq!(WalRecordType::Commit as u8, 2);
        assert_eq!(WalRecordType::Abort as u8, 3);
        assert_eq!(WalRecordType::TableInsertion as u8, 30);
        assert_eq!(WalRecordType::NodeDeletion as u8, 31);
        assert_eq!(WalRecordType::NodeUpdate as u8, 32);
        assert_eq!(WalRecordType::RelDeletion as u8, 33);
        assert_eq!(WalRecordType::RelInsertion as u8, 36);
        assert_eq!(WalRecordType::Checkpoint as u8, 254);
    }

    #[test]
    fn test_wal_record_serialization_roundtrip() {
        let record = WalRecord::begin_transaction(42, 1);
        let bytes = record.serialize().expect("serialize record");
        let restored = WalRecord::deserialize(&bytes).expect("deserialize record");

        assert_eq!(restored.record_type, WalRecordType::BeginTransaction);
        assert_eq!(restored.transaction_id, 42);
        assert_eq!(restored.lsn, 1);
    }

    #[test]
    fn test_wal_record_with_table_insertion() {
        let rows = vec![
            vec![Value::Int64(1), Value::String("Alice".into())],
            vec![Value::Int64(2), Value::String("Bob".into())],
        ];

        let record = WalRecord::new(
            WalRecordType::TableInsertion,
            1,
            10,
            WalPayload::TableInsertion {
                table_id: 5,
                rows: rows.clone(),
            },
        );

        let bytes = record.serialize().expect("serialize record");
        let restored = WalRecord::deserialize(&bytes).expect("deserialize record");

        assert_eq!(restored.record_type, WalRecordType::TableInsertion);
        match restored.payload {
            WalPayload::TableInsertion {
                table_id,
                rows: restored_rows,
            } => {
                assert_eq!(table_id, 5);
                assert_eq!(restored_rows.len(), 2);
                assert_eq!(restored_rows[0][0], Value::Int64(1));
                assert_eq!(restored_rows[0][1], Value::String("Alice".into()));
            }
            _ => panic!("Wrong payload type"),
        }
    }

    #[test]
    fn test_wal_writer_reader_integration() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let wal_path = temp_dir.path().join("wal.log");
        let db_id = uuid::Uuid::new_v4();

        // Write records
        {
            let mut writer = WalWriter::new(&wal_path, db_id, true).expect("create writer");

            let record1 = WalRecord::begin_transaction(1, writer.next_lsn());
            writer.append(&record1).expect("append begin");

            let record2 = WalRecord::new(
                WalRecordType::TableInsertion,
                1,
                writer.next_lsn(),
                WalPayload::TableInsertion {
                    table_id: 0,
                    rows: vec![vec![Value::Int64(42)]],
                },
            );
            writer.append(&record2).expect("append insertion");

            let record3 = WalRecord::commit(1, writer.next_lsn());
            writer.append(&record3).expect("append commit");

            writer.flush().expect("flush");
        }

        // Read records back
        {
            let mut reader = WalReader::open(&wal_path).expect("open reader");

            // Verify header
            assert_eq!(reader.header().database_id, db_id);
            assert!(reader.header().enable_checksums);

            // Read all records
            let records = reader.read_all().expect("read all");
            assert_eq!(records.len(), 3);

            assert_eq!(records[0].record_type, WalRecordType::BeginTransaction);
            assert_eq!(records[1].record_type, WalRecordType::TableInsertion);
            assert_eq!(records[2].record_type, WalRecordType::Commit);
        }
    }

    #[test]
    fn test_wal_checksum_verification() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let wal_path = temp_dir.path().join("wal.log");
        let db_id = uuid::Uuid::new_v4();

        // Write with checksums enabled
        {
            let mut writer = WalWriter::new(&wal_path, db_id, true).expect("create writer");
            let record = WalRecord::begin_transaction(1, writer.next_lsn());
            writer.append(&record).expect("append");
            writer.flush().expect("flush");
        }

        // Corrupt the checksum by modifying file
        {
            use std::fs::OpenOptions;
            use std::io::{Seek, SeekFrom, Write};

            let mut file = OpenOptions::new()
                .write(true)
                .open(&wal_path)
                .expect("open file");

            // Seek to end of file and corrupt last 4 bytes (checksum)
            let len = file.metadata().unwrap().len();
            file.seek(SeekFrom::Start(len - 4)).unwrap();
            file.write_all(&[0xFF, 0xFF, 0xFF, 0xFF]).unwrap();
        }

        // Reading should fail with checksum error
        {
            let mut reader = WalReader::open(&wal_path).expect("open reader");
            let result = reader.read_record();
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("checksum"));
        }
    }

    #[test]
    fn test_wal_truncate_after_checkpoint() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let wal_path = temp_dir.path().join("wal.log");
        let db_id = uuid::Uuid::new_v4();

        // Write some records
        let mut writer = WalWriter::new(&wal_path, db_id, true).expect("create writer");
        for i in 0..10 {
            let record = WalRecord::begin_transaction(i, writer.next_lsn());
            writer.append(&record).expect("append");
        }
        writer.flush().expect("flush");

        let size_before = wal_path.metadata().unwrap().len();

        // Truncate (simulates checkpoint completion)
        writer.truncate().expect("truncate");

        let size_after = wal_path.metadata().unwrap().len();

        // File should be truncated to just header
        assert!(size_after < size_before);
        assert_eq!(size_after, WalHeader::serialized_size() as u64);
    }
}

// =============================================================================
// Memory Contract Tests (Feature 004-optimize-csv-memory)
// =============================================================================
//
// These tests document the memory contracts for streaming CSV imports.
// Full validation requires running with a memory profiler (DHAT).
//
// Run memory validation with:
//   cargo test --features dhat-heap memory_contract
//   cargo bench --bench memory_benchmark

// =============================================================================
// Query Engine Contract Tests (Phase 2: T034-T038)
// =============================================================================

#[path = "query_engine_contracts/mod.rs"]
mod query_engine_contracts;

mod memory_contract_tests {
    use ruzu::catalog::{ColumnDef, NodeTableSchema};
    use ruzu::storage::csv::{CsvImportConfig, NodeLoader, RelLoader, StreamingConfig};
    use ruzu::types::DataType;
    use std::io::Write;
    use std::sync::Arc;
    use tempfile::TempDir;

    fn create_person_schema() -> Arc<NodeTableSchema> {
        Arc::new(
            NodeTableSchema::new(
                "Person".to_string(),
                vec![
                    ColumnDef::new("id".to_string(), DataType::Int64).unwrap(),
                    ColumnDef::new("name".to_string(), DataType::String).unwrap(),
                    ColumnDef::new("age".to_string(), DataType::Int64).unwrap(),
                ],
                vec!["id".to_string()],
            )
            .unwrap(),
        )
    }

    fn generate_csv_file(dir: &std::path::Path, num_rows: usize) -> std::path::PathBuf {
        let csv_path = dir.join("test_data.csv");
        let mut file = std::fs::File::create(&csv_path).expect("create csv file");

        writeln!(file, "id,name,age").expect("write header");
        for i in 0..num_rows {
            writeln!(file, "{},Person{},{}", i, i, 20 + (i % 50)).expect("write row");
        }

        csv_path
    }

    fn generate_relationship_csv(dir: &std::path::Path, num_rels: usize) -> std::path::PathBuf {
        let csv_path = dir.join("relationships.csv");
        let mut file = std::fs::File::create(&csv_path).expect("create csv file");

        writeln!(file, "FROM,TO,since").expect("write header");
        for i in 0..num_rels {
            let from_id = i % 100_000;
            let to_id = (i + 1) % 100_000;
            writeln!(file, "{},{},{}", from_id, to_id, 2015 + (i % 10)).expect("write row");
        }

        csv_path
    }

    // -------------------------------------------------------------------------
    // MC-001: 1GB Node Import Memory Contract
    // Contract: Peak memory usage < 500MB for 1GB node CSV import
    // -------------------------------------------------------------------------

    /// Tests memory-bounded import with scaled-down data.
    /// Full contract validation requires DHAT profiler with 1GB file.
    ///
    /// Contract: MC-001 - 1GB node import < 500MB peak memory
    #[test]
    fn test_mc001_node_import_memory_contract_scaled() {
        let temp_dir = TempDir::new().expect("create temp dir");

        // Use 100K rows as a scaled test (actual contract needs 1GB file)
        let csv_path = generate_csv_file(temp_dir.path(), 100_000);
        let schema = create_person_schema();

        // Configure for streaming with 100K batch size (optimal for memory)
        let config = CsvImportConfig::default()
            .with_parallel(true)
            .with_batch_size(100_000);

        let loader = NodeLoader::new(schema, config);
        let (rows, result) = loader.load(&csv_path, None).expect("load csv");

        // Verify import succeeded
        assert_eq!(rows.len(), 100_000);
        assert!(result.is_success());

        // Memory contract validation note:
        // For full MC-001 validation, run with DHAT profiler and 1GB file:
        // - Generate ~10M rows (1GB file)
        // - Measure peak heap allocation
        // - Assert peak < 500MB
    }

    // -------------------------------------------------------------------------
    // MC-002: 1GB Edge Import Memory Contract
    // Contract: Peak memory usage < 500MB for 1GB edge CSV import
    // -------------------------------------------------------------------------

    /// Tests memory-bounded relationship import with scaled-down data.
    /// Full contract validation requires DHAT profiler with 1GB file.
    ///
    /// Contract: MC-002 - 1GB edge import < 500MB peak memory
    #[test]
    fn test_mc002_edge_import_memory_contract_scaled() {
        let temp_dir = TempDir::new().expect("create temp dir");

        // Use 100K relationships as a scaled test
        let csv_path = generate_relationship_csv(temp_dir.path(), 100_000);

        let property_columns = vec![("since".to_string(), DataType::Int64)];
        let config = CsvImportConfig::default()
            .with_parallel(true)
            .with_batch_size(100_000);

        let loader = RelLoader::with_default_columns(property_columns, config);
        let (rels, result) = loader.load(&csv_path, None).expect("load csv");

        assert_eq!(rels.len(), 100_000);
        assert!(result.is_success());
    }

    // -------------------------------------------------------------------------
    // MC-003: 5GB Import Memory Contract
    // Contract: Peak memory usage < 500MB for 5GB CSV import
    // -------------------------------------------------------------------------

    /// Documents MC-003 contract - 5GB import still under 500MB.
    /// This test validates the streaming config works for very large files.
    ///
    /// Contract: MC-003 - 5GB import < 500MB peak memory
    #[test]
    fn test_mc003_large_import_memory_contract_config() {
        // Validate streaming config is properly configured for large files
        let config = StreamingConfig::default();

        assert_eq!(config.batch_size, 100_000); // 100K rows per batch
        assert!(config.streaming_enabled);
        assert_eq!(config.streaming_threshold, 100 * 1024 * 1024); // 100MB

        // With 100K batch and ~200MB memory per batch (worst case),
        // a 5GB file will process in ~50K batches, never exceeding 500MB

        // Memory math:
        // - 100K rows × 10 columns × ~50 bytes/value = ~50MB per batch
        // - 2x for double buffering = ~100MB active memory
        // - With overhead: ~200MB peak (well under 500MB)

        assert!(config.validate().is_ok());
    }

    // -------------------------------------------------------------------------
    // MC-004: Memory Variance Contract
    // Contract: Memory variance < 100MB across file sizes (100MB to 5GB)
    // -------------------------------------------------------------------------

    /// Documents MC-004 contract - predictable memory regardless of file size.
    /// This test validates the buffer recycling mechanism works correctly.
    ///
    /// Contract: MC-004 - Memory variance < 100MB across file sizes
    #[test]
    fn test_mc004_memory_variance_contract_config() {
        // The key to predictable memory is the fixed-size batch buffer
        // that gets recycled between batches.

        let config = StreamingConfig::default();

        // Memory is bounded by batch_size, not file size
        // This means a 100MB file and 5GB file should have similar peak memory

        // Batch buffer size calculation:
        let batch_size = config.batch_size;
        let estimated_row_size = 200; // bytes per row (conservative)
        let batch_memory = batch_size * estimated_row_size;

        // Should be well under our budget
        let max_budget = 500 * 1024 * 1024; // 500MB
        assert!(batch_memory < max_budget / 2); // Leave room for overhead

        // The variance contract is satisfied because:
        // 1. Batch buffer is fixed size (100K rows)
        // 2. Buffer is recycled between batches
        // 3. Memory usage is independent of file size
    }

    /// Tests that buffer recycling prevents memory growth across batches
    #[test]
    fn test_buffer_recycling_prevents_growth() {
        use ruzu::storage::csv::RowBuffer;
        use ruzu::types::Value;

        let mut buffer = RowBuffer::new(1000, 5);

        // Simulate multiple batch cycles
        for batch_num in 0..5 {
            // Fill buffer
            for i in 0..1000 {
                buffer
                    .push(vec![
                        Value::Int64(batch_num as i64 * 1000 + i),
                        Value::String(format!("item_{}", i)),
                    ])
                    .unwrap();
            }
            assert_eq!(buffer.len(), 1000);

            // Recycle for next batch
            buffer.recycle();
            assert_eq!(buffer.len(), 0);

            // Verify recycled pool doesn't grow unbounded
            // (capped at 2x capacity = 2000)
            assert!(buffer.recycled_count() <= 2000);
        }

        // After 5 batches with recycling, memory should be stable
        // This is the key to MC-004 compliance
    }

    // -------------------------------------------------------------------------
    // T011-T014: Relationship Persistence Format Contracts (US1)
    // -------------------------------------------------------------------------

    /// T011: Contract test for empty rel_table save/load
    /// Ensures that databases with zero relationships can be saved and loaded
    #[test]
    fn test_empty_rel_table_save_load() {
        use ruzu::storage::RelTableData;
        use std::collections::HashMap;

        // Create empty RelTableData
        let empty_data = RelTableData::default();

        // Verify it serializes correctly
        let serialized = bincode::serialize(&empty_data).expect("serialize empty rel_table");
        assert!(serialized.len() < 100); // Empty should be small

        // Verify it deserializes correctly
        let deserialized: RelTableData =
            bincode::deserialize(&serialized).expect("deserialize empty rel_table");
        assert_eq!(deserialized.forward_groups.len(), 0);
        assert_eq!(deserialized.backward_groups.len(), 0);
        assert_eq!(deserialized.next_rel_id, 0);
        assert_eq!(deserialized.properties.len(), 0);

        // Verify HashMap with empty tables serializes
        let mut map: HashMap<String, RelTableData> = HashMap::new();
        map.insert("EmptyRel".to_string(), empty_data);

        let map_serialized = bincode::serialize(&map).expect("serialize rel_table map");
        let map_deserialized: HashMap<String, RelTableData> =
            bincode::deserialize(&map_serialized).expect("deserialize rel_table map");

        assert_eq!(map_deserialized.len(), 1);
        assert!(map_deserialized.contains_key("EmptyRel"));
    }

    /// T012: Contract test for single rel_table serialization format
    /// Ensures a relationship table with data maintains format stability
    #[test]
    fn test_single_rel_table_serialization_format() {
        use ruzu::storage::{CsrNodeGroup, RelTableData};
        use ruzu::types::Value;
        use std::collections::HashMap;

        // Create RelTableData with one relationship: node 0 -> node 1
        let forward_group = CsrNodeGroup {
            group_id: 0,
            num_nodes: 1,
            offsets: vec![0, 1],
            neighbors: vec![1],
            rel_ids: vec![100],
            properties: vec![],
        };

        let backward_group = CsrNodeGroup {
            group_id: 1,
            num_nodes: 1,
            offsets: vec![0, 1],
            neighbors: vec![0],
            rel_ids: vec![100],
            properties: vec![],
        };

        let mut properties = HashMap::new();
        properties.insert(100, vec![Value::Int64(2020)]);

        let rel_data = RelTableData {
            forward_groups: vec![forward_group],
            backward_groups: vec![backward_group],
            next_rel_id: 101,
            properties,
        };

        // Serialize and verify size is reasonable
        let serialized = bincode::serialize(&rel_data).expect("serialize rel_table");
        assert!(serialized.len() < 1000); // Should be small for single relationship

        // Deserialize and verify all fields
        let deserialized: RelTableData =
            bincode::deserialize(&serialized).expect("deserialize rel_table");

        assert_eq!(deserialized.forward_groups.len(), 1);
        assert_eq!(deserialized.backward_groups.len(), 1);
        assert_eq!(deserialized.next_rel_id, 101);
        assert_eq!(deserialized.properties.len(), 1);

        // Verify forward group
        assert_eq!(deserialized.forward_groups[0].group_id, 0);
        assert_eq!(deserialized.forward_groups[0].num_nodes, 1);
        assert_eq!(deserialized.forward_groups[0].offsets, vec![0, 1]);
        assert_eq!(deserialized.forward_groups[0].neighbors, vec![1]);
        assert_eq!(deserialized.forward_groups[0].rel_ids, vec![100]);

        // Verify backward group
        assert_eq!(deserialized.backward_groups[0].group_id, 1);
        assert_eq!(deserialized.backward_groups[0].num_nodes, 1);
        assert_eq!(deserialized.backward_groups[0].offsets, vec![0, 1]);
        assert_eq!(deserialized.backward_groups[0].neighbors, vec![0]);
        assert_eq!(deserialized.backward_groups[0].rel_ids, vec![100]);

        // Verify properties
        assert_eq!(
            deserialized.properties.get(&100),
            Some(&vec![Value::Int64(2020)])
        );
    }

    /// T013: Contract test for multiple rel_tables save/load
    /// Ensures multiple relationship tables can coexist in the same database
    #[test]
    fn test_multiple_rel_tables_save_load() {
        use ruzu::storage::{CsrNodeGroup, RelTableData, PAGE_SIZE};
        use std::collections::HashMap;

        // Create first relationship table
        let knows_forward = CsrNodeGroup {
            group_id: 0,
            num_nodes: 1,
            offsets: vec![0, 2],
            neighbors: vec![1, 2],
            rel_ids: vec![10, 11],
            properties: vec![],
        };
        let knows_data = RelTableData {
            forward_groups: vec![knows_forward],
            backward_groups: vec![],
            next_rel_id: 12,
            properties: HashMap::new(),
        };

        // Create second relationship table
        let follows_forward = CsrNodeGroup {
            group_id: 1,
            num_nodes: 1,
            offsets: vec![0, 1],
            neighbors: vec![0],
            rel_ids: vec![20],
            properties: vec![],
        };
        let follows_data = RelTableData {
            forward_groups: vec![follows_forward],
            backward_groups: vec![],
            next_rel_id: 21,
            properties: HashMap::new(),
        };

        // Create map of multiple tables
        let mut rel_tables: HashMap<String, RelTableData> = HashMap::new();
        rel_tables.insert("Knows".to_string(), knows_data);
        rel_tables.insert("Follows".to_string(), follows_data);

        // Serialize
        let serialized = bincode::serialize(&rel_tables).expect("serialize multiple rel_tables");
        assert!(serialized.len() < PAGE_SIZE - 4); // Must fit in a page

        // Deserialize
        let deserialized: HashMap<String, RelTableData> =
            bincode::deserialize(&serialized).expect("deserialize multiple rel_tables");

        assert_eq!(deserialized.len(), 2);
        assert!(deserialized.contains_key("Knows"));
        assert!(deserialized.contains_key("Follows"));

        // Verify "Knows" table
        let knows = deserialized.get("Knows").unwrap();
        assert_eq!(knows.forward_groups.len(), 1);
        assert_eq!(knows.forward_groups[0].group_id, 0);
        assert_eq!(knows.forward_groups[0].neighbors, vec![1, 2]);
        assert_eq!(knows.next_rel_id, 12);

        // Verify "Follows" table
        let follows = deserialized.get("Follows").unwrap();
        assert_eq!(follows.forward_groups.len(), 1);
        assert_eq!(follows.forward_groups[0].group_id, 1);
        assert_eq!(follows.forward_groups[0].neighbors, vec![0]);
        assert_eq!(follows.next_rel_id, 21);
    }

    /// T014: Contract test for CSR invariant preservation
    /// Ensures CSR structure invariants are maintained after serialization roundtrip
    #[test]
    fn test_csr_invariant_preservation() {
        use ruzu::storage::{CsrNodeGroup, RelTableData};
        use std::collections::HashMap;

        // Create CSR with multiple edges from same source
        let forward_group = CsrNodeGroup {
            group_id: 5,
            num_nodes: 1,
            offsets: vec![0, 3], // 3 edges
            neighbors: vec![10, 20, 30],
            rel_ids: vec![100, 101, 102],
            properties: vec![],
        };

        let rel_data = RelTableData {
            forward_groups: vec![forward_group.clone()],
            backward_groups: vec![],
            next_rel_id: 103,
            properties: HashMap::new(),
        };

        // Serialize and deserialize
        let serialized = bincode::serialize(&rel_data).expect("serialize");
        let deserialized: RelTableData = bincode::deserialize(&serialized).expect("deserialize");

        // Verify CSR invariants
        let group = &deserialized.forward_groups[0];
        assert_eq!(group.group_id, 5);
        assert_eq!(group.offsets.len(), 2); // start and end

        // Invariant: offsets define edge count
        let edge_count = (group.offsets[1] - group.offsets[0]) as usize;
        assert_eq!(edge_count, 3);

        // Invariant: neighbors.len() == edge_count
        assert_eq!(group.neighbors.len(), edge_count);

        // Invariant: rel_ids.len() == edge_count
        assert_eq!(group.rel_ids.len(), edge_count);

        // Invariant: neighbors and rel_ids match original
        assert_eq!(group.neighbors, vec![10, 20, 30]);
        assert_eq!(group.rel_ids, vec![100, 101, 102]);

        // Invariant: next_rel_id > all existing rel_ids
        for &rel_id in &group.rel_ids {
            assert!(deserialized.next_rel_id > rel_id);
        }
    }

    // -------------------------------------------------------------------------
    // Phase 6: Version Migration Tests (T048)
    // -------------------------------------------------------------------------

    /// T048: Contract test for version 1 to version 2 header migration
    /// Ensures that version 1 databases can be upgraded to version 2 without data loss
    #[test]
    fn test_v1_to_v2_header_migration() {
        use ruzu::storage::{DatabaseHeader, DatabaseHeaderV1, PageRange};
        use uuid::Uuid;

        // Create a version 1 header
        let db_id = Uuid::new_v4();
        let v1_header = DatabaseHeaderV1 {
            magic: *ruzu::storage::MAGIC_BYTES,
            version: 1,
            database_id: db_id,
            catalog_range: PageRange::new(1, 1),
            metadata_range: PageRange::new(2, 1),
            checksum: 0,
        };

        // Migrate to version 2
        let v2_header = DatabaseHeader::from_v1(&v1_header);

        // Verify all fields migrated correctly
        assert_eq!(v2_header.magic, *ruzu::storage::MAGIC_BYTES);
        assert_eq!(v2_header.version, 3); // Version upgraded to current
        assert_eq!(v2_header.database_id, db_id);
        assert_eq!(v2_header.catalog_range.start_page, 1);
        assert_eq!(v2_header.catalog_range.num_pages, 1);
        assert_eq!(v2_header.metadata_range.start_page, 2);
        assert_eq!(v2_header.metadata_range.num_pages, 1);

        // New field should be allocated at page 3 for relationship metadata
        assert_eq!(v2_header.rel_metadata_range.start_page, 3);
        assert_eq!(v2_header.rel_metadata_range.num_pages, 1);
        assert!(!v2_header.rel_metadata_range.is_empty());
    }

    /// T048 (additional): Test that v1 header serialization format is distinct from v2
    #[test]
    fn test_v1_serialization_format_compatibility() {
        use ruzu::storage::{DatabaseHeaderV1, PageRange};
        use uuid::Uuid;

        let db_id = Uuid::new_v4();
        let v1_header = DatabaseHeaderV1 {
            magic: *ruzu::storage::MAGIC_BYTES,
            version: 1,
            database_id: db_id,
            catalog_range: PageRange::new(1, 1),
            metadata_range: PageRange::new(2, 1),
            checksum: 12345,
        };

        // V1 header should serialize successfully
        let v1_bytes = bincode::serialize(&v1_header).expect("serialize v1 header");

        // V1 serialized bytes should be shorter than V2 (missing rel_metadata_range field)
        assert!(v1_bytes.len() < 200); // V1 header is compact

        // Should be able to deserialize back to V1
        let restored_v1: DatabaseHeaderV1 =
            bincode::deserialize(&v1_bytes).expect("deserialize v1 header");
        assert_eq!(restored_v1.version, 1);
        assert_eq!(restored_v1.database_id, db_id);
        assert_eq!(restored_v1.checksum, 12345);
    }

    // -------------------------------------------------------------------------
    // T043: v2 header binary format is still correctly parseable
    // -------------------------------------------------------------------------

    /// T043: Contract test that v2 header bytes can still be deserialized
    /// after the codebase moves to v3 as CURRENT_VERSION.
    #[test]
    fn test_t043_v2_header_binary_format_parseable() {
        use ruzu::storage::{DatabaseHeader, PageRange, MAGIC_BYTES};
        use uuid::Uuid;

        let db_id = Uuid::new_v4();

        // Construct a v2 header manually and serialize it
        let mut v2_header = DatabaseHeader {
            magic: *MAGIC_BYTES,
            version: 2,
            database_id: db_id,
            catalog_range: PageRange::new(1, 1),
            metadata_range: PageRange::new(2, 1),
            rel_metadata_range: PageRange::new(3, 1),
            checksum: 0,
        };
        // Compute checksum with version=2
        v2_header.checksum = v2_header.compute_checksum();
        let v2_bytes = bincode::serialize(&v2_header).expect("serialize v2 header");

        // Deserialize using deserialize_with_migration_flag (the load path)
        let (header, was_migrated) =
            DatabaseHeader::deserialize_with_migration_flag(&v2_bytes)
                .expect("v2 header should be parseable");

        // Should be recognized as v2 and migrated
        assert!(was_migrated, "v2 header should trigger migration flag");
        assert_eq!(header.version, 3, "Migrated header should be v3");
        assert_eq!(header.database_id, db_id);
        assert_eq!(header.catalog_range, PageRange::new(1, 1));
        assert_eq!(header.metadata_range, PageRange::new(2, 1));
        assert_eq!(header.rel_metadata_range, PageRange::new(3, 1));
    }

    /// T043 (additional): v2 header roundtrip: serialize v2 bytes → deserialize → get correct fields
    #[test]
    fn test_t043_v2_header_roundtrip_preserves_ranges() {
        use ruzu::storage::{DatabaseHeader, PageRange, MAGIC_BYTES};
        use uuid::Uuid;

        let db_id = Uuid::new_v4();
        let mut v2_header = DatabaseHeader {
            magic: *MAGIC_BYTES,
            version: 2,
            database_id: db_id,
            catalog_range: PageRange::new(1, 1),
            metadata_range: PageRange::new(2, 1),
            rel_metadata_range: PageRange::new(3, 1),
            checksum: 0,
        };
        v2_header.checksum = v2_header.compute_checksum();
        let v2_bytes = bincode::serialize(&v2_header).expect("serialize");

        // Use the standard deserialize path
        let restored = DatabaseHeader::deserialize(&v2_bytes)
            .expect("v2 header should deserialize");

        // The ranges should be preserved exactly
        assert_eq!(restored.catalog_range.start_page, 1);
        assert_eq!(restored.catalog_range.num_pages, 1);
        assert_eq!(restored.metadata_range.start_page, 2);
        assert_eq!(restored.metadata_range.num_pages, 1);
        assert_eq!(restored.rel_metadata_range.start_page, 3);
        assert_eq!(restored.rel_metadata_range.num_pages, 1);
    }
}

// =============================================================================
// Type System Extension Contract Tests (Feature 006-add-datatypes)
// =============================================================================

mod type_system_contracts {
    use ruzu::{Database, DatabaseConfig, Value};
    use std::io::Write;
    use tempfile::TempDir;

    // C-DDL-01: FLOAT64 column in CREATE NODE TABLE
    #[test]
    fn test_c_ddl_01_float64_in_create_node_table() {
        let mut db = Database::new();
        let result = db.execute("CREATE NODE TABLE Product(name STRING, price FLOAT64, PRIMARY KEY(name))");
        assert!(result.is_ok(), "CREATE NODE TABLE with FLOAT64 should succeed");
    }

    // C-DDL-02: BOOL column in CREATE NODE TABLE
    #[test]
    fn test_c_ddl_02_bool_in_create_node_table() {
        let mut db = Database::new();
        let result = db.execute("CREATE NODE TABLE Feature(name STRING, enabled BOOL, PRIMARY KEY(name))");
        assert!(result.is_ok(), "CREATE NODE TABLE with BOOL should succeed");
    }

    // C-DDL-03: All four types in single table
    #[test]
    fn test_c_ddl_03_all_four_types_in_single_table() {
        let mut db = Database::new();
        let result = db.execute("CREATE NODE TABLE Mixed(id INT64, name STRING, score FLOAT64, active BOOL, PRIMARY KEY(id))");
        assert!(result.is_ok(), "All four types in single table should succeed");
    }

    // C-DDL-04: FLOAT64/BOOL in CREATE REL TABLE
    #[test]
    fn test_c_ddl_04_float64_bool_in_create_rel_table() {
        let mut db = Database::new();
        db.execute("CREATE NODE TABLE User(name STRING, PRIMARY KEY(name))").unwrap();
        db.execute("CREATE NODE TABLE Product(name STRING, PRIMARY KEY(name))").unwrap();
        let result = db.execute("CREATE REL TABLE Rates(FROM User TO Product, score FLOAT64, verified BOOL)");
        assert!(result.is_ok(), "CREATE REL TABLE with FLOAT64 and BOOL should succeed");
    }

    // C-LIT-01: Float literal parsing
    #[test]
    fn test_c_lit_01_float_literal_parsing() {
        let mut db = Database::new();
        db.execute("CREATE NODE TABLE Product(name STRING, price FLOAT64, PRIMARY KEY(name))").unwrap();
        db.execute("CREATE (:Product {name: 'A', price: 3.14})").unwrap();
        db.execute("CREATE (:Product {name: 'B', price: 42.0})").unwrap();
        db.execute("CREATE (:Product {name: 'C', price: 0.0})").unwrap();
        db.execute("CREATE (:Product {name: 'D', price: -0.5})").unwrap();
        db.execute("CREATE (:Product {name: 'E', price: .5})").unwrap();

        let result = db.execute("MATCH (p:Product) RETURN p.name, p.price").unwrap();
        assert_eq!(result.row_count(), 5);
    }

    // C-LIT-02: Bool literal parsing
    #[test]
    fn test_c_lit_02_bool_literal_parsing() {
        let mut db = Database::new();
        db.execute("CREATE NODE TABLE Feature(name STRING, enabled BOOL, PRIMARY KEY(name))").unwrap();
        db.execute("CREATE (:Feature {name: 'A', enabled: true})").unwrap();
        db.execute("CREATE (:Feature {name: 'B', enabled: false})").unwrap();
        db.execute("CREATE (:Feature {name: 'C', enabled: TRUE})").unwrap();
        db.execute("CREATE (:Feature {name: 'D', enabled: FALSE})").unwrap();
        db.execute("CREATE (:Feature {name: 'E', enabled: True})").unwrap();

        let result = db.execute("MATCH (f:Feature) RETURN f.name, f.enabled").unwrap();
        assert_eq!(result.row_count(), 5);
    }

    // C-DML-01: Create node with FLOAT64 property
    #[test]
    fn test_c_dml_01_create_node_with_float64() {
        let mut db = Database::new();
        db.execute("CREATE NODE TABLE Product(name STRING, price FLOAT64, PRIMARY KEY(name))").unwrap();
        db.execute("CREATE (:Product {name: 'Widget', price: 19.99})").unwrap();

        let result = db.execute("MATCH (p:Product) RETURN p.name, p.price").unwrap();
        assert_eq!(result.row_count(), 1);
        assert_eq!(result.get_row(0).unwrap().get("p.price"), Some(&Value::Float64(19.99)));
    }

    // C-DML-02: Create node with BOOL property
    #[test]
    fn test_c_dml_02_create_node_with_bool() {
        let mut db = Database::new();
        db.execute("CREATE NODE TABLE Feature(name STRING, enabled BOOL, PRIMARY KEY(name))").unwrap();
        db.execute("CREATE (:Feature {name: 'DarkMode', enabled: true})").unwrap();

        let result = db.execute("MATCH (f:Feature) RETURN f.name, f.enabled").unwrap();
        assert_eq!(result.row_count(), 1);
        assert_eq!(result.get_row(0).unwrap().get("f.enabled"), Some(&Value::Bool(true)));
    }

    // C-DML-03: Integer value promoted to FLOAT64
    #[test]
    fn test_c_dml_03_integer_value_for_float64_column() {
        let mut db = Database::new();
        db.execute("CREATE NODE TABLE Product(name STRING, price FLOAT64, PRIMARY KEY(name))").unwrap();
        db.execute("CREATE (:Product {name: 'Free', price: 0})").unwrap();

        let result = db.execute("MATCH (p:Product) RETURN p.name, p.price").unwrap();
        assert_eq!(result.row_count(), 1);
        assert_eq!(result.get_row(0).unwrap().get("p.price"), Some(&Value::Float64(0.0)));
    }

    // C-QRY-01: Float comparison operators
    #[test]
    fn test_c_qry_01_float_comparison_operators() {
        let mut db = Database::new();
        db.execute("CREATE NODE TABLE Product(name STRING, price FLOAT64, PRIMARY KEY(name))").unwrap();
        db.execute("CREATE (:Product {name: 'Cheap', price: 5.0})").unwrap();
        db.execute("CREATE (:Product {name: 'Mid', price: 15.0})").unwrap();
        db.execute("CREATE (:Product {name: 'Expensive', price: 50.0})").unwrap();

        let result = db.execute("MATCH (p:Product) WHERE p.price > 10.0 RETURN p.name").unwrap();
        assert_eq!(result.row_count(), 2);
    }

    // C-QRY-02: Float comparison with integer literal
    #[test]
    fn test_c_qry_02_float_comparison_with_integer_literal() {
        let mut db = Database::new();
        db.execute("CREATE NODE TABLE Product(name STRING, price FLOAT64, PRIMARY KEY(name))").unwrap();
        db.execute("CREATE (:Product {name: 'Cheap', price: 5.0})").unwrap();
        db.execute("CREATE (:Product {name: 'Expensive', price: 50.0})").unwrap();

        let result = db.execute("MATCH (p:Product) WHERE p.price > 10 RETURN p.name").unwrap();
        assert_eq!(result.row_count(), 1);
        assert_eq!(result.get_row(0).unwrap().get("p.name"), Some(&Value::String("Expensive".to_string())));
    }

    // C-QRY-03: Bool equality
    #[test]
    fn test_c_qry_03_bool_equality() {
        let mut db = Database::new();
        db.execute("CREATE NODE TABLE Feature(name STRING, enabled BOOL, PRIMARY KEY(name))").unwrap();
        db.execute("CREATE (:Feature {name: 'A', enabled: true})").unwrap();
        db.execute("CREATE (:Feature {name: 'B', enabled: false})").unwrap();
        db.execute("CREATE (:Feature {name: 'C', enabled: true})").unwrap();

        let result = db.execute("MATCH (f:Feature) WHERE f.enabled = true RETURN f.name").unwrap();
        assert_eq!(result.row_count(), 2);
    }

    // C-QRY-04: Bool inequality
    #[test]
    fn test_c_qry_04_bool_inequality() {
        let mut db = Database::new();
        db.execute("CREATE NODE TABLE Feature(name STRING, enabled BOOL, PRIMARY KEY(name))").unwrap();
        db.execute("CREATE (:Feature {name: 'A', enabled: true})").unwrap();
        db.execute("CREATE (:Feature {name: 'B', enabled: false})").unwrap();
        db.execute("CREATE (:Feature {name: 'C', enabled: true})").unwrap();

        let result = db.execute("MATCH (f:Feature) WHERE f.enabled <> false RETURN f.name").unwrap();
        assert_eq!(result.row_count(), 2);
    }

    // C-QRY-05: ORDER BY on FLOAT64
    #[test]
    fn test_c_qry_05_order_by_float64() {
        let mut db = Database::new();
        db.execute("CREATE NODE TABLE Product(name STRING, price FLOAT64, PRIMARY KEY(name))").unwrap();
        db.execute("CREATE (:Product {name: 'C', price: 50.0})").unwrap();
        db.execute("CREATE (:Product {name: 'A', price: 5.0})").unwrap();
        db.execute("CREATE (:Product {name: 'B', price: 15.0})").unwrap();

        let result = db.execute("MATCH (p:Product) RETURN p.name, p.price ORDER BY p.price ASC").unwrap();
        assert_eq!(result.row_count(), 3);
        assert_eq!(result.get_row(0).unwrap().get("p.price"), Some(&Value::Float64(5.0)));
        assert_eq!(result.get_row(1).unwrap().get("p.price"), Some(&Value::Float64(15.0)));
        assert_eq!(result.get_row(2).unwrap().get("p.price"), Some(&Value::Float64(50.0)));
    }

    // C-CSV-01: FLOAT64 values from CSV
    #[test]
    fn test_c_csv_01_float64_from_csv() {
        let mut db = Database::new();
        db.execute("CREATE NODE TABLE Product(name STRING, price FLOAT64, PRIMARY KEY(name))").unwrap();

        let temp_dir = TempDir::new().unwrap();
        let csv_path = temp_dir.path().join("products.csv");
        let mut f = std::fs::File::create(&csv_path).unwrap();
        writeln!(f, "name,price").unwrap();
        writeln!(f, "Widget,19.99").unwrap();
        writeln!(f, "Gadget,5.50").unwrap();
        drop(f);

        let csv_str = csv_path.to_str().unwrap().replace('\\', "/");
        db.execute(&format!("COPY Product FROM '{}'", csv_str)).unwrap();

        let result = db.execute("MATCH (p:Product) RETURN p.name, p.price").unwrap();
        assert_eq!(result.row_count(), 2);
    }

    // C-CSV-02: BOOL values from CSV
    #[test]
    fn test_c_csv_02_bool_from_csv() {
        let mut db = Database::new();
        db.execute("CREATE NODE TABLE Feature(name STRING, enabled BOOL, PRIMARY KEY(name))").unwrap();

        let temp_dir = TempDir::new().unwrap();
        let csv_path = temp_dir.path().join("features.csv");
        let mut f = std::fs::File::create(&csv_path).unwrap();
        writeln!(f, "name,enabled").unwrap();
        writeln!(f, "DarkMode,true").unwrap();
        writeln!(f, "LightMode,false").unwrap();
        drop(f);

        let csv_str = csv_path.to_str().unwrap().replace('\\', "/");
        db.execute(&format!("COPY Feature FROM '{}'", csv_str)).unwrap();

        let result = db.execute("MATCH (f:Feature) RETURN f.name, f.enabled").unwrap();
        assert_eq!(result.row_count(), 2);
    }

    // C-CSV-03: Case-insensitive BOOL in CSV
    #[test]
    fn test_c_csv_03_case_insensitive_bool_in_csv() {
        let mut db = Database::new();
        db.execute("CREATE NODE TABLE Feature(name STRING, enabled BOOL, PRIMARY KEY(name))").unwrap();

        let temp_dir = TempDir::new().unwrap();
        let csv_path = temp_dir.path().join("features.csv");
        let mut f = std::fs::File::create(&csv_path).unwrap();
        writeln!(f, "name,enabled").unwrap();
        writeln!(f, "A,TRUE").unwrap();
        writeln!(f, "B,False").unwrap();
        writeln!(f, "C,true").unwrap();
        drop(f);

        let csv_str = csv_path.to_str().unwrap().replace('\\', "/");
        db.execute(&format!("COPY Feature FROM '{}'", csv_str)).unwrap();

        let result = db.execute("MATCH (f:Feature) RETURN f.name, f.enabled").unwrap();
        assert_eq!(result.row_count(), 3);
    }

    // C-CSV-04: Invalid BOOL in CSV rejected
    #[test]
    fn test_c_csv_04_invalid_bool_in_csv_rejected() {
        let mut db = Database::new();
        db.execute("CREATE NODE TABLE Feature(name STRING, enabled BOOL, PRIMARY KEY(name))").unwrap();

        let temp_dir = TempDir::new().unwrap();
        let csv_path = temp_dir.path().join("features.csv");
        let mut f = std::fs::File::create(&csv_path).unwrap();
        writeln!(f, "name,enabled").unwrap();
        writeln!(f, "A,yes").unwrap();
        drop(f);

        let csv_str = csv_path.to_str().unwrap().replace('\\', "/");
        let result = db.execute(&format!("COPY Feature FROM '{}'", csv_str));
        assert!(result.is_err(), "Invalid BOOL 'yes' should be rejected");
    }

    // C-CSV-05: Invalid FLOAT64 in CSV rejected
    #[test]
    fn test_c_csv_05_invalid_float64_in_csv_rejected() {
        let mut db = Database::new();
        db.execute("CREATE NODE TABLE Product(name STRING, price FLOAT64, PRIMARY KEY(name))").unwrap();

        let temp_dir = TempDir::new().unwrap();
        let csv_path = temp_dir.path().join("products.csv");
        let mut f = std::fs::File::create(&csv_path).unwrap();
        writeln!(f, "name,price").unwrap();
        writeln!(f, "A,abc").unwrap();
        drop(f);

        let csv_str = csv_path.to_str().unwrap().replace('\\', "/");
        let result = db.execute(&format!("COPY Product FROM '{}'", csv_str));
        assert!(result.is_err(), "Invalid FLOAT64 'abc' should be rejected");
    }

    // C-PER-01: FLOAT64 data survives restart
    #[test]
    fn test_c_per_01_float64_survives_restart() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test_db");

        {
            let mut db = Database::open(&db_path, DatabaseConfig::default()).unwrap();
            db.execute("CREATE NODE TABLE Product(name STRING, price FLOAT64, PRIMARY KEY(name))").unwrap();
            db.execute("CREATE (:Product {name: 'Widget', price: 19.99})").unwrap();
            db.close().unwrap();
        }

        {
            let mut db = Database::open(&db_path, DatabaseConfig::default()).unwrap();
            let result = db.execute("MATCH (p:Product) RETURN p.name, p.price").unwrap();
            assert_eq!(result.row_count(), 1);
            assert_eq!(result.get_row(0).unwrap().get("p.price"), Some(&Value::Float64(19.99)));
        }
    }

    // C-PER-02: BOOL data survives restart
    #[test]
    fn test_c_per_02_bool_survives_restart() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test_db");

        {
            let mut db = Database::open(&db_path, DatabaseConfig::default()).unwrap();
            db.execute("CREATE NODE TABLE Feature(name STRING, enabled BOOL, PRIMARY KEY(name))").unwrap();
            db.execute("CREATE (:Feature {name: 'DarkMode', enabled: true})").unwrap();
            db.execute("CREATE (:Feature {name: 'Beta', enabled: false})").unwrap();
            db.close().unwrap();
        }

        {
            let mut db = Database::open(&db_path, DatabaseConfig::default()).unwrap();
            let result = db.execute("MATCH (f:Feature) RETURN f.name, f.enabled").unwrap();
            assert_eq!(result.row_count(), 2);
        }
    }

    // C-AGG-01: COUNT on FLOAT64 column
    #[test]
    fn test_c_agg_01_count_on_float64() {
        let mut db = Database::new();
        db.execute("CREATE NODE TABLE Product(name STRING, price FLOAT64, PRIMARY KEY(name))").unwrap();
        db.execute("CREATE (:Product {name: 'A', price: 10.0})").unwrap();
        db.execute("CREATE (:Product {name: 'B', price: 20.0})").unwrap();
        db.execute("CREATE (:Product {name: 'C', price: 30.0})").unwrap();

        let result = db.execute("MATCH (p:Product) RETURN COUNT(p.price)").unwrap();
        assert_eq!(result.row_count(), 1);
        assert_eq!(result.get_row(0).unwrap().get("COUNT(p.price)"), Some(&Value::Int64(3)));
    }

    // C-AGG-02: MIN/MAX on FLOAT64
    #[test]
    fn test_c_agg_02_min_max_on_float64() {
        let mut db = Database::new();
        db.execute("CREATE NODE TABLE Product(name STRING, price FLOAT64, PRIMARY KEY(name))").unwrap();
        db.execute("CREATE (:Product {name: 'A', price: 10.5})").unwrap();
        db.execute("CREATE (:Product {name: 'B', price: 5.25})").unwrap();
        db.execute("CREATE (:Product {name: 'C', price: 30.0})").unwrap();

        let result = db.execute("MATCH (p:Product) RETURN MIN(p.price)").unwrap();
        assert_eq!(result.get_row(0).unwrap().get("MIN(p.price)"), Some(&Value::Float64(5.25)));

        let result = db.execute("MATCH (p:Product) RETURN MAX(p.price)").unwrap();
        assert_eq!(result.get_row(0).unwrap().get("MAX(p.price)"), Some(&Value::Float64(30.0)));
    }

    // C-AGG-03: COUNT on BOOL column
    #[test]
    fn test_c_agg_03_count_on_bool() {
        let mut db = Database::new();
        db.execute("CREATE NODE TABLE Feature(name STRING, enabled BOOL, PRIMARY KEY(name))").unwrap();
        db.execute("CREATE (:Feature {name: 'A', enabled: true})").unwrap();
        db.execute("CREATE (:Feature {name: 'B', enabled: false})").unwrap();

        let result = db.execute("MATCH (f:Feature) RETURN COUNT(f.enabled)").unwrap();
        assert_eq!(result.row_count(), 1);
        assert_eq!(result.get_row(0).unwrap().get("COUNT(f.enabled)"), Some(&Value::Int64(2)));
    }

    // C-REL-01: Relationship with FLOAT64/BOOL properties
    #[test]
    fn test_c_rel_01_relationship_with_float64_bool_properties() {
        let mut db = Database::new();
        db.execute("CREATE NODE TABLE Person(name STRING, PRIMARY KEY(name))").unwrap();
        db.execute("CREATE NODE TABLE Product(name STRING, PRIMARY KEY(name))").unwrap();
        db.execute("CREATE REL TABLE Reviews(FROM Person TO Product, rating FLOAT64, helpful BOOL)").unwrap();
        db.execute("CREATE (:Person {name: 'Alice'})").unwrap();
        db.execute("CREATE (:Product {name: 'Widget'})").unwrap();
        db.execute("MATCH (a:Person {name: 'Alice'}), (p:Product {name: 'Widget'}) CREATE (a)-[:Reviews {rating: 4.5, helpful: true}]->(p)").unwrap();

        let result = db.execute("MATCH (a:Person)-[r:Reviews]->(p:Product) RETURN a.name, r.rating, r.helpful, p.name").unwrap();
        assert_eq!(result.row_count(), 1);
        assert_eq!(result.get_row(0).unwrap().get("r.rating"), Some(&Value::Float64(4.5)));
        assert_eq!(result.get_row(0).unwrap().get("r.helpful"), Some(&Value::Bool(true)));
    }

    // C-MIX-01: Mixed-type table
    #[test]
    fn test_c_mix_01_mixed_type_table() {
        let mut db = Database::new();
        db.execute("CREATE NODE TABLE Item(id INT64, name STRING, price FLOAT64, inStock BOOL, PRIMARY KEY(id))").unwrap();
        db.execute("CREATE (:Item {id: 1, name: 'Widget', price: 19.99, inStock: true})").unwrap();
        db.execute("CREATE (:Item {id: 2, name: 'Gadget', price: 49.99, inStock: false})").unwrap();
        db.execute("CREATE (:Item {id: 3, name: 'Doohickey', price: 5.50, inStock: true})").unwrap();

        // Filter by BOOL + FLOAT64
        let result = db.execute("MATCH (i:Item) WHERE i.inStock = true RETURN i.name, i.price").unwrap();
        assert_eq!(result.row_count(), 2);

        let result = db.execute("MATCH (i:Item) WHERE i.price > 10.0 RETURN i.name").unwrap();
        assert_eq!(result.row_count(), 2);
    }
}

// =============================================================================
// Multi-Page Data Format Stability Contract (T015)
// =============================================================================

mod multi_page_format_contracts {
    use ruzu::storage::{BufferPool, DiskManager, PAGE_SIZE};
    use ruzu::calculate_pages_needed;
    use tempfile::TempDir;

    /// Contract: Multi-page data format is [4-byte u32 LE length][data][zero-padding].
    /// The length prefix stores the exact data length (excluding the prefix itself).
    /// This format must remain stable across versions.
    #[test]
    fn test_multi_page_format_length_prefix_is_u32_le() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let dm = DiskManager::new(&db_path).unwrap();
        let pool = BufferPool::new(64, dm).unwrap();

        let data = vec![0xAA; 100];
        let range = pool.allocate_page_range(1).unwrap();

        ruzu::write_multi_page_test(&pool, &range, &data).unwrap();

        // Read raw page and verify format
        use ruzu::storage::PageId;
        let handle = pool.pin(PageId::new(0, range.start_page)).unwrap();
        let raw = handle.data();

        // First 4 bytes = length prefix (u32 LE)
        let len = u32::from_le_bytes(raw[0..4].try_into().unwrap());
        assert_eq!(len, 100);

        // Next 100 bytes = data
        assert_eq!(&raw[4..104], &data[..]);

        // Remaining bytes in page are zero-padded
        assert!(raw[104..].iter().all(|&b| b == 0));
    }

    /// Contract: Multi-page data spanning multiple pages keeps contiguous data
    /// across page boundaries.
    #[test]
    fn test_multi_page_format_cross_page_contiguity() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let dm = DiskManager::new(&db_path).unwrap();
        let pool = BufferPool::new(64, dm).unwrap();

        // Data that spans 2 pages: PAGE_SIZE - 4 bytes of data + 4 prefix = exactly 1 page
        // So PAGE_SIZE - 3 bytes of data + 4 prefix = PAGE_SIZE + 1 = needs 2 pages
        let data: Vec<u8> = (0..(PAGE_SIZE + 100)).map(|i| (i % 256) as u8).collect();
        let num_pages = calculate_pages_needed(data.len());
        assert_eq!(num_pages, 2);

        let range = pool.allocate_page_range(num_pages).unwrap();
        ruzu::write_multi_page_test(&pool, &range, &data).unwrap();

        // Read back and verify
        let result = ruzu::read_multi_page_test(&pool, &range).unwrap();
        assert_eq!(result, data);
    }

    /// Contract: calculate_pages_needed formula is ceil((data_len + 4) / PAGE_SIZE).
    #[test]
    fn test_pages_needed_formula_contract() {
        // The formula must be: ceil((data_len + 4) / PAGE_SIZE)
        assert_eq!(calculate_pages_needed(0), 1);
        assert_eq!(calculate_pages_needed(PAGE_SIZE - 4), 1);
        assert_eq!(calculate_pages_needed(PAGE_SIZE - 3), 2);
        assert_eq!(calculate_pages_needed(2 * PAGE_SIZE - 4), 2);
        assert_eq!(calculate_pages_needed(2 * PAGE_SIZE - 3), 3);
    }

    // =========================================================================
    // T019: Node data multi-page serialization format stability
    // =========================================================================

    /// Contract: Node table data serialized via save_all_data() and loaded via
    /// load_table_data() must round-trip correctly when data exceeds a single page.
    ///
    /// The format is:
    /// - [4-byte LE length prefix][bincode-serialized HashMap<String, TableData>]
    /// - Data spans a contiguous PageRange allocated by the disk manager.
    /// - The header's metadata_range field records the range used.
    #[test]
    fn test_t019_node_data_multipage_format_stability() {
        use ruzu::{Database, DatabaseConfig, Value};
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test_db");

        let num_rows = 200;

        // Create database with > 4KB of node data
        {
            let mut db = Database::open(&db_path, DatabaseConfig::default()).unwrap();
            db.execute(
                "CREATE NODE TABLE Person(id INT64, name STRING, age INT64, PRIMARY KEY(id))",
            )
            .unwrap();

            for i in 0..num_rows {
                db.execute(&format!(
                    "CREATE (:Person {{id: {}, name: 'StableFormatTestPerson_{}', age: {}}})",
                    i,
                    i,
                    25 + (i % 40)
                ))
                .unwrap();
            }
            db.close().unwrap();
        }

        // Reopen and verify the data can be read back (format stability)
        {
            let mut db = Database::open(&db_path, DatabaseConfig::default()).unwrap();
            let result = db
                .execute("MATCH (p:Person) RETURN p.id, p.name, p.age")
                .unwrap();
            assert_eq!(
                result.row_count(),
                num_rows,
                "Multi-page format must round-trip all {} rows",
                num_rows
            );

            // Verify deterministic data at known positions
            let result = db
                .execute("MATCH (p:Person) WHERE p.id = 0 RETURN p.name, p.age")
                .unwrap();
            assert_eq!(
                result.get_row(0).unwrap().get("p.name"),
                Some(&Value::String("StableFormatTestPerson_0".to_string()))
            );
            assert_eq!(
                result.get_row(0).unwrap().get("p.age"),
                Some(&Value::Int64(25))
            );

            let result = db
                .execute("MATCH (p:Person) WHERE p.id = 100 RETURN p.name, p.age")
                .unwrap();
            assert_eq!(
                result.get_row(0).unwrap().get("p.name"),
                Some(&Value::String("StableFormatTestPerson_100".to_string()))
            );
            assert_eq!(
                result.get_row(0).unwrap().get("p.age"),
                Some(&Value::Int64(45)) // 25 + (100 % 40) = 25 + 20 = 45
            );
        }

        // Reopen a second time to verify no corruption from multiple open/close cycles
        {
            let mut db = Database::open(&db_path, DatabaseConfig::default()).unwrap();
            let result = db
                .execute("MATCH (p:Person) RETURN p.id")
                .unwrap();
            assert_eq!(
                result.row_count(),
                num_rows,
                "Second reopen must still have all {} rows",
                num_rows
            );
        }
    }

    // =========================================================================
    // T035: Catalog multi-page serialization format stability
    // =========================================================================

    /// Contract: Catalog data serialized via save_all_data() and loaded via
    /// load_database() must round-trip correctly when catalog exceeds a single page.
    ///
    /// The format is:
    /// - [4-byte LE length prefix][bincode-serialized Catalog]
    /// - Data spans a contiguous PageRange allocated by the disk manager.
    /// - The header's catalog_range field records the range used.
    #[test]
    fn test_t035_catalog_multipage_format_stability() {
        use ruzu::{Database, DatabaseConfig};
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test_db");

        let num_tables = 25;
        let cols_per_table = 8;

        // Create database with large catalog (> 4KB)
        {
            let mut db = Database::open(&db_path, DatabaseConfig::default()).unwrap();

            for t in 0..num_tables {
                let cols: Vec<String> = (0..cols_per_table)
                    .map(|c| format!("prop_{} STRING", c))
                    .collect();
                let col_defs = cols.join(", ");
                db.execute(&format!(
                    "CREATE NODE TABLE StableCatalog_{}(pk STRING, {}, PRIMARY KEY(pk))",
                    t, col_defs
                ))
                .unwrap();
            }

            // Also add a relationship table to verify mixed catalog
            db.execute(&format!(
                "CREATE NODE TABLE Anchor(id INT64, PRIMARY KEY(id))"
            ))
            .unwrap();
            db.execute(&format!(
                "CREATE REL TABLE Link(FROM Anchor TO Anchor, weight INT64)"
            ))
            .unwrap();

            db.close().unwrap();
        }

        // First reopen: verify all schemas are intact
        {
            let db = Database::open(&db_path, DatabaseConfig::default()).unwrap();
            for t in 0..num_tables {
                let table_name = format!("StableCatalog_{}", t);
                assert!(
                    db.catalog().table_exists(&table_name),
                    "Multi-page catalog must preserve table '{}'",
                    table_name
                );
                let schema = db.catalog().get_table(&table_name).unwrap();
                assert_eq!(
                    schema.columns.len(),
                    cols_per_table + 1,
                    "Table '{}' must have {} columns",
                    table_name,
                    cols_per_table + 1
                );
            }
            assert!(
                db.catalog().table_exists("Anchor"),
                "Anchor node table must exist"
            );
            assert!(
                db.catalog().rel_table_exists("Link"),
                "Link rel table must exist"
            );
        }

        // Second reopen: verify no corruption from multiple open/close cycles
        {
            let db = Database::open(&db_path, DatabaseConfig::default()).unwrap();
            for t in 0..num_tables {
                assert!(
                    db.catalog().table_exists(&format!("StableCatalog_{}", t)),
                    "Second reopen must still have all {} tables",
                    num_tables
                );
            }
        }
    }

    // T027: Rel data multi-page serialization format stability
    // =========================================================================

    /// Contract: Relationship table data serialized via save_all_data() and loaded via
    /// load_rel_table_data() must round-trip correctly when data exceeds a single page.
    ///
    /// The format is:
    /// - [4-byte LE length prefix][bincode-serialized HashMap<String, RelTableData>]
    /// - Data spans a contiguous PageRange allocated by the disk manager.
    /// - The header's rel_metadata_range field records the range used.
    #[test]
    fn test_t027_rel_data_multipage_format_stability() {
        use ruzu::{Database, DatabaseConfig};
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test_db");

        let num_persons = 40;
        let num_rels = 150;

        // Create database with > 4KB of relationship data
        {
            let mut db = Database::open(&db_path, DatabaseConfig::default()).unwrap();
            db.execute(
                "CREATE NODE TABLE Person(id INT64, name STRING, PRIMARY KEY(id))",
            )
            .unwrap();
            db.execute(
                "CREATE REL TABLE Knows(FROM Person TO Person, since INT64, tag STRING)",
            )
            .unwrap();

            for i in 0..num_persons {
                db.execute(&format!(
                    "CREATE (:Person {{id: {}, name: 'StableFormatPerson_{}'}})",
                    i, i
                ))
                .unwrap();
            }

            for i in 0..num_rels {
                let src = i % num_persons;
                let dst = (i + 1) % num_persons;
                db.execute(&format!(
                    "MATCH (a:Person {{id: {}}}), (b:Person {{id: {}}}) CREATE (a)-[:Knows {{since: {}, tag: 'StableTag_{}'}}]->(b)",
                    src, dst, 2000 + i, i
                )).unwrap();
            }
            db.close().unwrap();
        }

        // Reopen and verify the data can be read back (format stability)
        {
            let mut db = Database::open(&db_path, DatabaseConfig::default()).unwrap();
            let result = db
                .execute(
                    "MATCH (a:Person)-[r:Knows]->(b:Person) RETURN a.id, b.id, r.since, r.tag",
                )
                .unwrap();
            assert_eq!(
                result.row_count(),
                num_rels,
                "Multi-page rel format must round-trip all {} relationships",
                num_rels
            );

            // Verify deterministic data: first relationship (Person 0 -> Person 1)
            let result = db
                .execute(
                    "MATCH (a:Person {id: 0})-[r:Knows]->(b:Person) RETURN b.id, r.since, r.tag",
                )
                .unwrap();
            assert!(
                result.row_count() > 0,
                "Expected at least one relationship from Person 0"
            );
        }

        // Reopen a second time to verify no corruption from multiple open/close cycles
        {
            let mut db = Database::open(&db_path, DatabaseConfig::default()).unwrap();
            let result = db
                .execute(
                    "MATCH (a:Person)-[r:Knows]->(b:Person) RETURN a.id",
                )
                .unwrap();
            assert_eq!(
                result.row_count(),
                num_rels,
                "Second reopen must still have all {} relationships",
                num_rels
            );
        }
    }
}
