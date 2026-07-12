use std::collections::{BTreeMap, BTreeSet};

use vectorkit_core::{
    ChunkId, ChunkIdentity, ChunkKey, CorpusId, ExactVectorIndex, FieldName, GenerationId, RecordId,
};

use crate::builder::GraphBuildStats;
use crate::error::{GraphError, Result};
use crate::schema::{FieldPath, GraphSchema, NodeType, RelationshipType, SchemaHash};
use crate::storage::{
    AdjacencyEntry, CsrAdjacency, EdgeId, EdgeProvenance, GraphScalar, GraphStorage, NodeId,
    NodeOrdinal, NodeSource, StoredEdge,
};

const GRAPH_MAGIC: &[u8; 4] = b"VKGS";
const GRAPH_FORMAT_VERSION: u32 = 1;
const MAX_PERSISTED_ITEMS: usize = 10_000_000;
const MAX_STRING_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphSnapshotPayload {
    pub schema_bytes: Vec<u8>,
    pub graph_bytes: Vec<u8>,
    pub schema_hash: SchemaHash,
}

pub(crate) fn encode_snapshot(
    storage: &GraphStorage,
    schema: &GraphSchema,
    corpus_id: &CorpusId,
    generation: GenerationId,
) -> Result<GraphSnapshotPayload> {
    let schema_bytes = schema.canonical_bytes()?;
    let schema_hash = schema.schema_hash()?;
    let graph_bytes = encode_graph(storage, corpus_id, generation, schema_hash)?;
    Ok(GraphSnapshotPayload {
        schema_bytes,
        graph_bytes,
        schema_hash,
    })
}

pub(crate) fn decode_snapshot(
    core: &ExactVectorIndex,
    payload: &GraphSnapshotPayload,
) -> Result<(GraphSchema, GraphStorage, GraphBuildStats)> {
    let schema = GraphSchema::from_canonical_bytes(&payload.schema_bytes)?;
    let schema_hash = schema.schema_hash()?;
    if schema_hash != payload.schema_hash {
        return Err(invalid_snapshot(
            "schema bytes do not match the declared BLAKE3 hash",
        ));
    }
    let decoded = decode_graph(&payload.graph_bytes)?;
    if decoded.schema_hash != schema_hash {
        return Err(invalid_snapshot(
            "graph payload schema hash does not match schema bytes",
        ));
    }
    if decoded.corpus_id != *core.corpus_id() || decoded.generation != core.generation() {
        return Err(invalid_snapshot(format!(
            "graph payload belongs to corpus '{}' generation {}, active core is '{}' generation {}",
            decoded.corpus_id.as_str(),
            decoded.generation.get(),
            core.corpus_id().as_str(),
            core.generation().get()
        )));
    }
    validate_storage(core, &schema, &decoded.storage)?;
    let stats = GraphBuildStats {
        records: core.record_store().len(),
        nodes: decoded.storage.nodes.len(),
        edges: decoded.storage.edges.len(),
        diagnostics: decoded.storage.diagnostics.len(),
    };
    Ok((schema, decoded.storage, stats))
}

struct DecodedGraph {
    corpus_id: CorpusId,
    generation: GenerationId,
    schema_hash: SchemaHash,
    storage: GraphStorage,
}

fn encode_graph(
    storage: &GraphStorage,
    corpus_id: &CorpusId,
    generation: GenerationId,
    schema_hash: SchemaHash,
) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(GRAPH_MAGIC);
    write_u32(&mut bytes, GRAPH_FORMAT_VERSION);
    write_string(&mut bytes, corpus_id.as_str(), "corpus ID")?;
    write_u64(&mut bytes, generation.get());
    bytes.extend_from_slice(schema_hash.as_bytes());

    write_len(&mut bytes, storage.nodes.len(), "node count")?;
    for node in &storage.nodes {
        write_node(&mut bytes, node)?;
    }

    write_len(&mut bytes, storage.edges.len(), "edge count")?;
    for edge in &storage.edges {
        write_string(
            &mut bytes,
            edge.id.relationship_type.as_str(),
            "relationship type",
        )?;
        write_u32(&mut bytes, edge.source);
        write_u32(&mut bytes, edge.target);
        write_u32(&mut bytes, edge.id.occurrence_ordinal);
        write_provenance(&mut bytes, &edge.provenance)?;
    }

    write_len(
        &mut bytes,
        storage.queryable_fields.len(),
        "queryable field count",
    )?;
    for (node_type, field) in &storage.queryable_fields {
        write_string(&mut bytes, node_type.as_str(), "node type")?;
        write_field_path(&mut bytes, field)?;
    }

    write_len(&mut bytes, storage.properties.len(), "property count")?;
    for ((node_type, field, scalar), ordinals) in &storage.properties {
        write_string(&mut bytes, node_type.as_str(), "node type")?;
        write_field_path(&mut bytes, field)?;
        write_scalar(&mut bytes, scalar)?;
        write_len(&mut bytes, ordinals.len(), "property ordinal count")?;
        for ordinal in ordinals {
            write_u32(&mut bytes, *ordinal);
        }
    }

    write_len(
        &mut bytes,
        storage.chunk_projections.len(),
        "projection count",
    )?;
    for (node, chunk_ids) in &storage.chunk_projections {
        let ordinal = storage.node_ordinals.get(node).ok_or_else(|| {
            invalid_snapshot("projection references a node without an internal ordinal")
        })?;
        write_u32(&mut bytes, *ordinal);
        write_len(&mut bytes, chunk_ids.len(), "projected chunk count")?;
        for chunk_id in chunk_ids {
            write_u64(&mut bytes, *chunk_id);
        }
    }

    write_len(&mut bytes, storage.diagnostics.len(), "diagnostic count")?;
    for diagnostic in &storage.diagnostics {
        write_string(&mut bytes, diagnostic, "diagnostic")?;
    }
    Ok(bytes)
}

fn decode_graph(bytes: &[u8]) -> Result<DecodedGraph> {
    let mut reader = Reader::new(bytes);
    if reader.read_exact(4, "graph magic")? != GRAPH_MAGIC {
        return Err(invalid_snapshot("graph payload has invalid magic"));
    }
    let version = reader.read_u32("graph format version")?;
    if version != GRAPH_FORMAT_VERSION {
        return Err(GraphError::IncompatibleVersion {
            message: format!("unsupported graph format version {version}"),
        });
    }
    let corpus_id = CorpusId::new(reader.read_string("corpus ID", 128)?)
        .map_err(|error| invalid_snapshot(error.to_string()))?;
    let generation = GenerationId::new(reader.read_u64("generation")?);
    let schema_hash = SchemaHash::from_bytes(
        reader
            .read_exact(32, "schema hash")?
            .try_into()
            .expect("schema hash length is fixed"),
    );

    let node_count = reader.read_count("node count")?;
    let mut nodes = Vec::with_capacity(node_count);
    for _ in 0..node_count {
        nodes.push(reader.read_node()?);
    }
    if !strictly_sorted(&nodes) {
        return Err(invalid_snapshot("nodes must be sorted and unique"));
    }
    let node_ordinals = nodes
        .iter()
        .cloned()
        .enumerate()
        .map(|(ordinal, node)| (node, ordinal as NodeOrdinal))
        .collect::<BTreeMap<_, _>>();

    let edge_count = reader.read_count("edge count")?;
    let mut edges = Vec::with_capacity(edge_count);
    for _ in 0..edge_count {
        let relationship_type =
            RelationshipType::new(reader.read_string("relationship type", MAX_STRING_BYTES)?)
                .map_err(|error| invalid_snapshot(error.to_string()))?;
        let source = reader.read_u32("edge source")?;
        let target = reader.read_u32("edge target")?;
        let source_node = nodes
            .get(source as usize)
            .ok_or_else(|| invalid_snapshot("edge source ordinal is out of bounds"))?
            .clone();
        let target_node = nodes
            .get(target as usize)
            .ok_or_else(|| invalid_snapshot("edge target ordinal is out of bounds"))?
            .clone();
        let occurrence_ordinal = reader.read_u32("edge occurrence ordinal")?;
        let provenance = reader.read_provenance()?;
        edges.push(StoredEdge {
            id: EdgeId {
                relationship_type,
                source: source_node,
                target: target_node,
                occurrence_ordinal,
            },
            provenance,
            source,
            target,
        });
    }
    if !edges.windows(2).all(|pair| pair[0].id < pair[1].id) {
        return Err(invalid_snapshot("edges must be sorted and unique"));
    }

    let queryable_count = reader.read_count("queryable field count")?;
    let mut queryable_fields = BTreeSet::new();
    for _ in 0..queryable_count {
        let node_type = reader.read_node_type()?;
        let field = reader.read_field_path()?;
        if !queryable_fields.insert((node_type, field)) {
            return Err(invalid_snapshot("queryable field entry is duplicated"));
        }
    }

    let property_count = reader.read_count("property count")?;
    let mut properties = BTreeMap::new();
    for _ in 0..property_count {
        let node_type = reader.read_node_type()?;
        let field = reader.read_field_path()?;
        let scalar = reader.read_scalar()?;
        let ordinal_count = reader.read_count("property ordinal count")?;
        let mut ordinals = Vec::with_capacity(ordinal_count);
        for _ in 0..ordinal_count {
            let ordinal = reader.read_u32("property node ordinal")?;
            if ordinal as usize >= nodes.len() {
                return Err(invalid_snapshot("property node ordinal is out of bounds"));
            }
            ordinals.push(ordinal);
        }
        if !strictly_sorted(&ordinals) {
            return Err(invalid_snapshot(
                "property ordinals must be sorted and unique",
            ));
        }
        if properties
            .insert((node_type, field, scalar), ordinals)
            .is_some()
        {
            return Err(invalid_snapshot("property entry is duplicated"));
        }
    }

    let projection_count = reader.read_count("projection count")?;
    let mut chunk_projections = BTreeMap::new();
    for _ in 0..projection_count {
        let node_ordinal = reader.read_u32("projection node ordinal")?;
        let node = nodes
            .get(node_ordinal as usize)
            .ok_or_else(|| invalid_snapshot("projection node ordinal is out of bounds"))?
            .clone();
        let chunk_count = reader.read_count("projected chunk count")?;
        let mut chunk_ids = Vec::with_capacity(chunk_count);
        for _ in 0..chunk_count {
            chunk_ids.push(reader.read_u64("projected chunk ID")?);
        }
        if !strictly_sorted(&chunk_ids) {
            return Err(invalid_snapshot(
                "projected chunk IDs must be sorted and unique",
            ));
        }
        if chunk_projections.insert(node, chunk_ids).is_some() {
            return Err(invalid_snapshot("projection entry is duplicated"));
        }
    }

    let diagnostic_count = reader.read_count("diagnostic count")?;
    let mut diagnostics = Vec::with_capacity(diagnostic_count);
    for _ in 0..diagnostic_count {
        diagnostics.push(reader.read_string("diagnostic", MAX_STRING_BYTES)?);
    }
    reader.finish()?;

    let forward = CsrAdjacency::build(
        nodes.len(),
        edges.iter().enumerate().map(|(edge_index, edge)| {
            (
                edge.source,
                AdjacencyEntry {
                    other: edge.target,
                    edge_index: edge_index as u32,
                },
            )
        }),
    );
    let reverse = CsrAdjacency::build(
        nodes.len(),
        edges.iter().enumerate().map(|(edge_index, edge)| {
            (
                edge.target,
                AdjacencyEntry {
                    other: edge.source,
                    edge_index: edge_index as u32,
                },
            )
        }),
    );
    Ok(DecodedGraph {
        corpus_id,
        generation,
        schema_hash,
        storage: GraphStorage {
            nodes,
            node_ordinals,
            edges,
            forward,
            reverse,
            properties,
            queryable_fields,
            chunk_projections,
            diagnostics,
        },
    })
}

fn validate_storage(
    core: &ExactVectorIndex,
    schema: &GraphSchema,
    storage: &GraphStorage,
) -> Result<()> {
    let validated = schema.validate_internal()?;
    let expected_queryable = schema
        .record_nodes
        .iter()
        .flat_map(|mapping| {
            mapping
                .queryable_fields
                .iter()
                .cloned()
                .map(|field| (mapping.node_type.clone(), field))
        })
        .collect::<BTreeSet<_>>();
    if storage.queryable_fields != expected_queryable {
        return Err(invalid_snapshot(
            "persisted queryable fields do not match the canonical schema",
        ));
    }

    for node in &storage.nodes {
        match &node.source {
            NodeSource::Record(record_id) => {
                let record = core
                    .record_store()
                    .get(record_id)
                    .ok_or_else(|| invalid_snapshot("record node source is unavailable"))?;
                let mapping = validated
                    .by_record_type
                    .get(&record.record_type)
                    .ok_or_else(|| {
                        invalid_snapshot("record node type has no canonical schema mapping")
                    })?;
                if node.node_type != mapping.node_type {
                    return Err(invalid_snapshot(
                        "record node type does not match its canonical schema mapping",
                    ));
                }
            }
            NodeSource::Chunk(identity) => {
                let chunk_schema = schema.chunk_nodes.as_ref().ok_or_else(|| {
                    invalid_snapshot("chunk node exists but schema has no chunk-node mapping")
                })?;
                if node.node_type != chunk_schema.node_type
                    || core.chunk_id_for_identity(identity).is_none()
                {
                    return Err(invalid_snapshot(
                        "chunk node does not resolve in the active core generation",
                    ));
                }
            }
        }
    }

    let allowed_relationships = schema
        .relationships
        .iter()
        .flat_map(|relationship| {
            std::iter::once(&relationship.relationship_type)
                .chain(relationship.inverse_relationship.iter())
        })
        .chain(schema.chunk_nodes.iter().flat_map(|chunk| {
            std::iter::once(&chunk.owns_relationship).chain(chunk.inverse_relationship.iter())
        }))
        .collect::<BTreeSet<_>>();
    if storage
        .edges
        .iter()
        .any(|edge| !allowed_relationships.contains(&edge.id.relationship_type))
    {
        return Err(invalid_snapshot(
            "persisted edge relationship is not declared by the canonical schema",
        ));
    }

    let mut projected_ids = BTreeSet::<ChunkId>::new();
    for (node, chunk_ids) in &storage.chunk_projections {
        if !storage.node_ordinals.contains_key(node) {
            return Err(invalid_snapshot(
                "projection references an unavailable node",
            ));
        }
        projected_ids.extend(chunk_ids.iter().copied());
    }
    core.candidate_scope(projected_ids)?;
    Ok(())
}

fn write_node(bytes: &mut Vec<u8>, node: &NodeId) -> Result<()> {
    write_string(bytes, node.node_type.as_str(), "node type")?;
    match &node.source {
        NodeSource::Record(record_id) => {
            write_u8(bytes, 0);
            write_string(bytes, record_id.as_str(), "record ID")?;
        }
        NodeSource::Chunk(identity) => {
            write_u8(bytes, 1);
            write_string(bytes, identity.record_id.as_str(), "record ID")?;
            write_string(bytes, identity.chunk_key.as_str(), "chunk key")?;
        }
    }
    Ok(())
}

fn write_provenance(bytes: &mut Vec<u8>, provenance: &EdgeProvenance) -> Result<()> {
    write_u32(bytes, provenance.schema_rule_index);
    write_string(
        bytes,
        provenance.source_record_id.as_str(),
        "provenance record ID",
    )?;
    match &provenance.source_field {
        Some(field) => {
            write_u8(bytes, 1);
            write_field_path(bytes, field)?;
        }
        None => write_u8(bytes, 0),
    }
    write_bool(bytes, provenance.derived_inverse);
    write_bool(bytes, provenance.built_in);
    Ok(())
}

fn write_field_path(bytes: &mut Vec<u8>, path: &FieldPath) -> Result<()> {
    write_len(bytes, path.segments().len(), "field path segment count")?;
    for segment in path.segments() {
        write_string(bytes, segment.as_str(), "field path segment")?;
    }
    Ok(())
}

fn write_scalar(bytes: &mut Vec<u8>, scalar: &GraphScalar) -> Result<()> {
    match scalar {
        GraphScalar::String(value) => {
            write_u8(bytes, 0);
            write_string(bytes, value, "property string")?;
        }
        GraphScalar::I64(value) => {
            write_u8(bytes, 1);
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        GraphScalar::Bool(value) => {
            write_u8(bytes, 2);
            write_bool(bytes, *value);
        }
    }
    Ok(())
}

fn write_string(bytes: &mut Vec<u8>, value: &str, label: &str) -> Result<()> {
    write_len(bytes, value.len(), label)?;
    bytes.extend_from_slice(value.as_bytes());
    Ok(())
}

fn write_len(bytes: &mut Vec<u8>, value: usize, label: &str) -> Result<()> {
    let value =
        u32::try_from(value).map_err(|_| invalid_snapshot(format!("{label} exceeds u32")))?;
    write_u32(bytes, value);
    Ok(())
}

fn write_bool(bytes: &mut Vec<u8>, value: bool) {
    write_u8(bytes, u8::from(value));
}

fn write_u8(bytes: &mut Vec<u8>, value: u8) {
    bytes.push(value);
}

fn write_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn write_u64(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn read_node(&mut self) -> Result<NodeId> {
        let node_type = self.read_node_type()?;
        match self.read_u8("node source tag")? {
            0 => Ok(NodeId::record(node_type, self.read_record_id()?)),
            1 => {
                let record_id = self.read_record_id()?;
                let chunk_key = ChunkKey::new(self.read_string("chunk key", 512)?)
                    .map_err(|error| invalid_snapshot(error.to_string()))?;
                Ok(NodeId::chunk(
                    node_type,
                    ChunkIdentity::new(record_id, chunk_key),
                ))
            }
            tag => Err(invalid_snapshot(format!("unknown node source tag {tag}"))),
        }
    }

    fn read_node_type(&mut self) -> Result<NodeType> {
        NodeType::new(self.read_string("node type", 64)?)
            .map_err(|error| invalid_snapshot(error.to_string()))
    }

    fn read_record_id(&mut self) -> Result<RecordId> {
        RecordId::new(self.read_string("record ID", 512)?)
            .map_err(|error| invalid_snapshot(error.to_string()))
    }

    fn read_provenance(&mut self) -> Result<EdgeProvenance> {
        let schema_rule_index = self.read_u32("schema rule index")?;
        let source_record_id = self.read_record_id()?;
        let source_field = match self.read_u8("source field presence")? {
            0 => None,
            1 => Some(self.read_field_path()?),
            value => {
                return Err(invalid_snapshot(format!(
                    "invalid source field presence byte {value}"
                )));
            }
        };
        Ok(EdgeProvenance {
            schema_rule_index,
            source_record_id,
            source_field,
            derived_inverse: self.read_bool("derived inverse")?,
            built_in: self.read_bool("built-in provenance")?,
        })
    }

    fn read_field_path(&mut self) -> Result<FieldPath> {
        let count = self.read_count("field path segment count")?;
        let mut segments = Vec::with_capacity(count);
        for _ in 0..count {
            let segment = FieldName::new(self.read_string("field path segment", 64)?)
                .map_err(|error| invalid_snapshot(error.to_string()))?;
            segments.push(segment);
        }
        FieldPath::new(segments).map_err(|error| invalid_snapshot(error.to_string()))
    }

    fn read_scalar(&mut self) -> Result<GraphScalar> {
        match self.read_u8("property scalar tag")? {
            0 => Ok(GraphScalar::String(
                self.read_string("property string", MAX_STRING_BYTES)?,
            )),
            1 => {
                let bytes: [u8; 8] = self
                    .read_exact(8, "property i64")?
                    .try_into()
                    .expect("i64 byte length is fixed");
                Ok(GraphScalar::I64(i64::from_le_bytes(bytes)))
            }
            2 => Ok(GraphScalar::Bool(self.read_bool("property bool")?)),
            tag => Err(invalid_snapshot(format!(
                "unknown property scalar tag {tag}"
            ))),
        }
    }

    fn read_string(&mut self, label: &str, maximum: usize) -> Result<String> {
        let length = self.read_count(label)?;
        if length > maximum {
            return Err(invalid_snapshot(format!(
                "{label} length {length} exceeds {maximum} bytes"
            )));
        }
        let bytes = self.read_exact(length, label)?;
        String::from_utf8(bytes.to_vec())
            .map_err(|error| invalid_snapshot(format!("{label} is not UTF-8: {error}")))
    }

    fn read_count(&mut self, label: &str) -> Result<usize> {
        let count = self.read_u32(label)? as usize;
        if count > MAX_PERSISTED_ITEMS {
            return Err(invalid_snapshot(format!(
                "{label} {count} exceeds safety cap {MAX_PERSISTED_ITEMS}"
            )));
        }
        Ok(count)
    }

    fn read_bool(&mut self, label: &str) -> Result<bool> {
        match self.read_u8(label)? {
            0 => Ok(false),
            1 => Ok(true),
            value => Err(invalid_snapshot(format!(
                "{label} byte {value} is not 0 or 1"
            ))),
        }
    }

    fn read_u8(&mut self, label: &str) -> Result<u8> {
        Ok(self.read_exact(1, label)?[0])
    }

    fn read_u32(&mut self, label: &str) -> Result<u32> {
        let bytes: [u8; 4] = self
            .read_exact(4, label)?
            .try_into()
            .expect("u32 byte length is fixed");
        Ok(u32::from_le_bytes(bytes))
    }

    fn read_u64(&mut self, label: &str) -> Result<u64> {
        let bytes: [u8; 8] = self
            .read_exact(8, label)?
            .try_into()
            .expect("u64 byte length is fixed");
        Ok(u64::from_le_bytes(bytes))
    }

    fn read_exact(&mut self, length: usize, label: &str) -> Result<&'a [u8]> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or_else(|| invalid_snapshot(format!("{label} length overflow")))?;
        let bytes = self.bytes.get(self.offset..end).ok_or_else(|| {
            invalid_snapshot(format!(
                "graph payload ended while reading {label} at byte {}",
                self.offset
            ))
        })?;
        self.offset = end;
        Ok(bytes)
    }

    fn finish(&self) -> Result<()> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            Err(invalid_snapshot(format!(
                "graph payload has {} trailing bytes",
                self.bytes.len() - self.offset
            )))
        }
    }
}

fn strictly_sorted<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

fn invalid_snapshot(message: impl Into<String>) -> GraphError {
    GraphError::InvalidSnapshot {
        message: message.into(),
    }
}
