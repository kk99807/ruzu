//! Error types for ruzu database operations.

use thiserror::Error;

/// Result type alias using [`RuzuError`].
pub type Result<T> = std::result::Result<T, RuzuError>;

/// Error types for ruzu database operations.
#[derive(Debug, Error)]
pub enum RuzuError {
    /// Parse error with location information.
    #[error("Parse error at line {line}, column {col}: {message}")]
    ParseError {
        line: usize,
        col: usize,
        message: String,
    },

    /// Schema-related errors (table not found, duplicate table, etc.).
    #[error("Schema error: {0}")]
    SchemaError(String),

    /// Type mismatch errors.
    #[error("Type error: expected {expected}, got {actual}")]
    TypeError { expected: String, actual: String },

    /// Constraint violations (primary key, uniqueness, etc.).
    #[error("Constraint violation: {0}")]
    ConstraintViolation(String),

    /// General execution errors.
    #[error("Execution error: {0}")]
    ExecutionError(String),

    // ==================== Storage Errors (Phase 1) ====================
    /// General storage/I/O error.
    #[error("Storage error: {0}")]
    StorageError(String),

    /// Page-related errors.
    #[error("Page error: {0}")]
    PageError(String),

    /// Buffer pool errors.
    #[error("Buffer pool error: {0}")]
    BufferPoolError(String),

    /// WAL (Write-Ahead Log) errors.
    #[error("WAL error: {0}")]
    WalError(String),

    /// Catalog persistence errors.
    #[error("Catalog error: {0}")]
    CatalogError(String),

    /// Checksum validation failure.
    #[error("Checksum mismatch: {0}")]
    ChecksumError(String),

    /// Database file corruption detected.
    #[error("Corrupted database: {0}")]
    CorruptedDatabase(String),

    /// Invalid database magic bytes.
    #[error("Invalid database file: {0}")]
    InvalidDatabaseFile(String),

    /// Unsupported database version.
    #[error("Unsupported database version: {version} (max supported: {max_supported})")]
    UnsupportedVersion { version: u32, max_supported: u32 },

    /// Referential integrity violation (relationship to non-existent node).
    #[error("Referential integrity error: {0}")]
    ReferentialIntegrity(String),

    /// CSV import error.
    #[error("Import error: {0}")]
    ImportError(String),

    /// Validation error.
    #[error("Validation error: {0}")]
    ValidationError(String),

    /// Relationship table load error.
    #[error("Relationship table load error: {0}")]
    RelTableLoadError(String),

    /// Relationship table corrupted error.
    #[error("Relationship table corrupted: {0}")]
    RelTableCorrupted(String),

    // ==================== Query Engine Errors (Phase 2) ====================
    /// Binding error (semantic analysis).
    #[error("Bind error: {0}")]
    BindError(String),

    /// Planning error (query planning).
    #[error("Plan error: {0}")]
    PlanError(String),

    // ==================== CSV Parallel Processing Errors (Phase 3) ====================
    /// Quoted newline detected in parallel CSV mode.
    #[error("Quoted newlines are not supported in parallel CSV mode. Please set parallel=false in the config. Detected at approximately row {row}")]
    QuotedNewlineInParallel { row: u64 },

    /// Worker thread panicked during parallel processing.
    #[error("Worker thread panicked: {0}")]
    ThreadPanic(String),

    // ==================== Query Execution Errors (Phase 10) ====================
    /// Memory limit exceeded during query execution.
    #[error("Memory limit exceeded: {used} bytes used, limit is {limit} bytes")]
    MemoryLimitExceeded { used: usize, limit: usize },

    /// Query timeout exceeded.
    #[error("Query timeout: execution exceeded {timeout_ms}ms")]
    QueryTimeout { timeout_ms: u64 },

    /// Invalid expression during query execution.
    #[error("Invalid expression: {0}")]
    InvalidExpression(String),

    /// Unsupported operation in the current context.
    #[error("Unsupported operation: {0}")]
    UnsupportedOperation(String),

    /// Division by zero in expression evaluation.
    #[error("Division by zero")]
    DivisionByZero,

    /// Null value in non-null context.
    #[error("Null value error: {0}")]
    NullValue(String),
}
