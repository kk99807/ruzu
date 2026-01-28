//! String interning for CSV import.
//!
//! This module provides string deduplication during import to reduce memory
//! allocations for repeated string values (e.g., category columns, status fields).
//!
//! # Usage
//!
//! String interning is optional and disabled by default. Enable it via
//! `CsvImportConfig::with_intern_strings(true)` for columns with high repetition.
//!
//! # Thread Safety
//!
//! For parallel import, use `SharedInterner` which wraps the interner in a
//! `parking_lot::RwLock` for efficient concurrent access.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;

/// Thread-safe string interner for deduplicating repeated values.
#[derive(Debug, Default)]
pub struct StringInterner {
    /// Interned strings map.
    strings: HashMap<Box<str>, Arc<str>>,
    /// Number of cache hits.
    hits: u64,
    /// Number of cache misses.
    misses: u64,
}

impl StringInterner {
    /// Creates a new empty string interner.
    #[must_use]
    pub fn new() -> Self {
        Self {
            strings: HashMap::new(),
            hits: 0,
            misses: 0,
        }
    }

    /// Creates a new interner with pre-allocated capacity.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            strings: HashMap::with_capacity(capacity),
            hits: 0,
            misses: 0,
        }
    }

    /// Intern a string, returning a reference-counted string.
    ///
    /// If the string was previously interned, returns the existing Arc.
    /// Otherwise, creates a new Arc and stores it for future lookups.
    pub fn intern(&mut self, s: &str) -> Arc<str> {
        if let Some(existing) = self.strings.get(s) {
            self.hits += 1;
            return Arc::clone(existing);
        }

        self.misses += 1;
        let arc: Arc<str> = Arc::from(s);
        self.strings.insert(s.into(), Arc::clone(&arc));
        arc
    }

    /// Returns the hit rate (0.0 to 1.0).
    ///
    /// A higher hit rate indicates more string reuse and better memory savings.
    #[must_use]
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total > 0 {
            self.hits as f64 / total as f64
        } else {
            0.0
        }
    }

    /// Returns the number of unique strings stored.
    #[must_use]
    pub fn unique_count(&self) -> usize {
        self.strings.len()
    }

    /// Returns the number of cache hits.
    #[must_use]
    pub fn hits(&self) -> u64 {
        self.hits
    }

    /// Returns the number of cache misses.
    #[must_use]
    pub fn misses(&self) -> u64 {
        self.misses
    }

    /// Clear all interned strings and reset statistics.
    pub fn clear(&mut self) {
        self.strings.clear();
        self.hits = 0;
        self.misses = 0;
    }
}

/// Thread-safe shared interner for parallel processing.
pub type SharedInterner = Arc<RwLock<StringInterner>>;

/// Creates a new thread-safe shared interner.
#[must_use]
pub fn shared_interner() -> SharedInterner {
    Arc::new(RwLock::new(StringInterner::new()))
}

/// Creates a new thread-safe shared interner with pre-allocated capacity.
#[must_use]
pub fn shared_interner_with_capacity(capacity: usize) -> SharedInterner {
    Arc::new(RwLock::new(StringInterner::with_capacity(capacity)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intern_new_string() {
        let mut interner = StringInterner::new();

        let s1 = interner.intern("hello");
        assert_eq!(&*s1, "hello");
        assert_eq!(interner.unique_count(), 1);
        assert_eq!(interner.hits(), 0);
        assert_eq!(interner.misses(), 1);
    }

    #[test]
    fn test_intern_existing_string() {
        let mut interner = StringInterner::new();

        let s1 = interner.intern("hello");
        let s2 = interner.intern("hello");

        // Should return same Arc
        assert!(Arc::ptr_eq(&s1, &s2));
        assert_eq!(interner.unique_count(), 1);
        assert_eq!(interner.hits(), 1);
        assert_eq!(interner.misses(), 1);
    }

    #[test]
    fn test_intern_multiple_strings() {
        let mut interner = StringInterner::new();

        let s1 = interner.intern("hello");
        let s2 = interner.intern("world");
        let s3 = interner.intern("hello");

        assert!(!Arc::ptr_eq(&s1, &s2));
        assert!(Arc::ptr_eq(&s1, &s3));
        assert_eq!(interner.unique_count(), 2);
    }

    #[test]
    fn test_hit_rate() {
        let mut interner = StringInterner::new();

        // Empty interner
        assert_eq!(interner.hit_rate(), 0.0);

        // All misses
        interner.intern("a");
        interner.intern("b");
        interner.intern("c");
        assert_eq!(interner.hit_rate(), 0.0);

        // 50% hits
        interner.intern("a");
        interner.intern("b");
        interner.intern("c");
        assert_eq!(interner.hit_rate(), 0.5);
    }

    #[test]
    fn test_clear() {
        let mut interner = StringInterner::new();

        interner.intern("hello");
        interner.intern("hello");

        assert_eq!(interner.unique_count(), 1);
        assert_eq!(interner.hits(), 1);

        interner.clear();

        assert_eq!(interner.unique_count(), 0);
        assert_eq!(interner.hits(), 0);
        assert_eq!(interner.misses(), 0);
    }

    #[test]
    fn test_with_capacity() {
        let interner = StringInterner::with_capacity(100);
        assert_eq!(interner.unique_count(), 0);
    }

    #[test]
    fn test_shared_interner() {
        let shared = shared_interner();

        // Write lock to intern
        {
            let mut interner = shared.write();
            interner.intern("test");
        }

        // Read lock to check
        {
            let interner = shared.read();
            assert_eq!(interner.unique_count(), 1);
        }
    }

    #[test]
    fn test_shared_interner_concurrent() {
        use std::thread;

        let shared = shared_interner();
        let mut handles = vec![];

        for i in 0..4 {
            let shared_clone = Arc::clone(&shared);
            let handle = thread::spawn(move || {
                for j in 0..100 {
                    let mut interner = shared_clone.write();
                    interner.intern(&format!("string_{}", j % 10));
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        let interner = shared.read();
        // Should have at most 10 unique strings
        assert!(interner.unique_count() <= 10);
        // Should have lots of hits
        assert!(interner.hit_rate() > 0.5);
    }
}
