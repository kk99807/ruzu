//! Relationship persistence benchmarks.
//!
//! Measures performance of:
//! - Database open time with varying relationship counts
//! - Relationship query performance before and after restart

use std::path::PathBuf;

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use ruzu::{Database, DatabaseConfig};

/// Helper: create a temp directory path for benchmarks.
fn temp_db_path(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ruzu_bench_{name}_{}", std::process::id()));
    if dir.exists() {
        std::fs::remove_dir_all(&dir).ok();
    }
    dir
}

/// Helper: create a persistent database with N nodes and M relationships.
fn setup_persistent_db(path: &std::path::Path, num_nodes: usize, num_rels: usize) {
    let mut db = Database::open(path, DatabaseConfig::default()).unwrap();

    db.execute("CREATE NODE TABLE Person (id INT64, name STRING, PRIMARY KEY (id))")
        .unwrap();
    db.execute("CREATE REL TABLE Knows (FROM Person TO Person, since INT64)")
        .unwrap();

    for i in 0..num_nodes {
        db.execute(&format!(
            "CREATE (:Person {{id: {i}, name: 'Person{i}'}})"
        ))
        .unwrap();
    }

    for i in 0..num_rels {
        let src = i % num_nodes;
        let dst = (i + 1) % num_nodes;
        db.execute(&format!(
            "MATCH (a:Person {{id: {src}}}), (b:Person {{id: {dst}}}) CREATE (a)-[:Knows {{since: {}}}]->(b)",
            2000 + (i % 25)
        ))
        .unwrap();
    }

    db.close().unwrap();
}

/// T055: Benchmark database open time with varying relationship counts.
fn bench_open_time_varying_rels(c: &mut Criterion) {
    let mut group = c.benchmark_group("rel_persist/open_time");
    group.sample_size(10);

    // Note: relationship metadata must fit within a single 4KB page (~4092 bytes).
    // Each relationship with properties takes ~50-100 bytes serialized, so we keep
    // counts well within the page limit.
    for &num_rels in &[0, 10, 25, 50] {
        let path = temp_db_path(&format!("open_{num_rels}"));
        let num_nodes = if num_rels == 0 { 10 } else { (num_rels + 1).max(10) };
        setup_persistent_db(&path, num_nodes, num_rels);

        group.bench_with_input(
            BenchmarkId::from_parameter(num_rels),
            &num_rels,
            |b, _| {
                b.iter(|| {
                    let db = Database::open(black_box(&path), DatabaseConfig::default()).unwrap();
                    black_box(&db);
                    // Explicit drop to close the database
                    drop(db);
                });
            },
        );

        std::fs::remove_dir_all(&path).ok();
    }

    group.finish();
}

/// T056: Benchmark relationship query performance before/after restart.
fn bench_query_after_restart(c: &mut Criterion) {
    let mut group = c.benchmark_group("rel_persist/query_after_restart");
    group.sample_size(10);

    let path = temp_db_path("query_restart");
    let num_nodes = 30;
    let num_rels = 25;
    setup_persistent_db(&path, num_nodes, num_rels);

    // Benchmark querying relationships after database restart
    group.bench_function("query_rels_after_open", |b| {
        b.iter_batched(
            || Database::open(&path, DatabaseConfig::default()).unwrap(),
            |mut db| {
                let result = db
                    .execute(black_box(
                        "MATCH (a:Person)-[k:Knows]->(b:Person) RETURN a.name, b.name",
                    ))
                    .unwrap();
                black_box(result)
            },
            criterion::BatchSize::LargeInput,
        );
    });

    // Benchmark querying relationships with filter after restart
    group.bench_function("query_rels_filtered_after_open", |b| {
        b.iter_batched(
            || Database::open(&path, DatabaseConfig::default()).unwrap(),
            |mut db| {
                let result = db
                    .execute(black_box(
                        "MATCH (a:Person)-[k:Knows]->(b:Person) WHERE k.since > 2010 RETURN a.name, k.since, b.name",
                    ))
                    .unwrap();
                black_box(result)
            },
            criterion::BatchSize::LargeInput,
        );
    });

    group.finish();
    std::fs::remove_dir_all(&path).ok();
}

criterion_group!(
    benches,
    bench_open_time_varying_rels,
    bench_query_after_restart,
);
criterion_main!(benches);
