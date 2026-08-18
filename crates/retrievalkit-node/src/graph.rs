use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use napi::bindgen_prelude::{AsyncTask, Float32Array, Task};
use napi::{Env, Result};
use napi_derive::napi;
use retrievalkit_core::{
    Bm25Config, ChunkIdentity, CorpusChunkInput, CorpusId, EmbeddedDocument, FieldName,
    HybridQuery, KeywordQuery, Record, RecordId, RecordInput, RecordType, SearchQuery,
};
use retrievalkit_graph::{
    Cardinality, ChunkNodeSchema, Direction, DuplicateReferencePolicy, FieldPath,
    GraphCandidateProjection, GraphDatabase, GraphDatabaseBuilder, GraphError, GraphQuery,
    GraphResult, GraphRetrievalDatabase, GraphRetrievalDatabaseBuilder, GraphScalar, GraphSchema,
    MissingTargetPolicy, NodeId, NodeSource, NodeType, QueryLimits, RecordNodeSchema,
    RelationshipSchema, RelationshipType, Seed, Traverse, TruncationReason,
};

use crate::common::{
    closed_error, core_error, invalid_boundary, metadata_from_native, parse_encoding, parse_metric,
    state_error, tagged_error, NativeFilter, NativeMetadataEntry, NativeRecordField,
    NativeRecordInput, OwnedRecordInput,
};
use crate::retrieval::{
    hybrid_hits, keyword_hits, search_hits, NativeHybridHit, NativeKeywordHit, NativeSearchHit,
};

#[napi(object)]
#[derive(Clone)]
pub struct NativeRecordNodeSchema {
    pub record_type: String,
    pub node_type: String,
    pub queryable_fields: Vec<Vec<String>>,
}

#[napi(object)]
#[derive(Clone)]
pub struct NativeRelationshipSchema {
    pub relationship_type: String,
    pub source_node_type: String,
    pub target_node_type: String,
    pub source_field: Vec<String>,
    pub cardinality: String,
    pub missing_target: String,
    pub duplicate_references: String,
    pub allow_self_edge: bool,
    pub inverse_relationship: Option<String>,
}

#[napi(object)]
#[derive(Clone)]
pub struct NativeChunkNodeSchema {
    pub node_type: String,
    pub owns_relationship: String,
    pub inverse_relationship: Option<String>,
}

#[napi(object)]
#[derive(Clone)]
pub struct NativeGraphSchema {
    pub record_nodes: Vec<NativeRecordNodeSchema>,
    pub relationships: Vec<NativeRelationshipSchema>,
    pub chunk_nodes: Option<NativeChunkNodeSchema>,
}

impl NativeGraphSchema {
    fn into_core(self) -> Result<GraphSchema> {
        let record_nodes = self
            .record_nodes
            .into_iter()
            .map(|mapping| {
                Ok(RecordNodeSchema {
                    record_type: RecordType::new(mapping.record_type).map_err(core_error)?,
                    node_type: NodeType::new(mapping.node_type).map_err(graph_error)?,
                    queryable_fields: mapping
                        .queryable_fields
                        .into_iter()
                        .map(field_path)
                        .collect::<Result<Vec<_>>>()?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let relationships = self
            .relationships
            .into_iter()
            .map(|relationship| {
                Ok(RelationshipSchema {
                    relationship_type: RelationshipType::new(relationship.relationship_type)
                        .map_err(graph_error)?,
                    source_node_type: NodeType::new(relationship.source_node_type)
                        .map_err(graph_error)?,
                    target_node_type: NodeType::new(relationship.target_node_type)
                        .map_err(graph_error)?,
                    source_field: field_path(relationship.source_field)?,
                    cardinality: match relationship.cardinality.as_str() {
                        "one" => Cardinality::One,
                        "optionalOne" => Cardinality::OptionalOne,
                        "many" => Cardinality::Many,
                        actual => {
                            return Err(invalid_boundary(
                                "schema.relationships.cardinality",
                                &format!("expected one, optionalOne, or many; got '{actual}'"),
                            ))
                        }
                    },
                    missing_target: match relationship.missing_target.as_str() {
                        "error" => MissingTargetPolicy::Error,
                        "omitEdge" => MissingTargetPolicy::OmitEdge,
                        actual => {
                            return Err(invalid_boundary(
                                "schema.relationships.missingTarget",
                                &format!("expected error or omitEdge; got '{actual}'"),
                            ))
                        }
                    },
                    duplicate_references: match relationship.duplicate_references.as_str() {
                        "error" => DuplicateReferencePolicy::Error,
                        "deduplicate" => DuplicateReferencePolicy::Deduplicate,
                        actual => {
                            return Err(invalid_boundary(
                                "schema.relationships.duplicateReferences",
                                &format!("expected error or deduplicate; got '{actual}'"),
                            ))
                        }
                    },
                    allow_self_edge: relationship.allow_self_edge,
                    inverse_relationship: relationship
                        .inverse_relationship
                        .map(RelationshipType::new)
                        .transpose()
                        .map_err(graph_error)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let mut schema = GraphSchema::new(record_nodes).with_relationships(relationships);
        if let Some(chunk_nodes) = self.chunk_nodes {
            schema = schema.with_chunk_nodes(ChunkNodeSchema {
                node_type: NodeType::new(chunk_nodes.node_type).map_err(graph_error)?,
                owns_relationship: RelationshipType::new(chunk_nodes.owns_relationship)
                    .map_err(graph_error)?,
                inverse_relationship: chunk_nodes
                    .inverse_relationship
                    .map(RelationshipType::new)
                    .transpose()
                    .map_err(graph_error)?,
            });
        }
        schema.validate().map_err(graph_error)?;
        Ok(schema)
    }
}

fn field_path(segments: Vec<String>) -> Result<FieldPath> {
    let segments = segments
        .into_iter()
        .map(|segment| FieldName::new(segment).map_err(core_error))
        .collect::<Result<Vec<_>>>()?;
    FieldPath::new(segments).map_err(graph_error)
}

#[napi(object)]
pub struct NativeGraphDocumentInput {
    pub id: String,
    pub text: String,
    pub metadata: Vec<NativeMetadataEntry>,
    pub embedding: Float32Array,
}

#[napi(object)]
pub struct NativeGraphRecordInput {
    pub id: String,
    pub record_type: String,
    pub fields: Vec<NativeRecordField>,
    pub content: Option<String>,
    pub metadata: Vec<NativeMetadataEntry>,
    pub embedding: Option<Float32Array>,
    pub documents: Vec<NativeGraphDocumentInput>,
}

struct OwnedGraphRecordInput {
    record: Record,
    metadata: retrievalkit_core::Metadata,
    embedding: Option<Vec<f32>>,
    documents: Vec<EmbeddedDocument>,
}

impl NativeGraphRecordInput {
    fn into_owned(self) -> Result<OwnedGraphRecordInput> {
        let record = NativeRecordInput {
            id: self.id,
            record_type: self.record_type,
            fields: self.fields,
            content: self.content,
            metadata: self.metadata,
            chunks: Vec::new(),
        }
        .into_owned()?;
        let documents = self
            .documents
            .into_iter()
            .map(|document| {
                Ok(EmbeddedDocument {
                    document: retrievalkit_core::Document {
                        id: document.id,
                        text: document.text,
                        metadata: metadata_from_native(document.metadata)?,
                    },
                    embedding: document.embedding.to_vec(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(OwnedGraphRecordInput {
            record: record.record,
            metadata: record.metadata,
            embedding: self.embedding.map(|value| value.to_vec()),
            documents,
        })
    }
}

#[napi(object)]
#[derive(Clone)]
pub struct NativeNodeId {
    pub node_type: String,
    pub source_kind: String,
    pub record_id: String,
    pub chunk_key: Option<String>,
}

#[napi(object)]
#[derive(Clone)]
pub struct NativeGraphSeed {
    pub kind: String,
    pub nodes: Option<Vec<NativeNodeId>>,
    pub node_type: Option<String>,
    pub field: Option<Vec<String>>,
    pub values: Option<Vec<NativeGraphScalar>>,
}

#[napi(object)]
#[derive(Clone)]
pub struct NativeGraphScalar {
    pub kind: String,
    pub string_value: Option<String>,
    pub integer_value: Option<String>,
    pub boolean_value: Option<bool>,
}

#[napi(object)]
#[derive(Clone)]
pub struct NativeTraverse {
    pub relationship: String,
    pub direction: String,
    pub min_hops: u32,
    pub max_hops: u32,
}

#[napi(object)]
#[derive(Clone)]
pub struct NativeGraphLimits {
    pub max_hops: u32,
    pub max_visited: u32,
    pub max_results: u32,
    pub max_working_bytes: f64,
}

#[napi(object)]
#[derive(Clone)]
pub struct NativeGraphQuery {
    pub seed: NativeGraphSeed,
    pub steps: Vec<NativeTraverse>,
    pub limits: Option<NativeGraphLimits>,
}

impl NativeGraphQuery {
    fn into_core(self) -> Result<GraphQuery> {
        let seed = match self.seed.kind.as_str() {
            "nodes" => Seed::NodeIds(
                self.seed
                    .nodes
                    .ok_or_else(|| invalid_boundary("query.seed", "nodes are required"))?
                    .into_iter()
                    .map(node_id_from_native)
                    .collect::<Result<Vec<_>>>()?,
            ),
            "equals" => Seed::Equals {
                node_type: NodeType::new(
                    self.seed
                        .node_type
                        .ok_or_else(|| invalid_boundary("query.seed", "nodeType is required"))?,
                )
                .map_err(graph_error)?,
                field: field_path(
                    self.seed
                        .field
                        .ok_or_else(|| invalid_boundary("query.seed", "field is required"))?,
                )?,
                values: self
                    .seed
                    .values
                    .ok_or_else(|| invalid_boundary("query.seed", "values are required"))?
                    .into_iter()
                    .map(graph_scalar)
                    .collect::<Result<Vec<_>>>()?,
            },
            actual => {
                return Err(invalid_boundary(
                    "query.seed.kind",
                    &format!("expected nodes or equals; got '{actual}'"),
                ))
            }
        };
        let mut query = GraphQuery::new(seed);
        for step in self.steps {
            query = query.traverse(Traverse {
                relationship: RelationshipType::new(step.relationship).map_err(graph_error)?,
                direction: match step.direction.as_str() {
                    "outgoing" => Direction::Outgoing,
                    "incoming" => Direction::Incoming,
                    actual => {
                        return Err(invalid_boundary(
                            "query.steps.direction",
                            &format!("expected outgoing or incoming; got '{actual}'"),
                        ))
                    }
                },
                min_hops: step.min_hops as usize,
                max_hops: step.max_hops as usize,
            });
        }
        if let Some(limits) = self.limits {
            if !limits.max_working_bytes.is_finite()
                || limits.max_working_bytes < 0.0
                || limits.max_working_bytes.fract() != 0.0
            {
                return Err(invalid_boundary(
                    "query.limits.maxWorkingBytes",
                    "expected a finite non-negative integer",
                ));
            }
            query = query.with_limits(QueryLimits {
                max_hops: limits.max_hops as usize,
                max_visited: limits.max_visited as usize,
                max_results: limits.max_results as usize,
                max_working_bytes: limits.max_working_bytes as usize,
            });
        }
        Ok(query)
    }
}

fn graph_scalar(value: NativeGraphScalar) -> Result<GraphScalar> {
    match value.kind.as_str() {
        "string" => value
            .string_value
            .map(GraphScalar::String)
            .ok_or_else(|| invalid_boundary("query.seed.values", "stringValue is required")),
        "integer" => value
            .integer_value
            .ok_or_else(|| invalid_boundary("query.seed.values", "integerValue is required"))?
            .parse::<i64>()
            .map(GraphScalar::I64)
            .map_err(|_| {
                invalid_boundary(
                    "query.seed.values",
                    "integerValue must be a signed 64-bit base-10 integer",
                )
            }),
        "boolean" => value
            .boolean_value
            .map(GraphScalar::Bool)
            .ok_or_else(|| invalid_boundary("query.seed.values", "booleanValue is required")),
        actual => Err(invalid_boundary(
            "query.seed.values",
            &format!("unsupported graph scalar kind '{actual}'"),
        )),
    }
}

fn node_id_from_native(value: NativeNodeId) -> Result<NodeId> {
    let node_type = NodeType::new(value.node_type).map_err(graph_error)?;
    let record_id = RecordId::new(value.record_id).map_err(core_error)?;
    match value.source_kind.as_str() {
        "record" => Ok(NodeId::record(node_type, record_id)),
        "chunk" => Ok(NodeId::chunk(
            node_type,
            ChunkIdentity::new(
                record_id,
                retrievalkit_core::ChunkKey::new(value.chunk_key.ok_or_else(|| {
                    invalid_boundary("node.chunkKey", "chunk nodes require chunkKey")
                })?)
                .map_err(core_error)?,
            ),
        )),
        actual => Err(invalid_boundary(
            "node.sourceKind",
            &format!("expected record or chunk; got '{actual}'"),
        )),
    }
}

fn node_id_to_native(value: &NodeId) -> NativeNodeId {
    match &value.source {
        NodeSource::Record(record_id) => NativeNodeId {
            node_type: value.node_type.as_str().to_owned(),
            source_kind: "record".to_owned(),
            record_id: record_id.as_str().to_owned(),
            chunk_key: None,
        },
        NodeSource::Chunk(identity) => NativeNodeId {
            node_type: value.node_type.as_str().to_owned(),
            source_kind: "chunk".to_owned(),
            record_id: identity.record_id.as_str().to_owned(),
            chunk_key: Some(identity.chunk_key.as_str().to_owned()),
        },
    }
}

#[napi(object)]
#[derive(Clone)]
pub struct NativeGraphEdgeProvenance {
    pub schema_rule_index: u32,
    pub source_record_id: String,
    pub source_field: Option<Vec<String>>,
    pub derived_inverse: bool,
    pub built_in: bool,
}

#[napi(object)]
#[derive(Clone)]
pub struct NativeGraphPathEdge {
    pub relationship: String,
    pub source: NativeNodeId,
    pub target: NativeNodeId,
    pub occurrence_ordinal: u32,
    pub provenance: NativeGraphEdgeProvenance,
}

#[napi(object)]
#[derive(Clone)]
pub struct NativeGraphMatch {
    pub node: NativeNodeId,
    pub depth: u32,
    pub path: Vec<NativeGraphPathEdge>,
}

#[napi(object)]
#[derive(Clone)]
pub struct NativeGraphTrace {
    pub seed_count: u32,
    pub visited_states: u32,
    pub traversed_edges: u32,
    pub result_count: u32,
    pub diagnostics: u32,
}

#[napi(object)]
#[derive(Clone)]
pub struct NativeGraphResult {
    pub matches: Vec<NativeGraphMatch>,
    pub truncated: Option<String>,
    pub trace: NativeGraphTrace,
}

#[napi(object)]
#[derive(Clone)]
pub struct NativeCandidateIdentity {
    pub record_id: String,
    pub chunk_key: String,
}

#[napi(object)]
#[derive(Clone)]
pub struct NativeCandidateProjection {
    pub candidates: Vec<NativeCandidateIdentity>,
    pub source_nodes: u32,
    pub projected_chunks_before_filter: u32,
    pub projected_chunks_after_filter: u32,
}

#[napi]
pub struct NativeGraphSelection {
    result: Arc<Mutex<Option<GraphResult>>>,
    closed: Arc<AtomicBool>,
}

#[napi]
impl NativeGraphSelection {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            result: Arc::new(Mutex::new(None)),
            closed: Arc::new(AtomicBool::new(false)),
        }
    }

    #[napi]
    pub fn close(&self) -> AsyncTask<CloseSelectionTask> {
        self.closed.store(true, Ordering::Release);
        AsyncTask::new(CloseSelectionTask {
            result: Arc::clone(&self.result),
        })
    }

    #[napi(getter)]
    pub fn closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }
}

impl Default for NativeGraphSelection {
    fn default() -> Self {
        Self::new()
    }
}

pub struct CloseSelectionTask {
    result: Arc<Mutex<Option<GraphResult>>>,
}

impl Task for CloseSelectionTask {
    type Output = ();
    type JsValue = ();

    fn compute(&mut self) -> Result<()> {
        *self.result.lock().map_err(|_| {
            state_error("graph selection lock was poisoned by a previous native failure")
        })? = None;
        Ok(())
    }

    fn resolve(&mut self, _env: Env, _output: ()) -> Result<()> {
        Ok(())
    }
}

enum GraphState {
    Empty,
    GraphBuilding(Box<GraphDatabaseBuilder>),
    CombinedBuilding(Box<GraphRetrievalDatabaseBuilder>),
    GraphReady(Box<GraphDatabase>),
    CombinedReady(Box<GraphRetrievalDatabase>),
}

struct GraphShared {
    state: Mutex<GraphState>,
    closed: AtomicBool,
}

impl GraphShared {
    fn require_open(&self) -> Result<()> {
        if self.closed.load(Ordering::Acquire) {
            Err(closed_error("graph database"))
        } else {
            Ok(())
        }
    }
}

#[napi]
pub struct NativeGraphHandle {
    shared: Arc<GraphShared>,
}

#[napi]
impl NativeGraphHandle {
    #[napi(constructor)]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        kind: String,
        corpus_id: String,
        schema: NativeGraphSchema,
        metric: Option<String>,
        encoding: Option<String>,
        bm25_k1: Option<f64>,
        bm25_b: Option<f64>,
        stop_words: Option<Vec<String>>,
    ) -> Result<Self> {
        let corpus_id = CorpusId::new(corpus_id).map_err(core_error)?;
        let schema = schema.into_core()?;
        let state = match kind.as_str() {
            "graph" => {
                GraphState::GraphBuilding(Box::new(GraphDatabaseBuilder::new(corpus_id, schema)))
            }
            "combined" => {
                let bm25 = Bm25Config::try_new(
                    bm25_k1.unwrap_or(1.2) as f32,
                    bm25_b.unwrap_or(0.75) as f32,
                    stop_words.unwrap_or_default(),
                )
                .map_err(core_error)?;
                GraphState::CombinedBuilding(Box::new(
                    GraphRetrievalDatabaseBuilder::new(
                        corpus_id,
                        schema,
                        parse_metric(metric.as_deref().unwrap_or("cosine"))?,
                        parse_encoding(encoding.as_deref().unwrap_or("i8"))?,
                    )
                    .try_with_bm25_config(bm25)
                    .map_err(graph_error)?,
                ))
            }
            actual => {
                return Err(invalid_boundary(
                    "kind",
                    &format!("expected graph or combined; got '{actual}'"),
                ))
            }
        };
        Ok(Self {
            shared: Arc::new(GraphShared {
                state: Mutex::new(state),
                closed: AtomicBool::new(false),
            }),
        })
    }

    #[napi(factory)]
    pub fn empty() -> Self {
        Self {
            shared: Arc::new(GraphShared {
                state: Mutex::new(GraphState::Empty),
                closed: AtomicBool::new(false),
            }),
        }
    }

    #[napi]
    pub fn add_records(
        &self,
        records: Vec<NativeGraphRecordInput>,
    ) -> Result<AsyncTask<AddGraphRecordsTask>> {
        self.shared.require_open()?;
        let records = records
            .into_iter()
            .map(NativeGraphRecordInput::into_owned)
            .collect::<Result<Vec<_>>>()?;
        Ok(AsyncTask::new(AddGraphRecordsTask {
            shared: Arc::clone(&self.shared),
            records,
        }))
    }

    #[napi(js_name = "_addFixtureRecords")]
    pub fn add_fixture_records(
        &self,
        records: Vec<NativeRecordInput>,
    ) -> Result<AsyncTask<AddGraphFixtureRecordsTask>> {
        self.shared.require_open()?;
        let records = records
            .into_iter()
            .map(NativeRecordInput::into_owned)
            .collect::<Result<Vec<_>>>()?;
        Ok(AsyncTask::new(AddGraphFixtureRecordsTask {
            shared: Arc::clone(&self.shared),
            records,
        }))
    }

    #[napi]
    pub fn build(&self) -> Result<AsyncTask<BuildGraphTask>> {
        self.shared.require_open()?;
        Ok(AsyncTask::new(BuildGraphTask {
            shared: Arc::clone(&self.shared),
        }))
    }

    #[napi]
    pub fn load(&self, kind: String, path: String) -> Result<AsyncTask<LoadGraphTask>> {
        self.shared.require_open()?;
        Ok(AsyncTask::new(LoadGraphTask {
            shared: Arc::clone(&self.shared),
            kind,
            path,
        }))
    }

    #[napi]
    pub fn query(
        &self,
        query: NativeGraphQuery,
        selection: &NativeGraphSelection,
    ) -> Result<AsyncTask<GraphQueryTask>> {
        self.shared.require_open()?;
        if selection.closed.load(Ordering::Acquire) {
            return Err(closed_error("graph selection"));
        }
        Ok(AsyncTask::new(GraphQueryTask {
            shared: Arc::clone(&self.shared),
            selection: Arc::clone(&selection.result),
            query: query.into_core()?,
        }))
    }

    #[napi]
    pub fn project_candidates(
        &self,
        selection: &NativeGraphSelection,
        filter: Option<NativeFilter>,
    ) -> Result<AsyncTask<ProjectCandidatesTask>> {
        self.shared.require_open()?;
        if selection.closed.load(Ordering::Acquire) {
            return Err(closed_error("graph selection"));
        }
        Ok(AsyncTask::new(ProjectCandidatesTask {
            shared: Arc::clone(&self.shared),
            selection: Arc::clone(&selection.result),
            filter: filter.map(NativeFilter::into_core).transpose()?,
        }))
    }

    #[napi]
    pub fn semantic_search(
        &self,
        embedding: Float32Array,
        top_k: u32,
        filter: Option<NativeFilter>,
        selection: Option<&NativeGraphSelection>,
    ) -> Result<AsyncTask<GraphSemanticSearchTask>> {
        self.shared.require_open()?;
        Ok(AsyncTask::new(GraphSemanticSearchTask {
            shared: Arc::clone(&self.shared),
            embedding: embedding.to_vec(),
            top_k: top_k as usize,
            filter: filter.map(NativeFilter::into_core).transpose()?,
            selection: selection.map(|value| Arc::clone(&value.result)),
        }))
    }

    #[napi]
    pub fn keyword_search(
        &self,
        text: String,
        top_k: u32,
        filter: Option<NativeFilter>,
        selection: Option<&NativeGraphSelection>,
    ) -> Result<AsyncTask<GraphKeywordSearchTask>> {
        self.shared.require_open()?;
        Ok(AsyncTask::new(GraphKeywordSearchTask {
            shared: Arc::clone(&self.shared),
            text,
            top_k: top_k as usize,
            filter: filter.map(NativeFilter::into_core).transpose()?,
            selection: selection.map(|value| Arc::clone(&value.result)),
        }))
    }

    #[napi]
    #[allow(clippy::too_many_arguments)]
    pub fn hybrid_search(
        &self,
        text: String,
        embedding: Option<Float32Array>,
        top_k: u32,
        filter: Option<NativeFilter>,
        alpha: f64,
        vector_candidates: Option<u32>,
        keyword_candidates: Option<u32>,
        selection: Option<&NativeGraphSelection>,
    ) -> Result<AsyncTask<GraphHybridSearchTask>> {
        self.shared.require_open()?;
        Ok(AsyncTask::new(GraphHybridSearchTask {
            shared: Arc::clone(&self.shared),
            text,
            embedding: embedding.map(|value| value.to_vec()).unwrap_or_default(),
            top_k: top_k as usize,
            filter: filter.map(NativeFilter::into_core).transpose()?,
            alpha: alpha as f32,
            vector_candidates: vector_candidates.map(|value| value as usize),
            keyword_candidates: keyword_candidates.map(|value| value as usize),
            selection: selection.map(|value| Arc::clone(&value.result)),
        }))
    }

    #[napi]
    pub fn save(&self, path: String) -> Result<AsyncTask<SaveGraphTask>> {
        self.shared.require_open()?;
        Ok(AsyncTask::new(SaveGraphTask {
            shared: Arc::clone(&self.shared),
            path,
        }))
    }

    #[napi]
    pub fn close(&self) -> AsyncTask<CloseGraphTask> {
        self.shared.closed.store(true, Ordering::Release);
        AsyncTask::new(CloseGraphTask {
            shared: Arc::clone(&self.shared),
        })
    }

    #[napi(getter)]
    pub fn closed(&self) -> bool {
        self.shared.closed.load(Ordering::Acquire)
    }
}

#[napi]
pub fn validate_graph(kind: String, path: String) -> AsyncTask<ValidateGraphTask> {
    AsyncTask::new(ValidateGraphTask { kind, path })
}

pub struct ValidateGraphTask {
    kind: String,
    path: String,
}

impl Task for ValidateGraphTask {
    type Output = ();
    type JsValue = ();

    fn compute(&mut self) -> Result<()> {
        match self.kind.as_str() {
            "graph" => GraphDatabase::validate_dir(&self.path),
            "combined" => GraphRetrievalDatabase::validate_dir(&self.path),
            actual => {
                return Err(invalid_boundary(
                    "kind",
                    &format!("expected graph or combined; got '{actual}'"),
                ))
            }
        }
        .map_err(graph_error)
    }

    fn resolve(&mut self, _env: Env, _output: ()) -> Result<()> {
        Ok(())
    }
}

pub struct CloseGraphTask {
    shared: Arc<GraphShared>,
}

impl Task for CloseGraphTask {
    type Output = ();
    type JsValue = ();

    fn compute(&mut self) -> Result<()> {
        *lock_graph(&self.shared)? = GraphState::Empty;
        Ok(())
    }

    fn resolve(&mut self, _env: Env, _output: ()) -> Result<()> {
        Ok(())
    }
}

pub struct AddGraphRecordsTask {
    shared: Arc<GraphShared>,
    records: Vec<OwnedGraphRecordInput>,
}

impl Task for AddGraphRecordsTask {
    type Output = ();
    type JsValue = ();

    fn compute(&mut self) -> Result<()> {
        self.shared.require_open()?;
        let mut state = lock_graph(&self.shared)?;
        for input in self.records.drain(..) {
            match &mut *state {
                GraphState::GraphBuilding(builder) => {
                    if input.embedding.is_some() || !input.documents.is_empty() {
                        return Err(invalid_boundary(
                            "records",
                            "graph-only records must not contain embeddings or retrieval documents",
                        ));
                    }
                    builder
                        .upsert_record(input.record, input.metadata)
                        .map_err(graph_error)?;
                }
                GraphState::CombinedBuilding(builder) => {
                    if !input.documents.is_empty() {
                        builder
                            .upsert_record_documents(input.record, input.metadata, input.documents)
                            .map_err(graph_error)?;
                    } else if let Some(embedding) = input.embedding {
                        builder
                            .upsert_record_with_embedding(input.record, input.metadata, embedding)
                            .map_err(graph_error)?;
                    } else {
                        builder
                            .upsert_record(input.record, input.metadata)
                            .map_err(graph_error)?;
                    }
                }
                _ => {
                    return Err(state_error(
                        "records can only be added before build(); create a new graph builder to replace the corpus",
                    ))
                }
            }
        }
        Ok(())
    }

    fn resolve(&mut self, _env: Env, _output: ()) -> Result<()> {
        Ok(())
    }
}

pub struct AddGraphFixtureRecordsTask {
    shared: Arc<GraphShared>,
    records: Vec<OwnedRecordInput>,
}

impl Task for AddGraphFixtureRecordsTask {
    type Output = ();
    type JsValue = ();

    fn compute(&mut self) -> Result<()> {
        self.shared.require_open()?;
        let mut state = lock_graph(&self.shared)?;
        for input in self.records.drain(..) {
            match &mut *state {
                GraphState::GraphBuilding(builder) => {
                    let chunks = input
                        .chunks
                        .into_iter()
                        .map(|chunk| {
                            if chunk.embedding.is_some() {
                                return Err(invalid_boundary(
                                    "fixture chunks",
                                    "graph-only fixture chunks must omit embeddings",
                                ));
                            }
                            Ok(CorpusChunkInput {
                                key: chunk.key,
                                text: chunk.text,
                                metadata: chunk.metadata,
                            })
                        })
                        .collect::<Result<Vec<_>>>()?;
                    builder
                        .upsert_input(RecordInput {
                            record: input.record,
                            metadata: input.metadata,
                            chunks,
                        })
                        .map_err(graph_error)?;
                }
                GraphState::CombinedBuilding(builder) => {
                    let (record, metadata, chunks) = crate::common::retrieval_chunks(input)?;
                    builder
                        .upsert_record_chunks(record, metadata, chunks)
                        .map_err(graph_error)?;
                }
                _ => {
                    return Err(state_error(
                        "fixture records can only be added before build()",
                    ))
                }
            }
        }
        Ok(())
    }

    fn resolve(&mut self, _env: Env, _output: ()) -> Result<()> {
        Ok(())
    }
}

pub struct BuildGraphTask {
    shared: Arc<GraphShared>,
}

impl Task for BuildGraphTask {
    type Output = ();
    type JsValue = ();

    fn compute(&mut self) -> Result<()> {
        self.shared.require_open()?;
        let mut state = lock_graph(&self.shared)?;
        let current = std::mem::replace(&mut *state, GraphState::Empty);
        match current {
            GraphState::GraphBuilding(builder) => {
                *state = GraphState::GraphReady(Box::new(builder.build().map_err(graph_error)?));
                Ok(())
            }
            GraphState::CombinedBuilding(builder) => {
                *state = GraphState::CombinedReady(Box::new(builder.build().map_err(graph_error)?));
                Ok(())
            }
            other => {
                *state = other;
                Err(state_error(
                    "build() may be called exactly once on a graph builder",
                ))
            }
        }
    }

    fn resolve(&mut self, _env: Env, _output: ()) -> Result<()> {
        Ok(())
    }
}

pub struct LoadGraphTask {
    shared: Arc<GraphShared>,
    kind: String,
    path: String,
}

impl Task for LoadGraphTask {
    type Output = ();
    type JsValue = ();

    fn compute(&mut self) -> Result<()> {
        self.shared.require_open()?;
        let loaded = match self.kind.as_str() {
            "graph" => GraphState::GraphReady(Box::new(
                GraphDatabase::load_from_dir(&self.path).map_err(graph_error)?,
            )),
            "combined" => GraphState::CombinedReady(Box::new(
                GraphRetrievalDatabase::load_from_dir(&self.path).map_err(graph_error)?,
            )),
            actual => {
                return Err(invalid_boundary(
                    "kind",
                    &format!("expected graph or combined; got '{actual}'"),
                ))
            }
        };
        *lock_graph(&self.shared)? = loaded;
        Ok(())
    }

    fn resolve(&mut self, _env: Env, _output: ()) -> Result<()> {
        Ok(())
    }
}

pub struct GraphQueryTask {
    shared: Arc<GraphShared>,
    selection: Arc<Mutex<Option<GraphResult>>>,
    query: GraphQuery,
}

impl Task for GraphQueryTask {
    type Output = NativeGraphResult;
    type JsValue = NativeGraphResult;

    fn compute(&mut self) -> Result<Self::Output> {
        self.shared.require_open()?;
        let state = lock_graph(&self.shared)?;
        let result = match &*state {
            GraphState::GraphReady(database) => database.graph_query(&self.query, None),
            GraphState::CombinedReady(database) => database.graph_query(&self.query, None),
            _ => {
                return Err(state_error(
                    "query requires a built or loaded graph database",
                ))
            }
        }
        .map_err(graph_error)?;
        let native = graph_result_to_native(&result);
        *self.selection.lock().map_err(|_| {
            state_error("graph selection lock was poisoned by a previous native failure")
        })? = Some(result);
        Ok(native)
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output)
    }
}

pub struct ProjectCandidatesTask {
    shared: Arc<GraphShared>,
    selection: Arc<Mutex<Option<GraphResult>>>,
    filter: Option<retrievalkit_core::Filter>,
}

impl Task for ProjectCandidatesTask {
    type Output = NativeCandidateProjection;
    type JsValue = NativeCandidateProjection;

    fn compute(&mut self) -> Result<Self::Output> {
        self.shared.require_open()?;
        let selection = require_selection(&self.selection)?;
        let state = lock_graph(&self.shared)?;
        let projection = match &*state {
            GraphState::GraphReady(database) => {
                database.project_candidate_identities(&selection, self.filter.as_ref())
            }
            GraphState::CombinedReady(database) => {
                database.project_candidate_identities(&selection, self.filter.as_ref())
            }
            _ => {
                return Err(state_error(
                    "candidate projection requires a built or loaded graph database",
                ))
            }
        }
        .map_err(graph_error)?;
        Ok(projection_to_native(projection))
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output)
    }
}

pub struct GraphSemanticSearchTask {
    shared: Arc<GraphShared>,
    embedding: Vec<f32>,
    top_k: usize,
    filter: Option<retrievalkit_core::Filter>,
    selection: Option<Arc<Mutex<Option<GraphResult>>>>,
}

impl Task for GraphSemanticSearchTask {
    type Output = Vec<NativeSearchHit>;
    type JsValue = Vec<NativeSearchHit>;

    fn compute(&mut self) -> Result<Self::Output> {
        self.shared.require_open()?;
        let state = lock_graph(&self.shared)?;
        let GraphState::CombinedReady(database) = &*state else {
            return Err(state_error(
                "retrieval search requires a combined graph-retrieval database",
            ));
        };
        let mut query = SearchQuery::new(std::mem::take(&mut self.embedding), self.top_k);
        if let Some(filter) = self.filter.take() {
            query = query.with_filter(filter);
        }
        let hits = if let Some(selection) = &self.selection {
            database.semantic_search_in_selection(&query, &require_selection(selection)?)
        } else {
            database.semantic_search(&query)
        }
        .map_err(graph_error)?;
        search_hits(database.retrieval(), hits)
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output)
    }
}

pub struct GraphKeywordSearchTask {
    shared: Arc<GraphShared>,
    text: String,
    top_k: usize,
    filter: Option<retrievalkit_core::Filter>,
    selection: Option<Arc<Mutex<Option<GraphResult>>>>,
}

impl Task for GraphKeywordSearchTask {
    type Output = Vec<NativeKeywordHit>;
    type JsValue = Vec<NativeKeywordHit>;

    fn compute(&mut self) -> Result<Self::Output> {
        self.shared.require_open()?;
        let state = lock_graph(&self.shared)?;
        let GraphState::CombinedReady(database) = &*state else {
            return Err(state_error(
                "retrieval search requires a combined graph-retrieval database",
            ));
        };
        let mut query = KeywordQuery::new(std::mem::take(&mut self.text), self.top_k);
        if let Some(filter) = self.filter.take() {
            query = query.with_filter(filter);
        }
        let hits = if let Some(selection) = &self.selection {
            database.keyword_search_in_selection(&query, &require_selection(selection)?)
        } else {
            database.keyword_search(&query)
        }
        .map_err(graph_error)?;
        keyword_hits(database.retrieval(), hits)
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output)
    }
}

pub struct GraphHybridSearchTask {
    shared: Arc<GraphShared>,
    text: String,
    embedding: Vec<f32>,
    top_k: usize,
    filter: Option<retrievalkit_core::Filter>,
    alpha: f32,
    vector_candidates: Option<usize>,
    keyword_candidates: Option<usize>,
    selection: Option<Arc<Mutex<Option<GraphResult>>>>,
}

impl Task for GraphHybridSearchTask {
    type Output = Vec<NativeHybridHit>;
    type JsValue = Vec<NativeHybridHit>;

    fn compute(&mut self) -> Result<Self::Output> {
        self.shared.require_open()?;
        let state = lock_graph(&self.shared)?;
        let GraphState::CombinedReady(database) = &*state else {
            return Err(state_error(
                "retrieval search requires a combined graph-retrieval database",
            ));
        };
        let mut query = HybridQuery::new(
            std::mem::take(&mut self.text),
            std::mem::take(&mut self.embedding),
            self.top_k,
        )
        .try_with_alpha(self.alpha)
        .map_err(core_error)?;
        let vector_top_k = self.vector_candidates.unwrap_or(query.vector_top_k);
        let keyword_top_k = self.keyword_candidates.unwrap_or(query.keyword_top_k);
        query = query.with_candidate_limits(vector_top_k, keyword_top_k);
        if let Some(filter) = self.filter.take() {
            query = query.with_filter(filter);
        }
        let hits = if let Some(selection) = &self.selection {
            database.hybrid_search_in_selection(&query, &require_selection(selection)?)
        } else {
            database.hybrid_search(&query)
        }
        .map_err(graph_error)?;
        hybrid_hits(database.retrieval(), hits, self.alpha)
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output)
    }
}

#[napi(object)]
#[derive(Clone)]
pub struct NativeGraphFileSizeReport {
    pub corpus_bytes: f64,
    pub schema_bytes: f64,
    pub graph_bytes: f64,
    pub total_bytes: f64,
}

pub struct SaveGraphTask {
    shared: Arc<GraphShared>,
    path: String,
}

impl Task for SaveGraphTask {
    type Output = NativeGraphFileSizeReport;
    type JsValue = NativeGraphFileSizeReport;

    fn compute(&mut self) -> Result<Self::Output> {
        self.shared.require_open()?;
        let state = lock_graph(&self.shared)?;
        let report = match &*state {
            GraphState::GraphReady(database) => database.save_to_dir(&self.path),
            GraphState::CombinedReady(database) => database.save_to_dir(&self.path),
            _ => {
                return Err(state_error(
                    "save requires a built or loaded graph database",
                ))
            }
        }
        .map_err(graph_error)?;
        Ok(NativeGraphFileSizeReport {
            corpus_bytes: report.corpus_bytes as f64,
            schema_bytes: report.schema_bytes as f64,
            graph_bytes: report.graph_bytes as f64,
            total_bytes: (report.corpus_bytes + report.schema_bytes + report.graph_bytes) as f64,
        })
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output)
    }
}

fn lock_graph(shared: &GraphShared) -> Result<std::sync::MutexGuard<'_, GraphState>> {
    shared
        .state
        .lock()
        .map_err(|_| state_error("graph database lock was poisoned by a previous native failure"))
}

fn require_selection(selection: &Arc<Mutex<Option<GraphResult>>>) -> Result<GraphResult> {
    selection
        .lock()
        .map_err(|_| state_error("graph selection lock was poisoned by a previous native failure"))?
        .clone()
        .ok_or_else(|| {
            state_error(
                "graph selection has no result; await graph.query() before using the selection",
            )
        })
}

fn graph_result_to_native(result: &GraphResult) -> NativeGraphResult {
    NativeGraphResult {
        matches: result
            .matches
            .iter()
            .map(|matched| NativeGraphMatch {
                node: node_id_to_native(&matched.node_id),
                depth: matched.depth as u32,
                path: matched
                    .path
                    .iter()
                    .map(|edge| NativeGraphPathEdge {
                        relationship: edge.edge_id.relationship_type.as_str().to_owned(),
                        source: node_id_to_native(&edge.edge_id.source),
                        target: node_id_to_native(&edge.edge_id.target),
                        occurrence_ordinal: edge.edge_id.occurrence_ordinal,
                        provenance: NativeGraphEdgeProvenance {
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
                        },
                    })
                    .collect(),
            })
            .collect(),
        truncated: result.truncated.map(|reason| match reason {
            TruncationReason::MaxHops => "maxHops".to_owned(),
            TruncationReason::MaxVisited => "maxVisited".to_owned(),
            TruncationReason::MaxResults => "maxResults".to_owned(),
            TruncationReason::MaxWorkingBytes => "maxWorkingBytes".to_owned(),
        }),
        trace: NativeGraphTrace {
            seed_count: result.trace.seed_count as u32,
            visited_states: result.trace.visited_states as u32,
            traversed_edges: result.trace.traversed_edges as u32,
            result_count: result.trace.result_count as u32,
            diagnostics: result.trace.diagnostics as u32,
        },
    }
}

fn projection_to_native(projection: GraphCandidateProjection) -> NativeCandidateProjection {
    NativeCandidateProjection {
        candidates: projection
            .candidates
            .into_iter()
            .map(|identity| NativeCandidateIdentity {
                record_id: identity.record_id.as_str().to_owned(),
                chunk_key: identity.chunk_key.as_str().to_owned(),
            })
            .collect(),
        source_nodes: projection.source_nodes as u32,
        projected_chunks_before_filter: projection.projected_chunks_before_filter as u32,
        projected_chunks_after_filter: projection.projected_chunks_after_filter as u32,
    }
}

fn graph_error(error: GraphError) -> napi::Error {
    let code = match error {
        GraphError::InvalidSchema { .. } => "RK_GRAPH_SCHEMA",
        GraphError::InvalidQuery { .. } | GraphError::QueryLimitExceeded { .. } => {
            "RK_INVALID_QUERY"
        }
        GraphError::InvalidDimension { .. } => "RK_DIMENSION",
        GraphError::MissingEmbedding { .. } => "RK_MISSING_EMBEDDING",
        GraphError::StaleGeneration { .. } => "RK_STALE_SELECTION",
        GraphError::InvalidSnapshot { .. }
        | GraphError::IncompatibleVersion { .. }
        | GraphError::WriterBusy { .. } => "RK_PERSISTENCE",
        GraphError::Cancelled | GraphError::TimedOut { .. } => "RK_CANCELLED",
        GraphError::InvalidRecord { .. }
        | GraphError::GraphUnavailable { .. }
        | GraphError::MissingTarget { .. }
        | GraphError::Core { .. } => "RK_GRAPH",
    };
    tagged_error(code, &error.to_string())
}
