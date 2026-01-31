//! Node bulk loader for CSV import.

use std::path::Path;
use std::sync::Arc;

use crate::catalog::NodeTableSchema;
use crate::error::{Result, RuzuError};
use crate::storage::csv::{
    parallel_read_all, shared_interner, CsvImportConfig, CsvParser, ImportError, ImportProgress,
    ImportResult, MmapReader, ProgressCallback, SharedInterner,
};
use crate::types::{DataType, Value};

/// Minimum file size to use parallel processing (256KB).
const MIN_PARALLEL_FILE_SIZE: u64 = 256 * 1024;

/// Bulk loader for importing nodes from CSV files.
pub struct NodeLoader {
    /// Table schema to validate against.
    schema: Arc<NodeTableSchema>,
    /// Import configuration.
    config: CsvImportConfig,
    /// Optional string interner for deduplication.
    interner: Option<SharedInterner>,
}

impl NodeLoader {
    /// Creates a new node loader for the given schema.
    #[must_use]
    pub fn new(schema: Arc<NodeTableSchema>, config: CsvImportConfig) -> Self {
        let interner = if config.intern_strings {
            Some(shared_interner())
        } else {
            None
        };
        Self {
            schema,
            config,
            interner,
        }
    }

    /// Creates a new node loader with a shared interner.
    ///
    /// Use this when you want to share an interner across multiple loaders.
    #[must_use]
    pub fn with_interner(
        schema: Arc<NodeTableSchema>,
        config: CsvImportConfig,
        interner: SharedInterner,
    ) -> Self {
        Self {
            schema,
            config,
            interner: Some(interner),
        }
    }

    /// Validates CSV headers against the schema.
    ///
    /// # Errors
    ///
    /// Returns an error if headers don't match schema columns.
    pub fn validate_headers(&self, headers: &[String]) -> Result<Vec<usize>> {
        let mut column_indices = Vec::new();

        for col in &self.schema.columns {
            match headers.iter().position(|h| h == &col.name) {
                Some(idx) => column_indices.push(idx),
                None => {
                    return Err(RuzuError::StorageError(format!(
                        "CSV missing required column '{}'",
                        col.name
                    )));
                }
            }
        }

        Ok(column_indices)
    }

    /// Parses a CSV field into a Value based on the expected type.
    ///
    /// # Errors
    ///
    /// Returns an error if the field cannot be parsed.
    pub fn parse_field(
        &self,
        field: &str,
        data_type: DataType,
        row_num: u64,
        col_name: &str,
    ) -> std::result::Result<Value, ImportError> {
        if field.is_empty() {
            return Ok(Value::Null);
        }

        match data_type {
            DataType::Int64 => field.parse::<i64>().map(Value::Int64).map_err(|e| {
                ImportError::column_error(row_num, col_name, format!("Invalid INT64: {e}"))
            }),
            DataType::Float32 => field.parse::<f32>().map(Value::Float32).map_err(|e| {
                ImportError::column_error(row_num, col_name, format!("Invalid FLOAT32: {e}"))
            }),
            DataType::Float64 => field.parse::<f64>().map(Value::Float64).map_err(|e| {
                ImportError::column_error(row_num, col_name, format!("Invalid FLOAT64: {e}"))
            }),
            DataType::Bool => match field.to_lowercase().as_str() {
                "true" => Ok(Value::Bool(true)),
                "false" => Ok(Value::Bool(false)),
                _ => Err(ImportError::column_error(
                    row_num,
                    col_name,
                    format!("Invalid BOOL: {field} (expected 'true' or 'false')"),
                )),
            },
            DataType::String => {
                // Use string interning if enabled
                if let Some(ref interner) = self.interner {
                    let interned = interner.write().intern(field);
                    Ok(Value::String(interned.to_string()))
                } else {
                    Ok(Value::String(field.to_string()))
                }
            }
            DataType::Date => {
                // Simple date parsing (YYYY-MM-DD format)
                // In a real implementation, we'd use chrono
                Ok(Value::String(field.to_string()))
            }
            DataType::Timestamp => {
                // Parse as microseconds from epoch or as ISO format string
                field.parse::<i64>().map(Value::Timestamp).map_err(|e| {
                    ImportError::column_error(row_num, col_name, format!("Invalid TIMESTAMP: {e}"))
                })
            }
        }
    }

    /// Parses a CSV record into a vector of Values.
    fn parse_record(
        &self,
        record: &csv::StringRecord,
        column_indices: &[usize],
        row_num: u64,
    ) -> std::result::Result<Vec<Value>, ImportError> {
        let mut values = Vec::with_capacity(self.schema.columns.len());

        for (col_idx, &csv_idx) in column_indices.iter().enumerate() {
            let field = record.get(csv_idx).unwrap_or("");
            let col_def = &self.schema.columns[col_idx];

            let value = self.parse_field(field, col_def.data_type, row_num, &col_def.name)?;
            values.push(value);
        }

        Ok(values)
    }

    /// Loads nodes from a CSV file.
    ///
    /// Returns the parsed rows as vectors of Values.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be parsed.
    pub fn load(
        &self,
        path: &Path,
        progress_callback: Option<ProgressCallback>,
    ) -> Result<(Vec<Vec<Value>>, ImportResult)> {
        // Validate config
        self.config.validate()?;

        // Check file size to decide on processing strategy
        let file_size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);

        // Use parallel processing for large files when enabled
        if self.config.parallel && file_size >= MIN_PARALLEL_FILE_SIZE {
            self.load_parallel(path, file_size, progress_callback)
        } else {
            self.load_sequential(path, progress_callback)
        }
    }

    /// Sequential loading (original implementation with timing).
    fn load_sequential(
        &self,
        path: &Path,
        progress_callback: Option<ProgressCallback>,
    ) -> Result<(Vec<Vec<Value>>, ImportResult)> {
        let parser = CsvParser::new(self.config.clone());
        let mut progress = ImportProgress::new();

        // Start timing
        progress.start();

        // Get total lines for progress
        if let Ok(total) = CsvParser::count_lines(path) {
            progress.rows_total = Some(total.saturating_sub(1)); // Exclude header
        }

        // Get headers and validate
        let headers = parser.headers(path)?;
        let column_indices = self.validate_headers(&headers)?;

        // Parse records
        let mut reader = parser.reader_from_path(path)?;
        let mut rows = Vec::new();
        let mut row_num = 1u64; // 1-indexed, after header
        let mut batch_bytes = 0u64;

        for result in reader.records() {
            row_num += 1;

            let record = match result {
                Ok(r) => r,
                Err(e) => {
                    let error = ImportError::row_error(row_num, format!("CSV parse error: {e}"));
                    if self.config.ignore_errors {
                        progress.add_error(error);
                        continue;
                    }
                    return Err(RuzuError::StorageError(error.to_string()));
                }
            };

            // Estimate bytes (rough approximation)
            batch_bytes += record.as_slice().len() as u64 + 1;

            match self.parse_record(&record, &column_indices, row_num) {
                Ok(values) => {
                    rows.push(values);
                }
                Err(error) => {
                    if self.config.ignore_errors {
                        progress.add_error(error);
                    } else {
                        return Err(RuzuError::StorageError(error.to_string()));
                    }
                }
            }

            // Report progress periodically using update() for throughput tracking
            if rows.len() % self.config.batch_size == 0 {
                progress.update(self.config.batch_size as u64, batch_bytes);
                batch_bytes = 0;

                if let Some(ref callback) = progress_callback {
                    callback(progress.clone());
                }
            }
        }

        // Update final progress
        let remaining_rows = rows.len() as u64 % self.config.batch_size as u64;
        if remaining_rows > 0 || batch_bytes > 0 {
            progress.update(remaining_rows, batch_bytes);
        }

        // Final progress report
        if let Some(callback) = progress_callback {
            callback(progress.clone());
        }

        let result = ImportResult::from_progress(progress);
        Ok((rows, result))
    }

    /// Parallel loading using mmap and rayon.
    fn load_parallel(
        &self,
        path: &Path,
        file_size: u64,
        progress_callback: Option<ProgressCallback>,
    ) -> Result<(Vec<Vec<Value>>, ImportResult)> {
        let mut progress = ImportProgress::new();
        progress.start();

        // Open file with mmap
        let mut reader = MmapReader::open(path, &self.config)?;
        let data = reader.as_slice()?;

        // Get headers first (we need to validate before parallel processing)
        let parser = CsvParser::new(self.config.clone());
        let headers = parser.headers(path)?;
        let column_indices = self.validate_headers(&headers)?;

        // Estimate total rows for progress
        let avg_row_size = super::estimate_avg_row_size(data);
        progress.rows_total = Some((file_size as usize / avg_row_size) as u64);

        // Create parsing closure that captures schema info
        let columns = self.schema.columns.clone();
        let column_indices_clone = column_indices.clone();
        let interner = self.interner.clone();
        let parse_row = move |record: &csv::ByteRecord,
                              row_num: u64|
              -> std::result::Result<Vec<Value>, ImportError> {
            let mut values = Vec::with_capacity(columns.len());

            for (col_idx, &csv_idx) in column_indices_clone.iter().enumerate() {
                let field_bytes = record.get(csv_idx).unwrap_or(b"");
                let field = std::str::from_utf8(field_bytes).map_err(|e| {
                    ImportError::column_error(
                        row_num,
                        &columns[col_idx].name,
                        format!("Invalid UTF-8: {e}"),
                    )
                })?;
                let col_def = &columns[col_idx];

                let value = parse_field_with_interner(
                    field,
                    col_def.data_type,
                    row_num,
                    &col_def.name,
                    interner.as_ref(),
                )?;
                values.push(value);
            }

            Ok(values)
        };

        // Report initial progress
        if let Some(ref callback) = progress_callback {
            callback(progress.clone());
        }

        // Run parallel parsing
        let (rows, errors, bytes_processed) = parallel_read_all(data, &self.config, parse_row)?;

        // Update progress with results
        progress.rows_processed = rows.len() as u64;
        progress.rows_failed = errors.len() as u64;
        progress.bytes_read = bytes_processed;
        progress.errors = errors;

        // Final progress report
        if let Some(callback) = progress_callback {
            callback(progress.clone());
        }

        let result = ImportResult::from_progress(progress);
        Ok((rows, result))
    }

    /// Loads nodes from a CSV file using streaming with a batch callback.
    ///
    /// Unlike `load()`, this method does NOT accumulate all rows in memory.
    /// Instead, it calls the batch callback for each batch of rows, allowing
    /// the caller to process and discard rows incrementally.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the CSV file
    /// * `batch_callback` - Called with each batch of rows; returns Ok(()) to continue
    /// * `progress_callback` - Optional progress reporting callback
    ///
    /// # Memory Behavior
    ///
    /// Memory usage is bounded by `config.batch_size` regardless of file size.
    /// The batch callback receives rows and should process them before returning.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The file cannot be opened or parsed
    /// - A batch callback returns an error
    /// - Config validation fails
    pub fn load_streaming<F>(
        &self,
        path: &Path,
        mut batch_callback: F,
        progress_callback: Option<ProgressCallback>,
    ) -> Result<ImportResult>
    where
        F: FnMut(Vec<Vec<Value>>) -> Result<()>,
    {
        // Validate config
        self.config.validate()?;

        let parser = CsvParser::new(self.config.clone());
        let mut progress = ImportProgress::new();

        // Start timing
        progress.start();

        // Get total lines for progress
        if let Ok(total) = CsvParser::count_lines(path) {
            progress.rows_total = Some(total.saturating_sub(1)); // Exclude header
        }

        // Get headers and validate
        let headers = parser.headers(path)?;
        let column_indices = self.validate_headers(&headers)?;

        // Parse records in batches
        let mut reader = parser.reader_from_path(path)?;
        let mut batch = Vec::with_capacity(self.config.batch_size);
        let mut row_num = 1u64; // 1-indexed, after header
        let mut batch_bytes = 0u64;

        for result in reader.records() {
            row_num += 1;

            let record = match result {
                Ok(r) => r,
                Err(e) => {
                    let error = ImportError::row_error(row_num, format!("CSV parse error: {e}"));
                    if self.config.ignore_errors {
                        progress.add_error(error);
                        continue;
                    }
                    return Err(RuzuError::StorageError(error.to_string()));
                }
            };

            // Estimate bytes
            batch_bytes += record.as_slice().len() as u64 + 1;

            match self.parse_record(&record, &column_indices, row_num) {
                Ok(values) => {
                    batch.push(values);
                }
                Err(error) => {
                    if self.config.ignore_errors {
                        progress.add_error(error);
                    } else {
                        return Err(RuzuError::StorageError(error.to_string()));
                    }
                }
            }

            // When batch is full, call the callback and reset
            if batch.len() >= self.config.batch_size {
                let batch_len = batch.len() as u64;

                // Call batch callback - it takes ownership and processes the rows
                batch_callback(std::mem::take(&mut batch))?;
                batch.reserve(self.config.batch_size);

                // Update progress
                progress.update(batch_len, batch_bytes);
                batch_bytes = 0;

                if let Some(ref callback) = progress_callback {
                    callback(progress.clone());
                }
            }
        }

        // Process remaining rows
        if !batch.is_empty() {
            let batch_len = batch.len() as u64;
            batch_callback(batch)?;
            progress.update(batch_len, batch_bytes);
        }

        // Final progress report
        if let Some(callback) = progress_callback {
            callback(progress.clone());
        }

        Ok(ImportResult::from_progress(progress))
    }
}

/// Field parsing function with optional string interning.
fn parse_field_with_interner(
    field: &str,
    data_type: DataType,
    row_num: u64,
    col_name: &str,
    interner: Option<&SharedInterner>,
) -> std::result::Result<Value, ImportError> {
    if field.is_empty() {
        return Ok(Value::Null);
    }

    match data_type {
        DataType::Int64 => field.parse::<i64>().map(Value::Int64).map_err(|e| {
            ImportError::column_error(row_num, col_name, format!("Invalid INT64: {e}"))
        }),
        DataType::Float32 => field.parse::<f32>().map(Value::Float32).map_err(|e| {
            ImportError::column_error(row_num, col_name, format!("Invalid FLOAT32: {e}"))
        }),
        DataType::Float64 => field.parse::<f64>().map(Value::Float64).map_err(|e| {
            ImportError::column_error(row_num, col_name, format!("Invalid FLOAT64: {e}"))
        }),
        DataType::Bool => match field.to_lowercase().as_str() {
            "true" => Ok(Value::Bool(true)),
            "false" => Ok(Value::Bool(false)),
            _ => Err(ImportError::column_error(
                row_num,
                col_name,
                format!("Invalid BOOL: {field} (expected 'true' or 'false')"),
            )),
        },
        DataType::String => {
            if let Some(interner) = interner {
                let interned_val = interner.write().intern(field);
                Ok(Value::String(interned_val.to_string()))
            } else {
                Ok(Value::String(field.to_string()))
            }
        }
        DataType::Date => Ok(Value::String(field.to_string())),
        DataType::Timestamp => field.parse::<i64>().map(Value::Timestamp).map_err(|e| {
            ImportError::column_error(row_num, col_name, format!("Invalid TIMESTAMP: {e}"))
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::ColumnDef;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_schema() -> Arc<NodeTableSchema> {
        Arc::new(
            NodeTableSchema::new(
                "Person".to_string(),
                vec![
                    ColumnDef::new("name".to_string(), DataType::String).unwrap(),
                    ColumnDef::new("age".to_string(), DataType::Int64).unwrap(),
                ],
                vec!["name".to_string()],
            )
            .unwrap(),
        )
    }

    fn create_test_csv(content: &str) -> (std::path::PathBuf, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("test.csv");

        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(content.as_bytes()).unwrap();

        (path, temp_dir)
    }

    #[test]
    fn test_load_simple_csv() {
        let schema = create_test_schema();
        let loader = NodeLoader::new(schema, CsvImportConfig::default());

        let csv_content = "name,age\nAlice,25\nBob,30\n";
        let (path, _temp) = create_test_csv(csv_content);

        let (rows, result) = loader.load(&path, None).unwrap();

        assert_eq!(rows.len(), 2);
        assert_eq!(result.rows_imported, 2);
        assert!(result.is_success());

        assert_eq!(rows[0][0], Value::String("Alice".to_string()));
        assert_eq!(rows[0][1], Value::Int64(25));
    }

    #[test]
    fn test_load_with_different_column_order() {
        let schema = create_test_schema();
        let loader = NodeLoader::new(schema, CsvImportConfig::default());

        // CSV has columns in different order than schema
        let csv_content = "age,name\n25,Alice\n30,Bob\n";
        let (path, _temp) = create_test_csv(csv_content);

        let (rows, result) = loader.load(&path, None).unwrap();

        assert_eq!(rows.len(), 2);
        assert!(result.is_success());

        // Values should be in schema order (name, age)
        assert_eq!(rows[0][0], Value::String("Alice".to_string()));
        assert_eq!(rows[0][1], Value::Int64(25));
    }

    #[test]
    fn test_load_with_errors_ignored() {
        let schema = create_test_schema();
        let config = CsvImportConfig::default().with_ignore_errors(true);
        let loader = NodeLoader::new(schema, config);

        // Row 2 has invalid age
        let csv_content = "name,age\nAlice,25\nBob,not_a_number\nCharlie,35\n";
        let (path, _temp) = create_test_csv(csv_content);

        let (rows, result) = loader.load(&path, None).unwrap();

        assert_eq!(rows.len(), 2); // Alice and Charlie
        assert_eq!(result.rows_imported, 2);
        assert_eq!(result.rows_failed, 1);
        assert!(!result.is_success());
    }

    #[test]
    fn test_missing_column_error() {
        let schema = create_test_schema();
        let loader = NodeLoader::new(schema, CsvImportConfig::default());

        let csv_content = "name\nAlice\nBob\n"; // Missing 'age' column
        let (path, _temp) = create_test_csv(csv_content);

        let result = loader.load(&path, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_bool_field() {
        let schema = Arc::new(
            NodeTableSchema::new(
                "Test".to_string(),
                vec![ColumnDef::new("active".to_string(), DataType::Bool).unwrap()],
                vec!["active".to_string()],
            )
            .unwrap(),
        );
        let loader = NodeLoader::new(schema, CsvImportConfig::default());

        // Test case-insensitive true/false (only accepted values per R4)
        let true_values = ["true", "True", "TRUE"];
        let false_values = ["false", "False", "FALSE"];

        for val in true_values {
            let result = loader.parse_field(val, DataType::Bool, 1, "test");
            assert_eq!(result.unwrap(), Value::Bool(true));
        }

        for val in false_values {
            let result = loader.parse_field(val, DataType::Bool, 1, "test");
            assert_eq!(result.unwrap(), Value::Bool(false));
        }

        // Verify rejected values
        let rejected_values = ["1", "0", "yes", "no", "t", "f"];
        for val in rejected_values {
            let result = loader.parse_field(val, DataType::Bool, 1, "test");
            assert!(result.is_err(), "Expected '{}' to be rejected", val);
        }
    }

    #[test]
    fn test_load_sequential_explicit() {
        let schema = create_test_schema();
        // Force sequential by using small file and parallel=false
        let config = CsvImportConfig::default().with_parallel(false);
        let loader = NodeLoader::new(schema, config);

        let csv_content = "name,age\nAlice,25\nBob,30\nCharlie,35\n";
        let (path, _temp) = create_test_csv(csv_content);

        let (rows, result) = loader.load(&path, None).unwrap();

        assert_eq!(rows.len(), 3);
        assert!(result.is_success());
    }

    #[test]
    fn test_load_with_string_interning() {
        // Schema with string column that will have repeated values
        let schema = Arc::new(
            NodeTableSchema::new(
                "Person".to_string(),
                vec![
                    ColumnDef::new("name".to_string(), DataType::String).unwrap(),
                    ColumnDef::new("city".to_string(), DataType::String).unwrap(),
                ],
                vec!["name".to_string()],
            )
            .unwrap(),
        );

        // Enable string interning
        let config = CsvImportConfig::default()
            .with_parallel(false)
            .with_intern_strings(true);
        let loader = NodeLoader::new(schema, config);

        // CSV with repeated city values
        let csv_content = "name,city\nAlice,NYC\nBob,NYC\nCharlie,LA\nDiana,NYC\nEve,LA\n";
        let (path, _temp) = create_test_csv(csv_content);

        let (rows, result) = loader.load(&path, None).unwrap();

        assert_eq!(rows.len(), 5);
        assert!(result.is_success());

        // Verify values are correct
        assert_eq!(rows[0][1], Value::String("NYC".to_string()));
        assert_eq!(rows[1][1], Value::String("NYC".to_string()));
        assert_eq!(rows[2][1], Value::String("LA".to_string()));

        // Check interner statistics
        if let Some(ref interner) = loader.interner {
            let interner = interner.read();
            // Should have fewer unique strings than total strings parsed
            // 5 names + 5 cities = 10 strings, but only 7 unique (5 names + NYC + LA)
            assert!(interner.unique_count() <= 7);
            assert!(interner.hit_rate() > 0.0); // Should have some hits
        }
    }

    #[test]
    fn test_load_with_shared_interner() {
        let schema = create_test_schema();
        let interner = shared_interner();

        let config = CsvImportConfig::default().with_parallel(false);
        let loader = NodeLoader::with_interner(schema, config, Arc::clone(&interner));

        let csv_content = "name,age\nAlice,25\nAlice,30\nAlice,35\n";
        let (path, _temp) = create_test_csv(csv_content);

        let (rows, result) = loader.load(&path, None).unwrap();

        assert_eq!(rows.len(), 3);
        assert!(result.is_success());

        // Check shared interner was used
        let interner = interner.read();
        assert_eq!(interner.unique_count(), 1); // Only "Alice"
        assert_eq!(interner.hits(), 2); // Two cache hits
    }
}
