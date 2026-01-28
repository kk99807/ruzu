//! Parser for Cypher query language subset.

pub mod ast;
mod grammar;

pub use grammar::parse_query;
