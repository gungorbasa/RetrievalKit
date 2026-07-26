use std::collections::{BTreeMap, BTreeSet};

use retrievalkit_core::{ChunkId, ChunkIdentity, RecordId};
use serde::{Deserialize, Serialize};

use crate::schema::{FieldPath, NodeType, RelationshipType};

pub(crate) type NodeOrdinal = u32;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum NodeSource {
    Record(RecordId),
    Chunk(ChunkIdentity),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct NodeId {
    pub node_type: NodeType,
    pub source: NodeSource,
}

impl NodeId {
    pub fn record(node_type: NodeType, record_id: RecordId) -> Self {
        Self {
            node_type,
            source: NodeSource::Record(record_id),
        }
    }

    pub fn chunk(node_type: NodeType, identity: ChunkIdentity) -> Self {
        Self {
            node_type,
            source: NodeSource::Chunk(identity),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct EdgeId {
    pub relationship_type: RelationshipType,
    pub source: NodeId,
    pub target: NodeId,
    pub occurrence_ordinal: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EdgeProvenance {
    pub schema_rule_index: u32,
    pub source_record_id: RecordId,
    pub source_field: Option<FieldPath>,
    pub derived_inverse: bool,
    pub built_in: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphPathEdge {
    pub edge_id: EdgeId,
    pub provenance: EdgeProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum GraphScalar {
    String(String),
    I64(i64),
    Bool(bool),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    Outgoing,
    Incoming,
}

#[derive(Debug, Clone)]
pub(crate) struct StoredEdge {
    pub id: EdgeId,
    pub provenance: EdgeProvenance,
    pub source: NodeOrdinal,
    pub target: NodeOrdinal,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct AdjacencyEntry {
    pub other: NodeOrdinal,
    pub edge_index: u32,
}

#[derive(Debug, Clone)]
pub(crate) struct CsrAdjacency {
    offsets: Vec<usize>,
    entries: Vec<AdjacencyEntry>,
}

impl CsrAdjacency {
    pub fn build(
        node_count: usize,
        entries: impl IntoIterator<Item = (NodeOrdinal, AdjacencyEntry)>,
    ) -> Self {
        let mut grouped = vec![Vec::<AdjacencyEntry>::new(); node_count];
        for (node, entry) in entries {
            if let Some(group) = grouped.get_mut(node as usize) {
                group.push(entry);
            }
        }
        let mut offsets = Vec::with_capacity(node_count + 1);
        let mut flat = Vec::new();
        offsets.push(0);
        for group in grouped {
            flat.extend(group);
            offsets.push(flat.len());
        }
        Self {
            offsets,
            entries: flat,
        }
    }

    pub fn neighbors(&self, node: NodeOrdinal) -> &[AdjacencyEntry] {
        let index = node as usize;
        let Some(&start) = self.offsets.get(index) else {
            return &[];
        };
        let Some(&end) = self.offsets.get(index + 1) else {
            return &[];
        };
        &self.entries[start..end]
    }
}

#[derive(Debug, Clone)]
pub(crate) struct GraphStorage {
    pub nodes: Vec<NodeId>,
    pub node_ordinals: BTreeMap<NodeId, NodeOrdinal>,
    pub edges: Vec<StoredEdge>,
    pub forward: CsrAdjacency,
    pub reverse: CsrAdjacency,
    pub properties: BTreeMap<(NodeType, FieldPath, GraphScalar), Vec<NodeOrdinal>>,
    pub queryable_fields: BTreeSet<(NodeType, FieldPath)>,
    pub chunk_projections: BTreeMap<NodeId, Vec<ChunkId>>,
    pub diagnostics: Vec<String>,
}

impl GraphStorage {
    pub fn node_ordinal(&self, node_id: &NodeId) -> Option<NodeOrdinal> {
        self.node_ordinals.get(node_id).copied()
    }

    pub fn neighbors<'a>(
        &'a self,
        node: NodeOrdinal,
        direction: Direction,
        relationship: &'a RelationshipType,
    ) -> impl Iterator<Item = (NodeOrdinal, u32, &'a StoredEdge)> + 'a {
        let entries = match direction {
            Direction::Outgoing => self.forward.neighbors(node),
            Direction::Incoming => self.reverse.neighbors(node),
        };
        entries.iter().filter_map(|entry| {
            let edge = self.edges.get(entry.edge_index as usize)?;
            (edge.id.relationship_type == *relationship).then_some((
                entry.other,
                entry.edge_index,
                edge,
            ))
        })
    }
}
