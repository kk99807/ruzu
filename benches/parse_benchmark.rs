//! Parser benchmarks for Cypher query parsing.
//!
//! Measures parse performance for different query types:
//! - CREATE NODE TABLE statements
//! - CREATE node statements
//! - MATCH queries with WHERE clauses

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use ruzu::parser;

/// Benchmark CREATE NODE TABLE parsing
fn bench_parse_create_node_table(c: &mut Criterion) {
    let query = "CREATE NODE TABLE Person (id INT64, name STRING, PRIMARY KEY (id))";

    c.bench_function("parse_create_node_table", |b| {
        b.iter(|| parser::parse_query(black_box(query)).unwrap());
    });
}

/// Benchmark CREATE node parsing
fn bench_parse_create_node(c: &mut Criterion) {
    let query = "CREATE (:Person {id: 1, name: 'Alice'})";

    c.bench_function("parse_create_node", |b| {
        b.iter(|| parser::parse_query(black_box(query)).unwrap());
    });
}

/// Benchmark simple MATCH query parsing
fn bench_parse_match_simple(c: &mut Criterion) {
    let query = "MATCH (p:Person) RETURN p.id, p.name";

    c.bench_function("parse_match_simple", |b| {
        b.iter(|| parser::parse_query(black_box(query)).unwrap());
    });
}

/// Benchmark MATCH query with WHERE clause parsing
fn bench_parse_match_with_filter(c: &mut Criterion) {
    let query = "MATCH (p:Person) WHERE p.id > 100 RETURN p.id, p.name";

    c.bench_function("parse_match_with_filter", |b| {
        b.iter(|| parser::parse_query(black_box(query)).unwrap());
    });
}

/// Benchmark parsing with varying query complexity
fn bench_parse_varying_complexity(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse_complexity");

    // Simple CREATE NODE TABLE
    let simple = "CREATE NODE TABLE T (id INT64, PRIMARY KEY (id))";
    group.bench_with_input(
        BenchmarkId::new("create_table", "simple"),
        simple,
        |b, q| {
            b.iter(|| parser::parse_query(black_box(q)).unwrap());
        },
    );

    // CREATE NODE TABLE with multiple columns
    let multi_col = "CREATE NODE TABLE Person (id INT64, name STRING, age INT64, city STRING, PRIMARY KEY (id))";
    group.bench_with_input(
        BenchmarkId::new("create_table", "multi_column"),
        multi_col,
        |b, q| {
            b.iter(|| parser::parse_query(black_box(q)).unwrap());
        },
    );

    // CREATE node with many properties
    let create_node =
        "CREATE (:Person {id: 42, name: 'Alice Bob Carol Dave Eve', age: 30, city: 'New York'})";
    group.bench_with_input(
        BenchmarkId::new("create_node", "multi_prop"),
        create_node,
        |b, q| {
            b.iter(|| parser::parse_query(black_box(q)).unwrap());
        },
    );

    // MATCH with string comparison
    let match_str = "MATCH (p:Person) WHERE p.name = 'Alice' RETURN p.id, p.name";
    group.bench_with_input(
        BenchmarkId::new("match", "string_filter"),
        match_str,
        |b, q| {
            b.iter(|| parser::parse_query(black_box(q)).unwrap());
        },
    );

    group.finish();
}

/// Benchmark batch parsing (parsing many queries sequentially)
fn bench_parse_batch(c: &mut Criterion) {
    let queries: Vec<String> = (0..100)
        .map(|i| format!("CREATE (:Person {{id: {i}, name: 'Person{i}'}})"))
        .collect();

    c.bench_function("parse_batch_100_creates", |b| {
        b.iter(|| {
            for query in &queries {
                parser::parse_query(black_box(query)).unwrap();
            }
        });
    });
}

criterion_group!(
    benches,
    bench_parse_create_node_table,
    bench_parse_create_node,
    bench_parse_match_simple,
    bench_parse_match_with_filter,
    bench_parse_varying_complexity,
    bench_parse_batch
);
criterion_main!(benches);
