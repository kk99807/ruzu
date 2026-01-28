# Contributing to ruzu

Thank you for your interest in contributing to **ruzu**!

## Project Status

🚧 **Early Development** - We're currently in Phase 0 (Proof of Concept). The project structure and initial implementation are being established.

## Development Principles

Please read our [Constitution](.specify/memory/constitution.md) before contributing. All contributions must adhere to:

1. **Port-First**: Reference the C++ KuzuDB implementation (C:\dev\kuzu or https://github.com/kuzudb/kuzu)
2. **TDD**: Write tests first (Red-Green-Refactor cycle)
3. **Benchmarking**: Add benchmarks for performance-critical code
4. **Rust Best Practices**: Follow clippy, rustfmt, and ecosystem conventions
5. **Safety First**: Correctness before performance

## Getting Started

### Prerequisites

- Rust 1.75 or later
- C++ KuzuDB source code (for reference): `git clone https://github.com/kuzudb/kuzu.git`

### Building

```bash
cargo build
cargo test
cargo clippy --all-targets --all-features
cargo fmt --check
```

### Running Tests

```bash
# Unit tests
cargo test

# Integration tests
cargo test --test '*'

# Benchmarks
cargo bench
```

## Contribution Workflow

1. **Open an issue** first to discuss your proposed change
2. **Fork the repository** and create a feature branch: `feature/###-description`
3. **Write tests first** (TDD - Red phase)
4. **Implement the feature** (Green phase)
5. **Refactor** for clarity and performance (Refactor phase)
6. **Ensure all tests pass** and clippy is clean
7. **Submit a PR** with:
   - Reference to the issue number
   - Description of changes
   - References to C++ implementation (file:line) for ported code
   - Benchmark results if performance-related

## Code Review Checklist

All PRs must pass:

- [ ] All tests pass (`cargo test --all-features`)
- [ ] Zero clippy warnings (`cargo clippy --all-targets --all-features -- -D warnings`)
- [ ] Code formatted (`cargo fmt -- --check`)
- [ ] Documentation builds (`cargo doc --no-deps`)
- [ ] Benchmarks show no regression (>20% slower = rejected)
- [ ] New public APIs have `///` doc comments
- [ ] `unsafe` code has SAFETY comments
- [ ] Code references C++ implementation where applicable

## Commit Messages

Follow [Conventional Commits](https://www.conventionalcommits.org/):

```
feat: add node table storage (#123)
fix: correct buffer pool eviction logic (#124)
test: add property tests for MVCC (#125)
perf: optimize page access with lock-free reads (#126)
docs: update README with installation instructions (#127)
```

## Testing Guidelines

### Test-Driven Development

```rust
// 1. RED: Write the test first (it should fail)
#[test]
fn test_create_node_table() {
    let db = Database::open(":memory:").unwrap();
    let conn = db.connect().unwrap();
    conn.execute("CREATE NODE TABLE Person(name STRING, PRIMARY KEY(name))")
        .unwrap();
    // Test fails because CREATE NODE TABLE not implemented yet
}

// 2. GREEN: Implement minimal code to make it pass
// (implement the feature)

// 3. REFACTOR: Improve the code while keeping tests green
// (optimize, clarify, use better Rust idioms)
```

### Test Organization

```
tests/
├── contract/        # API compatibility tests (e.g., file format)
├── integration/     # Multi-component tests (parse → execute)
├── unit/            # Module tests (also in src/ via #[cfg(test)])
└── benchmarks/      # Performance benchmarks (criterion)
```

## Documentation

- Public APIs must have `///` doc comments
- Include examples in doc comments (use `rust,ignore` if not yet implemented)
- Complex algorithms should reference C++ implementation:
  ```rust
  /// Implements second-chance page eviction.
  ///
  /// Reference: C++ implementation at
  /// C:\dev\kuzu\src\storage\buffer_manager\buffer_manager.cpp:142-167
  ```

## Questions?

Open an issue or discussion on GitHub!

## License

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
