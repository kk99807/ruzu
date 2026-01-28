//! Vectorized execution module.
//!
//! This module provides vectorized (batch-oriented) execution primitives
//! built on Apache Arrow's columnar data format.

pub mod batch;
pub mod evaluator;

pub use batch::{SelectionVector, VectorizedBatch, DEFAULT_BATCH_SIZE};
pub use evaluator::VectorizedEvaluator;
