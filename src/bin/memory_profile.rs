//! Memory profiling binary for CSV import.
//!
//! This binary measures memory usage during large CSV imports using DHAT.
//!
//! # Usage
//!
//! ```bash
//! cargo run --release --features dhat-heap --bin memory_profile -- [rows]
//! ```
//!
//! After running, open the generated `dhat-heap.json` file at:
//! https://nnethercote.github.io/dh_view/dh_view.html

#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

use std::io::Write;
use std::path::Path;
use std::sync::Arc;

use ruzu::catalog::{ColumnDef, NodeTableSchema};
use ruzu::storage::csv::{CsvImportConfig, NodeLoader, RelLoader};
use ruzu::types::DataType;

/// Generate a test CSV file with the specified number of rows.
/// Each row is approximately 50 bytes, so 20M rows ~= 1GB.
fn generate_csv_file(path: &Path, num_rows: usize) {
    println!("Generating CSV file with {} rows...", num_rows);
    let start = std::time::Instant::now();

    let mut file = std::fs::File::create(path).expect("create csv file");

    // Write header
    writeln!(file, "id,name,city,age,score").expect("write header");

    // Cities for repetition (tests string interning benefit)
    let cities = [
        "NYC",
        "LA",
        "Chicago",
        "Houston",
        "Phoenix",
        "Philadelphia",
        "San Antonio",
        "San Diego",
        "Dallas",
        "Austin",
    ];

    // Write rows
    for i in 0..num_rows {
        let city = cities[i % cities.len()];
        writeln!(
            file,
            "{},Person{},{},{},{:.2}",
            i,
            i,
            city,
            20 + (i % 60),
            (i % 10000) as f64 / 100.0
        )
        .expect("write row");

        // Progress update every 1M rows
        if i > 0 && i % 1_000_000 == 0 {
            println!("  Generated {} rows...", i);
        }
    }

    let elapsed = start.elapsed();
    let file_size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    println!(
        "Generated {:.2} MB CSV file in {:.2}s",
        file_size as f64 / 1024.0 / 1024.0,
        elapsed.as_secs_f64()
    );
}

/// Generate a relationship CSV file.
fn generate_rel_csv_file(path: &Path, num_nodes: usize, edges_per_node: usize) {
    let total_edges = num_nodes * edges_per_node;
    println!("Generating relationship CSV with {} edges...", total_edges);
    let start = std::time::Instant::now();

    let mut file = std::fs::File::create(path).expect("create csv file");

    // Write header
    writeln!(file, "FROM,TO,weight").expect("write header");

    // Generate edges
    let mut edge_count = 0;
    for from_id in 0..num_nodes {
        for j in 0..edges_per_node {
            let to_id = (from_id + j + 1) % num_nodes;
            writeln!(
                file,
                "Person{},Person{},{:.2}",
                from_id,
                to_id,
                (edge_count % 100) as f64 / 10.0
            )
            .expect("write row");
            edge_count += 1;
        }

        // Progress update every 100K nodes
        if from_id > 0 && from_id % 100_000 == 0 {
            println!("  Generated {} edges...", from_id * edges_per_node);
        }
    }

    let elapsed = start.elapsed();
    let file_size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    println!(
        "Generated {:.2} MB relationship CSV in {:.2}s",
        file_size as f64 / 1024.0 / 1024.0,
        elapsed.as_secs_f64()
    );
}

fn create_node_schema() -> Arc<NodeTableSchema> {
    Arc::new(
        NodeTableSchema::new(
            "Person".to_string(),
            vec![
                ColumnDef::new("id".to_string(), DataType::Int64).unwrap(),
                ColumnDef::new("name".to_string(), DataType::String).unwrap(),
                ColumnDef::new("city".to_string(), DataType::String).unwrap(),
                ColumnDef::new("age".to_string(), DataType::Int64).unwrap(),
                ColumnDef::new("score".to_string(), DataType::Float64).unwrap(),
            ],
            vec!["id".to_string()],
        )
        .unwrap(),
    )
}

fn profile_node_import(csv_path: &Path, config: CsvImportConfig, label: &str) {
    println!("\n=== Profiling Node Import (Streaming): {} ===", label);

    let batch_size = config.batch_size;
    let schema = create_node_schema();
    let loader = NodeLoader::new(schema, config);

    let start = std::time::Instant::now();
    let mut batch_count = 0u64;
    let mut total_rows = 0u64;

    // Use streaming import - batches are discarded after counting
    let result = loader
        .load_streaming(
            csv_path,
            |batch| {
                batch_count += 1;
                total_rows += batch.len() as u64;
                // Batch is dropped here - simulates writing to storage
                Ok(())
            },
            None,
        )
        .expect("load csv");

    let elapsed = start.elapsed();

    let throughput = result.rows_imported as f64 / elapsed.as_secs_f64();
    println!(
        "Imported {} rows in {} batches in {:.2}s ({:.2} rows/sec)",
        result.rows_imported,
        batch_count,
        elapsed.as_secs_f64(),
        throughput
    );
    println!(
        "Memory: Only batch buffer (~{}KB at a time)",
        batch_size * 50 / 1024
    );
}

fn profile_rel_import(csv_path: &Path, config: CsvImportConfig, label: &str) {
    println!("\n=== Profiling Relationship Import (Streaming): {} ===", label);

    let property_columns = vec![("weight".to_string(), DataType::Float64)];
    let loader = RelLoader::with_default_columns(property_columns, config.clone());

    let start = std::time::Instant::now();
    let mut batch_count = 0u64;
    let mut total_rels = 0u64;

    // Use streaming import - batches are discarded after counting
    let result = loader
        .load_streaming(
            csv_path,
            |batch| {
                batch_count += 1;
                total_rels += batch.len() as u64;
                // Batch is dropped here - simulates writing to storage
                Ok(())
            },
            None,
        )
        .expect("load csv");

    let elapsed = start.elapsed();

    let throughput = result.rows_imported as f64 / elapsed.as_secs_f64();
    println!(
        "Imported {} relationships in {} batches in {:.2}s ({:.2} rels/sec)",
        result.rows_imported,
        batch_count,
        elapsed.as_secs_f64(),
        throughput
    );
    println!(
        "Memory: Only batch buffer (~{}KB at a time)",
        config.batch_size * 100 / 1024
    );
}

fn main() {
    #[cfg(feature = "dhat-heap")]
    let _profiler = dhat::Profiler::new_heap();

    // Parse command line args
    let args: Vec<String> = std::env::args().collect();
    let num_rows: usize = args
        .get(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(20_000_000);

    println!("===========================================");
    println!("  RUZU Memory Profiler");
    println!("===========================================");
    println!(
        "Target rows: {} ({:.2}M)",
        num_rows,
        num_rows as f64 / 1_000_000.0
    );

    // Create temp directory
    let temp_dir = tempfile::TempDir::new().expect("create temp dir");
    let node_csv_path = temp_dir.path().join("nodes.csv");
    let rel_csv_path = temp_dir.path().join("relationships.csv");

    // Generate test data
    generate_csv_file(&node_csv_path, num_rows);

    // Calculate relationship count (aim for ~2x nodes in edges)
    let num_nodes_for_rels = num_rows / 10; // Use fewer nodes
    let edges_per_node = 20;
    generate_rel_csv_file(&rel_csv_path, num_nodes_for_rels, edges_per_node);

    println!("\n===========================================");
    println!("  Starting Memory Profiling");
    println!("===========================================");

    // Profile different configurations
    // Note: mmap_threshold minimum is 1MB (1_048_576 bytes)
    let min_mmap_threshold = 1_048_576;

    println!("\n--- Test 1: Sequential, No Mmap ---");
    profile_node_import(
        &node_csv_path,
        CsvImportConfig::default()
            .with_parallel(false)
            .with_mmap(false),
        "Sequential, Buffered I/O",
    );

    println!("\n--- Test 2: Parallel, With Mmap ---");
    profile_node_import(
        &node_csv_path,
        CsvImportConfig::default()
            .with_parallel(true)
            .with_mmap(true)
            .with_mmap_threshold(min_mmap_threshold), // Force mmap on small files
        "Parallel, Mmap",
    );

    println!("\n--- Test 3: Parallel, With String Interning ---");
    profile_node_import(
        &node_csv_path,
        CsvImportConfig::default()
            .with_parallel(true)
            .with_mmap(true)
            .with_mmap_threshold(min_mmap_threshold)
            .with_intern_strings(true),
        "Parallel, Mmap, String Interning",
    );

    println!("\n--- Test 4: Relationship Import ---");
    profile_rel_import(
        &rel_csv_path,
        CsvImportConfig::default()
            .with_parallel(true)
            .with_mmap(true)
            .with_mmap_threshold(min_mmap_threshold),
        "Parallel Relationships",
    );

    println!("\n===========================================");
    println!("  Profiling Complete!");
    println!("===========================================");
    println!("\nDHAT output written to: dhat-heap.json");
    println!("View results at: https://nnethercote.github.io/dh_view/dh_view.html");
}
