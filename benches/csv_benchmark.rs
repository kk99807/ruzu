//! CSV import benchmarks.
//!
//! Measures CSV import performance for:
//! - Node bulk import (target: 50K nodes/sec)
//! - Relationship bulk import (target: 100K rels/sec)
//!
//! Includes benchmarks matching KuzuDB study methodology:
//! - 100K nodes (comparable to kuzudb-study)
//! - ~2.4M relationships (comparable to kuzudb-study)

use std::io::Write;
use std::path::Path;
use std::sync::Arc;

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use ruzu::catalog::{ColumnDef, NodeTableSchema};
use ruzu::storage::csv::{CsvImportConfig, NodeLoader, RelLoader};
use ruzu::types::DataType;
use tempfile::TempDir;

/// Create a test schema for Person table
fn create_person_schema() -> Arc<NodeTableSchema> {
    let columns = vec![
        ColumnDef::new("id".to_string(), DataType::Int64).unwrap(),
        ColumnDef::new("name".to_string(), DataType::String).unwrap(),
        ColumnDef::new("age".to_string(), DataType::Int64).unwrap(),
    ];
    Arc::new(NodeTableSchema::new("Person".to_string(), columns, vec!["id".to_string()]).unwrap())
}

/// Generate a CSV file with the given number of rows
fn generate_csv_file(dir: &Path, num_rows: usize) -> std::path::PathBuf {
    let csv_path = dir.join("test_data.csv");
    let mut file = std::fs::File::create(&csv_path).expect("create csv file");

    // Write header
    writeln!(file, "id,name,age").expect("write header");

    // Write rows
    for i in 0..num_rows {
        writeln!(file, "{},Person{},{}", i, i, 20 + (i % 50)).expect("write row");
    }

    csv_path
}

/// Benchmark node bulk import (target: 50K nodes/sec)
fn bench_csv_node_import(c: &mut Criterion) {
    let mut group = c.benchmark_group("csv_node_import");
    let schema = create_person_schema();

    // Test different dataset sizes
    for size in &[1000, 10000, 50000] {
        let temp_dir = TempDir::new().expect("create temp dir");
        let csv_path = generate_csv_file(temp_dir.path(), *size);

        group.throughput(Throughput::Elements(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _| {
            b.iter(|| {
                let loader = NodeLoader::new(Arc::clone(&schema), CsvImportConfig::default());
                let (rows, result) = loader.load(&csv_path, None).expect("load csv");
                black_box((rows.len(), result.rows_imported))
            });
        });
    }

    group.finish();
}

/// Benchmark CSV parsing only (without insertion)
fn bench_csv_parse_only(c: &mut Criterion) {
    let mut group = c.benchmark_group("csv_parse_only");
    let schema = create_person_schema();

    let temp_dir = TempDir::new().expect("create temp dir");
    let csv_path = generate_csv_file(temp_dir.path(), 10000);

    group.throughput(Throughput::Elements(10000));
    group.bench_function("10k_rows", |b| {
        b.iter(|| {
            let loader = NodeLoader::new(Arc::clone(&schema), CsvImportConfig::default());
            let (rows, _) = loader.load(&csv_path, None).expect("load csv");
            black_box(rows.len())
        });
    });

    group.finish();
}

/// Benchmark CSV import with different batch sizes
fn bench_csv_batch_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("csv_batch_sizes");
    let schema = create_person_schema();

    let temp_dir = TempDir::new().expect("create temp dir");
    let csv_path = generate_csv_file(temp_dir.path(), 10000);

    for batch_size in &[512, 1024, 2048, 4096] {
        group.throughput(Throughput::Elements(10000));
        group.bench_with_input(
            BenchmarkId::from_parameter(batch_size),
            batch_size,
            |b, &batch_size| {
                b.iter(|| {
                    let config = CsvImportConfig::default().with_batch_size(batch_size);
                    let loader = NodeLoader::new(Arc::clone(&schema), config);
                    let (rows, _) = loader.load(&csv_path, None).expect("load csv");
                    black_box(rows.len())
                });
            },
        );
    }

    group.finish();
}

/// Generate a relationship CSV file with ~24 edges per node (avg degree)
/// This creates approximately num_nodes * 24 relationships
fn generate_relationship_csv(
    dir: &Path,
    num_nodes: usize,
    edges_per_node: usize,
) -> (std::path::PathBuf, usize) {
    let csv_path = dir.join("relationships.csv");
    let mut file = std::fs::File::create(&csv_path).expect("create csv file");

    // Write header
    writeln!(file, "FROM,TO,since").expect("write header");

    // Generate edges - each node connects to `edges_per_node` other nodes
    let total_edges = num_nodes * edges_per_node;
    let mut edge_count = 0;
    for from_id in 0..num_nodes {
        for j in 0..edges_per_node {
            let to_id = (from_id + j + 1) % num_nodes;
            writeln!(file, "{},{},{}", from_id, to_id, 2015 + (edge_count % 10))
                .expect("write row");
            edge_count += 1;
        }
    }

    (csv_path, total_edges)
}

/// Benchmark matching KuzuDB study: 100K nodes
/// Reference: kuzudb-study reports ~769K nodes/sec (100K in 0.13 sec)
fn bench_kuzu_study_nodes(c: &mut Criterion) {
    let mut group = c.benchmark_group("kuzu_study_comparison");
    group.sample_size(10); // Larger dataset, fewer samples

    let schema = create_person_schema();
    let num_nodes = 100_000;

    let temp_dir = TempDir::new().expect("create temp dir");
    let csv_path = generate_csv_file(temp_dir.path(), num_nodes);

    group.throughput(Throughput::Elements(num_nodes as u64));
    group.bench_function("100k_nodes", |b| {
        b.iter(|| {
            let loader = NodeLoader::new(Arc::clone(&schema), CsvImportConfig::default());
            let (rows, result) = loader.load(&csv_path, None).expect("load csv");
            black_box((rows.len(), result.rows_imported))
        });
    });

    group.finish();
}

/// Benchmark matching KuzuDB study: ~2.4M edges
/// Reference: kuzudb-study reports ~5.3M edges/sec (2.4M in 0.45 sec)
fn bench_kuzu_study_edges(c: &mut Criterion) {
    let mut group = c.benchmark_group("kuzu_study_comparison");
    group.sample_size(10); // Large dataset, fewer samples

    // 100K nodes with ~24 edges each = ~2.4M edges
    let num_nodes = 100_000;
    let edges_per_node = 24;

    let temp_dir = TempDir::new().expect("create temp dir");
    let (csv_path, total_edges) =
        generate_relationship_csv(temp_dir.path(), num_nodes, edges_per_node);

    // RelLoader for parsing relationship CSV
    let property_columns = vec![("since".to_string(), DataType::Int64)];

    group.throughput(Throughput::Elements(total_edges as u64));
    group.bench_function("2.4m_edges", |b| {
        b.iter(|| {
            let loader = RelLoader::with_default_columns(
                property_columns.clone(),
                CsvImportConfig::default(),
            );
            let (rels, result) = loader.load(&csv_path, None).expect("load csv");
            black_box((rels.len(), result.rows_imported))
        });
    });

    group.finish();
}

/// Benchmark parallel vs sequential CSV parsing
/// This tests the new parallel processing optimizations
fn bench_parallel_vs_sequential(c: &mut Criterion) {
    let mut group = c.benchmark_group("parallel_vs_sequential");
    group.sample_size(10);

    let schema = create_person_schema();
    let num_rows = 100_000;

    let temp_dir = TempDir::new().expect("create temp dir");
    let csv_path = generate_csv_file(temp_dir.path(), num_rows);

    group.throughput(Throughput::Elements(num_rows as u64));

    // Sequential mode
    group.bench_function("sequential_100k", |b| {
        b.iter(|| {
            let config = CsvImportConfig::default().with_parallel(false);
            let loader = NodeLoader::new(Arc::clone(&schema), config);
            let (rows, result) = loader.load(&csv_path, None).expect("load csv");
            black_box((rows.len(), result.rows_imported))
        });
    });

    // Parallel mode (default)
    group.bench_function("parallel_100k", |b| {
        b.iter(|| {
            let config = CsvImportConfig::default().with_parallel(true);
            let loader = NodeLoader::new(Arc::clone(&schema), config);
            let (rows, result) = loader.load(&csv_path, None).expect("load csv");
            black_box((rows.len(), result.rows_imported))
        });
    });

    group.finish();
}

/// Benchmark mmap vs buffered I/O (requires large file)
fn bench_mmap_vs_buffered(c: &mut Criterion) {
    let mut group = c.benchmark_group("mmap_vs_buffered");
    group.sample_size(10);

    let schema = create_person_schema();
    let num_rows = 100_000;

    let temp_dir = TempDir::new().expect("create temp dir");
    let csv_path = generate_csv_file(temp_dir.path(), num_rows);

    group.throughput(Throughput::Elements(num_rows as u64));

    // With mmap enabled (threshold lowered to force mmap)
    group.bench_function("with_mmap", |b| {
        b.iter(|| {
            let config = CsvImportConfig::default()
                .with_mmap(true)
                .with_mmap_threshold(1024 * 1024); // 1MB threshold (minimum allowed)
            let loader = NodeLoader::new(Arc::clone(&schema), config);
            let (rows, result) = loader.load(&csv_path, None).expect("load csv");
            black_box((rows.len(), result.rows_imported))
        });
    });

    // Without mmap
    group.bench_function("without_mmap", |b| {
        b.iter(|| {
            let config = CsvImportConfig::default().with_mmap(false);
            let loader = NodeLoader::new(Arc::clone(&schema), config);
            let (rows, result) = loader.load(&csv_path, None).expect("load csv");
            black_box((rows.len(), result.rows_imported))
        });
    });

    group.finish();
}

/// Benchmark streaming node import throughput (target: ≥7M nodes/sec)
/// This tests the streaming import path for large files.
fn bench_streaming_node_import(c: &mut Criterion) {
    let mut group = c.benchmark_group("streaming_throughput");
    group.sample_size(10);

    let schema = create_person_schema();

    // Test with 500K nodes to simulate large file behavior
    let num_nodes = 500_000;
    let temp_dir = TempDir::new().expect("create temp dir");
    let csv_path = generate_csv_file(temp_dir.path(), num_nodes);

    group.throughput(Throughput::Elements(num_nodes as u64));

    // Streaming mode with batch writes
    group.bench_function("streaming_500k_nodes", |b| {
        b.iter(|| {
            let config = CsvImportConfig::default()
                .with_parallel(true)
                .with_batch_size(100_000); // 100K batch size
            let loader = NodeLoader::new(Arc::clone(&schema), config);
            let (rows, result) = loader.load(&csv_path, None).expect("load csv");
            black_box((rows.len(), result.rows_imported))
        });
    });

    // Compare with non-streaming for reference
    group.bench_function("non_streaming_500k_nodes", |b| {
        b.iter(|| {
            let config = CsvImportConfig::default()
                .with_parallel(true)
                .with_batch_size(2048); // Default smaller batch
            let loader = NodeLoader::new(Arc::clone(&schema), config);
            let (rows, result) = loader.load(&csv_path, None).expect("load csv");
            black_box((rows.len(), result.rows_imported))
        });
    });

    group.finish();
}

/// Benchmark streaming edge import throughput (target: ≥3M edges/sec)
/// This tests the streaming import path for large relationship files.
fn bench_streaming_edge_import(c: &mut Criterion) {
    let mut group = c.benchmark_group("streaming_throughput");
    group.sample_size(10);

    // 100K nodes with 24 edges each = ~2.4M edges (same as kuzu study)
    let num_nodes = 100_000;
    let edges_per_node = 24;

    let temp_dir = TempDir::new().expect("create temp dir");
    let (csv_path, total_edges) =
        generate_relationship_csv(temp_dir.path(), num_nodes, edges_per_node);

    let property_columns = vec![("since".to_string(), DataType::Int64)];

    group.throughput(Throughput::Elements(total_edges as u64));

    // Streaming mode with batch writes
    group.bench_function("streaming_2.4m_edges", |b| {
        b.iter(|| {
            let config = CsvImportConfig::default()
                .with_parallel(true)
                .with_batch_size(100_000); // 100K batch size
            let loader = RelLoader::with_default_columns(property_columns.clone(), config);
            let (rels, result) = loader.load(&csv_path, None).expect("load csv");
            black_box((rels.len(), result.rows_imported))
        });
    });

    // Compare with non-streaming for reference
    group.bench_function("non_streaming_2.4m_edges", |b| {
        b.iter(|| {
            let config = CsvImportConfig::default()
                .with_parallel(true)
                .with_batch_size(2048); // Default smaller batch
            let loader = RelLoader::with_default_columns(property_columns.clone(), config);
            let (rels, result) = loader.load(&csv_path, None).expect("load csv");
            black_box((rels.len(), result.rows_imported))
        });
    });

    group.finish();
}

/// Benchmark different streaming batch sizes to find optimal configuration
fn bench_streaming_batch_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("streaming_batch_sizes");
    group.sample_size(10);

    let schema = create_person_schema();
    let num_rows = 200_000;

    let temp_dir = TempDir::new().expect("create temp dir");
    let csv_path = generate_csv_file(temp_dir.path(), num_rows);

    group.throughput(Throughput::Elements(num_rows as u64));

    // Test different batch sizes for streaming
    for batch_size in &[10_000, 50_000, 100_000, 200_000, 500_000] {
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("batch_{batch_size}")),
            batch_size,
            |b, &batch_size| {
                b.iter(|| {
                    let config = CsvImportConfig::default()
                        .with_parallel(true)
                        .with_batch_size(batch_size);
                    let loader = NodeLoader::new(Arc::clone(&schema), config);
                    let (rows, _) = loader.load(&csv_path, None).expect("load csv");
                    black_box(rows.len())
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_csv_node_import,
    bench_csv_parse_only,
    bench_csv_batch_sizes,
    bench_kuzu_study_nodes,
    bench_kuzu_study_edges,
    bench_parallel_vs_sequential,
    bench_mmap_vs_buffered,
    bench_streaming_node_import,
    bench_streaming_edge_import,
    bench_streaming_batch_sizes
);
criterion_main!(benches);
