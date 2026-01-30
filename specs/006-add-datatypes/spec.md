# Feature Specification: Add Additional Datatypes

**Feature Branch**: `006-add-datatypes`
**Created**: 2026-01-30
**Status**: Draft
**Input**: User description: "Add additional datatypes. the PEST grammar seems to indicate only string and int are currently supported. We need at minimum to also add FLOAT64 and BOOL for this to be useful in other projects"

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Define Tables with FLOAT64 Columns (Priority: P1)

A user wants to create node or relationship tables that store floating-point numeric data such as prices, coordinates, measurements, or scores. They should be able to use the FLOAT64 datatype in CREATE NODE TABLE and CREATE REL TABLE statements, just as they currently use INT64 and STRING.

**Why this priority**: Floating-point data is among the most commonly needed datatypes for real-world datasets. Without FLOAT64 support, users cannot model financial data, scientific measurements, geographic coordinates, or any numeric data requiring decimal precision.

**Independent Test**: Can be fully tested by creating a node table with a FLOAT64 column, inserting float values, and querying them back. Delivers immediate value for any dataset with decimal numbers.

**Acceptance Scenarios**:

1. **Given** a running database, **When** the user executes `CREATE NODE TABLE Product(name STRING, price FLOAT64, PRIMARY KEY(name))`, **Then** the table is created successfully with a FLOAT64 column.
2. **Given** a table with a FLOAT64 column, **When** the user creates a node with a decimal value (e.g., `19.99`), **Then** the value is stored and can be queried back accurately.
3. **Given** a table with a FLOAT64 column, **When** the user imports data via COPY FROM a CSV file containing decimal values, **Then** the values are parsed and stored correctly.
4. **Given** a table with a FLOAT64 column, **When** the user uses comparison operators (>, <, >=, <=, =, <>) on FLOAT64 values in WHERE clauses, **Then** the comparisons produce correct results.
5. **Given** a FLOAT64 column, **When** the database is closed and reopened, **Then** all FLOAT64 data persists and is readable.

---

### User Story 2 - Define Tables with BOOL Columns (Priority: P1)

A user wants to create node or relationship tables that store boolean (true/false) data such as active status, feature flags, or binary attributes. They should be able to use the BOOL datatype in CREATE NODE TABLE and CREATE REL TABLE statements.

**Why this priority**: Boolean data is fundamental to almost every data model. Without BOOL support, users must use workarounds like INT64 (0/1) or STRING ("true"/"false"), which is error-prone and unintuitive.

**Independent Test**: Can be fully tested by creating a node table with a BOOL column, inserting true/false values, and querying them back. Delivers immediate value for any dataset with binary attributes.

**Acceptance Scenarios**:

1. **Given** a running database, **When** the user executes `CREATE NODE TABLE Feature(name STRING, enabled BOOL, PRIMARY KEY(name))`, **Then** the table is created successfully with a BOOL column.
2. **Given** a table with a BOOL column, **When** the user creates a node with `true` or `false`, **Then** the value is stored and can be queried back correctly.
3. **Given** a table with a BOOL column, **When** the user imports data via COPY FROM a CSV file containing boolean values (e.g., `true`, `false`), **Then** the values are parsed and stored correctly.
4. **Given** a table with a BOOL column, **When** the user uses equality comparison (`=`, `<>`) on BOOL values in WHERE clauses, **Then** the comparisons produce correct results.
5. **Given** a BOOL column, **When** the database is closed and reopened, **Then** all BOOL data persists and is readable.

---

### User Story 3 - Use FLOAT64 and BOOL in Relationship Properties (Priority: P2)

A user wants to define relationship tables with FLOAT64 and BOOL properties. For example, modeling a weighted graph where edges have a FLOAT64 weight, or a social network where relationships have a BOOL "active" flag.

**Why this priority**: Relationship properties are essential for graph modeling. The new datatypes must work consistently for both node tables and relationship tables.

**Independent Test**: Can be fully tested by creating a relationship table with FLOAT64 and BOOL properties, inserting relationships with those properties, and querying them back.

**Acceptance Scenarios**:

1. **Given** existing node tables, **When** the user executes `CREATE REL TABLE Knows(FROM Person TO Person, weight FLOAT64, active BOOL)`, **Then** the relationship table is created with both new datatypes.
2. **Given** a relationship table with FLOAT64 and BOOL properties, **When** the user creates a relationship with values for those properties, **Then** the values are stored and retrievable.
3. **Given** a relationship table with new datatype properties, **When** relationships are imported via COPY FROM, **Then** FLOAT64 and BOOL values in the CSV are parsed correctly.

---

### User Story 4 - Mixed-Type Queries (Priority: P2)

A user wants to query tables that combine STRING, INT64, FLOAT64, and BOOL columns, filtering and returning values of different types in the same query.

**Why this priority**: Real-world queries will naturally span multiple datatypes. Users need confidence that mixed-type queries work correctly.

**Independent Test**: Can be tested by creating a table with columns of all four types, inserting data, and running queries that filter on one type while returning others.

**Acceptance Scenarios**:

1. **Given** a table with STRING, INT64, FLOAT64, and BOOL columns, **When** the user runs a query filtering on a FLOAT64 column and returning a BOOL column, **Then** correct results are returned.
2. **Given** a table with all four datatypes, **When** the user runs ORDER BY on a FLOAT64 column, **Then** results are sorted correctly by numeric value.

---

### Edge Cases

- What happens when a user provides an integer value (e.g., `42`) for a FLOAT64 column? The system should accept it and convert to `42.0`.
- What happens when a FLOAT64 value in a CSV is not a valid number (e.g., `abc`)? The system should report a parse error.
- What happens when a BOOL column in a CSV contains values other than `true`/`false` (e.g., `yes`, `1`, `0`)? The system should accept case-insensitive `true`/`false` and reject other values with a clear error.
- What happens when a FLOAT64 value is `NaN` or `Infinity`? The system should reject these as invalid values with a clear error message.
- What happens when comparing a FLOAT64 value with an integer literal in a WHERE clause (e.g., `WHERE p.price > 10`)? The system should handle this by treating the integer literal as a float for comparison.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The system MUST accept FLOAT64 as a valid datatype in CREATE NODE TABLE column definitions.
- **FR-002**: The system MUST accept BOOL as a valid datatype in CREATE NODE TABLE column definitions.
- **FR-003**: The system MUST accept FLOAT64 and BOOL as valid datatypes in CREATE REL TABLE property definitions.
- **FR-004**: The system MUST parse floating-point literal values (e.g., `3.14`, `-0.5`, `42.0`) in CREATE node statements and query literals.
- **FR-005**: The system MUST parse boolean literal values (`true`, `false`, case-insensitive) in CREATE node statements and query literals.
- **FR-006**: The system MUST support FLOAT64 and BOOL values in CSV import via COPY FROM, correctly parsing decimal numbers and boolean strings from CSV fields.
- **FR-007**: The system MUST support comparison operators (>, <, >=, <=, =, <>) for FLOAT64 values in WHERE clauses.
- **FR-008**: The system MUST support equality operators (=, <>) for BOOL values in WHERE clauses.
- **FR-009**: The system MUST persist FLOAT64 and BOOL column data across database close and reopen cycles.
- **FR-010**: The system MUST support ORDER BY on FLOAT64 columns with correct numeric sorting.
- **FR-011**: The system MUST reject invalid FLOAT64 values (NaN, Infinity, non-numeric strings) with a clear error message.
- **FR-012**: The system MUST reject invalid BOOL values (anything other than case-insensitive true/false) with a clear error message.
- **FR-013**: The system MUST support FLOAT64 and BOOL values in aggregation functions (COUNT, MIN, MAX for FLOAT64; COUNT for BOOL) where applicable.

### Key Entities

- **DataType**: Represents the type of a column. Extended to include FLOAT64 and BOOL in addition to existing STRING and INT64.
- **Value**: Represents a runtime data value. Extended to support Float64 and Bool variants alongside existing Int64, String, and Null.
- **Literal**: A constant value in a query. Extended to support float literals (decimal numbers) and boolean literals (true/false).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Users can create tables with FLOAT64 and BOOL columns using standard DDL syntax with no errors.
- **SC-002**: All four datatypes (STRING, INT64, FLOAT64, BOOL) can be used together in a single table and queried correctly.
- **SC-003**: CSV import correctly parses and stores FLOAT64 and BOOL values with the same throughput characteristics as existing types.
- **SC-004**: All existing tests continue to pass with no regressions.
- **SC-005**: FLOAT64 and BOOL data survives database restart with no data loss.
- **SC-006**: Invalid datatype values produce clear, actionable error messages that identify the problematic value and expected format.

## Assumptions

- The user explicitly requested FLOAT64 and BOOL as the minimum additions. Other datatypes already defined in code (FLOAT32, DATE, TIMESTAMP) are out of scope for this feature. They can be enabled in a future feature.
- FLOAT64 precision follows IEEE 754 double-precision semantics (standard for 64-bit floats).
- Boolean CSV parsing accepts case-insensitive `true` and `false` only; other representations (1/0, yes/no) are not supported.
- Integer literals provided for FLOAT64 columns are implicitly converted to float (e.g., `42` becomes `42.0`).
- The existing code already has DataType::Float64, DataType::Bool, Value::Float64, and Value::Bool enum variants implemented; this feature focuses on enabling them through the parser grammar, literal syntax, and table creation paths.
