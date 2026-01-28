//! Query execution benchmarks for Phase 10.
//!
//! Benchmarks:
//! - T118: Simple match query (MATCH...RETURN)
//! - T119: Filtered match query (MATCH...WHERE...RETURN)
//! - T120: Match with ORDER BY and LIMIT
//! - T121: Aggregation query (COUNT, SUM)

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use ruzu::Database;

/// Helper: Create a database with Person table and N nodes.
fn setup_database_with_nodes(n: i64) -> Database {
    let mut db = Database::new();
    db.execute("CREATE NODE TABLE Person (id INT64, name STRING, age INT64, PRIMARY KEY (id))")
        .unwrap();
    for i in 0..n {
        let age = 20 + (i % 60);
        db.execute(&format!(
            "CREATE (:Person {{id: {}, name: 'Person{}', age: {}}})",
            i, i, age
        ))
        .unwrap();
    }
    db
}

/// Helper: Create a database with Person, Company and WORKS_AT relationship.
fn setup_database_with_relationships(node_count: i64, edge_count: i64) -> Database {
    let mut db = Database::new();
    db.execute("CREATE NODE TABLE Person (id INT64, name STRING, age INT64, PRIMARY KEY (id))")
        .unwrap();
    db.execute("CREATE NODE TABLE Company (id INT64, name STRING, PRIMARY KEY (id))")
        .unwrap();
    db.execute("CREATE REL TABLE WORKS_AT (FROM Person TO Company, since INT64)")
        .unwrap();

    // Create persons
    for i in 0..node_count {
        let age = 20 + (i % 60);
        db.execute(&format!(
            "CREATE (:Person {{id: {}, name: 'Person{}', age: {}}})",
            i, i, age
        ))
        .unwrap();
    }

    // Create companies
    let company_count = edge_count.min(node_count / 10).max(1);
    for i in 0..company_count {
        db.execute(&format!(
            "CREATE (:Company {{id: {}, name: 'Company{}'}})",
            i + 100000, i
        ))
        .unwrap();
    }

    db
}

/// T118: Benchmark simple match query (MATCH...RETURN).
fn bench_simple_match(c: &mut Criterion) {
    let mut group = c.benchmark_group("query_simple_match");

    for size in [100, 1000, 10000].iter() {
        group.throughput(Throughput::Elements(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            b.iter_batched(
                || setup_database_with_nodes(size),
                |mut db| {
                    db.execute(black_box("MATCH (p:Person) RETURN p.id, p.name, p.age"))
                        .unwrap()
                },
                criterion::BatchSize::LargeInput,
            );
        });
    }
    group.finish();
}

/// T119: Benchmark filtered match query (MATCH...WHERE...RETURN).
fn bench_filtered_match(c: &mut Criterion) {
    let mut group = c.benchmark_group("query_filtered_match");

    for size in [100, 1000, 10000].iter() {
        group.throughput(Throughput::Elements(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            b.iter_batched(
                || setup_database_with_nodes(size),
                |mut db| {
                    db.execute(black_box(
                        "MATCH (p:Person) WHERE p.age > 40 RETURN p.id, p.name, p.age",
                    ))
                    .unwrap()
                },
                criterion::BatchSize::LargeInput,
            );
        });
    }
    group.finish();
}

/// T120: Benchmark match with ORDER BY and LIMIT.
fn bench_ordered_match(c: &mut Criterion) {
    let mut group = c.benchmark_group("query_ordered_match");

    for size in [100, 1000, 10000].iter() {
        group.throughput(Throughput::Elements(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            b.iter_batched(
                || setup_database_with_nodes(size),
                |mut db| {
                    db.execute(black_box(
                        "MATCH (p:Person) RETURN p.id, p.name, p.age ORDER BY p.age DESC LIMIT 10",
                    ))
                    .unwrap()
                },
                criterion::BatchSize::LargeInput,
            );
        });
    }
    group.finish();
}

/// T121: Benchmark aggregation query (COUNT).
fn bench_aggregation(c: &mut Criterion) {
    let mut group = c.benchmark_group("query_aggregation");

    for size in [100, 1000, 10000].iter() {
        group.throughput(Throughput::Elements(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            b.iter_batched(
                || setup_database_with_nodes(size),
                |mut db| {
                    db.execute(black_box("MATCH (p:Person) RETURN COUNT(*)"))
                        .unwrap()
                },
                criterion::BatchSize::LargeInput,
            );
        });
    }
    group.finish();
}

/// Benchmark EXPLAIN query (should be fast since no execution).
fn bench_explain(c: &mut Criterion) {
    c.bench_function("query_explain", |b| {
        b.iter_batched(
            || setup_database_with_nodes(100),
            |mut db| {
                db.execute(black_box(
                    "EXPLAIN MATCH (p:Person) WHERE p.age > 30 RETURN p.id, p.name",
                ))
                .unwrap()
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

criterion_group!(
    benches,
    bench_simple_match,
    bench_filtered_match,
    bench_ordered_match,
    bench_aggregation,
    bench_explain,
);
criterion_main!(benches);
