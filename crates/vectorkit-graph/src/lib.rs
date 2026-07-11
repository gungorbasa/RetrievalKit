mod builder;
mod error;
mod persistence;
mod query;
mod schema;
mod snapshot;
mod storage;

use std::collections::BTreeSet;

use vectorkit_core::{
    CandidateScope, ChunkId, ExactVectorIndex, HybridHit, HybridQuery, KeywordHit, KeywordQuery,
    SearchHit, SearchQuery,
};

pub use builder::GraphBuildStats;
pub use error::{GraphError, Result};
pub use persistence::GraphDatabaseFileSizes;
pub use query::{
    CancellationToken, GraphMatch, GraphQuery, GraphQueryTrace, GraphResult, QueryLimits, Seed,
    Traverse, TruncationReason,
};
pub use schema::{
    Cardinality, ChunkNodeSchema, DuplicateReferencePolicy, FieldPath, GraphSchema,
    MissingTargetPolicy, NodeType, RecordNodeSchema, RelationshipSchema, RelationshipType,
    SchemaHash,
};
pub use snapshot::GraphSnapshotPayload;
pub use storage::{
    Direction, EdgeId, EdgeProvenance, GraphPathEdge, GraphScalar, NodeId, NodeSource,
};

use builder::build_graph;
use query::execute;
use storage::GraphStorage;

#[derive(Debug)]
pub struct GraphIndex {
    core: ExactVectorIndex,
    schema: GraphSchema,
    storage: GraphStorage,
    build_stats: GraphBuildStats,
    _generation_lease: Option<persistence::GenerationLease>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectionTrace {
    pub source_nodes: usize,
    pub resolved_chunks: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectedScope {
    pub scope: CandidateScope,
    pub trace: ProjectionTrace,
}

impl GraphIndex {
    /// Consumes the core index so graph-enabled callers have one mutable owner.
    pub fn build(core: ExactVectorIndex, schema: GraphSchema) -> Result<Self> {
        let schema = schema.canonicalized()?;
        let (storage, build_stats) = build_graph(&core, &schema)?;
        Ok(Self {
            core,
            schema,
            storage,
            build_stats,
            _generation_lease: None,
        })
    }

    /// Encodes the canonical schema and generation-bound graph state without
    /// touching the filesystem. Atomic bundle persistence is layered on this
    /// payload by the package persistence API.
    pub fn snapshot_payload(&self) -> Result<GraphSnapshotPayload> {
        snapshot::encode_snapshot(
            &self.storage,
            &self.schema,
            self.core.corpus_id(),
            self.core.generation(),
        )
    }

    /// Restores a graph snapshot against its canonical core generation.
    /// Corpus, generation, schema hash, node sources, and projected chunk IDs
    /// are validated before the index becomes queryable.
    pub fn from_snapshot_payload(
        core: ExactVectorIndex,
        payload: &GraphSnapshotPayload,
    ) -> Result<Self> {
        let (schema, storage, build_stats) = snapshot::decode_snapshot(&core, payload)?;
        Ok(Self {
            core,
            schema,
            storage,
            build_stats,
            _generation_lease: None,
        })
    }

    /// Atomically publishes a composite core + graph snapshot directory.
    pub fn save_to_dir(
        &self,
        directory: impl AsRef<std::path::Path>,
    ) -> Result<GraphDatabaseFileSizes> {
        persistence::save(self, directory.as_ref())
    }

    /// Opens the active composite snapshot after validating all payloads.
    pub fn load_from_dir(directory: impl AsRef<std::path::Path>) -> Result<Self> {
        persistence::load(directory.as_ref())
    }

    /// Runs the complete read-only validation path used by `load_from_dir`.
    pub fn validate_dir(directory: impl AsRef<std::path::Path>) -> Result<()> {
        persistence::validate(directory.as_ref())
    }

    pub fn schema(&self) -> &GraphSchema {
        &self.schema
    }

    pub fn build_stats(&self) -> GraphBuildStats {
        self.build_stats
    }

    pub fn node_count(&self) -> usize {
        self.storage.nodes.len()
    }

    pub fn edge_count(&self) -> usize {
        self.storage.edges.len()
    }

    pub fn search(&self, query: &SearchQuery) -> Result<Vec<SearchHit>> {
        self.core.search(query).map_err(GraphError::from)
    }

    pub fn keyword_search(&self, query: &KeywordQuery) -> Result<Vec<KeywordHit>> {
        self.core.keyword_search(query).map_err(GraphError::from)
    }

    pub fn hybrid_search(&self, query: &HybridQuery) -> Result<Vec<HybridHit>> {
        self.core.hybrid_search(query).map_err(GraphError::from)
    }

    pub fn graph_query(
        &self,
        query: &GraphQuery,
        cancellation: Option<&CancellationToken>,
    ) -> Result<GraphResult> {
        execute(
            &self.storage,
            self.core.corpus_id().clone(),
            self.core.generation(),
            query,
            cancellation,
        )
    }

    pub fn project_candidates(&self, result: &GraphResult) -> Result<ProjectedScope> {
        if result.corpus_id != *self.core.corpus_id() || result.generation != self.core.generation()
        {
            return Err(GraphError::Core {
                message: format!(
                    "stale graph result belongs to corpus '{}' generation {}, active is '{}' generation {}",
                    result.corpus_id.as_str(),
                    result.generation.get(),
                    self.core.corpus_id().as_str(),
                    self.core.generation().get()
                ),
            });
        }

        let mut chunk_ids = BTreeSet::<ChunkId>::new();
        for matched in &result.matches {
            if let Some(projected) = self.storage.chunk_projections.get(&matched.node_id) {
                chunk_ids.extend(projected);
            }
        }
        let trace = ProjectionTrace {
            source_nodes: result.matches.len(),
            resolved_chunks: chunk_ids.len(),
        };
        let scope = self.core.candidate_scope(chunk_ids)?;
        Ok(ProjectedScope { scope, trace })
    }

    pub fn search_in_candidates(
        &self,
        query: &SearchQuery,
        scope: &CandidateScope,
    ) -> Result<Vec<SearchHit>> {
        self.core
            .search_in_candidates(query, scope)
            .map_err(GraphError::from)
    }

    pub fn keyword_search_in_candidates(
        &self,
        query: &KeywordQuery,
        scope: &CandidateScope,
    ) -> Result<Vec<KeywordHit>> {
        self.core
            .keyword_search_in_candidates(query, scope)
            .map_err(GraphError::from)
    }

    pub fn hybrid_search_in_candidates(
        &self,
        query: &HybridQuery,
        scope: &CandidateScope,
    ) -> Result<Vec<HybridHit>> {
        self.core
            .hybrid_search_in_candidates(query, scope)
            .map_err(GraphError::from)
    }
}
