//! CSV parser wrapper with configurable options.

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use crate::error::{Result, RuzuError};
use crate::storage::csv::CsvImportConfig;

/// CSV parser that wraps the csv crate with custom configuration.
pub struct CsvParser {
    /// Configuration for parsing.
    config: CsvImportConfig,
}

impl CsvParser {
    /// Creates a new CSV parser with the given configuration.
    #[must_use]
    pub fn new(config: CsvImportConfig) -> Self {
        Self { config }
    }

    /// Creates a parser with default configuration.
    #[must_use]
    pub fn default_config() -> Self {
        Self::new(CsvImportConfig::default())
    }

    /// Builds a `csv::Reader` from a file path.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be opened.
    pub fn reader_from_path(&self, path: &Path) -> Result<csv::Reader<File>> {
        let file = File::open(path)
            .map_err(|e| RuzuError::StorageError(format!("Failed to open CSV file: {e}")))?;

        let reader = self.build_reader(file);
        Ok(reader)
    }

    /// Builds a `csv::Reader` from any Read impl.
    fn build_reader<R: std::io::Read>(&self, rdr: R) -> csv::Reader<R> {
        csv::ReaderBuilder::new()
            .delimiter(self.config.delimiter as u8)
            .quote(self.config.quote as u8)
            .escape(Some(self.config.escape as u8))
            .has_headers(self.config.has_header)
            .flexible(self.config.ignore_errors)
            .from_reader(rdr)
    }

    /// Counts the number of lines in a file (for progress reporting).
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read.
    pub fn count_lines(path: &Path) -> Result<u64> {
        let file = File::open(path).map_err(|e| {
            RuzuError::StorageError(format!("Failed to open file for counting: {e}"))
        })?;

        let reader = BufReader::new(file);
        let count = reader.lines().count() as u64;

        Ok(count)
    }

    /// Returns the file size in bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if the file metadata cannot be read.
    pub fn file_size(path: &Path) -> Result<u64> {
        let metadata = std::fs::metadata(path)
            .map_err(|e| RuzuError::StorageError(format!("Failed to get file metadata: {e}")))?;

        Ok(metadata.len())
    }

    /// Returns the configuration.
    #[must_use]
    pub fn config(&self) -> &CsvImportConfig {
        &self.config
    }

    /// Parses a CSV file and returns all records as string vectors.
    ///
    /// # Errors
    ///
    /// Returns an error if parsing fails.
    pub fn parse_all(&self, path: &Path) -> Result<Vec<csv::StringRecord>> {
        let mut reader = self.reader_from_path(path)?;
        let mut records = Vec::new();

        for (idx, result) in reader.records().enumerate() {
            let record = result.map_err(|e| {
                RuzuError::StorageError(format!("Failed to parse CSV row {}: {e}", idx + 1))
            })?;
            records.push(record);
        }

        Ok(records)
    }

    /// Returns an iterator over CSV records.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be opened.
    pub fn iter_records(
        &self,
        path: &Path,
    ) -> Result<impl Iterator<Item = Result<csv::StringRecord>>> {
        let mut reader = self.reader_from_path(path)?;

        // Skip configured rows
        for _ in 0..self.config.skip_rows {
            let mut record = csv::StringRecord::new();
            if reader.read_record(&mut record).is_err() {
                break;
            }
        }

        // Move reader into the iterator
        Ok(CsvRecordIterator { reader })
    }

    /// Returns the headers from a CSV file.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or has no headers.
    pub fn headers(&self, path: &Path) -> Result<Vec<String>> {
        let mut reader = self.reader_from_path(path)?;

        let headers = reader
            .headers()
            .map_err(|e| RuzuError::StorageError(format!("Failed to read CSV headers: {e}")))?;

        Ok(headers.iter().map(String::from).collect())
    }
}

/// Iterator over CSV records.
struct CsvRecordIterator {
    reader: csv::Reader<File>,
}

impl Iterator for CsvRecordIterator {
    type Item = Result<csv::StringRecord>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut record = csv::StringRecord::new();

        match self.reader.read_record(&mut record) {
            Ok(true) => Some(Ok(record)),
            Ok(false) => None,
            Err(e) => Some(Err(RuzuError::StorageError(format!(
                "Failed to read CSV record: {e}"
            )))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_csv(content: &str) -> (std::path::PathBuf, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("test.csv");

        let mut file = File::create(&path).unwrap();
        file.write_all(content.as_bytes()).unwrap();

        (path, temp_dir)
    }

    #[test]
    fn test_parse_simple_csv() {
        let csv_content = "name,age\nAlice,25\nBob,30\n";
        let (path, _temp) = create_test_csv(csv_content);

        let parser = CsvParser::default_config();
        let records = parser.parse_all(&path).unwrap();

        assert_eq!(records.len(), 2);
        assert_eq!(&records[0][0], "Alice");
        assert_eq!(&records[0][1], "25");
    }

    #[test]
    fn test_headers() {
        let csv_content = "name,age,city\nAlice,25,NYC\n";
        let (path, _temp) = create_test_csv(csv_content);

        let parser = CsvParser::default_config();
        let headers = parser.headers(&path).unwrap();

        assert_eq!(headers, vec!["name", "age", "city"]);
    }

    #[test]
    fn test_count_lines() {
        let csv_content = "name,age\nAlice,25\nBob,30\nCharlie,35\n";
        let (path, _temp) = create_test_csv(csv_content);

        let count = CsvParser::count_lines(&path).unwrap();
        assert_eq!(count, 4); // Including header
    }

    #[test]
    fn test_custom_delimiter() {
        let csv_content = "name;age\nAlice;25\nBob;30\n";
        let (path, _temp) = create_test_csv(csv_content);

        let config = CsvImportConfig::new().with_delimiter(';');
        let parser = CsvParser::new(config);
        let records = parser.parse_all(&path).unwrap();

        assert_eq!(records.len(), 2);
        assert_eq!(&records[0][0], "Alice");
    }

    #[test]
    fn test_quoted_fields() {
        let csv_content = r#"name,description
"Alice","Has ""quotes"" inside"
"Bob","Simple description"
"#;
        let (path, _temp) = create_test_csv(csv_content);

        let parser = CsvParser::default_config();
        let records = parser.parse_all(&path).unwrap();

        assert_eq!(records.len(), 2);
        assert_eq!(&records[0][1], r#"Has "quotes" inside"#);
    }

    #[test]
    fn test_no_header() {
        let csv_content = "Alice,25\nBob,30\n";
        let (path, _temp) = create_test_csv(csv_content);

        let config = CsvImportConfig::new().with_header(false);
        let parser = CsvParser::new(config);
        let records = parser.parse_all(&path).unwrap();

        assert_eq!(records.len(), 2);
        assert_eq!(&records[0][0], "Alice");
    }
}
