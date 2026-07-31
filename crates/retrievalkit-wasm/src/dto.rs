use std::collections::BTreeMap;

use retrievalkit_core::{
    ChunkIdentity, ChunkKey, FieldName, Filter, HybridHit, KeywordHit, Metadata, MetadataValue,
    Record, RecordChunkInput, RecordId, RecordType, RecordValue, SearchHit, StoredChunk,
};
use retrievalkit_graph::{
    Cardinality, ChunkNodeSchema, Direction, DuplicateReferencePolicy, FieldPath, GraphMatch,
    GraphQuery, GraphResult, GraphScalar, GraphSchema, MissingTargetPolicy, NodeId, NodeSource,
    NodeType, QueryLimits, RecordNodeSchema, RelationshipSchema, RelationshipType, Seed, Traverse,
    TruncationReason,
};
use serde::{Deserialize, Serialize};

use crate::error::{BoundaryError, Result};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RecordInputDto {
    pub id: String,
    pub record_type: String,
    #[serde(default)]
    pub fields: Vec<RecordFieldDto>,
    pub content: Option<String>,
    #[serde(default)]
    pub metadata: Vec<MetadataEntryDto>,
    #[serde(default)]
    pub chunks: Vec<ChunkInputDto>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChunkInputDto {
    pub key: String,
    pub text: String,
    #[serde(default)]
    pub metadata: Vec<MetadataEntryDto>,
    pub embedding_index: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RecordFieldDto {
    pub field: String,
    pub value: RecordValueDto,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum RecordValueDto {
    Null,
    Boolean { value: bool },
    Integer { value: String },
    Float { value: f64 },
    String { value: String },
    List { value: Vec<RecordValueDto> },
    Map { value: Vec<RecordFieldDto> },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct MetadataEntryDto {
    pub field: String,
    pub value: MetadataValueDto,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "camelCase")]
pub(crate) enum MetadataValueDto {
    String(String),
    Integer(String),
    Float(f64),
    Boolean(bool),
    Timestamp(String),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum FilterDto {
    Equals {
        field: String,
        value: MetadataValueDto,
    },
    NotEquals {
        field: String,
        value: MetadataValueDto,
    },
    In {
        field: String,
        values: Vec<MetadataValueDto>,
    },
    Range {
        field: String,
        lower: Option<MetadataValueDto>,
        upper: Option<MetadataValueDto>,
    },
    Exists {
        field: String,
    },
    All {
        children: Vec<FilterDto>,
    },
    Any {
        children: Vec<FilterDto>,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SearchOptionsDto {
    pub top_k: usize,
    pub filter: Option<FilterDto>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HybridOptionsDto {
    pub text: String,
    pub top_k: usize,
    pub alpha: f32,
    pub vector_candidates: Option<usize>,
    pub keyword_candidates: Option<usize>,
    pub filter: Option<FilterDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SearchHitDto {
    pub chunk_id: String,
    pub document_id: String,
    pub text: String,
    pub metadata: Vec<MetadataEntryDto>,
    pub score: f32,
    pub vector_score: f32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HybridHitDto {
    pub chunk_id: String,
    pub document_id: String,
    pub text: String,
    pub metadata: Vec<MetadataEntryDto>,
    pub score: f32,
    pub vector_score: Option<f32>,
    pub keyword_score: Option<f32>,
    pub vector_rank: Option<usize>,
    pub keyword_rank: Option<usize>,
    pub normalized_vector_score: Option<f32>,
    pub normalized_keyword_score: Option<f32>,
    pub matched_terms: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct KeywordHitDto {
    pub chunk_id: String,
    pub document_id: String,
    pub text: String,
    pub metadata: Vec<MetadataEntryDto>,
    pub score: f32,
    pub matched_terms: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GraphSchemaDto {
    pub record_nodes: Vec<RecordNodeSchemaDto>,
    #[serde(default)]
    pub relationships: Vec<RelationshipSchemaDto>,
    pub chunk_nodes: Option<ChunkNodeSchemaDto>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RecordNodeSchemaDto {
    pub record_type: String,
    pub node_type: String,
    #[serde(default)]
    pub queryable_fields: Vec<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RelationshipSchemaDto {
    pub relationship_type: String,
    pub source_node_type: String,
    pub target_node_type: String,
    pub source_field: Vec<String>,
    pub cardinality: CardinalityDto,
    pub missing_target: MissingTargetDto,
    pub duplicate_references: DuplicateReferencesDto,
    #[serde(default)]
    pub allow_self_edge: bool,
    pub inverse_relationship: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum CardinalityDto {
    One,
    OptionalOne,
    Many,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum MissingTargetDto {
    Error,
    OmitEdge,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum DuplicateReferencesDto {
    Error,
    Deduplicate,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChunkNodeSchemaDto {
    pub node_type: String,
    pub owns_relationship: String,
    pub inverse_relationship: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GraphQueryDto {
    pub seed: GraphSeedDto,
    #[serde(default)]
    pub steps: Vec<TraverseDto>,
    pub limits: Option<QueryLimitsDto>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub(crate) enum GraphSeedDto {
    NodeIds {
        nodes: Vec<NodeIdDto>,
    },
    Equals {
        node_type: String,
        field: Vec<String>,
        values: Vec<GraphScalarDto>,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NodeIdDto {
    pub node_type: String,
    pub source_kind: NodeSourceKindDto,
    pub record_id: String,
    pub chunk_key: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum NodeSourceKindDto {
    Record,
    Chunk,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "camelCase")]
pub(crate) enum GraphScalarDto {
    String(String),
    Integer(String),
    Boolean(bool),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TraverseDto {
    pub relationship: String,
    pub direction: DirectionDto,
    pub min_hops: usize,
    pub max_hops: usize,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum DirectionDto {
    Outgoing,
    Incoming,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct QueryLimitsDto {
    pub max_hops: usize,
    pub max_visited: usize,
    pub max_results: usize,
    pub max_working_bytes: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GraphResultDto {
    pub selection_id: u32,
    pub corpus_id: String,
    pub generation: String,
    pub matches: Vec<GraphMatchDto>,
    pub truncated: Option<String>,
    pub trace: GraphTraceDto,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GraphMatchDto {
    pub node_type: String,
    pub source_kind: &'static str,
    pub record_id: String,
    pub chunk_key: Option<String>,
    pub depth: usize,
    pub path: Vec<GraphPathEdgeDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GraphPathEdgeDto {
    pub relationship: String,
    pub source: GraphNodeDto,
    pub target: GraphNodeDto,
    pub occurrence_ordinal: u32,
    pub schema_rule_index: u32,
    pub source_record_id: String,
    pub source_field: Option<Vec<String>>,
    pub derived_inverse: bool,
    pub built_in: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GraphNodeDto {
    pub node_type: String,
    pub source_kind: &'static str,
    pub record_id: String,
    pub chunk_key: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GraphTraceDto {
    pub seed_count: usize,
    pub visited_states: usize,
    pub traversed_edges: usize,
    pub result_count: usize,
    pub diagnostics: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CandidateProjectionDto {
    pub candidates: Vec<ChunkIdentityDto>,
    pub source_nodes: usize,
    pub projected_chunks_before_filter: usize,
    pub projected_chunks_after_filter: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChunkIdentityDto {
    pub record_id: String,
    pub chunk_key: String,
}

pub(crate) fn candidate_projection(
    projection: retrievalkit_graph::GraphCandidateProjection,
) -> CandidateProjectionDto {
    CandidateProjectionDto {
        candidates: projection
            .candidates
            .into_iter()
            .map(|identity| ChunkIdentityDto {
                record_id: identity.record_id.as_str().to_owned(),
                chunk_key: identity.chunk_key.as_str().to_owned(),
            })
            .collect(),
        source_nodes: projection.source_nodes,
        projected_chunks_before_filter: projection.projected_chunks_before_filter,
        projected_chunks_after_filter: projection.projected_chunks_after_filter,
    }
}

impl RecordInputDto {
    pub fn into_record(self) -> Result<(Record, Metadata, Vec<ChunkInputDto>)> {
        let fields = self
            .fields
            .into_iter()
            .map(|entry| {
                Ok((
                    FieldName::new(entry.field).map_err(BoundaryError::core)?,
                    entry.value.into_core("record field")?,
                ))
            })
            .collect::<Result<BTreeMap<_, _>>>()?;
        Ok((
            Record {
                id: RecordId::new(self.id).map_err(BoundaryError::core)?,
                record_type: RecordType::new(self.record_type).map_err(BoundaryError::core)?,
                fields,
                content: self.content,
            },
            metadata_from_dto(self.metadata)?,
            self.chunks,
        ))
    }
}

impl RecordValueDto {
    fn into_core(self, path: &str) -> Result<RecordValue> {
        Ok(match self {
            Self::Null => RecordValue::Null,
            Self::Boolean { value } => RecordValue::Bool(value),
            Self::Integer { value } => RecordValue::I64(parse_i64(&value, path)?),
            Self::Float { value } if value.is_finite() => RecordValue::F64(value),
            Self::Float { .. } => return Err(BoundaryError::invalid(path, "float must be finite")),
            Self::String { value } => RecordValue::String(value),
            Self::List { value } => RecordValue::List(
                value
                    .into_iter()
                    .enumerate()
                    .map(|(index, item)| item.into_core(&format!("{path}[{index}]")))
                    .collect::<Result<_>>()?,
            ),
            Self::Map { value } => RecordValue::Map(
                value
                    .into_iter()
                    .map(|entry| {
                        let field = FieldName::new(entry.field).map_err(BoundaryError::core)?;
                        let value = entry.value.into_core(field.as_str())?;
                        Ok((field, value))
                    })
                    .collect::<Result<_>>()?,
            ),
        })
    }
}

pub(crate) fn metadata_from_dto(entries: Vec<MetadataEntryDto>) -> Result<Metadata> {
    entries
        .into_iter()
        .map(|entry| {
            let field = entry.field;
            Ok((field.clone(), entry.value.into_core(&field)?))
        })
        .collect()
}

impl MetadataValueDto {
    fn into_core(self, path: &str) -> Result<MetadataValue> {
        Ok(match self {
            Self::String(value) => MetadataValue::String(value),
            Self::Integer(value) => MetadataValue::Integer(parse_i64(&value, path)?),
            Self::Float(value) if value.is_finite() => MetadataValue::Float(value),
            Self::Float(_) => return Err(BoundaryError::invalid(path, "float must be finite")),
            Self::Boolean(value) => MetadataValue::Boolean(value),
            Self::Timestamp(value) => MetadataValue::TimestampMillis(parse_i64(&value, path)?),
        })
    }
}

impl FilterDto {
    pub fn into_core(self) -> Result<Filter> {
        Ok(match self {
            Self::Equals { field, value } => Filter::Equals {
                value: value.into_core(&field)?,
                field,
            },
            Self::NotEquals { field, value } => Filter::NotEquals {
                value: value.into_core(&field)?,
                field,
            },
            Self::In { field, values } => Filter::In {
                values: values
                    .into_iter()
                    .map(|value| value.into_core(&field))
                    .collect::<Result<_>>()?,
                field,
            },
            Self::Range {
                field,
                lower,
                upper,
            } => Filter::Range {
                lower: lower.map(|value| value.into_core(&field)).transpose()?,
                upper: upper.map(|value| value.into_core(&field)).transpose()?,
                field,
            },
            Self::Exists { field } => Filter::Exists { field },
            Self::All { children } => Filter::All(
                children
                    .into_iter()
                    .map(Self::into_core)
                    .collect::<Result<_>>()?,
            ),
            Self::Any { children } => Filter::Any(
                children
                    .into_iter()
                    .map(Self::into_core)
                    .collect::<Result<_>>()?,
            ),
        })
    }
}

impl GraphSchemaDto {
    pub fn into_core(self) -> Result<GraphSchema> {
        let record_nodes = self
            .record_nodes
            .into_iter()
            .map(|mapping| {
                Ok(RecordNodeSchema {
                    record_type: RecordType::new(mapping.record_type)
                        .map_err(BoundaryError::core)?,
                    node_type: NodeType::new(mapping.node_type).map_err(BoundaryError::graph)?,
                    queryable_fields: mapping
                        .queryable_fields
                        .into_iter()
                        .map(field_path)
                        .collect::<Result<_>>()?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let relationships = self
            .relationships
            .into_iter()
            .map(|relationship| {
                Ok(RelationshipSchema {
                    relationship_type: RelationshipType::new(relationship.relationship_type)
                        .map_err(BoundaryError::graph)?,
                    source_node_type: NodeType::new(relationship.source_node_type)
                        .map_err(BoundaryError::graph)?,
                    target_node_type: NodeType::new(relationship.target_node_type)
                        .map_err(BoundaryError::graph)?,
                    source_field: field_path(relationship.source_field)?,
                    cardinality: match relationship.cardinality {
                        CardinalityDto::One => Cardinality::One,
                        CardinalityDto::OptionalOne => Cardinality::OptionalOne,
                        CardinalityDto::Many => Cardinality::Many,
                    },
                    missing_target: match relationship.missing_target {
                        MissingTargetDto::Error => MissingTargetPolicy::Error,
                        MissingTargetDto::OmitEdge => MissingTargetPolicy::OmitEdge,
                    },
                    duplicate_references: match relationship.duplicate_references {
                        DuplicateReferencesDto::Error => DuplicateReferencePolicy::Error,
                        DuplicateReferencesDto::Deduplicate => {
                            DuplicateReferencePolicy::Deduplicate
                        }
                    },
                    allow_self_edge: relationship.allow_self_edge,
                    inverse_relationship: relationship
                        .inverse_relationship
                        .map(RelationshipType::new)
                        .transpose()
                        .map_err(BoundaryError::graph)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let mut schema = GraphSchema::new(record_nodes).with_relationships(relationships);
        if let Some(chunk) = self.chunk_nodes {
            schema = schema.with_chunk_nodes(ChunkNodeSchema {
                node_type: NodeType::new(chunk.node_type).map_err(BoundaryError::graph)?,
                owns_relationship: RelationshipType::new(chunk.owns_relationship)
                    .map_err(BoundaryError::graph)?,
                inverse_relationship: chunk
                    .inverse_relationship
                    .map(RelationshipType::new)
                    .transpose()
                    .map_err(BoundaryError::graph)?,
            });
        }
        schema.validate().map_err(BoundaryError::graph)?;
        Ok(schema)
    }
}

impl GraphQueryDto {
    pub fn into_core(self) -> Result<GraphQuery> {
        let seed = match self.seed {
            GraphSeedDto::NodeIds { nodes } => Seed::NodeIds(
                nodes
                    .into_iter()
                    .map(NodeIdDto::into_core)
                    .collect::<Result<_>>()?,
            ),
            GraphSeedDto::Equals {
                node_type,
                field,
                values,
            } => Seed::Equals {
                node_type: NodeType::new(node_type).map_err(BoundaryError::graph)?,
                field: field_path(field)?,
                values: values
                    .into_iter()
                    .map(GraphScalarDto::into_core)
                    .collect::<Result<_>>()?,
            },
        };
        let mut query = GraphQuery::new(seed);
        for step in self.steps {
            query = query.traverse(Traverse {
                relationship: RelationshipType::new(step.relationship)
                    .map_err(BoundaryError::graph)?,
                direction: match step.direction {
                    DirectionDto::Outgoing => Direction::Outgoing,
                    DirectionDto::Incoming => Direction::Incoming,
                },
                min_hops: step.min_hops,
                max_hops: step.max_hops,
            });
        }
        if let Some(limits) = self.limits {
            query = query.with_limits(QueryLimits {
                max_hops: limits.max_hops,
                max_visited: limits.max_visited,
                max_results: limits.max_results,
                max_working_bytes: limits.max_working_bytes,
            });
        }
        Ok(query)
    }
}

impl NodeIdDto {
    fn into_core(self) -> Result<NodeId> {
        let node_type = NodeType::new(self.node_type).map_err(BoundaryError::graph)?;
        let record_id = RecordId::new(self.record_id).map_err(BoundaryError::core)?;
        Ok(match self.source_kind {
            NodeSourceKindDto::Record => NodeId::record(node_type, record_id),
            NodeSourceKindDto::Chunk => {
                let key = self.chunk_key.ok_or_else(|| {
                    BoundaryError::invalid("seed.nodes.chunkKey", "required for a chunk node")
                })?;
                NodeId::chunk(
                    node_type,
                    ChunkIdentity::new(record_id, ChunkKey::new(key).map_err(BoundaryError::core)?),
                )
            }
        })
    }
}

impl GraphScalarDto {
    fn into_core(self) -> Result<GraphScalar> {
        Ok(match self {
            Self::String(value) => GraphScalar::String(value),
            Self::Integer(value) => GraphScalar::I64(parse_i64(&value, "graph scalar")?),
            Self::Boolean(value) => GraphScalar::Bool(value),
        })
    }
}

pub(crate) fn record_chunks(
    chunks: Vec<ChunkInputDto>,
    vectors: &[f32],
    dimension: usize,
) -> Result<Vec<RecordChunkInput>> {
    chunks
        .into_iter()
        .map(|chunk| {
            let row = chunk.embedding_index.ok_or_else(|| {
                BoundaryError::invalid(
                    "records.chunks.embeddingIndex",
                    "required for every retrieval chunk",
                )
            })? as usize;
            let start = row.checked_mul(dimension).ok_or_else(|| {
                BoundaryError::invalid("records.chunks.embeddingIndex", "offset overflow")
            })?;
            let end = start.checked_add(dimension).ok_or_else(|| {
                BoundaryError::invalid("records.chunks.embeddingIndex", "offset overflow")
            })?;
            let embedding = vectors.get(start..end).ok_or_else(|| {
                BoundaryError::invalid(
                    "records.chunks.embeddingIndex",
                    format!("row {row} exceeds the flattened Float32Array (dimension {dimension})"),
                )
            })?;
            Ok(RecordChunkInput {
                key: ChunkKey::new(chunk.key).map_err(BoundaryError::core)?,
                text: chunk.text,
                embedding: embedding.to_vec(),
                metadata: metadata_from_dto(chunk.metadata)?,
            })
        })
        .collect()
}

pub(crate) fn search_hits(
    hits: Vec<SearchHit>,
    chunk: impl Fn(u64) -> Option<StoredChunk>,
) -> Vec<SearchHitDto> {
    hits.into_iter()
        .filter_map(|hit| {
            let stored = chunk(hit.chunk_id)?;
            Some(SearchHitDto {
                chunk_id: hit.chunk_id.to_string(),
                document_id: hit.document_id,
                text: stored.text,
                metadata: metadata_to_dto(&stored.metadata),
                score: hit.score,
                vector_score: hit.trace.vector_score,
            })
        })
        .collect()
}

pub(crate) fn hybrid_hits(
    hits: Vec<HybridHit>,
    chunk: impl Fn(u64) -> Option<StoredChunk>,
) -> Vec<HybridHitDto> {
    hits.into_iter()
        .filter_map(|hit| {
            let stored = chunk(hit.chunk_id)?;
            Some(HybridHitDto {
                chunk_id: hit.chunk_id.to_string(),
                document_id: hit.document_id,
                text: stored.text,
                metadata: metadata_to_dto(&stored.metadata),
                score: hit.score,
                vector_score: hit.vector_score,
                keyword_score: hit.keyword_score,
                vector_rank: hit.trace.vector_rank,
                keyword_rank: hit.trace.keyword_rank,
                normalized_vector_score: hit.trace.normalized_vector_score,
                normalized_keyword_score: hit.trace.normalized_keyword_score,
                matched_terms: hit.trace.matched_terms,
            })
        })
        .collect()
}

pub(crate) fn keyword_hits(
    hits: Vec<KeywordHit>,
    chunk: impl Fn(u64) -> Option<StoredChunk>,
) -> Vec<KeywordHitDto> {
    hits.into_iter()
        .filter_map(|hit| {
            let stored = chunk(hit.chunk_id)?;
            Some(KeywordHitDto {
                chunk_id: hit.chunk_id.to_string(),
                document_id: hit.document_id,
                text: stored.text,
                metadata: metadata_to_dto(&stored.metadata),
                score: hit.score,
                matched_terms: hit.matched_terms,
            })
        })
        .collect()
}

pub(crate) fn graph_result(selection_id: u32, result: &GraphResult) -> GraphResultDto {
    GraphResultDto {
        selection_id,
        corpus_id: result.corpus_id.as_str().to_owned(),
        generation: result.generation.get().to_string(),
        matches: result.matches.iter().map(graph_match).collect(),
        truncated: result.truncated.map(truncation_reason),
        trace: GraphTraceDto {
            seed_count: result.trace.seed_count,
            visited_states: result.trace.visited_states,
            traversed_edges: result.trace.traversed_edges,
            result_count: result.trace.result_count,
            diagnostics: result.trace.diagnostics,
        },
    }
}

fn truncation_reason(reason: TruncationReason) -> String {
    match reason {
        TruncationReason::MaxHops => "maxHops",
        TruncationReason::MaxVisited => "maxVisited",
        TruncationReason::MaxResults => "maxResults",
        TruncationReason::MaxWorkingBytes => "maxWorkingBytes",
    }
    .to_owned()
}

fn graph_match(value: &GraphMatch) -> GraphMatchDto {
    let node = graph_node(&value.node_id);
    GraphMatchDto {
        node_type: value.node_id.node_type.as_str().to_owned(),
        source_kind: node.source_kind,
        record_id: node.record_id,
        chunk_key: node.chunk_key,
        depth: value.depth,
        path: value
            .path
            .iter()
            .map(|edge| GraphPathEdgeDto {
                relationship: edge.edge_id.relationship_type.as_str().to_owned(),
                source: graph_node(&edge.edge_id.source),
                target: graph_node(&edge.edge_id.target),
                occurrence_ordinal: edge.edge_id.occurrence_ordinal,
                schema_rule_index: edge.provenance.schema_rule_index,
                source_record_id: edge.provenance.source_record_id.as_str().to_owned(),
                source_field: edge.provenance.source_field.as_ref().map(|path| {
                    path.segments()
                        .iter()
                        .map(|segment| segment.as_str().to_owned())
                        .collect()
                }),
                derived_inverse: edge.provenance.derived_inverse,
                built_in: edge.provenance.built_in,
            })
            .collect(),
    }
}

fn graph_node(value: &NodeId) -> GraphNodeDto {
    let (source_kind, record_id, chunk_key) = match &value.source {
        NodeSource::Record(record_id) => ("record", record_id.as_str().to_owned(), None),
        NodeSource::Chunk(identity) => (
            "chunk",
            identity.record_id.as_str().to_owned(),
            Some(identity.chunk_key.as_str().to_owned()),
        ),
    };
    GraphNodeDto {
        node_type: value.node_type.as_str().to_owned(),
        source_kind,
        record_id,
        chunk_key,
    }
}

fn metadata_to_dto(metadata: &Metadata) -> Vec<MetadataEntryDto> {
    metadata
        .iter()
        .map(|(field, value)| MetadataEntryDto {
            field: field.clone(),
            value: match value {
                MetadataValue::String(value) => MetadataValueDto::String(value.clone()),
                MetadataValue::Integer(value) => MetadataValueDto::Integer(value.to_string()),
                MetadataValue::Float(value) => MetadataValueDto::Float(*value),
                MetadataValue::Boolean(value) => MetadataValueDto::Boolean(*value),
                MetadataValue::TimestampMillis(value) => {
                    MetadataValueDto::Timestamp(value.to_string())
                }
            },
        })
        .collect()
}

fn field_path(segments: Vec<String>) -> Result<FieldPath> {
    FieldPath::new(
        segments
            .into_iter()
            .map(|segment| FieldName::new(segment).map_err(BoundaryError::core))
            .collect::<Result<_>>()?,
    )
    .map_err(BoundaryError::graph)
}

fn parse_i64(value: &str, path: &str) -> Result<i64> {
    value.parse().map_err(|_| {
        BoundaryError::invalid(
            path,
            format!("'{value}' is not a base-10 signed 64-bit integer"),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use retrievalkit_graph::{EdgeId, EdgeProvenance, GraphPathEdge};

    #[test]
    fn flattened_vectors_are_selected_by_row_without_text_json() {
        let chunks = vec![ChunkInputDto {
            key: "summary".to_owned(),
            text: "fast wasm".to_owned(),
            metadata: Vec::new(),
            embedding_index: Some(1),
        }];
        let converted = record_chunks(chunks, &[9.0, 9.0, 1.0, 0.0], 2).unwrap();
        assert_eq!(converted[0].embedding, vec![1.0, 0.0]);
    }

    #[test]
    fn exact_integers_use_string_transport() {
        let value = MetadataValueDto::Integer("9007199254740993".to_owned())
            .into_core("metadata")
            .unwrap();
        assert_eq!(value, MetadataValue::Integer(9_007_199_254_740_993));
    }

    #[test]
    fn graph_match_dto_preserves_path_edge_and_provenance() {
        let source_record = RecordId::new("source").unwrap();
        let target_record = RecordId::new("target").unwrap();
        let source = NodeId::record(NodeType::new("Topic").unwrap(), source_record.clone());
        let target = NodeId::record(NodeType::new("Topic").unwrap(), target_record);
        let value = GraphMatch {
            node_id: target.clone(),
            depth: 1,
            path: vec![GraphPathEdge {
                edge_id: EdgeId {
                    relationship_type: RelationshipType::new("linksTo").unwrap(),
                    source,
                    target,
                    occurrence_ordinal: 2,
                },
                provenance: EdgeProvenance {
                    schema_rule_index: 3,
                    source_record_id: source_record,
                    source_field: Some(FieldPath::single(FieldName::new("links").unwrap())),
                    derived_inverse: false,
                    built_in: false,
                },
            }],
        };

        let converted = graph_match(&value);
        assert_eq!(converted.path.len(), 1);
        assert_eq!(converted.path[0].relationship, "linksTo");
        assert_eq!(converted.path[0].source.record_id, "source");
        assert_eq!(converted.path[0].target.record_id, "target");
        assert_eq!(converted.path[0].occurrence_ordinal, 2);
        assert_eq!(converted.path[0].schema_rule_index, 3);
        assert_eq!(
            converted.path[0].source_field,
            Some(vec!["links".to_owned()])
        );
    }

    #[test]
    fn truncation_reasons_have_stable_camel_case_values() {
        assert_eq!(truncation_reason(TruncationReason::MaxHops), "maxHops");
        assert_eq!(
            truncation_reason(TruncationReason::MaxVisited),
            "maxVisited"
        );
        assert_eq!(
            truncation_reason(TruncationReason::MaxResults),
            "maxResults"
        );
        assert_eq!(
            truncation_reason(TruncationReason::MaxWorkingBytes),
            "maxWorkingBytes"
        );
    }

    #[test]
    fn graph_equals_seed_accepts_camel_case_boundary_fields() {
        let query: GraphQueryDto = serde_json::from_value(serde_json::json!({
            "seed": {
                "kind": "equals",
                "nodeType": "Event",
                "field": ["name"],
                "values": [{ "kind": "string", "value": "Landing" }]
            }
        }))
        .unwrap();

        match query.seed {
            GraphSeedDto::Equals {
                node_type,
                field,
                values,
            } => {
                assert_eq!(node_type, "Event");
                assert_eq!(field, vec!["name"]);
                assert_eq!(values.len(), 1);
            }
            GraphSeedDto::NodeIds { .. } => panic!("expected equals seed"),
        }
    }
}
