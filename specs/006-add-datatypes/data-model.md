# Data Model: Add Additional Datatypes

**Feature**: 006-add-datatypes
**Date**: 2026-01-30

## Entities

### DataType (Extended)

**Location**: `src/types/value.rs` — `DataType` enum

| Variant | Status | Byte Size | Numeric | Orderable |
|---------|--------|-----------|---------|-----------|
| Int64 | Existing | 8 | Yes | Yes |
| Float32 | Existing (unused) | 4 | Yes | Yes |
| **Float64** | **Enable** | 8 | Yes | Yes |
| **Bool** | **Enable** | 1 | No | No |
| String | Existing | Variable | No | Yes |
| Date | Existing (unused) | 4 | No | Yes |
| Timestamp | Existing (unused) | 8 | No | Yes |

**Validation Rules**:
- Float64 values must be finite (reject NaN, Infinity, -Infinity)
- Bool values must be `true` or `false` (case-insensitive)

### Value (Extended)

**Location**: `src/types/value.rs` — `Value` enum

| Variant | Status | Rust Type | Serde |
|---------|--------|-----------|-------|
| Int64(i64) | Existing | i64 | Auto |
| **Float64(f64)** | **Enable** | f64 | Auto |
| **Bool(bool)** | **Enable** | bool | Auto |
| String(String) | Existing | String | Auto |
| Null | Existing | — | Auto |

**Comparison Rules** (already implemented in `Value::compare()`):
- Float64 vs Float64: `f64::partial_cmp` (returns None for NaN — but NaN rejected at parse)
- Bool vs Bool: `bool::cmp` (false < true)
- Cross-type Int64 vs Float64: **NEW** — promote Int64 to Float64, then compare

### Literal (Extended)

**Location**: `src/parser/ast.rs` — `Literal` enum

| Variant | Status | Grammar Rule |
|---------|--------|-------------|
| String(String) | Existing | `string_literal` |
| Int64(i64) | Existing | `integer_literal` |
| **Float64(f64)** | **New** | `float_literal` |
| **Bool(bool)** | **New** | `bool_literal` |

### Grammar Rules (New/Modified)

**Location**: `src/parser/grammar.pest`

```pest
# Modified: add FLOAT64 and BOOL keywords
data_type = { ^"STRING" | ^"INT64" | ^"FLOAT64" | ^"BOOL" }

# Modified: add float and bool literal variants
literal = { float_literal | bool_literal | string_literal | integer_literal }

# New: float literal (decimal number)
float_literal = @{ "-"? ~ ASCII_DIGIT* ~ "." ~ ASCII_DIGIT+ }

# Existing: already defined for COPY options, now reused for general literals
bool_literal = { ^"TRUE" | ^"FALSE" }
```

**Ordering Note**: `float_literal` must come before `integer_literal` in the `literal` rule to ensure `3.14` is parsed as float, not as integer `3` followed by `.14`. `bool_literal` must come before `string_literal` (but since string literals are quote-delimited, there's no ambiguity).

## State Transitions

No state transitions. DataType and Value are stateless data containers.

## Relationships

```
DataType 1 --- * ColumnDef (schema definition)
DataType 1 --- * Value (runtime constraint)
Literal  1 --- 1 Value (execution-time conversion)
```

## Serialization Format

No changes to serialization. `Value::Float64(f64)` and `Value::Bool(bool)` already derive `Serialize`/`Deserialize` via serde, and bincode handles them automatically. The on-disk format is stable because bincode uses enum discriminant ordering, and Float64/Bool were already defined in the enum before Int64 was used.

**Wire format** (bincode):
- `Value::Float64(3.14)` → discriminant `2` (0-indexed) + 8 bytes IEEE 754
- `Value::Bool(true)` → discriminant `3` + 1 byte (`0x01`)
