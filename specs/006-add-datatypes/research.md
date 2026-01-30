# Research: Add Additional Datatypes

**Feature**: 006-add-datatypes
**Date**: 2026-01-30

## Research Items

### R1: Float Literal Syntax in PEST Grammar

**Decision**: Support `RegularDecimalReal` format only (e.g., `3.14`, `-0.5`, `.5`, `42.0`). No scientific notation.

**Rationale**: The KuzuDB C++ reference supports both `RegularDecimalReal` and `ExponentDecimalReal` (scientific notation like `1.5e10`). However, scientific notation is rarely used in interactive Cypher queries for graph data. Adding it later is trivial (grammar-only change). Keeping the grammar simple reduces parser complexity for the PoC phase.

**Alternatives Considered**:
- Full scientific notation (`1.5e10`): Deferred — not needed for the use cases described in the spec (prices, coordinates, measurements, scores). Can be added as a follow-up.
- Integer-only with decimal point required: Rejected — too restrictive (can't write `42.0` as just `42` for a float column).

**Grammar Rule**:
```pest
float_literal = @{ "-"? ~ ASCII_DIGIT* ~ "." ~ ASCII_DIGIT+ }
```
This accepts: `3.14`, `-0.5`, `.5`, `42.0`, `0.0`. Does NOT accept: `42` (integer), `1e10` (scientific).

**Integer-to-Float Promotion**: When an integer literal (e.g., `42`) is used with a FLOAT64 column in a WHERE clause, the execution layer promotes it to `42.0` at comparison time. This is handled in the executor, not the grammar.

### R2: Boolean Literal Syntax

**Decision**: Case-insensitive `true` and `false` keywords in the grammar.

**Rationale**: KuzuDB C++ uses case-insensitive boolean keywords. The existing grammar already has a `bool_literal` rule (line 191) for COPY options that does exactly this: `^"TRUE" | ^"FALSE"`. We reuse this rule for general literals.

**Alternatives Considered**:
- Accept `1`/`0`, `yes`/`no`: Rejected — spec explicitly limits to true/false only, matching KuzuDB's grammar-level behavior (the C++ code accepts 1/0 only in string-to-bool casting, not in the grammar).

### R3: NaN and Infinity Handling

**Decision**: Reject NaN and Infinity at parse time with a clear error message.

**Rationale**: The spec explicitly requires rejection (FR-011). IEEE 754 special values cause problems in comparisons (NaN != NaN), sorting, and serialization. KuzuDB C++ also rejects these.

**Implementation**: After `str::parse::<f64>()`, check `f64::is_finite()`. If false, return a parse error.

### R4: CSV Boolean Parsing Alignment

**Decision**: Tighten CSV bool parsing to accept only case-insensitive `true`/`false`, removing support for `1`/`0`/`yes`/`no`/`t`/`f`.

**Rationale**: The spec (edge case section) explicitly states: "The system should accept case-insensitive true/false and reject other values with a clear error." The existing CSV code accepts extra formats (`1`, `0`, `yes`, `no`, `t`, `f`) which contradicts the spec. KuzuDB C++ accepts `1`/`0` and `t`/`f` in its string-to-bool cast, but since the spec explicitly narrows this, we follow the spec.

**Risk**: Users with existing CSVs using `1`/`0` format will get errors. This is acceptable because the feature is new (no existing BOOL columns exist yet).

### R5: Type Comparison Cross-Promotion

**Decision**: Support Int64-to-Float64 promotion in WHERE clause comparisons only. No other cross-type promotions.

**Rationale**: The spec edge case says: "The system should handle this by treating the integer literal as a float for comparison." KuzuDB C++ supports implicit INT→DOUBLE with a cost-based system. For simplicity, we handle the single case of Int64 literal compared to Float64 column value by promoting the Int64 to Float64 at comparison time.

**Implementation**: In the executor's `evaluate_expression()`, when comparing a `Value::Int64` against a `Value::Float64` column, convert the Int64 to Float64 before comparison.

### R6: Display Format for New Types

**Decision**:
- Float64: Use Rust's default `Display` formatting (e.g., `3.14`, `42.0`)
- Bool: Display as `true` / `false` (lowercase, matching Rust convention)

**Rationale**: KuzuDB C++ displays booleans as `True`/`False` (capitalized). However, Cypher convention and most modern systems use lowercase. Since our grammar accepts case-insensitive input, lowercase output is simpler and idiomatic.

## Summary

All NEEDS CLARIFICATION items resolved. No new dependencies required. Changes are limited to parser grammar, AST, parser builder, and execution paths. Storage, serialization, CSV import, and comparison logic already support the new types.
