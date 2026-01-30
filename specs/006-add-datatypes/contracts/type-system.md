# Contract: Type System Extension

**Feature**: 006-add-datatypes
**Date**: 2026-01-30

## Overview

This contract defines the expected behavior for FLOAT64 and BOOL datatype support across all database operations.

## Contract: DDL — CREATE NODE TABLE with FLOAT64/BOOL

### C-DDL-01: FLOAT64 column in CREATE NODE TABLE

**Input**: `CREATE NODE TABLE Product(name STRING, price FLOAT64, PRIMARY KEY(name))`
**Expected**: Table created with schema `[("name", STRING), ("price", FLOAT64)]`, primary key `["name"]`.
**Error**: None.

### C-DDL-02: BOOL column in CREATE NODE TABLE

**Input**: `CREATE NODE TABLE Feature(name STRING, enabled BOOL, PRIMARY KEY(name))`
**Expected**: Table created with schema `[("name", STRING), ("enabled", BOOL)]`, primary key `["name"]`.
**Error**: None.

### C-DDL-03: All four types in single table

**Input**: `CREATE NODE TABLE Mixed(id INT64, name STRING, score FLOAT64, active BOOL, PRIMARY KEY(id))`
**Expected**: Table created with all four column types.

### C-DDL-04: FLOAT64/BOOL in CREATE REL TABLE

**Input**: `CREATE REL TABLE Rates(FROM User TO Product, score FLOAT64, verified BOOL)`
**Expected**: Relationship table created with FLOAT64 and BOOL properties.

## Contract: Literal Parsing

### C-LIT-01: Float literal

**Input (grammar)**: `3.14`, `-0.5`, `42.0`, `.5`, `0.0`
**Expected**: Parsed as `Literal::Float64(f64)`.

### C-LIT-02: Bool literal

**Input (grammar)**: `true`, `false`, `TRUE`, `FALSE`, `True`, `False`
**Expected**: Parsed as `Literal::Bool(bool)`.

### C-LIT-03: Integer literal unchanged

**Input (grammar)**: `42`, `-10`
**Expected**: Still parsed as `Literal::Int64(i64)` — not promoted to Float64 at parse time.

### C-LIT-04: Invalid float — NaN rejected

**Input**: Float value that parses to NaN
**Expected**: Parse error with message indicating NaN is not a valid FLOAT64 value.

### C-LIT-05: Invalid float — Infinity rejected

**Input**: Float value that parses to Infinity
**Expected**: Parse error with message indicating Infinity is not a valid FLOAT64 value.

## Contract: DML — CREATE Node with FLOAT64/BOOL

### C-DML-01: Create node with FLOAT64 property

**Input**: `CREATE (:Product {name: 'Widget', price: 19.99})`
**Expected**: Node created with `price = Value::Float64(19.99)`.

### C-DML-02: Create node with BOOL property

**Input**: `CREATE (:Feature {name: 'DarkMode', enabled: true})`
**Expected**: Node created with `enabled = Value::Bool(true)`.

### C-DML-03: Create node with integer value for FLOAT64 column

**Input**: `CREATE (:Product {name: 'Free', price: 0})`
**Expected**: Node created with `price = Value::Float64(0.0)` (integer promoted to float). Alternatively, `price = Value::Int64(0)` stored as-is if the column type allows runtime coercion.

## Contract: Query — WHERE with FLOAT64/BOOL

### C-QRY-01: Float comparison operators

**Input**: `MATCH (p:Product) WHERE p.price > 10.0 RETURN p.name`
**Expected**: Returns products where price > 10.0.

### C-QRY-02: Float comparison with integer literal

**Input**: `MATCH (p:Product) WHERE p.price > 10 RETURN p.name`
**Expected**: Integer `10` promoted to `10.0`, comparison works correctly.

### C-QRY-03: Bool equality

**Input**: `MATCH (f:Feature) WHERE f.enabled = true RETURN f.name`
**Expected**: Returns features where enabled is true.

### C-QRY-04: Bool inequality

**Input**: `MATCH (f:Feature) WHERE f.enabled <> false RETURN f.name`
**Expected**: Returns features where enabled is not false (i.e., true).

### C-QRY-05: ORDER BY on FLOAT64

**Input**: `MATCH (p:Product) RETURN p.name, p.price ORDER BY p.price ASC`
**Expected**: Products sorted by price ascending with correct numeric ordering.

## Contract: CSV Import

### C-CSV-01: FLOAT64 values from CSV

**Input CSV**: `name,price\nWidget,19.99\nGadget,5.50`
**Expected**: Parsed as `Value::Float64(19.99)` and `Value::Float64(5.50)`.

### C-CSV-02: BOOL values from CSV

**Input CSV**: `name,enabled\nDarkMode,true\nLightMode,false`
**Expected**: Parsed as `Value::Bool(true)` and `Value::Bool(false)`.

### C-CSV-03: Case-insensitive BOOL in CSV

**Input CSV**: `name,enabled\nA,TRUE\nB,False\nC,true`
**Expected**: All parsed correctly as Bool values.

### C-CSV-04: Invalid BOOL in CSV — rejected

**Input CSV**: `name,enabled\nA,yes\nB,1\nC,on`
**Expected**: Parse error for each row with message indicating invalid BOOL value.

### C-CSV-05: Invalid FLOAT64 in CSV — rejected

**Input CSV**: `name,price\nA,abc\nB,NaN\nC,Infinity`
**Expected**: Parse error for each row with message indicating invalid FLOAT64 value.

## Contract: Persistence

### C-PER-01: FLOAT64 data survives restart

**Setup**: Create table with FLOAT64 column, insert data, close database.
**Input**: Open database, query FLOAT64 data.
**Expected**: All FLOAT64 values identical to pre-restart values.

### C-PER-02: BOOL data survives restart

**Setup**: Create table with BOOL column, insert data, close database.
**Input**: Open database, query BOOL data.
**Expected**: All BOOL values identical to pre-restart values.

## Contract: Aggregation

### C-AGG-01: COUNT on FLOAT64 column

**Input**: `MATCH (p:Product) RETURN COUNT(p.price)`
**Expected**: Returns count of non-null FLOAT64 values.

### C-AGG-02: MIN/MAX on FLOAT64 column

**Input**: `MATCH (p:Product) RETURN MIN(p.price), MAX(p.price)`
**Expected**: Returns minimum and maximum FLOAT64 values.

### C-AGG-03: COUNT on BOOL column

**Input**: `MATCH (f:Feature) RETURN COUNT(f.enabled)`
**Expected**: Returns count of non-null BOOL values.
