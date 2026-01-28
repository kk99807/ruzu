//! Storage benchmarks for columnar storage operations.
//!
//! Measures storage performance for:
//! - Column storage push operations
//! - Node table creation and insertion
//! - Bulk insert operations (1000 nodes)

use std::collections::HashMap;
use std::sync::Arc;

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use ruzu::catalog::{ColumnDef, NodeTableSchema};
use ruzu::storage::{ColumnStorage, NodeTable};
use ruzu::types::{DataType, Value};

/// Create a test schema for Person table
fn create_person_schema() -> Arc<NodeTableSchema> {
    let columns = vec![
        ColumnDef::new("id".to_string(), DataType::Int64).unwrap(),
        ColumnDef::new("name".to_string(), DataType::String).unwrap(),
    ];
    Arc::new(NodeTableSchema::new("Person".to_string(), columns, vec!["id".to_string()]).unwrap())
}

/// Benchmark column storage push operation
fn bench_column_push(c: &mut Criterion) {
    let mut group = c.benchmark_group("column_push");

    // Push Int64 values
    group.bench_function("int64", |b| {
        b.iter(|| {
            let mut col = ColumnStorage::new();
            for i in 0..1000 {
                col.push(black_box(Value::Int64(i)));
            }
            col
        });
    });

    // Push String values
    group.bench_function("string", |b| {
        b.iter(|| {
            let mut col = ColumnStorage::new();
            for i in 0..1000 {
                col.push(black_box(Value::String(format!("Person{i}"))));
            }
            col
        });
    });

    group.finish();
}

/// Benchmark column storage get operation
fn bench_column_get(c: &mut Criterion) {
    // Pre-populate column with 1000 values
    let mut col = ColumnStorage::new();
    for i in 0..1000 {
        col.push(Value::Int64(i));
    }

    c.bench_function("column_get_1000", |b| {
        b.iter(|| {
            for i in 0..1000 {
                black_box(col.get(i));
            }
        });
    });
}

/// Benchmark node table single insert
fn bench_table_single_insert(c: &mut Criterion) {
    let schema = create_person_schema();

    c.bench_function("table_single_insert", |b| {
        b.iter_batched(
            || NodeTable::new(Arc::clone(&schema)),
            |mut table| {
                let mut row = HashMap::new();
                row.insert("id".to_string(), Value::Int64(1));
                row.insert("name".to_string(), Value::String("Alice".to_string()));
                table.insert(&row).unwrap();
                table
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

/// Benchmark node table bulk insert (1000 nodes)
fn bench_table_bulk_insert(c: &mut Criterion) {
    let schema = create_person_schema();

    c.bench_function("table_bulk_insert_1000", |b| {
        b.iter_batched(
            || NodeTable::new(Arc::clone(&schema)),
            |mut table| {
                for i in 0..1000 {
                    let mut row = HashMap::new();
                    row.insert("id".to_string(), Value::Int64(i));
                    row.insert("name".to_string(), Value::String(format!("Person{i}")));
                    table.insert(&row).unwrap();
                }
                table
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

/// Benchmark varying insert sizes
fn bench_table_insert_scaling(c: &mut Criterion) {
    let schema = create_person_schema();
    let mut group = c.benchmark_group("table_insert_scaling");

    for size in &[100, 500, 1000, 2000] {
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            b.iter_batched(
                || NodeTable::new(Arc::clone(&schema)),
                |mut table| {
                    for i in 0..size {
                        let mut row = HashMap::new();
                        row.insert("id".to_string(), Value::Int64(i64::from(i)));
                        row.insert("name".to_string(), Value::String(format!("Person{i}")));
                        table.insert(&row).unwrap();
                    }
                    table
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

/// Benchmark table creation
fn bench_table_creation(c: &mut Criterion) {
    let schema = create_person_schema();

    c.bench_function("table_creation", |b| {
        b.iter(|| NodeTable::new(black_box(Arc::clone(&schema))));
    });
}

criterion_group!(
    benches,
    bench_column_push,
    bench_column_get,
    bench_table_single_insert,
    bench_table_bulk_insert,
    bench_table_insert_scaling,
    bench_table_creation
);
criterion_main!(benches);
