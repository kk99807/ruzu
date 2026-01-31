<!--
Sync Impact Report:
- Version: 0.0.0 → 1.0.0
- Change Type: Initial creation (MAJOR version for first ratification)
- Principles Established:
  1. Port-First (Reference Implementation)
  2. TDD with Red-Green-Refactor
  3. Benchmarking & Performance Tracking
  4. Rust Best Practices & Idioms
  5. Safety & Correctness Over Performance (Initially)
- New Sections:
  - Development Workflow
  - Quality Gates
- Templates Status:
  - ✅ plan-template.md: Constitution Check section aligns with all 5 principles
  - ✅ spec-template.md: User stories support independent testing (TDD compatible)
  - ✅ tasks-template.md: Test-first workflow with Red-Green-Refactor cycle
- Follow-up: None - all placeholders filled
- Date: 2025-12-05
-->

# ruzu Constitution

**ruzu** (Rust + Kuzu) - A pure Rust port of the KuzuDB graph database focused on correctness, safety, and incremental performance optimization for analytical workloads.

## Core Principles

### I. Port-First (Reference Implementation)

**This is a port of KuzuDB (C++), not greenfield development.**

- MUST reference the C++ implementation at C:\dev\kuzu as the authoritative design
- MUST preserve core algorithms and data structures from the reference implementation
- MUST NOT research alternative approaches when the C++ implementation provides a clear solution
- MAY deviate from C++ implementation only when:
  - Rust idioms provide equivalent functionality with better safety guarantees
  - Leveraging existing Rust ecosystem libraries (e.g., Apache Arrow, DataFusion) reduces complexity
  - C++ approach is Windows/platform-specific and Rust provides cross-platform alternative
- MUST document any significant deviations from reference implementation with rationale

**Rationale**: The C++ KuzuDB codebase (~326K LOC) is a proven, well-architected implementation. Reusing its design decisions accelerates development and provides a clear specification. This is NOT a research project to find "better" graph database architectures.

### II. TDD with Red-Green-Refactor (NON-NEGOTIABLE)

**All features MUST follow strict Test-Driven Development.**

- **Red**: Write tests FIRST, verify they FAIL
- **Green**: Implement minimal code to make tests PASS
- **Refactor**: Improve code while keeping tests GREEN

**Workflow:**
1. Write test(s) for new functionality
2. Run tests → confirm FAILURE (Red)
3. Implement feature with minimal code
4. Run tests → confirm SUCCESS (Green)
5. Refactor for clarity, performance, Rust idioms
6. Run tests → confirm still GREEN
7. Commit

**Test Categories (in order of priority):**
- **Contract tests**: API contracts, file format compatibility
- **Integration tests**: Multi-component workflows (e.g., parse → plan → execute)
- **Unit tests**: Individual functions and modules

**Exceptions**: Proof-of-concept code may defer tests, but MUST NOT merge to main without tests.

**Rationale**: Graph databases have complex invariants (e.g., referential integrity, transaction isolation). TDD ensures correctness and prevents regressions during the port. The C++ codebase has tests we can reference for expected behavior.

### III. Benchmarking & Performance Tracking

**Performance MUST be measurable and tracked continuously.**

- MUST establish baseline benchmarks from C++ KuzuDB early (Phase 0/1)
- MUST use Rust's `criterion` crate for micro-benchmarks
- MUST track performance against C++ baseline:
  - PoC (Phase 0): 5-10x slower is acceptable
  - MVP (Phase 3): 2-3x slower is acceptable
  - v1.0: 0.8-1.2x (parity) is the goal
- MUST use LDBC Social Network Benchmark (SF-1) for macro-benchmarks
- MUST integrate benchmarks into CI/CD pipeline
- MUST alert on >20% performance regression in PRs

**Benchmark Suite:**
1. Microbenchmarks: Page access, buffer pool operations, type conversions
2. Query benchmarks: Simple scans, 1-hop/2-hop traversals, aggregations, joins
3. LDBC benchmarks: Standard graph database workloads (I1-I14 queries)

**Rationale**: Performance is a primary motivation for this port (alongside safety). Continuous tracking ensures we don't unknowingly introduce regressions and validates that Rust can match C++ performance.

### IV. Rust Best Practices & Idioms

**Code MUST follow Rust community standards and leverage the ecosystem.**

- MUST pass `cargo clippy` with zero warnings
- Preferably, make the fixes in code. Clippy fixes MUST be as narrow as possible: use targeted `#[allow(...)]` on the specific item (function, block, or statement) rather than module-wide or crate-wide suppression. Blanket `#[allow(...)]` at module or crate level requires explicit approval from the developer/maintainer before applying.
- MUST format with `rustfmt` (standard configuration)
- MUST use `cargo-deny` to check dependencies for security/licensing issues
- MUST prefer safe Rust; justify any `unsafe` blocks with SAFETY comments
- MUST use Rust idioms:
  - `Result<T, E>` for error handling (no panics in library code except `unimplemented!()` during development)
  - Ownership and borrowing over reference counting where possible
  - `Arc<T>` and `Mutex<T>`/`RwLock<T>` for shared mutable state
  - Iterators and combinators over manual loops
  - Traits for polymorphism over enum dispatch where appropriate
- MUST document public APIs with `///` doc comments
- MUST run `cargo test --all-features` and `cargo doc --no-deps` without errors

**Key Crates to Leverage:**
- **Apache Arrow**: Columnar data format (replaces custom C++ vectors)
- **DataFusion**: Query execution engine and optimizer (reuse ~30-40% of code)
- **pest** or **nom**: Parser generation (replaces ANTLR4)
- **memmap2**: Memory-mapped I/O
- **parking_lot**: Faster synchronization primitives
- **crossbeam**: Lock-free data structures

**Rationale**: Rust's ecosystem has mature database building blocks. Using them reduces code to write/maintain and benefits from community optimization efforts. Following Rust conventions ensures code is maintainable by any Rust developer.

### V. Safety & Correctness Over Performance (Initially)

**Correctness is non-negotiable; performance is iterative.**

- MUST prioritize correctness over performance in MVP phases
- MUST use simple, correct algorithms before optimizing (e.g., LRU eviction before clock algorithm)
- MUST validate correctness with property-based testing (via `proptest` or `quickcheck`) for critical invariants
- MAY defer performance optimizations (SIMD, lock-free algorithms) to post-MVP
- MUST NOT introduce unsafe code solely for performance without proving safety

**Critical Invariants to Test:**
1. **Referential integrity**: Node/relationship IDs always valid
2. **Transaction isolation**: MVCC guarantees serializable isolation
3. **Crash recovery**: WAL replay produces identical state
4. **Buffer pool**: No use-after-free, no double-eviction
5. **Concurrency**: No data races (verified by Miri and ThreadSanitizer)

**Optimization Workflow:**
1. Implement correct version (safe Rust)
2. Benchmark and profile
3. Identify bottlenecks (use `cargo flamegraph`)
4. Optimize hot paths only
5. Re-benchmark to confirm improvement
6. Consider unsafe if and only if profile-guided

**Rationale**: The C++ implementation is heavily optimized. We cannot compete immediately. By establishing correctness first, we build a foundation for incremental optimization. Rust's safety guarantees help us avoid entire classes of bugs.

## Development Workflow

### Phase Structure

All features follow the SpecKit phased approach:

1. **Phase 0: Proof of Concept** (6-8 weeks)
   - Minimal end-to-end workflow (parse → execute simple query)
   - Validate feasibility and identify technical risks
   - Establish baseline benchmarks

2. **Phase 1: Persistent Storage** (6-8 weeks)
   - Disk-based storage with buffer management
   - WAL and catalog persistence

3. **Phase 2: Query Engine** (8-10 weeks)
   - Full query pipeline with optimizations
   - DataFusion integration
   - Graph-specific operators

4. **Phase 3: Transactions & Polish** (4-6 weeks)
   - MVCC transactions
   - Checkpointing
   - Performance tuning

5. **Phase 4+: Post-MVP Enhancements**
   - Concurrent transactions
   - Advanced compression
   - Additional Cypher features

### Branching Strategy

- **main**: Production-ready code only
- **develop**: Integration branch for completed features
- **feature/###-name**: Individual feature branches (### = issue number)
- All work MUST go through PR review before merging to develop
- develop → main merges only after full phase validation

### Commit Discipline

- Commits MUST be atomic (single logical change)
- Commit messages MUST follow conventional commits:
  - `feat:` new feature
  - `fix:` bug fix
  - `refactor:` code restructuring without behavior change
  - `test:` adding or updating tests
  - `perf:` performance improvement
  - `docs:` documentation changes
  - `chore:` build, tooling, dependencies
- MUST reference issue numbers: `feat: add node table storage (#123)`

## Quality Gates

### Pre-Merge Checklist (All PRs)

- [ ] All tests pass (`cargo test --all-features`)
- [ ] Zero clippy warnings (`cargo clippy --all-targets --all-features -- -D warnings`)
- [ ] Code formatted (`cargo fmt -- --check`)
- [ ] Documentation builds (`cargo doc --no-deps`)
- [ ] Benchmarks run without regression (>20% slower = block merge)
- [ ] New public APIs have doc comments
- [ ] Unsafe code has SAFETY comments explaining invariants
- [ ] Code references C++ implementation (file:line) for ported logic

### Phase Completion Gates

**Phase 0 Gate:**
- [ ] Simple query executes end-to-end (parse → plan → execute)
- [ ] Baseline benchmarks established and documented
- [ ] Performance within 10x of C++ KuzuDB
- [ ] Decision: Continue or pivot?

**Phase 1 Gate:**
- [ ] Data persists to disk and survives restart
- [ ] WAL replay recovers from crash
- [ ] Buffer pool evicts and reloads pages correctly
- [ ] Performance within 5x of C++ KuzuDB

**Phase 2 Gate:**
- [ ] All MVP Cypher queries execute correctly
- [ ] LDBC SF-1 queries run successfully (correctness)
- [ ] Performance within 3x of C++ KuzuDB

**Phase 3 Gate (MVP Release):**
- [ ] ACID transactions work (single-writer MVCC)
- [ ] All acceptance criteria from spec.md met
- [ ] Performance within 2x of C++ KuzuDB
- [ ] 200+ tests passing
- [ ] No data corruption in crash tests
- [ ] Documentation complete (README, API docs, examples)

### Testing Requirements

**Minimum Test Coverage:**
- Phase 0: 50% line coverage (focus on critical paths)
- Phase 1: 70% line coverage
- Phase 2: 80% line coverage
- Phase 3+: 85% line coverage (use `cargo-tarpaulin`)

**Test Organization:**
```
tests/
├── contract/        # API compatibility, file format
├── integration/     # Multi-component workflows
├── unit/            # Module-level tests (also in src/ via #[cfg(test)])
└── benchmarks/      # Criterion benchmarks
```

**Test Naming:**
- Contract: `test_<component>_contract.rs`
- Integration: `test_<workflow>.rs`
- Unit: `<module>_tests.rs` or `#[cfg(test)] mod tests`

## Additional Constraints

### Scope Management

**MVP Scope (explicitly IN):**
- Core database functionality (nodes, relationships, properties)
- Minimal Cypher subset (MATCH, CREATE, WHERE, RETURN)
- Disk-based storage with buffer pool
- Single-writer MVCC transactions
- Basic indexes (hash indexes only)

**Explicitly OUT of MVP:**
- Extensions framework
- Parquet import/export
- Full-text search
- Vector indices
- Advanced Cypher (MERGE, OPTIONAL MATCH, subqueries)
- Multi-database support
- User management
- Concurrent write transactions

**Rationale**: MVP must ship in 4-6 months with 2-3 engineers. Scope creep is the primary risk to timeline.

### Dependency Policy

- MUST use stable crates (1.0+ or widely adopted pre-1.0 like tokio)
- MUST review licenses (Apache-2.0, MIT preferred)
- MUST run `cargo-deny` to check for vulnerabilities
- SHOULD minimize dependencies (use workspace to deduplicate)
- MAY vendor critical dependencies if abandonment risk

### Platform Support

**MVP Targets:**
- Linux (x86_64, aarch64)
- macOS (x86_64, aarch64)
- Windows (x86_64)

**Deferred:**
- WASM
- Mobile (iOS/Android)

## Governance

### Constitution Authority

- This constitution SUPERSEDES all other project practices
- All design decisions MUST be justified against these principles
- PRs violating principles MUST be rejected or explicitly justified in "Complexity Tracking" section of plan.md

### Amendment Procedure

1. Propose amendment via GitHub issue with `constitution-amendment` label
2. Discuss rationale and impact on existing work
3. Require approval from project maintainers (unanimous for MAJOR changes)
4. Update constitution with version bump per semantic versioning rules
5. Propagate changes to dependent templates (plan, spec, tasks)
6. Document in Sync Impact Report (HTML comment at top of this file)

### Version Semantics

- **MAJOR**: Backward incompatible principle changes (e.g., removing TDD requirement)
- **MINOR**: New principles added or material expansions (e.g., adding security requirements)
- **PATCH**: Clarifications, wording improvements, typo fixes

### Compliance Review

- Weekly: Review PRs for principle adherence
- Monthly: Audit codebase for technical debt violating principles
- Phase gates: Formal constitution compliance check before phase advancement

### Complexity Justification

If a feature requires violating a principle:
1. Document in plan.md "Complexity Tracking" table
2. Explain why needed
3. Explain why simpler alternative was rejected
4. Get explicit approval from maintainers
5. Create follow-up issue to remediate if possible

**Version**: 1.0.0 | **Ratified**: 2025-12-05 | **Last Amended**: 2025-12-05
