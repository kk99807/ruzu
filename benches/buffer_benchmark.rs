//! Buffer pool benchmarks.
//!
//! Measures buffer pool performance for:
//! - Page allocation
//! - Pin/unpin operations
//! - Cache hit/miss scenarios
//! - Eviction under pressure

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use ruzu::storage::{BufferPool, DiskManager, PAGE_SIZE};
use tempfile::TempDir;

/// Benchmark page allocation
fn bench_page_allocation(c: &mut Criterion) {
    let mut group = c.benchmark_group("buffer_pool_allocation");

    for capacity in &[64, 128, 256, 512] {
        let temp_dir = TempDir::new().expect("create temp dir");
        let db_path = temp_dir.path().join("test.db");
        let disk_manager = DiskManager::new(&db_path).expect("create disk manager");
        let pool = BufferPool::new(*capacity, disk_manager).expect("create pool");

        group.throughput(Throughput::Elements(1));
        group.bench_with_input(BenchmarkId::from_parameter(capacity), capacity, |b, _| {
            b.iter(|| {
                let handle = pool.new_page().expect("allocate page");
                black_box(handle.page_id())
            });
        });
    }

    group.finish();
}

/// Benchmark sequential page access (cache hits)
fn bench_sequential_access(c: &mut Criterion) {
    let mut group = c.benchmark_group("buffer_pool_sequential");

    let temp_dir = TempDir::new().expect("create temp dir");
    let db_path = temp_dir.path().join("test.db");
    let disk_manager = DiskManager::new(&db_path).expect("create disk manager");
    let pool = BufferPool::new(256, disk_manager).expect("create pool");

    // Pre-allocate pages
    let mut page_ids = Vec::new();
    for _ in 0..100 {
        let handle = pool.new_page().expect("allocate page");
        page_ids.push(handle.page_id());
    }

    group.throughput(Throughput::Elements(100));
    group.bench_function("100_pages_sequential", |b| {
        b.iter(|| {
            for &page_id in &page_ids {
                let handle = pool.pin(page_id).expect("pin page");
                black_box(handle.data()[0]);
            }
        });
    });

    group.finish();
}

/// Benchmark random page access
fn bench_random_access(c: &mut Criterion) {
    let mut group = c.benchmark_group("buffer_pool_random");

    let temp_dir = TempDir::new().expect("create temp dir");
    let db_path = temp_dir.path().join("test.db");
    let disk_manager = DiskManager::new(&db_path).expect("create disk manager");
    let pool = BufferPool::new(128, disk_manager).expect("create pool");

    // Pre-allocate pages
    let mut page_ids = Vec::new();
    for _ in 0..100 {
        let handle = pool.new_page().expect("allocate page");
        page_ids.push(handle.page_id());
    }
    pool.flush_all().expect("flush");

    // Generate pseudo-random access pattern
    let mut access_pattern = Vec::new();
    let mut seed: u32 = 12345;
    for _ in 0..1000 {
        seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
        access_pattern.push(page_ids[(seed as usize) % page_ids.len()]);
    }

    group.throughput(Throughput::Elements(1000));
    group.bench_function("1000_random_accesses", |b| {
        b.iter(|| {
            for &page_id in &access_pattern {
                let handle = pool.pin(page_id).expect("pin page");
                black_box(handle.data()[0]);
            }
        });
    });

    group.finish();
}

/// Benchmark page write operations
fn bench_page_write(c: &mut Criterion) {
    let mut group = c.benchmark_group("buffer_pool_write");

    let temp_dir = TempDir::new().expect("create temp dir");
    let db_path = temp_dir.path().join("test.db");
    let disk_manager = DiskManager::new(&db_path).expect("create disk manager");
    let pool = BufferPool::new(256, disk_manager).expect("create pool");

    // Pre-allocate pages
    let mut page_ids = Vec::new();
    for _ in 0..50 {
        let handle = pool.new_page().expect("allocate page");
        page_ids.push(handle.page_id());
    }

    group.throughput(Throughput::Bytes(PAGE_SIZE as u64 * 50));
    group.bench_function("50_pages_write", |b| {
        b.iter(|| {
            for &page_id in &page_ids {
                let mut handle = pool.pin(page_id).expect("pin page");
                // Write full page
                for byte in handle.data_mut().iter_mut() {
                    *byte = 0x42;
                }
            }
        });
    });

    group.finish();
}

/// Benchmark eviction under memory pressure
fn bench_eviction_pressure(c: &mut Criterion) {
    let mut group = c.benchmark_group("buffer_pool_eviction");

    // Small pool to force evictions
    let pool_size = 32;
    let access_pages = 100; // More pages than pool can hold

    let temp_dir = TempDir::new().expect("create temp dir");
    let db_path = temp_dir.path().join("test.db");
    let disk_manager = DiskManager::new(&db_path).expect("create disk manager");
    let pool = BufferPool::new(pool_size, disk_manager).expect("create pool");

    // Pre-allocate pages (will cause evictions)
    let mut page_ids = Vec::new();
    for _ in 0..access_pages {
        let handle = pool.new_page().expect("allocate page");
        page_ids.push(handle.page_id());
    }
    pool.flush_all().expect("flush");

    group.throughput(Throughput::Elements(access_pages as u64));
    group.bench_function("eviction_heavy", |b| {
        b.iter(|| {
            // Access all pages sequentially, causing many evictions
            for &page_id in &page_ids {
                let handle = pool.pin(page_id).expect("pin page");
                black_box(handle.data()[0]);
            }
        });
    });

    group.finish();
}

/// Benchmark cache hit rate under working set
fn bench_working_set(c: &mut Criterion) {
    let mut group = c.benchmark_group("buffer_pool_working_set");

    let pool_size = 64;
    let temp_dir = TempDir::new().expect("create temp dir");
    let db_path = temp_dir.path().join("test.db");
    let disk_manager = DiskManager::new(&db_path).expect("create disk manager");
    let pool = BufferPool::new(pool_size, disk_manager).expect("create pool");

    // Create pages
    let mut page_ids = Vec::new();
    for _ in 0..100 {
        let handle = pool.new_page().expect("allocate page");
        page_ids.push(handle.page_id());
    }
    pool.flush_all().expect("flush");

    // Test different working set sizes
    for working_set_size in &[32, 48, 64, 80] {
        let working_set: Vec<_> = page_ids.iter().take(*working_set_size).copied().collect();

        group.throughput(Throughput::Elements(*working_set_size as u64 * 100));
        group.bench_with_input(
            BenchmarkId::from_parameter(working_set_size),
            working_set_size,
            |b, _| {
                b.iter(|| {
                    // Access working set multiple times
                    for _ in 0..100 {
                        for &page_id in &working_set {
                            let handle = pool.pin(page_id).expect("pin page");
                            black_box(handle.data()[0]);
                        }
                    }
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_page_allocation,
    bench_sequential_access,
    bench_random_access,
    bench_page_write,
    bench_eviction_pressure,
    bench_working_set
);
criterion_main!(benches);
