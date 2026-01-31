//! Relationship bulk loader for CSV import.
//!
//! This module handles loading relationships from CSV files.
//! Relationships require special handling because:
//! - They reference source and destination nodes
//! - They need to be indexed in both directions (forward and backward)

use std::path::Path;

use crate::error::{Result, RuzuError};
use crate::storage::csv::{
    parallel_read_all, shared_interner, CsvImportConfig, CsvParser, ImportError, ImportProgress,
    ImportResult, MmapReader, ProgressCallback, SharedInterner,
};
use crate::types::{DataType, Value};

/// Minimum file size to use parallel processing (256KB).
const MIN_PARALLEL_FILE_SIZE: u64 = 256 * 1024;

/// A parsed relationship from CSV.
#[derive(Debug, Clone)]
pub struct ParsedRelationship {
    /// Source node identifier (primary key value).
    pub from_key: Value,
    /// Destination node identifier (primary key value).
    pub to_key: Value,
    /// Relationship properties.
    pub properties: Vec<Value>,
}

/// Bulk loader for importing relationships from CSV files.
pub struct RelLoader {
    /// Name of the source key column in CSV (default: "FROM").
    from_column: String,
    /// Name of the destination key column in CSV (default: "TO").
    to_column: String,
    /// Property column names and types.
    property_columns: Vec<(String, DataType)>,
    /// Import configuration.
    config: CsvImportConfig,
    /// Optional string interner for deduplication.
    interner: Option<SharedInterner>,
}

impl RelLoader {
    /// Creates a new relationship loader.
    #[must_use]
    pub fn new(
        from_column: String,
        to_column: String,
        property_columns: Vec<(String, DataType)>,
        config: CsvImportConfig,
    ) -> Self {
        let interner = if config.intern_strings {
            Some(shared_interner())
        } else {
            None
        };
        Self {
            from_column,
            to_column,
            property_columns,
            config,
            interner,
        }
    }

    /// Creates a loader with default FROM/TO column names.
    #[must_use]
    pub fn with_default_columns(
        property_columns: Vec<(String, DataType)>,
        config: CsvImportConfig,
    ) -> Self {
        Self::new(
            "FROM".to_string(),
            "TO".to_string(),
            property_columns,
            config,
        )
    }

    /// Creates a new relationship loader with a shared interner.
    ///
    /// Use this when you want to share an interner across multiple loaders.
    #[must_use]
    pub fn with_interner(
        from_column: String,
        to_column: String,
        property_columns: Vec<(String, DataType)>,
        config: CsvImportConfig,
        interner: SharedInterner,
    ) -> Self {
        Self {
            from_column,
            to_column,
            property_columns,
            config,
            interner: Some(interner),
        }
    }

    /// Validates CSV headers and returns column indices.
    ///
    /// # Errors
    ///
    /// Returns an error if required columns are missing.
    pub fn validate_headers(&self, headers: &[String]) -> Result<(usize, usize, Vec<usize>)> {
        // Find FROM column
        let from_idx = headers
            .iter()
            .position(|h| h.eq_ignore_ascii_case(&self.from_column))
            .ok_or_else(|| {
                RuzuError::StorageError(format!(
                    "CSV missing required '{}' column",
                    self.from_column
                ))
            })?;

        // Find TO column
        let to_idx = headers
            .iter()
            .position(|h| h.eq_ignore_ascii_case(&self.to_column))
            .ok_or_else(|| {
                RuzuError::StorageError(format!("CSV missing required '{}' column", self.to_column))
            })?;

        // Find property columns
        let mut prop_indices = Vec::new();
        for (col_name, _) in &self.property_columns {
            match headers.iter().position(|h| h == col_name) {
                Some(idx) => prop_indices.push(idx),
                None => {
                    return Err(RuzuError::StorageError(format!(
                        "CSV missing property column '{col_name}'"
                    )));
                }
            }
        }

        Ok((from_idx, to_idx, prop_indices))
    }

    /// Parses a field value based on type.
    fn parse_field(
        &self,
        field: &str,
        data_type: DataType,
        row_num: u64,
        col_name: &str,
    ) -> std::result::Result<Value, ImportError> {
        parse_field_with_interner(field, data_type, row_num, col_name, self.interner.as_ref())
    }

    /// Parses a CSV record into a `ParsedRelationship`.
    fn parse_record(
        &self,
        record: &csv::StringRecord,
        from_idx: usize,
        to_idx: usize,
        prop_indices: &[usize],
        row_num: u64,
    ) -> std::result::Result<ParsedRelationship, ImportError> {
        // Parse FROM key (assumed to be string for now)
        let from_field = record.get(from_idx).unwrap_or("");

        // Validate non-empty keys
        if from_field.is_empty() {
            return Err(ImportError::column_error(
                row_num,
                &self.from_column,
                "FROM key cannot be empty",
            ));
        }

        // Parse TO key
        let to_field = record.get(to_idx).unwrap_or("");
        if to_field.is_empty() {
            return Err(ImportError::column_error(
                row_num,
                &self.to_column,
                "TO key cannot be empty",
            ));
        }

        // Intern keys if interner is available
        let from_key = if let Some(ref interner) = self.interner {
            let interned = interner.write().intern(from_field);
            Value::String(interned.to_string())
        } else {
            Value::String(from_field.to_string())
        };

        let to_key = if let Some(ref interner) = self.interner {
            let interned = interner.write().intern(to_field);
            Value::String(interned.to_string())
        } else {
            Value::String(to_field.to_string())
        };

        // Parse properties
        let mut properties = Vec::with_capacity(self.property_columns.len());
        for (i, &csv_idx) in prop_indices.iter().enumerate() {
            let field = record.get(csv_idx).unwrap_or("");
            let (col_name, data_type) = &self.property_columns[i];
            let value = self.parse_field(field, *data_type, row_num, col_name)?;
            properties.push(value);
        }

        Ok(ParsedRelationship {
            from_key,
            to_key,
            properties,
        })
    }

    /// Loads relationships from a CSV file.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be parsed.
    pub fn load(
        &self,
        path: &Path,
        progress_callback: Option<ProgressCallback>,
    ) -> Result<(Vec<ParsedRelationship>, ImportResult)> {
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
    ) -> Result<(Vec<ParsedRelationship>, ImportResult)> {
        let parser = CsvParser::new(self.config.clone());
        let mut progress = ImportProgress::new();

        // Start timing
        progress.start();

        // Get total lines for progress
        if let Ok(total) = CsvParser::count_lines(path) {
            progress.rows_total = Some(total.saturating_sub(1));
        }

        // Get headers and validate
        let headers = parser.headers(path)?;
        let (from_idx, to_idx, prop_indices) = self.validate_headers(&headers)?;

        // Parse records
        let mut reader = parser.reader_from_path(path)?;
        let mut relationships = Vec::new();
        let mut row_num = 1u64;
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

            match self.parse_record(&record, from_idx, to_idx, &prop_indices, row_num) {
                Ok(rel) => {
                    relationships.push(rel);
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
            if relationships.len() % self.config.batch_size == 0 {
                progress.update(self.config.batch_size as u64, batch_bytes);
                batch_bytes = 0;

                if let Some(ref callback) = progress_callback {
                    callback(progress.clone());
                }
            }
        }

        // Update final progress
        let remaining_rows = relationships.len() as u64 % self.config.batch_size as u64;
        if remaining_rows > 0 || batch_bytes > 0 {
            progress.update(remaining_rows, batch_bytes);
        }

        if let Some(callback) = progress_callback {
            callback(progress.clone());
        }

        let result = ImportResult::from_progress(progress);
        Ok((relationships, result))
    }

    /// Parallel loading using mmap and rayon.
    fn load_parallel(
        &self,
        path: &Path,
        file_size: u64,
        progress_callback: Option<ProgressCallback>,
    ) -> Result<(Vec<ParsedRelationship>, ImportResult)> {
        let mut progress = ImportProgress::new();
        progress.start();

        // Open file with mmap
        let mut reader = MmapReader::open(path, &self.config)?;
        let data = reader.as_slice()?;

        // Get headers first (we need to validate before parallel processing)
        let parser = CsvParser::new(self.config.clone());
        let headers = parser.headers(path)?;
        let (from_idx, to_idx, prop_indices) = self.validate_headers(&headers)?;

        // Estimate total rows for progress
        let avg_row_size = if data.is_empty() {
            100
        } else {
            let sample_size = 64 * 1024.min(data.len());
            let newlines = data[..sample_size].iter().fold(0usize, |n, &b| n + usize::from(b == b'\n'));
            if newlines > 0 {
                sample_size / newlines
            } else {
                100
            }
        };
        progress.rows_total = Some((file_size as usize / avg_row_size) as u64);

        // Create parsing closure that captures loader info
        let from_col = self.from_column.clone();
        let to_col = self.to_column.clone();
        let prop_cols: Vec<(String, DataType)> = self.property_columns.clone();
        let prop_indices_clone = prop_indices.clone();
        let interner = self.interner.clone();

        let parse_row = move |record: &csv::ByteRecord,
                              row_num: u64|
              -> std::result::Result<Vec<Value>, ImportError> {
            // Parse FROM key
            let from_bytes = record.get(from_idx).unwrap_or(b"");
            let from_field = std::str::from_utf8(from_bytes).map_err(|e| {
                ImportError::column_error(row_num, &from_col, format!("Invalid UTF-8: {e}"))
            })?;

            // Parse TO key
            let to_bytes = record.get(to_idx).unwrap_or(b"");
            let to_field = std::str::from_utf8(to_bytes).map_err(|e| {
                ImportError::column_error(row_num, &to_col, format!("Invalid UTF-8: {e}"))
            })?;

            // Validate non-empty keys
            if from_field.is_empty() {
                return Err(ImportError::column_error(
                    row_num,
                    &from_col,
                    "FROM key cannot be empty",
                ));
            }
            if to_field.is_empty() {
                return Err(ImportError::column_error(
                    row_num,
                    &to_col,
                    "TO key cannot be empty",
                ));
            }

            // Build result: [from_key, to_key, ...properties]
            let mut values = Vec::with_capacity(2 + prop_cols.len());

            // Intern FROM/TO keys if interner is available
            if let Some(ref interner) = interner {
                let from_interned = interner.write().intern(from_field);
                let to_interned = interner.write().intern(to_field);
                values.push(Value::String(from_interned.to_string()));
                values.push(Value::String(to_interned.to_string()));
            } else {
                values.push(Value::String(from_field.to_string()));
                values.push(Value::String(to_field.to_string()));
            }

            // Parse properties
            for (i, &csv_idx) in prop_indices_clone.iter().enumerate() {
                let field_bytes = record.get(csv_idx).unwrap_or(b"");
                let field = std::str::from_utf8(field_bytes).map_err(|e| {
                    ImportError::column_error(
                        row_num,
                        &prop_cols[i].0,
                        format!("Invalid UTF-8: {e}"),
                    )
                })?;
                let value = parse_field_with_interner(
                    field,
                    prop_cols[i].1,
                    row_num,
                    &prop_cols[i].0,
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
        let (raw_rows, errors, bytes_processed) = parallel_read_all(data, &self.config, parse_row)?;

        // Convert raw rows back to ParsedRelationship
        let relationships: Vec<ParsedRelationship> = raw_rows
            .into_iter()
            .map(|values| {
                let from_key = values.first().cloned().unwrap_or(Value::Null);
                let to_key = values.get(1).cloned().unwrap_or(Value::Null);
                let properties: Vec<Value> = values.into_iter().skip(2).collect();
                ParsedRelationship {
                    from_key,
                    to_key,
                    properties,
                }
            })
            .collect();

        // Update progress with results
        progress.rows_processed = relationships.len() as u64;
        progress.rows_failed = errors.len() as u64;
        progress.bytes_read = bytes_processed;
        progress.errors = errors;

        // Final progress report
        if let Some(callback) = progress_callback {
            callback(progress.clone());
        }

        let result = ImportResult::from_progress(progress);
        Ok((relationships, result))
    }

    /// Loads relationships from a CSV file using streaming with a batch callback.
    ///
    /// Unlike `load()`, this method does NOT accumulate all rows in memory.
    /// Instead, it calls the batch callback for each batch of relationships,
    /// allowing the caller to process and discard them incrementally.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the CSV file
    /// * `batch_callback` - Called with each batch of relationships; returns Ok(()) to continue
    /// * `progress_callback` - Optional progress reporting callback
    ///
    /// # Memory Behavior
    ///
    /// Memory usage is bounded by `config.batch_size` regardless of file size.
    /// The batch callback receives relationships and should process them before returning.
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
        F: FnMut(Vec<ParsedRelationship>) -> Result<()>,
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

        // Get headers and validate/find column indices
        let headers = parser.headers(path)?;
        let (from_idx, to_idx, prop_indices) = self.find_column_indices(&headers)?;

        // Parse records in batches
        let mut reader = parser.reader_from_path(path)?;
        let mut batch: Vec<ParsedRelationship> = Vec::with_capacity(self.config.batch_size);
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

            match self.parse_relationship_record(&record, from_idx, to_idx, &prop_indices, row_num) {
                Ok(rel) => {
                    batch.push(rel);
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

    /// Parse a single relationship record.
    fn parse_relationship_record(
        &self,
        record: &csv::StringRecord,
        from_idx: usize,
        to_idx: usize,
        prop_indices: &[usize],
        row_num: u64,
    ) -> std::result::Result<ParsedRelationship, ImportError> {
        // Parse FROM key
        let from_field = record.get(from_idx).unwrap_or("");
        if from_field.is_empty() {
            return Err(ImportError::column_error(
                row_num,
                &self.from_column,
                "FROM key cannot be empty",
            ));
        }

        // Parse TO key
        let to_field = record.get(to_idx).unwrap_or("");
        if to_field.is_empty() {
            return Err(ImportError::column_error(
                row_num,
                &self.to_column,
                "TO key cannot be empty",
            ));
        }

        // Build keys with optional interning
        let (from_key, to_key) = if let Some(ref interner) = self.interner {
            let from_interned = interner.write().intern(from_field);
            let to_interned = interner.write().intern(to_field);
            (
                Value::String(from_interned.to_string()),
                Value::String(to_interned.to_string()),
            )
        } else {
            (
                Value::String(from_field.to_string()),
                Value::String(to_field.to_string()),
            )
        };

        // Parse properties
        let mut properties = Vec::with_capacity(self.property_columns.len());
        for (i, &csv_idx) in prop_indices.iter().enumerate() {
            let field = record.get(csv_idx).unwrap_or("");
            let value = parse_field_with_interner(
                field,
                self.property_columns[i].1,
                row_num,
                &self.property_columns[i].0,
                self.interner.as_ref(),
            )?;
            properties.push(value);
        }

        Ok(ParsedRelationship {
            from_key,
            to_key,
            properties,
        })
    }

    /// Find column indices for FROM, TO, and property columns.
    fn find_column_indices(
        &self,
        headers: &[String],
    ) -> Result<(usize, usize, Vec<usize>)> {
        let from_idx = headers
            .iter()
            .position(|h| h == &self.from_column)
            .ok_or_else(|| {
                RuzuError::StorageError(format!(
                    "CSV missing required FROM column '{}'",
                    self.from_column
                ))
            })?;

        let to_idx = headers
            .iter()
            .position(|h| h == &self.to_column)
            .ok_or_else(|| {
                RuzuError::StorageError(format!(
                    "CSV missing required TO column '{}'",
                    self.to_column
                ))
            })?;

        let mut prop_indices = Vec::new();
        for (prop_name, _) in &self.property_columns {
            let idx = headers.iter().position(|h| h == prop_name).ok_or_else(|| {
                RuzuError::StorageError(format!("CSV missing property column '{prop_name}'"))
            })?;
            prop_indices.push(idx);
        }

        Ok((from_idx, to_idx, prop_indices))
    }
}

/// Static field parsing function for use in closures (without interning).
#[allow(dead_code)]
fn parse_field_static(
    field: &str,
    data_type: DataType,
    row_num: u64,
    col_name: &str,
) -> std::result::Result<Value, ImportError> {
    parse_field_with_interner(field, data_type, row_num, col_name, None)
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
            "true" | "1" | "yes" | "t" => Ok(Value::Bool(true)),
            "false" | "0" | "no" | "f" => Ok(Value::Bool(false)),
            _ => Err(ImportError::column_error(
                row_num,
                col_name,
                format!("Invalid BOOL: {field}"),
            )),
        },
        DataType::String => {
            if let Some(interner) = interner {
                let interned = interner.write().intern(field);
                Ok(Value::String(interned.to_string()))
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
    use std::io::Write;
    use std::sync::Arc;
    use tempfile::TempDir;

    fn create_test_csv(content: &str) -> (std::path::PathBuf, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("test.csv");

        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(content.as_bytes()).unwrap();

        (path, temp_dir)
    }

    #[test]
    fn test_load_simple_relationships() {
        let loader = RelLoader::with_default_columns(
            vec![("since".to_string(), DataType::Int64)],
            CsvImportConfig::default(),
        );

        let csv_content = "FROM,TO,since\nAlice,Bob,2020\nBob,Charlie,2019\n";
        let (path, _temp) = create_test_csv(csv_content);

        let (rels, result) = loader.load(&path, None).unwrap();

        assert_eq!(rels.len(), 2);
        assert!(result.is_success());

        assert_eq!(rels[0].from_key, Value::String("Alice".to_string()));
        assert_eq!(rels[0].to_key, Value::String("Bob".to_string()));
        assert_eq!(rels[0].properties[0], Value::Int64(2020));
    }

    #[test]
    fn test_custom_column_names() {
        let loader = RelLoader::new(
            "source".to_string(),
            "target".to_string(),
            vec![],
            CsvImportConfig::default(),
        );

        let csv_content = "source,target\nA,B\nB,C\n";
        let (path, _temp) = create_test_csv(csv_content);

        let (rels, result) = loader.load(&path, None).unwrap();

        assert_eq!(rels.len(), 2);
        assert!(result.is_success());
    }

    #[test]
    fn test_missing_from_column() {
        let loader = RelLoader::with_default_columns(vec![], CsvImportConfig::default());

        let csv_content = "TO,weight\nBob,100\n"; // Missing FROM
        let (path, _temp) = create_test_csv(csv_content);

        let result = loader.load(&path, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_key_error() {
        let loader = RelLoader::with_default_columns(vec![], CsvImportConfig::default());

        let csv_content = "FROM,TO\nAlice,\n"; // Empty TO
        let (path, _temp) = create_test_csv(csv_content);

        let result = loader.load(&path, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_ignore_errors() {
        let loader = RelLoader::with_default_columns(
            vec![],
            CsvImportConfig::default().with_ignore_errors(true),
        );

        let csv_content = "FROM,TO\nAlice,Bob\nCharlie,\nDiana,Eve\n"; // Row 2 has empty TO
        let (path, _temp) = create_test_csv(csv_content);

        let (rels, result) = loader.load(&path, None).unwrap();

        assert_eq!(rels.len(), 2); // Alice->Bob and Diana->Eve
        assert_eq!(result.rows_failed, 1);
    }

    #[test]
    fn test_load_sequential_explicit() {
        let loader = RelLoader::with_default_columns(
            vec![],
            CsvImportConfig::default().with_parallel(false),
        );

        let csv_content = "FROM,TO\nAlice,Bob\nBob,Charlie\nCharlie,Diana\n";
        let (path, _temp) = create_test_csv(csv_content);

        let (rels, result) = loader.load(&path, None).unwrap();

        assert_eq!(rels.len(), 3);
        assert!(result.is_success());
    }

    #[test]
    fn test_load_with_string_interning() {
        // Enable string interning
        let config = CsvImportConfig::default()
            .with_parallel(false)
            .with_intern_strings(true);
        let loader = RelLoader::with_default_columns(vec![], config);

        // CSV with repeated node names (typical in relationship data)
        let csv_content = "FROM,TO\nAlice,Bob\nAlice,Charlie\nBob,Alice\nCharlie,Alice\n";
        let (path, _temp) = create_test_csv(csv_content);

        let (rels, result) = loader.load(&path, None).unwrap();

        assert_eq!(rels.len(), 4);
        assert!(result.is_success());

        // Check interner statistics - should have hits since names repeat
        if let Some(ref interner) = loader.interner {
            let interner = interner.read();
            // Only 3 unique names: Alice, Bob, Charlie
            assert_eq!(interner.unique_count(), 3);
            // Should have cache hits since Alice, Bob, Charlie each appear multiple times
            assert!(interner.hit_rate() > 0.0);
        }
    }

    #[test]
    fn test_load_with_shared_interner() {
        let interner = shared_interner();

        let config = CsvImportConfig::default().with_parallel(false);
        let loader = RelLoader::with_interner(
            "FROM".to_string(),
            "TO".to_string(),
            vec![],
            config,
            Arc::clone(&interner),
        );

        // Same person in FROM and TO
        let csv_content = "FROM,TO\nAlice,Bob\nAlice,Bob\nAlice,Bob\n";
        let (path, _temp) = create_test_csv(csv_content);

        let (rels, result) = loader.load(&path, None).unwrap();

        assert_eq!(rels.len(), 3);
        assert!(result.is_success());

        // Check shared interner was used
        let interner = interner.read();
        assert_eq!(interner.unique_count(), 2); // Only "Alice" and "Bob"
        assert_eq!(interner.hits(), 4); // 4 cache hits (3 Alice hits, 3 Bob hits - 2 initial misses = 4 hits)
    }
}
