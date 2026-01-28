//! End-to-end benchmarks for complete query workflows.
//!
//! Measures full query execution performance including:
//! - Schema creation
//! - Data insertion (1000 nodes)
//! - MATCH queries with filtering and projection
//! - Target query from spec: CREATE TABLE -> INSERT 1000 -> MATCH WHERE

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use ruzu::Database;

/// Helper: Create a database with Person schema
fn setup_database_with_schema() -> Database {
    let mut db = Database::new();
    db.execute("CREATE NODE TABLE Person (id INT64, name STRING, PRIMARY KEY (id))")
        .unwrap();
    db
}

/// Helper: Create a database with schema and N nodes
fn setup_database_with_nodes(n: i64) -> Database {
    let mut db = setup_database_with_schema();
    for i in 0..n {
        db.execute(&format!("CREATE (:Person {{id: {i}, name: 'Person{i}'}})"))
            .unwrap();
    }
    db
}

/// Benchmark schema creation
fn bench_create_node_table(c: &mut Criterion) {
    c.bench_function("e2e_create_node_table", |b| {
        b.iter(|| {
            let mut db = Database::new();
            db.execute(black_box(
                "CREATE NODE TABLE Person (id INT64, name STRING, PRIMARY KEY (id))",
            ))
            .unwrap();
            db
        });
    });
}

/// Benchmark single node insertion
fn bench_create_single_node(c: &mut Criterion) {
    c.bench_function("e2e_create_single_node", |b| {
        b.iter_batched(
            setup_database_with_schema,
            |mut db| {
                db.execute(black_box("CREATE (:Person {id: 1, name: 'Alice'})"))
                    .unwrap();
                db
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

/// Benchmark bulk node insertion (1000 nodes)
fn bench_create_1000_nodes(c: &mut Criterion) {
    c.bench_function("e2e_create_1000_nodes", |b| {
        b.iter_batched(
            setup_database_with_schema,
            |mut db| {
                for i in 0..1000 {
                    db.execute(&format!("CREATE (:Person {{id: {i}, name: 'Person{i}'}})"))
                        .unwrap();
                }
                db
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

/// Benchmark simple MATCH query (return all)
fn bench_match_return_all(c: &mut Criterion) {
    c.bench_function("e2e_match_return_all_1000", |b| {
        b.iter_batched(
            || setup_database_with_nodes(1000),
            |mut db| {
                db.execute(black_box("MATCH (p:Person) RETURN p.id, p.name"))
                    .unwrap()
            },
            criterion::BatchSize::LargeInput,
        );
    });
}

/// Benchmark MATCH query with WHERE filter
fn bench_match_with_filter(c: &mut Criterion) {
    c.bench_function("e2e_match_where_filter_1000", |b| {
        b.iter_batched(
            || setup_database_with_nodes(1000),
            |mut db| {
                db.execute(black_box(
                    "MATCH (p:Person) WHERE p.id > 500 RETURN p.id, p.name",
                ))
                .unwrap()
            },
            criterion::BatchSize::LargeInput,
        );
    });
}

/// Benchmark MATCH query with string equality filter
fn bench_match_string_filter(c: &mut Criterion) {
    c.bench_function("e2e_match_string_filter_1000", |b| {
        b.iter_batched(
            || setup_database_with_nodes(1000),
            |mut db| {
                db.execute(black_box(
                    "MATCH (p:Person) WHERE p.name = 'Person500' RETURN p.id, p.name",
                ))
                .unwrap()
            },
            criterion::BatchSize::LargeInput,
        );
    });
}

/// Benchmark TARGET QUERY: Full end-to-end workflow from spec
/// 1. CREATE NODE TABLE
/// 2. INSERT 1000 nodes
/// 3. MATCH with WHERE filter and RETURN
fn bench_target_query_full_workflow(c: &mut Criterion) {
    c.bench_function("e2e_target_query_full_workflow", |b| {
        b.iter(|| {
            // Step 1: Create database and schema
            let mut db = Database::new();
            db.execute("CREATE NODE TABLE Person (id INT64, name STRING, PRIMARY KEY (id))")
                .unwrap();

            // Step 2: Insert 1000 nodes
            for i in 0..1000 {
                db.execute(&format!("CREATE (:Person {{id: {i}, name: 'Person{i}'}})"))
                    .unwrap();
            }

            // Step 3: Query with filter
            let result = db
                .execute("MATCH (p:Person) WHERE p.id > 500 RETURN p.id, p.name")
                .unwrap();

            black_box(result)
        });
    });
}

/// Benchmark query execution only (pre-populated database)
fn bench_target_query_execution_only(c: &mut Criterion) {
    c.bench_function("e2e_target_query_execution_only", |b| {
        b.iter_batched(
            || setup_database_with_nodes(1000),
            |mut db| {
                db.execute(black_box(
                    "MATCH (p:Person) WHERE p.id > 500 RETURN p.id, p.name",
                ))
                .unwrap()
            },
            criterion::BatchSize::LargeInput,
        );
    });
}

/// Benchmark scaling: query performance with varying data sizes
fn bench_query_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("e2e_query_scaling");

    for size in &[100, 500, 1000, 2000] {
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            b.iter_batched(
                || setup_database_with_nodes(size),
                |mut db| {
                    db.execute(black_box(&format!(
                        "MATCH (p:Person) WHERE p.id > {} RETURN p.id, p.name",
                        size / 2
                    )))
                    .unwrap()
                },
                criterion::BatchSize::LargeInput,
            );
        });
    }

    group.finish();
}

/// Benchmark insert scaling
fn bench_insert_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("e2e_insert_scaling");

    for size in &[100, 500, 1000] {
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            b.iter_batched(
                setup_database_with_schema,
                |mut db| {
                    for i in 0..size {
                        db.execute(&format!("CREATE (:Person {{id: {i}, name: 'Person{i}'}})"))
                            .unwrap();
                    }
                    db
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_create_node_table,
    bench_create_single_node,
    bench_create_1000_nodes,
    bench_match_return_all,
    bench_match_with_filter,
    bench_match_string_filter,
    bench_target_query_full_workflow,
    bench_target_query_execution_only,
    bench_query_scaling,
    bench_insert_scaling
);
criterion_main!(benches);
