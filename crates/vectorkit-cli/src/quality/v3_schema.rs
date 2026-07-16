use std::collections::BTreeMap;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Collection {
    pub schema_version: u32,
    pub collection_id: String,
    pub collection_version: String,
    pub corpus_id: String,
    pub split: String,
    pub top_k: usize,
    pub evaluation_depth: usize,
    pub relevance_threshold: u8,
    pub paths: CollectionPaths,
    pub counts: CollectionCounts,
    pub files: Vec<FileEntry>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CollectionPaths {
    pub chunking_manifest: String,
    pub corpus_embeddings_f32: String,
    pub embedding_manifest: String,
    pub evidence_judgments: String,
    pub exclusions: String,
    pub expected_paths: String,
    pub graph_construction_manifest: String,
    pub graph_schema: String,
    pub preprocessing_manifest: String,
    pub qrels: String,
    pub queries: String,
    pub query_embeddings_f32: String,
    pub records: String,
    pub seed_policy_manifest: String,
    pub split_manifest: String,
}

impl CollectionPaths {
    pub(super) fn entries(&self) -> [(&'static str, &str); 15] {
        [
            ("chunking_manifest", &self.chunking_manifest),
            ("corpus_embeddings_f32", &self.corpus_embeddings_f32),
            ("embedding_manifest", &self.embedding_manifest),
            ("evidence_judgments", &self.evidence_judgments),
            ("exclusions", &self.exclusions),
            ("expected_paths", &self.expected_paths),
            (
                "graph_construction_manifest",
                &self.graph_construction_manifest,
            ),
            ("graph_schema", &self.graph_schema),
            ("preprocessing_manifest", &self.preprocessing_manifest),
            ("qrels", &self.qrels),
            ("queries", &self.queries),
            ("query_embeddings_f32", &self.query_embeddings_f32),
            ("records", &self.records),
            ("seed_policy_manifest", &self.seed_policy_manifest),
            ("split_manifest", &self.split_manifest),
        ]
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CollectionCounts {
    pub records: usize,
    pub chunks: usize,
    pub queries: usize,
    pub qrel_rows: usize,
    pub evidence_rows: usize,
    pub expected_path_rows: usize,
    pub exclusion_rows: usize,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FileEntry {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Record {
    pub record_id: String,
    pub record_type: String,
    pub content: Option<String>,
    pub fields: BTreeMap<String, Value>,
    pub metadata: BTreeMap<String, Value>,
    pub chunks: Vec<RecordChunk>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RecordChunk {
    pub chunk_key: String,
    pub text: String,
    pub metadata: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GraphSchema {
    pub version: u32,
    pub record_nodes: Vec<RecordNodeRule>,
    pub relationships: Vec<RelationshipRule>,
    pub chunk_nodes: Option<ChunkNodeRule>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RecordNodeRule {
    pub record_type: String,
    pub node_type: String,
    pub queryable_fields: Vec<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RelationshipRule {
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

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ChunkNodeRule {
    pub node_type: String,
    pub owns_relationship: String,
    pub inverse_relationship: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Query {
    pub query_id: String,
    pub split: String,
    pub category: String,
    pub text: String,
    pub metadata_filter: Option<Value>,
    pub explicit_seed: Option<Value>,
    pub derived_seed_policy_id: Option<String>,
    pub traversal: Traversal,
    pub tasks: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Traversal {
    pub steps: Vec<TraversalStep>,
    pub limits: TraversalLimits,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TraversalStep {
    pub relationship_type: String,
    pub direction: String,
    pub min_hops: usize,
    pub max_hops: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TraversalLimits {
    pub max_hops: usize,
    pub max_visited: usize,
    pub max_results: usize,
    pub max_working_bytes: usize,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CorpusEmbedding {
    pub record_id: String,
    pub chunk_key: String,
    pub values: Vec<f32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct QueryEmbedding {
    pub query_id: String,
    pub values: Vec<f32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EvidenceJudgment {
    pub evidence_sets: Vec<Vec<String>>,
    pub query_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ExpectedPaths {
    pub expected_paths: Vec<Vec<PathEdge>>,
    pub query_id: String,
    pub seed_policy: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PathEdge {
    pub direction: String,
    pub occurrence_ordinal: usize,
    pub relationship_type: String,
    pub source_node: NodeIdentity,
    pub target_node: NodeIdentity,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct NodeIdentity {
    pub node_type: String,
    pub source: NodeSource,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub(super) enum NodeSource {
    #[serde(rename = "record")]
    Record { record_id: String },
    #[serde(rename = "chunk")]
    Chunk {
        record_id: String,
        chunk_key: String,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Exclusion {
    pub details: String,
    pub lane: String,
    pub phase: String,
    pub query_id: String,
    pub reason: String,
    pub source: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TransformationManifest {
    pub inputs: Vec<ManifestInput>,
    pub outputs: Vec<ManifestOutput>,
    pub parameters: Value,
    pub policy_id: String,
    pub policy_version: String,
    pub schema_version: u32,
    pub tool: ManifestTool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ManifestInput {
    pub sha256: String,
    pub source_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ManifestOutput {
    pub path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ManifestTool {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone)]
pub(super) struct Qrel {
    pub query_id: String,
    pub record_id: String,
    pub relevance: u8,
}

pub(super) fn from_value<T: DeserializeOwned>(file: &str, value: Value) -> Result<T, String> {
    serde_json::from_value(value).map_err(|error| format!("{file}: closed-schema error: {error}"))
}

pub(super) fn object_fields<'a>(
    file: &str,
    field: &str,
    value: &'a Value,
    required: &[&str],
) -> Result<&'a Map<String, Value>, String> {
    let object = value.as_object().ok_or_else(|| {
        format!(
            "{file}: field '{field}' expected object, actual {}",
            super::v3_canonical::kind(value)
        )
    })?;
    let mut actual = object.keys().map(String::as_str).collect::<Vec<_>>();
    actual.sort_unstable();
    let mut expected = required.to_vec();
    expected.sort_unstable();
    if actual != expected {
        return Err(format!(
            "{file}: field '{field}' expected exact keys {:?}, actual {:?}",
            expected, actual
        ));
    }
    Ok(object)
}
