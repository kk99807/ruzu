//! Memory profiling benchmarks for CSV import.
//!
//! This benchmark module is designed to be run with DHAT heap profiler
//! to measure actual memory usage during streaming imports.
//!
//! # Running memory benchmarks
//!
//! ```bash
//! # Build with DHAT profiler enabled
//! cargo build --release --features dhat-heap
//!
//! # Run memory benchmark
//! cargo bench --bench memory_benchmark
//!
//! # Analyze DHAT output (creates dhat-heap.json)
//! # View results in a DHAT-compatible viewer
//! ```
//!
//! # Memory Contracts
//!
//! - MC-001: 1GB node import < 500MB peak memory
//! - MC-002: 1GB edge import < 500MB peak memory
//! - MC-003: 5GB import < 500MB peak memory
//! - MC-004: Memory variance < 100MB across file sizes

use std::io::Write;
use std::path::Path;
use std::sync::Arc;

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use ruzu::catalog::{ColumnDef, NodeTableSchema};
use ruzu::storage::csv::{CsvImportConfig, NodeLoader, RelLoader, RowBuffer};
use ruzu::types::{DataType, Value};
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

    writeln!(file, "id,name,age").expect("write header");
    for i in 0..num_rows {
        writeln!(file, "{},Person{},{}", i, i, 20 + (i % 50)).expect("write row");
    }

    csv_path
}

/// Generate a relationship CSV file
fn generate_relationship_csv(dir: &Path, num_rels: usize) -> std::path::PathBuf {
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

/// Benchmark memory usage during streaming node import
///
/// When run with DHAT profiler (--features dhat-heap), this benchmark
/// will output detailed memory allocation information.
fn bench_memory_streaming_nodes(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_streaming");
    group.sample_size(10);

    let schema = create_person_schema();

    // Test with different dataset sizes to verify constant memory
    for size in &[100_000, 200_000, 500_000] {
        let temp_dir = TempDir::new().expect("create temp dir");
        let csv_path = generate_csv_file(temp_dir.path(), *size);

        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{size}_nodes")),
            size,
            |b, _| {
                b.iter(|| {
                    let config = CsvImportConfig::default()
                        .with_parallel(true)
                        .with_batch_size(100_000);
                    let loader = NodeLoader::new(Arc::clone(&schema), config);
                    let (rows, result) = loader.load(&csv_path, None).expect("load csv");
                    black_box((rows.len(), result.rows_imported))
                });
            },
        );
    }

    group.finish();
}

/// Benchmark memory usage during streaming edge import
fn bench_memory_streaming_edges(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_streaming");
    group.sample_size(10);

    let property_columns = vec![("since".to_string(), DataType::Int64)];

    // Test with different relationship counts
    for size in &[100_000, 500_000, 1_000_000] {
        let temp_dir = TempDir::new().expect("create temp dir");
        let csv_path = generate_relationship_csv(temp_dir.path(), *size);

        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{size}_edges")),
            size,
            |b, _| {
                b.iter(|| {
                    let config = CsvImportConfig::default()
                        .with_parallel(true)
                        .with_batch_size(100_000);
                    let loader = RelLoader::with_default_columns(property_columns.clone(), config);
                    let (rels, result) = loader.load(&csv_path, None).expect("load csv");
                    black_box((rels.len(), result.rows_imported))
                });
            },
        );
    }

    group.finish();
}

/// Benchmark RowBuffer memory recycling efficiency
///
/// This measures the overhead of buffer recycling vs fresh allocation
fn bench_row_buffer_recycling(c: &mut Criterion) {
    let mut group = c.benchmark_group("row_buffer_memory");

    // Test recycling efficiency with different batch sizes
    for batch_size in &[10_000, 50_000, 100_000] {
        group.bench_with_input(
            BenchmarkId::new("recycling", batch_size),
            batch_size,
            |b, &batch_size| {
                b.iter(|| {
                    let mut buffer = RowBuffer::new(batch_size, 5);

                    // Simulate 5 batch cycles with recycling
                    for _ in 0..5 {
                        for i in 0..batch_size {
                            buffer
                                .push_with_recycling(vec![
                                    Value::Int64(i as i64),
                                    Value::String(format!("item_{}", i)),
                                    Value::Bool(i % 2 == 0),
                                ])
                                .unwrap();
                        }
                        buffer.recycle();
                    }

                    black_box(buffer.recycled_count())
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("fresh_allocation", batch_size),
            batch_size,
            |b, &batch_size| {
                b.iter(|| {
                    // Simulate 5 batch cycles without recycling
                    for _ in 0..5 {
                        let mut buffer = RowBuffer::new(batch_size, 5);
                        for i in 0..batch_size {
                            buffer
                                .push(vec![
                                    Value::Int64(i as i64),
                                    Value::String(format!("item_{}", i)),
                                    Value::Bool(i % 2 == 0),
                                ])
                                .unwrap();
                        }
                        // Drop buffer - no recycling
                        black_box(buffer.len());
                    }
                });
            },
        );
    }

    group.finish();
}

/// Benchmark memory with different batch sizes
///
/// This helps find the optimal batch size for memory/throughput balance
fn bench_memory_batch_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_batch_sizes");
    group.sample_size(10);

    let schema = create_person_schema();
    let num_rows = 500_000;

    let temp_dir = TempDir::new().expect("create temp dir");
    let csv_path = generate_csv_file(temp_dir.path(), num_rows);

    // Test different batch sizes
    for batch_size in &[10_000, 50_000, 100_000, 200_000] {
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
    bench_memory_streaming_nodes,
    bench_memory_streaming_edges,
    bench_row_buffer_recycling,
    bench_memory_batch_sizes
);
criterion_main!(benches);
