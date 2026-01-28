# Feature Specification: Optimize Peak Memory During CSV Import

**Feature Branch**: `004-optimize-csv-memory`
**Created**: 2025-12-07
**Status**: Draft
**Input**: User description: "Optimize peak memory during CSV import. See C:\dev\ruzu\specs\003-optimize-csv-import\spec.md, Lines 1-38."

---

## Background & Context

This feature addresses the memory usage gap identified in feature `003-optimize-csv-import`. While that feature achieved excellent throughput (8.9M nodes/sec, 3.8M edges/sec), memory usage did not meet the target:

| Metric | Target | Achieved | Gap |
|--------|--------|----------|-----|
| Memory usage (1GB import) | <500MB | ~4.5GB (extrapolated) | 9x over budget |

**Root Cause** (from 003 analysis): The current architecture loads all parsed rows into memory before returning to caller, resulting in a 4.4x memory amplification ratio.

**Proposed Strategies** (from 003 spec, lines 32-38):
1. **Streaming Writes**: Write batches to storage as they complete instead of collecting all rows
2. **Row Buffer Recycling**: Reuse allocated `Vec<Value>` buffers across batches
3. **Direct-to-Page Parsing**: Parse CSV fields directly into page-format storage

---

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Import Large CSV Files Within Memory Budget (Priority: P1)

As a data engineer, I want to import CSV files of 1GB or larger without exceeding a 500MB memory footprint, so that I can run imports on memory-constrained systems or alongside other workloads.

**Why this priority**: This is the core value proposition - enabling large imports on limited-memory systems. Without this, users cannot import datasets larger than available RAM.

**Independent Test**: Can be tested by importing a 1GB CSV file while monitoring peak memory usage with system tools. Success is achieved when the import completes with peak memory under 500MB.

**Acceptance Scenarios**:

1. **Given** a 1GB CSV file containing 5 million node records, **When** I run the COPY import command, **Then** the peak memory usage stays below 500MB throughout the import.

2. **Given** a 1GB CSV file containing 10 million edge records, **When** I run the COPY import command, **Then** the peak memory usage stays below 500MB throughout the import.

3. **Given** a system with only 512MB of available RAM, **When** I import a 1GB CSV file, **Then** the import completes successfully without out-of-memory errors.

---

### User Story 2 - Maintain Import Throughput Performance (Priority: P1)

As a data engineer, I want memory optimizations to not significantly degrade import speed, so that I don't have to trade unacceptable performance for memory efficiency.

**Why this priority**: Throughput was a major achievement of 003. Users won't accept a 10x memory improvement if it comes with a 10x slowdown.

**Independent Test**: Run the same benchmark suite used in 003 and compare throughput results. Success is throughput within 20% of 003 results.

**Acceptance Scenarios**:

1. **Given** the memory-optimized import implementation, **When** I import a large node CSV file, **Then** throughput is at least 7M nodes/sec (within 20% of the 8.9M achieved in 003).

2. **Given** the memory-optimized import implementation, **When** I import a large edge CSV file, **Then** throughput is at least 3M edges/sec (within 20% of the 3.8M achieved in 003).

---

### User Story 3 - Predictable Memory Usage Regardless of File Size (Priority: P2)

As a system administrator, I want CSV import memory usage to be bounded and predictable regardless of input file size, so that I can confidently allocate resources and run imports without monitoring.

**Why this priority**: Predictability enables automation and reduces operational burden. However, the core memory reduction (P1) must come first.

**Independent Test**: Import files of varying sizes (100MB, 500MB, 1GB, 5GB) and verify memory usage stays within a narrow band (e.g., 400-500MB) for all sizes.

**Acceptance Scenarios**:

1. **Given** CSV files of sizes 100MB, 500MB, 1GB, and 5GB, **When** I import each file, **Then** peak memory usage for all imports falls within a 100MB range (demonstrating size-independent memory usage).

2. **Given** a 5GB CSV file, **When** I import it, **Then** peak memory usage does not exceed 500MB.

---

### User Story 4 - Progress Visibility During Streaming Import (Priority: P3)

As a data engineer, I want to see import progress during long-running imports, so that I can estimate completion time and verify the import is proceeding correctly.

**Why this priority**: Good UX but not essential for core functionality. The existing progress callback mechanism should continue to work.

**Independent Test**: Start a large import and verify progress updates are displayed at regular intervals throughout the import process.

**Acceptance Scenarios**:

1. **Given** a streaming import of a large CSV file, **When** the import is running, **Then** progress updates showing rows processed are provided at least every 100,000 rows.

2. **Given** a streaming import, **When** I observe progress updates, **Then** the reported row counts increase monotonically and reflect actual progress.

---

### Edge Cases

- What happens when a batch write fails mid-import? The system should support both rollback-all and continue-partial modes based on user configuration.
- How does the system handle CSV files with highly variable row sizes (some rows with very large string fields)? Memory usage should remain bounded regardless of row size variation.
- What happens if disk I/O becomes a bottleneck during streaming writes? Throughput may degrade but memory should remain bounded.
- How does memory usage behave with concurrent imports? Each import should maintain its own memory budget.

---

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST complete CSV imports with peak memory usage below 500MB for files up to 5GB in size.

- **FR-002**: System MUST write parsed data to storage incrementally as batches complete, rather than accumulating all rows in memory.

- **FR-003**: System MUST recycle row buffers across batches to minimize allocation overhead.

- **FR-004**: System MUST maintain import throughput of at least 7M nodes/sec and 3M edges/sec.

- **FR-005**: System MUST continue to support the existing COPY FROM syntax without changes.

- **FR-006**: System MUST provide progress callbacks during streaming import at intervals no greater than every 100,000 rows.

- **FR-007**: System MUST handle import failures gracefully, reporting which rows failed and allowing the import to continue or abort based on configuration.

- **FR-008**: System MUST support the existing error handling options (continue-on-error, abort-on-error) from the 002 implementation.

---

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Peak memory usage during 1GB CSV import is below 500MB (measured via heap profiler or system memory monitoring).

- **SC-002**: Peak memory usage during 5GB CSV import is below 500MB.

- **SC-003**: Node import throughput is at least 7M nodes/sec (within 20% of 003 baseline of 8.9M).

- **SC-004**: Edge import throughput is at least 3M edges/sec (within 20% of 003 baseline of 3.8M).

- **SC-005**: Memory usage variance across file sizes (100MB to 5GB) is less than 100MB, demonstrating size-independent memory behavior.

- **SC-006**: All existing CSV import tests continue to pass (backward compatibility).

---

## Assumptions

- The 4.4x memory amplification ratio identified in 003 is accurate and represents the primary optimization target.
- Streaming writes to the existing page-based storage system are feasible without major architectural changes.
- Disk I/O will not become a bottleneck that negates memory optimization benefits (modern SSDs assumed).
- The existing batch processing infrastructure can be adapted for streaming without complete rewrite.
- Buffer recycling can be implemented without introducing significant complexity or bugs.
- The 500MB target is inclusive of all memory used by the import operation, including buffers, parsed data, and storage engine overhead.

---

## Dependencies

- Existing CSV parsing infrastructure from 003-optimize-csv-import
- Page-based storage system from 002-persistent-storage
- Buffer pool and WAL infrastructure from 002-persistent-storage
- Benchmarking framework established in 003 for performance validation
