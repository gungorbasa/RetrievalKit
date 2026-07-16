mod builder;
mod error;
mod persistence;
mod query;
mod schema;
mod snapshot;
mod storage;

use std::collections::BTreeSet;

use vectorkit_core::{
    CandidateScope, ChunkId, ChunkIdentity, CorpusIndex, ExactVectorIndex, Filter, HybridHit,
    HybridQuery, KeywordHit, KeywordQuery, RetrievalDatabase, SearchHit, SearchQuery,
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

/// Schema-driven graph state derived from one canonical corpus generation.
#[derive(Debug, Clone)]
pub struct GraphEngine {
    schema: GraphSchema,
    storage: GraphStorage,
    build_stats: GraphBuildStats,
}

#[derive(Debug)]
pub struct GraphIndex {
    core: ExactVectorIndex,
    graph: GraphEngine,
    _generation_lease: Option<persistence::GenerationLease>,
}

/// A graph-only database. It contains no vector, BM25, or retrieval payload.
#[derive(Debug)]
pub struct GraphDatabase {
    corpus: CorpusIndex,
    graph: GraphEngine,
    _generation_lease: Option<persistence::GenerationLease>,
}

/// One canonical corpus with both graph and retrieval derived capabilities.
#[derive(Debug)]
pub struct GraphRetrievalDatabase {
    retrieval: RetrievalDatabase,
    graph: GraphEngine,
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

/// Stable external candidate identities projected from one graph result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphCandidateProjection {
    pub candidates: Vec<ChunkIdentity>,
    pub source_nodes: usize,
    pub projected_chunks_before_filter: usize,
    pub projected_chunks_after_filter: usize,
}

impl GraphEngine {
    pub fn build(corpus: &CorpusIndex, schema: GraphSchema) -> Result<Self> {
        let schema = schema.canonicalized()?;
        let (storage, build_stats) = build_graph(corpus, &schema)?;
        Ok(Self {
            schema,
            storage,
            build_stats,
        })
    }

    pub fn from_snapshot_payload(
        corpus: &CorpusIndex,
        payload: &GraphSnapshotPayload,
    ) -> Result<Self> {
        let (schema, storage, build_stats) = snapshot::decode_snapshot(corpus, payload)?;
        Ok(Self {
            schema,
            storage,
            build_stats,
        })
    }

    pub fn snapshot_payload(&self, corpus: &CorpusIndex) -> Result<GraphSnapshotPayload> {
        snapshot::encode_snapshot(
            &self.storage,
            &self.schema,
            corpus.corpus_id(),
            corpus.generation(),
        )
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

    pub fn query(
        &self,
        corpus: &CorpusIndex,
        query: &GraphQuery,
        cancellation: Option<&CancellationToken>,
    ) -> Result<GraphResult> {
        execute(
            &self.storage,
            corpus.corpus_id().clone(),
            corpus.generation(),
            query,
            cancellation,
        )
    }

    pub fn project_candidates(
        &self,
        corpus: &CorpusIndex,
        result: &GraphResult,
    ) -> Result<ProjectedScope> {
        if result.corpus_id != *corpus.corpus_id() || result.generation != corpus.generation() {
            return Err(GraphError::StaleGeneration {
                message: format!(
                    "stale graph result belongs to corpus '{}' generation {}, active is '{}' generation {}",
                    result.corpus_id.as_str(),
                    result.generation.get(),
                    corpus.corpus_id().as_str(),
                    corpus.generation().get()
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
        let scope = corpus.candidate_scope(chunk_ids)?;
        Ok(ProjectedScope { scope, trace })
    }
}

impl GraphIndex {
    /// Consumes the core index so graph-enabled callers have one mutable owner.
    pub fn build(core: ExactVectorIndex, schema: GraphSchema) -> Result<Self> {
        let graph = GraphEngine::build(core.corpus(), schema)?;
        Ok(Self {
            core,
            graph,
            _generation_lease: None,
        })
    }

    /// Encodes the canonical schema and generation-bound graph state without
    /// touching the filesystem. Atomic bundle persistence is layered on this
    /// payload by the package persistence API.
    pub fn snapshot_payload(&self) -> Result<GraphSnapshotPayload> {
        self.graph.snapshot_payload(self.core.corpus())
    }

    /// Restores a graph snapshot against its canonical core generation.
    /// Corpus, generation, schema hash, node sources, and projected chunk IDs
    /// are validated before the index becomes queryable.
    pub fn from_snapshot_payload(
        core: ExactVectorIndex,
        payload: &GraphSnapshotPayload,
    ) -> Result<Self> {
        let graph = GraphEngine::from_snapshot_payload(core.corpus(), payload)?;
        Ok(Self {
            core,
            graph,
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
        self.graph.schema()
    }

    pub fn build_stats(&self) -> GraphBuildStats {
        self.graph.build_stats()
    }

    pub fn node_count(&self) -> usize {
        self.graph.node_count()
    }

    pub fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }

    pub fn chunk_text(&self, chunk_id: ChunkId) -> Option<&str> {
        self.core.chunk(chunk_id).map(|chunk| chunk.text.as_str())
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
        self.graph.query(self.core.corpus(), query, cancellation)
    }

    pub fn project_candidates(&self, result: &GraphResult) -> Result<ProjectedScope> {
        self.graph.project_candidates(self.core.corpus(), result)
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

impl GraphDatabase {
    pub fn build(corpus: CorpusIndex, schema: GraphSchema) -> Result<Self> {
        let graph = GraphEngine::build(&corpus, schema)?;
        Ok(Self {
            corpus,
            graph,
            _generation_lease: None,
        })
    }

    pub fn corpus(&self) -> &CorpusIndex {
        &self.corpus
    }

    pub fn graph(&self) -> &GraphEngine {
        &self.graph
    }

    pub fn chunk_text(&self, chunk_id: ChunkId) -> Option<&str> {
        self.corpus.chunk(chunk_id).map(|chunk| chunk.text.as_str())
    }

    pub fn graph_query(
        &self,
        query: &GraphQuery,
        cancellation: Option<&CancellationToken>,
    ) -> Result<GraphResult> {
        self.graph.query(&self.corpus, query, cancellation)
    }

    pub fn project_candidates(&self, result: &GraphResult) -> Result<ProjectedScope> {
        self.graph.project_candidates(&self.corpus, result)
    }

    /// Projects, optionally filters, and materializes stable chunk identities.
    pub fn project_candidate_identities(
        &self,
        result: &GraphResult,
        filter: Option<&Filter>,
    ) -> Result<GraphCandidateProjection> {
        materialize_candidate_projection(&self.corpus, self.project_candidates(result)?, filter)
    }

    pub fn save_to_dir(
        &self,
        directory: impl AsRef<std::path::Path>,
    ) -> Result<GraphDatabaseFileSizes> {
        persistence::save_graph_database(self, directory.as_ref())
    }

    pub fn load_from_dir(directory: impl AsRef<std::path::Path>) -> Result<Self> {
        persistence::load_graph_database(directory.as_ref())
    }

    pub fn validate_dir(directory: impl AsRef<std::path::Path>) -> Result<()> {
        persistence::validate_graph_database(directory.as_ref())
    }
}

impl GraphRetrievalDatabase {
    pub fn build(retrieval: RetrievalDatabase, schema: GraphSchema) -> Result<Self> {
        let graph = GraphEngine::build(retrieval.corpus(), schema)?;
        Ok(Self {
            retrieval,
            graph,
            _generation_lease: None,
        })
    }

    pub fn corpus(&self) -> &CorpusIndex {
        self.retrieval.corpus()
    }

    pub fn graph(&self) -> &GraphEngine {
        &self.graph
    }

    pub fn retrieval(&self) -> &RetrievalDatabase {
        &self.retrieval
    }

    pub fn graph_query(
        &self,
        query: &GraphQuery,
        cancellation: Option<&CancellationToken>,
    ) -> Result<GraphResult> {
        self.graph
            .query(self.retrieval.corpus(), query, cancellation)
    }

    pub fn project_candidates(&self, result: &GraphResult) -> Result<ProjectedScope> {
        self.graph
            .project_candidates(self.retrieval.corpus(), result)
    }

    /// Projects, optionally filters, and materializes stable chunk identities.
    pub fn project_candidate_identities(
        &self,
        result: &GraphResult,
        filter: Option<&Filter>,
    ) -> Result<GraphCandidateProjection> {
        materialize_candidate_projection(
            self.retrieval.corpus(),
            self.project_candidates(result)?,
            filter,
        )
    }

    pub fn semantic_search(&self, query: &SearchQuery) -> Result<Vec<SearchHit>> {
        self.retrieval
            .semantic_search(query)
            .map_err(GraphError::from)
    }

    pub fn hybrid_search(&self, query: &HybridQuery) -> Result<Vec<HybridHit>> {
        self.retrieval
            .hybrid_search(query)
            .map_err(GraphError::from)
    }

    pub fn semantic_search_in_selection(
        &self,
        query: &SearchQuery,
        selection: &GraphResult,
    ) -> Result<Vec<SearchHit>> {
        let projected = self.project_candidates(selection)?;
        self.retrieval
            .semantic_search_in_candidates(query, &projected.scope)
            .map_err(GraphError::from)
    }

    pub fn hybrid_search_in_selection(
        &self,
        query: &HybridQuery,
        selection: &GraphResult,
    ) -> Result<Vec<HybridHit>> {
        let projected = self.project_candidates(selection)?;
        self.retrieval
            .hybrid_search_in_candidates(query, &projected.scope)
            .map_err(GraphError::from)
    }

    pub fn save_to_dir(
        &self,
        directory: impl AsRef<std::path::Path>,
    ) -> Result<GraphDatabaseFileSizes> {
        persistence::save_graph_retrieval_database(self, directory.as_ref())
    }

    pub fn load_from_dir(directory: impl AsRef<std::path::Path>) -> Result<Self> {
        persistence::load_graph_retrieval_database(directory.as_ref())
    }

    pub fn validate_dir(directory: impl AsRef<std::path::Path>) -> Result<()> {
        persistence::validate_graph_retrieval_database(directory.as_ref())
    }
}

fn materialize_candidate_projection(
    corpus: &CorpusIndex,
    projected: ProjectedScope,
    filter: Option<&Filter>,
) -> Result<GraphCandidateProjection> {
    let source_nodes = projected.trace.source_nodes;
    let projected_chunks_before_filter = projected.trace.resolved_chunks;
    let filtered = corpus.filter_candidate_scope(&projected.scope, filter)?;
    let candidates = corpus.candidate_scope_identities(&filtered)?;
    Ok(GraphCandidateProjection {
        projected_chunks_after_filter: candidates.len(),
        candidates,
        source_nodes,
        projected_chunks_before_filter,
    })
}
