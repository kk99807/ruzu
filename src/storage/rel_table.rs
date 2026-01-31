//! Relationship table storage using Compressed Sparse Row (CSR) format.
//!
//! This module provides storage for relationship/edge data in a graph database.
//! Relationships are stored using CSR format which provides efficient:
//! - Forward traversal: given source node, find all destination nodes
//! - Backward traversal: given destination node, find all source nodes
//!
//! # CSR Format
//!
//! The CSR format uses three parallel arrays:
//! - `offsets`: For each node, stores the starting index in `neighbors`
//! - `neighbors`: Stores destination node IDs for all edges
//! - `rel_ids`: Stores relationship IDs parallel to neighbors
//!
//! Example:
//! ```text
//! Node 0 -> [1, 3]     offsets = [0, 2, 3, 6]
//! Node 1 -> [2]        neighbors = [1, 3, 2, 0, 1, 3]
//! Node 2 -> [0, 1, 3]  rel_ids = [0, 1, 2, 3, 4, 5]
//! ```

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::catalog::RelTableSchema;
use crate::error::{Result, RuzuError};
use crate::storage::ColumnStorage;
use crate::types::Value;

/// Number of nodes per node group (2^17 = 131072).
pub const NODE_GROUP_SIZE: usize = 131_072;

/// Compressed Sparse Row storage for relationships within a node group.
///
/// Each `CsrNodeGroup` stores edges for a contiguous range of source nodes.
/// The node group contains:
/// - Offsets array: `num_nodes + 1` entries mapping node local IDs to edge ranges
/// - Neighbors array: destination node IDs for all edges
/// - Rel IDs array: unique relationship IDs parallel to neighbors
/// - Properties: columnar storage for relationship properties
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CsrNodeGroup {
    /// Node group identifier (which chunk of source nodes).
    pub group_id: u32,
    /// Number of nodes in this group (may be less than `NODE_GROUP_SIZE` for last group).
    pub num_nodes: u32,
    /// Edge start offsets for each node. Length = `num_nodes` + 1.
    /// offsets[i] is the starting index in neighbors for node i's edges.
    /// offsets[`num_nodes`] equals `neighbors.len()`.
    pub offsets: Vec<u64>,
    /// Destination node IDs for all edges in this group.
    pub neighbors: Vec<u64>,
    /// Unique relationship IDs parallel to neighbors.
    pub rel_ids: Vec<u64>,
    /// Property columns for relationships (parallel to neighbors).
    #[serde(default)]
    pub properties: Vec<ColumnStorage>,
}

impl CsrNodeGroup {
    /// Creates a new empty CSR node group.
    #[must_use]
    pub fn new(group_id: u32) -> Self {
        Self {
            group_id,
            num_nodes: 0,
            offsets: vec![0], // Start with single 0 offset
            neighbors: Vec::new(),
            rel_ids: Vec::new(),
            properties: Vec::new(),
        }
    }

    /// Creates a CSR node group with preallocated capacity.
    #[must_use]
    pub fn with_capacity(group_id: u32, num_nodes: u32, estimated_edges: usize) -> Self {
        let mut offsets = Vec::with_capacity(num_nodes as usize + 1);
        offsets.push(0);

        Self {
            group_id,
            num_nodes,
            offsets,
            neighbors: Vec::with_capacity(estimated_edges),
            rel_ids: Vec::with_capacity(estimated_edges),
            properties: Vec::new(),
        }
    }

    /// Returns the neighbors (destination nodes) for a given local node ID.
    ///
    /// # Panics
    ///
    /// Panics if `local_node_id >= num_nodes`.
    #[must_use]
    pub fn get_neighbors(&self, local_node_id: u32) -> &[u64] {
        let start = self.offsets[local_node_id as usize] as usize;
        let end = self.offsets[local_node_id as usize + 1] as usize;
        &self.neighbors[start..end]
    }

    /// Returns the relationship IDs for a given local node ID.
    ///
    /// # Panics
    ///
    /// Panics if `local_node_id >= num_nodes`.
    #[must_use]
    pub fn get_rel_ids(&self, local_node_id: u32) -> &[u64] {
        let start = self.offsets[local_node_id as usize] as usize;
        let end = self.offsets[local_node_id as usize + 1] as usize;
        &self.rel_ids[start..end]
    }

    /// Returns the number of edges in this node group.
    #[must_use]
    pub fn num_edges(&self) -> usize {
        self.neighbors.len()
    }

    /// Returns the degree (number of outgoing edges) for a given local node ID.
    ///
    /// # Panics
    ///
    /// Panics if `local_node_id >= num_nodes`.
    #[must_use]
    pub fn degree(&self, local_node_id: u32) -> usize {
        let start = self.offsets[local_node_id as usize] as usize;
        let end = self.offsets[local_node_id as usize + 1] as usize;
        end - start
    }

    /// Checks if the CSR invariants are satisfied.
    ///
    /// # Errors
    ///
    /// Returns an error if any CSR invariant is violated (offsets, monotonicity,
    /// or neighbor/rel-id array length mismatches).
    ///
    /// # Panics
    ///
    /// Panics if the offsets array is non-empty but `last()` returns `None` (unreachable).
    pub fn validate(&self) -> Result<()> {
        // Invariant 1: offsets[0] == 0
        if self.offsets.is_empty() || self.offsets[0] != 0 {
            return Err(RuzuError::StorageError("CSR offsets[0] must be 0".into()));
        }

        // Invariant 2: offsets length = num_nodes + 1
        if self.offsets.len() != self.num_nodes as usize + 1 {
            return Err(RuzuError::StorageError(format!(
                "CSR offsets length {} != num_nodes + 1 ({})",
                self.offsets.len(),
                self.num_nodes + 1
            )));
        }

        // Invariant 3: offsets are monotonically non-decreasing
        for i in 0..self.num_nodes as usize {
            if self.offsets[i] > self.offsets[i + 1] {
                return Err(RuzuError::StorageError(format!(
                    "CSR offsets not monotonic at index {}: {} > {}",
                    i,
                    self.offsets[i],
                    self.offsets[i + 1]
                )));
            }
        }

        // Invariant 4: offsets[num_nodes] == neighbors.len()
        let last_offset = *self.offsets.last().unwrap();
        if last_offset as usize != self.neighbors.len() {
            return Err(RuzuError::StorageError(format!(
                "CSR final offset {} != neighbors.len() {}",
                last_offset,
                self.neighbors.len()
            )));
        }

        // Invariant 5: rel_ids.len() == neighbors.len()
        if self.rel_ids.len() != self.neighbors.len() {
            return Err(RuzuError::StorageError(format!(
                "CSR rel_ids.len() {} != neighbors.len() {}",
                self.rel_ids.len(),
                self.neighbors.len()
            )));
        }

        Ok(())
    }

    /// Adds a node's edges in bulk (used during construction).
    ///
    /// This extends the offsets array and appends the edges.
    /// Nodes must be added in order (node 0, then 1, etc.).
    ///
    /// # Panics
    ///
    /// Panics if `neighbors` and `rel_ids` have different lengths.
    pub fn add_node_edges(&mut self, neighbors: &[u64], rel_ids: &[u64]) {
        assert_eq!(
            neighbors.len(),
            rel_ids.len(),
            "neighbors and rel_ids must have same length"
        );

        // Extend neighbors and rel_ids
        self.neighbors.extend_from_slice(neighbors);
        self.rel_ids.extend_from_slice(rel_ids);

        // Add offset for next node
        self.offsets.push(self.neighbors.len() as u64);
        self.num_nodes += 1;
    }

    /// Inserts a single edge for a node.
    ///
    /// Note: This is expensive as it may require rebuilding the CSR.
    /// For bulk insertion, use `add_node_edges` during initial construction.
    ///
    /// # Errors
    ///
    /// Returns an error if `local_node_id` is out of range.
    pub fn insert_edge(&mut self, local_node_id: u32, neighbor: u64, rel_id: u64) -> Result<()> {
        if local_node_id as usize >= self.num_nodes as usize {
            return Err(RuzuError::StorageError(format!(
                "Node ID {} out of range (max {})",
                local_node_id,
                self.num_nodes - 1
            )));
        }

        // Find insertion point (end of this node's edges)
        let insert_idx = self.offsets[local_node_id as usize + 1] as usize;

        // Insert into neighbors and rel_ids
        self.neighbors.insert(insert_idx, neighbor);
        self.rel_ids.insert(insert_idx, rel_id);

        // Update all subsequent offsets
        for i in (local_node_id as usize + 1)..self.offsets.len() {
            self.offsets[i] += 1;
        }

        Ok(())
    }

    /// Serializes the CSR node group to bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if bincode serialization fails.
    pub fn serialize(&self) -> Result<Vec<u8>> {
        bincode::serialize(self)
            .map_err(|e| RuzuError::StorageError(format!("Failed to serialize CSR: {e}")))
    }

    /// Deserializes a CSR node group from bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if the data is malformed or bincode deserialization fails.
    pub fn deserialize(data: &[u8]) -> Result<Self> {
        bincode::deserialize(data)
            .map_err(|e| RuzuError::StorageError(format!("Failed to deserialize CSR: {e}")))
    }
}

/// Serializable representation of relationship table data.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RelTableData {
    /// Forward CSR groups (src -> dst).
    pub forward_groups: Vec<CsrNodeGroup>,
    /// Backward CSR groups (dst -> src).
    pub backward_groups: Vec<CsrNodeGroup>,
    /// Next relationship ID to allocate.
    pub next_rel_id: u64,
    /// Property values indexed by `rel_id`.
    pub properties: HashMap<u64, Vec<Value>>,
}

/// In-memory relationship table with CSR storage.
///
/// Maintains both forward (src -> dst) and backward (dst -> src) indices
/// for efficient traversal in both directions.
#[derive(Debug)]
pub struct RelTable {
    /// Table schema.
    schema: Arc<RelTableSchema>,
    /// Forward CSR groups indexed by source node group ID.
    forward_groups: HashMap<u32, CsrNodeGroup>,
    /// Backward CSR groups indexed by destination node group ID.
    backward_groups: HashMap<u32, CsrNodeGroup>,
    /// Next relationship ID to allocate.
    next_rel_id: u64,
    /// Property values indexed by `rel_id`.
    properties: HashMap<u64, Vec<Value>>,
}

impl RelTable {
    /// Creates a new empty relationship table.
    #[must_use]
    pub fn new(schema: Arc<RelTableSchema>) -> Self {
        Self {
            schema,
            forward_groups: HashMap::new(),
            backward_groups: HashMap::new(),
            next_rel_id: 0,
            properties: HashMap::new(),
        }
    }

    /// Creates a relationship table from serialized data.
    ///
    /// # Panics
    ///
    /// Panics in debug builds if CSR invariants are violated (empty offsets,
    /// mismatched neighbor/rel-id counts).
    #[must_use]
    pub fn from_data(schema: Arc<RelTableSchema>, data: RelTableData) -> Self {
        // Debug assertions for CSR invariants
        #[cfg(debug_assertions)]
        {
            // Validate forward CSR groups
            for group in &data.forward_groups {
                debug_assert!(
                    !group.offsets.is_empty(),
                    "CSR group offsets cannot be empty"
                );
                debug_assert!(
                    group.offsets.len() >= 2,
                    "CSR group should have at least 2 offsets (for num_nodes = 1), got {}",
                    group.offsets.len()
                );
                // Validate offsets.len() == num_nodes + 1
                let num_nodes = group.offsets.len() - 1;
                let total_edges = group.offsets.last().unwrap() - group.offsets[0];
                debug_assert_eq!(
                    group.neighbors.len(),
                    total_edges as usize,
                    "Forward CSR group {}: neighbors.len() ({}) != total edges ({})",
                    group.group_id,
                    group.neighbors.len(),
                    total_edges
                );
                debug_assert_eq!(
                    group.rel_ids.len(),
                    total_edges as usize,
                    "Forward CSR group {}: rel_ids.len() ({}) != total edges ({})",
                    group.group_id,
                    group.rel_ids.len(),
                    total_edges
                );
                // Validate offsets are monotonically non-decreasing
                for i in 0..num_nodes {
                    debug_assert!(
                        group.offsets[i] <= group.offsets[i + 1],
                        "Forward CSR group {}: offsets not monotonic at {}: {} > {}",
                        group.group_id,
                        i,
                        group.offsets[i],
                        group.offsets[i + 1]
                    );
                }
            }

            // Validate backward CSR groups
            for group in &data.backward_groups {
                debug_assert!(
                    !group.offsets.is_empty(),
                    "CSR group offsets cannot be empty"
                );
                debug_assert!(
                    group.offsets.len() >= 2,
                    "CSR group should have at least 2 offsets (for num_nodes = 1), got {}",
                    group.offsets.len()
                );
                // Validate offsets.len() == num_nodes + 1
                let num_nodes = group.offsets.len() - 1;
                let total_edges = group.offsets.last().unwrap() - group.offsets[0];
                debug_assert_eq!(
                    group.neighbors.len(),
                    total_edges as usize,
                    "Backward CSR group {}: neighbors.len() ({}) != total edges ({})",
                    group.group_id,
                    group.neighbors.len(),
                    total_edges
                );
                debug_assert_eq!(
                    group.rel_ids.len(),
                    total_edges as usize,
                    "Backward CSR group {}: rel_ids.len() ({}) != total edges ({})",
                    group.group_id,
                    group.rel_ids.len(),
                    total_edges
                );
                // Validate offsets are monotonically non-decreasing
                for i in 0..num_nodes {
                    debug_assert!(
                        group.offsets[i] <= group.offsets[i + 1],
                        "Backward CSR group {}: offsets not monotonic at {}: {} > {}",
                        group.group_id,
                        i,
                        group.offsets[i],
                        group.offsets[i + 1]
                    );
                }
            }

            // Validate next_rel_id is greater than all existing rel_ids
            for &rel_id in data.properties.keys() {
                debug_assert!(
                    rel_id < data.next_rel_id,
                    "Relationship ID {} >= next_rel_id {}",
                    rel_id,
                    data.next_rel_id
                );
            }
        }

        let forward_groups = data
            .forward_groups
            .into_iter()
            .map(|g| (g.group_id, g))
            .collect();
        let backward_groups = data
            .backward_groups
            .into_iter()
            .map(|g| (g.group_id, g))
            .collect();

        Self {
            schema,
            forward_groups,
            backward_groups,
            next_rel_id: data.next_rel_id,
            properties: data.properties,
        }
    }

    /// Converts the relationship table to a serializable format.
    #[must_use]
    pub fn to_data(&self) -> RelTableData {
        RelTableData {
            forward_groups: self.forward_groups.values().cloned().collect(),
            backward_groups: self.backward_groups.values().cloned().collect(),
            next_rel_id: self.next_rel_id,
            properties: self.properties.clone(),
        }
    }

    /// Returns the table schema.
    #[must_use]
    pub fn schema(&self) -> &RelTableSchema {
        &self.schema
    }

    /// Inserts a new relationship.
    ///
    /// # Arguments
    ///
    /// * `src_node_id` - Global source node offset
    /// * `dst_node_id` - Global destination node offset
    /// * `props` - Property values (must match schema columns)
    ///
    /// # Returns
    ///
    /// The assigned relationship ID.
    ///
    /// # Errors
    ///
    /// Returns an error if the property count does not match the schema.
    pub fn insert(&mut self, src_node_id: u64, dst_node_id: u64, props: Vec<Value>) -> Result<u64> {
        // Validate property count
        if props.len() != self.schema.columns.len() {
            return Err(RuzuError::ExecutionError(format!(
                "Expected {} properties, got {}",
                self.schema.columns.len(),
                props.len()
            )));
        }

        // Allocate relationship ID
        let rel_id = self.next_rel_id;
        self.next_rel_id += 1;

        // Calculate node group IDs
        let src_group_id = (src_node_id / NODE_GROUP_SIZE as u64) as u32;
        let dst_group_id = (dst_node_id / NODE_GROUP_SIZE as u64) as u32;
        let src_local_id = (src_node_id % NODE_GROUP_SIZE as u64) as u32;
        let dst_local_id = (dst_node_id % NODE_GROUP_SIZE as u64) as u32;

        // Insert into forward index (src -> dst)
        self.ensure_forward_group(src_group_id, src_local_id + 1);
        if let Some(group) = self.forward_groups.get_mut(&src_group_id) {
            group.insert_edge(src_local_id, dst_node_id, rel_id)?;
        }

        // Insert into backward index (dst -> src)
        self.ensure_backward_group(dst_group_id, dst_local_id + 1);
        if let Some(group) = self.backward_groups.get_mut(&dst_group_id) {
            group.insert_edge(dst_local_id, src_node_id, rel_id)?;
        }

        // Store properties
        if !props.is_empty() {
            self.properties.insert(rel_id, props);
        }

        Ok(rel_id)
    }

    /// Ensures a forward group exists with at least `min_nodes` capacity.
    fn ensure_forward_group(&mut self, group_id: u32, min_nodes: u32) {
        self.forward_groups.entry(group_id).or_insert_with(|| {
            let mut group = CsrNodeGroup::new(group_id);
            // Pre-allocate empty node entries
            for _ in 0..min_nodes {
                group.offsets.push(0);
                group.num_nodes += 1;
            }
            group
        });

        // Extend if needed
        if let Some(group) = self.forward_groups.get_mut(&group_id) {
            while group.num_nodes < min_nodes {
                group.offsets.push(group.neighbors.len() as u64);
                group.num_nodes += 1;
            }
        }
    }

    /// Ensures a backward group exists with at least `min_nodes` capacity.
    fn ensure_backward_group(&mut self, group_id: u32, min_nodes: u32) {
        self.backward_groups.entry(group_id).or_insert_with(|| {
            let mut group = CsrNodeGroup::new(group_id);
            for _ in 0..min_nodes {
                group.offsets.push(0);
                group.num_nodes += 1;
            }
            group
        });

        if let Some(group) = self.backward_groups.get_mut(&group_id) {
            while group.num_nodes < min_nodes {
                group.offsets.push(group.neighbors.len() as u64);
                group.num_nodes += 1;
            }
        }
    }

    /// Gets all outgoing edges from a source node (forward traversal).
    ///
    /// Returns (`destination_node_id`, `rel_id`) pairs.
    #[must_use]
    pub fn get_forward_edges(&self, src_node_id: u64) -> Vec<(u64, u64)> {
        let group_id = (src_node_id / NODE_GROUP_SIZE as u64) as u32;
        let local_id = (src_node_id % NODE_GROUP_SIZE as u64) as u32;

        if let Some(group) = self.forward_groups.get(&group_id) {
            if (local_id as usize) < group.num_nodes as usize {
                let neighbors = group.get_neighbors(local_id);
                let rel_ids = group.get_rel_ids(local_id);
                return neighbors
                    .iter()
                    .copied()
                    .zip(rel_ids.iter().copied())
                    .collect();
            }
        }

        Vec::new()
    }

    /// Gets all incoming edges to a destination node (backward traversal).
    ///
    /// Returns (`source_node_id`, `rel_id`) pairs.
    #[must_use]
    pub fn get_backward_edges(&self, dst_node_id: u64) -> Vec<(u64, u64)> {
        let group_id = (dst_node_id / NODE_GROUP_SIZE as u64) as u32;
        let local_id = (dst_node_id % NODE_GROUP_SIZE as u64) as u32;

        if let Some(group) = self.backward_groups.get(&group_id) {
            if (local_id as usize) < group.num_nodes as usize {
                let neighbors = group.get_neighbors(local_id);
                let rel_ids = group.get_rel_ids(local_id);
                return neighbors
                    .iter()
                    .copied()
                    .zip(rel_ids.iter().copied())
                    .collect();
            }
        }

        Vec::new()
    }

    /// Gets properties for a relationship by ID.
    #[must_use]
    pub fn get_properties(&self, rel_id: u64) -> Option<&Vec<Value>> {
        self.properties.get(&rel_id)
    }

    /// Returns the total number of relationships.
    #[must_use]
    pub fn len(&self) -> usize {
        self.forward_groups
            .values()
            .map(CsrNodeGroup::num_edges)
            .sum()
    }

    /// Returns true if the table has no relationships.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.forward_groups.values().all(|g| g.num_edges() == 0)
    }

    /// Returns an iterator over all relationships.
    ///
    /// Yields (`src_node_id`, `dst_node_id`, `rel_id`) tuples.
    pub fn iter(&self) -> impl Iterator<Item = (u64, u64, u64)> + '_ {
        self.forward_groups.iter().flat_map(|(&group_id, group)| {
            let base_node_id = u64::from(group_id) * NODE_GROUP_SIZE as u64;
            (0..group.num_nodes).flat_map(move |local_id| {
                let src = base_node_id + u64::from(local_id);
                let neighbors = group.get_neighbors(local_id);
                let rel_ids = group.get_rel_ids(local_id);
                neighbors
                    .iter()
                    .copied()
                    .zip(rel_ids.iter().copied())
                    .map(move |(dst, rel_id)| (src, dst, rel_id))
            })
        })
    }

    /// Inserts multiple relationships in a single batch.
    ///
    /// This is more efficient than repeated single inserts for bulk operations:
    /// - Pre-validates all property counts upfront
    /// - Pre-allocates property storage capacity
    ///
    /// # Arguments
    ///
    /// * `relationships` - Vector of (`src_node_id`, `dst_node_id`, properties) tuples
    ///
    /// # Returns
    ///
    /// The number of relationships inserted.
    ///
    /// # Errors
    ///
    /// Returns an error if any relationship has the wrong number of properties.
    pub fn insert_batch(&mut self, relationships: Vec<(u64, u64, Vec<Value>)>) -> Result<usize> {
        if relationships.is_empty() {
            return Ok(0);
        }

        let expected_props = self.schema.columns.len();

        // Validate property counts first
        for (idx, (_, _, props)) in relationships.iter().enumerate() {
            if props.len() != expected_props {
                return Err(RuzuError::ExecutionError(format!(
                    "Relationship {} has {} properties, expected {}",
                    idx,
                    props.len(),
                    expected_props
                )));
            }
        }

        // Pre-allocate property storage capacity for throughput
        let count = relationships.len();
        self.properties.reserve(count);

        // Insert all relationships
        for (src_node_id, dst_node_id, props) in relationships {
            self.insert(src_node_id, dst_node_id, props)?;
        }

        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{ColumnDef, Direction};
    use crate::types::DataType;

    fn create_test_schema() -> Arc<RelTableSchema> {
        Arc::new(
            RelTableSchema::new(
                "KNOWS".to_string(),
                "Person".to_string(),
                "Person".to_string(),
                vec![ColumnDef::new("since".to_string(), DataType::Int64).unwrap()],
                Direction::Both,
            )
            .unwrap(),
        )
    }

    #[test]
    fn test_csr_node_group_creation() {
        let group = CsrNodeGroup::new(0);
        assert_eq!(group.group_id, 0);
        assert_eq!(group.num_nodes, 0);
        assert_eq!(group.offsets, vec![0]);
        assert!(group.neighbors.is_empty());
        assert!(group.rel_ids.is_empty());
    }

    #[test]
    fn test_csr_add_node_edges() {
        let mut group = CsrNodeGroup::new(0);

        // Node 0 -> [1, 3]
        group.add_node_edges(&[1, 3], &[0, 1]);
        // Node 1 -> [2]
        group.add_node_edges(&[2], &[2]);
        // Node 2 -> [0, 1, 3]
        group.add_node_edges(&[0, 1, 3], &[3, 4, 5]);

        assert_eq!(group.num_nodes, 3);
        assert_eq!(group.offsets, vec![0, 2, 3, 6]);
        assert_eq!(group.neighbors, vec![1, 3, 2, 0, 1, 3]);
        assert_eq!(group.rel_ids, vec![0, 1, 2, 3, 4, 5]);

        assert!(group.validate().is_ok());
    }

    #[test]
    fn test_csr_get_neighbors() {
        let mut group = CsrNodeGroup::new(0);
        group.add_node_edges(&[1, 3], &[0, 1]);
        group.add_node_edges(&[2], &[2]);
        group.add_node_edges(&[0, 1, 3], &[3, 4, 5]);

        assert_eq!(group.get_neighbors(0), &[1, 3]);
        assert_eq!(group.get_neighbors(1), &[2]);
        assert_eq!(group.get_neighbors(2), &[0, 1, 3]);

        assert_eq!(group.get_rel_ids(0), &[0, 1]);
        assert_eq!(group.get_rel_ids(1), &[2]);
        assert_eq!(group.get_rel_ids(2), &[3, 4, 5]);
    }

    #[test]
    fn test_csr_degree() {
        let mut group = CsrNodeGroup::new(0);
        group.add_node_edges(&[1, 3], &[0, 1]);
        group.add_node_edges(&[2], &[2]);
        group.add_node_edges(&[0, 1, 3], &[3, 4, 5]);

        assert_eq!(group.degree(0), 2);
        assert_eq!(group.degree(1), 1);
        assert_eq!(group.degree(2), 3);
    }

    #[test]
    fn test_csr_validation() {
        let mut group = CsrNodeGroup::new(0);
        group.add_node_edges(&[1], &[0]);
        assert!(group.validate().is_ok());

        // Break invariant: wrong rel_ids length
        group.rel_ids.pop();
        assert!(group.validate().is_err());
    }

    #[test]
    fn test_csr_serialization() {
        let mut group = CsrNodeGroup::new(0);
        group.add_node_edges(&[1, 3], &[0, 1]);
        group.add_node_edges(&[2], &[2]);

        let bytes = group.serialize().unwrap();
        let restored = CsrNodeGroup::deserialize(&bytes).unwrap();

        assert_eq!(group.group_id, restored.group_id);
        assert_eq!(group.num_nodes, restored.num_nodes);
        assert_eq!(group.offsets, restored.offsets);
        assert_eq!(group.neighbors, restored.neighbors);
        assert_eq!(group.rel_ids, restored.rel_ids);
    }

    #[test]
    fn test_rel_table_insert_and_query() {
        let schema = create_test_schema();
        let mut table = RelTable::new(schema);

        // Insert relationship: node 0 -> node 1 with property since=2020
        let rel_id = table.insert(0, 1, vec![Value::Int64(2020)]).unwrap();
        assert_eq!(rel_id, 0);

        // Query forward (src -> dst)
        let forward = table.get_forward_edges(0);
        assert_eq!(forward.len(), 1);
        assert_eq!(forward[0], (1, 0)); // (dst, rel_id)

        // Query backward (dst -> src)
        let backward = table.get_backward_edges(1);
        assert_eq!(backward.len(), 1);
        assert_eq!(backward[0], (0, 0)); // (src, rel_id)

        // Query properties
        let props = table.get_properties(0).unwrap();
        assert_eq!(props, &vec![Value::Int64(2020)]);
    }

    #[test]
    fn test_rel_table_multiple_edges() {
        let schema = create_test_schema();
        let mut table = RelTable::new(schema);

        // Node 0 knows nodes 1, 2, 3
        table.insert(0, 1, vec![Value::Int64(2018)]).unwrap();
        table.insert(0, 2, vec![Value::Int64(2019)]).unwrap();
        table.insert(0, 3, vec![Value::Int64(2020)]).unwrap();

        let forward = table.get_forward_edges(0);
        assert_eq!(forward.len(), 3);

        // Verify all destinations are present
        let dsts: Vec<u64> = forward.iter().map(|(d, _)| *d).collect();
        assert!(dsts.contains(&1));
        assert!(dsts.contains(&2));
        assert!(dsts.contains(&3));
    }

    #[test]
    fn test_rel_table_iter() {
        let schema = create_test_schema();
        let mut table = RelTable::new(schema);

        table.insert(0, 1, vec![Value::Int64(2020)]).unwrap();
        table.insert(1, 2, vec![Value::Int64(2021)]).unwrap();

        let edges: Vec<_> = table.iter().collect();
        assert_eq!(edges.len(), 2);
    }

    #[test]
    fn test_rel_table_to_from_data() {
        let schema = create_test_schema();
        let mut table = RelTable::new(Arc::clone(&schema));

        table.insert(0, 1, vec![Value::Int64(2020)]).unwrap();
        table.insert(1, 2, vec![Value::Int64(2021)]).unwrap();

        let data = table.to_data();
        let restored = RelTable::from_data(schema, data);

        assert_eq!(restored.len(), 2);
        assert_eq!(restored.get_forward_edges(0).len(), 1);
        assert_eq!(restored.get_forward_edges(1).len(), 1);
    }
}
